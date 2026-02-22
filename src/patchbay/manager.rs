use std::sync::Arc;

use super::rules::{pattern_matches, AutoConnectRule};
use crate::pipewire::{GraphState, Link, MediaType, Node, NodeType, ObjectId, Port, PwCommand};

/// Patchbay manager that applies routing rules
pub struct PatchbayManager {
    /// Reference to the graph state
    graph: Arc<GraphState>,
    /// Auto-connect rules
    rules: Vec<AutoConnectRule>,
    /// Whether rules are enabled
    pub enabled: bool,
    /// Set to true whenever rules change — the UI uses this to trigger a save
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

    // ── Rule CRUD ──────────────────────────────────────────────────────────

    /// Replace all rules.
    pub fn set_rules(&mut self, rules: Vec<AutoConnectRule>) {
        self.rules = rules;
        self.rules_dirty = true;
    }

    /// Add a rule.
    pub fn add_rule(&mut self, rule: AutoConnectRule) {
        self.rules.push(rule);
        self.rules_dirty = true;
    }

    /// Remove a rule by ID.
    pub fn remove_rule(&mut self, id: &str) {
        self.rules.retain(|r| r.id != id);
        self.rules_dirty = true;
    }

    /// Get all rules (immutable).
    pub fn rules(&self) -> &[AutoConnectRule] {
        &self.rules
    }

    /// Toggle a rule's enabled state. Returns the new state.
    pub fn toggle_rule(&mut self, id: &str) -> Option<bool> {
        if let Some(rule) = self.rules.iter_mut().find(|r| r.id == id) {
            rule.enabled = !rule.enabled;
            self.rules_dirty = true;
            Some(rule.enabled)
        } else {
            None
        }
    }

    // ── Auto-learn ─────────────────────────────────────────────────────────

