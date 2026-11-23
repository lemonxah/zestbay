use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::rules::{AutoConnectRule, PortMapping, RuleAction, pattern_matches};
use crate::pipewire::{GraphState, Link, MediaType, Node, NodeType, ObjectId, Port, PwCommand};

const MAX_DESTROYS_PER_WINDOW: usize = 3;
const DESTROY_WINDOW: Duration = Duration::from_secs(30);

pub struct PatchbayManager {
    graph: Arc<GraphState>,
    rules: Vec<AutoConnectRule>,
    pub enabled: bool,
    pub exclusive: bool,
    pub hide_unnamed: bool,
    pub rules_dirty: bool,
    default_target: Option<String>,
    recent_destroys: HashMap<(ObjectId, ObjectId), Vec<Instant>>,
    redirects: HashMap<ObjectId, ObjectId>,
    redirect_origins: HashMap<ObjectId, ObjectId>,
    standby_ports: HashSet<ObjectId>,
}

const MAX_REDIRECT_HOPS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Ungoverned,
    Authorized,
    Unauthorized,
}

impl PatchbayManager {
    pub fn new(graph: Arc<GraphState>) -> Self {
        Self {
            graph,
            rules: Vec::new(),
            enabled: true,
            exclusive: true,
            hide_unnamed: true,
            rules_dirty: false,
            default_target: None,
            recent_destroys: HashMap::new(),
            redirects: HashMap::new(),
            redirect_origins: HashMap::new(),
            standby_ports: HashSet::new(),
        }
    }

    pub fn set_port_redirects(
        &mut self,
        redirects: HashMap<ObjectId, ObjectId>,
        standby_ports: HashSet<ObjectId>,
    ) {
        self.redirect_origins = redirects.iter().map(|(from, to)| (*to, *from)).collect();
        self.redirects = redirects;
        self.standby_ports = standby_ports;
    }

    fn physical_port(&self, port_id: ObjectId) -> ObjectId {
        let mut id = port_id;
        for _ in 0..MAX_REDIRECT_HOPS {
            match self.redirects.get(&id) {
                Some(next) if *next != id => id = *next,
                _ => break,
            }
        }
        id
    }

    fn logical_port(&self, port_id: ObjectId) -> ObjectId {
        let mut id = port_id;
        for _ in 0..MAX_REDIRECT_HOPS {
            match self.redirect_origins.get(&id) {
                Some(prev) if *prev != id => id = *prev,
                _ => break,
            }
        }
        id
    }

    fn is_redirect_internal(&self, output_port_id: ObjectId, input_port_id: ObjectId) -> bool {
        let node_of = |port_id: ObjectId| self.graph.get_port(port_id).map(|p| p.node_id);
        let joins = |anchor: ObjectId, other: ObjectId| {
            self.redirects
                .get(&anchor)
                .and_then(|stand_in| node_of(*stand_in))
                .is_some_and(|meter_node| node_of(other) == Some(meter_node))
        };
        joins(output_port_id, input_port_id) || joins(input_port_id, output_port_id)
    }

    fn skips_redirect(&self, output_port_id: ObjectId, input_port_id: ObjectId) -> bool {
        (self.redirects.contains_key(&output_port_id)
            || self.redirects.contains_key(&input_port_id))
            && !self.is_redirect_internal(output_port_id, input_port_id)
    }

    pub fn rename_node(&mut self, old: &str, new: &str) -> bool {
        if old == new {
            return false;
        }
        let is_plugin = |t: Option<NodeType>| matches!(t, None | Some(NodeType::Plugin));
        let mut changed = false;
        for rule in &mut self.rules {
            if rule.source_pattern == old && is_plugin(rule.source_node_type) {
                rule.source_pattern = new.to_string();
                changed = true;
            }
            if rule.target_pattern == old && is_plugin(rule.target_node_type) {
                rule.target_pattern = new.to_string();
                changed = true;
            }
        }
        if !changed {
            return false;
        }
        let mut merged: Vec<AutoConnectRule> = Vec::with_capacity(self.rules.len());
        for rule in std::mem::take(&mut self.rules) {
            let twin = merged.iter_mut().find(|m| {
                m.source_pattern == rule.source_pattern
                    && m.source_node_type == rule.source_node_type
                    && m.target_pattern == rule.target_pattern
                    && m.target_node_type == rule.target_node_type
                    && m.action == rule.action
                    && m.enabled == rule.enabled
                    && m.port_mappings.is_empty() == rule.port_mappings.is_empty()
            });
            match twin {
                Some(existing) => {
                    for mapping in rule.port_mappings {
                        if !existing.port_mappings.contains(&mapping) {
                            existing.port_mappings.push(mapping);
                        }
                    }
                    if existing.target_node_id.is_none() {
                        existing.target_node_id = rule.target_node_id;
                    }
                }
                None => merged.push(rule),
            }
        }
        self.rules = merged;
        self.rules_dirty = true;
        true
    }

    pub fn forget_plugin_node(&mut self, name: &str) -> bool {
        let is_plugin = |t: Option<NodeType>| t == Some(NodeType::Plugin);
        let before = self.rules.len();
        self.rules.retain(|r| {
            !((r.source_pattern == name && is_plugin(r.source_node_type))
                || (r.target_pattern == name && is_plugin(r.target_node_type)))
        });
        let changed = self.rules.len() != before;
        if changed {
            self.rules_dirty = true;
        }
        changed
    }

