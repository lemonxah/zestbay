use super::*;

impl qobject::AppController {
    pub fn get_layout_json(self: Pin<&mut Self>) -> QString {
        QString::from(&read_config_json_text("layout.json", "{}"))
    }

    pub fn save_layout(self: Pin<&mut Self>, json: QString) {
        write_config_json_text("layout.json", &json.to_string());
    }

    pub fn auto_layout(
        mut self: Pin<&mut Self>,
        node_sizes_json: QString,
        pinned_positions_json: QString,
        grid_size: f64,
    ) -> QString {
        use crate::layout;

        let sizes_str: String = node_sizes_json.to_string();
        let node_sizes: HashMap<String, Vec<f64>> =
            serde_json::from_str(&sizes_str).unwrap_or_default();
        let size_of = |id: u32| -> Option<(f64, f64)> {
            node_sizes
                .get(&id.to_string())
                .filter(|s| s.len() >= 2 && s[0].is_finite() && s[1].is_finite())
                .map(|s| (s[0], s[1]))
        };

        let pinned_str: String = pinned_positions_json.to_string();
        let pinned_raw: HashMap<String, Vec<f64>> =
            serde_json::from_str(&pinned_str).unwrap_or_default();

        let graph = match self.rust().graph.clone() {
            Some(g) => g,
            None => return QString::from("{}"),
        };

        let mut all_nodes = graph.get_all_nodes();
        all_nodes.sort_by_key(|n| n.id);
        let mut all_links = graph.get_all_links();
        all_links.sort_by_key(|l| l.id);

        let mut layout_nodes: Vec<(u32, String, &str, f64, f64)> = Vec::new();
        let mut layout_ports: Vec<(u32, u32, usize, bool)> = Vec::new();
        let mut layout_links: Vec<(u32, u32, u32, u32, u32)> = Vec::new();
        let mut included: std::collections::HashSet<u32> = std::collections::HashSet::new();

        let mut bridge_vids: Vec<(u32, u32, String, String)> = Vec::new();
        for n in all_nodes.iter().filter(|n| n.ready && n.is_bridge) {
            let groups = graph.get_bridge_port_groups(n.id);
            for (group, device_name) in &groups {
                let vid = self
                    .as_mut()
                    .rust_mut()
                    .bridge_split
                    .get_or_create_virtual_id(n.id, group);
                bridge_vids.push((n.id, vid, group.clone(), device_name.clone()));
            }
        }

        for n in all_nodes.iter().filter(|n| n.ready) {
            let has_groups = n.is_bridge && bridge_vids.iter().any(|(rid, ..)| *rid == n.id);
            if has_groups {
                for (real_id, vid, group, device_name) in &bridge_vids {
                    if *real_id != n.id {
                        continue;
                    }
                    let Some((w, h)) = size_of(*vid) else {
                        continue;
                    };

                    let mut group_ports = graph.get_ports_for_bridge_group(n.id, group);
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

                    layout_nodes.push((*vid, device_name.clone(), type_str, w, h));
                    included.insert(*vid);
                    sort_ports_for_layout(&mut group_ports, true);
                    add_ports_to_layout(&group_ports, *vid, &mut layout_ports);
                }
            } else {
                let Some((w, h)) = size_of(n.id) else {
                    continue;
                };
                let type_str = node_type_str(n);
                layout_nodes.push((n.id, n.display_name().to_string(), type_str, w, h));
                included.insert(n.id);

                let mut ports = graph.get_ports_for_node(n.id);
                sort_ports_for_layout(&mut ports, false);
                add_ports_to_layout(&ports, n.id, &mut layout_ports);
            }
        }

        let mut view_nodes: Vec<(u32, sgui::ViewNode)> = self
            .rust()
            .subgraphs
            .vm
            .nodes
            .iter()
            .map(|(id, vn)| (*id, vn.clone()))
            .collect();
        view_nodes.sort_by_key(|(id, _)| *id);
        for (vid, vn) in &view_nodes {
            let Some((w, h)) = size_of(*vid) else {
                continue;
            };
            let type_str = match vn.kind {
                ViewNodeKind::Card => "Duplex",
                ViewNodeKind::Input => "Source",
                ViewNodeKind::Output => "Sink",
            };
            layout_nodes.push((*vid, vn.subgraph.clone(), type_str, w, h));
            included.insert(*vid);
            let mut ports = vn.ports.clone();
            sort_ports_for_layout(&mut ports, false);
            add_ports_to_layout(&ports, *vid, &mut layout_ports);
        }

        for l in &all_links {
            let out_node = self
                .rust()
                .subgraphs
                .vm
                .port_owner
                .get(&l.output_port_id)
                .copied()
                .or_else(|| {
                    self.rust()
                        .bridge_split
                        .resolve_port_virtual_node(l.output_port_id)
                })
                .unwrap_or(l.output_node_id);
            let in_node = self
                .rust()
                .subgraphs
                .vm
                .port_owner
                .get(&l.input_port_id)
                .copied()
                .or_else(|| {
                    self.rust()
                        .bridge_split
                        .resolve_port_virtual_node(l.input_port_id)
                })
                .unwrap_or(l.input_node_id);

            if included.contains(&out_node) && included.contains(&in_node) {
                layout_links.push((l.id, out_node, l.output_port_id, in_node, l.input_port_id));
            }
        }

        let spacing = f64::from(self.rust().prefs.node_spacing_px);
        let config = layout::graph::LayoutConfig {
            margin_x: spacing * 2.5,
            margin_y: spacing,
            max_column_nodes: self.rust().prefs.column_wrap as usize,
            ..Default::default()
        };
        let mut pinned_by_id: HashMap<u32, (f64, f64)> = HashMap::new();
        for (key, pos) in &pinned_raw {
            if pos.len() >= 2
                && let Ok(id) = key.parse::<u32>()
                && included.contains(&id)
            {
                pinned_by_id.insert(id, (pos[0], pos[1]));
            }
        }

        for &(id, ref name, type_str, w, h) in &layout_nodes {
            let pin_tag = if pinned_by_id.contains_key(&id) {
                " [PINNED]"
            } else {
                ""
            };
            log::debug!(
                "  node {}: {}({}) {}x{}{}",
                id,
                name,
                type_str,
                w as i32,
                h as i32,
                pin_tag
            );
        }
        log::info!(
            "auto_layout: {} nodes ({} pinned), {} links",
            layout_nodes.len(),
            pinned_by_id.len(),
            layout_links.len()
        );

        let sizes_by_id: HashMap<u32, (f64, f64)> = layout_nodes
            .iter()
            .map(|&(id, _, _, w, h)| (id, (w, h)))
            .collect();
        let gap = config.margin_y;
        let mut positions = layout::sugiyama_layout(
            layout_nodes,
            layout_ports,
            layout_links,
            config,
            &pinned_by_id,
        );
        layout::snap_to_grid(&mut positions, &sizes_by_id, &pinned_by_id, grid_size, gap);

        let mut result: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        for (node_id, (x, y)) in &positions {
            if included.contains(node_id) && x.is_finite() && y.is_finite() {
                result.insert(node_id.to_string(), serde_json::json!([x, y]));
            }
        }

        let json = serde_json::to_string(&serde_json::Value::Object(result))
            .unwrap_or_else(|_| "{}".to_string());
        log::info!("auto_layout: computed {} positions", positions.len());
        QString::from(&json)
    }

