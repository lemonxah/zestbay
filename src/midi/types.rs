//! MIDI controller mapping types.
//!
//! Each mapping binds a (device_name, channel, CC number) triple to a specific
//! plugin parameter (instance_id, port_index).  Mappings are per-CC granularity:
//! different parameters on the same plugin can come from different devices.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::plugin::types::PluginInstanceId;

// ---------------------------------------------------------------------------
// Mapping mode (toggle vs. momentary for button-type controls)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MidiMessageType {
    Cc,
    Note,
}

impl Default for MidiMessageType {
    fn default() -> Self {
        Self::Cc
    }
}

/// How a MIDI CC value is interpreted when the target parameter is boolean-ish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MappingMode {
    /// Continuous: CC 0-127 is mapped linearly to [min, max].
    /// This is the default for sliders / knobs.
    Continuous,
    /// Toggle: a CC value > 63 flips the parameter on/off.
    /// Each "press" (transition from <= 63 to > 63) toggles.
    Toggle,
    /// Momentary: CC > 63 = on, CC <= 63 = off.
    /// The parameter follows the button state exactly.
    Momentary,
}

impl Default for MappingMode {
    fn default() -> Self {
        Self::Continuous
    }
}

// ---------------------------------------------------------------------------
// A single CC mapping
// ---------------------------------------------------------------------------

/// Unique key for a CC source: which device, which MIDI channel, which CC number.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MidiCcSource {
    /// PipeWire node name of the MIDI device (e.g. "Midi Through Port-0").
    pub device_name: String,
    /// MIDI channel 0-15.  `None` means "any channel".
    pub channel: Option<u8>,
    /// MIDI CC number 0-127 (when `message_type == Cc`) or note number (when `Note`).
    pub cc: u8,
    #[serde(default)]
    pub message_type: MidiMessageType,
}

/// Describes the target end of a mapping: which plugin parameter to control.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MidiCcTarget {
    pub instance_id: PluginInstanceId,
    pub port_index: usize,
}

/// A complete CC mapping: source CC -> target parameter + mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiCcMapping {
    pub source: MidiCcSource,
    pub target: MidiCcTarget,
    pub mode: MappingMode,
    /// Human-readable label for display (e.g. "LSP Compressor > Threshold").
    #[serde(default)]
    pub label: String,
}

// ---------------------------------------------------------------------------
// Mapping table (owned by the manager, shared with the RT MIDI filter)
// ---------------------------------------------------------------------------

/// The mapping table used by the RT MIDI callback.
///
/// Lookups happen on the RT thread, so the table is shared via
/// `Arc<MidiMappingTable>` and swapped atomically (the manager builds a new
/// table and replaces the old `Arc` pointer).
#[derive(Debug, Clone, Default)]
pub struct MidiMappingTable {
    /// Source -> mapping.  Because each source can only map to one target,
    /// this is a 1:1 map.  Conflict detection is done at insert time.
    by_source: HashMap<MidiCcSource, MidiCcMapping>,
}

