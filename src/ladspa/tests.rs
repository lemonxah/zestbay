use super::{host::LadspaPluginInstance, scanner};
use crate::plugin::types::{PluginFormat, PluginInfo, PluginPortType};
use std::path::PathBuf;
use std::process::Command;

struct Fixture {
    dir: PathBuf,
    library: PathBuf,
}

impl Fixture {
    fn new(defines: &[&str]) -> Self {
        let dir = std::env::temp_dir().join(format!("zestbay-ladspa-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("fixture.c");
        let library = dir.join("fixture_ladspa.so");
        std::fs::write(&source, include_str!("test_plugin.c")).unwrap();
        let output = Command::new("cc")
            .args(["-shared", "-fPIC", "-std=c11", "-O1"])
            .args(defines.iter().map(|d| format!("-D{d}")))
            .arg(&source)
            .arg("-o")
            .arg(&library)
            .output()
            .expect("C compiler for ABI fixture");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        Self { dir, library }
    }

    fn scan(&self) -> Vec<PluginInfo> {
        let mut result = Vec::new();
        scanner::scan_ladspa_file(&self.library, &mut result);
        result
    }

    fn instance(&self, info: &PluginInfo) -> LadspaPluginInstance {
        unsafe {
            LadspaPluginInstance::new(&info.library_path, &info.uri, info, 48000.0, None)
        }
        .expect("instantiate fixture")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn run(inst: &LadspaPluginInstance, input: &[f32]) -> Vec<f32> {
    let mut out = vec![0.0; input.len()];
    unsafe { inst.process(&[input], &mut [&mut out[..]], input.len(), &[]) };
    out
}

#[test]
fn scan_reports_ports_ranges_and_uri() {
    let fx = Fixture::new(&[]);
    let found = fx.scan();
    assert_eq!(found.len(), 1);
    let info = &found[0];
    assert_eq!(info.format, PluginFormat::Ladspa);
    assert_eq!(info.uri, "ladspa:fixture_ladspa.so:test_gain");
    assert_eq!(info.name, "Test Gain");
    assert_eq!(info.author.as_deref(), Some("ZestBay"));
    assert_eq!((info.audio_inputs, info.audio_outputs), (1, 1));
    assert_eq!((info.control_inputs, info.control_outputs), (1, 1));
    assert!(info.compatible && !info.has_ui);
    let gain = &info.ports[0];
    assert_eq!(gain.port_type, PluginPortType::ControlInput);
    assert_eq!((gain.min_value, gain.max_value, gain.default_value), (0.0, 2.0, 1.0));
    assert_eq!(info.ports[3].port_type, PluginPortType::ControlOutput);
    assert_eq!(scanner::parse_label(&info.uri), Some("test_gain"));
}

#[test]
fn scan_finds_every_descriptor_in_a_library() {
    let fx = Fixture::new(&["SECOND_PLUGIN"]);
    let mut uris: Vec<_> = fx.scan().into_iter().map(|p| p.uri).collect();
    uris.sort();
    assert_eq!(
        uris,
        ["ladspa:fixture_ladspa.so:other_gain", "ladspa:fixture_ladspa.so:test_gain"]
    );
}

#[test]
fn process_applies_parameter_and_reports_control_output() {
    let fx = Fixture::new(&[]);
    let info = fx.scan().remove(0);
    let inst = fx.instance(&info);
    assert_eq!((inst.audio_input_channels, inst.audio_output_channels), (1, 1));
    assert_eq!(run(&inst, &[1.0, 0.5]), vec![1.0, 0.5]); // default gain 1
    inst.set_parameter(0, 0.5);
    assert_eq!(run(&inst, &[1.0, 0.5]), vec![0.5, 0.25]);
    assert_eq!(inst.port_updates.control_outputs[0].value.load(), 0.25);
    // Out-of-range and non-finite values are clamped / ignored.
    inst.set_parameter(0, 100.0);
    assert_eq!(inst.get_parameters()[0].value, 2.0);
    inst.set_parameter(0, f32::NAN);
    assert_eq!(inst.get_parameters()[0].value, 2.0);
    inst.set_parameter(99, 1.0); // unknown port: ignored
}

#[test]
fn bypass_passes_audio_through() {
    let fx = Fixture::new(&[]);
    let info = fx.scan().remove(0);
    let inst = fx.instance(&info);
    inst.set_parameter(0, 0.0);
    assert_eq!(run(&inst, &[0.25, 0.75]), vec![0.0, 0.0]);
    inst.set_bypassed(true);
    assert!(inst.is_bypassed());
    assert_eq!(run(&inst, &[0.25, 0.75]), vec![0.25, 0.75]);
}

#[test]
fn blocks_longer_than_scratch_are_chunked() {
    let fx = Fixture::new(&[]);
    let info = fx.scan().remove(0);
    let inst = fx.instance(&info);
    inst.set_parameter(0, 2.0);
    let n = crate::midi::rt::MAX_BLOCK * 2 + 7;
    let input: Vec<f32> = (0..n).map(|i| i as f32).collect();
    let out = run(&inst, &input);
    assert!(out.iter().zip(&input).all(|(o, i)| *o == i * 2.0));
}

#[test]
fn missing_ports_do_not_crash() {
    let fx = Fixture::new(&[]);
    let info = fx.scan().remove(0);
    let inst = fx.instance(&info);
    // No input and no output buffers from the graph.
    unsafe { inst.process(&[], &mut [], 32, &[]) };
    // Extra output channels are silenced.
    let mut a = vec![1.0; 8];
    let mut b = vec![1.0; 8];
    unsafe { inst.process(&[&[1.0; 8][..]], &mut [&mut a[..], &mut b[..]], 8, &[]) };
    assert_eq!(a, vec![1.0; 8]);
    assert_eq!(b, vec![0.0; 8]);
}

#[test]
fn unknown_label_or_bad_uri_is_rejected() {
    let fx = Fixture::new(&[]);
    let info = fx.scan().remove(0);
    let new = |uri: &str| unsafe {
        LadspaPluginInstance::new(&info.library_path, uri, &info, 48000.0, None)
    };
    assert!(new("ladspa:fixture_ladspa.so:nope").is_none());
    assert!(new("vst2:00001234:fixture_ladspa.so").is_none());
    assert!(new("ladspa:no-label").is_none());
    assert!(
        unsafe { LadspaPluginInstance::new(&info.library_path, &info.uri, &info, 0.0, None) }
            .is_none()
    );
}

#[test]
fn non_ladspa_library_scans_to_nothing() {
    let mut found = Vec::new();
    scanner::scan_ladspa_file(std::path::Path::new("/nonexistent/x.so"), &mut found);
    assert!(found.is_empty());
}
