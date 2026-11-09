//! Per-plugin parameter presets.
//!
//! A preset is a named set of parameter values for one plugin (keyed by the
//! plugin URI). Each plugin can have one default preset, which is applied to
//! new instances of it. Which preset an instance has selected is remembered
//! per instance (by its stable id), so the node can show it after a restart.
//!
//! Only parameter values are stored. Plugin state that isn't exposed as a
//! parameter (sample paths, internal patches) is not part of a preset.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use crate::lv2::Lv2ParameterValue;
use crate::lv2::state::StateEntry;

/// Where a preset's copies of an LV2 plugin's state files live. The name is
/// hex-encoded so distinct preset names never share a directory.
fn preset_files_dir(plugin_uri: &str, name: &str) -> PathBuf {
    let encoded: String = name.bytes().map(|b| format!("{b:02x}")).collect();
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("zestbay")
        .join("preset-files")
        .join(crate::lv2::state::sanitize_uri(plugin_uri))
        .join(encoded)
}

/// Snapshot the files an LV2 instance keeps in its state directory (made via
/// the state makePath/mapPath features) into the preset, so the preset can be
/// applied to any other instance. State entries refer to those files by
/// paths relative to the instance's directory.
pub fn store_preset_files(plugin_uri: &str, scope: &str, name: &str) {
    let src = crate::lv2::state::instance_state_dir(plugin_uri, scope);
    let dst = preset_files_dir(plugin_uri, name);
    let _ = std::fs::remove_dir_all(&dst);
    if let Err(e) = crate::lv2::state::copy_dir_contents(&src, &dst) {
        log::warn!("Could not store state files of preset \"{}\": {}", name, e);
    }
}

/// Copy a preset's state files into an instance's state directory, before
/// the preset's state is restored into it.
pub fn restore_preset_files(plugin_uri: &str, scope: &str, name: &str) {
    let src = preset_files_dir(plugin_uri, name);
    let dst = crate::lv2::state::instance_state_dir(plugin_uri, scope);
    if let Err(e) = crate::lv2::state::copy_dir_contents(&src, &dst) {
        log::warn!("Could not restore state files of preset \"{}\": {}", name, e);
    }
}