impl MidiMappingTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a mapping.  Returns `Some(old_mapping)` if the source was already bound.
    pub fn insert(&mut self, mapping: MidiCcMapping) -> Option<MidiCcMapping> {
        self.by_source.insert(mapping.source.clone(), mapping)
    }

    /// Remove the mapping for a given source.
    pub fn remove(&mut self, source: &MidiCcSource) -> Option<MidiCcMapping> {
        self.by_source.remove(source)
    }

    /// Remove all mappings that target a specific plugin instance.
    pub fn remove_by_instance(&mut self, instance_id: PluginInstanceId) {
        self.by_source
            .retain(|_, m| m.target.instance_id != instance_id);
    }

    pub fn remove_by_device(&mut self, device_name: &str) {
        self.by_source
            .retain(|source, _| source.device_name != device_name);
    }

    /// Lookup by source (used in the RT callback).
    pub fn get(&self, source: &MidiCcSource) -> Option<&MidiCcMapping> {
        self.by_source.get(source)
    }

    /// Lookup by source, but with wildcard channel matching.
    /// First tries exact match, then tries `channel: None` (any-channel mapping).
    pub fn get_with_wildcard(
        &self,
        device_name: &str,
        channel: u8,
        cc: u8,
        message_type: MidiMessageType,
    ) -> Option<&MidiCcMapping> {
        let exact = MidiCcSource {
            device_name: device_name.to_string(),
            channel: Some(channel),
            cc,
            message_type,
        };
        if let Some(m) = self.by_source.get(&exact) {
            return Some(m);
        }
        let wildcard = MidiCcSource {
            device_name: device_name.to_string(),
            channel: None,
            cc,
            message_type,
        };
        self.by_source.get(&wildcard)
    }

    /// Find an existing mapping for a target (instance_id + port_index).
    pub fn find_by_target(&self, target: &MidiCcTarget) -> Option<&MidiCcMapping> {
        self.by_source.values().find(|m| m.target == *target)
    }

    /// Check if a source is already mapped.  Returns the label of the existing
    /// mapping's target if so.
    pub fn conflict_check(&self, source: &MidiCcSource) -> Option<&str> {
        self.by_source.get(source).map(|m| m.label.as_str())
    }

    /// All mappings (for persistence / UI).
    pub fn all_mappings(&self) -> Vec<&MidiCcMapping> {
        self.by_source.values().collect()
    }

    /// Build from a list of mappings (for loading from persistence).
    pub fn from_mappings(mappings: Vec<MidiCcMapping>) -> Self {
        let mut table = Self::new();
        for m in mappings {
            table.insert(m);
        }
        table
    }
}

// ---------------------------------------------------------------------------
// Persistence wrapper
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedMidiMappings {
    pub mappings: Vec<MidiCcMapping>,
}

// ---------------------------------------------------------------------------
// MIDI learn state (held by the manager, not shared with RT)
// ---------------------------------------------------------------------------

/// When the user clicks "Learn" on a parameter, the manager enters this state
/// and waits for the next CC event from any device.
#[derive(Debug, Clone)]
pub struct MidiLearnState {
    pub target: MidiCcTarget,
    /// Human-readable label for the target (e.g. "LSP Compressor > Threshold").
    pub label: String,
    /// Preferred mapping mode (set by the UI based on parameter type).
    pub mode: MappingMode,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_source(device: &str, channel: Option<u8>, cc: u8) -> MidiCcSource {
        MidiCcSource {
            device_name: device.to_string(),
            channel,
            cc,
            message_type: MidiMessageType::Cc,
        }
    }

    fn make_mapping(device: &str, channel: Option<u8>, cc: u8, instance_id: u64, port_index: usize) -> MidiCcMapping {
        MidiCcMapping {
            source: make_source(device, channel, cc),
            target: MidiCcTarget { instance_id, port_index },
            mode: MappingMode::Continuous,
            label: format!("test-{}-{}", instance_id, port_index),
        }
    }

    // ---- MidiMappingTable basics ----

    #[test]
    fn table_insert_and_get() {
        let mut table = MidiMappingTable::new();
        let m = make_mapping("device1", Some(0), 1, 100, 0);
        assert!(table.insert(m.clone()).is_none());

        let source = make_source("device1", Some(0), 1);
        let result = table.get(&source);
        assert!(result.is_some());
        assert_eq!(result.unwrap().target.instance_id, 100);
    }

    #[test]
    fn table_insert_replaces_existing() {
        let mut table = MidiMappingTable::new();
        let m1 = make_mapping("device1", Some(0), 1, 100, 0);
        let m2 = make_mapping("device1", Some(0), 1, 200, 1);

        assert!(table.insert(m1).is_none());
        let old = table.insert(m2);
        assert!(old.is_some());
        assert_eq!(old.unwrap().target.instance_id, 100);

        let source = make_source("device1", Some(0), 1);
        assert_eq!(table.get(&source).unwrap().target.instance_id, 200);
    }

    #[test]
    fn table_remove() {
        let mut table = MidiMappingTable::new();
        let m = make_mapping("device1", Some(0), 1, 100, 0);
        table.insert(m);

        let source = make_source("device1", Some(0), 1);
        let removed = table.remove(&source);
        assert!(removed.is_some());
        assert!(table.get(&source).is_none());
    }

    #[test]
    fn table_remove_nonexistent() {
        let mut table = MidiMappingTable::new();
        let source = make_source("device1", Some(0), 1);
        assert!(table.remove(&source).is_none());
    }

