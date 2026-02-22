use std::sync::Arc;

use super::rules::{pattern_matches, AutoConnectRule};
use crate::pipewire::{GraphState, Link, MediaType, Node, NodeType, ObjectId, Port, PwCommand};

pub struct PatchbayManager {
    graph: Arc<GraphState>,
    rules: Vec<AutoConnectRule>,
    pub enabled: bool,
    pub rules_dirty: bool,
}

impl PatchbayManager {
    pub fn new(graph: Arc<GraphState>) -> Self {
        Self {
            graph,
            rules: Vec::new(),
            enabled: true,
            rules_dirty: false,
        }
    }

    pub fn set_rules(&mut self, rules: Vec<AutoConnectRule>) {
        self.rules = rules;
        self.rules_dirty = true;
    }

    pub fn add_rule(&mut self, rule: AutoConnectRule) {
        self.rules.push(rule);
        self.rules_dirty = true;
    }

    pub fn remove_rule(&mut self, id: &str) {
        self.rules.retain(|r| r.id != id);
        self.rules_dirty = true;
    }

    pub fn rules(&self) -> &[AutoConnectRule] {
        &self.rules
    }

    pub fn toggle_rule(&mut self, id: &str) -> Option<bool> {
        if let Some(rule) = self.rules.iter_mut().find(|r| r.id == id) {
            rule.enabled = !rule.enabled;
            self.rules_dirty = true;
            Some(rule.enabled)
        } else {
            None
        }
    }

    pub fn learn_from_link(
        &mut self,
        source_node: &Node,
        target_node: &Node,
        output_port: &Port,
        input_port: &Port,
    ) -> bool {
        if source_node.id == target_node.id {
            return false;
        }
        if !Self::is_routable_node(source_node) || !Self::is_routable_node(target_node) {
            return false;
        }

        let source_name = source_node.display_name().to_string();

        let existing = self.rules.iter_mut().find(|r| {
            r.source_pattern == source_name
                && r.matches_target(
                    target_node.display_name(),
                    target_node.node_type,
                    target_node.id,
                )
        });

        if let Some(rule) = existing {
            let changed = rule.add_port_mapping(output_port.name.clone(), input_port.name.clone());
            if changed {
                self.rules_dirty = true;
            }
            return changed;
        }

        let mut rule = AutoConnectRule::new(
            source_name,
            source_node.node_type,
            target_node.display_name(),
            target_node.node_type,
            Some(target_node.id),
        );
        rule.add_port_mapping(output_port.name.clone(), input_port.name.clone());
        self.rules.push(rule);
        self.rules_dirty = true;
        true
    }

    pub fn unlearn_from_link(
        &mut self,
        source_node: &Node,
        target_node: &Node,
        output_port: &Port,
        input_port: &Port,
    ) -> bool {
        if !Self::is_routable_node(source_node) || !Self::is_routable_node(target_node) {
            return false;
        }

        let source_name = source_node.display_name();
        let mut changed = false;

        for rule in &mut self.rules {
            if !rule.matches_source(source_name, source_node.node_type) {
                continue;
            }
            if !rule.matches_target(
                target_node.display_name(),
                target_node.node_type,
                target_node.id,
            ) {
                continue;
            }

            let before = rule.port_mappings.len();
            rule.port_mappings.retain(|m| {
                !(m.output_port_name == output_port.name && m.input_port_name == input_port.name)
            });
            if rule.port_mappings.len() != before {
                changed = true;
            }
        }

        let before = self.rules.len();
        self.rules.retain(|r| !r.port_mappings.is_empty());
        if self.rules.len() != before {
            changed = true;
        }

        if changed {
            self.rules_dirty = true;
        }
        changed
    }

