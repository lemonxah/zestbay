use super::*;

impl qobject::AppController {
    pub(super) fn begin_pipewire_reconnect(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().pw_reconnecting = true;
        let lost: Vec<u64> = self
            .rust()
            .plugin_manager
            .iter()
            .flat_map(|mgr| mgr.active_instances().values())
            .filter(|info| info.pw_node_id.is_some())
            .map(|info| info.id)
            .collect();
        if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
            for id in &lost {
                if let Some(info) = mgr.get_instance_mut(*id) {
                    info.pw_node_id = None;
                }
            }
        }
        let mut rust = self.as_mut().rust_mut();
        rust.reconnect_instances.extend(lost);
        let remembered: Vec<SavedPluginLink> = rust
            .last_graph_links
            .iter()
            .filter(|l| !rust.pending_links.iter().any(|p| p.link == **l))
            .cloned()
            .collect();
        rust.pending_links
            .extend(remembered.into_iter().map(PendingLink::new));
    }

    pub fn poll_events(mut self: Pin<&mut Self>) {
        let scheme = crate::color_scheme::current();
        if scheme != self.rust().desktop_color_scheme {
            self.as_mut().set_desktop_color_scheme(scheme);
        }
        self.as_mut().poll_custom_themes();
        self.as_mut().poll_plugin_scan();
        self.as_mut().poll_osc();
        self.as_mut().poll_session_swap();
        if !self.rust().rules_loaded {
            self.as_mut().rust_mut().rules_loaded = true;
            self.as_mut().load_rules_and_default_node();
        }

        let mut changed = false;
        let mut link_changed = false;
        let mut errors: Vec<String> = Vec::new();
        let mut plugin_events: Vec<PluginEvent> = Vec::new();

        let has_events = self.rust().event_rx.is_some();
        if has_events {
            let rx = self.as_mut().rust_mut().event_rx.take();
            if let Some(rx) = rx {
                let mut stashed = std::mem::take(&mut self.as_mut().rust_mut().stashed_events);
                while let Some(event) = stashed.pop_front().or_else(|| rx.try_recv().ok()) {
                    match event {
                        PwEvent::NodeChanged(_) => {
                            changed = true;
                            if self.rust().pw_reconnecting {
                                self.as_mut().rust_mut().pw_reconnecting = false;
                                self.as_mut()
                                    .suppress_link_persist(LINK_SETTLE_AFTER_RECONNECT);
                            }
                        }
                        PwEvent::NodeRemoved(_)
                        | PwEvent::PortChanged(_)
                        | PwEvent::PortRemoved { .. }
                        | PwEvent::BatchComplete => {
                            changed = true;
                        }
                        PwEvent::LinkChanged(_) | PwEvent::LinkRemoved(_) => {
                            changed = true;
                            link_changed = true;
                        }
                        PwEvent::Error(msg) => {
                            log::error!("PipeWire error: {}", msg);
                            if msg.contains("reconnecting to PipeWire") {
                                self.as_mut().begin_pipewire_reconnect();
                            }
                            if !errors.contains(&msg) {
                                errors.push(msg);
                            }
                        }
                        PwEvent::Plugin(plugin_event) => {
                            plugin_events.push(plugin_event);
                        }
                    }
                }
                self.as_mut().rust_mut().event_rx = Some(rx);
            }
        }

        let mut param_updates: Vec<(u64, u32, f32)> = Vec::new();

        for event in plugin_events {
            match event {
                PluginEvent::PluginAdded {
                    instance_id,
                    pw_node_id,
                    display_name,
                } => {
                    changed = true;
                    log::info!(
                        "LV2 plugin added: instance={} pw_node={} name={}",
                        instance_id,
                        pw_node_id,
                        display_name
                    );
                    let reconnected = self
                        .as_mut()
                        .rust_mut()
                        .reconnect_instances
                        .remove(&instance_id);
                    let re_added = reconnected
                        || self
                            .rust()
                            .plugin_manager
                            .as_ref()
                            .and_then(|mgr| mgr.get_instance(instance_id))
                            .is_some_and(|info| info.pw_node_id.is_some());
                    if re_added {
                        log::info!(
                            "Plugin instance {} re-added (reconnect/reload)",
                            instance_id
                        );
                        self.as_mut()
                            .suppress_link_persist(LINK_SETTLE_AFTER_RECONNECT);
                    }
                    if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
                        mgr.set_instance_pw_node_id(instance_id, pw_node_id);
                        if let Some(info) = mgr.get_instance_mut(instance_id) {
                            info.active = true;
                        }
                    }
                    if pw_node_id != 0
                        && pw_node_id != u32::MAX
                        && let Some(ref graph) = self.rust().graph
                    {
                        graph.set_node_type(pw_node_id, NodeType::Plugin);
                        if let Some(name) = self
                            .rust()
                            .plugin_manager
                            .as_ref()
                            .and_then(|mgr| mgr.get_instance(instance_id))
                            .map(|info| info.display_name.clone())
                        {
                            graph.set_node_description(pw_node_id, &name);
                        }
                    }
                    self.as_mut()
                        .rust_mut()
                        .forgotten_link_nodes
                        .remove(&display_name);

                    if !re_added
                        && let Some(ref mgr) = self.rust().plugin_manager
                        && let Some(info) = mgr.get_instance(instance_id)
                        && (!info.parameters.is_empty() || info.bypassed)
                        && let Some(ref tx) = self.rust().cmd_tx
                    {
                        for param in &info.parameters {
                            let _ = tx.send(PwCommand::SetPluginParameter {
                                instance_id,
                                port_index: param.port_index,
                                value: param.value,
                            });
                        }
                        if info.bypassed {
                            let _ = tx.send(PwCommand::SetPluginBypass {
                                instance_id,
                                bypassed: true,
                            });
                        }
                        log::info!(
                            "Restored {} params + bypass={} for instance {}",
                            info.parameters.len(),
                            info.bypassed,
                            instance_id
                        );
                    }

                    self.as_mut().finish_restore_of(instance_id);
                }
                PluginEvent::PluginRemoved { instance_id } => {
                    changed = true;
                    log::info!("LV2 plugin removed: instance={}", instance_id);
                    let removed_sid = self
                        .rust()
                        .plugin_manager
                        .as_ref()
                        .and_then(|mgr| mgr.get_instance(instance_id))
                        .map(|info| info.stable_id.clone());
                    if let Some(sid) = removed_sid {
                        self.as_mut().drop_subgraph_member(&sid);
                    }
                    let name = self
                        .rust()
                        .plugin_manager
                        .as_ref()
                        .and_then(|mgr| mgr.get_instance(instance_id))
                        .map(|info| info.display_name.clone());
                    if let Some(name) = name {
                        self.as_mut().forget_plugin_links(&name);
                    }
                    if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
                        mgr.remove_instance(instance_id);
                    }
                    self.as_mut().drop_plugin_midi_mappings(instance_id);
                    persist_active_plugins(self.rust().plugin_manager.as_ref());
                    self.as_mut().mark_links_dirty();
                }
                PluginEvent::ParameterChanged {
                    instance_id,
                    port_index,
                    value,
                } => {
                    if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
                        mgr.update_parameter(instance_id, port_index, value);
                    }
                    self.as_mut().rust_mut().params_dirty = true;
                    if self.rust().params_dirty_since.is_none() {
                        self.as_mut().rust_mut().params_dirty_since = Some(Instant::now());
                    }
                    let port = port_index as u32;
                    if let Some(entry) = param_updates
                        .iter_mut()
                        .find(|(i, p, _)| *i == instance_id && *p == port)
                    {
                        entry.2 = value;
                    } else {
                        param_updates.push((instance_id, port, value));
                    }
                }
                PluginEvent::ParametersDescribed {
                    instance_id,
                    params,
                } => {
                    changed = true;
                    if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
                        mgr.describe_parameters(instance_id, params);
                    }
                }
                PluginEvent::PluginSkipped { instance_id } => {
                    changed = true;
                    log::warn!("Plugin instance {} skipped for this session", instance_id);
                    if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager
                        && let Some(info) = mgr.get_instance_mut(instance_id)
                    {
                        info.active = false;
                        info.pw_node_id = None;
                    }
                    self.as_mut().finish_restore_of(instance_id);
                }
                PluginEvent::PluginUiOpened { instance_id } => {
                    log::info!("LV2 plugin UI opened: instance={}", instance_id);
                }
                PluginEvent::PluginUiClosed { instance_id } => {
                    log::info!("LV2 plugin UI closed: instance={}", instance_id);
                }
                PluginEvent::PluginError {
                    instance_id,
                    message,
                    fatal,
                } => {
                    log::error!(
                        "LV2 plugin error: instance={:?} fatal={} msg={}",
                        instance_id,
                        fatal,
                        message
                    );

                    if let Some(id) = instance_id {
                        let plugin_name = self
                            .rust()
                            .plugin_manager
                            .as_ref()
                            .and_then(|mgr| mgr.get_instance(id))
                            .map(|info| info.display_name.clone());

                        if fatal {
                            changed = true;
                            if let Some(ref name) = plugin_name {
                                self.as_mut().forget_plugin_links(name);
                            }
                            let sid = self
                                .rust()
                                .plugin_manager
                                .as_ref()
                                .and_then(|mgr| mgr.get_instance(id))
                                .map(|info| info.stable_id.clone());
                            if let Some(sid) = sid {
                                self.as_mut().drop_subgraph_member(&sid);
                            }
                            if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
                                mgr.remove_instance(id);
                            }
                            self.as_mut().drop_plugin_midi_mappings(id);
                            persist_active_plugins(self.rust().plugin_manager.as_ref());
                            self.as_mut().finish_restore_of(id);
                        }

                        errors.push(match (plugin_name, fatal) {
                            (Some(name), true) => {
                                format!("Plugin \"{}\" failed to load: {}", name, message)
                            }
                            (Some(name), false) => format!("Plugin \"{}\": {}", name, message),
                            (None, true) => format!("Plugin failed to load: {}", message),
                            (None, false) => message,
                        });
                    } else {
                        errors.push(message);
                    }
                }
                PluginEvent::MidiLearnStarted {
                    instance_id,
                    port_index,
                } => {
                    log::info!(
                        "MIDI learn started: instance={} port={}",
                        instance_id,
                        port_index
                    );
                    self.as_mut()
                        .midi_learn_started(instance_id, port_index as u32);
                }
                PluginEvent::MidiLearnCancelled => {
                    log::info!("MIDI learn cancelled");
                    self.as_mut().midi_learn_cancelled();
                }
                PluginEvent::MidiMappingAdded(ref mapping) => {
                    log::info!(
                        "MIDI mapping added: {:?} -> {:?}",
                        mapping.source,
                        mapping.target
                    );
                    let replaced: Vec<crate::midi::MidiCcTarget> = self
                        .rust()
                        .midi_mappings
                        .iter()
                        .filter(|m| m.source == mapping.source && m.target != mapping.target)
                        .map(|m| m.target.clone())
                        .collect();
                    self.as_mut()
                        .rust_mut()
                        .midi_mappings
                        .retain(|m| m.target != mapping.target && m.source != mapping.source);
                    self.as_mut().rust_mut().midi_mappings.push(mapping.clone());
                    if self
                        .rust()
                        .last_learned_mapping
                        .as_ref()
                        .is_some_and(|m| m.source == mapping.source)
                    {
                        self.as_mut().rust_mut().last_learned_mapping = None;
                    }
                    self.as_mut().persist_midi();
                    for t in replaced {
                        self.as_mut()
                            .midi_mapping_removed(t.instance_id, t.port_index as u32);
                    }
                    let json = serde_json::to_string(mapping).unwrap_or_else(|_| "{}".to_string());
                    self.as_mut().midi_mapping_added(QString::from(&json));
                }
                PluginEvent::MidiMappingRemoved(ref source) => {
                    log::info!("MIDI mapping removed: {:?}", source);
                    let removed: Vec<(u64, u32)> = self
                        .rust()
                        .midi_mappings
                        .iter()
                        .filter(|m| m.source == *source)
                        .map(|m| (m.target.instance_id, m.target.port_index as u32))
                        .collect();
                    let device_gone = self.rust().graph.as_ref().is_some_and(|g| {
                        !node_name_present(g, &g.get_all_nodes(), &source.device_name)
                    });
                    if device_gone {
                        let gone: Vec<crate::midi::MidiCcMapping> = self
                            .rust()
                            .midi_mappings
                            .iter()
                            .filter(|m| m.source == *source)
                            .cloned()
                            .collect();
                        let mut rust = self.as_mut().rust_mut();
                        rust.dormant_midi_mappings.retain(|m| m.source != *source);
                        rust.dormant_midi_mappings.extend(gone);
                    }
                    self.as_mut()
                        .rust_mut()
                        .midi_mappings
                        .retain(|m| m.source != *source);
                    self.as_mut().persist_midi();
                    for (iid, pidx) in removed {
                        self.as_mut().midi_mapping_removed(iid, pidx);
                    }
                }
                PluginEvent::MidiMappingConflict {
                    ref source,
                    ref existing_label,
                } => {
                    log::warn!(
                        "MIDI mapping conflict: {:?} already assigned to '{}'",
                        source,
                        existing_label
                    );
                    let learned = self.as_mut().rust_mut().last_learned_mapping.take();
                    self.as_mut().rust_mut().pending_conflict_mapping =
                        learned.filter(|m| m.source == *source);
                    let source_json =
                        serde_json::to_string(source).unwrap_or_else(|_| "{}".to_string());
                    self.as_mut().midi_mapping_conflict(
                        QString::from(&source_json),
                        QString::from(existing_label.as_str()),
                    );
                }
                PluginEvent::PluginStatesDone => {
                    self.as_mut().rust_mut().state_request_sent_at = None;
                    persist_active_plugins(self.rust().plugin_manager.as_ref());
                }
                PluginEvent::PluginStateSaved {
                    instance_id,
                    lv2_state,
                    blob,
                } => {
                    self.as_mut()
                        .apply_plugin_state(instance_id, lv2_state, blob);
                }
                PluginEvent::MidiCcReceived {
                    ref device_name,
                    channel,
                    cc,
                    message_type,
                } => {
                    if let Some((instance_id, port_index, label, mode)) =
                        self.as_mut().rust_mut().midi_learn_target.take()
                    {
                        let mapping = crate::midi::MidiCcMapping {
                            source: crate::midi::MidiCcSource {
                                device_name: device_name.clone(),
                                channel: Some(channel),
                                cc,
                                message_type,
                            },
                            target: crate::midi::MidiCcTarget {
                                instance_id,
                                port_index,
                            },
                            mode,
                            label,
                        };
                        self.as_mut().rust_mut().last_learned_mapping = Some(mapping.clone());
                        if let Some(ref tx) = self.rust().cmd_tx {
                            let _ = tx.send(PwCommand::CancelMidiLearn);
                            let _ = tx.send(PwCommand::AddMidiMapping(mapping));
                        }
                    }
                }
            }
        }

        if let Some(ref graph) = self.rust().graph {
            let current = graph.change_counter();
            let last = self.rust().last_change_counter;
            if current != last {
                changed = true;
                self.as_mut().rust_mut().last_change_counter = current;
            }
        }

        if changed {
            self.as_mut().rust_mut().last_change_time = Some(Instant::now());
            self.as_mut().rust_mut().rules_apply_pending = true;
            self.as_mut().rust_mut().links_retry_due = true;
            self.as_mut().revive_dormant_midi_mappings();
        }

        if link_changed && self.rust().restore_instance_ids.is_empty() {
            self.as_mut().mark_links_dirty();
        }

        let rule_settle_ms = self.rust().prefs.rule_settle_ms;
        let should_apply = {
            let pending = self.rust().rules_apply_pending;
            let patchbay_enabled = self
                .rust()
                .patchbay
                .as_ref()
                .map(|p| p.enabled)
                .unwrap_or(false);
            let settled = self
                .rust()
                .last_change_time
                .map(|t| t.elapsed() >= Duration::from_millis(rule_settle_ms))
                .unwrap_or(false);
            pending && patchbay_enabled && settled
        };

        if should_apply {
            self.as_mut().rust_mut().rules_apply_pending = false;
            self.as_mut().refresh_meter_redirects();
            let commands = if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
                patchbay.scan()
            } else {
                Vec::new()
            };
            if !commands.is_empty() {
                log::info!("Auto-applying {} patchbay rule commands", commands.len());
                if let Some(ref tx) = self.rust().cmd_tx {
                    for cmd in commands {
                        let _ = tx.send(cmd);
                    }
                }
            }
            if self
                .rust()
                .patchbay
                .as_ref()
                .map(|p| p.rules_dirty)
                .unwrap_or(false)
            {
                if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
                    patchbay.rules_dirty = false;
                }
                save_rules(self.rust().patchbay.as_ref());
            }
        }

        let params_persist_ms = self.rust().prefs.params_persist_ms;
        let should_persist_params = {
            self.rust().params_dirty
                && self
                    .rust()
                    .params_dirty_since
                    .map(|t| t.elapsed() >= Duration::from_millis(params_persist_ms))
                    .unwrap_or(false)
        };
        if should_persist_params {
            self.as_mut().rust_mut().params_dirty = false;
            self.as_mut().rust_mut().params_dirty_since = None;
            persist_active_plugins(self.rust().plugin_manager.as_ref());
        }

        let should_retry_links = {
            let settled = self
                .rust()
                .last_change_time
                .map(|t| t.elapsed() >= Duration::from_millis(rule_settle_ms))
                .unwrap_or(false);
            let expired = self.rust().pending_links.iter().any(|p| {
                p.connect_sent_at
                    .is_some_and(|t| t.elapsed() >= LINK_CONNECT_GRACE)
            });
            self.rust().restore_instance_ids.is_empty()
                && !self.rust().pending_links.is_empty()
                && ((self.rust().links_retry_due && settled) || expired)
        };
        if should_retry_links {
            self.as_mut().rust_mut().links_retry_due = false;
            self.as_mut().retry_pending_links();
        }
        if !self.rust().subgraphs.jobs.is_empty() {
            self.as_mut().process_subgraph_jobs();
        }
        if self
            .rust()
            .subgraphs
            .vdev_start_at
            .is_some_and(|t| Instant::now() >= t)
        {
            self.as_mut().start_virtual_devices();
        }

        let links_persist_ms = self.rust().prefs.links_persist_ms;
        let should_persist_links = {
            self.rust().links_dirty
                && self
                    .rust()
                    .links_dirty_since
                    .map(|t| t.elapsed() >= Duration::from_millis(links_persist_ms))
                    .unwrap_or(false)
        };
        if should_persist_links && !self.links_persist_suppressed() {
            self.as_mut().rust_mut().links_dirty = false;
            self.as_mut().rust_mut().links_dirty_since = None;
            self.as_mut().persist_links_now();
        }

        let tray_state = self.rust().tray_state.clone();
        if let Some(ref tray) = tray_state {
            use std::sync::atomic::Ordering;
            if tray.quit_requested.load(Ordering::Acquire) {
                self.as_mut().request_quit();
                return;
            }
            if tray.show_requested.swap(false, Ordering::AcqRel) {
                log::info!("Tray: show window requested — emitting signal to QML");
                tray.window_visible.store(true, Ordering::Release);
                self.as_mut().show_window_requested();
                tray.refresh();
            }
            if tray.hide_requested.swap(false, Ordering::AcqRel) {
                log::info!("Tray: hide window requested — emitting signal to QML");
                tray.window_visible.store(false, Ordering::Release);
                self.as_mut().hide_window_requested();
                tray.refresh();
            }

            if let Ok(mut req) = tray.open_plugin_ui.lock()
                && let Some(node_id) = req.take()
            {
                log::info!("Tray: open plugin UI requested for node_id={}", node_id);
                self.as_mut().open_plugin_ui(node_id);
            }

            for action in tray.take_actions() {
                use crate::tray::TrayAction;
                let ui_action = match action {
                    TrayAction::AddPlugin => Some("add_plugin"),
                    TrayAction::Rules => Some("rules"),
                    TrayAction::Preferences => Some("preferences"),
                    TrayAction::ToggleRules => {
                        let on = !self.rust().patchbay_enabled;
                        self.as_mut().toggle_patchbay(on);
                        None
                    }
                    TrayAction::ToggleExclusive => {
                        let on = !self.rust().patchbay_exclusive;
                        self.as_mut().toggle_patchbay_exclusive(on);
                        None
                    }
                };
                if let Some(name) = ui_action {
                    tray.window_visible.store(true, Ordering::Release);
                    self.as_mut().show_window_requested();
                    self.as_mut().tray_action_requested(QString::from(name));
                    tray.refresh();
                }
            }
        }

        errors.extend(take_config_errors());
        if !errors.is_empty() {
            let qmsg = QString::from(&errors.join("\n\n"));
            self.as_mut().error_occurred(qmsg);
        }

        if !self.rust().plugins_scanning
            && !self.rust().restore_instance_ids.is_empty()
            && let Some(started) = self.rust().restore_started_at
        {
            let cap = restore_safety_cap(self.rust().restore_batch_size);
            if started.elapsed() > cap {
                let remaining = self.rust().restore_instance_ids.len();
                log::error!(
                    "Plugin restore timed out after {}s with {} plugin(s) still pending. \
                     Clearing restore state.",
                    cap.as_secs(),
                    remaining,
                );
                self.as_mut().rust_mut().restore_instance_ids.clear();
                self.as_mut().rust_mut().restore_started_at = None;
                self.as_mut().rust_mut().links_retry_due = true;
                remove_crash_marker();
                self.as_mut().finish_plugin_startup();
            }
        }

        self.as_mut().maybe_autosave_plugin_states();

        if changed {
            self.as_mut().refresh_cache();
            self.as_mut().sync_tray_plugins();
            self.as_mut().graph_changed();
        }

        for (instance_id, port_index, value) in param_updates {
            self.as_mut()
                .plugin_parameter_changed(instance_id, port_index, value);
        }

        let mut prev_ticks = self.rust().prev_cpu_ticks;
        let mut prev_time = self.rust().prev_cpu_time;
        let mut avg = self.rust().cpu_avg;
        let cpu_str = measure_cpu_usage(&mut prev_ticks, &mut prev_time, &mut avg);
        self.as_mut().rust_mut().prev_cpu_ticks = prev_ticks;
        self.as_mut().rust_mut().prev_cpu_time = prev_time;
        self.as_mut().rust_mut().cpu_avg = avg;
        {
            let h = &mut self.as_mut().rust_mut().cpu_history;
            if h.len() >= 120 {
                h.remove(0);
            }
            h.push(avg);
        }
        self.as_mut().set_cpu_usage(QString::from(&cpu_str));
    }
}
