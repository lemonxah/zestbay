use super::*;

fn mapping(instance_id: u64, port_index: usize, cc: u8) -> crate::midi::MidiCcMapping {
    crate::midi::MidiCcMapping {
        source: crate::midi::MidiCcSource {
            device_name: "dev".into(),
            channel: Some(0),
            cc,
            message_type: crate::midi::MidiMessageType::Cc,
        },
        target: crate::midi::MidiCcTarget {
            instance_id,
            port_index,
        },
        mode: crate::midi::MappingMode::Continuous,
        label: format!("p{}", port_index),
    }
}

fn link(a: &str, b: &str) -> SavedPluginLink {
    SavedPluginLink {
        output_node_name: a.into(),
        output_port_name: "out".into(),
        input_node_name: b.into(),
        input_port_name: "in".into(),
    }
}

#[test]
fn midi_mappings_resolve_by_stable_id() {
    let saved = vec![
        SavedMidiMapping {
            mapping: mapping(7, 1, 10),
            stable_id: "a".into(),
        },
        SavedMidiMapping {
            mapping: mapping(3, 2, 11),
            stable_id: "gone".into(),
        },
    ];
    let map: HashMap<String, u64> = [("a".to_string(), 42u64)].into_iter().collect();
    let out = resolve_saved_midi_mappings(saved, &map, &[42]);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].target.instance_id, 42);
    assert_eq!(out[0].target.port_index, 1);
}

#[test]
fn restore_safety_cap_scales_with_batch() {
    assert_eq!(restore_safety_cap(0), Duration::from_secs(30));
    assert_eq!(restore_safety_cap(1), Duration::from_secs(45));
    assert_eq!(restore_safety_cap(20), Duration::from_secs(330));
}

#[test]
fn legacy_midi_mappings_use_restore_order() {
    let json = r#"[
        {"source":{"device_name":"dev","channel":0,"cc":1},"target":{"instance_id":2,"port_index":5},"mode":"Continuous","label":"x"},
        {"source":{"device_name":"dev","channel":0,"cc":2},"target":{"instance_id":9,"port_index":5},"mode":"Continuous","label":"y"}
    ]"#;
    let saved: Vec<SavedMidiMapping> = serde_json::from_str(json).unwrap();
    assert!(saved.iter().all(|s| s.stable_id.is_empty()));
    let out = resolve_saved_midi_mappings(saved, &HashMap::new(), &[100, 101]);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].target.instance_id, 101);
}

#[test]
fn saved_midi_mapping_roundtrip_keeps_stable_id() {
    let s = SavedMidiMapping {
        mapping: mapping(1, 2, 3),
        stable_id: "sid".into(),
    };
    let json = serde_json::to_string(&s).unwrap();
    let back: SavedMidiMapping = serde_json::from_str(&json).unwrap();
    assert_eq!(back.stable_id, "sid");
    assert_eq!(back.mapping.target.port_index, 2);
}

