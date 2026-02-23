use serde::{Deserialize, Serialize};

use crate::pipewire::{NodeType, ObjectId};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PortMapping {
    pub output_port_name: String,
    pub input_port_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoConnectRule {
    pub id: String,
    pub source_pattern: String,
    pub source_node_type: Option<NodeType>,
    pub target_pattern: String,
    pub target_node_type: Option<NodeType>,
    pub target_node_id: Option<ObjectId>,
    #[serde(default)]
    pub port_mappings: Vec<PortMapping>,
    pub enabled: bool,
}

impl AutoConnectRule {
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

    pub fn matches_source(&self, display_name: &str, node_type: Option<NodeType>) -> bool {
        if let Some(expected) = self.source_node_type
            && node_type != Some(expected)
        {
            return false;
        }
        pattern_matches(&self.source_pattern, display_name)
    }

    pub fn matches_target(
        &self,
        display_name: &str,
        node_type: Option<NodeType>,
        node_id: ObjectId,
    ) -> bool {
        if let Some(expected_id) = self.target_node_id
            && node_id == expected_id
        {
            return true;
        }

        if let Some(expected) = self.target_node_type
            && node_type != Some(expected)
        {
            return false;
        }
        pattern_matches(&self.target_pattern, display_name)
    }

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

    pub fn source_label(&self) -> String {
        let type_str = self
            .source_node_type
            .map(|t| format!(" [{}]", node_type_label(t)))
            .unwrap_or_default();
        format!("{}{}", self.source_pattern, type_str)
    }
}

pub fn node_type_label(nt: NodeType) -> &'static str {
    match nt {
        NodeType::Sink => "Sink",
        NodeType::Source => "Source",
        NodeType::StreamOutput => "App Out",
        NodeType::StreamInput => "App In",
        NodeType::Duplex => "Duplex",
        NodeType::Plugin => "Plugin",
    }
}

pub fn pattern_matches(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if !pattern.contains('*') && !pattern.contains('?') {
        return text == pattern;
    }

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
        // Exact match (no globs)
        assert!(pattern_matches("Firefox", "Firefox"));
        assert!(!pattern_matches("Firefox", "Firefox on YouTube")); // exact: no substring
        assert!(!pattern_matches("Chromium", "Chromium Sink"));     // exact: no substring
        assert!(!pattern_matches("Chromium Sink", "Chromium"));     // exact: no substring

        // Wildcard
        assert!(pattern_matches("*", "anything"));

        // Glob patterns
        assert!(pattern_matches("Fire*", "Firefox"));
        assert!(pattern_matches("*fox", "Firefox"));
        assert!(pattern_matches("Firefox*", "Firefox on YouTube")); // glob for substring
        assert!(pattern_matches("*Chromium*", "Chromium Sink"));    // glob for contains
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
        assert!(rule.matches_target("Headphones", Some(NodeType::Sink), 42));
        assert!(rule.matches_target("Speakers", Some(NodeType::Source), 42));
        assert!(rule.matches_target("Headphones", Some(NodeType::Sink), 99));
        assert!(!rule.matches_target("Headphones", Some(NodeType::Source), 99));
    }
}
