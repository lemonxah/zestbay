use super::*;

impl qobject::AppController {
    pub(super) fn drop_subgraph_member(mut self: Pin<&mut Self>, stable_id: &str) {
        if self.rust().subgraphs.owner_of(stable_id).is_none() {
            return;
        }
        for def in self.as_mut().rust_mut().subgraphs.defs.iter_mut() {
            def.members.retain(|m| m != stable_id);
        }
        self.as_mut().persist_subgraphs();
        self.as_mut().subgraphs_changed();
    }
}

impl qobject::AppController {
    pub fn get_available_plugins_json(self: Pin<&mut Self>) -> QString {
        if let Some(ref mgr) = self.rust().plugin_manager {
            let json_plugins: Vec<serde_json::Value> = mgr
                .available_plugins()
                .iter()
                .map(|p| {
                    let mut entry = serde_json::json!({
                        "uri": p.uri,
                        "name": p.name,
                        "category": p.category.display_name(),
                        "author": p.author.as_deref().unwrap_or(""),
                        "audioIn": p.audio_inputs,
                        "audioOut": p.audio_outputs,
                        "controlIn": p.control_inputs,
                        "controlOut": p.control_outputs,
                        "compatible": p.compatible,
                        "requiredFeatures": p.required_features,
                        "hasUi": p.has_ui,
                        "format": p.format.as_str(),
                    });
                    if let Some(twin) = mgr.ui_twin(p) {
                        entry["uiTwinFormat"] = serde_json::json!(twin.format.as_str());
                        entry["uiTwinName"] = serde_json::json!(twin.name);
                        entry["uiTwinUri"] = serde_json::json!(twin.uri);
                    }
                    entry
                })
                .collect();
            let json = serde_json::to_string(&json_plugins).unwrap_or_default();
            QString::from(&json)
        } else {
            QString::from("[]")
        }
    }

    pub fn rescan_plugins(mut self: Pin<&mut Self>) {
        if self.rust().plugins_scanning
            || self.rust().plugins_rescanning
            || self.rust().plugin_scan_rx.is_some()
        {
            return;
        }
        match crate::plugin::scan::start() {
            Ok(rx) => {
                log::info!("Rescanning plugins");
                self.as_mut().rust_mut().plugin_scan_rx = Some(rx);
                self.as_mut().set_plugin_rescan_progress(0.0);
                self.as_mut().set_plugins_rescanning(true);
            }
            Err(e) => {
                self.as_mut().error_occurred(QString::from(
                    format!("Could not start plugin discovery: {e}").as_str(),
                ));
            }
        }
    }

    pub fn rescan_plugins_if_stale(self: Pin<&mut Self>) {
        if self
            .rust()
            .last_plugin_scan
            .is_some_and(|t| t.elapsed() >= PLUGIN_RESCAN_AFTER)
        {
            self.rescan_plugins();
        }
    }
}

