use super::*;

impl qobject::AppController {
    pub(super) fn persist_midi(self: Pin<&mut Self>) {
        let mut all = self.rust().midi_mappings.clone();
        for m in &self.rust().dormant_midi_mappings {
            if !all
                .iter()
                .any(|a| a.source == m.source || a.target == m.target)
            {
                all.push(m.clone());
            }
        }
        persist_midi_mappings(&all, self.rust().plugin_manager.as_ref());
    }

    pub(super) fn revive_dormant_midi_mappings(mut self: Pin<&mut Self>) {
        if self.rust().dormant_midi_mappings.is_empty() {
            return;
        }
        let Some(graph) = self.rust().graph.clone() else {
            return;
        };
        let all_nodes = graph.get_all_nodes();
        let (back, still): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.as_mut().rust_mut().dormant_midi_mappings)
                .into_iter()
                .partition(|m| node_name_present(&graph, &all_nodes, &m.source.device_name));
        self.as_mut().rust_mut().dormant_midi_mappings = still;
        if back.is_empty() {
            return;
        }
        log::info!("MIDI device back: re-adding {} mapping(s)", back.len());
        if let Some(ref tx) = self.rust().cmd_tx {
            for m in back {
                let _ = tx.send(PwCommand::AddMidiMapping(m));
            }
        }
    }
}

impl qobject::AppController {
    pub(super) fn drop_plugin_midi_mappings(mut self: Pin<&mut Self>, instance_id: u64) {
        if let Some(ref tx) = self.rust().cmd_tx {
            let _ = tx.send(PwCommand::RemoveMidiMappingsForPlugin { instance_id });
        }
        let had_dormant = self
            .rust()
            .dormant_midi_mappings
            .iter()
            .any(|m| m.target.instance_id == instance_id);
        self.as_mut()
            .rust_mut()
            .dormant_midi_mappings
            .retain(|m| m.target.instance_id != instance_id);
        let removed: Vec<u32> = self
            .rust()
            .midi_mappings
            .iter()
            .filter(|m| m.target.instance_id == instance_id)
            .map(|m| m.target.port_index as u32)
            .collect();
        if removed.is_empty() {
            if had_dormant {
                self.as_mut().persist_midi();
            }
            return;
        }
        self.as_mut()
            .rust_mut()
            .midi_mappings
            .retain(|m| m.target.instance_id != instance_id);
        self.as_mut().persist_midi();
        for port in removed {
            self.as_mut().midi_mapping_removed(instance_id, port);
        }
    }
}

impl qobject::AppController {
    pub fn reassign_midi_mapping(mut self: Pin<&mut Self>) -> bool {
        let Some(mapping) = self.as_mut().rust_mut().pending_conflict_mapping.take() else {
            log::warn!("reassign_midi_mapping: no pending conflict");
            return false;
        };
        log::info!(
            "Reassigning MIDI {:?} to instance {} port {}",
            mapping.source,
            mapping.target.instance_id,
            mapping.target.port_index
        );
        if let Some(ref tx) = self.rust().cmd_tx {
            let _ = tx.send(PwCommand::RemoveMidiMapping(mapping.source.clone()));
            let _ = tx.send(PwCommand::AddMidiMapping(mapping));
            true
        } else {
            false
        }
    }

    pub fn dismiss_midi_conflict(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().pending_conflict_mapping = None;
    }
}

impl qobject::AppController {
    pub fn start_midi_learn(
        mut self: Pin<&mut Self>,
        instance_id: u64,
        port_index: u32,
        label: QString,
        mode: QString,
    ) {
        let mode_str = mode.to_string();
        let mapping_mode = match mode_str.as_str() {
            "toggle" => crate::midi::MappingMode::Toggle,
            "momentary" => crate::midi::MappingMode::Momentary,
            _ => crate::midi::MappingMode::Continuous,
        };
        let label_str = label.to_string();
        self.as_mut().rust_mut().midi_learn_target = Some((
            instance_id,
            port_index as usize,
            label_str.clone(),
            mapping_mode,
        ));
        if let Some(ref tx) = self.rust().cmd_tx {
            let _ = tx.send(PwCommand::StartMidiLearn {
                instance_id,
                port_index: port_index as usize,
                label: label_str,
                mode: mapping_mode,
            });
        }
    }

    pub fn cancel_midi_learn(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().midi_learn_target = None;
        if let Some(ref tx) = self.rust().cmd_tx {
            let _ = tx.send(PwCommand::CancelMidiLearn);
        }
    }

