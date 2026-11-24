use super::*;

impl qobject::AppController {
    pub fn init(mut self: Pin<&mut Self>) {
        log::info!("AppController::init — starting PipeWire");

        let prefs = self.rust().prefs.clone();
        crate::plugin::PASSIVE_EFFECTS
            .store(prefs.passive_plugins, std::sync::atomic::Ordering::Relaxed);
        crate::pipewire::HIDE_UNNAMED_NODES.store(
            prefs.hide_unnamed_nodes,
            std::sync::atomic::Ordering::Relaxed,
        );
        log::info!(
            "Preferences: rule_settle={}ms, params_persist={}ms, links_persist={}ms, poll={}ms, auto_learn={}, pw_tick={}ms, pw_cooldown={}ms",
            prefs.rule_settle_ms,
            prefs.params_persist_ms,
            prefs.links_persist_ms,
            prefs.poll_interval_ms,
            prefs.auto_learn_rules,
            prefs.pw_tick_interval_ms,
            prefs.pw_operation_cooldown_ms,
        );
        self.as_mut().rust_mut().prefs = prefs;
        self.as_mut().restart_osc();

        let graph = GraphState::new();

        let plugin_manager = PluginManager::new();
        self.as_mut().set_plugins_scanning(true);
        match crate::plugin::scan::start() {
            Ok(rx) => self.as_mut().rust_mut().plugin_scan_rx = Some(rx),
            Err(e) => {
                self.as_mut().set_plugins_scanning(false);
                self.as_mut().error_occurred(QString::from(
                    format!("Could not start plugin discovery: {e}").as_str(),
                ));
            }
        }

        let (event_rx, cmd_tx) = crate::pipewire::start(
            graph.clone(),
            self.rust().prefs.pw_tick_interval_ms,
            self.rust().prefs.pw_operation_cooldown_ms,
        );

        let mut patchbay = PatchbayManager::new(graph.clone());
        patchbay.exclusive = self.rust().prefs.patchbay_exclusive;
        let exclusive = patchbay.exclusive;
        self.as_mut().set_patchbay_exclusive(exclusive);
        patchbay.hide_unnamed = self.rust().prefs.hide_unnamed_nodes;
        let hide_unnamed = patchbay.hide_unnamed;
        self.as_mut().set_patchbay_hide_unnamed(hide_unnamed);

        self.as_mut().rust_mut().graph = Some(graph);
        self.as_mut().rust_mut().event_rx = Some(event_rx);
        self.as_mut().rust_mut().cmd_tx = Some(cmd_tx);
        self.as_mut().rust_mut().patchbay = Some(patchbay);
        self.as_mut().rust_mut().plugin_manager = Some(plugin_manager);

        let crashed_uris_str = self.as_mut().begin_session_restore();

        let tray_state = crate::tray::spawn_tray();
        if self.rust().prefs.start_minimized {
            tray_state
                .window_visible
                .store(false, std::sync::atomic::Ordering::Release);
        }
        self.as_mut().rust_mut().tray_state = Some(tray_state);
        self.as_mut().sync_tray_flags();

        self.as_mut().notify_crash_recovery(crashed_uris_str);

        log::info!("AppController initialized successfully");
    }