    pub fn snapshot_current_connections(&mut self) {
        use std::collections::HashMap;

        let links = self.graph.get_all_links();

        let mut rule_map: HashMap<(String, String, Option<NodeType>, ObjectId), AutoConnectRule> =
            HashMap::new();

        for link in &links {
            let source = self.graph.get_node(link.output_node_id);
            let target = self.graph.get_node(link.input_node_id);
            let out_port = self.graph.get_port(link.output_port_id);
            let in_port = self.graph.get_port(link.input_port_id);

            if let (Some(source), Some(target), Some(out_port), Some(in_port)) =
                (source, target, out_port, in_port)
            {
                if source.id == target.id {
                    continue;
                }
                if !Self::is_routable_node(&source) || !Self::is_routable_node(&target) {
                    continue;
                }
                let key = (
                    source.display_name().to_string(),
                    target.display_name().to_string(),
                    target.node_type,
                    target.id,
                );

                let rule = rule_map.entry(key).or_insert_with(|| {
                    AutoConnectRule::new(
                        source.display_name(),
                        source.node_type,
                        target.display_name(),
                        target.node_type,
                        Some(target.id),
                    )
                });

                rule.add_port_mapping(out_port.name.clone(), in_port.name.clone());
            }
        }

        self.rules = rule_map.into_values().collect();
        self.rules_dirty = true;
    }

    pub fn refresh_target_ids(&mut self) {
        let nodes = self.graph.get_all_nodes();
        let mut dirty = false;

        for rule in &mut self.rules {
            if let Some(old_id) = rule.target_node_id {
                let id_still_valid = nodes.iter().any(|n| {
                    n.id == old_id
                        && n.ready
                        && pattern_matches(&rule.target_pattern, n.display_name())
                });

                if !id_still_valid {
                    let new_match = nodes.iter().find(|n| {
                        n.ready
                            && n.node_type.map(|t| t.has_inputs()).unwrap_or(false)
                            && pattern_matches(&rule.target_pattern, n.display_name())
                            && (rule.target_node_type.is_none()
                                || n.node_type == rule.target_node_type)
                    });

                    let new_id = new_match.map(|n| n.id);
                    if rule.target_node_id != new_id {
                        log::info!(
                            "Rule '{}→{}': updating stale target_node_id {:?} → {:?}",
                            rule.source_pattern,
                            rule.target_pattern,
                            rule.target_node_id,
                            new_id,
                        );
                        rule.target_node_id = new_id;
                        dirty = true;
                    }
                }
            }
        }

        if dirty {
            self.rules_dirty = true;
        }
    }

    pub fn scan(&mut self) -> Vec<PwCommand> {
        if !self.enabled || self.rules.is_empty() {
            return Vec::new();
        }

        self.refresh_target_ids();

        let mut commands = Vec::new();
        let nodes = self.graph.get_all_nodes();

        for node in &nodes {
            if !node.ready {
                continue;
            }

            if !Self::is_routable_node(node) {
                continue;
            }

            if !node.node_type.map(|t| t.has_outputs()).unwrap_or(false) {
                continue;
            }

            let output_ports = self.graph.get_output_ports(node.id);
            if output_ports.is_empty() {
                continue;
            }

            let matching_rules: Vec<&AutoConnectRule> = self
                .rules
                .iter()
                .filter(|r| r.enabled && r.matches_source(node.display_name(), node.node_type))
                .collect();

            for rule in &matching_rules {
                if let Some(target) = self.find_matching_target(rule, &nodes, node.id) {
                    commands.extend(self.generate_connections(rule, target, &output_ports));
                }
            }
        }

        let links = self.graph.get_all_links();
        for link in &links {
            if self.should_remove_link(link) {
                commands.push(PwCommand::Disconnect { link_id: link.id });
            }
        }

        commands
    }

