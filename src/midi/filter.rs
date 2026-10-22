//! Resolved MIDI mapping tables used by the plugin filters' RT callbacks.

use super::types::{MappingMode, MidiCcSource, MidiMessageType};
use crate::plugin::types::SharedPortUpdates;

pub struct ResolvedMappingEntry {
    pub port_updates: SharedPortUpdates,
    pub port_index: usize,
    pub instance_id: u64,
    pub min: f32,
    pub max: f32,
    pub mode: MappingMode,
    pub source: MidiCcSource,
    pub is_logarithmic: bool,
    pub is_toggle: bool,
}

/// Immutable snapshot of all resolved mappings for one plugin.
/// The manager builds a new `Arc<ResolvedMappings>` and hands it to the
/// filter's `update_mappings`, which swaps it in lock-free and releases the
/// previous table on the main thread once the RT callback is done with it
/// (see `midi::rt::MappingSwap`).
pub struct ResolvedMappings {
    entries: Vec<ResolvedMappingEntry>,
}

impl ResolvedMappings {
    pub fn new(entries: Vec<ResolvedMappingEntry>) -> Self {
        Self { entries }
    }

    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Match by channel + cc only (ignoring device_name).
    /// Used for per-plugin MIDI ports where routing determines the device.
    pub fn find_any_device(
        &self,
        channel: u8,
        cc: u8,
        message_type: MidiMessageType,
    ) -> Option<&ResolvedMappingEntry> {
        let exact = self.entries.iter().find(|e| {
            e.source.channel == Some(channel)
                && e.source.cc == cc
                && e.source.message_type == message_type
        });
        if exact.is_some() {
            return exact;
        }
        self.entries.iter().find(|e| {
            e.source.channel.is_none() && e.source.cc == cc && e.source.message_type == message_type
        })
    }
}

unsafe impl Send for ResolvedMappings {}
unsafe impl Sync for ResolvedMappings {}

#[cfg(test)]
mod resolved_mappings_tests {
    use super::*;
    use crate::plugin::types::*;
    use std::sync::Arc;

    fn make_entry(
        device: &str,
        channel: Option<u8>,
        cc: u8,
        msg_type: MidiMessageType,
    ) -> ResolvedMappingEntry {
        let port_updates = Arc::new(PortUpdates {
            control_inputs: vec![PortSlot {
                port_index: 0,
                value: AtomicF32::new(0.0),
            }],
            control_outputs: Vec::new(),
            atom_outputs: Vec::new(),
            atom_inputs: Vec::new(),
        });
        ResolvedMappingEntry {
            port_updates,
            port_index: 0,
            instance_id: 1,
            min: 0.0,
            max: 1.0,
            mode: MappingMode::Continuous,
            source: MidiCcSource {
                device_name: device.to_string(),
                channel,
                cc,
                message_type: msg_type,
            },
            is_logarithmic: false,
            is_toggle: false,
        }
    }

    #[test]
    fn find_exact_preferred_over_wildcard() {
        let mut exact = make_entry("dev1", Some(5), 10, MidiMessageType::Cc);
        exact.instance_id = 100;
        let mut wildcard = make_entry("dev1", None, 10, MidiMessageType::Cc);
        wildcard.instance_id = 200;

        let mappings = ResolvedMappings::new(vec![wildcard, exact]);

        let found = mappings
            .find_any_device(5, 10, MidiMessageType::Cc)
            .unwrap();
        assert_eq!(found.instance_id, 100); // exact wins
        let found = mappings
            .find_any_device(6, 10, MidiMessageType::Cc)
            .unwrap();
        assert_eq!(found.instance_id, 200);
    }

    #[test]
    fn find_wrong_message_type() {
        let mappings =
            ResolvedMappings::new(vec![make_entry("dev1", Some(0), 60, MidiMessageType::Cc)]);
        assert!(
            mappings
                .find_any_device(0, 60, MidiMessageType::Note)
                .is_none()
        );
    }

    #[test]
    fn find_any_device_exact_channel() {
        let mappings =
            ResolvedMappings::new(vec![make_entry("dev1", Some(5), 10, MidiMessageType::Cc)]);
        // find_any_device ignores device_name
        assert!(
            mappings
                .find_any_device(5, 10, MidiMessageType::Cc)
                .is_some()
        );
    }

    #[test]
    fn find_any_device_wildcard_channel() {
        let mappings =
            ResolvedMappings::new(vec![make_entry("dev1", None, 10, MidiMessageType::Cc)]);
        assert!(
            mappings
                .find_any_device(3, 10, MidiMessageType::Cc)
                .is_some()
        );
        assert!(
            mappings
                .find_any_device(15, 10, MidiMessageType::Cc)
                .is_some()
        );
    }

    #[test]
    fn find_any_device_no_match() {
        let mappings =
            ResolvedMappings::new(vec![make_entry("dev1", Some(5), 10, MidiMessageType::Cc)]);
        assert!(
            mappings
                .find_any_device(3, 10, MidiMessageType::Cc)
                .is_none()
        );
    }

    #[test]
    fn empty_mappings() {
        let mappings = ResolvedMappings::empty();
        assert!(
            mappings
                .find_any_device(0, 1, MidiMessageType::Cc)
                .is_none()
        );
    }
}