    pub fn forget_absent_plugin_nodes(
        &mut self,
        is_candidate: impl Fn(&str) -> bool,
        present: &HashSet<String>,
    ) -> bool {
        let is_plugin = |t: Option<NodeType>| t == Some(NodeType::Plugin);
        let stale = |pattern: &str, node_type: Option<NodeType>| {
            is_plugin(node_type) && is_candidate(pattern) && !present.contains(pattern)
        };
        let before = self.rules.len();
        self.rules.retain(|r| {
            !(stale(&r.source_pattern, r.source_node_type)
                || stale(&r.target_pattern, r.target_node_type))
        });
        let changed = self.rules.len() != before;
        if changed {
            self.rules_dirty = true;
        }
        changed
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
        if Self::is_subgraph_internal(source_node, target_node) {
            return false;
        }
        if let (Some(a), Some(b)) = (output_port.media_type, input_port.media_type)
            && a != b
        {
            return false;
        }
        if self.is_blocked(source_node, target_node, output_port, input_port) {
            return false;
        }

        let source_name = source_node.display_name().to_string();

        let existing = self.rules.iter_mut().find(|r| {
            !r.is_block()
                && r.source_pattern == source_name
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
        let mut emptied: Vec<String> = Vec::new();

        for rule in &mut self.rules {
            if rule.is_block() || !rule.matches_source(source_name, source_node.node_type) {
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

    pub fn block_pair(&mut self, source: &Node, target: &Node) -> bool {
        if source.id == target.id {
            return false;
        }
        let source_name = source.display_name().to_string();
        let target_name = target.display_name().to_string();
        let mut changed = false;

        let before = self.rules.len();
        self.rules.retain(|r| {
            r.is_block() || r.source_pattern != source_name || r.target_pattern != target_name
        });
        changed |= self.rules.len() != before;

        let existing = self.rules.iter_mut().find(|r| {
            r.is_block()
                && r.port_mappings.is_empty()
                && r.matches_source(&source_name, source.node_type)
                && r.matches_target(&target_name, target.node_type, target.id)
        });
        match existing {
            Some(rule) if rule.enabled => {}
            Some(rule) => {
                rule.enabled = true;
                changed = true;
            }
            None => {
                self.rules.push(
                    AutoConnectRule::new(
                        source_name,
                        source.node_type,
                        target_name,
                        target.node_type,
                        Some(target.id),
                    )
                    .with_action(RuleAction::Block),
                );
                changed = true;
            }
        }

        if changed {
            self.rules_dirty = true;
        }
        changed
    }

    pub fn blocking_rule(
        &self,
        source: &Node,
        target: &Node,
        out: &Port,
        inp: &Port,
    ) -> Option<&AutoConnectRule> {
        self.rules.iter().find(|r| {
            r.enabled
                && r.is_block()
                && r.matches_source(source.display_name(), source.node_type)
                && r.matches_target(target.display_name(), target.node_type, target.id)
                && r.covers_ports(&out.name, &inp.name)
        })
    }

    pub fn is_blocked(&self, source: &Node, target: &Node, out: &Port, inp: &Port) -> bool {
        self.blocking_rule(source, target, out, inp).is_some()
    }

    pub fn blocked_pairs(&self) -> Vec<(ObjectId, ObjectId, String)> {
        let blocks: Vec<&AutoConnectRule> = self
            .rules
            .iter()
            .filter(|r| r.enabled && r.is_block())
            .collect();
        if blocks.is_empty() {
            return Vec::new();
        }
        let nodes = self.graph.get_all_nodes();
        let mut pairs = Vec::new();
        for rule in blocks {
            for source in nodes
                .iter()
                .filter(|n| n.ready && rule.matches_source(n.display_name(), n.node_type))
            {
                if self.graph.get_output_ports(source.id).is_empty() {
                    continue;
                }
                for target in nodes.iter().filter(|n| {
                    n.ready
                        && n.id != source.id
                        && rule.matches_target(n.display_name(), n.node_type, n.id)
                }) {
                    if self.graph.get_input_ports(target.id).is_empty() {
                        continue;
                    }
                    if !pairs
                        .iter()
                        .any(|(s, t, _)| *s == source.id && *t == target.id)
                    {
                        pairs.push((source.id, target.id, rule.id.clone()));
                    }
                }
            }
        }
        pairs
    }

    fn ports_blocked(&self, output_port_id: ObjectId, input_port_id: ObjectId) -> bool {
        self.blocking_rule_for_ports(output_port_id, input_port_id)
            .is_some()
    }

    pub fn blocking_rule_for_ports(
        &self,
        output_port_id: ObjectId,
        input_port_id: ObjectId,
    ) -> Option<&AutoConnectRule> {
        self.blocking_rule_between(output_port_id, input_port_id)
            .or_else(|| {
                let logical_out = self.logical_port(output_port_id);
                let logical_in = self.logical_port(input_port_id);
                if (logical_out, logical_in) == (output_port_id, input_port_id) {
                    return None;
                }
                self.blocking_rule_between(logical_out, logical_in)
            })
    }

    fn blocking_rule_between(
        &self,
        output_port_id: ObjectId,
        input_port_id: ObjectId,
    ) -> Option<&AutoConnectRule> {
        let out = self.graph.get_port(output_port_id)?;
        let inp = self.graph.get_port(input_port_id)?;
        let source = self.graph.get_node(out.node_id)?;
        let target = self.graph.get_node(inp.node_id)?;
        self.blocking_rule(&source, &target, &out, &inp)
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

        let mut rules: Vec<AutoConnectRule> = self
            .rules
            .iter()
            .filter(|r| r.is_block())
            .cloned()
            .collect();
        rules.extend(rule_map.into_values());
        self.rules = rules;
        self.rules_dirty = true;
    }

    pub fn remap_rules(
        &mut self,
        map: &EndpointMap,
        targets: bool,
        skip: &HashSet<String>,
    ) -> bool {
        let mut out: Vec<AutoConnectRule> = Vec::with_capacity(self.rules.len());
        let mut changed = false;
        for rule in std::mem::take(&mut self.rules) {
            let (node, other) = if targets {
                (rule.target_pattern.clone(), rule.source_pattern.clone())
            } else {
                (rule.source_pattern.clone(), rule.target_pattern.clone())
            };
            let Some(new_nodes) = map.nodes.get(&node) else {
                out.push(rule);
                continue;
            };
            if skip.contains(&other) {
                out.push(rule);
                continue;
            }
            let retarget = |rule: &AutoConnectRule, new_node: &str, mappings: Vec<PortMapping>| {
                let mut r = AutoConnectRule::new(
                    rule.source_pattern.clone(),
                    rule.source_node_type,
                    rule.target_pattern.clone(),
                    rule.target_node_type,
                    rule.target_node_id,
                )
                .with_action(rule.action);
                r.enabled = rule.enabled;
                if targets {
                    r.target_pattern = new_node.to_string();
                    r.target_node_type = None;
                    r.target_node_id = None;
                } else {
                    r.source_pattern = new_node.to_string();
                    r.source_node_type = None;
                }
                r.port_mappings = mappings;
                r
            };

            if rule.port_mappings.is_empty() {
                for n in new_nodes {
                    let r = retarget(&rule, n, Vec::new());
                    if !out.iter().any(|o| same_rule(o, &r)) {
                        out.push(r);
                    }
                }
                changed = true;
                continue;
            }

            let mut by_node: BTreeMap<String, Vec<PortMapping>> = BTreeMap::new();
            let mut keep: Vec<PortMapping> = Vec::new();
            for m in &rule.port_mappings {
                let port = if targets {
                    &m.input_port_name
                } else {
                    &m.output_port_name
                };
                match map.ports.get(&(node.clone(), port.clone())) {
                    Some(dests) => {
                        for (n, p) in dests {
                            let pm = if targets {
                                PortMapping {
                                    output_port_name: m.output_port_name.clone(),
                                    input_port_name: p.clone(),
                                }
                            } else {
                                PortMapping {
                                    output_port_name: p.clone(),
                                    input_port_name: m.input_port_name.clone(),
                                }
                            };
                            let list = by_node.entry(n.clone()).or_default();
                            if !list.contains(&pm) {
                                list.push(pm);
                            }
                        }
                    }
                    None => keep.push(m.clone()),
                }
            }
            if by_node.is_empty() {
                out.push(rule);
                continue;
            }
            changed = true;
            for (n, mappings) in by_node {
                out.push(retarget(&rule, &n, mappings));
            }
            if !keep.is_empty() {
                let mut r = rule;
                r.port_mappings = keep;
                out.push(r);
            }
        }
        self.rules = out;
        if changed {
            self.rules_dirty = true;
        }
        changed
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
        let mut wanted: Vec<(ObjectId, ObjectId)> = Vec::new();
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
                .filter(|r| {
                    r.enabled
                        && !r.is_block()
                        && r.matches_source(node.display_name(), node.node_type)
                })
                .collect();

            let rule_targets: Vec<(&AutoConnectRule, &Node)> = matching_rules
                .iter()
                .filter_map(|rule| {
                    self.find_matching_target(rule, &nodes, node.id)
                        .map(|target| (*rule, target))
                })
                .collect();

            if rule_targets.is_empty() {
                if node.node_type == Some(NodeType::StreamOutput)
                    && let Some(target) = self.default_target_node(&nodes, node.id)
                {
                    let target_ports = self.graph.get_input_ports(target.id);
                    for source_port in &output_ports {
                        if let Some(target_port) =
                            self.find_matching_port(source_port, &target_ports)
                        {
                            wanted.push((source_port.id, target_port.id));
                        }
                    }
                }
            } else {
                for (rule, target) in rule_targets {
                    wanted.extend(self.generate_connections(rule, target, &output_ports));
                }
            }
        }

        let mut requested: HashSet<(ObjectId, ObjectId)> = HashSet::new();
        for (logical_out, logical_in) in wanted {
            if self.ports_blocked(logical_out, logical_in) {
                continue;
            }
            let pair = (
                self.physical_port(logical_out),
                self.physical_port(logical_in),
            );
            if self.graph.find_link(pair.0, pair.1).is_some() {
                continue;
            }
            let rerouted = pair != (logical_out, logical_in);
            if rerouted
                && !self.exclusive
                && self.graph.find_link(logical_out, logical_in).is_some()
            {
                continue;
            }
            if rerouted && self.ports_blocked(pair.0, pair.1) {
                continue;
            }
            if requested.insert(pair) {
                commands.push(PwCommand::Connect {
                    output_port_id: pair.0,
                    input_port_id: pair.1,
                });
            }
        }

        let now = Instant::now();
        self.recent_destroys.retain(|_, times| {
            times.retain(|t| now.duration_since(*t) < DESTROY_WINDOW);
            !times.is_empty()
        });

        let links = self.graph.get_all_links();
        for link in &links {
            let blocked = self.link_blocked(link);
            let verdict = if self.exclusive {
                self.link_verdict(link, &nodes)
            } else {
                Verdict::Ungoverned
            };
            let reroute = !blocked
                && verdict == Verdict::Authorized
                && self.skips_redirect(link.output_port_id, link.input_port_id);
            if !blocked && verdict != Verdict::Unauthorized && !reroute {
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
                    times.push(now);
                }
                continue;
            }
            times.push(now);
            if reroute {
                let pair = (
                    self.physical_port(link.output_port_id),
                    self.physical_port(link.input_port_id),
                );
                if self.graph.find_link(pair.0, pair.1).is_none()
                    && !self.ports_blocked(pair.0, pair.1)
                    && requested.insert(pair)
                {
                    commands.push(PwCommand::Connect {
                        output_port_id: pair.0,
                        input_port_id: pair.1,
                    });
                }
            }
            commands.push(PwCommand::Disconnect { link_id: link.id });
        }

        commands
    }

    fn generate_connections(
        &self,
        rule: &AutoConnectRule,
        target: &Node,
        source_ports: &[Port],
    ) -> Vec<(ObjectId, ObjectId)> {
        let mut commands = Vec::new();
        let target_ports = self.graph.get_input_ports(target.id);

        if rule.port_mappings.is_empty() {
            for source_port in source_ports {
                if let Some(target_port) = self.find_matching_port(source_port, &target_ports) {
                    commands.push((source_port.id, target_port.id));
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
                    if let (Some(out_mt), Some(in_mt)) = (out_port.media_type, in_port.media_type)
                        && out_mt != in_mt
                    {
                        continue;
                    }
                    commands.push((out_port.id, in_port.id));
                }
            }
        }

        commands
    }

    fn find_matching_port<'a>(&self, source: &Port, targets: &'a [Port]) -> Option<&'a Port> {
        let compatible: Vec<&Port> = targets
            .iter()
            .filter(|p| match (source.media_type, p.media_type) {
                (Some(a), Some(b)) => a == b,
                _ => true,
            })
            .collect();

        if compatible.is_empty() {
            return None;
        }

        if let Some(ref channel) = source.channel
            && let Some(target) = compatible
                .iter()
                .find(|p| p.channel.as_ref() == Some(channel))
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

    fn default_target_node<'a>(&self, nodes: &'a [Node], exclude: ObjectId) -> Option<&'a Node> {
        let name = self.default_target.as_deref()?;
        nodes.iter().find(|n| {
            n.id != exclude
                && n.ready
                && n.node_type.map(|t| t.has_inputs()).unwrap_or(false)
                && n.display_name() == name
        })
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
            && node.node_type.map(|t| t.has_inputs()).unwrap_or(false)
        {
            return Some(node);
        }

        nodes.iter().find(|n| {
            n.id != exclude_node_id
                && n.ready
                && n.node_type.map(|t| t.has_inputs()).unwrap_or(false)
                && rule.matches_target(n.display_name(), n.node_type, n.id)
        })
    }