    pub(super) fn begin_session_restore(mut self: Pin<&mut Self>) -> Option<String> {
        let saved_links = load_saved_links();
        if !saved_links.is_empty() {
            log::info!(
                "Loaded {} saved plugin/MIDI links for restoration",
                saved_links.len()
            );
            self.as_mut().rust_mut().pending_links =
                saved_links.into_iter().map(PendingLink::new).collect();
        }

        let saved = match load_saved_plugins() {
            Ok(saved) => saved,
            Err(()) => {
                self.as_mut().rust_mut().plugins_load_failed = true;
                Vec::new()
            }
        };
        let safe_mode = crate::SAFE_MODE.load(std::sync::atomic::Ordering::SeqCst);

        let crashed_uris_str = if !safe_mode && has_crash_marker() {
            let crashed_uris = read_crash_marker();
            log::warn!(
                "Crash marker detected! Previous plugin restore crashed. \
                 Auto-entering safe mode. Crashed URIs: {:?}",
                crashed_uris
            );
            remove_crash_marker();
            Some(crashed_uris.join(", "))
        } else {
            None
        };
        let auto_safe = crashed_uris_str.is_some();

        let skip_restore = safe_mode || auto_safe;

        let mut stable_to_instance: HashMap<String, u64> = HashMap::new();
        let mut restore_order: Vec<u64> = Vec::new();

        if skip_restore {
            log::warn!(
                "Safe mode active: skipping restoration of {} saved plugins. \
                 Plugins remain in plugins.json but will not be loaded. \
                 Restart without --safe-mode to try again.",
                saved.len()
            );
            self.as_mut().rust_mut().plugins_frozen = true;
            crate::PLUGINS_FROZEN.store(true, std::sync::atomic::Ordering::SeqCst);
        } else if !saved.is_empty() {
            log::info!("Restoring {} saved plugins", saved.len());
            let restore_uris: Vec<String> = saved.iter().map(|sp| sp.uri.clone()).collect();
            write_crash_marker(&restore_uris);
            self.as_mut().rust_mut().restore_started_at = Some(Instant::now());
            self.as_mut().rust_mut().restore_batch_size = saved.len();
            for sp in saved {
                let (instance_id, sid) = self.as_mut().spawn_saved_plugin(sp);
                self.as_mut()
                    .rust_mut()
                    .restore_instance_ids
                    .insert(instance_id);
                restore_order.push(instance_id);
                stable_to_instance.insert(sid, instance_id);
            }
        }

        if !skip_restore {
            let saved_midi = load_midi_mappings();
            if !saved_midi.is_empty() {
                let resolved =
                    resolve_saved_midi_mappings(saved_midi, &stable_to_instance, &restore_order);
                log::info!("Restoring {} saved MIDI mappings", resolved.len());
                if let Some(ref tx) = self.rust().cmd_tx {
                    for mapping in &resolved {
                        let _ = tx.send(PwCommand::AddMidiMapping(mapping.clone()));
                    }
                }
                self.as_mut().rust_mut().midi_mappings = resolved;
            }
        }
        self.as_mut().load_subgraphs();
        crashed_uris_str
    }

    pub(super) fn spawn_saved_plugin(mut self: Pin<&mut Self>, sp: SavedPlugin) -> (u64, String) {
        let instance_id = self.rust().next_instance_id;
        self.as_mut().rust_mut().next_instance_id += 1;

        let restored_params: Vec<crate::lv2::Lv2ParameterValue> =
            if let Some(ref mgr) = self.rust().plugin_manager {
                if let Some(plugin_info) = mgr.find_plugin(&sp.uri) {
                    plugin_info
                        .ports
                        .iter()
                        .filter(|port| port.port_type == crate::lv2::Lv2PortType::ControlInput)
                        .map(|port| {
                            let saved_value = sp.parameters.iter().find(|s| {
                                s.port_index == port.index
                                    || (!s.symbol.is_empty() && s.symbol == port.symbol)
                            });
                            crate::lv2::Lv2ParameterValue {
                                port_index: port.index,
                                symbol: port.symbol.clone(),
                                name: port.name.clone(),
                                value: saved_value.map(|s| s.value).unwrap_or(port.default_value),
                                min: port.min_value,
                                max: port.max_value,
                                default: port.default_value,
                                is_toggle: port.is_toggle,
                            }
                        })
                        .collect()
                } else {
                    sp.parameters
                        .iter()
                        .map(|p| crate::lv2::Lv2ParameterValue {
                            port_index: p.port_index,
                            symbol: p.symbol.clone(),
                            name: String::new(),
                            value: p.value,
                            min: 0.0,
                            max: 1.0,
                            default: 0.0,
                            is_toggle: false,
                        })
                        .collect()
                }
            } else {
                Vec::new()
            };

        let sid = if sp.stable_id.is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            sp.stable_id.clone()
        };