impl qobject::AppController {
    pub fn add_plugin(mut self: Pin<&mut Self>, uri: QString) -> QString {
        let uri_str: String = uri.to_string();

        let (display_name, initial_params, plugin_format) =
            if let Some(ref mgr) = self.rust().plugin_manager {
                let plugin = mgr.find_plugin(&uri_str);
                let base_name = plugin
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| uri_str.clone());
                let name = self.unique_display_name(&base_name);
                let format = plugin
                    .map(|p| p.format)
                    .unwrap_or(crate::plugin::PluginFormat::Lv2);
                let params: Vec<crate::lv2::Lv2ParameterValue> = plugin
                    .map(|p| {
                        p.ports
                            .iter()
                            .filter(|port| port.port_type == crate::lv2::Lv2PortType::ControlInput)
                            .map(|port| crate::lv2::Lv2ParameterValue {
                                port_index: port.index,
                                symbol: port.symbol.clone(),
                                name: port.name.clone(),
                                value: port.default_value,
                                min: port.min_value,
                                max: port.max_value,
                                default: port.default_value,
                                is_toggle: port.is_toggle,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                (name, params, format)
            } else {
                return QString::from("");
            };

        let mut initial_params = initial_params;
        let default_preset = self
            .rust()
            .presets
            .default_name(&uri_str)
            .map(str::to_string);
        let mut default_values: Vec<(usize, f32)> = Vec::new();
        let mut default_state: Vec<crate::lv2::state::StateEntry> = Vec::new();
        if let Some(ref name) = default_preset
            && let Some(preset) = self.rust().presets.get(&uri_str, name)
        {
            if plugin_format == crate::plugin::PluginFormat::Lv2 {
                default_state = preset.lv2_state.clone();
            }
            default_values = crate::presets::values_to_apply(preset, &initial_params);
            for (port_index, value) in &default_values {
                if let Some(p) = initial_params
                    .iter_mut()
                    .find(|p| p.port_index == *port_index)
                {
                    p.value = *value;
                }
            }
        }

        let instance_id = self.rust().next_instance_id;
        self.as_mut().rust_mut().next_instance_id += 1;

        let format_str = plugin_format.as_str().to_string();
        let scan_info = self
            .rust()
            .plugin_manager
            .as_ref()
            .and_then(|mgr| mgr.find_plugin_with_format(&uri_str, plugin_format))
            .filter(|info| info.format != crate::plugin::PluginFormat::Lv2)
            .cloned();

        let stable_id = uuid::Uuid::new_v4().to_string();
        let stable_id_for_group = stable_id.clone();
        self.as_mut().forget_plugin_links(&display_name);
        if let Some(ref name) = default_preset {
            self.as_mut()
                .rust_mut()
                .presets
                .select(&stable_id, Some(name));
            persist_presets(&self.rust().presets);
            if plugin_format == crate::plugin::PluginFormat::Lv2 {
                crate::presets::restore_preset_files(&uri_str, &stable_id, name);
            }
        }
        if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
            let info = crate::lv2::Lv2InstanceInfo {
                id: instance_id,
                stable_id: stable_id.clone(),
                plugin_uri: uri_str.clone(),
                format: plugin_format,
                scan_info: scan_info.clone(),
                display_name: display_name.clone(),
                pw_node_id: None,
                parameters: initial_params,
                active: true,
                bypassed: false,
                enabled: true,
                lv2_state: default_state.clone(),
                plugin_state: None,
                port_names: Default::default(),
            };
            mgr.register_instance(info);
        }

        if let Some(ref tx) = self.rust().cmd_tx {
            log::info!(
                "Adding plugin: uri={} instance_id={} name={} format={}",
                uri_str,
                instance_id,
                display_name,
                format_str
            );
            let _ = tx.send(PwCommand::AddPlugin {
                plugin_uri: uri_str.clone(),
                instance_id,
                display_name: display_name.clone(),
                format: format_str,
                scan_info,
                lv2_state: default_state,
                plugin_state: None,
                stable_id,
                enabled: true,
            });
            for (port_index, value) in default_values {
                let _ = tx.send(PwCommand::SetPluginParameter {
                    instance_id,
                    port_index,
                    value,
                });
            }
        }

        persist_active_plugins(self.rust().plugin_manager.as_ref());

        if let Some(view) = self.rust().subgraphs.view.clone() {
            if let Some(def) = self.as_mut().rust_mut().subgraphs.find_mut(&view) {
                def.members.push(stable_id_for_group.clone());
            }
            self.as_mut().persist_subgraphs();
            self.as_mut().subgraphs_changed();
        }

        QString::from(&display_name)
    }

    pub fn remove_plugin(mut self: Pin<&mut Self>, node_id: u32) {
        let instance_id = self.find_instance_id_for_node(node_id);
        if let Some(instance_id) = instance_id {
            let bridge = self
                .rust()
                .graph
                .as_ref()
                .map(|g| plan_bridge_on_remove(g, node_id))
                .unwrap_or_default();
            if let Some(ref tx) = self.rust().cmd_tx {
                log::info!(
                    "Remove plugin: node_id={} instance_id={}",
                    node_id,
                    instance_id
                );
                let _ = tx.send(PwCommand::RemovePlugin { instance_id });
            }
            self.as_mut().connect_bridge(&bridge);
            self.as_mut().forget_meter_rules(instance_id);
        } else {
            log::warn!(
                "remove_plugin: no LV2 instance found for node_id={}",
                node_id
            );
        }
    }

    pub fn open_plugin_ui(self: Pin<&mut Self>, node_id: u32) {
        let instance_id = self.find_instance_id_for_node(node_id);
        if let Some(instance_id) = instance_id {
            if let Some(ref tx) = self.rust().cmd_tx {
                log::info!(
                    "Open plugin UI: node_id={} instance_id={}",
                    node_id,
                    instance_id
                );
                let _ = tx.send(PwCommand::OpenPluginUI { instance_id });
            }
        } else {
            log::warn!(
                "open_plugin_ui: no LV2 instance found for node_id={}",
                node_id
            );
        }
    }

    pub fn rename_plugin(mut self: Pin<&mut Self>, node_id: u32, new_name: QString) {
        let name_str: String = new_name.to_string();
        let instance_id = self.find_instance_id_for_node(node_id);
        if let Some(instance_id) = instance_id {
            log::info!(
                "Rename plugin: node_id={} instance_id={} new_name={}",
                node_id,
                instance_id,
                name_str
            );
            let old_name = self
                .rust()
                .plugin_manager
                .as_ref()
                .and_then(|mgr| mgr.get_instance(instance_id))
                .map(|info| info.display_name.clone());
            if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager
                && let Some(info) = mgr.get_instance_mut(instance_id)
            {
                info.display_name = name_str.clone();
            }
            if let Some(ref graph) = self.rust().graph {
                graph.set_node_description(node_id, &name_str);
            }
            crate::plugin::cpu_stats::global_cpu_tracker().rename(instance_id, &name_str);
            if let Some(ref tx) = self.rust().cmd_tx {
                let _ = tx.send(PwCommand::RenamePlugin {
                    instance_id,
                    display_name: name_str.clone(),
                });
            }
            if let Some(old_name) = old_name
                && old_name != name_str
            {
                self.as_mut().rename_plugin_links(&old_name, &name_str);
                let renamed = self
                    .as_mut()
                    .rust_mut()
                    .patchbay
                    .as_mut()
                    .is_some_and(|pb| pb.rename_node(&old_name, &name_str));
                if renamed {
                    if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
                        patchbay.rules_dirty = false;
                    }
                    save_rules(self.rust().patchbay.as_ref());
                    self.as_mut().rules_changed();
                }
            }
            self.as_mut()
                .rust_mut()
                .forgotten_link_nodes
                .remove(&name_str);
            self.as_mut().mark_links_dirty();
            persist_active_plugins(self.rust().plugin_manager.as_ref());
            self.as_mut().sync_tray_plugins();
            self.as_mut().graph_changed();
        } else {
            log::warn!(
                "rename_plugin: no LV2 instance found for node_id={}",
                node_id
            );
        }
    }

    pub fn rename_port(mut self: Pin<&mut Self>, port_id: u32, new_name: QString) {
        let new_name = new_name.to_string().trim().to_string();
        let Some(port) = self.rust().graph.as_ref().and_then(|g| g.get_port(port_id)) else {
            return;
        };
        let Some(instance_id) = self.find_instance_id_for_node(port.node_id) else {
            log::warn!("rename_port: port {} does not belong to a plugin", port_id);
            return;
        };
        if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager
            && let Some(info) = mgr.get_instance_mut(instance_id)
        {
            if new_name.is_empty() {
                info.port_names.remove(&port.name);
            } else {
                info.port_names.insert(port.name.clone(), new_name);
            }
        }
        persist_active_plugins(self.rust().plugin_manager.as_ref());
        self.as_mut().graph_changed();
    }
}

impl qobject::AppController {
    pub fn get_active_plugins_json(self: Pin<&mut Self>) -> QString {
        if let Some(ref mgr) = self.rust().plugin_manager {
            let mut entries: Vec<serde_json::Value> = mgr
                .active_instances()
                .values()
                .map(|info| {
                    let params: Vec<serde_json::Value> = info
                        .parameters
                        .iter()
                        .map(|p| {
                            serde_json::json!({
                                "portIndex": p.port_index,
                                "symbol": p.symbol,
                                "name": p.name,
                                "value": p.value,
                                "min": p.min,
                                "max": p.max,
                                "default": p.default,
                                "isToggle": p.is_toggle,
                            })
                        })
                        .collect();
                    serde_json::json!({
                        "instanceId": info.id,
                        "stableId": info.stable_id,
                        "pluginUri": info.plugin_uri,
                        "displayName": info.display_name,
                        "bypassed": info.bypassed,
                        "enabled": info.enabled,
                        "active": info.pw_node_id.is_some(),
                        "parameters": params,
                    })
                })
                .collect();
            entries.sort_by(|a, b| {
                let a_name = a["displayName"].as_str().unwrap_or("");
                let b_name = b["displayName"].as_str().unwrap_or("");
                a_name.cmp(b_name)
            });
            let json = serde_json::to_string(&entries).unwrap_or_default();
            QString::from(&json)
        } else {
            QString::from("[]")
        }
    }

