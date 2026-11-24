use super::*;

impl qobject::AppController {
    pub(super) fn preset_target(&self, node_id: u32) -> Option<(u64, String, String)> {
        let mgr = self.rust().plugin_manager.as_ref()?;
        mgr.active_instances()
            .iter()
            .find(|(_, info)| info.pw_node_id == Some(node_id))
            .map(|(id, info)| (*id, info.stable_id.clone(), info.plugin_uri.clone()))
    }

    pub fn get_plugin_presets_all_json(self: Pin<&mut Self>) -> QString {
        let mut out = serde_json::Map::new();
        if let Some(mgr) = self.rust().plugin_manager.as_ref() {
            let store = &self.rust().presets;
            for info in mgr.active_instances().values() {
                let Some(node_id) = info.pw_node_id else {
                    continue;
                };
                let selected = store.selected_name(&info.stable_id, &info.plugin_uri);
                let modified = selected
                    .and_then(|n| store.get(&info.plugin_uri, n))
                    .is_some_and(|p| {
                        crate::presets::is_modified(p, &info.parameters, &info.lv2_state)
                            || crate::presets::port_names_differ(p, &info.port_names)
                    });
                out.insert(
                    node_id.to_string(),
                    serde_json::json!({
                        "name": selected.unwrap_or(""),
                        "isDefault": selected.is_some()
                            && selected == store.default_name(&info.plugin_uri),
                        "modified": modified,
                        "count": store.names(&info.plugin_uri).len(),
                    }),
                );
            }
        }
        QString::from(&serde_json::Value::Object(out).to_string())
    }

    pub fn get_plugin_presets_json(self: Pin<&mut Self>, node_id: u32) -> QString {
        let Some((instance_id, stable_id, uri)) = self.preset_target(node_id) else {
            return QString::from("{}");
        };
        let store = &self.rust().presets;
        let selected = store.selected_name(&stable_id, &uri).unwrap_or("");
        let changes: Vec<String> = self
            .rust()
            .plugin_manager
            .as_ref()
            .and_then(|m| m.get_instance(instance_id))
            .zip(store.get(&uri, selected))
            .map(|(info, preset)| {
                let mut changes =
                    crate::presets::differences(preset, &info.parameters, &info.lv2_state);
                if crate::presets::port_names_differ(preset, &info.port_names) {
                    changes.push("Port names".to_string());
                }
                changes
            })
            .unwrap_or_default();
        if !changes.is_empty() {
            log::info!(
                "Preset \"{}\" differs from the plugin: {}",
                selected,
                changes.join("; ")
            );
        }
        QString::from(
            &serde_json::json!({
                "presets": store.names(&uri),
                "selected": selected,
                "default": store.default_name(&uri).unwrap_or(""),
                "changes": changes,
            })
            .to_string(),
        )
    }

