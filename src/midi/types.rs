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