    fn clean_incompatible_mappings(&mut self) {
        let nodes = self.graph.get_all_nodes();
        let mut dirty = false;
        let mut emptied: Vec<String> = Vec::new();

        for rule in &mut self.rules {
            if rule.port_mappings.is_empty() {
                continue;
            }

            let target = nodes
                .iter()
                .find(|n| n.ready && rule.matches_target(n.display_name(), n.node_type, n.id));
            let Some(target) = target else { continue };

            let source = nodes
                .iter()
                .find(|n| n.ready && rule.matches_source(n.display_name(), n.node_type));
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
                                m.output_port_name,
                                a,
                                m.input_port_name,
                                b,
                            );
                            false
                        }
                        _ => true,
                    }
                } else {
                    true
                }
            });
            if rule.port_mappings.len() != before {
                dirty = true;
                if rule.port_mappings.is_empty() {
                    emptied.push(rule.id.clone());
                }
            }
        }

        if !emptied.is_empty() {
            self.rules.retain(|r| !emptied.contains(&r.id));
        }
        if dirty {
            self.rules_dirty = true;
        }
    }

    fn is_subgraph_internal(source: &Node, target: &Node) -> bool {
        use crate::pipewire::SubgraphRole;
        source
            .subgraph
            .as_ref()
            .is_some_and(|t| t.role == SubgraphRole::In)
            || target
                .subgraph
                .as_ref()
                .is_some_and(|t| t.role == SubgraphRole::Out)
    }

    fn is_routable_node(node: &Node) -> bool {
        if node
            .subgraph
            .as_ref()
            .is_some_and(|t| t.side == crate::pipewire::SubgraphSide::Internal)
        {
            return false;
        }
        !matches!(node.media_type, Some(MediaType::Video))
    }

    fn link_verdict(&self, link: &Link, nodes: &[Node]) -> Verdict {
        if self.is_redirect_internal(link.output_port_id, link.input_port_id)
            || self.standby_ports.contains(&link.output_port_id)
            || self.standby_ports.contains(&link.input_port_id)
        {
            return Verdict::Ungoverned;
        }
        let physical = self.pair_verdict(link.output_port_id, link.input_port_id, nodes);
        let logical_pair = (
            self.logical_port(link.output_port_id),
            self.logical_port(link.input_port_id),
        );
        if logical_pair == (link.output_port_id, link.input_port_id) {
            return physical;
        }
        let logical = self.pair_verdict(logical_pair.0, logical_pair.1, nodes);
        if physical == Verdict::Authorized || logical == Verdict::Authorized {
            Verdict::Authorized
        } else if physical == Verdict::Unauthorized || logical == Verdict::Unauthorized {
            Verdict::Unauthorized
        } else {
            Verdict::Ungoverned
        }
    }

    fn pair_verdict(
        &self,
        output_port_id: ObjectId,
        input_port_id: ObjectId,
        nodes: &[Node],
    ) -> Verdict {
        let (Some(out_p), Some(in_p)) = (
            self.graph.get_port(output_port_id),
            self.graph.get_port(input_port_id),
        ) else {
            return Verdict::Ungoverned;
        };
        let (Some(source_node), Some(target_node)) = (
            self.graph.get_node(out_p.node_id),
            self.graph.get_node(in_p.node_id),
        ) else {
            return Verdict::Ungoverned;
        };

        if !Self::is_routable_node(&source_node) || !Self::is_routable_node(&target_node) {
            return Verdict::Ungoverned;
        }
        if Self::is_subgraph_internal(&source_node, &target_node) {
            return Verdict::Ungoverned;
        }

        let source_rules: Vec<&AutoConnectRule> = self
            .rules
            .iter()
            .filter(|r| {
                r.enabled
                    && !r.is_block()
                    && r.matches_source(source_node.display_name(), source_node.node_type)
            })
            .collect();

        let any_target_present = source_rules.iter().any(|r| {
            self.find_matching_target(r, nodes, source_node.id)
                .is_some()
        });
        if !any_target_present {
            if source_node.node_type != Some(NodeType::StreamOutput) {
                return Verdict::Ungoverned;
            }
            return match self.default_target_node(nodes, source_node.id) {
                Some(d) if d.id == target_node.id => Verdict::Authorized,
                Some(_) => Verdict::Unauthorized,
                None => Verdict::Ungoverned,
            };
        }

        let authorized = source_rules.iter().any(|rule| {
            rule.matches_target(
                target_node.display_name(),
                target_node.node_type,
                target_node.id,
            ) && rule.covers_ports(&out_p.name, &in_p.name)
        });

        if authorized {
            Verdict::Authorized
        } else {
            Verdict::Unauthorized
        }
    }

    fn link_blocked(&self, link: &Link) -> bool {
        self.ports_blocked(link.output_port_id, link.input_port_id)
    }
}