    pub fn remove_midi_mapping_for_param(
        mut self: Pin<&mut Self>,
        instance_id: u64,
        port_index: u32,
    ) {
        let target = crate::midi::MidiCcTarget {
            instance_id,
            port_index: port_index as usize,
        };
        if self
            .rust()
            .dormant_midi_mappings
            .iter()
            .any(|m| m.target == target)
        {
            self.as_mut()
                .rust_mut()
                .dormant_midi_mappings
                .retain(|m| m.target != target);
            self.as_mut().persist_midi();
        }
        let source = self
            .rust()
            .midi_mappings
            .iter()
            .find(|m| m.target == target)
            .map(|m| m.source.clone());
        if let Some(source) = source
            && let Some(ref tx) = self.rust().cmd_tx
        {
            let _ = tx.send(PwCommand::RemoveMidiMapping(source));
        }
    }

    pub fn get_midi_mappings_json(self: Pin<&mut Self>) -> QString {
        let json =
            serde_json::to_string(&self.rust().midi_mappings).unwrap_or_else(|_| "[]".to_string());
        QString::from(&json)
    }

    pub fn get_midi_mapping_for_param_json(
        self: Pin<&mut Self>,
        instance_id: u64,
        port_index: u32,
    ) -> QString {
        let target = crate::midi::MidiCcTarget {
            instance_id,
            port_index: port_index as usize,
        };
        let mapping = self
            .rust()
            .midi_mappings
            .iter()
            .find(|m| m.target == target);
        match mapping {
            Some(m) => {
                let json = serde_json::to_string(m).unwrap_or_else(|_| "{}".to_string());
                QString::from(&json)
            }
            None => QString::from(""),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub(super) struct SavedMidiMapping {
    #[serde(flatten)]
    pub(super) mapping: crate::midi::MidiCcMapping,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(super) stable_id: String,
}

pub(super) fn load_midi_mappings() -> Vec<SavedMidiMapping> {
    load_config::<Vec<SavedMidiMapping>>("midi_mappings.json")
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub(super) fn resolve_saved_midi_mappings(
    saved: Vec<SavedMidiMapping>,
    stable_to_instance: &HashMap<String, u64>,
    restore_order: &[u64],
) -> Vec<crate::midi::MidiCcMapping> {
    let mut out: Vec<crate::midi::MidiCcMapping> = Vec::new();
    for s in saved {
        let mut mapping = s.mapping;
        let instance_id = if !s.stable_id.is_empty() {
            match stable_to_instance.get(&s.stable_id) {
                Some(&id) => id,
                None => {
                    log::warn!(
                        "Dropping MIDI mapping '{}': plugin {} no longer exists",
                        mapping.label,
                        s.stable_id
                    );
                    continue;
                }
            }
        } else {
            let old = mapping.target.instance_id;
            match old
                .checked_sub(1)
                .and_then(|i| restore_order.get(i as usize))
            {
                Some(&id) => {
                    log::warn!(
                        "Legacy MIDI mapping '{}': mapping old instance {} to {} by restore order",
                        mapping.label,
                        old,
                        id
                    );
                    id
                }
                None => {
                    log::warn!(
                        "Dropping legacy MIDI mapping '{}': unknown old instance {}",
                        mapping.label,
                        old
                    );
                    continue;
                }
            }
        };
        mapping.target.instance_id = instance_id;
        out.retain(|m| m.source != mapping.source && m.target != mapping.target);
        out.push(mapping);
    }
    out
}

pub(super) fn persist_midi_mappings(
    mappings: &[crate::midi::MidiCcMapping],
    plugin_manager: Option<&PluginManager>,
) {
    if !plugin_manager.is_some_and(|mgr| mgr.startup_complete()) {
        return;
    }
    if crate::PLUGINS_FROZEN.load(std::sync::atomic::Ordering::SeqCst) {
        log::info!("persist_midi_mappings: skipped (plugins frozen in safe mode)");
        return;
    }
    let saved: Vec<SavedMidiMapping> = mappings
        .iter()
        .filter_map(|m| {
            let stable_id = plugin_manager
                .and_then(|mgr| mgr.get_instance(m.target.instance_id))
                .map(|info| info.stable_id.clone())
                .filter(|sid| !sid.is_empty());
            match stable_id {
                Some(stable_id) => Some(SavedMidiMapping {
                    mapping: m.clone(),
                    stable_id,
                }),
                None => {
                    log::warn!(
                        "Not saving MIDI mapping '{}': plugin instance {} unknown",
                        m.label,
                        m.target.instance_id
                    );
                    None
                }
            }
        })
        .collect();
    let json = serde_json::to_string_pretty(&saved).unwrap_or_default();
    if write_config("midi_mappings.json", json.as_bytes(), saved.is_empty()) {
        log::debug!("persist_midi_mappings: {} mappings written", saved.len());
    }
}