    pub fn remove_plugin_by_stable_id(mut self: Pin<&mut Self>, stable_id: QString) {
        let sid: String = stable_id.to_string();

        let instance_id = self
            .rust()
            .plugin_manager
            .as_ref()
            .and_then(|mgr| mgr.instance_id_for_stable_id(&sid));

        if let Some(instance_id) = instance_id {
            let bridge = self
                .rust()
                .plugin_manager
                .as_ref()
                .and_then(|mgr| mgr.get_instance(instance_id))
                .and_then(|info| info.pw_node_id)
                .zip(self.rust().graph.as_ref())
                .map(|(nid, g)| plan_bridge_on_remove(g, nid))
                .unwrap_or_default();
            if let Some(ref tx) = self.rust().cmd_tx {
                let _ = tx.send(PwCommand::RemovePlugin { instance_id });
            }
            self.as_mut().connect_bridge(&bridge);
            self.as_mut().forget_meter_rules(instance_id);
            let name = self
                .rust()
                .plugin_manager
                .as_ref()
                .and_then(|mgr| mgr.get_instance(instance_id))
                .map(|info| info.display_name.clone());
            if let Some(name) = name {
                self.as_mut().forget_plugin_links(&name);
            }
            self.as_mut().drop_subgraph_member(&sid);
            if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
                mgr.remove_instance(instance_id);
            }
            self.as_mut().drop_plugin_midi_mappings(instance_id);
            persist_active_plugins(self.rust().plugin_manager.as_ref());
            log::info!("Removed plugin instance (stable_id={})", sid);
        } else {
            log::warn!(
                "remove_plugin_by_stable_id: no instance found for stable_id={}",
                sid
            );
        }
    }

