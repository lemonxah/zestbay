//! Patchbay routing rules
//!
//! Defines rules for automatic connection management.
//! Rules match nodes by display name (the human-readable name shown in the UI).
//!
//! ## Source matching
//! Source side uses `NodeType` + display name pattern (glob).
//! All nodes with the same display name are routed identically — e.g. every
//! "Firefox" window routes to the same target.
//!
//! ## Target matching
//! Target side uses `NodeType` + display name + PipeWire node ID.
//! The node ID is the most specific — if the target disappears and reappears
//! with a different ID (e.g. USB replug), we fall back to display name +
//! node type matching.

use serde::{Deserialize, Serialize};

use crate::pipewire::{NodeType, ObjectId};

/// A specific port-to-port mapping within a rule.
///
/// Uses port names (e.g. `playback_FL`, `monitor_FR`) which are stable
/// across sessions — unlike port IDs which change every time PipeWire
/// restarts or a node reappears.
///
/// Multiple mappings per rule are allowed:
/// - FL → FL, FR → FR (stereo)
/// - FL → AUX_L, FR → AUX_R (cross-wiring)
/// - FL → mono (fan-in)
///
/// One output port can appear in multiple mappings (fan-out to several
/// input ports on the same target node).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PortMapping {
    /// PipeWire port name on the source node (output port)
    pub output_port_name: String,
    /// PipeWire port name on the target node (input port)
    pub input_port_name: String,
}

/// A rule for automatically connecting nodes.
///
/// Created either by auto-learning from user drag-to-connect actions
/// or by snapshotting all current connections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoConnectRule {
    /// Unique identifier for this rule
    pub id: String,
    /// Pattern to match source node display name (supports wildcards)
    pub source_pattern: String,
    /// Expected source node type (StreamOutput, Lv2Plugin, etc.)
    pub source_node_type: Option<NodeType>,
    /// Pattern to match target node display name (supports wildcards)
    pub target_pattern: String,
    /// Expected target node type (Sink, Lv2Plugin, Duplex, etc.)
    pub target_node_type: Option<NodeType>,
    /// Specific PipeWire node ID for the target (most precise match).
    /// Falls back to target_pattern + target_node_type when this node
    /// is not present in the graph.
    pub target_node_id: Option<ObjectId>,
    /// Explicit port-to-port mappings.
    ///
    /// When non-empty, only these specific port pairs are connected.
    /// When empty, falls back to heuristic matching (channel name,
    /// port name, then positional).
    #[serde(default)]
    pub port_mappings: Vec<PortMapping>,
    /// Whether this rule is enabled
    pub enabled: bool,
}

impl AutoConnectRule {
    /// Create a new auto-connect rule from concrete node information.
    pub fn new(
        source_pattern: impl Into<String>,
        source_node_type: Option<NodeType>,
        target_pattern: impl Into<String>,
        target_node_type: Option<NodeType>,
        target_node_id: Option<ObjectId>,
    ) -> Self {
        Self {
            id: uuid_simple(),
            source_pattern: source_pattern.into(),
            source_node_type,
            target_pattern: target_pattern.into(),
            target_node_type,
            target_node_id,
            port_mappings: Vec::new(),
            enabled: true,
        }
    }

    /// Add a port mapping to this rule, deduplicating.
    /// Returns true if a new mapping was added.
    pub fn add_port_mapping(&mut self, output_port_name: String, input_port_name: String) -> bool {
        let mapping = PortMapping {
            output_port_name,
            input_port_name,
        };
        if !self.port_mappings.contains(&mapping) {
            self.port_mappings.push(mapping);
            true
        } else {
            false
        }
    }

    /// Check if a node display name (and optionally type) matches the source.
    pub fn matches_source(&self, display_name: &str, node_type: Option<NodeType>) -> bool {
        if let Some(expected) = self.source_node_type
            && node_type != Some(expected)
        {
            return false;
        }
        pattern_matches(&self.source_pattern, display_name)
    }

    /// Check if a node matches the target of this rule.
    ///
    /// Priority: node ID match > display name + node type match.
    pub fn matches_target(
        &self,
        display_name: &str,
        node_type: Option<NodeType>,
        node_id: ObjectId,
    ) -> bool {
        // Exact node ID match is highest priority
        if let Some(expected_id) = self.target_node_id
            && node_id == expected_id
        {
            return true;
        }

        // Fall back to display name + node type matching
        if let Some(expected) = self.target_node_type
            && node_type != Some(expected)
        {
            return false;
        }
        pattern_matches(&self.target_pattern, display_name)
    }