/// Remove a preset's stored state files.
pub fn delete_preset_files(plugin_uri: &str, name: &str) {
    let _ = std::fs::remove_dir_all(preset_files_dir(plugin_uri, name));
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct PresetParam {
    #[serde(default)]
    pub symbol: String,
    pub port_index: usize,
    pub value: f32,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Preset {
    pub params: Vec<PresetParam>,
    /// LV2 plugin state (things that aren't parameters, such as the LSP
    /// mixer's channel names). Empty for plugins without state.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lv2_state: Vec<StateEntry>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
pub struct PluginPresets {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default)]
    pub presets: BTreeMap<String, Preset>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
pub struct PresetStore {
    /// By plugin URI.
    #[serde(default)]
    pub plugins: BTreeMap<String, PluginPresets>,
    /// Instance stable id -> the preset it has selected.
    #[serde(default)]
    pub selected: HashMap<String, String>,
}

impl PresetStore {
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty() && self.selected.is_empty()
    }

    /// Preset names for a plugin, sorted.
    pub fn names(&self, uri: &str) -> Vec<String> {
        self.plugins
            .get(uri)
            .map(|p| p.presets.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn default_name(&self, uri: &str) -> Option<&str> {
        self.plugins
            .get(uri)
            .and_then(|p| p.default.as_deref())
            .filter(|name| self.get(uri, name).is_some())
    }

    pub fn get(&self, uri: &str, name: &str) -> Option<&Preset> {
        self.plugins.get(uri).and_then(|p| p.presets.get(name))
    }

    pub fn selected_name(&self, stable_id: &str, uri: &str) -> Option<&str> {
        self.selected
            .get(stable_id)
            .map(String::as_str)
            .filter(|name| self.get(uri, name).is_some())
    }

    /// Create the preset, or overwrite it if the name exists. Returns true
    /// if it already existed.
    pub fn save(
        &mut self,
        uri: &str,
        name: &str,
        params: &[Lv2ParameterValue],
        lv2_state: &[StateEntry],
    ) -> bool {
        let preset = Preset {
            params: params
                .iter()
                .map(|p| PresetParam {
                    symbol: p.symbol.clone(),
                    port_index: p.port_index,
                    value: p.value,
                })
                .collect(),
            lv2_state: lv2_state.to_vec(),
        };
        self.plugins
            .entry(uri.to_string())
            .or_default()
            .presets
            .insert(name.to_string(), preset)
            .is_some()
    }

    /// Replace a preset's stored LV2 state (a fresher snapshot arrived after
    /// the preset was saved).
    pub fn set_lv2_state(&mut self, uri: &str, name: &str, lv2_state: Vec<StateEntry>) {
        if let Some(p) = self.plugins.get_mut(uri).and_then(|p| p.presets.get_mut(name)) {
            p.lv2_state = lv2_state;
        }
    }

    pub fn delete(&mut self, uri: &str, name: &str) {
        if let Some(p) = self.plugins.get_mut(uri) {
            p.presets.remove(name);
            if p.default.as_deref() == Some(name) {
                p.default = None;
            }
            if p.presets.is_empty() && p.default.is_none() {
                self.plugins.remove(uri);
            }
        }
        // Instances that had it selected need no cleanup: selected_name()
        // ignores a selection whose preset no longer exists.
    }

    /// Make `name` the plugin's default, or clear it with None. A name that
    /// doesn't exist is ignored.
    pub fn set_default(&mut self, uri: &str, name: Option<&str>) {
        match name {
            Some(n) if self.get(uri, n).is_some() => {
                self.plugins.entry(uri.to_string()).or_default().default = Some(n.to_string());
            }
            Some(_) => {}
            None => {
                if let Some(p) = self.plugins.get_mut(uri) {
                    p.default = None;
                }
            }
        }
    }

    pub fn select(&mut self, stable_id: &str, name: Option<&str>) {
        match name {
            Some(n) => {
                self.selected.insert(stable_id.to_string(), n.to_string());
            }
            None => {
                self.selected.remove(stable_id);
            }
        }
    }
}

/// The (port_index, value) pairs to apply `preset` to an instance with
/// `params`. Parameters are matched by symbol, falling back to the port
/// index when the symbol is empty. Parameters the instance doesn't have are
/// skipped.
pub fn values_to_apply(preset: &Preset, params: &[Lv2ParameterValue]) -> Vec<(usize, f32)> {
    ParamIndex::new(params).values_to_apply(preset)
}

/// An instance's parameters indexed by the keys a preset matches on, so a
/// preset is compared in linear time (this runs for every plugin on every
/// graph refresh, and plugins can have thousands of parameters).  Each
/// lookup returns the first parameter in list order that matches, like a
/// linear search would.
struct ParamIndex<'a> {
    params: &'a [Lv2ParameterValue],
    /// First parameter with each (non-empty) symbol.
    by_symbol: std::collections::HashMap<&'a str, usize>,
    /// First parameter with each port index.
    by_port: std::collections::HashMap<usize, usize>,
    /// First parameter without a symbol for each port index.
    unnamed_by_port: std::collections::HashMap<usize, usize>,
}

impl<'a> ParamIndex<'a> {
    fn new(params: &'a [Lv2ParameterValue]) -> Self {
        let mut index = Self {
            params,
            by_symbol: std::collections::HashMap::with_capacity(params.len()),
            by_port: std::collections::HashMap::with_capacity(params.len()),
            unnamed_by_port: std::collections::HashMap::new(),
        };
        for (pos, p) in params.iter().enumerate() {
            index.by_port.entry(p.port_index).or_insert(pos);
            if p.symbol.is_empty() {
                index.unnamed_by_port.entry(p.port_index).or_insert(pos);
            } else {
                index.by_symbol.entry(p.symbol.as_str()).or_insert(pos);
            }
        }
        index
    }

    /// The parameter a preset entry applies to: by symbol, or by port index
    /// when either side has no symbol.
    fn target(&self, pp: &PresetParam) -> Option<&'a Lv2ParameterValue> {
        let pos = if pp.symbol.is_empty() {
            self.by_port.get(&pp.port_index).copied()
        } else {
            let named = self.by_symbol.get(pp.symbol.as_str()).copied();
            let unnamed = self.unnamed_by_port.get(&pp.port_index).copied();
            match (named, unnamed) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            }
        };
        pos.map(|pos| &self.params[pos])
    }

    fn by_port(&self, port_index: usize) -> Option<&'a Lv2ParameterValue> {
        self.by_port.get(&port_index).map(|&pos| &self.params[pos])
    }

    fn values_to_apply(&self, preset: &Preset) -> Vec<(usize, f32)> {
        preset
            .params
            .iter()
            .filter_map(|pp| {
                self.target(pp)
                    .map(|p| (p.port_index, clamp_to(p, pp.value)))
            })
            .collect()
    }
}

