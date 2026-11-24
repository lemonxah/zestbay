use super::*;

impl qobject::AppController {
    pub(super) fn mark_links_dirty(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().links_dirty = true;
        if self.rust().links_dirty_since.is_none() {
            self.as_mut().rust_mut().links_dirty_since = Some(Instant::now());
        }
    }

    pub(super) fn rename_plugin_links(mut self: Pin<&mut Self>, old: &str, new: &str) {
        let rename = |l: &mut SavedPluginLink| {
            if l.output_node_name == old {
                l.output_node_name = new.to_string();
            }
            if l.input_node_name == old {
                l.input_node_name = new.to_string();
            }
        };
        {
            let mut rust = self.as_mut().rust_mut();
            rust.pending_links
                .iter_mut()
                .for_each(|p| rename(&mut p.link));
            rust.last_graph_links.iter_mut().for_each(rename);
            rust.forgotten_link_nodes.remove(new);
        }
        self.as_mut().mark_links_dirty();
    }

    pub(super) fn forget_plugin_links(mut self: Pin<&mut Self>, name: &str) {
        let involves =
            |l: &SavedPluginLink| l.output_node_name == name || l.input_node_name == name;
        {
            let mut rust = self.as_mut().rust_mut();
            rust.forgotten_link_nodes.insert(name.to_string());
            rust.pending_links.retain(|p| !involves(&p.link));
            rust.last_graph_links.retain(|l| !involves(l));
        }
        self.as_mut().mark_links_dirty();
    }

    pub(super) fn persist_links_now(mut self: Pin<&mut Self>) {
        let Some(graph) = self.rust().graph.clone() else {
            return;
        };
        let current = build_persistable_links(&graph);
        let all_nodes = graph.get_all_nodes();
        let present = |name: &str| node_name_present(&graph, &all_nodes, name);
        let forgotten = self.rust().forgotten_link_nodes.clone();
        let last = std::mem::take(&mut self.as_mut().rust_mut().last_graph_links);
        let mut pending = std::mem::take(&mut self.as_mut().rust_mut().pending_links);
        let merged = merge_links_for_persist(&current, &last, &mut pending, &forgotten, present);
        self.as_mut().rust_mut().pending_links = pending;
        self.as_mut().rust_mut().last_graph_links = current;
        write_saved_links(&merged);
    }

    pub(super) fn retry_pending_links(mut self: Pin<&mut Self>) {
        let Some(graph) = self.rust().graph.clone() else {
            return;
        };
        let current = build_persistable_links(&graph);
        let all_nodes = graph.get_all_nodes();
        let mut pending = std::mem::take(&mut self.as_mut().rust_mut().pending_links);
        let before = pending.len();
        let mut connects: Vec<(u32, u32)> = Vec::new();
        pending.retain_mut(|p| {
            if current.contains(&p.link) {
                return false;
            }
            if let Some(sent) = p.connect_sent_at {
                if sent.elapsed() < LINK_CONNECT_GRACE {
                    return true;
                }
                if node_name_present(&graph, &all_nodes, &p.link.output_node_name)
                    && node_name_present(&graph, &all_nodes, &p.link.input_node_name)
                {
                    log::warn!(
                        "Saved link {}:{} -> {}:{} did not come up; forgetting it",
                        p.link.output_node_name,
                        p.link.output_port_name,
                        p.link.input_node_name,
                        p.link.input_port_name
                    );
                    return false;
                }
                p.connect_sent_at = None;
            }
            if let (Some(out_id), Some(in_id)) = find_saved_link_ports(&graph, &all_nodes, &p.link)
            {
                log::info!(
                    "Restoring link: {}:{} -> {}:{}",
                    p.link.output_node_name,
                    p.link.output_port_name,
                    p.link.input_node_name,
                    p.link.input_port_name
                );
                connects.push((out_id, in_id));
                p.connect_sent_at = Some(Instant::now());
            }
            true
        });
        let dropped = before != pending.len();
        self.as_mut().rust_mut().pending_links = pending;
        if let Some(ref tx) = self.rust().cmd_tx {
            for (output_port_id, input_port_id) in connects {
                let _ = tx.send(PwCommand::Connect {
                    output_port_id,
                    input_port_id,
                });
            }
        }
        if dropped {
            self.as_mut().mark_links_dirty();
        }
    }
}

impl qobject::AppController {
    pub(super) fn suppress_link_persist(mut self: Pin<&mut Self>, for_how_long: Duration) {
        let until = Instant::now() + for_how_long;
        let current = self.rust().links_suppressed_until;
        if current.is_none_or(|t| t < until) {
            self.as_mut().rust_mut().links_suppressed_until = Some(until);
        }
    }

    pub(super) fn links_persist_suppressed(&self) -> bool {
        !self
            .rust()
            .plugin_manager
            .as_ref()
            .is_some_and(|mgr| mgr.startup_complete())
            || self.rust().pw_reconnecting
            || self
                .rust()
                .links_suppressed_until
                .is_some_and(|t| Instant::now() < t)
    }
}