    /// Human-readable description of the target for display in the UI.
    pub fn target_label(&self) -> String {
        let type_str = self
            .target_node_type
            .map(|t| format!(" [{}]", node_type_label(t)))
            .unwrap_or_default();
        let ports_str = if self.port_mappings.is_empty() {
            String::new()
        } else {
            format!(" ({} ports)", self.port_mappings.len())
        };
        format!("{}{}{}", self.target_pattern, type_str, ports_str)
    }

    /// Human-readable description of the source for display in the UI.
    pub fn source_label(&self) -> String {
        let type_str = self
            .source_node_type
            .map(|t| format!(" [{}]", node_type_label(t)))
            .unwrap_or_default();
        format!("{}{}", self.source_pattern, type_str)
    }
}

/// Short human-readable label for a NodeType.
pub fn node_type_label(nt: NodeType) -> &'static str {
    match nt {
        NodeType::Sink => "Sink",
        NodeType::Source => "Source",
        NodeType::StreamOutput => "App Out",
        NodeType::StreamInput => "App In",
        NodeType::Duplex => "Duplex",
        NodeType::Lv2Plugin => "Plugin",
    }
}

/// Simple pattern matching with wildcards.
/// Supports:
/// - `*` matches any sequence of characters
/// - `?` matches any single character
/// - Plain strings: exact match or substring match
pub fn pattern_matches(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if !pattern.contains('*') && !pattern.contains('?') {
        // Exact match or substring match
        return text == pattern || text.contains(pattern);
    }

    // Simple glob matching using dynamic programming approach
    let pattern_bytes = pattern.as_bytes();
    let text_bytes = text.as_bytes();
    let m = pattern_bytes.len();
    let n = text_bytes.len();

    let mut dp = vec![vec![false; n + 1]; m + 1];
    dp[0][0] = true;

    for i in 1..=m {
        if pattern_bytes[i - 1] == b'*' {
            dp[i][0] = dp[i - 1][0];
        }
    }

    for i in 1..=m {
        for j in 1..=n {
            if pattern_bytes[i - 1] == b'*' {
                dp[i][j] = dp[i - 1][j] || dp[i][j - 1];
            } else if pattern_bytes[i - 1] == b'?' || pattern_bytes[i - 1] == text_bytes[j - 1] {
                dp[i][j] = dp[i - 1][j - 1];
            }
        }
    }

    dp[m][n]
}

/// Generate a simple unique ID
pub fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{:x}{:x}", duration.as_secs(), duration.subsec_nanos())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_matching() {
        assert!(pattern_matches("Firefox", "Firefox"));
        assert!(pattern_matches("Firefox", "Firefox on YouTube"));
        assert!(pattern_matches("*", "anything"));
        assert!(pattern_matches("Fire*", "Firefox"));
        assert!(pattern_matches("*fox", "Firefox"));
        assert!(!pattern_matches("Chrome", "Firefox"));
    }

    #[test]
    fn test_rule_matching_source() {
        let rule = AutoConnectRule::new(
            "Firefox*",
            Some(NodeType::StreamOutput),
            "Headphones",
            Some(NodeType::Sink),
            Some(42),
        );
        // Source matches by display name + type
        assert!(rule.matches_source("Firefox", Some(NodeType::StreamOutput)));
        assert!(rule.matches_source("Firefox on YouTube", Some(NodeType::StreamOutput)));
        assert!(!rule.matches_source("Firefox", Some(NodeType::StreamInput)));
        assert!(!rule.matches_source("Chrome", Some(NodeType::StreamOutput)));
    }

    #[test]
    fn test_rule_matching_target() {
        let rule = AutoConnectRule::new(
            "Firefox*",
            Some(NodeType::StreamOutput),
            "Headphones",
            Some(NodeType::Sink),
            Some(42),
        );
        // Exact node ID match
        assert!(rule.matches_target("Headphones", Some(NodeType::Sink), 42));
        // Node ID match even with wrong name (ID takes priority)
        assert!(rule.matches_target("Speakers", Some(NodeType::Source), 42));
        // Fallback to name + type
        assert!(rule.matches_target("Headphones", Some(NodeType::Sink), 99));
        // Wrong type, wrong ID
        assert!(!rule.matches_target("Headphones", Some(NodeType::Source), 99));
    }
}
