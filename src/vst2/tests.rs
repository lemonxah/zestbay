use super::{abi, host::Vst2PluginInstance, scanner};
use crate::midi::processing::RawMidiEvent;
use crate::plugin::types::{PluginFormat, PluginInfo};
use std::path::PathBuf;
use std::process::Command;

struct Fixture {
    dir: PathBuf,
    library: PathBuf,
}

impl Fixture {
    fn new(defines: &[&str]) -> Self {
        let dir = std::env::temp_dir().join(format!("zestbay-vst2-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("fixture.c");
        let library = dir.join("fixture.so");
        std::fs::write(&source, include_str!("test_plugin.c")).unwrap();
        let output = Command::new("cc")
            .args(["-shared", "-fPIC", "-std=c11", "-O1"])
            .args(defines.iter().map(|d| format!("-D{d}")))
            .arg(&source)
            .arg("-o")
            .arg(&library)
            .output()
            .expect("C compiler for ABI fixture");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Self { dir, library }
    }

    fn scan(&self) -> Vec<PluginInfo> {
        let mut result = Vec::new();
        scanner::scan_vst2_file(&self.library, &mut result);
        result
    }

    fn load(&self, info: &PluginInfo, state: Option<&[u8]>) -> Vst2PluginInstance {
        unsafe {
            Vst2PluginInstance::new(
                self.library.to_str().unwrap(),
                &info.uri,
                info,
                48000.0,
                state,
            )
        }
        .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn scan_process_parameters_midi_bypass_and_chunk_restore() {
    let fixture = Fixture::new(&[]);
    let plugins = fixture.scan();
    assert_eq!(plugins.len(), 1);
    let info = &plugins[0];
    assert_eq!(info.format, PluginFormat::Vst2);
    assert_eq!(
        (info.audio_inputs, info.audio_outputs, info.control_inputs),
        (2, 2, 2)
    );
    assert!(info.has_ui && info.compatible);
    assert_eq!(info.author.as_deref(), Some("ZestBay tests"));
    let inst = fixture.load(info, None);
    assert!(inst.has_midi_in);
    assert!(inst.params[1].is_toggle);
    inst.set_parameter(0, 0.25);
    let input = [1.0f32; 16];
    let mut output = [9.0f32; 16];
    let mut missing = [9.0f32; 16];
    unsafe {
        inst.process(&[&input], &mut [&mut output, &mut missing], 16, &[]);
    }
    assert_eq!(output, [0.25; 16]);
    assert_eq!(missing, [0.0; 16]);
    let midi = [RawMidiEvent {
        offset: 7,
        data: [0x90, 60, 100],
        size: 3,
    }];
    unsafe {
        inst.process(&[&input], &mut [&mut output], 16, &midi);
    }
    assert_eq!(&output[..7], &[0.25; 7]);
    assert_eq!(&output[7..], &[0.5; 9]);
    inst.set_bypassed(true);
    unsafe {
        inst.process(&[&input], &mut [&mut output, &mut missing], 16, &[]);
    }
    assert_eq!(output, input);
    assert_eq!(missing, [0.0; 16]);
    let state = inst.get_state().unwrap();
    drop(inst);
    let restored = fixture.load(info, Some(&state));
    assert_eq!(restored.get_parameters()[0].value, 0.25);
    unsafe {
        restored.process(&[&input], &mut [&mut output], 16, &[]);
    }
    assert_eq!(output, [0.25; 16]);
    // Deserialization must not route a VST2 saved descriptor through LV2.
    let json = serde_json::to_vec(info).unwrap();
    let saved: PluginInfo = serde_json::from_slice(&json).unwrap();
    assert_eq!(saved.format, PluginFormat::Vst2);
    assert_eq!(saved.uri, info.uri);
}

#[test]
fn legacy_main_accumulating_process_and_parameter_state() {
    let fixture = Fixture::new(&["LEGACY", "NO_CHUNKS"]);
    let info = fixture.scan().remove(0);
    let inst = fixture.load(&info, None);
    inst.set_parameter(0, 0.75);
    // Saving before any audio must flush pending parameter changes.
    let state = inst.get_state().unwrap();
    drop(inst);
    let inst = fixture.load(&info, Some(&state));
    let input = [1.0; 32];
    let mut output = [9.0; 32];
    for _ in 0..2 {
        unsafe {
            inst.process(&[&input], &mut [&mut output], 32, &[]);
        }
        assert_eq!(output, [0.75; 32]);
    }
}

#[test]
fn shell_children_have_distinct_restorable_ids() {
    let fixture = Fixture::new(&["SHELL"]);
    let plugins = fixture.scan();
    assert_eq!(plugins.len(), 2);
    assert_ne!(plugins[0].uri, plugins[1].uri);
    for info in plugins {
        let inst = fixture.load(&info, None);
        let state = inst.get_state().unwrap();
        drop(inst);
        drop(fixture.load(&info, Some(&state)));
    }
}

#[test]
fn invalid_layout_and_identity_are_rejected() {
    let bad = Fixture::new(&["INVALID"]);
    assert!(bad.scan().is_empty());
    let fixture = Fixture::new(&[]);
    let info = fixture.scan().remove(0);
    assert!(
        unsafe {
            Vst2PluginInstance::new(
                &info.library_path,
                "vst2:00000001:fixture.so",
                &info,
                48000.0,
                None,
            )
        }
        .is_none()
    );
    assert!(
        unsafe {
            Vst2PluginInstance::new(
                &info.library_path,
                &info.uri,
                &info,
                48000.0,
                Some(b"bad state"),
            )
        }
        .is_none()
    );
    assert!(scanner::parse_id("VST3").is_none());
}

#[test]
fn idle_leaves_running_audio_alone() {
    let fixture = Fixture::new(&[]);
    let info = fixture.scan().remove(0);
    let inst = fixture.load(&info, None);
    let input = [1.0f32; 8];
    let mut output = [0.0f32; 8];
    let mut run = |inst: &Vst2PluginInstance| {
        unsafe {
            inst.process(&[&input], &mut [&mut output], 8, &[]);
        }
        output
    };
    run(&inst);
    // The plugin changes a value itself and asks for a display update while
    // audio is running: the value is published without suspending audio.
    unsafe {
        abi::dispatch(inst.effect_ptr(), 1002, 0, 0, std::ptr::null_mut(), 0.4);
    }
    inst.idle();
    assert_eq!(inst.get_parameters()[0].value, 0.4);
    assert_eq!(run(&inst), [0.4; 8]);
    // A host edit that has not reached the plugin yet wins over the
    // plugin's own value, as it does when audio is idle.
    inst.set_parameter(0, 0.2);
    unsafe {
        abi::dispatch(inst.effect_ptr(), 1002, 0, 0, std::ptr::null_mut(), 0.9);
    }
    inst.idle();
    assert_eq!(inst.get_parameters()[0].value, 0.2);
    assert_eq!(run(&inst), [0.2; 8]);
}

#[test]
fn host_callbacks_sync_gui_edits_and_park_changed_layouts() {
    let fixture = Fixture::new(&[]);
    let info = fixture.scan().remove(0);
    let inst = fixture.load(&info, None);
    unsafe {
        abi::dispatch(inst.effect_ptr(), 1000, 0, 0, std::ptr::null_mut(), 0.6);
    }
    assert_eq!(inst.get_parameters()[0].value, 0.6);
    inst.idle();
    unsafe {
        abi::dispatch(inst.effect_ptr(), 1002, 0, 0, std::ptr::null_mut(), 0.4);
    }
    inst.idle();
    assert_eq!(inst.get_parameters()[0].value, 0.4);
    unsafe {
        abi::command(inst.effect_ptr(), 1001, 0);
    }
    assert!(inst.needs_restart());
    assert!(!inst.restart());
    assert!(inst.is_parked());
    let mut output = [0.0; 4];
    unsafe {
        inst.process(&[&[1.0; 4]], &mut [&mut output], 4, &[]);
    }
    assert_eq!(output, [1.0; 4]);
    assert!(inst.get_state().is_some());
    assert!(inst.is_parked());
}

#[test]
#[ignore = "requires X11; run under xvfb-run"]
fn native_editor_reopens_and_closes_before_unload() {
    let fixture = Fixture::new(&[]);
    let info = fixture.scan().remove(0);
    let inst = fixture.load(&info, None);
    let (events, rx) = std::sync::mpsc::channel();
    let (commands, _) = std::sync::mpsc::channel();
    for _ in 0..2 {
        unsafe {
            super::ui::open_vst2_gui(inst.effect_ptr(), 42, "VST2 test", &events, &commands);
        }
        super::ui::idle();
        assert!(matches!(
            rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap(),
            crate::pipewire::PwEvent::Plugin(crate::pipewire::PluginEvent::PluginUiOpened {
                instance_id: 42
            })
        ));
        super::ui::close_vst2_gui(42, &events);
        assert!(matches!(
            rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap(),
            crate::pipewire::PwEvent::Plugin(crate::pipewire::PluginEvent::PluginUiClosed {
                instance_id: 42
            })
        ));
    }
    unsafe {
        super::ui::open_vst2_gui(inst.effect_ptr(), 42, "VST2 test", &events, &commands);
    }
    drop(inst); // fixture asserts that EDIT_CLOSE precedes CLOSE
}

#[test]
#[ignore = "uses a user-selected installed plugin; set ZESTBAY_TEST_VST2"]
fn installed_plugin_processes_and_restores() {
    let path = std::env::var("ZESTBAY_TEST_VST2").expect("plugin path");
    let mut plugins = Vec::new();
    scanner::scan_vst2_file(std::path::Path::new(&path), &mut plugins);
    assert!(!plugins.is_empty());
    for info in plugins {
        let inst =
            unsafe { Vst2PluginInstance::new(&path, &info.uri, &info, 48000.0, None) }.unwrap();
        let input = vec![vec![0.01f32; 256]; inst.audio_input_channels];
        let mut output = vec![vec![0.0f32; 256]; inst.audio_output_channels];
        let inputs: Vec<&[f32]> = input.iter().map(Vec::as_slice).collect();
        let mut outputs: Vec<&mut [f32]> = output.iter_mut().map(Vec::as_mut_slice).collect();
        for _ in 0..10 {
            unsafe {
                inst.process(&inputs, &mut outputs, 256, &[]);
            }
        }
        assert!(output.iter().flatten().all(|x| x.is_finite()));
        let state = inst.get_state().unwrap();
        drop(inst);
        let restored =
            unsafe { Vst2PluginInstance::new(&path, &info.uri, &info, 48000.0, Some(&state)) }
                .unwrap();
        drop(restored);
    }
}