#[test]
fn plugin_state_base64_roundtrip() {
    use base64::Engine;
    let blob = vec![0u8, 1, 2, 255];
    let sp = SavedPlugin {
        stable_id: "s".into(),
        uri: "u".into(),
        display_name: "d".into(),
        bypassed: false,
        enabled: true,
        parameters: Vec::new(),
        format: "CLAP".into(),
        lv2_state: Vec::new(),
        plugin_state: Some(base64::engine::general_purpose::STANDARD.encode(&blob)),
        scan_info: None,
        port_names: Default::default(),
    };
    let json = serde_json::to_string(&sp).unwrap();
    let back: SavedPlugin = serde_json::from_str(&json).unwrap();
    assert_eq!(back.decode_plugin_state(), Some(blob));
    let legacy: SavedPlugin = serde_json::from_str(r#"{"uri":"u","display_name":"d"}"#).unwrap();
    assert_eq!(legacy.decode_plugin_state(), None);
    assert!(legacy.scan_info.is_none());
    assert!(legacy.usable_scan_info().is_none());
}

#[test]
fn old_config_gains_a_direct_start_descriptor_without_losing_state() {
    use crate::plugin::{PluginCategory, PluginFormat, PluginInfo, PluginInstanceInfo};
    for format in [PluginFormat::Clap, PluginFormat::Vst2] {
        let mut old: SavedPlugin = serde_json::from_str(
            r#"{
            "stable_id":"stable", "uri":"test.instrument", "display_name":"My synth",
            "format":"CLAP", "bypassed":true, "plugin_state":"AAEC/w==",
            "parameters":[{"port_index":3,"symbol":"gain","value":0.7}]
        }"#,
        )
        .unwrap();
        old.format = format.as_str().into();
        assert!(old.usable_scan_info().is_none());
        let library = std::env::temp_dir().join(format!("zestbay-{}.clap", uuid::Uuid::new_v4()));
        std::fs::write(&library, "fixture").unwrap();
        let discovered = PluginInfo {
            uri: old.uri.clone(),
            name: "Synth".into(),
            format,
            category: PluginCategory::Instrument,
            author: None,
            ports: Vec::new(),
            audio_inputs: 0,
            audio_outputs: 2,
            control_inputs: 0,
            control_outputs: 0,
            required_features: Vec::new(),
            compatible: true,
            has_ui: true,
            library_path: library.to_str().unwrap().into(),
            ladspa_ids: Vec::new(),
        };
        let mut manager = PluginManager::new();
        manager.register_instance(PluginInstanceInfo {
            id: 1,
            stable_id: old.stable_id.clone(),
            plugin_uri: old.uri.clone(),
            format,
            scan_info: None,
            display_name: old.display_name.clone(),
            pw_node_id: None,
            parameters: vec![crate::plugin::ParameterValue {
                port_index: 3,
                symbol: "gain".into(),
                name: "Gain".into(),
                value: 0.7,
                min: 0.0,
                max: 1.0,
                default: 0.5,
                is_toggle: false,
            }],
            active: true,
            bypassed: old.bypassed,
            enabled: old.enabled,
            lv2_state: Vec::new(),
            plugin_state: old.decode_plugin_state(),
            port_names: Default::default(),
        });
        assert!(!manager.startup_complete());
        manager.set_available_plugins(vec![discovered.clone()]);
        assert!(!manager.startup_complete());
        manager.finish_startup();
        let saved = SavedPlugin::from(manager.get_instance(1).unwrap());
        let json = serde_json::to_string(&saved).unwrap();
        let next_start: SavedPlugin = serde_json::from_str(&json).unwrap();
        let info = next_start
            .usable_scan_info()
            .expect("next start can bypass discovery");
        assert_eq!(info.library_path, discovered.library_path);
        assert_eq!(info.category, PluginCategory::Instrument);
        assert_eq!(next_start.stable_id, old.stable_id);
        assert_eq!(next_start.decode_plugin_state(), old.decode_plugin_state());
        assert!(next_start.bypassed);
        assert!(next_start.enabled);
        assert_eq!(next_start.parameters[0].value, 0.7);

        let mut mismatched = next_start.clone();
        mismatched.uri = "another.plugin".into();
        assert!(mismatched.usable_scan_info().is_none());
        mismatched.uri = next_start.uri.clone();
        mismatched.format = "VST3".into();
        assert!(mismatched.usable_scan_info().is_none());

        std::fs::remove_file(&library).unwrap();
        assert!(
            next_start.usable_scan_info().is_none(),
            "missing paths must wait for discovery"
        );
        let mut relocated = discovered.clone();
        relocated.library_path = "/new/location/synth.clap".into();
        manager.set_available_plugins(vec![relocated.clone()]);
        assert_eq!(
            SavedPlugin::from(manager.get_instance(1).unwrap())
                .scan_info
                .unwrap()
                .library_path,
            relocated.library_path
        );
    }
}

#[test]
fn merge_links_remembers_links_of_absent_nodes() {
    let forgotten = std::collections::HashSet::new();
    let mut pending = Vec::new();
    let last = vec![link("A", "B"), link("A", "C")];
    let current = vec![];
    let present = |n: &str| n != "B";
    let merged = merge_links_for_persist(&current, &last, &mut pending, &forgotten, present);
    assert_eq!(merged, vec![link("A", "B")]);
    assert_eq!(pending.len(), 1);

    let current = vec![link("A", "B")];
    let merged = merge_links_for_persist(&current, &[], &mut pending, &forgotten, |_| true);
    assert_eq!(merged, vec![link("A", "B")]);
    assert!(pending.is_empty());
}

#[test]
fn merge_links_drops_forgotten_plugins() {
    let forgotten: std::collections::HashSet<String> = ["P".to_string()].into_iter().collect();
    let mut pending = vec![
        PendingLink::new(link("P", "X")),
        PendingLink::new(link("Y", "X")),
    ];
    let merged =
        merge_links_for_persist(&[], &[link("P", "Z")], &mut pending, &forgotten, |_| false);
    assert_eq!(merged, vec![link("Y", "X")]);
}

#[test]
fn backup_stem_parsing_is_panic_free() {
    assert_eq!(
        parse_backup_stem("20240102_030405_My rules"),
        (
            Some("2024-01-02 03:04:05".to_string()),
            "My rules".to_string()
        )
    );
    assert_eq!(
        parse_backup_stem("20240102_030405"),
        (Some("2024-01-02 03:04:05".to_string()), String::new())
    );
    assert_eq!(
        parse_backup_stem("ééééééééééééééééé"),
        (None, String::new())
    );
    assert_eq!(
        parse_backup_stem("20240102_03040é_x"),
        (None, String::new())
    );
    assert_eq!(
        parse_backup_stem("20240102_030405_é"),
        (Some("2024-01-02 03:04:05".to_string()), "é".to_string())
    );
    assert_eq!(parse_backup_stem("short"), (None, String::new()));
}

#[test]
fn plain_file_names_only() {
    assert!(is_plain_file_name("20240101_000000.json"));
    assert!(!is_plain_file_name("../rules.json"));
    assert!(!is_plain_file_name("a/b.json"));
    assert!(!is_plain_file_name(".."));
    assert!(!is_plain_file_name(""));
}

#[test]
fn atomic_write_replaces_file() {
    let dir = std::env::temp_dir().join(format!("zestbay-test-{}", std::process::id()));
    let path = dir.join("x.json");
    write_atomic(&path, b"[1]").unwrap();
    write_atomic(&path, b"[2]").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "[2]");
    let entries = std::fs::read_dir(&dir).unwrap().count();
    assert_eq!(entries, 1);
    let _ = std::fs::remove_dir_all(&dir);
}
