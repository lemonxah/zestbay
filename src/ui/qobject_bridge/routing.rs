use super::*;

impl qobject::AppController {
    pub fn connect_ports(mut self: Pin<&mut Self>, output_port_id: u32, input_port_id: u32) {
        if let Some(ref graph) = self.rust().graph {
            let out_port = graph.get_port(output_port_id);
            let in_port = graph.get_port(input_port_id);
            if let (Some(op), Some(ip)) = (&out_port, &in_port)
                && op.node_id == ip.node_id
            {
                let same_group = match (&op.port_group, &ip.port_group) {
                    (Some(og), Some(ig)) => og == ig,
                    _ => true,
                };
                if same_group {
                    log::warn!(
                        "Rejected self-loop connect: ports {} and {} belong to the same node/device",
                        output_port_id,
                        input_port_id
                    );
                    return;
                }
            }
        }

        let reason = self.as_mut().blocked_reason(output_port_id, input_port_id);
        if !reason.is_empty() {
            log::info!("Connect refused, blocked by rule: {}", reason);
            return;
        }

        if let Some(ref tx) = self.rust().cmd_tx {
            log::info!("Connect request: {} -> {}", output_port_id, input_port_id);
            let _ = tx.send(PwCommand::Connect {
                output_port_id,
                input_port_id,
            });
        }

        let learned = if !self.rust().prefs.auto_learn_rules {
            false
        } else {
            let graph = self.rust().graph.clone();
            if let Some(ref graph) = graph {
                if let (Some(out_port), Some(in_port)) = (
                    graph.get_port(output_port_id),
                    graph.get_port(input_port_id),
                ) {
                    if let (Some(source_node), Some(target_node)) = (
                        graph.get_node(out_port.node_id),
                        graph.get_node(in_port.node_id),
                    ) {
                        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
                            let changed = patchbay.learn_from_link(
                                &source_node,
                                &target_node,
                                &out_port,
                                &in_port,
                            );
                            if changed {
                                log::info!(
                                    "Auto-learned rule: {}:{} -> {}:{}",
                                    source_node.display_name(),
                                    out_port.name,
                                    target_node.display_name(),
                                    in_port.name,
                                );
                            }
                            changed
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        };

        if learned {
            save_rules(self.rust().patchbay.as_ref());
            self.as_mut().rules_changed();
        }

        self.as_mut().rust_mut().links_dirty = true;
        if self.rust().links_dirty_since.is_none() {
            self.as_mut().rust_mut().links_dirty_since = Some(Instant::now());
        }
    }

    pub fn disconnect_link(mut self: Pin<&mut Self>, link_id: u32) {
        let link_info = self.rust().graph.as_ref().and_then(|g| g.get_link(link_id));

        if let Some(ref tx) = self.rust().cmd_tx {
            log::info!("Disconnect request: {}", link_id);
            let _ = tx.send(PwCommand::Disconnect { link_id });
        }

        if let Some(link) = link_info {
            let unlearned = {
                let graph = self.rust().graph.clone();
                if let Some(ref graph) = graph {
                    if let (Some(out_port), Some(in_port)) = (
                        graph.get_port(link.output_port_id),
                        graph.get_port(link.input_port_id),
                    ) {
                        if let (Some(source_node), Some(target_node)) = (
                            graph.get_node(link.output_node_id),
                            graph.get_node(link.input_node_id),
                        ) {
                            if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
                                let changed = patchbay.unlearn_from_link(
                                    &source_node,
                                    &target_node,
                                    &out_port,
                                    &in_port,
                                );
                                if changed {
                                    log::info!(
                                        "Unlearned rule: {}:{} -> {}:{}",
                                        source_node.display_name(),
                                        out_port.name,
                                        target_node.display_name(),
                                        in_port.name,
                                    );
                                }
                                changed
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            };

            if unlearned {
                save_rules(self.rust().patchbay.as_ref());
                self.as_mut().rules_changed();
            }
        }

        self.as_mut().rust_mut().links_dirty = true;
        if self.rust().links_dirty_since.is_none() {
            self.as_mut().rust_mut().links_dirty_since = Some(Instant::now());
        }
    }

    pub fn block_link(self: Pin<&mut Self>, link_id: u32) {
        let link = self.rust().graph.as_ref().and_then(|g| g.get_link(link_id));
        if let Some(link) = link {
            self.block_nodes(link.output_node_id, link.input_node_id);
        }
    }

    pub fn block_ports(self: Pin<&mut Self>, output_port_id: u32, input_port_id: u32) {
        let nodes = self.rust().graph.as_ref().and_then(|g| {
            Some((
                g.get_port(output_port_id)?.node_id,
                g.get_port(input_port_id)?.node_id,
            ))
        });
        if let Some((source_id, target_id)) = nodes {
            self.block_nodes(source_id, target_id);
        }
    }

    pub fn blocked_reason(
        mut self: Pin<&mut Self>,
        output_port_id: u32,
        input_port_id: u32,
    ) -> QString {
        self.as_mut().refresh_meter_redirects();
        let Some(patchbay) = self.rust().patchbay.as_ref().filter(|pb| pb.enabled) else {
            return QString::default();
        };
        match patchbay.blocking_rule_for_ports(output_port_id, input_port_id) {
            Some(rule) => QString::from(&format!(
                "{} \u{2192} {}",
                rule.source_pattern, rule.target_pattern
            )),
            None => QString::default(),
        }
    }

    pub fn get_blocked_pairs_json(self: Pin<&mut Self>) -> QString {
        let Some(patchbay) = self.rust().patchbay.as_ref().filter(|pb| pb.enabled) else {
            return QString::from("[]");
        };
        let pairs: Vec<serde_json::Value> = patchbay
            .blocked_pairs()
            .into_iter()
            .map(|(source, target, rule_id)| {
                let label = patchbay
                    .rules()
                    .iter()
                    .find(|r| r.id == rule_id)
                    .map(|r| format!("{} \u{2192} {}", r.source_pattern, r.target_pattern))
                    .unwrap_or_default();
                let owner = &self.rust().subgraphs.vm.real_owner;
                serde_json::json!({
                    "source": owner.get(&source).copied().unwrap_or(source),
                    "target": owner.get(&target).copied().unwrap_or(target),
                    "ruleId": rule_id,
                    "label": label,
                })
            })
            .collect();
        QString::from(&serde_json::to_string(&pairs).unwrap_or_else(|_| "[]".into()))
    }

    pub(super) fn block_nodes(mut self: Pin<&mut Self>, source_id: u32, target_id: u32) {
        let Some(graph) = self.rust().graph.clone() else {
            return;
        };
        let (Some(source), Some(target)) = (graph.get_node(source_id), graph.get_node(target_id))
        else {
            return;
        };
        if source.id == target.id {
            return;
        }

        let changed = self
            .as_mut()
            .rust_mut()
            .patchbay
            .as_mut()
            .is_some_and(|pb| pb.block_pair(&source, &target));
        if changed {
            log::info!(
                "Blocked: {} -> {}",
                source.display_name(),
                target.display_name()
            );
            save_rules(self.rust().patchbay.as_ref());
        }
        self.as_mut().rules_changed();

        if let Some(ref tx) = self.rust().cmd_tx {
            for link in graph
                .get_all_links()
                .iter()
                .filter(|l| l.output_node_id == source.id && l.input_node_id == target.id)
            {
                let _ = tx.send(PwCommand::Disconnect { link_id: link.id });
            }
        }

        self.as_mut().rust_mut().links_dirty = true;
        if self.rust().links_dirty_since.is_none() {
            self.as_mut().rust_mut().links_dirty_since = Some(Instant::now());
        }
    }

    pub fn insert_on_link_preview_json(
        self: Pin<&mut Self>,
        link_id: u32,
        node_id: u32,
    ) -> QString {
        let Some(ref graph) = self.rust().graph else {
            return QString::from("");
        };
        let Ok(plan) = plan_insert_on_link(graph, link_id, node_id) else {
            return QString::from("");
        };
        let add: Vec<[u32; 2]> = plan
            .rewires
            .iter()
            .flat_map(|r| {
                [
                    [r.upstream_out, r.plugin_in],
                    [r.plugin_out, r.downstream_in],
                ]
            })
            .collect();
        let json = serde_json::json!({ "remove": plan.links_to_remove, "add": add });
        QString::from(&json.to_string())
    }

    pub fn insert_node_on_link(mut self: Pin<&mut Self>, link_id: u32, node_id: u32) {
        let graph = self.rust().graph.clone();
        let Some(ref graph) = graph else { return };

        let plan = match plan_insert_on_link(graph, link_id, node_id) {
            Ok(plan) => plan,
            Err(why) => {
                log::warn!("insert_node_on_link: {}", why);
                return;
            }
        };

        if let Some(ref tx) = self.rust().cmd_tx {
            for lid in &plan.links_to_remove {
                let _ = tx.send(PwCommand::Disconnect { link_id: *lid });
            }
            for r in &plan.rewires {
                let _ = tx.send(PwCommand::Connect {
                    output_port_id: r.upstream_out,
                    input_port_id: r.plugin_in,
                });
                let _ = tx.send(PwCommand::Connect {
                    output_port_id: r.plugin_out,
                    input_port_id: r.downstream_in,
                });
            }
        }

        let upstream_node_id = plan.upstream_node_id;
        let downstream_node_id = plan.downstream_node_id;
        log::info!(
            "insert_node_on_link: inserted node {} on {} links between nodes {} and {}",
            node_id,
            plan.links_to_remove.len(),
            upstream_node_id,
            downstream_node_id
        );

        let mut rule_data: Vec<(Node, Node, Port, Port)> = Vec::new();
        type NewLinkData = (Node, Node, Port, Port, Node, Node, Port, Port);
        let mut new_link_data: Vec<NewLinkData> = Vec::new();

        for r in &plan.rewires {
            if let (Some(source_node), Some(target_node), Some(out_port), Some(in_port)) = (
                graph.get_node(upstream_node_id),
                graph.get_node(downstream_node_id),
                graph.get_port(r.upstream_out),
                graph.get_port(r.downstream_in),
            ) {
                rule_data.push((source_node, target_node, out_port, in_port));
            }

            if let (Some(up_node), Some(ins_node), Some(up_port), Some(ins_in_port)) = (
                graph.get_node(upstream_node_id),
                graph.get_node(node_id),
                graph.get_port(r.upstream_out),
                graph.get_port(r.plugin_in),
            ) && let (Some(ins_node2), Some(dn_node), Some(ins_out_port), Some(dn_port)) = (
                graph.get_node(node_id),
                graph.get_node(downstream_node_id),
                graph.get_port(r.plugin_out),
                graph.get_port(r.downstream_in),
            ) {
                new_link_data.push((
                    up_node,
                    ins_node,
                    up_port,
                    ins_in_port,
                    ins_node2,
                    dn_node,
                    ins_out_port,
                    dn_port,
                ));
            }
        }

        let mut rules_changed = false;
        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            for (source_node, target_node, out_port, in_port) in &rule_data {
                if patchbay.unlearn_from_link(source_node, target_node, out_port, in_port) {
                    log::info!(
                        "insert_node_on_link: unlearned rule {}:{} -> {}:{}",
                        source_node.display_name(),
                        out_port.name,
                        target_node.display_name(),
                        in_port.name,
                    );
                    rules_changed = true;
                }
            }

            for (
                up_node,
                ins_node,
                up_port,
                ins_in_port,
                ins_node2,
                dn_node,
                ins_out_port,
                dn_port,
            ) in &new_link_data
            {
                if patchbay.learn_from_link(up_node, ins_node, up_port, ins_in_port) {
                    log::info!(
                        "insert_node_on_link: learned rule {}:{} -> {}:{}",
                        up_node.display_name(),
                        up_port.name,
                        ins_node.display_name(),
                        ins_in_port.name,
                    );
                    rules_changed = true;
                }
                if patchbay.learn_from_link(ins_node2, dn_node, ins_out_port, dn_port) {
                    log::info!(
                        "insert_node_on_link: learned rule {}:{} -> {}:{}",
                        ins_node2.display_name(),
                        ins_out_port.name,
                        dn_node.display_name(),
                        dn_port.name,
                    );
                    rules_changed = true;
                }
            }
        }

        if rules_changed {
            save_rules(self.rust().patchbay.as_ref());
        }

        self.as_mut().rust_mut().links_dirty = true;
        if self.rust().links_dirty_since.is_none() {
            self.as_mut().rust_mut().links_dirty_since = Some(Instant::now());
        }
    }
}

impl qobject::AppController {
    pub(super) fn connect_bridge(mut self: Pin<&mut Self>, pairs: &[(u32, u32)]) {
        for &(out_port, in_port) in pairs {
            log::info!("Bridging removed plugin: {} -> {}", out_port, in_port);
            self.as_mut().connect_ports(out_port, in_port);
        }
    }
}

pub(super) fn plan_bridge_on_remove(graph: &GraphState, node_id: u32) -> Vec<(u32, u32)> {
    use crate::pipewire::MediaType;
    let audio_ports = |dir: PortDirection| -> Vec<Port> {
        let mut ports: Vec<Port> = graph
            .get_ports_for_node(node_id)
            .into_iter()
            .filter(|p| p.direction == dir && p.media_type == Some(MediaType::Audio))
            .collect();
        ports.sort_by(|a, b| crate::pipewire::state::natural_cmp(&a.name, &b.name));
        ports
    };
    let inputs = audio_ports(PortDirection::Input);
    let outputs = audio_ports(PortDirection::Output);
    if inputs.is_empty() || inputs.len() != outputs.len() {
        return Vec::new();
    }

    let links = graph.get_all_links();
    let mut pairs: Vec<(u32, u32)> = Vec::new();
    for (inp, out) in inputs.iter().zip(outputs.iter()) {
        let feeds: Vec<_> = links
            .iter()
            .filter(|l| l.input_port_id == inp.id && l.output_node_id != node_id)
            .collect();
        let fed: Vec<_> = links
            .iter()
            .filter(|l| l.output_port_id == out.id && l.input_node_id != node_id)
            .collect();
        for up in &feeds {
            for down in &fed {
                let pair = (up.output_port_id, down.input_port_id);
                let wired = links
                    .iter()
                    .any(|l| l.output_port_id == pair.0 && l.input_port_id == pair.1);
                if up.output_node_id != down.input_node_id && !wired && !pairs.contains(&pair) {
                    pairs.push(pair);
                }
            }
        }
    }
    pairs
}

pub(super) struct InsertRewire {
    pub(super) upstream_out: u32,
    pub(super) plugin_in: u32,
    pub(super) plugin_out: u32,
    pub(super) downstream_in: u32,
}

pub(super) struct InsertPlan {
    pub(super) upstream_node_id: u32,
    pub(super) downstream_node_id: u32,
    pub(super) links_to_remove: Vec<u32>,
    pub(super) rewires: Vec<InsertRewire>,
}

pub(super) fn plan_insert_on_link(
    graph: &GraphState,
    link_id: u32,
    node_id: u32,
) -> Result<InsertPlan, String> {
    use crate::pipewire::MediaType;
    let link = graph
        .get_link(link_id)
        .ok_or_else(|| format!("link {} not found", link_id))?;
    let node = graph
        .get_node(node_id)
        .ok_or_else(|| format!("node {} not found", node_id))?;
    if link.output_node_id == node_id || link.input_node_id == node_id {
        return Err(format!(
            "node {} is already part of link {}",
            node_id, link_id
        ));
    }
    if node.node_type != Some(NodeType::Plugin) {
        return Err(format!("node {} is not a plugin", node_id));
    }

    let audio_ports = |nid: u32, dir: PortDirection| -> Vec<Port> {
        graph
            .get_ports_for_node(nid)
            .into_iter()
            .filter(|p| p.direction == dir && p.media_type == Some(MediaType::Audio))
            .collect()
    };
    let mut node_inputs = audio_ports(node_id, PortDirection::Input);
    let mut node_outputs = audio_ports(node_id, PortDirection::Output);
    if node_inputs.is_empty() || node_outputs.is_empty() {
        return Err(format!("node {} has no audio input/output ports", node_id));
    }
    node_inputs.sort_by(|a, b| crate::pipewire::state::natural_cmp(&a.name, &b.name));
    node_outputs.sort_by(|a, b| crate::pipewire::state::natural_cmp(&a.name, &b.name));

    let upstream_node_id = link.output_node_id;
    let downstream_node_id = link.input_node_id;
    let upstream_ports = audio_ports(upstream_node_id, PortDirection::Output);
    let downstream_ports = audio_ports(downstream_node_id, PortDirection::Input);

    let upstream_idx = upstream_ports
        .iter()
        .position(|p| p.id == link.output_port_id);
    let downstream_idx = downstream_ports
        .iter()
        .position(|p| p.id == link.input_port_id);
    let (Some(upstream_idx), Some(downstream_idx)) = (upstream_idx, downstream_idx) else {
        return Err(format!("link {} is not an audio link", link_id));
    };

    let mut links_to_remove = Vec::new();
    let mut pairs: Vec<(u32, usize, u32, usize)> = Vec::new();
    for existing in graph.get_all_links() {
        if existing.output_node_id == upstream_node_id
            && existing.input_node_id == downstream_node_id
        {
            let u_idx = upstream_ports
                .iter()
                .position(|p| p.id == existing.output_port_id);
            let d_idx = downstream_ports
                .iter()
                .position(|p| p.id == existing.input_port_id);
            if let (Some(ui), Some(di)) = (u_idx, d_idx) {
                links_to_remove.push(existing.id);
                pairs.push((existing.output_port_id, ui, existing.input_port_id, di));
            }
        }
    }
    if links_to_remove.is_empty() {
        links_to_remove.push(link_id);
        pairs.push((
            link.output_port_id,
            upstream_idx,
            link.input_port_id,
            downstream_idx,
        ));
    }

    let max_in = node_inputs.len() - 1;
    let max_out = node_outputs.len() - 1;
    let rewires = pairs
        .into_iter()
        .map(|(up_port, up_idx, down_port, down_idx)| InsertRewire {
            upstream_out: up_port,
            plugin_in: node_inputs[up_idx.min(max_in)].id,
            plugin_out: node_outputs[down_idx.min(max_out)].id,
            downstream_in: down_port,
        })
        .collect();
    Ok(InsertPlan {
        upstream_node_id,
        downstream_node_id,
        links_to_remove,
        rewires,
    })
}