        let plugin_format = match sp.format.as_str() {
            "CLAP" => crate::plugin::PluginFormat::Clap,
            "VST2" => crate::plugin::PluginFormat::Vst2,
            "VST3" => crate::plugin::PluginFormat::Vst3,
            "LADSPA" => crate::plugin::PluginFormat::Ladspa,
            _ => crate::plugin::PluginFormat::Lv2,
        };

        let plugin_state = sp.decode_plugin_state();
        let scan_info = sp.usable_scan_info().or_else(|| {
            self.rust()
                .plugin_manager
                .as_ref()
                .and_then(|mgr| mgr.find_plugin_with_format(&sp.uri, plugin_format))
                .filter(|info| info.format != crate::plugin::PluginFormat::Lv2)
                .cloned()
        });

        if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
            let info = crate::lv2::Lv2InstanceInfo {
                id: instance_id,
                stable_id: sid.clone(),
                plugin_uri: sp.uri.clone(),
                format: plugin_format,
                scan_info: scan_info.clone(),
                display_name: sp.display_name.clone(),
                pw_node_id: None,
                parameters: restored_params,
                active: true,
                bypassed: sp.bypassed,
                enabled: sp.enabled,
                lv2_state: sp.lv2_state.clone(),
                plugin_state: plugin_state.clone(),
                port_names: sp.port_names.clone(),
            };
            mgr.register_instance(info);
        }

        log::info!(
            "Restoring plugin: {} ({}) [{}]",
            sp.display_name,
            sp.uri,
            sp.format
        );
        self.as_mut().send_or_defer_plugin(PwCommand::AddPlugin {
            plugin_uri: sp.uri,
            instance_id,
            display_name: sp.display_name,
            format: sp.format,
            scan_info,
            lv2_state: sp.lv2_state,
            plugin_state,
            stable_id: sid.clone(),
            enabled: sp.enabled,
        });
        (instance_id, sid)
    }

    pub(super) fn send_or_defer_plugin(mut self: Pin<&mut Self>, command: PwCommand) {
        if matches!(&command, PwCommand::AddPlugin { format, scan_info: None, .. } if format != "LV2")
            && self.rust().plugins_scanning
        {
            self.as_mut().rust_mut().pending_plugin_adds.push(command);
        } else if let Some(tx) = &self.rust().cmd_tx {
            let _ = tx.send(command);
        }
    }

    pub(super) fn poll_plugin_scan(mut self: Pin<&mut Self>) {
        use crate::plugin::scan::ScanEvent;
        use std::sync::mpsc::TryRecvError;
        loop {
            let Some(rx) = &self.rust().plugin_scan_rx else {
                return;
            };
            match rx.try_recv() {
                Ok(ScanEvent::Progress { completed, total }) => {
                    let fraction = (completed as f64 / total.max(1) as f64).min(0.99);
                    if self.rust().plugins_rescanning {
                        let progress = fraction.max(self.rust().plugin_rescan_progress);
                        self.as_mut().set_plugin_rescan_progress(progress);
                    } else {
                        let progress = fraction.max(self.rust().plugin_scan_progress);
                        self.as_mut().set_plugin_scan_progress(progress);
                    }
                }
                Ok(ScanEvent::Complete(plugins)) => {
                    if let Some(mgr) = &mut self.as_mut().rust_mut().plugin_manager {
                        mgr.set_available_plugins(plugins);
                    }
                    self.as_mut().rust_mut().plugin_scan_rx = None;
                    self.as_mut().rust_mut().last_plugin_scan = Some(Instant::now());
                    self.as_mut().set_plugin_scan_progress(1.0);
                    self.as_mut().set_plugins_scanning(false);
                    self.as_mut().set_plugin_rescan_progress(1.0);
                    self.as_mut().set_plugins_rescanning(false);
                    let pending = std::mem::take(&mut self.as_mut().rust_mut().pending_plugin_adds);
                    for mut command in pending {
                        if let PwCommand::AddPlugin {
                            instance_id,
                            scan_info,
                            ..
                        } = &mut command
                        {
                            *scan_info = self
                                .rust()
                                .plugin_manager
                                .as_ref()
                                .and_then(|mgr| mgr.get_instance(*instance_id))
                                .and_then(|info| info.scan_info.clone());
                        }
                        self.as_mut().send_or_defer_plugin(command);
                    }
                    if !self.rust().restore_instance_ids.is_empty() {
                        self.as_mut().rust_mut().restore_started_at = Some(Instant::now());
                    }
                    self.as_mut().finish_plugin_startup();
                    self.as_mut().sync_tray_plugins();
                    self.as_mut().graph_changed();
                    return;
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.as_mut().rust_mut().plugin_scan_rx = None;
                    if self.rust().plugins_rescanning {
                        self.as_mut().set_plugins_rescanning(false);
                        self.as_mut().error_occurred(QString::from(
                            "Plugin rescan stopped unexpectedly. The plugin list is unchanged.",
                        ));
                        return;
                    }
                    self.as_mut().set_plugins_scanning(false);
                    self.as_mut().error_occurred(QString::from("Plugin discovery stopped unexpectedly. Restart to try again; the saved setup has been preserved."));
                    return;
                }
            }
        }
    }

    pub(super) fn finish_plugin_startup(mut self: Pin<&mut Self>) {
        if self.rust().plugins_scanning
            || self.rust().plugin_scan_progress < 1.0
            || !self.rust().restore_instance_ids.is_empty()
            || !self.rust().pending_plugin_adds.is_empty()
        {
            return;
        }
        if let Some(mgr) = &mut self.as_mut().rust_mut().plugin_manager {
            mgr.finish_startup();
        }
        self.as_mut().forget_stale_meter_leftovers();
        persist_active_plugins(self.rust().plugin_manager.as_ref());
        self.as_mut().persist_midi();
    }

    pub(super) fn load_rules_and_default_node(mut self: Pin<&mut Self>) {
        let rules = load_rules();
        if !rules.is_empty() {
            log::info!("Loaded {} patchbay rules", rules.len());
        }
        let default_node = std::fs::read_to_string(config_path("default_node.txt"))
            .ok()
            .map(|key| key.trim().to_string())
            .filter(|key| !key.is_empty())
            .map(|key| match key.find(':') {
                Some(pos) => key[pos + 1..].to_string(),
                None => key,
            });
        if let Some(ref name) = default_node {
            log::info!("Loaded default node: {}", name);
        }
        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            patchbay.set_rules(rules);
            patchbay.rules_dirty = false;
            patchbay.set_default_target(default_node);
        }
        self.as_mut().rust_mut().docked_meters = load_docked_meters();
    }
}

impl qobject::AppController {
    pub(super) fn finish_restore_of(mut self: Pin<&mut Self>, instance_id: u64) {
        if !self
            .as_mut()
            .rust_mut()
            .restore_instance_ids
            .remove(&instance_id)
        {
            return;
        }
        if self.rust().restore_instance_ids.is_empty() {
            log::info!("All plugins restored — will attempt link restoration");
            self.as_mut().rust_mut().restore_started_at = None;
            self.as_mut().rust_mut().links_retry_due = true;
            remove_crash_marker();
            self.as_mut().finish_plugin_startup();
        }
    }
}

impl qobject::AppController {
    pub fn get_app_version(self: Pin<&mut Self>) -> QString {
        QString::from(env!("CARGO_PKG_VERSION"))
    }

    pub fn get_qt_version(self: Pin<&mut Self>) -> QString {
        QString::from(env!("QT_VERSION"))
    }
}