#[derive(Debug, Default, Clone)]
pub struct EndpointMap {
    pub ports: HashMap<(String, String), Vec<(String, String)>>,
    pub nodes: HashMap<String, Vec<String>>,
}

impl EndpointMap {
    pub fn add(&mut self, from: (&str, &str), to: (&str, &str)) {
        let list = self
            .ports
            .entry((from.0.to_string(), from.1.to_string()))
            .or_default();
        let to_owned = (to.0.to_string(), to.1.to_string());
        if !list.contains(&to_owned) {
            list.push(to_owned);
        }
        let nodes = self.nodes.entry(from.0.to_string()).or_default();
        if !nodes.iter().any(|n| n == to.0) {
            nodes.push(to.0.to_string());
        }
    }
}

fn same_rule(a: &AutoConnectRule, b: &AutoConnectRule) -> bool {
    a.source_pattern == b.source_pattern
        && a.target_pattern == b.target_pattern
        && a.action == b.action
        && a.port_mappings == b.port_mappings
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
            subgraph: None,
            vdev_id: None,
        }
    }

    fn port(
        id: ObjectId,
        node_id: ObjectId,
        name: &str,
        dir: PortDirection,
        mt: MediaType,
    ) -> Port {
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
        let mut rule = AutoConnectRule::new(
            "App",
            Some(NodeType::StreamOutput),
            "Headphones",
            Some(NodeType::Sink),
            Some(2),
        );
        rule.add_port_mapping(out.name.clone(), "midi_in".into());
        let user = AutoConnectRule::new("App", None, "Speakers", None, None);
        pb.set_rules(vec![rule, user]);
        let cmds = pb.scan();
        assert_eq!(pb.rules().len(), 1);
        assert_eq!(pb.rules()[0].target_pattern, "Speakers");
        assert!(!connects(&cmds).contains(&(10, 20)));
    }

    #[test]
    fn links_from_unruled_sources_are_not_removed() {
        let (graph, out, hp_in, _) = setup();
        graph.insert_node(node(4, "Other", NodeType::StreamOutput));
        let other_out = port(40, 4, "output_FL", PortDirection::Output, MediaType::Audio);
        graph.insert_port(other_out.clone());
        graph.insert_link(link(100, &other_out, &hp_in));
        graph.insert_link(link(101, &out, &hp_in));
        let mut pb = PatchbayManager::new(graph.clone());
        pb.add_rule(AutoConnectRule::new(
            "App",
            None,
            "Headphones",
            None,
            Some(2),
        ));
        let cmds = pb.scan();
        assert!(disconnects(&cmds).is_empty());
    }

    #[test]
    fn ruled_source_links_kept_while_target_absent() {
        let (graph, out, _, sp_in) = setup();
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
        assert!(disconnects(&pb.scan()).is_empty());
        assert!(disconnects(&pb.scan()).is_empty());
    }

    #[test]
    fn non_exclusive_keeps_unauthorized_links_but_still_connects() {
        let (graph, out, hp_in, sp_in) = setup();
        graph.insert_link(link(100, &out, &sp_in));
        let mut pb = PatchbayManager::new(graph.clone());
        pb.exclusive = false;
        pb.add_rule(AutoConnectRule::new("App", None, "Headphones", None, None));
        let cmds = pb.scan();
        assert!(disconnects(&cmds).is_empty());
        assert_eq!(connects(&cmds), vec![(out.id, hp_in.id)]);
    }

    fn block(source: &str, target: &str) -> AutoConnectRule {
        AutoConnectRule::new(source, None, target, None, None).with_action(RuleAction::Block)
    }

    #[test]
    fn blocked_link_removed_even_when_not_exclusive() {
        let (graph, out, _, sp_in) = setup();
        graph.insert_link(link(100, &out, &sp_in));
        let mut pb = PatchbayManager::new(graph.clone());
        pb.exclusive = false;
        pb.add_rule(block("App", "Speakers"));
        assert_eq!(disconnects(&pb.scan()), vec![100]);
    }

    #[test]
    fn blocked_link_removed_with_loop_guard() {
        let (graph, out, _, sp_in) = setup();
        graph.insert_link(link(100, &out, &sp_in));
        let mut pb = PatchbayManager::new(graph.clone());
        pb.add_rule(block("App", "Speakers"));
        for _ in 0..MAX_DESTROYS_PER_WINDOW {
            assert_eq!(disconnects(&pb.scan()), vec![100]);
        }
        assert!(disconnects(&pb.scan()).is_empty());
    }

    #[test]
    fn block_beats_connect_rule() {
        let (graph, _, _, _) = setup();
        let mut pb = PatchbayManager::new(graph.clone());
        pb.add_rule(AutoConnectRule::new("App", None, "Speakers", None, None));
        pb.add_rule(block("App", "Speakers"));
        assert!(connects(&pb.scan()).is_empty());
    }

    #[test]
    fn block_beats_default_target() {
        let (graph, _, _, _) = setup();
        let mut pb = PatchbayManager::new(graph.clone());
        pb.set_default_target(Some("Speakers".into()));
        pb.add_rule(block("App", "Speakers"));
        assert!(connects(&pb.scan()).is_empty());
    }

    #[test]
    fn block_rule_alone_does_not_govern_other_links() {
        let (graph, out, hp_in, _) = setup();
        graph.insert_link(link(100, &out, &hp_in));
        let mut pb = PatchbayManager::new(graph.clone());
        pb.add_rule(block("App", "Speakers"));
        assert!(disconnects(&pb.scan()).is_empty());
    }

    #[test]
    fn wildcard_block() {
        let (graph, out, hp_in, sp_in) = setup();
        graph.insert_link(link(100, &out, &hp_in));
        graph.insert_link(link(101, &out, &sp_in));
        let mut pb = PatchbayManager::new(graph.clone());
        pb.add_rule(block("App", "*"));
        let mut d = disconnects(&pb.scan());
        d.sort();
        assert_eq!(d, vec![100, 101]);
    }

    #[test]
    fn disabled_block_is_ignored() {
        let (graph, out, _, sp_in) = setup();
        graph.insert_link(link(100, &out, &sp_in));
        let mut pb = PatchbayManager::new(graph.clone());
        let rule = block("App", "Speakers");
        let id = rule.id.clone();
        pb.add_rule(rule);
        pb.toggle_rule(&id);
        assert!(disconnects(&pb.scan()).is_empty());
    }

    #[test]
    fn block_pair_replaces_connect_rules_and_stays() {
        let (graph, out, _, sp_in) = setup();
        let mut pb = PatchbayManager::new(graph.clone());
        let app = graph.get_node(1).unwrap();
        let sp = graph.get_node(3).unwrap();
        assert!(pb.learn_from_link(&app, &sp, &out, &sp_in));
        assert!(pb.block_pair(&app, &sp));
        assert_eq!(pb.rules().len(), 1);
        assert!(pb.rules()[0].is_block());
        assert!(!pb.block_pair(&app, &sp));
        assert!(!pb.learn_from_link(&app, &sp, &out, &sp_in));
        assert_eq!(pb.rules().len(), 1);
        assert!(pb.is_blocked(&app, &sp, &out, &sp_in));
    }

    #[test]
    fn wildcard_block_blocks_every_target() {
        let (graph, out, hp_in, sp_in) = setup();
        let mut pb = PatchbayManager::new(graph.clone());
        pb.add_rule(block("App", "*"));
        let app = graph.get_node(1).unwrap();
        let hp = graph.get_node(2).unwrap();
        let sp = graph.get_node(3).unwrap();
        assert!(pb.is_blocked(&app, &hp, &out, &hp_in));
        assert!(pb.is_blocked(&app, &sp, &out, &sp_in));
        assert!(!pb.learn_from_link(&app, &sp, &out, &sp_in));
    }

    #[test]
    fn blocked_pairs_lists_present_node_pairs() {
        let (graph, _, _, _) = setup();
        let mut pb = PatchbayManager::new(graph.clone());
        let rule = block("App", "Speakers");
        let id = rule.id.clone();
        pb.add_rule(rule);
        pb.add_rule(block("Missing", "Speakers"));
        assert_eq!(pb.blocked_pairs(), vec![(1, 3, id.clone())]);
        pb.toggle_rule(&id);
        assert!(pb.blocked_pairs().is_empty());
    }

    #[test]
    fn unlearn_leaves_block_rules() {
        let (graph, out, _, sp_in) = setup();
        let mut pb = PatchbayManager::new(graph.clone());
        let app = graph.get_node(1).unwrap();
        let sp = graph.get_node(3).unwrap();
        pb.block_pair(&app, &sp);
        assert!(!pb.unlearn_from_link(&app, &sp, &out, &sp_in));
        assert_eq!(pb.rules().len(), 1);
    }

    #[test]
    fn snapshot_keeps_block_rules() {
        let (graph, out, hp_in, _) = setup();
        graph.insert_link(link(100, &out, &hp_in));
        let mut pb = PatchbayManager::new(graph.clone());
        pb.add_rule(block("App", "Speakers"));
        pb.snapshot_current_connections();
        assert_eq!(pb.rules().iter().filter(|r| r.is_block()).count(), 1);
        assert_eq!(pb.rules().len(), 2);
    }

    #[test]
    fn old_rules_json_defaults_to_connect() {
        let json = r#"{"id":"x","source_pattern":"App","source_node_type":null,
            "target_pattern":"Speakers","target_node_type":null,"target_node_id":null,
            "port_mappings":[],"enabled":true}"#;
        let rule: AutoConnectRule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.action, RuleAction::Connect);
    }

    fn endpoint_map(entries: &[(&str, &str, &str, &str)]) -> EndpointMap {
        let mut m = EndpointMap::default();
        for (on, op, nn, np) in entries {
            m.add((on, op), (nn, np));
        }
        m
    }

    #[test]
    fn remap_targets_moves_mapped_rules_to_subgraph_edge() {
        let (graph, _, _, _) = setup();
        let mut pb = PatchbayManager::new(graph);
        let mut learned =
            AutoConnectRule::new("Firefox", None, "EQ", Some(NodeType::Plugin), Some(7));
        learned.add_port_mapping("output_FL".into(), "in_l".into());
        learned.add_port_mapping("output_FR".into(), "in_r".into());
        pb.add_rule(learned);
        pb.add_rule(AutoConnectRule::new("Gate", None, "EQ", None, None));
        pb.add_rule(AutoConnectRule::new("Spotify", None, "EQ", None, None));
        let map = endpoint_map(&[
            ("EQ", "in_l", "Chain In", "in_FL"),
            ("EQ", "in_r", "Chain In", "in_FR"),
        ]);
        let skip: HashSet<String> = ["Gate".to_string(), "EQ".to_string()].into();
        assert!(pb.remap_rules(&map, true, &skip));
        let rules = pb.rules();
        assert_eq!(rules.len(), 3);
        let ff = rules
            .iter()
            .find(|r| r.source_pattern == "Firefox")
            .unwrap();
        assert_eq!(ff.target_pattern, "Chain In");
        assert_eq!(ff.target_node_id, None);
        assert_eq!(
            ff.port_mappings
                .iter()
                .map(|m| m.input_port_name.as_str())
                .collect::<Vec<_>>(),
            ["in_FL", "in_FR"]
        );
        assert!(
            rules
                .iter()
                .any(|r| r.source_pattern == "Gate" && r.target_pattern == "EQ")
        );
        assert!(
            rules
                .iter()
                .any(|r| r.source_pattern == "Spotify" && r.target_pattern == "Chain In")
        );
    }

    #[test]
    fn remap_sources_fans_out_on_ungroup() {
        let (graph, _, _, _) = setup();
        let mut pb = PatchbayManager::new(graph);
        let mut r = AutoConnectRule::new("Chain Out", None, "Headset", None, None);
        r.add_port_mapping("out_MONO".into(), "playback_FL".into());
        pb.add_rule(r);
        let map = endpoint_map(&[("Chain Out", "out_MONO", "Comp", "output_0")]);
        assert!(pb.remap_rules(&map, false, &HashSet::new()));
        let r = &pb.rules()[0];
        assert_eq!(
            (
                r.source_pattern.as_str(),
                r.port_mappings[0].output_port_name.as_str()
            ),
            ("Comp", "output_0")
        );
    }

    fn edge(id: ObjectId, name: &str, role: crate::pipewire::SubgraphRole) -> Node {
        let mut n = node(id, name, NodeType::Duplex);
        n.subgraph = Some(crate::pipewire::SubgraphTag {
            id: "sg".into(),
            role,
            side: crate::pipewire::SubgraphSide::Both,
        });
        n
    }

    #[test]
    fn subgraph_internal_wiring_is_neither_learned_nor_removed() {
        let (graph, out, _, _) = setup();
        graph.insert_node(edge(5, "Chain In", crate::pipewire::SubgraphRole::In));
        let in_out = port(50, 5, "out_FL", PortDirection::Output, MediaType::Audio);
        graph.insert_port(in_out.clone());
        graph.insert_node(node(6, "Gate", NodeType::Plugin));
        let gate_in = port(60, 6, "input_0", PortDirection::Input, MediaType::Audio);
        graph.insert_port(gate_in.clone());
        graph.insert_link(link(100, &in_out, &gate_in));
        let mut pb = PatchbayManager::new(graph.clone());
        pb.add_rule(AutoConnectRule::new(
            "Chain In", None, "Speakers", None, None,
        ));
        assert!(!disconnects(&pb.scan()).contains(&100));
        let (e, g) = (graph.get_node(5).unwrap(), graph.get_node(6).unwrap());
        assert!(!pb.learn_from_link(&e, &g, &in_out, &gate_in));
        let _ = out;
    }

    #[test]
    fn internal_side_of_device_subgraph_is_not_routable() {
        let mut n = node(9, "Chain In (internal)", NodeType::StreamOutput);
        n.subgraph = Some(crate::pipewire::SubgraphTag {
            id: "sg".into(),
            role: crate::pipewire::SubgraphRole::In,
            side: crate::pipewire::SubgraphSide::Internal,
        });
        assert!(!PatchbayManager::is_routable_node(&n));
        n.subgraph.as_mut().unwrap().side = crate::pipewire::SubgraphSide::External;
        assert!(PatchbayManager::is_routable_node(&n));
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
    fn exclusive_default_target_removes_other_links() {
        let (graph, out, hp_in, sp_in) = setup();
        graph.insert_link(link(100, &out, &hp_in));
        graph.insert_link(link(101, &out, &sp_in));
        let mut pb = PatchbayManager::new(graph.clone());
        pb.set_default_target(Some("Speakers".into()));
        assert_eq!(disconnects(&pb.scan()), vec![100]);
        pb.exclusive = false;
        assert!(disconnects(&pb.scan()).is_empty());
    }

    #[test]
    fn default_target_absent_keeps_links() {
        let (graph, out, hp_in, _) = setup();
        graph.insert_link(link(100, &out, &hp_in));
        let mut pb = PatchbayManager::new(graph.clone());
        pb.set_default_target(Some("Missing".into()));
        assert!(disconnects(&pb.scan()).is_empty());
    }

    #[test]
    fn no_rules_no_default_does_nothing() {
        let (graph, out, _, sp_in) = setup();
        graph.insert_link(link(100, &out, &sp_in));
        let mut pb = PatchbayManager::new(graph);
        assert!(pb.scan().is_empty());
    }

    fn meter(graph: &GraphState) -> (Port, Port) {
        graph.insert_node(node(5, "Headphones in meter", NodeType::Plugin));
        let m_in = port(50, 5, "input_0", PortDirection::Input, MediaType::Audio);
        let m_out = port(51, 5, "output_0", PortDirection::Output, MediaType::Audio);
        graph.insert_port(m_in.clone());
        graph.insert_port(m_out.clone());
        (m_in, m_out)
    }

    #[test]
    fn default_target_routes_through_snapped_meter() {
        let (graph, _, hp_in, _) = setup();
        let (m_in, m_out) = meter(&graph);
        graph.insert_link(link(200, &m_out, &hp_in));
        let mut pb = PatchbayManager::new(graph.clone());
        pb.set_default_target(Some("Headphones".into()));
        pb.set_port_redirects(HashMap::from([(hp_in.id, m_in.id)]), HashSet::new());
        let cmds = pb.scan();
        assert_eq!(connects(&cmds), vec![(10, m_in.id)]);
        assert!(disconnects(&cmds).is_empty());
    }

    #[test]
    fn link_through_snapped_meter_counts_as_link_to_its_port() {
        let (graph, out, hp_in, _) = setup();
        let (m_in, m_out) = meter(&graph);
        graph.insert_link(link(200, &m_out, &hp_in));
        graph.insert_link(link(201, &out, &m_in));
        let mut pb = PatchbayManager::new(graph.clone());
        pb.set_default_target(Some("Headphones".into()));
        pb.set_port_redirects(HashMap::from([(hp_in.id, m_in.id)]), HashSet::new());
        assert!(pb.scan().is_empty());
        pb.set_port_redirects(HashMap::new(), HashSet::new());
        assert_eq!(disconnects(&pb.scan()), vec![201]);
    }

    #[test]
    fn wires_on_a_snapped_meter_waiting_for_its_port_are_kept() {
        let (graph, out, _, _) = setup();
        let (m_in, _) = meter(&graph);
        graph.insert_link(link(201, &out, &m_in));
        let mut pb = PatchbayManager::new(graph.clone());
        pb.set_default_target(Some("Headphones".into()));
        pb.set_port_redirects(HashMap::new(), HashSet::from([m_in.id]));
        assert!(disconnects(&pb.scan()).is_empty());
    }

    #[test]
    fn exclusive_moves_direct_link_onto_snapped_meter() {
        let (graph, out, hp_in, _) = setup();
        let (m_in, m_out) = meter(&graph);
        graph.insert_link(link(200, &m_out, &hp_in));
        graph.insert_link(link(202, &out, &hp_in));
        let mut pb = PatchbayManager::new(graph.clone());
        pb.set_default_target(Some("Headphones".into()));
        pb.set_port_redirects(HashMap::from([(hp_in.id, m_in.id)]), HashSet::new());
        let cmds = pb.scan();
        assert_eq!(connects(&cmds), vec![(10, m_in.id)]);
        assert_eq!(disconnects(&cmds), vec![202]);
        pb.exclusive = false;
        assert!(pb.scan().is_empty());
    }

    #[test]
    fn meter_snapped_to_ruled_source_keeps_its_wires() {
        let (graph, out, hp_in, _) = setup();
        let (m_in, m_out) = meter(&graph);
        graph.insert_link(link(200, &out, &m_in));
        graph.insert_link(link(201, &m_out, &hp_in));
        let mut pb = PatchbayManager::new(graph.clone());
        pb.add_rule(AutoConnectRule::new("App", None, "Headphones", None, None));
        pb.set_port_redirects(HashMap::from([(out.id, m_out.id)]), HashSet::new());
        assert!(pb.scan().is_empty());
    }

    #[test]
    fn block_applies_through_snapped_meter() {
        let (graph, out, hp_in, _) = setup();
        let (m_in, m_out) = meter(&graph);
        graph.insert_link(link(200, &m_out, &hp_in));
        graph.insert_link(link(201, &out, &m_in));
        let mut pb = PatchbayManager::new(graph.clone());
        pb.add_rule(
            AutoConnectRule::new("App", None, "Headphones", None, None)
                .with_action(RuleAction::Block),
        );
        pb.set_port_redirects(HashMap::from([(hp_in.id, m_in.id)]), HashSet::new());
        assert!(pb.blocking_rule_for_ports(out.id, m_in.id).is_some());
        assert_eq!(disconnects(&pb.scan()), vec![201]);
    }

    #[test]
    fn rules_whose_targets_are_gone_fall_back_to_default() {
        let (graph, out, hp_in, sp_in) = setup();
        graph.insert_link(link(100, &out, &hp_in));
        let mut pb = PatchbayManager::new(graph.clone());
        pb.add_rule(AutoConnectRule::new(
            "App",
            None,
            "Meter (2 ch)",
            Some(NodeType::Plugin),
            None,
        ));
        pb.set_default_target(Some("Speakers".into()));
        let cmds = pb.scan();
        assert_eq!(connects(&cmds), vec![(10, sp_in.id)]);
        assert_eq!(disconnects(&cmds), vec![100]);
    }

    #[test]
    fn rename_node_moves_rules_and_merges_twins() {
        let (graph, _, _, _) = setup();
        let mut pb = PatchbayManager::new(graph);
        let mut old = AutoConnectRule::new(
            "App",
            Some(NodeType::StreamOutput),
            "Meter (2 ch)",
            Some(NodeType::Plugin),
            Some(5),
        );
        old.add_port_mapping("output_FL".into(), "input_0".into());
        let mut twin = AutoConnectRule::new(
            "App",
            Some(NodeType::StreamOutput),
            "Headphones in meter",
            Some(NodeType::Plugin),
            None,
        );
        twin.add_port_mapping("output_FR".into(), "input_1".into());
        let from_meter = AutoConnectRule::new(
            "Meter (2 ch)",
            Some(NodeType::Plugin),
            "Headphones",
            Some(NodeType::Sink),
            Some(2),
        );
        let sink_named_alike = AutoConnectRule::new(
            "App",
            Some(NodeType::StreamOutput),
            "Meter (2 ch)",
            Some(NodeType::Sink),
            None,
        );
        pb.set_rules(vec![old, twin, from_meter, sink_named_alike]);
        assert!(pb.rename_node("Meter (2 ch)", "Headphones in meter"));
        assert_eq!(pb.rules().len(), 3);
        assert_eq!(pb.rules()[0].target_pattern, "Headphones in meter");
        assert_eq!(pb.rules()[0].port_mappings.len(), 2);
        assert_eq!(pb.rules()[1].source_pattern, "Headphones in meter");
        assert_eq!(pb.rules()[2].target_pattern, "Meter (2 ch)");
        assert!(!pb.rename_node("Nothing", "Else"));
    }

    #[test]
    fn forgetting_a_plugin_node_drops_only_its_rules() {
        let (graph, _, _, _) = setup();
        let mut pb = PatchbayManager::new(graph);
        pb.set_rules(vec![
            AutoConnectRule::new("App", None, "Meter (2 ch)", Some(NodeType::Plugin), None),
            AutoConnectRule::new("Meter (2 ch)", Some(NodeType::Plugin), "Headphones", None, None),
            AutoConnectRule::new("App", None, "Headphones", None, None),
        ]);
        let present = HashSet::from(["Meter (4 ch)".to_string()]);
        assert!(!pb.forget_absent_plugin_nodes(|n| n == "Meter (4 ch)", &present));
        assert!(pb.forget_absent_plugin_nodes(|n| n == "Meter (2 ch)", &HashSet::new()));
        assert_eq!(pb.rules().len(), 1);
        assert_eq!(pb.rules()[0].target_pattern, "Headphones");
    }
}
