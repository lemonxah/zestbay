use super::*;

impl qobject::AppController {
    pub fn get_rules_json(self: Pin<&mut Self>) -> QString {
        if let Some(ref patchbay) = self.rust().patchbay {
            let json_rules: Vec<serde_json::Value> = patchbay
                .rules()
                .iter()
                .map(|r| {
                    let mappings: Vec<serde_json::Value> = r
                        .port_mappings
                        .iter()
                        .map(|m| {
                            serde_json::json!({
                                "outputPort": m.output_port_name,
                                "inputPort": m.input_port_name,
                            })
                        })
                        .collect();
                    serde_json::json!({
                        "id": r.id,
                        "sourcePattern": r.source_pattern,
                        "sourceType": r.source_node_type.map(rules::node_type_label).unwrap_or("Any"),
                        "targetPattern": r.target_pattern,
                        "targetType": r.target_node_type.map(rules::node_type_label).unwrap_or("Any"),
                        "sourceLabel": r.source_label(),
                        "targetLabel": r.target_label(),
                        "enabled": r.enabled,
                        "action": if r.is_block() { "block" } else { "connect" },
                        "portMappings": mappings,
                    })
                })
                .collect();
            let json = serde_json::to_string(&json_rules).unwrap_or_default();
            QString::from(&json)
        } else {
            QString::from("[]")
        }
    }

    pub fn toggle_rule(mut self: Pin<&mut Self>, rule_id: QString) {
        let id: String = rule_id.to_string();
        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            patchbay.toggle_rule(&id);
        }
        save_rules(self.rust().patchbay.as_ref());
        self.as_mut().rules_changed();
    }

    pub fn remove_rule(mut self: Pin<&mut Self>, rule_id: QString) {
        let id: String = rule_id.to_string();
        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            patchbay.remove_rule(&id);
        }
        save_rules(self.rust().patchbay.as_ref());
        self.as_mut().rules_changed();
    }

    pub fn apply_rules(mut self: Pin<&mut Self>) {
        self.as_mut().refresh_meter_redirects();
        let commands = if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            patchbay.scan()
        } else {
            Vec::new()
        };
        if let Some(ref tx) = self.rust().cmd_tx {
            for cmd in commands {
                let _ = tx.send(cmd);
            }
        }
    }

    pub fn snapshot_rules(mut self: Pin<&mut Self>) {
        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            patchbay.snapshot_current_connections();
        }
        save_rules(self.rust().patchbay.as_ref());
        self.as_mut().rules_changed();
        log::info!("Snapshot: replaced rules with current connections");
    }

    pub fn toggle_patchbay(mut self: Pin<&mut Self>, enabled: bool) {
        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            patchbay.enabled = enabled;
        }
        self.as_mut().set_patchbay_enabled(enabled);
        self.as_mut().sync_tray_flags();
        self.as_mut().rules_changed();
    }

    pub fn toggle_patchbay_exclusive(mut self: Pin<&mut Self>, exclusive: bool) {
        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            patchbay.exclusive = exclusive;
        }
        self.as_mut().rust_mut().prefs.patchbay_exclusive = exclusive;
        save_preferences(&self.rust().prefs);
        self.as_mut().set_patchbay_exclusive(exclusive);
        self.as_mut().sync_tray_flags();
        log::info!("Patchbay exclusive mode: {}", exclusive);
        if exclusive && self.rust().patchbay_enabled {
            self.as_mut().apply_rules();
        }
    }

    pub fn toggle_patchbay_hide_unnamed(mut self: Pin<&mut Self>, enabled: bool) {
        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            patchbay.hide_unnamed = enabled;
            crate::pipewire::HIDE_UNNAMED_NODES
                .store(enabled, std::sync::atomic::Ordering::Relaxed);
        }
        self.as_mut().rust_mut().prefs.hide_unnamed_nodes = enabled;
        save_preferences(&self.rust().prefs);
        self.as_mut().set_patchbay_hide_unnamed(enabled);
        self.as_mut().sync_tray_flags();
        if self.rust().patchbay_enabled {
            self.as_mut().apply_rules();
        }
    }

    pub fn get_node_names_json(self: Pin<&mut Self>) -> QString {
        if let Some(ref graph) = self.rust().graph {
            let nodes = graph.get_all_nodes();
            let mut entries: Vec<serde_json::Value> = Vec::new();

            let internal = |n: &&Node| {
                n.subgraph
                    .as_ref()
                    .is_some_and(|t| t.side == crate::pipewire::SubgraphSide::Internal)
            };
            for n in nodes.iter().filter(|n| n.ready).filter(|n| !internal(n)) {
                let media_str = match n.media_type {
                    Some(crate::pipewire::MediaType::Audio) => "Audio",
                    Some(crate::pipewire::MediaType::Video) => "Video",
                    Some(crate::pipewire::MediaType::Midi) => "Midi",
                    None => "Unknown",
                };

                if n.is_bridge {
                    let groups = graph.get_bridge_port_groups(n.id);
                    for device_name in groups.values() {
                        entries.push(serde_json::json!({
                            "name": device_name,
                            "type": "Duplex",
                            "mediaType": media_str,
                        }));
                    }
                } else {
                    let type_str = match n.node_type {
                        Some(NodeType::Sink) => "Sink",
                        Some(NodeType::Source) => "Source",
                        Some(NodeType::StreamOutput) => "App Out",
                        Some(NodeType::StreamInput) => "App In",
                        Some(NodeType::Duplex) => "Duplex",
                        Some(NodeType::Plugin) => "Plugin",
                        None => "Unknown",
                    };
                    entries.push(serde_json::json!({
                        "name": n.display_name(),
                        "type": type_str,
                        "mediaType": media_str,
                    }));
                }
            }

            entries.sort_by(|a, b| {
                let a_name = a["name"].as_str().unwrap_or("");
                let b_name = b["name"].as_str().unwrap_or("");
                a_name.cmp(b_name)
            });
            entries.dedup_by(|a, b| {
                a["name"].as_str() == b["name"].as_str() && a["type"].as_str() == b["type"].as_str()
            });
            let json = serde_json::to_string(&entries).unwrap_or_default();
            QString::from(&json)
        } else {
            QString::from("[]")
        }
    }

    pub fn add_rule(
        mut self: Pin<&mut Self>,
        source_pattern: QString,
        source_type: QString,
        target_pattern: QString,
        target_type: QString,
        action: QString,
    ) {
        let src_pat: String = source_pattern.to_string();
        let src_type: String = source_type.to_string();
        let tgt_pat: String = target_pattern.to_string();
        let tgt_type: String = target_type.to_string();

        let src_node_type = parse_node_type(&src_type);
        let tgt_node_type = parse_node_type(&tgt_type);

        let rule = crate::patchbay::rules::AutoConnectRule::new(
            src_pat,
            src_node_type,
            tgt_pat,
            tgt_node_type,
            None,
        )
        .with_action(if action.to_string() == "block" {
            rules::RuleAction::Block
        } else {
            rules::RuleAction::Connect
        });

        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            patchbay.add_rule(rule);
        }
        save_rules(self.rust().patchbay.as_ref());
        self.as_mut().rules_changed();
    }
}