    fn generate_connections(
        &self,
        rule: &AutoConnectRule,
        target: &Node,
        source_ports: &[Port],
    ) -> Vec<PwCommand> {
        let mut commands = Vec::new();
        let target_ports = self.graph.get_input_ports(target.id);

        if rule.port_mappings.is_empty() {
            for source_port in source_ports {
                if let Some(target_port) = self.find_matching_port(source_port, &target_ports)
                    && self
                        .graph
                        .find_link(source_port.id, target_port.id)
                        .is_none()
                {
                    commands.push(PwCommand::Connect {
                        output_port_id: source_port.id,
                        input_port_id: target_port.id,
                    });
                }
            }
        } else {
            for mapping in &rule.port_mappings {
                let out_port = source_ports
                    .iter()
                    .find(|p| p.name == mapping.output_port_name);
                let in_port = target_ports
                    .iter()
                    .find(|p| p.name == mapping.input_port_name);

                if let (Some(out_port), Some(in_port)) = (out_port, in_port)
                    && self.graph.find_link(out_port.id, in_port.id).is_none()
                {
                    commands.push(PwCommand::Connect {
                        output_port_id: out_port.id,
                        input_port_id: in_port.id,
                    });
                }
            }
        }

        commands
    }

    fn find_matching_port<'a>(&self, source: &Port, targets: &'a [Port]) -> Option<&'a Port> {
        if let Some(ref channel) = source.channel
            && let Some(target) = targets.iter().find(|p| p.channel.as_ref() == Some(channel))
        {
            return Some(target);
        }

        if let Some(target) = targets.iter().find(|p| p.name == source.name) {
            return Some(target);
        }

        let source_index = source.physical_index.unwrap_or(0);
        targets
            .iter()
            .find(|p| p.physical_index.unwrap_or(0) == source_index)
            .or_else(|| targets.first())
    }

    fn find_matching_target<'a>(
        &self,
        rule: &AutoConnectRule,
        nodes: &'a [Node],
        exclude_node_id: ObjectId,
    ) -> Option<&'a Node> {
        if let Some(target_id) = rule.target_node_id
            && target_id != exclude_node_id
            && let Some(node) = nodes.iter().find(|n| n.id == target_id && n.ready)
            && node.node_type.map(|t| t.has_inputs()).unwrap_or(false) {
                return Some(node);
        }

        nodes.iter().find(|n| {
            n.id != exclude_node_id
                && n.ready
                && n.node_type.map(|t| t.has_inputs()).unwrap_or(false)
                && rule.matches_target(n.display_name(), n.node_type, n.id)
        })
    }

    fn is_routable_node(node: &Node) -> bool {
        match node.media_type {
            Some(MediaType::Video) => false,
            _ => true,
        }
    }

    fn should_remove_link(&self, link: &Link) -> bool {
        let source_node = match self.graph.get_node(link.output_node_id) {
            Some(n) => n,
            None => return false,
        };

        let target_node = match self.graph.get_node(link.input_node_id) {
            Some(n) => n,
            None => return false,
        };

        if !Self::is_routable_node(&source_node) || !Self::is_routable_node(&target_node) {
            return false;
        }

        let out_port = self.graph.get_port(link.output_port_id);
        let in_port = self.graph.get_port(link.input_port_id);

        let link_authorized_by = |rule: &AutoConnectRule| -> bool {
            if !rule.enabled {
                return false;
            }
            if !rule.matches_source(source_node.display_name(), source_node.node_type) {
                return false;
            }
            if !rule.matches_target(
                target_node.display_name(),
                target_node.node_type,
                target_node.id,
            ) {
                return false;
            }

            if rule.port_mappings.is_empty() {
                return true;
            }

            if let (Some(out_p), Some(in_p)) = (&out_port, &in_port) {
                rule.port_mappings
                    .iter()
                    .any(|m| m.output_port_name == out_p.name && m.input_port_name == in_p.name)
            } else {
                false
            }
        };

        let has_any_rule_for_source = self.rules.iter().any(|r| {
            r.enabled && r.matches_source(source_node.display_name(), source_node.node_type)
        });

        if has_any_rule_for_source {
            let authorized = self.rules.iter().any(|r| link_authorized_by(r));
            if !authorized {
                return true;
            }
        }

        let has_any_rule_for_target = self.rules.iter().any(|r| {
            r.enabled
                && r.matches_target(
                    target_node.display_name(),
                    target_node.node_type,
                    target_node.id,
                )
        });

        if has_any_rule_for_target {
            let authorized = self.rules.iter().any(|r| link_authorized_by(r));
            if !authorized {
                return true;
            }
        }

        false
    }
}
