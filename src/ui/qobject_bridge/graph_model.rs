use super::*;

impl qobject::AppController {
    pub(super) fn refresh_cache(mut self: Pin<&mut Self>) {
        let (node_count, link_count, nodes) = {
            if let Some(ref graph) = self.rust().graph {
                let nodes = graph.get_all_nodes();
                let links = graph.get_all_links();
                (nodes.len() as i32, links.len() as i32, nodes)
            } else {
                (0, 0, Vec::new())
            }
        };

        self.as_mut().set_node_count(node_count);
        self.as_mut().set_link_count(link_count);
        self.as_mut().rust_mut().cached_nodes = nodes;
    }

    pub fn get_nodes_json(mut self: Pin<&mut Self>) -> QString {
        if let Some(graph) = self.rust().graph.clone() {
            let nodes = graph.get_all_nodes();
            log::debug!(
                "get_nodes_json: {} nodes ({} ready)",
                nodes.len(),
                nodes.iter().filter(|n| n.ready).count()
            );

            self.as_mut().rust_mut().bridge_split.clear_ports();

            let members = self.member_nodes();
            self.as_mut()
                .rust_mut()
                .subgraphs
                .build_view(&graph, &nodes, &|s| members.get(s).copied());

            let mut json_nodes: Vec<serde_json::Value> = Vec::new();

            for n in nodes.iter().filter(|n| n.ready) {
                if !self.rust().subgraphs.vm.visible_real.contains(&n.id) {
                    continue;
                }
                let media_str = match n.media_type {
                    Some(crate::pipewire::MediaType::Audio) => "Audio",
                    Some(crate::pipewire::MediaType::Video) => "Video",
                    Some(crate::pipewire::MediaType::Midi) => "Midi",
                    None => "Unknown",
                };

                if n.is_bridge {
                    let groups = graph.get_bridge_port_groups(n.id);
                    if groups.is_empty() {
                        let mgr = self.rust().plugin_manager.as_ref();
                        json_nodes.push(node_to_json(n, mgr));
                    } else {
                        for (group, device_name) in &groups {
                            let vid = self
                                .as_mut()
                                .rust_mut()
                                .bridge_split
                                .get_or_create_virtual_id(n.id, group);

                            let group_ports = graph.get_ports_for_bridge_group(n.id, group);
                            for port in &group_ports {
                                self.as_mut()
                                    .rust_mut()
                                    .bridge_split
                                    .register_port(port.id, vid);
                            }

                            let has_inputs = group_ports
                                .iter()
                                .any(|p| p.direction == PortDirection::Input);
                            let has_outputs = group_ports
                                .iter()
                                .any(|p| p.direction == PortDirection::Output);
                            let type_str = if has_inputs && has_outputs {
                                "Duplex"
                            } else if has_outputs {
                                "Source"
                            } else if has_inputs {
                                "Sink"
                            } else {
                                "Duplex"
                            };

                            json_nodes.push(serde_json::json!({
                                "id": vid,
                                "name": device_name,
                                "type": type_str,
                                "mediaType": media_str,
                                "isVirtual": n.is_virtual,
                                "isJack": n.is_jack,
                                "layoutKey": format!("MidiBridge:{}", device_name),
                                "ready": true,
                            }));
                        }
                    }
                } else {
                    let mgr = self.rust().plugin_manager.as_ref();
                    json_nodes.push(node_to_json(n, mgr));
                }
            }
            json_nodes.extend(self.view_nodes_json());

            let json = serde_json::to_string(&json_nodes).unwrap_or_default();
            QString::from(&json)
        } else {
            QString::from("[]")
        }
    }

    pub fn get_links_json(self: Pin<&mut Self>) -> QString {
        if let Some(ref graph) = self.rust().graph {
            let links = graph.get_all_links();
            let json_links: Vec<serde_json::Value> = links
                .iter()
                .filter_map(|l| {
                    let out_node = self.view_node_for(l.output_port_id, l.output_node_id)?;
                    let in_node = self.view_node_for(l.input_port_id, l.input_node_id)?;
                    Some(serde_json::json!({
                        "id": l.id,
                        "outputNodeId": out_node,
                        "outputPortId": l.output_port_id,
                        "inputNodeId": in_node,
                        "inputPortId": l.input_port_id,
                        "active": l.active,
                    }))
                })
                .collect();
            let json = serde_json::to_string(&json_links).unwrap_or_default();
            QString::from(&json)
        } else {
            QString::from("[]")
        }
    }