fn clamp_to(p: &Lv2ParameterValue, v: f32) -> f32 {
    if p.max > p.min { v.clamp(p.min, p.max) } else { v }
}

/// Whether applying `preset` would change the plugin's LV2 state.
pub fn state_differs(preset: &Preset, current: &[StateEntry]) -> bool {
    !preset.lv2_state.is_empty() && preset.lv2_state != current
}

/// What differs between the instance and the preset, for display: one line
/// per changed parameter ("Gain: 1.000 \u{2192} 0.982", preset value first),
/// plus a line if the plugin state differs.
pub fn differences(preset: &Preset, params: &[Lv2ParameterValue], state: &[StateEntry]) -> Vec<String> {
    let index = ParamIndex::new(params);
    let mut out: Vec<String> = index
        .values_to_apply(preset)
        .into_iter()
        .filter_map(|(port, value)| {
            let p = index.by_port(port)?;
            ((p.value - value).abs() > 1e-4 * value.abs().max(1.0)).then(|| {
                let label = if p.name.is_empty() { &p.symbol } else { &p.name };
                format!("{}: {:.3} \u{2192} {:.3}", label, value, p.value)
            })
        })
        .collect();
    if state_differs(preset, state) {
        out.push("Plugin state (e.g. channel names)".to_string());
    }
    out
}