    pub fn apply_plugin_preset(mut self: Pin<&mut Self>, node_id: u32, name: QString) -> bool {
        let name = name.to_string();
        let Some((instance_id, stable_id, uri)) = self.preset_target(node_id) else {
            return false;
        };
        let (values, reload_state, port_names) = {
            let Some(preset) = self.rust().presets.get(&uri, &name) else {
                return false;
            };
            let Some(info) = self
                .rust()
                .plugin_manager
                .as_ref()
                .and_then(|m| m.get_instance(instance_id))
            else {
                return false;
            };
            let state = (info.format == crate::plugin::PluginFormat::Lv2
                && crate::presets::state_differs(preset, &info.lv2_state))
            .then(|| preset.lv2_state.clone());
            (
                crate::presets::values_to_apply(preset, &info.parameters),
                state,
                preset.port_names.clone(),
            )
        };
        if !port_names.is_empty() {
            if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager
                && let Some(info) = mgr.get_instance_mut(instance_id)
            {
                info.port_names = port_names;
            }
            persist_active_plugins(self.rust().plugin_manager.as_ref());
            self.as_mut().graph_changed();
        }
        if let Some(state) = reload_state {
            crate::presets::restore_preset_files(&uri, &stable_id, &name);
            if let Some(ref tx) = self.rust().cmd_tx {
                let _ = tx.send(PwCommand::ReloadPluginWithState {
                    instance_id,
                    lv2_state: state.clone(),
                    params: values.clone(),
                });
            }
            if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
                if let Some(info) = mgr.get_instance_mut(instance_id) {
                    info.lv2_state = state;
                }
                for (port_index, value) in &values {
                    mgr.update_parameter(instance_id, *port_index, *value);
                }
            }
        } else {
            for (port_index, value) in values {
                if let Some(ref tx) = self.rust().cmd_tx {
                    let _ = tx.send(PwCommand::SetPluginParameter {
                        instance_id,
                        port_index,
                        value,
                    });
                }
                if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
                    mgr.update_parameter(instance_id, port_index, value);
                }
            }
        }
        self.as_mut()
            .rust_mut()
            .presets
            .select(&stable_id, Some(&name));
        persist_presets(&self.rust().presets);
        self.as_mut().rust_mut().params_dirty = true;
        if self.rust().params_dirty_since.is_none() {
            self.as_mut().rust_mut().params_dirty_since = Some(Instant::now());
        }
        true
    }

    pub fn save_plugin_preset(mut self: Pin<&mut Self>, node_id: u32, name: QString) -> bool {
        let name = name.to_string().trim().to_string();
        if name.is_empty() {
            return false;
        }
        let Some((instance_id, stable_id, uri)) = self.preset_target(node_id) else {
            return false;
        };
        let (params, lv2_state, is_lv2, port_names) = match self
            .rust()
            .plugin_manager
            .as_ref()
            .and_then(|m| m.get_instance(instance_id))
        {
            Some(info) => (
                info.parameters.clone(),
                info.lv2_state.clone(),
                info.format == crate::plugin::PluginFormat::Lv2,
                info.port_names.clone(),
            ),
            None => return false,
        };
        let existed = self
            .as_mut()
            .rust_mut()
            .presets
            .save(&uri, &name, &params, &lv2_state);
        self.as_mut()
            .rust_mut()
            .presets
            .set_port_names(&uri, &name, &port_names);
        self.as_mut()
            .rust_mut()
            .presets
            .select(&stable_id, Some(&name));
        persist_presets(&self.rust().presets);
        if is_lv2 {
            crate::presets::store_preset_files(&uri, &stable_id, &name);
        }
        if is_lv2
            && let Some(ref tx) = self.rust().cmd_tx
            && tx.send(PwCommand::RequestPluginStates).is_ok()
        {
            self.as_mut()
                .rust_mut()
                .preset_state_refresh
                .insert(instance_id, (uri.clone(), name.clone()));
        }
        log::info!(
            "{} preset \"{}\" for {} ({} parameters, {} state entries)",
            if existed { "Updated" } else { "Saved" },
            name,
            uri,
            params.len(),
            lv2_state.len()
        );
        true
    }

    pub fn save_modified_presets(mut self: Pin<&mut Self>) -> i32 {
        let targets: Vec<(u32, String)> = {
            let Some(mgr) = self.rust().plugin_manager.as_ref() else {
                return 0;
            };
            let store = &self.rust().presets;
            mgr.active_instances()
                .values()
                .filter_map(|info| {
                    let node_id = info.pw_node_id?;
                    let name = store.selected_name(&info.stable_id, &info.plugin_uri)?;
                    let preset = store.get(&info.plugin_uri, name)?;
                    (crate::presets::is_modified(preset, &info.parameters, &info.lv2_state)
                        || crate::presets::port_names_differ(preset, &info.port_names))
                    .then(|| (node_id, name.to_string()))
                })
                .collect()
        };
        let mut saved = 0;
        for (node_id, name) in targets {
            if self
                .as_mut()
                .save_plugin_preset(node_id, QString::from(&name))
            {
                saved += 1;
            }
        }
        saved
    }

    pub fn start_new_plugin_preset(mut self: Pin<&mut Self>, node_id: u32) {
        let Some((instance_id, stable_id, _)) = self.preset_target(node_id) else {
            return;
        };
        let (defaults, has_lv2_state) = match self
            .rust()
            .plugin_manager
            .as_ref()
            .and_then(|m| m.get_instance(instance_id))
        {
            Some(info) => (
                info.parameters
                    .iter()
                    .map(|p| (p.port_index, p.default))
                    .collect::<Vec<(usize, f32)>>(),
                info.format == crate::plugin::PluginFormat::Lv2 && !info.lv2_state.is_empty(),
            ),
            None => return,
        };
        if has_lv2_state {
            if let Some(ref tx) = self.rust().cmd_tx {
                let _ = tx.send(PwCommand::ReloadPluginWithState {
                    instance_id,
                    lv2_state: Vec::new(),
                    params: defaults.clone(),
                });
            }
            if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager
                && let Some(info) = mgr.get_instance_mut(instance_id)
            {
                info.lv2_state.clear();
            }
        } else if let Some(ref tx) = self.rust().cmd_tx {
            for (port_index, value) in &defaults {
                let _ = tx.send(PwCommand::SetPluginParameter {
                    instance_id,
                    port_index: *port_index,
                    value: *value,
                });
            }
        }
        if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
            for (port_index, value) in &defaults {
                mgr.update_parameter(instance_id, *port_index, *value);
            }
        }
        self.as_mut().rust_mut().presets.select(&stable_id, None);
        persist_presets(&self.rust().presets);
        self.as_mut().rust_mut().params_dirty = true;
        if self.rust().params_dirty_since.is_none() {
            self.as_mut().rust_mut().params_dirty_since = Some(Instant::now());
        }
    }

    pub fn delete_plugin_preset(mut self: Pin<&mut Self>, node_id: u32, name: QString) {
        let Some((_, _, uri)) = self.preset_target(node_id) else {
            return;
        };
        let name = name.to_string();
        self.as_mut().rust_mut().presets.delete(&uri, &name);
        crate::presets::delete_preset_files(&uri, &name);
        persist_presets(&self.rust().presets);
    }

    pub fn set_default_plugin_preset(mut self: Pin<&mut Self>, node_id: u32, name: QString) {
        let Some((_, _, uri)) = self.preset_target(node_id) else {
            return;
        };
        let name = name.to_string();
        self.as_mut()
            .rust_mut()
            .presets
            .set_default(&uri, if name.is_empty() { None } else { Some(&name) });
        persist_presets(&self.rust().presets);
    }
}

pub(super) fn load_presets() -> crate::presets::PresetStore {
    load_config::<crate::presets::PresetStore>("presets.json")
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub(super) fn persist_presets(store: &crate::presets::PresetStore) {
    let json = serde_json::to_string_pretty(store).unwrap_or_default();
    write_config("presets.json", json.as_bytes(), store.is_empty());
}