pub(super) fn load_rules() -> Vec<crate::patchbay::rules::AutoConnectRule> {
    load_config::<Vec<crate::patchbay::rules::AutoConnectRule>>("rules.json")
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub(super) fn save_rules(patchbay: Option<&PatchbayManager>) {
    let rules: Vec<crate::patchbay::rules::AutoConnectRule> = if let Some(mgr) = patchbay {
        mgr.rules().to_vec()
    } else {
        Vec::new()
    };
    let json = serde_json::to_string_pretty(&rules).unwrap_or_default();
    if write_config("rules.json", json.as_bytes(), rules.is_empty()) {
        log::debug!("save_rules: {} rules written", rules.len());
    }
}

pub(super) fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains('\0')
}

pub(super) fn parse_backup_stem(stem: &str) -> (Option<String>, String) {
    let b = stem.as_bytes();
    let digits =
        |r: std::ops::Range<usize>| b.get(r).is_some_and(|s| s.iter().all(u8::is_ascii_digit));
    if b.len() >= 15 && digits(0..8) && b[8] == b'_' && digits(9..15) {
        let date = format!(
            "{}-{}-{} {}:{}:{}",
            &stem[0..4],
            &stem[4..6],
            &stem[6..8],
            &stem[9..11],
            &stem[11..13],
            &stem[13..15]
        );
        let name = if b.get(15) == Some(&b'_') {
            stem.get(16..).unwrap_or("").to_string()
        } else {
            String::new()
        };
        (Some(date), name)
    } else {
        (None, String::new())
    }
}
