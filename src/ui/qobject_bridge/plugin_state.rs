use super::*;

impl qobject::AppController {
    pub(super) fn apply_plugin_state(
        mut self: Pin<&mut Self>,
        instance_id: u64,
        lv2_state: Vec<crate::lv2::state::StateEntry>,
        blob: Option<Vec<u8>>,
    ) {
        if let Some((uri, name)) = self
            .as_mut()
            .rust_mut()
            .preset_state_refresh
            .remove(&instance_id)
            && !lv2_state.is_empty()
        {
            self.as_mut()
                .rust_mut()
                .presets
                .set_lv2_state(&uri, &name, lv2_state.clone());
            persist_presets(&self.rust().presets);
            if let Some(scope) = self
                .rust()
                .plugin_manager
                .as_ref()
                .and_then(|m| m.get_instance(instance_id))
                .map(|i| i.stable_id.clone())
            {
                crate::presets::store_preset_files(&uri, &scope, &name);
            }
        }
        let mut labels_changed = false;
        if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager
            && let Some(info) = mgr.get_instance_mut(instance_id)
        {
            log::debug!(
                "Plugin state received for instance {}: {} LV2 entries, blob {} bytes",
                instance_id,
                lv2_state.len(),
                blob.as_ref().map(|b| b.len()).unwrap_or(0)
            );
            match info.format {
                crate::plugin::PluginFormat::Lv2 => {
                    labels_changed = crate::lv2::atom_state::is_lsp_mixer(&info.plugin_uri)
                        && info
                            .lv2_state
                            .iter()
                            .find(|e| e.key_uri.ends_with("/KVT"))
                            .map(|e| (&e.value, &e.urid_references))
                            != lv2_state
                                .iter()
                                .find(|e| e.key_uri.ends_with("/KVT"))
                                .map(|e| (&e.value, &e.urid_references));
                    info.lv2_state = lv2_state;
                }
                _ => {
                    if blob.is_some() {
                        info.plugin_state = blob;
                    }
                }
            }
        }
        if labels_changed {
            if self.plugins_persistable() {
                persist_active_plugins(self.rust().plugin_manager.as_ref());
            }
            self.as_mut().graph_changed();
        }
    }

    pub(super) fn plugins_persistable(&self) -> bool {
        !crate::PLUGINS_FROZEN.load(std::sync::atomic::Ordering::SeqCst)
            && self
                .rust()
                .plugin_manager
                .as_ref()
                .is_some_and(|mgr| mgr.startup_complete() && !mgr.active_instances().is_empty())
    }

    pub(super) fn maybe_autosave_plugin_states(mut self: Pin<&mut Self>) {
        const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(60);
        const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
        if let Some(sent) = self.rust().state_request_sent_at {
            if sent.elapsed() < REQUEST_TIMEOUT {
                return;
            }
            log::warn!("Plugin state request timed out; will retry");
            self.as_mut().rust_mut().state_request_sent_at = None;
        }
        if self.rust().last_state_autosave.elapsed() < AUTOSAVE_INTERVAL
            || !self.rust().restore_instance_ids.is_empty()
            || !self.plugins_persistable()
        {
            return;
        }
        self.as_mut().rust_mut().last_state_autosave = Instant::now();
        if let Some(ref tx) = self.rust().cmd_tx
            && tx.send(PwCommand::RequestPluginStates).is_ok()
        {
            self.as_mut().rust_mut().state_request_sent_at = Some(Instant::now());
        }
    }

    pub(super) fn collect_plugin_states_blocking(mut self: Pin<&mut Self>, timeout: Duration) {
        let Some(rx) = self.as_mut().rust_mut().event_rx.take() else {
            return;
        };
        let sent = self
            .rust()
            .cmd_tx
            .as_ref()
            .is_some_and(|tx| tx.send(PwCommand::RequestPluginStates).is_ok());
        if sent {
            let mut dones_needed = if self.rust().state_request_sent_at.is_some() {
                2
            } else {
                1
            };
            let deadline = Instant::now() + timeout;
            while dones_needed > 0 {
                let now = Instant::now();
                if now >= deadline {
                    log::warn!("Timed out waiting for plugin states on quit");
                    break;
                }
                match rx.recv_timeout(deadline - now) {
                    Ok(PwEvent::Plugin(PluginEvent::PluginStateSaved {
                        instance_id,
                        lv2_state,
                        blob,
                    })) => {
                        self.as_mut()
                            .apply_plugin_state(instance_id, lv2_state, blob);
                    }
                    Ok(PwEvent::Plugin(PluginEvent::PluginStatesDone)) => dones_needed -= 1,
                    Ok(other) => self.as_mut().rust_mut().stashed_events.push_back(other),
                    Err(_) => break,
                }
            }
            self.as_mut().rust_mut().state_request_sent_at = None;
        }
        self.as_mut().rust_mut().event_rx = Some(rx);
    }
}

pub(super) fn load_saved_plugins() -> Result<Vec<SavedPlugin>, ()> {
    load_config::<Vec<SavedPlugin>>("plugins.json").map(Option::unwrap_or_default)
}

pub(super) fn restore_safety_cap(batch_size: usize) -> Duration {
    Duration::from_secs(30 + 15 * batch_size as u64)
}

pub(super) fn persist_active_plugins(plugin_manager: Option<&PluginManager>) {
    if !plugin_manager.is_some_and(|mgr| mgr.startup_complete()) {
        return;
    }
    if crate::PLUGINS_FROZEN.load(std::sync::atomic::Ordering::SeqCst) {
        log::info!("persist_active_plugins: skipped (plugins frozen in safe mode)");
        return;
    }
    let mut plugins: Vec<SavedPlugin> = plugin_manager
        .into_iter()
        .flat_map(|mgr| mgr.active_instances().values())
        .map(SavedPlugin::from)
        .collect();
    plugins.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    let json = serde_json::to_string_pretty(&plugins).unwrap_or_default();
    if write_config("plugins.json", json.as_bytes(), plugins.is_empty()) {
        log::debug!("persist_active_plugins: {} plugins written", plugins.len());
    }
}