    /// Create or update a rule from a manual user connection.
    ///
    /// If a rule already exists for the same source→target pair, the port
    /// mapping is added to it.  Otherwise a new rule is created.
    ///
    /// One source can have rules to **multiple** different targets — e.g.
    /// Firefox → Headphones AND Firefox → Recording Sink.
    ///
    /// Returns `true` if the rule set changed.
    pub fn learn_from_link(
        &mut self,
        source_node: &Node,
        target_node: &Node,
        output_port: &Port,
        input_port: &Port,
    ) -> bool {
        // Only learn rules for routable nodes (skip video)
        if !Self::is_routable_node(source_node) || !Self::is_routable_node(target_node) {
            return false;
        }

        let source_name = source_node.display_name().to_string();

        // Look for an existing rule matching this exact source → target pair
        let existing = self.rules.iter_mut().find(|r| {
            r.source_pattern == source_name
                && r.matches_target(
                    target_node.display_name(),
                    target_node.node_type,
                    target_node.id,
                )
        });

        if let Some(rule) = existing {
            // Rule for this source→target already exists — add the port mapping
            let changed = rule.add_port_mapping(output_port.name.clone(), input_port.name.clone());
            if changed {
                self.rules_dirty = true;
            }
            return changed;
        }

        // No existing rule for this source→target — create one
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

    // ── Auto-unlearn ────────────────────────────────────────────────────────

    /// Remove a port mapping from rules when a user manually disconnects a link.
    ///
    /// Finds rules matching this source→target pair and removes the specific
    /// port mapping.  If the rule has no remaining port mappings, the entire
    /// rule is removed.
    ///
    /// Returns `true` if the rule set changed.
    pub fn unlearn_from_link(
        &mut self,
        source_node: &Node,
        target_node: &Node,
        output_port: &Port,
        input_port: &Port,
    ) -> bool {
        // Only unlearn rules for routable nodes (skip video)
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

            // Remove the specific port mapping
            let before = rule.port_mappings.len();
            rule.port_mappings.retain(|m| {
                !(m.output_port_name == output_port.name && m.input_port_name == input_port.name)
            });
            if rule.port_mappings.len() != before {
                changed = true;
            }
        }

        // Remove rules that have no port mappings left
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

    // ── Snapshot current connections ────────────────────────────────────────

    /// Replace all rules with a snapshot of the current graph connections.
    ///
    /// Each unique (source display name, target display name, target node type)
    /// triple becomes one rule, with all port-to-port links collected as
    /// port mappings on that rule.
    pub fn snapshot_current_connections(&mut self) {
        use std::collections::HashMap;

        let links = self.graph.get_all_links();

        // Group links by (source_display_name, target_display_name, target_node_type, target_id)
        // into a single rule with accumulated port mappings.
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
                // Only snapshot routable links (skip video)
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

    // ── Node ID refresh ──────────────────────────────────────────────────

    /// Refresh stale `target_node_id` values in rules.
    ///
    /// When a node (e.g. a game) restarts, it gets a new PipeWire node ID.
    /// Rules that stored the old ID would fail to match on exact ID and
    /// fall back to name+type matching — but worse, the stale ID could be
    /// reused by a completely different node, causing false matches.
    ///
    /// This method checks each rule's `target_node_id`:
    /// - If the ID still points to a node whose display name matches the
    ///   rule's target pattern, it's fine.
    /// - Otherwise, search all nodes for one matching the target pattern +
    ///   type and update the ID (or clear it if no match is found).
    pub fn refresh_target_ids(&mut self) {
        let nodes = self.graph.get_all_nodes();
        let mut dirty = false;

        for rule in &mut self.rules {
            if let Some(old_id) = rule.target_node_id {
                // Check if the stored ID still points to the right node
                let id_still_valid = nodes.iter().any(|n| {
                    n.id == old_id
                        && n.ready
                        && pattern_matches(&rule.target_pattern, n.display_name())
                });

                if !id_still_valid {
                    // Find the node by name + type instead
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

    // ── Scan & apply ───────────────────────────────────────────────────────

    /// Scan the graph and generate commands to apply rules.
    ///
    /// Returns a list of Connect/Disconnect commands.  Connections are
    /// generated before disconnections ("make-before-break").
    pub fn scan(&mut self) -> Vec<PwCommand> {
        if !self.enabled || self.rules.is_empty() {
            return Vec::new();
        }

        // Refresh stale target node IDs before evaluating rules.
        // Nodes get new IDs when they restart (e.g. game relaunch).
        self.refresh_target_ids();

        let mut commands = Vec::new();
        let nodes = self.graph.get_all_nodes();

        // 1. Generate connection commands (skip video-only nodes)
        for node in &nodes {
            if !node.ready {
                continue;
            }

            // Only process routable nodes — leave video nodes alone
            if !Self::is_routable_node(node) {
                continue;
            }

            // Only process nodes that have output ports
            if !node.node_type.map(|t| t.has_outputs()).unwrap_or(false) {
                continue;
            }

            let output_ports = self.graph.get_output_ports(node.id);
            if output_ports.is_empty() {
                continue;
            }

            // Find matching rules (match by display_name + node_type)
            let matching_rules: Vec<&AutoConnectRule> = self
                .rules
                .iter()
                .filter(|r| r.enabled && r.matches_source(node.display_name(), node.node_type))
                .collect();

            // Apply matching rules
            for rule in &matching_rules {
                if let Some(target) = self.find_matching_target(rule, &nodes) {
                    commands.extend(self.generate_connections(rule, target, &output_ports));
                }
            }
        }

        // 2. Generate disconnection commands (only for nodes that have rules)
        let links = self.graph.get_all_links();
        for link in &links {
            if self.should_remove_link(link) {
                commands.push(PwCommand::Disconnect { link_id: link.id });
            }
        }

        commands
    }

    /// Generate connection commands for a source-target pair.
    ///
    /// If the rule has explicit port mappings, only those specific port pairs
    /// are connected.  Otherwise falls back to heuristic matching (channel
    /// name, port name, positional).
    fn generate_connections(
        &self,
        rule: &AutoConnectRule,
        target: &Node,
        source_ports: &[Port],
    ) -> Vec<PwCommand> {
        let mut commands = Vec::new();
        let target_ports = self.graph.get_input_ports(target.id);

        if rule.port_mappings.is_empty() {
            // Heuristic fallback: match by channel/name/position
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
            // Explicit port mappings: connect exactly the listed pairs
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

    /// Find a matching target port for a source port.
    fn find_matching_port<'a>(&self, source: &Port, targets: &'a [Port]) -> Option<&'a Port> {
        // First try exact channel name match
        if let Some(ref channel) = source.channel
            && let Some(target) = targets.iter().find(|p| p.channel.as_ref() == Some(channel))
        {
            return Some(target);
        }

        // Try port name match
        if let Some(target) = targets.iter().find(|p| p.name == source.name) {
            return Some(target);
        }

        // Fallback: match by position (first output to first input, etc.)
        let source_index = source.physical_index.unwrap_or(0);
        targets
            .iter()
            .find(|p| p.physical_index.unwrap_or(0) == source_index)
            .or_else(|| targets.first())
    }

    /// Find a target node matching a rule.
    ///
    /// Priority: exact node ID → display name + node type fallback.
    fn find_matching_target<'a>(
        &self,
        rule: &AutoConnectRule,
        nodes: &'a [Node],
    ) -> Option<&'a Node> {
        // First: try exact node ID match (if the rule has one)
        if let Some(target_id) = rule.target_node_id 
            && let Some(node) = nodes.iter().find(|n| n.id == target_id && n.ready) 
            // Verify the node still has input ports
            && node.node_type.map(|t| t.has_inputs()).unwrap_or(false) {
                return Some(node);
        }

        // Fallback: display name + node type matching
        nodes.iter().find(|n| {
            n.ready
                && n.node_type.map(|t| t.has_inputs()).unwrap_or(false)
                && rule.matches_target(n.display_name(), n.node_type, n.id)
        })
    }

    /// Returns true if the node should be managed by the patchbay.
    /// Only Video nodes are excluded — audio and MIDI are both managed.
    fn is_routable_node(node: &Node) -> bool {
        match node.media_type {
            Some(MediaType::Video) => false,
            _ => true, // Audio, Midi, or unknown → managed by patchbay
        }
    }

    /// Check if a link should be removed according to rules.
    ///
    /// A link is removed if **either** of these conditions holds:
    ///
    /// **Source-side check (existing):**
    /// 1. We have at least one enabled rule for the source node, AND
    /// 2. No rule authorizes this specific connection.
    ///
    /// **Target-side check (new):**
    /// 1. The target node is the target of at least one enabled rule, AND
    /// 2. No rule authorizes this specific link's source→target pair.
    ///
    /// The target-side check handles the case where WirePlumber (or another
    /// session manager) connects a default source to a target node, but
    /// our rules say that target should receive from a different source
    /// (e.g. rule says "LSP Mixer → Game Input" but WirePlumber connected
    /// "Mic → Game Input").
    ///
    /// Links involving nodes that have no rules at all are left untouched.
    fn should_remove_link(&self, link: &Link) -> bool {
        // Get the source node
        let source_node = match self.graph.get_node(link.output_node_id) {
            Some(n) => n,
            None => return false,
        };

        // Get the target node
        let target_node = match self.graph.get_node(link.input_node_id) {
            Some(n) => n,
            None => return false,
        };

        // Never touch video links — the patchbay manages audio and MIDI
        // routing but leaves video links to whatever created them
        // (e.g. screen capture portals).
        if !Self::is_routable_node(&source_node) || !Self::is_routable_node(&target_node) {
            return false;
        }

        let out_port = self.graph.get_port(link.output_port_id);
        let in_port = self.graph.get_port(link.input_port_id);

        // Helper: check if any rule authorizes this exact link
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

            // Node matches. Now check port mappings.
            if rule.port_mappings.is_empty() {
                // No explicit port mappings — any link to this target is authorized
                return true;
            }

            // Check if this specific port pair is in the rule's mappings
            if let (Some(out_p), Some(in_p)) = (&out_port, &in_port) {
                rule.port_mappings
                    .iter()
                    .any(|m| m.output_port_name == out_p.name && m.input_port_name == in_p.name)
            } else {
                false
            }
        };

        // ── Source-side check ───────────────────────────────────────────
        // If the source has rules, this link must be authorized by one.
        let has_any_rule_for_source = self.rules.iter().any(|r| {
            r.enabled && r.matches_source(source_node.display_name(), source_node.node_type)
        });

        if has_any_rule_for_source {
            let authorized = self.rules.iter().any(|r| link_authorized_by(r));
            if !authorized {
                return true;
            }
        }

        // ── Target-side check ───────────────────────────────────────────
        // If the target is claimed by any rule (i.e. some rule says
        // "X → this target"), then only links from authorized sources
        // are allowed.  This removes links created by WirePlumber or
        // other session managers that don't match our rules.
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