    pub fn get_hidden_json(self: Pin<&mut Self>) -> QString {
        QString::from(&read_config_json_text("hidden.json", "[]"))
    }

    pub fn save_hidden(self: Pin<&mut Self>, json: QString) {
        write_config_json_text("hidden.json", &json.to_string());
    }

    pub fn get_docked_meters_json(self: Pin<&mut Self>) -> QString {
        QString::from(&read_config_json_text("docked-meters.json", "{}"))
    }

    pub fn save_docked_meters(mut self: Pin<&mut Self>, json: QString) {
        let text = json.to_string();
        write_config_json_text("docked-meters.json", &text);
        self.as_mut().rust_mut().docked_meters = parse_docked_meters(&text);
        self.as_mut().rust_mut().rules_apply_pending = true;
    }

    pub fn get_pinned_json(self: Pin<&mut Self>) -> QString {
        QString::from(&read_config_json_text("pinned.json", "[]"))
    }

    pub fn save_pinned(self: Pin<&mut Self>, json: QString) {
        write_config_json_text("pinned.json", &json.to_string());
    }
}

impl qobject::AppController {
    pub fn get_node_styles_json(self: Pin<&mut Self>) -> QString {
        QString::from(&read_config_json_text("node-style.json", "{}"))
    }

    pub fn save_node_styles(self: Pin<&mut Self>, json: QString) {
        write_config_json_text("node-style.json", &json.to_string());
    }
}

impl qobject::AppController {
    pub fn get_window_geometry_json(self: Pin<&mut Self>) -> QString {
        QString::from(&read_config_json_text("window.json", "{}"))
    }

    pub fn save_window_geometry(self: Pin<&mut Self>, json: QString) {
        write_config_json_text("window.json", &json.to_string());
    }

    pub fn get_viewport_json(self: Pin<&mut Self>) -> QString {
        QString::from(&read_config_json_text("viewport.json", "{}"))
    }

    pub fn save_viewport(self: Pin<&mut Self>, json: QString) {
        write_config_json_text("viewport.json", &json.to_string());
    }
}

impl qobject::AppController {
    pub fn get_default_node(self: Pin<&mut Self>) -> QString {
        let path = config_path("default_node.txt");
        match std::fs::read_to_string(&path) {
            Ok(s) => QString::from(&s.trim().to_string()),
            Err(_) => QString::from(""),
        }
    }

    pub fn set_default_node(mut self: Pin<&mut Self>, layout_key: QString) {
        let key: String = layout_key.to_string();
        let path = config_path("default_node.txt");
        if key.is_empty() {
            let _ = std::fs::remove_file(&path);
            log::info!("Cleared default node");
        } else if write_atomic(&path, key.as_bytes()).is_ok() {
            log::info!("Set default node: {}", key);
        }

        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            if key.is_empty() {
                patchbay.set_default_target(None);
            } else {
                let display_name = if let Some(pos) = key.find(':') {
                    key[pos + 1..].to_string()
                } else {
                    key.clone()
                };
                patchbay.set_default_target(Some(display_name));
            }
        }
    }
}
