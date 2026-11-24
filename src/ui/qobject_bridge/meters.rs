use super::*;

impl qobject::AppController {
    pub(super) fn forget_stale_meter_leftovers(mut self: Pin<&mut Self>) {
        if self.rust().plugins_frozen || self.rust().plugins_load_failed {
            return;
        }
        let present: std::collections::HashSet<String> = self
            .rust()
            .plugin_manager
            .iter()
            .flat_map(|mgr| mgr.active_instances().values())
            .map(|info| info.display_name.clone())
            .collect();
        let stale = |name: &str| is_default_meter_name(name) && !present.contains(name);
        let before = self.rust().pending_links.len();
        self.as_mut()
            .rust_mut()
            .pending_links
            .retain(|p| !stale(&p.link.output_node_name) && !stale(&p.link.input_node_name));
        if self.rust().pending_links.len() != before {
            log::info!(
                "Dropped {} saved link(s) left behind by meters that no longer exist",
                before - self.rust().pending_links.len()
            );
            self.as_mut().mark_links_dirty();
        }
        let changed = self
            .as_mut()
            .rust_mut()
            .patchbay
            .as_mut()
            .is_some_and(|pb| pb.forget_absent_plugin_nodes(is_default_meter_name, &present));
        if changed {
            log::info!("Removed rules left behind by meters that no longer exist");
            if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
                patchbay.rules_dirty = false;
            }
            save_rules(self.rust().patchbay.as_ref());
            self.as_mut().rules_changed();
        }
    }

    pub(super) fn forget_meter_rules(mut self: Pin<&mut Self>, instance_id: u64) {
        let name = self
            .rust()
            .plugin_manager
            .as_ref()
            .and_then(|mgr| mgr.get_instance(instance_id))
            .filter(|info| crate::ladspa::builtin::is_builtin_uri(&info.plugin_uri))
            .map(|info| info.display_name.clone());
        let Some(name) = name else {
            return;
        };
        let changed = self
            .as_mut()
            .rust_mut()
            .patchbay
            .as_mut()
            .is_some_and(|pb| pb.forget_plugin_node(&name));
        if changed {
            if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
                patchbay.rules_dirty = false;
            }
            save_rules(self.rust().patchbay.as_ref());
            self.as_mut().rules_changed();
        }
    }

    pub(super) fn refresh_meter_redirects(mut self: Pin<&mut Self>) {
        let (redirects, standby) = match (&self.rust().graph, &self.rust().plugin_manager) {
            (Some(graph), Some(mgr)) => {
                docked_meter_redirects(graph, mgr, &self.rust().docked_meters)
            }
            _ => Default::default(),
        };
        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            patchbay.set_port_redirects(redirects, standby);
        }
    }
}

impl qobject::AppController {
    pub fn add_meter(self: Pin<&mut Self>, channels: i32) -> QString {
        let Ok(channels) = usize::try_from(channels) else {
            return QString::from("");
        };
        if !crate::ladspa::builtin::CHANNEL_COUNTS.contains(&channels) {
            return QString::from("");
        }
        let uri = crate::ladspa::builtin::meter_uri(channels);
        self.add_plugin(QString::from(&uri))
    }

    pub fn rename_meter(mut self: Pin<&mut Self>, node_id: u32, base_name: QString) -> QString {
        let Some(instance_id) = self.find_instance_id_for_node(node_id) else {
            return QString::from("");
        };
        let Some((uri, current, others)) = self.rust().plugin_manager.as_ref().and_then(|mgr| {
            let info = mgr.get_instance(instance_id)?;
            let others: Vec<String> = mgr
                .active_instances()
                .values()
                .filter(|i| i.id != instance_id)
                .map(|i| i.display_name.clone())
                .collect();
            Some((info.plugin_uri.clone(), info.display_name.clone(), others))
        }) else {
            return QString::from("");
        };
        if !crate::ladspa::builtin::is_builtin_uri(&uri) {
            return QString::from("");
        }
        let base = base_name.to_string().trim().to_string();
        let base = if base.is_empty() {
            crate::ladspa::builtin::info(&uri)
                .map(|i| i.name.clone())
                .unwrap_or_else(|| "Meter".to_string())
        } else {
            base
        };
        let mut name = base.clone();
        let mut n = 2;
        while others.contains(&name) {
            name = format!("{base} #{n}");
            n += 1;
        }
        if name != current {
            self.as_mut().rename_plugin(node_id, QString::from(&name));
        }
        QString::from(&name)
    }