/// Whether the instance's current values or state differ from the preset.
pub fn is_modified(preset: &Preset, params: &[Lv2ParameterValue], state: &[StateEntry]) -> bool {
    if state_differs(preset, state) {
        return true;
    }
    let index = ParamIndex::new(params);
    index.values_to_apply(preset).into_iter().any(|(port, value)| {
        index
            .by_port(port)
            .is_some_and(|p| (p.value - value).abs() > 1e-4 * value.abs().max(1.0))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param(port_index: usize, symbol: &str, value: f32) -> Lv2ParameterValue {
        Lv2ParameterValue {
            port_index,
            symbol: symbol.to_string(),
            name: symbol.to_string(),
            value,
            min: 0.0,
            max: 10.0,
            default: 0.0,
            is_toggle: false,
        }
    }

    /// The linear search `ParamIndex` replaced; the index must agree with
    /// it, including which of several candidates comes first.
    fn values_to_apply_linear(preset: &Preset, params: &[Lv2ParameterValue]) -> Vec<(usize, f32)> {
        preset
            .params
            .iter()
            .filter_map(|pp| {
                params
                    .iter()
                    .find(|p| {
                        if pp.symbol.is_empty() || p.symbol.is_empty() {
                            p.port_index == pp.port_index
                        } else {
                            p.symbol == pp.symbol
                        }
                    })
                    .map(|p| (p.port_index, clamp_to(p, pp.value)))
            })
            .collect()
    }

    #[test]
    fn indexed_matching_agrees_with_linear_search() {
        let entry = |port_index: usize, symbol: &str, value: f32| PresetParam {
            symbol: symbol.to_string(),
            port_index,
            value,
        };
        // Named, unnamed, duplicated symbols, duplicated port indices, and
        // an unnamed parameter sharing a port index with a named one.
        let params = vec![
            param(0, "gain", 1.0),
            param(1, "", 2.0),
            param(2, "mix", 3.0),
            param(2, "", 4.0),
            param(3, "gain", 5.0),
            param(5, "tone", 6.0),
            param(1, "late", 7.0),
        ];
        let preset = Preset {
            params: vec![
                entry(0, "gain", 0.5),
                entry(9, "gain", 0.6),
                entry(1, "", 0.7),
                entry(1, "other", 0.8),
                entry(2, "mix", 0.9),
                entry(2, "nomatch", 1.1),
                entry(2, "", 1.2),
                entry(5, "", 99.0),
                entry(7, "tone", -4.0),
                entry(8, "missing", 1.0),
                entry(8, "", 1.0),
            ],
            ..Default::default()
        };
        let indexed = values_to_apply(&preset, &params);
        assert_eq!(indexed, values_to_apply_linear(&preset, &params));
        assert!(!indexed.is_empty());
        assert_eq!(values_to_apply(&preset, &[]), Vec::new());
    }

    #[test]
    fn save_then_overwrite() {
        let mut s = PresetStore::default();
        assert!(!s.save("urn:p", "A", &[param(0, "gain", 1.0)], &[]));
        assert!(s.save("urn:p", "A", &[param(0, "gain", 2.0)], &[]));
        assert_eq!(s.names("urn:p"), vec!["A"]);
        assert_eq!(s.get("urn:p", "A").unwrap().params[0].value, 2.0);
    }

    #[test]
    fn default_must_exist_and_clears_on_delete() {
        let mut s = PresetStore::default();
        s.save("urn:p", "A", &[param(0, "gain", 1.0)], &[]);
        s.set_default("urn:p", Some("missing"));
        assert_eq!(s.default_name("urn:p"), None);
        s.set_default("urn:p", Some("A"));
        assert_eq!(s.default_name("urn:p"), Some("A"));
        s.delete("urn:p", "A");
        assert_eq!(s.default_name("urn:p"), None);
        assert!(s.names("urn:p").is_empty());
    }

    #[test]
    fn selection_forgets_deleted_preset() {
        let mut s = PresetStore::default();
        s.save("urn:p", "A", &[param(0, "gain", 1.0)], &[]);
        s.select("inst", Some("A"));
        assert_eq!(s.selected_name("inst", "urn:p"), Some("A"));
        s.delete("urn:p", "A");
        assert_eq!(s.selected_name("inst", "urn:p"), None);
    }

    #[test]
    fn apply_matches_by_symbol_then_port_and_clamps() {
        let mut s = PresetStore::default();
        s.save("urn:p", "A", &[param(0, "gain", 50.0), param(1, "", 3.0), param(2, "gone", 1.0)], &[]);
        let target = [param(7, "gain", 0.0), param(1, "", 0.0)];
        let v = values_to_apply(s.get("urn:p", "A").unwrap(), &target);
        assert_eq!(v, vec![(7, 10.0), (1, 3.0)]);
    }

    #[test]
    fn modified_detection() {
        let mut s = PresetStore::default();
        s.save("urn:p", "A", &[param(0, "gain", 2.0)], &[]);
        let preset = s.get("urn:p", "A").unwrap();
        assert!(!is_modified(preset, &[param(0, "gain", 2.0)], &[]));
        assert!(is_modified(preset, &[param(0, "gain", 2.5)], &[]));
    }

    fn entry(key: &str, value: &str) -> StateEntry {
        StateEntry::new_string(key, value)
    }

    #[test]
    fn lv2_state_is_stored_and_compared() {
        let mut s = PresetStore::default();
        let names = [entry("urn:k", "AI Voice L")];
        s.save("urn:p", "A", &[param(0, "gain", 2.0)], &names);
        let preset = s.get("urn:p", "A").unwrap();
        assert!(!state_differs(preset, &names));
        assert!(state_differs(preset, &[entry("urn:k", "Other")]));
        assert!(is_modified(preset, &[param(0, "gain", 2.0)], &[entry("urn:k", "Other")]));
        assert!(!is_modified(preset, &[param(0, "gain", 2.0)], &names));
        // A preset with no stored state never reports a state difference.
        s.save("urn:p", "B", &[param(0, "gain", 2.0)], &[]);
        assert!(!state_differs(s.get("urn:p", "B").unwrap(), &names));
    }

    #[test]
    fn differences_lists_changes() {
        let mut s = PresetStore::default();
        s.save("urn:p", "A", &[param(0, "gain", 2.0), param(1, "pan", 1.0)], &[entry("urn:k", "a")]);
        let preset = s.get("urn:p", "A").unwrap();
        let live = [param(0, "gain", 2.5), param(1, "pan", 1.0)];
        let d = differences(preset, &live, &[entry("urn:k", "b")]);
        assert_eq!(d.len(), 2);
        assert!(d[0].starts_with("gain: 2.000"));
        assert!(differences(preset, &[param(0, "gain", 2.0), param(1, "pan", 1.0)], &[entry("urn:k", "a")]).is_empty());
    }

    #[test]
    fn set_lv2_state_replaces_snapshot() {
        let mut s = PresetStore::default();
        s.save("urn:p", "A", &[], &[entry("urn:k", "old")]);
        s.set_lv2_state("urn:p", "A", vec![entry("urn:k", "new")]);
        assert_eq!(s.get("urn:p", "A").unwrap().lv2_state, vec![entry("urn:k", "new")]);
    }

    #[test]
    fn roundtrips_through_json() {
        let mut s = PresetStore::default();
        s.save("urn:p", "A", &[param(0, "gain", 2.0)], &[]);
        s.set_default("urn:p", Some("A"));
        s.select("inst", Some("A"));
        let back: PresetStore = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back.default_name("urn:p"), Some("A"));
        assert_eq!(back.selected_name("inst", "urn:p"), Some("A"));
    }
}