    pub fn get_ports_json(self: Pin<&mut Self>, node_id: u32) -> QString {
        log::debug!("get_ports_json: node_id={}", node_id);
        if let Some(ref graph) = self.rust().graph {
            let ports = if let Some(vn) = self.rust().subgraphs.vm.nodes.get(&node_id) {
                vn.ports.clone()
            } else if let Some((real_node_id, group)) = self
                .rust()
                .bridge_split
                .resolve_virtual_node(node_id)
                .cloned()
            {
                graph.get_ports_for_bridge_group(real_node_id, &group)
            } else {
                graph.get_ports_for_node(node_id)
            };

            let json_ports: Vec<serde_json::Value> = ports
                .iter()
                .map(|p| {
                    let media_str = match p.media_type {
                        Some(crate::pipewire::MediaType::Audio) => "Audio",
                        Some(crate::pipewire::MediaType::Video) => "Video",
                        Some(crate::pipewire::MediaType::Midi) => "Midi",
                        None => "Unknown",
                    };
                    let display_name = if self.rust().subgraphs.vm.nodes.contains_key(&node_id) {
                        p.display_name().to_string()
                    } else {
                        port_ui_name(p, self.rust().bridge_split.is_virtual_id(node_id))
                    };
                    let display_name = self
                        .rust()
                        .plugin_manager
                        .as_ref()
                        .and_then(|m| {
                            m.active_instances()
                                .values()
                                .find(|i| i.pw_node_id == Some(p.node_id))
                        })
                        .and_then(|i| {
                            crate::lv2::atom_state::port_label(&i.plugin_uri, &i.lv2_state, &p.name)
                        })
                        .unwrap_or(display_name);
                    let sort_name = display_name.clone();
                    let display_name = self
                        .rust()
                        .plugin_manager
                        .as_ref()
                        .and_then(|m| {
                            m.active_instances()
                                .values()
                                .find(|i| i.pw_node_id == Some(p.node_id))
                        })
                        .and_then(|i| i.port_names.get(&p.name).cloned())
                        .unwrap_or(display_name);
                    serde_json::json!({
                        "id": p.id,
                        "name": display_name,
                        "sortName": sort_name,
                        "direction": format!("{:?}", p.direction),
                        "nodeId": node_id,
                        "mediaType": media_str,
                    })
                })
                .collect();
            let json = serde_json::to_string(&json_ports).unwrap_or_default();
            QString::from(&json)
        } else {
            QString::from("[]")
        }
    }
}

pub(super) fn sort_ports_for_layout(ports: &mut [Port], bridge_sub_node: bool) {
    ports.sort_by(|a, b| {
        let am = a.media_type != Some(crate::pipewire::MediaType::Midi);
        let bm = b.media_type != Some(crate::pipewire::MediaType::Midi);
        am.cmp(&bm)
            .then_with(|| {
                crate::pipewire::state::natural_cmp(
                    &port_ui_name(a, bridge_sub_node),
                    &port_ui_name(b, bridge_sub_node),
                )
            })
            .then_with(|| a.id.cmp(&b.id))
    });
}

pub(super) fn port_ui_name(p: &Port, bridge_sub_node: bool) -> String {
    if bridge_sub_node
        && let Some(ref alias) = p.port_alias
        && let Some(colon_pos) = alias.find(':')
    {
        return alias[colon_pos + 1..].trim().to_string();
    }
    p.display_name().to_string()
}
pub(super) fn parse_node_type(s: &str) -> Option<NodeType> {
    match s {
        "Sink" => Some(NodeType::Sink),
        "Source" => Some(NodeType::Source),
        "App Out" | "StreamOutput" => Some(NodeType::StreamOutput),
        "App In" | "StreamInput" => Some(NodeType::StreamInput),
        "Duplex" => Some(NodeType::Duplex),
        "Plugin" | "Lv2Plugin" => Some(NodeType::Plugin),
        _ => None,
    }
}

