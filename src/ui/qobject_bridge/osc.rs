use super::*;

impl qobject::AppController {
    pub(super) fn restart_osc(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().osc = None;
        let (enabled, port, all) = {
            let p = &self.rust().prefs;
            (p.osc_enabled, p.osc_port, p.osc_listen_all)
        };
        if !enabled {
            return;
        }
        match crate::osc::OscServer::start(all, port) {
            Ok(server) => self.as_mut().rust_mut().osc = Some(server),
            Err(e) => {
                log::error!("OSC could not listen on port {port}: {e}");
                self.as_mut().error_occurred(QString::from(
                    format!("OSC could not listen on port {port}: {e}").as_str(),
                ));
            }
        }
    }

    pub(super) fn find_osc_plugin(&self, key: &str) -> Option<u64> {
        let want = crate::osc::slug(key);
        self.rust()
            .plugin_manager
            .as_ref()?
            .active_instances()
            .iter()
            .filter(|(_, i)| i.stable_id == key || crate::osc::slug(&i.display_name) == want)
            .map(|(id, _)| *id)
            .min()
    }

    pub(super) fn poll_osc(mut self: Pin<&mut Self>) {
        let requests = match self.rust().osc.as_ref() {
            Some(server) => server.drain(),
            None => return,
        };
        if requests.is_empty() {
            return;
        }
        let mut changed = false;
        for req in requests {
            changed |= self.as_mut().handle_osc(req);
        }
        if changed {
            self.as_mut().graph_changed();
        }
    }

    pub(super) fn handle_osc(mut self: Pin<&mut Self>, req: crate::osc::OscRequest) -> bool {
        use crate::osc::{OscAction, slug};
        use rosc::OscType;

        let reply = |this: &Self, addr: &str, args: Vec<OscType>| {
            if let Some(server) = this.rust().osc.as_ref() {
                server.reply(req.from, addr, args);
            }
        };
        if self.rust().session_swap.is_some() && !matches!(req.action, OscAction::ListSessions) {
            log::debug!("OSC: ignored while the session is switching");
            return false;
        }
        match req.action {
            OscAction::ListSessions => {
                let current = crate::session::current_name();
                for name in crate::session::list() {
                    let is_current = current.as_deref() == Some(name.as_str());
                    reply(
                        &self,
                        "/zestbay/session",
                        vec![OscType::String(name), OscType::Int(is_current as i32)],
                    );
                }
                false
            }
            OscAction::LoadSession { name } => {
                if let Err(e) = self.as_mut().begin_session_swap(&name) {
                    log::warn!("OSC: could not load session \"{name}\": {e}");
                    reply(&self, "/zestbay/error", vec![OscType::String(e)]);
                }
                false
            }
            OscAction::ListPlugins => {
                let mut rows: Vec<(u64, String, String, bool, bool)> = self
                    .rust()
                    .plugin_manager
                    .iter()
                    .flat_map(|m| m.active_instances().iter())
                    .map(|(id, i)| {
                        (
                            *id,
                            slug(&i.display_name),
                            i.display_name.clone(),
                            i.enabled,
                            i.bypassed,
                        )
                    })
                    .collect();
                rows.sort();
                for (_, key, name, enabled, bypassed) in rows {
                    reply(
                        &self,
                        "/zestbay/plugin",
                        vec![
                            OscType::String(key),
                            OscType::String(name),
                            OscType::Int(enabled as i32),
                            OscType::Int(bypassed as i32),
                        ],
                    );
                }
                false
            }
            OscAction::ListParams { plugin } => {
                let Some(id) = self.find_osc_plugin(&plugin) else {
                    log::debug!("OSC: no plugin \"{plugin}\"");
                    return false;
                };
                let rows: Vec<(String, crate::plugin::ParameterValue)> = self
                    .rust()
                    .plugin_manager
                    .as_ref()
                    .and_then(|m| m.get_instance(id))
                    .map(|i| (slug(&i.display_name), i.parameters.clone()))
                    .map(|(key, params)| params.into_iter().map(|p| (key.clone(), p)).collect())
                    .unwrap_or_default();
                for (key, p) in rows {
                    reply(
                        &self,
                        "/zestbay/param",
                        vec![
                            OscType::String(key),
                            OscType::String(p.symbol),
                            OscType::String(p.name),
                            OscType::Float(p.value),
                            OscType::Float(p.min),
                            OscType::Float(p.max),
                        ],
                    );
                }
                false
            }
            OscAction::Enable { plugin, enabled } => {
                let Some(id) = self.find_osc_plugin(&plugin) else {
                    log::debug!("OSC: no plugin \"{plugin}\"");
                    return false;
                };
                self.as_mut().set_instance_enabled(id, enabled);
                true
            }
            OscAction::Bypass { plugin, bypassed } => {
                let Some(id) = self.find_osc_plugin(&plugin) else {
                    log::debug!("OSC: no plugin \"{plugin}\"");
                    return false;
                };
                self.as_mut().set_instance_bypass(id, bypassed);
                true
            }
            OscAction::Toggle { plugin } => {
                let Some(id) = self.find_osc_plugin(&plugin) else {
                    log::debug!("OSC: no plugin \"{plugin}\"");
                    return false;
                };
                let enabled = self
                    .rust()
                    .plugin_manager
                    .as_ref()
                    .and_then(|m| m.get_instance(id))
                    .is_none_or(|i| i.enabled);
                self.as_mut().set_instance_enabled(id, !enabled);
                true
            }
            OscAction::Param {
                plugin,
                param,
                value,
                normalized,
            } => {
                let Some(id) = self.find_osc_plugin(&plugin) else {
                    log::debug!("OSC: no plugin \"{plugin}\"");
                    return false;
                };
                let target = self
                    .rust()
                    .plugin_manager
                    .as_ref()
                    .and_then(|m| m.get_instance(id))
                    .and_then(|i| {
                        let want = slug(&param);
                        i.parameters
                            .iter()
                            .find(|p| p.symbol.eq_ignore_ascii_case(&param))
                            .or_else(|| {
                                i.parameters
                                    .iter()
                                    .find(|p| !p.name.is_empty() && slug(&p.name) == want)
                            })
                            .or_else(|| {
                                let index = param.parse::<usize>().ok()?;
                                i.parameters.iter().find(|p| p.port_index == index)
                            })
                            .cloned()
                    });
                let Some(p) = target else {
                    log::debug!("OSC: plugin \"{plugin}\" has no parameter \"{param}\"");
                    return false;
                };
                let value = if normalized {
                    p.min + value.clamp(0.0, 1.0) * (p.max - p.min)
                } else {
                    value.max(p.min).min(p.max)
                };
                self.as_mut()
                    .set_instance_parameter(id, p.port_index, value);
                true
            }
            OscAction::Preset { plugin, name } => {
                let node_id = self.find_osc_plugin(&plugin).and_then(|id| {
                    self.rust()
                        .plugin_manager
                        .as_ref()?
                        .get_instance(id)?
                        .pw_node_id
                });
                let Some(node_id) = node_id else {
                    log::debug!("OSC: no running plugin \"{plugin}\"");
                    return false;
                };
                if !self
                    .as_mut()
                    .apply_plugin_preset(node_id, QString::from(&name))
                {
                    log::debug!("OSC: plugin \"{plugin}\" has no preset \"{name}\"");
                    return false;
                }
                true
            }
        }
    }
}
