use super::*;

impl qobject::AppController {
    pub fn restart_pipewire(self: Pin<&mut Self>) {
        std::thread::spawn(|| {
            let mut cmd = std::process::Command::new("systemctl");
            cmd.args([
                "--user",
                "restart",
                "pipewire.service",
                "pipewire-pulse.service",
                "wireplumber.service",
            ]);
            log::warn!("Restarting PipeWire (user request)");
            match cmd.output() {
                Ok(out) if out.status.success() => log::info!("PipeWire restarted"),
                Ok(out) => push_config_error(format!(
                    "Restarting PipeWire failed ({}): {}",
                    out.status,
                    String::from_utf8_lossy(&out.stderr).trim()
                )),
                Err(e) => push_config_error(format!(
                    "Could not run systemctl to restart PipeWire: {}",
                    e
                )),
            }
        });
    }

    pub fn get_sessions_json(self: Pin<&mut Self>) -> QString {
        QString::from(
            &serde_json::json!({
                "current": crate::session::current_name().unwrap_or_default(),
                "path": crate::session::dir().to_string_lossy(),
                "sessions": crate::session::list(),
            })
            .to_string(),
        )
    }

    pub fn create_session(self: Pin<&mut Self>, name: QString) -> QString {
        match crate::session::create(name.to_string().trim()) {
            Ok(()) => QString::from(""),
            Err(e) => QString::from(&e),
        }
    }

    pub fn delete_session(self: Pin<&mut Self>, name: QString) -> QString {
        match crate::session::delete(&name.to_string()) {
            Ok(()) => QString::from(""),
            Err(e) => QString::from(&e),
        }
    }

    pub fn switch_session(mut self: Pin<&mut Self>, name: QString) -> QString {
        match self.as_mut().begin_session_swap(&name.to_string()) {
            Ok(()) => QString::from(""),
            Err(e) => QString::from(&e),
        }
    }

