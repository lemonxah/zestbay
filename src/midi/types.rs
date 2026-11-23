use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::plugin::types::PluginInstanceId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum MidiMessageType {
    #[default]
    Cc,
    Note,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MappingMode {
    #[default]
    Continuous,
    Toggle,
    Momentary,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MidiCcSource {
    pub device_name: String,
    pub channel: Option<u8>,
    pub cc: u8,
    #[serde(default)]
    pub message_type: MidiMessageType,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MidiCcTarget {
    pub instance_id: PluginInstanceId,
    pub port_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiCcMapping {
    pub source: MidiCcSource,
    pub target: MidiCcTarget,
    pub mode: MappingMode,
    #[serde(default)]
    pub label: String,
}

#[derive(Debug, Clone, Default)]
pub struct MidiMappingTable {
    by_source: HashMap<MidiCcSource, MidiCcMapping>,
}

impl MidiMappingTable {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::default()
    }


    pub fn insert(&mut self, mapping: MidiCcMapping) -> Option<MidiCcMapping> {
        self.by_source.insert(mapping.source.clone(), mapping)
    }

    #[cfg(test)]
    pub fn remove_by_instance(&mut self, instance_id: PluginInstanceId) {
        self.by_source
            .retain(|_, m| m.target.instance_id != instance_id);
    }

    #[cfg(test)]
    pub fn remove_by_device(&mut self, device_name: &str) {
        self.by_source
            .retain(|source, _| source.device_name != device_name);
    }

    #[cfg(test)]
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

    #[cfg(test)]
    pub fn conflict_check(&self, source: &MidiCcSource) -> Option<&str> {
        self.by_source.get(source).map(|m| m.label.as_str())
    }

    #[cfg(test)]
    pub fn from_mappings(mappings: Vec<MidiCcMapping>) -> Self {
        let mut table = Self::new();
        for m in mappings {
            table.insert(m);
        }
        table
    }

    pub fn remove(&mut self, source: &MidiCcSource) -> Option<MidiCcMapping> {
        self.by_source.remove(source)
    }

    pub fn get(&self, source: &MidiCcSource) -> Option<&MidiCcMapping> {
        self.by_source.get(source)
    }

    pub fn find_by_target(&self, target: &MidiCcTarget) -> Option<&MidiCcMapping> {
        self.by_source.values().find(|m| m.target == *target)
    }

    pub fn all_mappings(&self) -> Vec<&MidiCcMapping> {
        self.by_source.values().collect()
    }
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

    #[test]
    fn table_remove_by_device() {
        let mut table = MidiMappingTable::new();
        table.insert(make_mapping("device_a", Some(0), 1, 100, 0));
        table.insert(make_mapping("device_b", Some(0), 1, 200, 0));

        table.remove_by_device("device_a");

        assert_eq!(table.all_mappings().len(), 1);
        assert_eq!(table.all_mappings()[0].source.device_name, "device_b");
    }

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
        table.insert(make_mapping("dev", None, 10, 100, 0));

        let result = table.get_with_wildcard("dev", 3, 10, MidiMessageType::Cc);
        assert!(result.is_some());
        assert_eq!(result.unwrap().target.instance_id, 100);
    }

    #[test]
    fn table_get_with_wildcard_exact_preferred_over_wildcard() {
        let mut table = MidiMappingTable::new();
        table.insert(make_mapping("dev", None, 10, 100, 0));
        table.insert(make_mapping("dev", Some(5), 10, 200, 0));

        let result = table.get_with_wildcard("dev", 5, 10, MidiMessageType::Cc);
        assert_eq!(result.unwrap().target.instance_id, 200);
    }

    #[test]
    fn table_get_with_wildcard_no_match() {
        let table = MidiMappingTable::new();
        assert!(table.get_with_wildcard("dev", 0, 1, MidiMessageType::Cc).is_none());
    }

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

    #[test]
    fn table_conflict_check() {
        let mut table = MidiMappingTable::new();
        table.insert(make_mapping("d", Some(0), 1, 100, 0));

        let source = make_source("d", Some(0), 1);
        assert!(table.conflict_check(&source).is_some());

        let source2 = make_source("d", Some(0), 99);
        assert!(table.conflict_check(&source2).is_none());
    }

    #[test]
    fn table_from_mappings() {
        let mappings = vec![
            make_mapping("d", Some(0), 1, 100, 0),
            make_mapping("d", Some(0), 2, 200, 0),
        ];
        let table = MidiMappingTable::from_mappings(mappings);
        assert_eq!(table.all_mappings().len(), 2);
    }

    #[test]
    fn mapping_mode_default_is_continuous() {
        assert_eq!(MappingMode::default(), MappingMode::Continuous);
    }

    #[test]
    fn midi_message_type_default_is_cc() {
        assert_eq!(MidiMessageType::default(), MidiMessageType::Cc);
    }
}