    pub fn reset_plugin_params_by_stable_id(mut self: Pin<&mut Self>, stable_id: QString) {
        let sid: String = stable_id.to_string();

        let resets: Vec<(u64, usize, f32)> = if let Some(ref mgr) = self.rust().plugin_manager {
            if let Some(info) = mgr.find_by_stable_id(&sid) {
                info.parameters
                    .iter()
                    .map(|p| (info.id, p.port_index, p.default))
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        if resets.is_empty() {
            return;
        }

        let instance_id = resets[0].0;
        for (_, port_index, default) in &resets {
            if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
                mgr.update_parameter(instance_id, *port_index, *default);
            }
            if let Some(ref tx) = self.rust().cmd_tx {
                let _ = tx.send(PwCommand::SetPluginParameter {
                    instance_id,
                    port_index: *port_index,
                    value: *default,
                });
            }
        }

        self.as_mut().rust_mut().params_dirty = true;
        if self.rust().params_dirty_since.is_none() {
            self.as_mut().rust_mut().params_dirty_since = Some(Instant::now());
        }
        log::info!(
            "Reset {} params to defaults for stable_id={}",
            resets.len(),
            sid
        );
    }

    pub fn set_plugin_param_by_stable_id(
        mut self: Pin<&mut Self>,
        stable_id: QString,
        port_index: u32,
        value: f32,
    ) {
        let sid: String = stable_id.to_string();

        let instance_id = self
            .rust()
            .plugin_manager
            .as_ref()
            .and_then(|mgr| mgr.instance_id_for_stable_id(&sid));

        if let Some(instance_id) = instance_id {
            if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
                mgr.update_parameter(instance_id, port_index as usize, value);
            }
            if let Some(ref tx) = self.rust().cmd_tx {
                let _ = tx.send(PwCommand::SetPluginParameter {
                    instance_id,
                    port_index: port_index as usize,
                    value,
                });
            }
            self.as_mut().rust_mut().params_dirty = true;
            if self.rust().params_dirty_since.is_none() {
                self.as_mut().rust_mut().params_dirty_since = Some(Instant::now());
            }
        }
    }
}

impl qobject::AppController {
    pub(super) fn find_instance_id_for_node(&self, node_id: u32) -> Option<u64> {
        if let Some(ref mgr) = self.rust().plugin_manager {
            for (id, info) in mgr.active_instances() {
                if info.pw_node_id == Some(node_id) {
                    return Some(*id);
                }
            }
        }
        None
    }

    pub(super) fn unique_display_name(&self, base_name: &str) -> String {
        let existing: Vec<String> = if let Some(ref mgr) = self.rust().plugin_manager {
            mgr.active_instances()
                .values()
                .map(|info| info.display_name.clone())
                .collect()
        } else {
            Vec::new()
        };

        if !existing.iter().any(|n| n == base_name) {
            return base_name.to_string();
        }

        for n in 2.. {
            let candidate = format!("{} #{}", base_name, n);
            if !existing.iter().any(|n| n == &candidate) {
                return candidate;
            }
        }
        unreachable!()
    }
}