pub(super) fn node_to_json(
    n: &Node,
    plugin_manager: Option<&crate::plugin::manager::PluginManager>,
) -> serde_json::Value {
    let type_str = match n.node_type {
        Some(NodeType::Sink) => "Sink",
        Some(NodeType::Source) => "Source",
        Some(NodeType::StreamOutput) => "StreamOutput",
        Some(NodeType::StreamInput) => "StreamInput",
        Some(NodeType::Duplex) => "Duplex",
        Some(NodeType::Plugin) => "Plugin",
        None => "Unknown",
    };
    let media_str = match n.media_type {
        Some(crate::pipewire::MediaType::Audio) => "Audio",
        Some(crate::pipewire::MediaType::Video) => "Video",
        Some(crate::pipewire::MediaType::Midi) => "Midi",
        None => "Unknown",
    };

    let mut val = serde_json::json!({
        "id": n.id,
        "name": n.display_name(),
        "type": type_str,
        "mediaType": media_str,
        "isVirtual": n.is_virtual,
        "isJack": n.is_jack,
        "layoutKey": layout_key(n, plugin_manager),
        "ready": n.ready,
    });

    if let Some(ref vdev) = n.vdev_id {
        val["vdevId"] = serde_json::json!(vdev);
    }

    if n.node_type == Some(NodeType::Plugin)
        && let Some(mgr) = plugin_manager
        && let Some(instance) = mgr
            .active_instances()
            .values()
            .find(|inst| inst.pw_node_id == Some(n.id))
    {
        let format_str = instance.format.as_str();
        let has_ui = mgr
            .find_plugin_with_format(&instance.plugin_uri, instance.format)
            .or(instance.scan_info.as_ref())
            .map(|p| p.has_ui)
            .unwrap_or(false);

        val["pluginFormat"] = serde_json::json!(format_str);
        val["pluginHasUi"] = serde_json::json!(has_ui);
        if let Some(channels) = crate::ladspa::builtin::meter_channels(&instance.plugin_uri) {
            val["meterChannels"] = serde_json::json!(channels);
        }
        val["pluginBypassed"] = serde_json::json!(instance.bypassed);
        val["pluginEnabled"] = serde_json::json!(instance.enabled);
    }

    val
}

pub(super) fn layout_key(
    node: &Node,
    plugin_manager: Option<&crate::plugin::manager::PluginManager>,
) -> String {
    let prefix = match node.node_type {
        Some(NodeType::Sink) => "Sink".to_string(),
        Some(NodeType::Source) => "Source".to_string(),
        Some(NodeType::StreamOutput) => "StreamOut".to_string(),
        Some(NodeType::StreamInput) => "StreamIn".to_string(),
        Some(NodeType::Duplex) => "Duplex".to_string(),
        Some(NodeType::Plugin) => {
            if let Some(mgr) = plugin_manager {
                mgr.active_instances()
                    .values()
                    .find(|inst| inst.pw_node_id == Some(node.id))
                    .map(|inst| inst.format.as_str().to_string())
                    .unwrap_or_else(|| "Plugin".to_string())
            } else {
                "Plugin".to_string()
            }
        }
        None => "Unknown".to_string(),
    };
    format!("{}:{}", prefix, node.display_name())
}

pub(super) fn node_type_str(n: &Node) -> &'static str {
    match n.node_type {
        Some(NodeType::Sink) => "Sink",
        Some(NodeType::Source) => "Source",
        Some(NodeType::StreamOutput) => "StreamOutput",
        Some(NodeType::StreamInput) => "StreamInput",
        Some(NodeType::Duplex) => "Duplex",
        Some(NodeType::Plugin) => "Plugin",
        None => "Unknown",
    }
}

pub(super) fn add_ports_to_layout(
    ports: &[crate::pipewire::Port],
    node_id: u32,
    layout_ports: &mut Vec<(u32, u32, usize, bool)>,
) {
    let mut input_idx = 0usize;
    let mut output_idx = 0usize;
    for p in ports {
        let is_output = p.direction == crate::pipewire::PortDirection::Output;
        let port_index = if is_output {
            let idx = output_idx;
            output_idx += 1;
            idx
        } else {
            let idx = input_idx;
            input_idx += 1;
            idx
        };
        layout_ports.push((p.id, node_id, port_index, is_output));
    }
}