    pub(super) fn begin_session_swap(mut self: Pin<&mut Self>, name: &str) -> Result<(), String> {
        if self.rust().session_swap.is_some() {
            return Err("A session switch is already in progress.".into());
        }
        if !crate::session::list().iter().any(|n| n == name) {
            return Err(format!("There is no session called \"{name}\"."));
        }
        if crate::session::current_name().as_deref() == Some(name) {
            return Err(format!("\"{name}\" is already open."));
        }
        let ready = !self.rust().plugins_scanning
            && self.rust().plugin_scan_progress >= 1.0
            && self.rust().restore_instance_ids.is_empty()
            && self.rust().pending_plugin_adds.is_empty()
            && self
                .rust()
                .plugin_manager
                .as_ref()
                .is_some_and(|mgr| mgr.startup_complete());
        if !ready {
            return Err("ZestBay is still starting up. Try again in a moment.".into());
        }
        log::info!("Switching to session \"{name}\"");
        self.as_mut().session_changing();
        self.as_mut().save_session_now();

        crate::session::suspend_writes(true);

        let (owned_keys, running, idle): (Vec<String>, Vec<u64>, Vec<u64>) = {
            let sgs = &self.rust().subgraphs;
            let mut keys: Vec<String> = sgs
                .defs
                .iter()
                .flat_map(|d| d.node_specs().into_iter().map(|(k, _)| k))
                .collect();
            keys.extend(sgs.vdevs.iter().map(|v| v.key()));
            let (mut running, mut idle) = (Vec::new(), Vec::new());
            if let Some(mgr) = self.rust().plugin_manager.as_ref() {
                for (id, info) in mgr.active_instances() {
                    if info.active && info.pw_node_id.is_some() {
                        running.push(*id);
                    } else {
                        idle.push(*id);
                    }
                }
            }
            (keys, running, idle)
        };
        for key in owned_keys {
            self.send_owned(key, None);
        }
        if let Some(ref tx) = self.rust().cmd_tx {
            for instance_id in &running {
                let _ = tx.send(PwCommand::RemovePlugin {
                    instance_id: *instance_id,
                });
            }
        }
        for instance_id in idle {
            if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
                mgr.remove_instance(instance_id);
            }
            self.as_mut().drop_plugin_midi_mappings(instance_id);
        }
        self.as_mut().rust_mut().session_swap = Some(SessionSwap {
            target: name.to_string(),
            started: Instant::now(),
        });
        Ok(())
    }

    pub(super) fn poll_session_swap(mut self: Pin<&mut Self>) {
        const GIVE_UP_AFTER: Duration = Duration::from_secs(8);
        let Some(swap) = self.rust().session_swap.as_ref() else {
            return;
        };
        let drained = self
            .rust()
            .plugin_manager
            .as_ref()
            .is_none_or(|mgr| mgr.active_instances().is_empty());
        if !drained && swap.started.elapsed() < GIVE_UP_AFTER {
            return;
        }
        if !drained {
            log::warn!("Old plugins did not all stop in time; switching session anyway");
        }
        let target = swap.target.clone();
        self.as_mut().finish_session_swap(&target);
    }

    pub(super) fn finish_session_swap(mut self: Pin<&mut Self>, target: &str) {
        self.as_mut().rust_mut().session_swap = None;
        if let Err(e) = crate::session::activate(target) {
            crate::session::suspend_writes(false);
            self.as_mut().error_occurred(QString::from(&e));
            return;
        }

        crate::PLUGINS_FROZEN.store(false, std::sync::atomic::Ordering::SeqCst);
        list_clear(&LOAD_FAILED);
        list_clear(&SAVE_BLOCKED);
        {
            let mut rust = self.as_mut().rust_mut();
            rust.plugins_frozen = false;
            rust.plugins_load_failed = false;
            rust.restore_instance_ids.clear();
            rust.restore_started_at = None;
            rust.restore_batch_size = 0;
            rust.pending_links.clear();
            rust.last_graph_links.clear();
            rust.forgotten_link_nodes.clear();
            rust.links_retry_due = false;
            rust.links_dirty = false;
            rust.links_dirty_since = None;
            rust.links_suppressed_until = None;
            rust.params_dirty = false;
            rust.params_dirty_since = None;
            rust.preset_state_refresh.clear();
            rust.midi_mappings.clear();
            rust.dormant_midi_mappings.clear();
            rust.midi_learn_target = None;
            rust.last_learned_mapping = None;
            rust.pending_conflict_mapping = None;
            rust.state_request_sent_at = None;
            rust.last_state_autosave = Instant::now();
            rust.subgraphs = Subgraphs::default();
            if let Some(ref mut mgr) = rust.plugin_manager {
                mgr.reset_for_new_session();
            }
        }
        if let Some(ref tx) = self.rust().cmd_tx {
            let _ = tx.send(PwCommand::CancelMidiLearn);
        }
        crate::session::suspend_writes(false);

        self.as_mut().load_rules_and_default_node();
        self.as_mut().rust_mut().rules_loaded = true;
        let crashed = self.as_mut().begin_session_restore();
        self.as_mut().notify_crash_recovery(crashed);
        self.as_mut().rust_mut().links_retry_due = true;
        self.as_mut().finish_plugin_startup();
        self.as_mut().refresh_cache();
        self.as_mut().sync_tray_plugins();
        self.as_mut().subgraphs_changed();
        self.as_mut().session_changed();
        self.as_mut().graph_changed();
        log::info!("Session \"{target}\" is open");
    }

    pub(super) fn notify_crash_recovery(mut self: Pin<&mut Self>, crashed: Option<String>) {
        let Some(uris) = crashed else { return };
        if has_known_good_plugins() {
            self.as_mut()
                .crash_recovery_available(QString::from(uris.as_str()));
        } else {
            let msg = format!(
                "Crash detected during plugin restore. No known-good snapshot available.\n\
                 Crashed plugins: {}\n\
                 Restart without --safe-mode to try again, or manually edit plugins.json.",
                uris
            );
            self.as_mut().error_occurred(QString::from(msg.as_str()));
        }
    }

    pub(super) fn save_session_now(mut self: Pin<&mut Self>) {
        let frozen = crate::PLUGINS_FROZEN.load(std::sync::atomic::Ordering::SeqCst);
        if self.plugins_persistable() {
            self.as_mut()
                .collect_plugin_states_blocking(Duration::from_secs(3));
        }
        let restore_complete = self.rust().restore_instance_ids.is_empty()
            && self
                .rust()
                .plugin_manager
                .as_ref()
                .is_some_and(|mgr| mgr.startup_complete());
        remove_crash_marker();
        if !self.links_persist_suppressed() {
            self.as_mut().persist_links_now();
        }
        persist_active_plugins(self.rust().plugin_manager.as_ref());
        self.as_mut().persist_midi();
        self.persist_subgraphs();
        if !frozen && !self.rust().plugins_load_failed && restore_complete {
            save_known_good_plugins();
        }
    }

    pub fn request_quit(mut self: Pin<&mut Self>) {
        log::info!("Quit requested");
        self.as_mut().save_session_now();
        crate::lv2::ui::shutdown_gtk_thread();
        crate::exit_now(0);
    }

    pub fn restore_known_good(self: Pin<&mut Self>) -> bool {
        if restore_known_good_plugins() {
            log::info!("Known-good plugins restored. Restart to load them.");
            true
        } else {
            false
        }
    }
}
