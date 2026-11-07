//! Persist URI references embedded in LV2 atoms independently of process-local
//! URID allocation order. Also reads LSP mixer's public KVT state for labels.
use super::{state::StateEntry, urid::UridMapper};
const ATOM: &str = "http://lv2plug.in/ns/ext/atom#";
fn word(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_ne_bytes(
        data.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}
fn padded(n: usize) -> Option<usize> {
    n.checked_add(7).map(|n| n & !7)
}

pub fn references(data: &[u8], ty: &str, mapper: &UridMapper) -> Vec<(usize, String)> {
    fn reference(data: &[u8], at: usize, mapper: &UridMapper, out: &mut Vec<(usize, String)>) {
        if let Some(id) = word(data, at).filter(|id| *id != 0) {
            if let Some(uri) = mapper.unmap(id) {
                out.push((at, uri));
            }
        }
    }
    fn body(
        data: &[u8],
        start: usize,
        end: usize,
        ty: &str,
        mapper: &UridMapper,
        out: &mut Vec<(usize, String)>,
        depth: usize,
    ) -> Option<()> {
        if depth > 32 || end > data.len() || start > end {
            return None;
        }
        let object = matches!(ty.strip_prefix(ATOM), Some("Object" | "Blank" | "Resource"));
        let tuple = ty == format!("{ATOM}Tuple");
        if ty == format!("{ATOM}URID") {
            if end - start >= 4 {
                reference(data, start, mapper, out);
            }
        }
        if !object && !tuple {
            return Some(());
        }
        let mut at = start;
        if object {
            if end - start < 8 {
                return None;
            }
            // Object ID zero denotes an anonymous object. Blank IDs are local
            // labels, while Object/Resource IDs are URI references.
            if ty != format!("{ATOM}Blank") {
                reference(data, at, mapper, out);
            }
            reference(data, at + 4, mapper, out);
            at += 8;
        }
        while at < end {
            if object {
                if end - at < 8 {
                    return None;
                }
                reference(data, at, mapper, out);
                reference(data, at + 4, mapper, out);
                at += 8;
            }
            if end - at < 8 {
                return None;
            }
            let size = word(data, at)? as usize;
            let child_type = mapper.unmap(word(data, at + 4)?)?;
            reference(data, at + 4, mapper, out);
            let child = at + 8;
            let child_end = child.checked_add(size)?;
            if child_end > end {
                return None;
            }
            body(data, child, child_end, &child_type, mapper, out, depth + 1)?;
            at = child.checked_add(padded(size)?)?;
        }
        Some(())
    }
    let mut out = Vec::new();
    if body(data, 0, data.len(), ty, mapper, &mut out, 0).is_none() {
        out.clear();
    }
    out
}

pub fn restored_bytes(entry: &StateEntry, mapper: &UridMapper) -> Vec<u8> {
    let mut bytes = entry.value.clone();
    for (offset, uri) in &entry.urid_references {
        if let Some(dest) = offset
            .checked_add(4)
            .and_then(|end| bytes.get_mut(*offset..end))
        {
            dest.copy_from_slice(&mapper.map(uri).to_ne_bytes());
        }
    }
    bytes
}

pub fn is_lsp_mixer(uri: &str) -> bool {
    let Some(name) = uri.strip_prefix("http://lsp-plug.in/plugins/lv2/mixer_x") else {
        return false;
    };
    matches!(
        name,
        "4_mono" | "4_stereo" | "8_mono" | "8_stereo" | "16_mono" | "16_stereo"
    )
}

pub fn port_label(uri: &str, entries: &[StateEntry], port: &str) -> Option<String> {
    if !is_lsp_mixer(uri) {
        return None;
    }
    // PipeWire ports retain their stable input_N names. LSP's master
    // input precedes its mixer channels in both mono and stereo layouts.
    let symbol;
    let port = if let Some(index) = port.strip_prefix("input_") {
        let index: usize = index.parse().ok()?;
        let stereo = uri.ends_with("_stereo");
        let channels = if stereo { 2 } else { 1 };
        if index < channels {
            return None;
        }
        let mixer_channel = index / channels;
        let side = if !stereo {
            ""
        } else if index % 2 == 0 {
            "l"
        } else {
            "r"
        };
        symbol = format!("in_{mixer_channel}{side}");
        symbol.as_str()
    } else {
        port
    };
    let channel = port.strip_prefix("in_")?;
    let (number, suffix) = if let Some(n) = channel.strip_suffix('l') {
        (n, " L")
    } else if let Some(n) = channel.strip_suffix('r') {
        (n, " R")
    } else {
        (channel, "")
    };
    let number: usize = number.parse().ok()?;
    if !(1..=16).contains(&number) {
        return None;
    }
    let key = format!("/channel/{number}/name");
    let state = entries
        .iter()
        .find(|s| s.key_uri == format!("{uri}/KVT") && s.type_uri == format!("{ATOM}Tuple"))?;
    let data = &state.value;
    let reference = |at| {
        state
            .urid_references
            .iter()
            .find(|(offset, _)| *offset == at)
            .map(|(_, uri)| uri.as_str())
    };
    let string = |at: usize, len: usize| -> Option<&str> {
        let bytes = data.get(at..at.checked_add(len)?)?;
        std::str::from_utf8(bytes.strip_suffix(&[0]).unwrap_or(bytes)).ok()
    };
    let mut at = 0;
    while at < data.len() {
        let size = word(data, at)? as usize;
        let start = at.checked_add(8)?;
        let end = start.checked_add(size)?;
        if end > data.len() || size < 8 {
            return None;
        }
        if reference(at + 4) == Some("http://lv2plug.in/ns/ext/atom#Object")
            && reference(start + 4).is_some_and(|uri| uri.ends_with("/KVTEntry"))
        {
            let mut property = start + 8;
            let mut name = None;
            let mut value = None;
            while property < end {
                if end - property < 16 {
                    return None;
                }
                let size = word(data, property + 8)? as usize;
                let value_start = property + 16;
                if value_start.checked_add(size)? > end {
                    return None;
                }
                if reference(property + 12) == Some("http://lv2plug.in/ns/ext/atom#String") {
                    match reference(property) {
                        Some(uri) if uri.ends_with("/KVTEntry#key") => {
                            name = string(value_start, size)
                        }
                        Some(uri) if uri.ends_with("/KVTEntry#value") => {
                            value = string(value_start, size)
                        }
                        _ => {}
                    }
                }
                property = value_start.checked_add(padded(size)?)?;
            }
            if name == Some(key.as_str()) {
                let value = value?.trim();
                if value.is_empty() || value.contains('\0') {
                    return None;
                }
                return Some(format!("{value}{suffix}"));
            }
        }
        at = start.checked_add(padded(size)?)?;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(mapper: &UridMapper) -> StateEntry {
        fn word(out: &mut Vec<u8>, value: u32) {
            out.extend(value.to_ne_bytes());
        }
        fn property(out: &mut Vec<u8>, mapper: &UridMapper, key: &str, value: &str) {
            word(out, mapper.map(key));
            word(out, 0);
            word(out, (value.len() + 1) as u32);
            word(out, mapper.map(&format!("{ATOM}String")));
            out.extend(value.as_bytes());
            out.push(0);
            while out.len() % 8 != 0 {
                out.push(0);
            }
        }
        let mut body = Vec::new();
        word(&mut body, 0);
        word(
            &mut body,
            mapper.map("http://lsp-plug.in/types/lv2/KVTEntry"),
        );
        property(
            &mut body,
            mapper,
            "http://lsp-plug.in/types/lv2/KVTEntry#key",
            "/channel/1/name",
        );
        property(
            &mut body,
            mapper,
            "http://lsp-plug.in/types/lv2/KVTEntry#value",
            "Vocals 🎤",
        );
        let mut data = Vec::new();
        word(&mut data, body.len() as u32);
        word(&mut data, mapper.map(&format!("{ATOM}Object")));
        data.extend(body);
        StateEntry {
            key_uri: "http://lsp-plug.in/plugins/lv2/mixer_x4_stereo/KVT".into(),
            type_uri: format!("{ATOM}Tuple"),
            urid_references: references(&data, &format!("{ATOM}Tuple"), mapper),
            value: data,
            flags: 3,
        }
    }
    #[test]
    fn labels_survive_json_and_different_urid_order_without_renaming_ports() {
        let mapper = UridMapper::new();
        let entry = fixture(&mapper);
        let json = serde_json::to_string(&entry).unwrap();
        let saved: StateEntry = serde_json::from_str(&json).unwrap();
        let uri = "http://lsp-plug.in/plugins/lv2/mixer_x4_stereo";
        assert_eq!(
            port_label(uri, &[saved.clone()], "input_2").as_deref(),
            Some("Vocals 🎤 L")
        );
        assert_eq!(
            port_label(uri, &[saved.clone()], "input_3").as_deref(),
            Some("Vocals 🎤 R")
        );
        assert!(port_label(uri, &[saved.clone()], "input_0").is_none());
        assert!(port_label(uri, &[saved.clone()], "output_0").is_none());
        assert!(port_label("urn:unrelated", &[saved.clone()], "input_2").is_none());
        let next = UridMapper::new();
        next.map("urn:allocated-before-plugin");
        let restored = restored_bytes(&saved, &next);
        assert_ne!(restored, saved.value);
        let refs = references(&restored, &saved.type_uri, &next);
        assert_eq!(refs, saved.urid_references);
        let resaved = StateEntry {
            value: restored,
            urid_references: refs,
            ..saved
        };
        assert_eq!(
            port_label(uri, &[resaved], "input_2").as_deref(),
            Some("Vocals 🎤 L")
        );
    }
    #[test]
    fn malformed_atom_data_does_not_panic_or_patch_arbitrary_bytes() {
        let mapper = UridMapper::new();
        let entry = fixture(&mapper);
        for end in 0..entry.value.len() {
            let mut broken = entry.clone();
            broken.value.truncate(end);
            assert!(references(&broken.value, &broken.type_uri, &mapper).is_empty());
            assert!(
                port_label(
                    "http://lsp-plug.in/plugins/lv2/mixer_x4_stereo",
                    &[broken],
                    "input_2"
                )
                .is_none()
            );
        }
    }

    #[test]
    #[ignore = "requires installed LSP Mixer x4 Stereo LV2 plugin"]
    fn installed_lsp_mixer_restores_channel_labels() {
        use crate::lv2::{host::Lv2PluginInstance, scanner, types::PluginPortType};
        use std::sync::Arc;
        let uri = "http://lsp-plug.in/plugins/lv2/mixer_x4_stereo";
        let world = lilv::World::with_load_all();
        let plugin = world
            .plugins()
            .iter()
            .find(|p| p.uri().as_uri() == Some(uri))
            .expect("installed LSP mixer");
        let info = scanner::scan_plugins_with_world(&world)
            .into_iter()
            .find(|p| p.uri == uri)
            .unwrap();
        let inputs: Vec<_> = info
            .ports
            .iter()
            .filter(|p| p.port_type == PluginPortType::AudioInput)
            .map(|p| p.symbol.as_str())
            .collect();
        assert_eq!(&inputs[..4], &["in_l", "in_r", "in_1l", "in_1r"]);
        let mut saved = vec![fixture(&UridMapper::new())];
        for cycle in 0..2 {
            let mapper = Arc::new(UridMapper::new());
            for id in 0..(31 + cycle * 19) {
                mapper.map(&format!("urn:zestbay:test:unrelated:{id}"));
            }
            let instance =
                unsafe { Lv2PluginInstance::new(&world, &plugin, &info, 48000.0, 256, &mapper) }
                    .unwrap();
            unsafe {
                instance.restore_state(&saved);
            }
            saved = unsafe { instance.save_state() }.expect("LSP state save");
            assert_eq!(
                port_label(uri, &saved, "input_2").as_deref(),
                Some("Vocals 🎤 L")
            );
            assert_eq!(
                port_label(uri, &saved, "input_3").as_deref(),
                Some("Vocals 🎤 R")
            );
            saved = serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
            drop(instance);
        }
    }
}
