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
    /// The sample rate reported by PipeWire (set after PW init).
    pub sample_rate: f64,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            available_plugins: Vec::new(),
            active_instances: HashMap::new(),
            sample_rate: 48000.0,
        }
    }

    // ----- Catalog -----

    /// Replace the entire catalog (called after scanning all formats).
    pub fn set_available_plugins(&mut self, plugins: Vec<PluginInfo>) {
        self.available_plugins = plugins;
    }

    /// Append additional plugins (e.g. after a single-format rescan).
    pub fn extend_available_plugins(&mut self, plugins: Vec<PluginInfo>) {
        self.available_plugins.extend(plugins);
    }

    /// Sort the catalog alphabetically by name (case-insensitive).
    pub fn sort_catalog(&mut self) {
        self.available_plugins
            .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    }

    pub fn available_plugins(&self) -> &[PluginInfo] {
        &self.available_plugins
    }

    pub fn find_plugin(&self, uri: &str) -> Option<&PluginInfo> {
        self.available_plugins.iter().find(|p| p.uri == uri)
    }

    pub fn find_plugin_with_format(&self, uri: &str, format: PluginFormat) -> Option<&PluginInfo> {
        self.available_plugins
            .iter()
            .find(|p| p.uri == uri && p.format == format)
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
                });
            }
        }
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

    pub fn find_by_stable_id_mut(&mut self, stable_id: &str) -> Option<&mut PluginInstanceInfo> {
        self.active_instances
            .values_mut()
            .find(|info| info.stable_id == stable_id)
    }

    pub fn instance_id_for_stable_id(&self, stable_id: &str) -> Option<PluginInstanceId> {
        self.active_instances
            .iter()
            .find(|(_, info)| info.stable_id == stable_id)
            .map(|(id, _)| *id)
    }
}