pub(super) fn load_saved_links() -> Vec<SavedPluginLink> {
    load_config::<Vec<SavedPluginLink>>("links.json")
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub(super) fn write_saved_links(links: &[SavedPluginLink]) {
    let json = serde_json::to_string_pretty(links).unwrap_or_default();
    if write_config("links.json", json.as_bytes(), links.is_empty()) {
        log::debug!("persist links: {} links written", links.len());
    }
}

pub(super) fn bridge_device_name_for_port(port: &Port) -> Option<String> {
    port.port_alias
        .as_ref()
        .and_then(|alias| alias.find(':').map(|pos| alias[..pos].to_string()))
}

pub(super) fn build_persistable_links(graph: &GraphState) -> Vec<SavedPluginLink> {
    let links = graph.get_all_links();
    let mut saved_links = Vec::new();

    for link in &links {
        let out_node = graph.get_node(link.output_node_id);
        let in_node = graph.get_node(link.input_node_id);
        let out_port = graph.get_port(link.output_port_id);
        let in_port = graph.get_port(link.input_port_id);

        let owned = |n: &Option<Node>| {
            n.as_ref().is_some_and(|n| {
                n.node_type == Some(NodeType::Plugin) || n.subgraph.is_some() || n.vdev_id.is_some()
            })
        };
        let involves_lv2 = owned(&out_node) || owned(&in_node);

        let involves_midi = out_port
            .as_ref()
            .map(|p| p.media_type == Some(crate::pipewire::MediaType::Midi))
            .unwrap_or(false)
            || in_port
                .as_ref()
                .map(|p| p.media_type == Some(crate::pipewire::MediaType::Midi))
                .unwrap_or(false);

        if !involves_lv2 && !involves_midi {
            continue;
        }

        if let (Some(out_node), Some(in_node), Some(out_port), Some(in_port)) =
            (out_node, in_node, out_port, in_port)
        {
            let out_name = if out_node.is_bridge {
                bridge_device_name_for_port(&out_port)
                    .unwrap_or_else(|| out_node.display_name().to_string())
            } else {
                out_node.display_name().to_string()
            };
            let in_name = if in_node.is_bridge {
                bridge_device_name_for_port(&in_port)
                    .unwrap_or_else(|| in_node.display_name().to_string())
            } else {
                in_node.display_name().to_string()
            };
            let saved = SavedPluginLink {
                output_node_name: out_name,
                output_port_name: out_port.name.clone(),
                input_node_name: in_name,
                input_port_name: in_port.name.clone(),
            };
            if !saved_links.contains(&saved) {
                saved_links.push(saved);
            }
        }
    }

    saved_links.sort_by(|a, b| {
        (
            &a.output_node_name,
            &a.output_port_name,
            &a.input_node_name,
            &a.input_port_name,
        )
            .cmp(&(
                &b.output_node_name,
                &b.output_port_name,
                &b.input_node_name,
                &b.input_port_name,
            ))
    });
    saved_links
}

pub(super) fn node_name_present(graph: &GraphState, all_nodes: &[Node], name: &str) -> bool {
    all_nodes.iter().any(|n| {
        n.display_name() == name
            || (n.is_bridge
                && graph
                    .get_bridge_port_groups(n.id)
                    .values()
                    .any(|device| device == name))
    })
}

pub(super) fn find_saved_link_port(
    graph: &GraphState,
    all_nodes: &[Node],
    node_name: &str,
    port_name: &str,
    direction: PortDirection,
) -> Option<u32> {
    for n in all_nodes.iter().filter(|n| n.display_name() == node_name) {
        let ports = graph.get_ports_for_node(n.id);
        if let Some(p) = ports
            .iter()
            .find(|p| p.name == port_name && p.direction == direction)
        {
            return Some(p.id);
        }
    }
    for n in all_nodes.iter().filter(|n| n.is_bridge) {
        for (group, device_name) in &graph.get_bridge_port_groups(n.id) {
            if device_name != node_name {
                continue;
            }
            let ports = graph.get_ports_for_bridge_group(n.id, group);
            if let Some(p) = ports
                .iter()
                .find(|p| p.name == port_name && p.direction == direction)
            {
                return Some(p.id);
            }
        }
    }
    None
}

pub(super) fn find_saved_link_ports(
    graph: &GraphState,
    all_nodes: &[Node],
    link: &SavedPluginLink,
) -> (Option<u32>, Option<u32>) {
    (
        find_saved_link_port(
            graph,
            all_nodes,
            &link.output_node_name,
            &link.output_port_name,
            PortDirection::Output,
        ),
        find_saved_link_port(
            graph,
            all_nodes,
            &link.input_node_name,
            &link.input_port_name,
            PortDirection::Input,
        ),
    )
}

pub(super) fn merge_links_for_persist(
    current: &[SavedPluginLink],
    last_current: &[SavedPluginLink],
    pending: &mut Vec<PendingLink>,
    forgotten: &std::collections::HashSet<String>,
    node_present: impl Fn(&str) -> bool,
) -> Vec<SavedPluginLink> {
    let is_forgotten = |l: &SavedPluginLink| {
        forgotten.contains(&l.output_node_name) || forgotten.contains(&l.input_node_name)
    };
    pending.retain(|p| !current.contains(&p.link) && !is_forgotten(&p.link));
    for l in last_current {
        if current.contains(l) || is_forgotten(l) || pending.iter().any(|p| p.link == *l) {
            continue;
        }
        if !node_present(&l.output_node_name) || !node_present(&l.input_node_name) {
            pending.push(PendingLink::new(l.clone()));
        }
    }
    let mut merged: Vec<SavedPluginLink> = current.to_vec();
    for p in pending.iter() {
        if !merged.contains(&p.link) {
            merged.push(p.link.clone());
        }
    }
    merged
}