    pub fn meter_levels_json(self: Pin<&mut Self>) -> QString {
        let levels = crate::ladspa::builtin::levels();
        let mut out = serde_json::Map::new();
        if let Some(ref mgr) = self.rust().plugin_manager {
            for (instance_id, values) in levels {
                if let Some(node_id) = mgr.get_instance(instance_id).and_then(|i| i.pw_node_id) {
                    out.insert(node_id.to_string(), serde_json::json!(values));
                }
            }
        }
        QString::from(&serde_json::Value::Object(out).to_string())
    }
}

pub(super) fn parse_docked_meters(json: &str) -> std::collections::HashMap<String, String> {
    serde_json::from_str::<std::collections::HashMap<String, String>>(json).unwrap_or_default()
}

pub(super) fn load_docked_meters() -> std::collections::HashMap<String, String> {
    parse_docked_meters(&read_config_json_text("docked-meters.json", "{}"))
}

pub(super) fn is_default_meter_name(name: &str) -> bool {
    let base = match name.rsplit_once(" #") {
        Some((head, n)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => head,
        _ => name,
    };
    crate::ladspa::builtin::CHANNEL_COUNTS
        .iter()
        .any(|&channels| {
            crate::ladspa::builtin::info(&crate::ladspa::builtin::meter_uri(channels))
                .is_some_and(|info| info.name == base)
        })
}

pub(super) fn docked_meter_redirects(
    graph: &GraphState,
    plugin_manager: &PluginManager,
    docked: &std::collections::HashMap<String, String>,
) -> (
    std::collections::HashMap<u32, u32>,
    std::collections::HashSet<u32>,
) {
    let mut redirects = std::collections::HashMap::new();
    let mut standby = std::collections::HashSet::new();
    if docked.is_empty() {
        return (redirects, standby);
    }
    let mut links = graph.get_all_links();
    links.sort_by_key(|l| l.id);
    let mut meters: Vec<(u64, u32)> = plugin_manager
        .active_instances()
        .values()
        .filter(|info| crate::ladspa::builtin::meter_channels(&info.plugin_uri).is_some())
        .filter_map(|info| Some((info.id, info.pw_node_id?)))
        .collect();
    meters.sort_unstable();
    for (_, node_id) in meters {
        let Some(node) = graph.get_node(node_id) else {
            continue;
        };
        let to_source = match docked
            .get(&layout_key(&node, Some(plugin_manager)))
            .map(String::as_str)
        {
            Some("source") => true,
            Some("dest") => false,
            _ => continue,
        };
        let audio_ports = |dir: PortDirection| -> Vec<Port> {
            let mut ports: Vec<Port> = graph
                .get_ports_for_node(node_id)
                .into_iter()
                .filter(|p| {
                    p.direction == dir
                        && !matches!(
                            p.media_type,
                            Some(crate::pipewire::MediaType::Midi)
                                | Some(crate::pipewire::MediaType::Video)
                        )
                })
                .collect();
            ports.sort_by(|a, b| crate::pipewire::state::natural_cmp(&a.name, &b.name));
            ports
        };
        let inputs = audio_ports(PortDirection::Input);
        let outputs = audio_ports(PortDirection::Output);
        for (inp, out) in inputs.iter().zip(outputs.iter()) {
            if to_source {
                match links
                    .iter()
                    .find(|l| l.input_port_id == inp.id && l.output_node_id != node_id)
                {
                    Some(feed) => {
                        redirects.entry(feed.output_port_id).or_insert(out.id);
                    }
                    None => {
                        standby.insert(out.id);
                    }
                }
            } else {
                match links
                    .iter()
                    .find(|l| l.output_port_id == out.id && l.input_node_id != node_id)
                {
                    Some(fed) => {
                        redirects.entry(fed.input_port_id).or_insert(inp.id);
                    }
                    None => {
                        standby.insert(inp.id);
                    }
                }
            }
        }
    }
    (redirects, standby)
}