    // ---- remove_by_instance ----

    #[test]
    fn table_remove_by_instance() {
        let mut table = MidiMappingTable::new();
        table.insert(make_mapping("d", Some(0), 1, 100, 0));
        table.insert(make_mapping("d", Some(0), 2, 100, 1));
        table.insert(make_mapping("d", Some(0), 3, 200, 0));

        table.remove_by_instance(100);

        assert_eq!(table.all_mappings().len(), 1);
        assert_eq!(table.all_mappings()[0].target.instance_id, 200);
    }

    // ---- remove_by_device ----

    #[test]
    fn table_remove_by_device() {
        let mut table = MidiMappingTable::new();
        table.insert(make_mapping("device_a", Some(0), 1, 100, 0));
        table.insert(make_mapping("device_b", Some(0), 1, 200, 0));

        table.remove_by_device("device_a");

        assert_eq!(table.all_mappings().len(), 1);
        assert_eq!(table.all_mappings()[0].source.device_name, "device_b");
    }

    // ---- get_with_wildcard ----

    #[test]
    fn table_get_with_wildcard_exact_match() {
        let mut table = MidiMappingTable::new();
        table.insert(make_mapping("dev", Some(5), 10, 100, 0));

        let result = table.get_with_wildcard("dev", 5, 10, MidiMessageType::Cc);
        assert!(result.is_some());
        assert_eq!(result.unwrap().target.instance_id, 100);
    }

    #[test]
    fn table_get_with_wildcard_falls_back_to_any_channel() {
        let mut table = MidiMappingTable::new();
        // Insert with channel=None (wildcard)
        table.insert(make_mapping("dev", None, 10, 100, 0));

        // Query with specific channel — should fall back to wildcard
        let result = table.get_with_wildcard("dev", 3, 10, MidiMessageType::Cc);
        assert!(result.is_some());
        assert_eq!(result.unwrap().target.instance_id, 100);
    }

    #[test]
    fn table_get_with_wildcard_exact_preferred_over_wildcard() {
        let mut table = MidiMappingTable::new();
        table.insert(make_mapping("dev", None, 10, 100, 0)); // wildcard
        table.insert(make_mapping("dev", Some(5), 10, 200, 0)); // exact

        let result = table.get_with_wildcard("dev", 5, 10, MidiMessageType::Cc);
        assert_eq!(result.unwrap().target.instance_id, 200); // exact wins
    }

    #[test]
    fn table_get_with_wildcard_no_match() {
        let table = MidiMappingTable::new();
        assert!(table.get_with_wildcard("dev", 0, 1, MidiMessageType::Cc).is_none());
    }

    // ---- find_by_target ----

    #[test]
    fn table_find_by_target() {
        let mut table = MidiMappingTable::new();
        table.insert(make_mapping("d", Some(0), 1, 100, 5));

        let target = MidiCcTarget { instance_id: 100, port_index: 5 };
        let result = table.find_by_target(&target);
        assert!(result.is_some());

        let missing = MidiCcTarget { instance_id: 999, port_index: 0 };
        assert!(table.find_by_target(&missing).is_none());
    }

    // ---- conflict_check ----

    #[test]
    fn table_conflict_check() {
        let mut table = MidiMappingTable::new();
        table.insert(make_mapping("d", Some(0), 1, 100, 0));

        let source = make_source("d", Some(0), 1);
        assert!(table.conflict_check(&source).is_some());

        let source2 = make_source("d", Some(0), 99);
        assert!(table.conflict_check(&source2).is_none());
    }

    // ---- from_mappings ----

    #[test]
    fn table_from_mappings() {
        let mappings = vec![
            make_mapping("d", Some(0), 1, 100, 0),
            make_mapping("d", Some(0), 2, 200, 0),
        ];
        let table = MidiMappingTable::from_mappings(mappings);
        assert_eq!(table.all_mappings().len(), 2);
    }

    // ---- Defaults ----

    #[test]
    fn mapping_mode_default_is_continuous() {
        assert_eq!(MappingMode::default(), MappingMode::Continuous);
    }

    #[test]
    fn midi_message_type_default_is_cc() {
        assert_eq!(MidiMessageType::default(), MidiMessageType::Cc);
    }
}
