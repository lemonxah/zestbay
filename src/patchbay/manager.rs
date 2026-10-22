use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::rules::{pattern_matches, AutoConnectRule};
use crate::pipewire::{GraphState, Link, MediaType, Node, NodeType, ObjectId, Port, PwCommand};

/// How many times the same (output port, input port) pair may be destroyed by
/// rule enforcement within `DESTROY_WINDOW` before we stop fighting whoever
/// keeps re-creating it (usually the session manager).
const MAX_DESTROYS_PER_WINDOW: usize = 3;
const DESTROY_WINDOW: Duration = Duration::from_secs(30);

pub struct PatchbayManager {
    graph: Arc<GraphState>,
    rules: Vec<AutoConnectRule>,
    pub enabled: bool,
    pub rules_dirty: bool,
    /// Display name of the default target node. When a source node has no
    /// matching rules, its output ports will be connected to this node instead.
    default_target: Option<String>,
    /// Recent rule-enforced disconnects per (output port, input port), used to
    /// break destroy/relink loops.
    recent_destroys: HashMap<(ObjectId, ObjectId), Vec<Instant>>,
}

impl PatchbayManager {
    pub fn new(graph: Arc<GraphState>) -> Self {
        Self {
            graph,
            rules: Vec::new(),
            enabled: true,
            rules_dirty: false,
            default_target: None,
            recent_destroys: HashMap::new(),
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

    pub fn set_default_target(&mut self, name: Option<String>) {
        self.default_target = name;
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
        if let (Some(a), Some(b)) = (output_port.media_type, input_port.media_type)
            && a != b
        {
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
        // Rules whose explicit mappings were all removed by this unlearn.
        // Rules that never had mappings (auto-match rules) are left alone.
        let mut emptied: Vec<String> = Vec::new();

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
                if rule.port_mappings.is_empty() {
                    emptied.push(rule.id.clone());
                }
            }
        }

        if !emptied.is_empty() {
            self.rules.retain(|r| !emptied.contains(&r.id));
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
        if !self.enabled || (self.rules.is_empty() && self.default_target.is_none()) {
            return Vec::new();
        }

        self.refresh_target_ids();
        self.clean_incompatible_mappings();

        let mut commands = Vec::new();
        let nodes = self.graph.get_all_nodes();

        for node in &nodes {
            if !node.ready {
                continue;
            }

            if !Self::is_routable_node(node) {
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

            if matching_rules.is_empty() {
                // No rules match this source — use the default target if set,
                // but only for application streams (StreamOutput), not hardware
                // sources like microphones or other node types.
                let is_app_stream = node.node_type == Some(NodeType::StreamOutput);
                if is_app_stream {
                if let Some(ref default_name) = self.default_target {
                    if let Some(target) = nodes.iter().find(|n| {
                        n.id != node.id
                            && n.ready
                            && n.node_type.map(|t| t.has_inputs()).unwrap_or(false)
                            && n.display_name() == default_name
                    }) {
                        // Auto-connect by port matching (no explicit port mappings)
                        let target_ports = self.graph.get_input_ports(target.id);
                        for source_port in &output_ports {
                            if let Some(target_port) =
                                self.find_matching_port(source_port, &target_ports)
                            {
                                if self
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
                        }
                    }
                }
                } // is_app_stream
            } else {
                for rule in &matching_rules {
                    if let Some(target) = self.find_matching_target(rule, &nodes, node.id) {
                        commands.extend(self.generate_connections(rule, target, &output_ports));
                    }
                }
            }
        }

        let now = Instant::now();
        self.recent_destroys.retain(|_, times| {
            times.retain(|t| now.duration_since(*t) < DESTROY_WINDOW);
            !times.is_empty()
        });

        let links = self.graph.get_all_links();
        for link in &links {
            if !self.should_remove_link(link, &nodes) {
                continue;
            }
            let key = (link.output_port_id, link.input_port_id);
            let times = self.recent_destroys.entry(key).or_default();
            if times.len() >= MAX_DESTROYS_PER_WINDOW {
                if times.len() == MAX_DESTROYS_PER_WINDOW {
                    log::warn!(
                        "Patchbay: link {} -> {} keeps being re-created; \
                         not removing it again for {}s",
                        link.output_port_id,
                        link.input_port_id,
                        DESTROY_WINDOW.as_secs(),
                    );
                    // One extra entry so the warning is logged once per window.
                    times.push(now);
                }
                continue;
            }
            times.push(now);
            commands.push(PwCommand::Disconnect { link_id: link.id });
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

                if let (Some(out_port), Some(in_port)) = (out_port, in_port) {
                    // Skip media type mismatches (e.g. Midi → Audio)
                    if let (Some(out_mt), Some(in_mt)) =
                        (out_port.media_type, in_port.media_type)
                    {
                        if out_mt != in_mt {
                            continue;
                        }
                    }
                    if self.graph.find_link(out_port.id, in_port.id).is_none() {
                        commands.push(PwCommand::Connect {
                            output_port_id: out_port.id,
                            input_port_id: in_port.id,
                        });
                    }
                }
            }
        }

        commands
    }

    fn find_matching_port<'a>(&self, source: &Port, targets: &'a [Port]) -> Option<&'a Port> {
        // Filter targets to compatible media types (don't connect Midi→Audio or Audio→Midi)
        let compatible: Vec<&Port> = targets
            .iter()
            .filter(|p| match (source.media_type, p.media_type) {
                (Some(a), Some(b)) => a == b,
                _ => true, // If either is unknown, allow
            })
            .collect();

        if compatible.is_empty() {
            return None;
        }

        if let Some(ref channel) = source.channel
            && let Some(target) = compatible.iter().find(|p| p.channel.as_ref() == Some(channel))
        {
            return Some(target);
        }

        if let Some(target) = compatible.iter().find(|p| p.name == source.name) {
            return Some(target);
        }

        let source_index = source.physical_index.unwrap_or(0);
        compatible
            .iter()
            .find(|p| p.physical_index.unwrap_or(0) == source_index)
            .copied()
            .or_else(|| compatible.first().copied())
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

    /// Remove port mappings where media types are incompatible (e.g. a rule
    /// learned before MIDI port detection that maps Midi → Audio).
    fn clean_incompatible_mappings(&mut self) {
        let nodes = self.graph.get_all_nodes();
        let mut dirty = false;
        let mut emptied: Vec<String> = Vec::new();

        for rule in &mut self.rules {
            if rule.port_mappings.is_empty() {
                continue;
            }

            // Find a target node for this rule
            let target = nodes.iter().find(|n| {
                n.ready && rule.matches_target(n.display_name(), n.node_type, n.id)
            });
            let Some(target) = target else { continue };

            // Find a source node
            let source = nodes.iter().find(|n| {
                n.ready && rule.matches_source(n.display_name(), n.node_type)
            });
            let Some(source) = source else { continue };

            let source_ports = self.graph.get_output_ports(source.id);
            let target_ports = self.graph.get_input_ports(target.id);

            let before = rule.port_mappings.len();
            rule.port_mappings.retain(|m| {
                let out_port = source_ports.iter().find(|p| p.name == m.output_port_name);
                let in_port = target_ports.iter().find(|p| p.name == m.input_port_name);
                if let (Some(op), Some(ip)) = (out_port, in_port) {
                    match (op.media_type, ip.media_type) {
                        (Some(a), Some(b)) if a != b => {
                            log::info!(
                                "Removing incompatible port mapping: {} ({:?}) -> {} ({:?})",
                                m.output_port_name, a, m.input_port_name, b,
                            );
                            false
                        }
                        _ => true,
                    }
                } else {
                    true // ports not found yet — keep mapping
                }
            });
            if rule.port_mappings.len() != before {
                dirty = true;
                if rule.port_mappings.is_empty() {
                    emptied.push(rule.id.clone());
                }
            }
        }

        // Rules with empty port_mappings use auto-matching, so a learned rule
        // whose mappings were all cleaned away must be removed rather than
        // turning into a "connect everything" rule.
        if !emptied.is_empty() {
            self.rules.retain(|r| !emptied.contains(&r.id));
        }
        if dirty {
            self.rules_dirty = true;
        }
    }

    fn is_routable_node(node: &Node) -> bool {
        match node.media_type {
            Some(MediaType::Video) => false,
            _ => true,
        }
    }

    /// Whether rule enforcement should destroy `link`.
    ///
    /// Only links whose *source* is governed by an enabled rule are ever
    /// removed, and only while at least one of that source's rule targets is
    /// present (so session-manager fallback routing while the target is gone
    /// is left alone). Links from sources no rule matches are never touched.
    fn should_remove_link(&self, link: &Link, nodes: &[Node]) -> bool {
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

        // If either port has already been removed from the graph (e.g. during
        // device disconnection), the link is stale and will be cleaned up by
        // PipeWire.  Don't actively try to remove it — doing so can race with
        // PipeWire's own cleanup and disrupt the audio graph.
        let (Some(out_p), Some(in_p)) = (
            self.graph.get_port(link.output_port_id),
            self.graph.get_port(link.input_port_id),
        ) else {
            return false;
        };

        let source_rules: Vec<&AutoConnectRule> = self
            .rules
            .iter()
            .filter(|r| {
                r.enabled && r.matches_source(source_node.display_name(), source_node.node_type)
            })
            .collect();
        if source_rules.is_empty() {
            return false;
        }

        let any_target_present = source_rules
            .iter()
            .any(|r| self.find_matching_target(r, nodes, source_node.id).is_some());
        if !any_target_present {
            return false;
        }

        let authorized = source_rules.iter().any(|rule| {
            if !rule.matches_target(
                target_node.display_name(),
                target_node.node_type,
                target_node.id,
            ) {
                return false;
            }
            rule.port_mappings.is_empty()
                || rule
                    .port_mappings
                    .iter()
                    .any(|m| m.output_port_name == out_p.name && m.input_port_name == in_p.name)
        });

        !authorized
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipewire::PortDirection;

    fn node(id: ObjectId, name: &str, node_type: NodeType) -> Node {
        Node {
            id,
            name: name.to_string(),
            description: name.to_string(),
            media_type: Some(MediaType::Audio),
            node_type: Some(node_type),
            is_virtual: false,
            is_jack: false,
            is_bridge: false,
            ready: true,
        }
    }

    fn port(id: ObjectId, node_id: ObjectId, name: &str, dir: PortDirection, mt: MediaType) -> Port {
        Port {
            id,
            node_id,
            name: name.to_string(),
            direction: dir,
            media_type: Some(mt),
            channel: None,
            physical_index: None,
            port_group: None,
            port_alias: None,
        }
    }

    fn link(id: ObjectId, out: &Port, inp: &Port) -> Link {
        Link {
            id,
            output_node_id: out.node_id,
            output_port_id: out.id,
            input_node_id: inp.node_id,
            input_port_id: inp.id,
            active: true,
        }
    }

    /// App (1) with one output, Headphones (2) and Speakers (3) sinks.
    fn setup() -> (Arc<GraphState>, Port, Port, Port) {
        let graph = GraphState::new();
        graph.insert_node(node(1, "App", NodeType::StreamOutput));
        graph.insert_node(node(2, "Headphones", NodeType::Sink));
        graph.insert_node(node(3, "Speakers", NodeType::Sink));
        let out = port(10, 1, "output_FL", PortDirection::Output, MediaType::Audio);
        let hp_in = port(20, 2, "playback_FL", PortDirection::Input, MediaType::Audio);
        let sp_in = port(30, 3, "playback_FL", PortDirection::Input, MediaType::Audio);
        graph.insert_port(out.clone());
        graph.insert_port(hp_in.clone());
        graph.insert_port(sp_in.clone());
        (graph, out, hp_in, sp_in)
    }

    fn disconnects(cmds: &[PwCommand]) -> Vec<ObjectId> {
        cmds.iter()
            .filter_map(|c| match c {
                PwCommand::Disconnect { link_id } => Some(*link_id),
                _ => None,
            })
            .collect()
    }

    fn connects(cmds: &[PwCommand]) -> Vec<(ObjectId, ObjectId)> {
        cmds.iter()
            .filter_map(|c| match c {
                PwCommand::Connect {
                    output_port_id,
                    input_port_id,
                } => Some((*output_port_id, *input_port_id)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn unlearn_keeps_user_auto_match_rules() {
        let (graph, out, hp_in, _) = setup();
        let mut pb = PatchbayManager::new(graph.clone());
        // User rules (no mappings = auto-match ports).
        pb.add_rule(AutoConnectRule::new("Other", None, "Speakers", None, None));
        pb.add_rule(AutoConnectRule::new("App", None, "Headphones", None, None));
        let app = graph.get_node(1).unwrap();
        let hp = graph.get_node(2).unwrap();
        pb.unlearn_from_link(&app, &hp, &out, &hp_in);
        assert_eq!(pb.rules().len(), 2);
    }

    #[test]
    fn unlearn_removes_learned_rule_emptied_by_it() {
        let (graph, out, hp_in, _) = setup();
        let mut pb = PatchbayManager::new(graph.clone());
        pb.add_rule(AutoConnectRule::new("Other", None, "Speakers", None, None));
        let app = graph.get_node(1).unwrap();
        let hp = graph.get_node(2).unwrap();
        assert!(pb.learn_from_link(&app, &hp, &out, &hp_in));
        assert_eq!(pb.rules().len(), 2);
        assert!(pb.unlearn_from_link(&app, &hp, &out, &hp_in));
        assert_eq!(pb.rules().len(), 1);
        assert_eq!(pb.rules()[0].source_pattern, "Other");
    }

    #[test]
    fn learn_rejects_media_type_mismatch() {
        let (graph, out, _, _) = setup();
        let midi_in = port(21, 2, "midi_in", PortDirection::Input, MediaType::Midi);
        graph.insert_port(midi_in.clone());
        let mut pb = PatchbayManager::new(graph.clone());
        let app = graph.get_node(1).unwrap();
        let hp = graph.get_node(2).unwrap();
        assert!(!pb.learn_from_link(&app, &hp, &out, &midi_in));
        assert!(pb.rules().is_empty());
    }

    #[test]
    fn learned_rule_cleaned_to_zero_is_removed() {
        let (graph, out, _, _) = setup();
        let midi_in = port(21, 2, "midi_in", PortDirection::Input, MediaType::Midi);
        graph.insert_port(midi_in);
        let mut pb = PatchbayManager::new(graph.clone());
        // A learned rule containing only an incompatible mapping (e.g. learned
        // before MIDI port detection).
        let mut rule = AutoConnectRule::new(
            "App",
            Some(NodeType::StreamOutput),
            "Headphones",
            Some(NodeType::Sink),
            Some(2),
        );
        rule.add_port_mapping(out.name.clone(), "midi_in".into());
        // And a user auto-match rule that must survive.
        let user = AutoConnectRule::new("App", None, "Speakers", None, None);
        pb.set_rules(vec![rule, user]);
        let cmds = pb.scan();
        assert_eq!(pb.rules().len(), 1);
        assert_eq!(pb.rules()[0].target_pattern, "Speakers");
        // The removed rule must not have turned into a connect-everything rule.
        assert!(!connects(&cmds).contains(&(10, 20)));
    }

    #[test]
    fn links_from_unruled_sources_are_not_removed() {
        let (graph, out, hp_in, _) = setup();
        graph.insert_node(node(4, "Other", NodeType::StreamOutput));
        let other_out = port(40, 4, "output_FL", PortDirection::Output, MediaType::Audio);
        graph.insert_port(other_out.clone());
        // Rule: App -> Headphones. "Other" has no rule but links into Headphones.
        graph.insert_link(link(100, &other_out, &hp_in));
        graph.insert_link(link(101, &out, &hp_in));
        let mut pb = PatchbayManager::new(graph.clone());
        pb.add_rule(AutoConnectRule::new("App", None, "Headphones", None, Some(2)));
        let cmds = pb.scan();
        assert!(disconnects(&cmds).is_empty());
    }

    #[test]
    fn ruled_source_links_kept_while_target_absent() {
        let (graph, out, _, sp_in) = setup();
        // Session manager routed App to Speakers because Headphones vanished.
        graph.remove_node(2);
        graph.remove_port(20);
        graph.insert_link(link(100, &out, &sp_in));
        let mut pb = PatchbayManager::new(graph.clone());
        pb.add_rule(AutoConnectRule::new("App", None, "Headphones", None, None));
        assert!(disconnects(&pb.scan()).is_empty());
    }

    #[test]
    fn ruled_source_unauthorized_link_removed_with_loop_guard() {
        let (graph, out, _, sp_in) = setup();
        graph.insert_link(link(100, &out, &sp_in));
        let mut pb = PatchbayManager::new(graph.clone());
        pb.add_rule(AutoConnectRule::new("App", None, "Headphones", None, None));
        for _ in 0..MAX_DESTROYS_PER_WINDOW {
            assert_eq!(disconnects(&pb.scan()), vec![100]);
        }
        // The link keeps coming back: stop fighting it.
        assert!(disconnects(&pb.scan()).is_empty());
        assert!(disconnects(&pb.scan()).is_empty());
    }

    #[test]
    fn default_target_routes_without_rules() {
        let (graph, _, hp_in, _) = setup();
        let mut pb = PatchbayManager::new(graph.clone());
        pb.set_default_target(Some("Headphones".into()));
        let cmds = pb.scan();
        assert_eq!(connects(&cmds), vec![(10, hp_in.id)]);
    }

    #[test]
    fn no_rules_no_default_does_nothing() {
        let (graph, out, _, sp_in) = setup();
        graph.insert_link(link(100, &out, &sp_in));
        let mut pb = PatchbayManager::new(graph);
        assert!(pb.scan().is_empty());
    }
}
