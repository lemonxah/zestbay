//! Unified plugin manager that wraps format-specific backends (LV2, CLAP, VST3).
//!
//! This is the single entry-point that the UI and PipeWire layers talk to for
//! plugin catalog queries, instance registration, and parameter bookkeeping.

use std::collections::HashMap;

use super::types::*;

/// A unified manager holding the catalog of available plugins (from all
/// formats) and the registry of active plugin instances.
pub struct PluginManager {
    /// All available plugins, merged from all format-specific scanners.
    available_plugins: Vec<PluginInfo>,
    /// Currently active instances, keyed by instance ID.
    active_instances: HashMap<PluginInstanceId, PluginInstanceInfo>,
    startup_complete: bool,
    /// The sample rate reported by PipeWire (set after PW init).
    #[allow(dead_code)] // written at init; no reader yet
    pub sample_rate: f64,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            available_plugins: Vec::new(),
            active_instances: HashMap::new(),
            startup_complete: false,
            sample_rate: 48000.0,
        }
    }

    // ----- Catalog -----

    pub fn startup_complete(&self) -> bool {
        self.startup_complete
    }

    pub fn finish_startup(&mut self) {
        self.startup_complete = true;
    }

    /// Forget every instance and go back to "starting up", for loading
    /// another session. The catalog of available plugins stays.
    pub fn reset_for_new_session(&mut self) {
        self.active_instances.clear();
        self.startup_complete = false;
    }

    /// Replace the entire catalog (called after scanning all formats).
    pub fn set_available_plugins(&mut self, plugins: Vec<PluginInfo>) {
        // Backfill old saves and refresh paths for relocated plugins. Keep a
        // directly restored descriptor if its library is outside search dirs.
        for instance in self.active_instances.values_mut() {
            if instance.format != PluginFormat::Lv2
                && let Some(info) = plugins.iter().find(|p| {
                    p.uri == instance.plugin_uri && p.format == instance.format
                })
            {
                instance.scan_info = Some(info.clone());
            }
        }
        self.available_plugins = plugins;
    }

    pub fn available_plugins(&self) -> &[PluginInfo] {
        &self.available_plugins
    }

    /// A catalog plugin, or one of ZestBay's built-ins (which the catalog,
    /// and so the browser, never lists).
    pub fn find_plugin(&self, uri: &str) -> Option<&PluginInfo> {
        self.available_plugins
            .iter()
            .find(|p| p.uri == uri)
            .or_else(|| crate::ladspa::builtin::info(uri))
    }

    pub fn find_plugin_with_format(&self, uri: &str, format: PluginFormat) -> Option<&PluginInfo> {
        self.available_plugins
            .iter()
            .find(|p| p.uri == uri && p.format == format)
            .or_else(|| crate::ladspa::builtin::info(uri).filter(|p| p.format == format))
    }

    // ----- Active instances -----

    pub fn register_instance(&mut self, info: PluginInstanceInfo) {
        self.active_instances.insert(info.id, info);
    }

    pub fn set_instance_pw_node_id(&mut self, instance_id: PluginInstanceId, pw_node_id: u32) {
        if let Some(info) = self.active_instances.get_mut(&instance_id) {
            info.pw_node_id = Some(pw_node_id);
        }
    }

    pub fn remove_instance(&mut self, instance_id: PluginInstanceId) {
        self.active_instances.remove(&instance_id);
    }

    pub fn update_parameter(
        &mut self,
        instance_id: PluginInstanceId,
        port_index: usize,
        value: f32,
    ) {
        if let Some(info) = self.active_instances.get_mut(&instance_id) {
            if let Some(param) = info
                .parameters
                .iter_mut()
                .find(|p| p.port_index == port_index)
            {
                param.value = value;
            } else {
                info.parameters.push(ParameterValue {
                    port_index,
                    symbol: String::new(),
                    name: String::new(),
                    value,
                    min: 0.0,
                    max: 1.0,
                    default: 0.0,
                    is_toggle: false,
                });
            }
        }
    }

    /// Replace an instance's parameter list with the one its running host
    /// reported.  Names, ranges and defaults come from `described`; a value
    /// this side already holds (a saved or just-edited one) is kept, matched
    /// by symbol and otherwise by port index.
    pub fn describe_parameters(
        &mut self,
        instance_id: PluginInstanceId,
        described: Vec<ParameterValue>,
    ) {
        let Some(info) = self.active_instances.get_mut(&instance_id) else {
            return;
        };
        let known = std::mem::take(&mut info.parameters);
        info.parameters = described
            .into_iter()
            .map(|mut d| {
                let held = known
                    .iter()
                    .find(|k| !k.symbol.is_empty() && k.symbol == d.symbol)
                    .or_else(|| known.iter().find(|k| k.port_index == d.port_index));
                if let Some(k) = held
                    && k.value.is_finite()
                {
                    d.value = k.value;
                }
                d
            })
            .collect();
    }

    pub fn active_instances(&self) -> &HashMap<PluginInstanceId, PluginInstanceInfo> {
        &self.active_instances
    }

    pub fn get_instance(&self, id: PluginInstanceId) -> Option<&PluginInstanceInfo> {
        self.active_instances.get(&id)
    }

    pub fn get_instance_mut(&mut self, id: PluginInstanceId) -> Option<&mut PluginInstanceInfo> {
        self.active_instances.get_mut(&id)
    }

    pub fn find_by_stable_id(&self, stable_id: &str) -> Option<&PluginInstanceInfo> {
        self.active_instances
            .values()
            .find(|info| info.stable_id == stable_id)
    }

    pub fn instance_id_for_stable_id(&self, stable_id: &str) -> Option<PluginInstanceId> {
        self.active_instances
            .iter()
            .find(|(_, info)| info.stable_id == stable_id)
            .map(|(id, _)| *id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param(port_index: usize, symbol: &str, name: &str, value: f32, max: f32) -> ParameterValue {
        ParameterValue {
            port_index,
            symbol: symbol.to_string(),
            name: name.to_string(),
            value,
            min: 0.0,
            max,
            default: 0.5,
            is_toggle: false,
        }
    }

    fn manager_with(parameters: Vec<ParameterValue>) -> PluginManager {
        let mut manager = PluginManager::new();
        manager.register_instance(PluginInstanceInfo {
            id: 1,
            stable_id: "sid".to_string(),
            plugin_uri: "urn:test".to_string(),
            format: PluginFormat::Lv2,
            scan_info: None,
            display_name: "Test".to_string(),
            pw_node_id: None,
            parameters,
            active: true,
            bypassed: false,
            lv2_state: Vec::new(),
            plugin_state: None,
            port_names: Default::default(),
        });
        manager
    }

    #[test]
    fn described_parameters_take_metadata_and_keep_saved_values() {
        // What a restore before the catalog scan leaves behind: saved
        // values with no names and a 0..1 range.
        let mut manager = manager_with(vec![
            param(0, "gain", "", 12.0, 1.0),
            param(1, "", "", 0.25, 1.0),
        ]);
        manager.describe_parameters(
            1,
            vec![
                // Port order changed since the save: matched by symbol.
                param(3, "gain", "Gain", 0.0, 24.0),
                // No symbol on the saved side: matched by port index.
                param(1, "mix", "Mix", 1.0, 1.0),
                // New parameter: the host's value stands.
                param(2, "tone", "Tone", 0.7, 1.0),
            ],
        );
        let params = &manager.get_instance(1).unwrap().parameters;
        assert_eq!(params.len(), 3);
        assert_eq!((params[0].port_index, params[0].name.as_str()), (3, "Gain"));
        assert_eq!((params[0].value, params[0].max), (12.0, 24.0));
        assert_eq!((params[1].name.as_str(), params[1].value), ("Mix", 0.25));
        assert_eq!((params[2].name.as_str(), params[2].value), ("Tone", 0.7));
    }

    #[test]
    fn described_parameters_fill_an_instance_that_had_none() {
        let mut manager = manager_with(Vec::new());
        manager.describe_parameters(1, vec![param(0, "param_7", "Cutoff", 0.3, 1.0)]);
        let params = &manager.get_instance(1).unwrap().parameters;
        assert_eq!(params.len(), 1);
        assert_eq!((params[0].name.as_str(), params[0].value), ("Cutoff", 0.3));
        // Unknown instances are ignored.
        manager.describe_parameters(99, vec![param(0, "x", "X", 0.0, 1.0)]);
    }
}
