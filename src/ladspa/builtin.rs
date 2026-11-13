//! ZestBay's built-in meters: LADSPA plugins compiled into ZestBay itself,
//! so they run through the ordinary LADSPA host (drop-on-wire, removal,
//! saving, subgraphs) without a library on disk.
//!
//! "Meter N" has N audio inputs passed straight through to N audio outputs,
//! and N control outputs carrying each channel's level: the block peak, or
//! the previous level falling at 20 dB/s, whichever is higher. The graph
//! draws these levels on the node; there is no plugin UI.
//!
//! They are not in the plugin catalog (the browser never lists them); the
//! graph's context menu adds them by URI.

use std::collections::HashMap;
use std::ffi::{CString, c_int, c_ulong};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use super::abi::*;
use crate::plugin::types::{PluginInfo, SharedPortUpdates};

/// Stand-in library path: `Library::open` serves the built-in descriptors
/// for it instead of loading a file.
pub const LIBRARY_PATH: &str = "zestbay-builtin";

/// Channel counts on offer: 1 to 6, and 8 (7.1).
pub const CHANNEL_COUNTS: [usize; 7] = [1, 2, 3, 4, 5, 6, 8];

/// The widest meter.
pub const MAX_CHANNELS: usize = 8;

/// How far a level falls per second once the signal drops, in dB.
const FALL_DB_PER_SECOND: f32 = 20.0;

/// Levels are linear amplitude (1.0 = 0 dBFS); anything this loud or worse
/// (inf) is reported as this.
const MAX_LEVEL: f32 = 16.0;

/// `ladspa:zestbay-builtin:meter-N`.
pub fn meter_uri(channels: usize) -> String {
    format!("ladspa:{LIBRARY_PATH}:{}", meter_label(channels))
}

fn meter_label(channels: usize) -> String {
    format!("meter-{channels}")
}

/// The channel count of a built-in meter URI.
pub fn meter_channels(uri: &str) -> Option<usize> {
    let n: usize = uri
        .strip_prefix(&format!("ladspa:{LIBRARY_PATH}:meter-"))?
        .parse()
        .ok()?;
    CHANNEL_COUNTS.contains(&n).then_some(n)
}

pub fn is_builtin_uri(uri: &str) -> bool {
    uri.starts_with(&format!("ladspa:{LIBRARY_PATH}:"))
}

pub fn is_builtin_library(path: &Path) -> bool {
    path == Path::new(LIBRARY_PATH)
}

// ---------------------------------------------------------------------------
// Descriptors
// ---------------------------------------------------------------------------

/// One meter's descriptor and the strings and arrays it points into.
struct BuiltMeter {
    desc: Descriptor,
    _label: CString,
    _name: CString,
    _maker: CString,
    _copyright: CString,
    _port_descriptors: Vec<c_int>,
    _port_names: Vec<CString>,
    _port_name_ptrs: Vec<*const std::ffi::c_char>,
    _hints: Vec<PortRangeHint>,
}

/// Built once, never freed or moved again: every pointer in a descriptor
/// (into the strings and arrays, all on the heap) stays valid for the life
/// of the process.
struct Builtins(Vec<BuiltMeter>);
// SAFETY: the descriptors are immutable after construction and only point
// at data owned by (and living as long as) this value.
unsafe impl Send for Builtins {}
unsafe impl Sync for Builtins {}

fn builtins() -> &'static Builtins {
    static BUILTINS: OnceLock<Builtins> = OnceLock::new();
    BUILTINS.get_or_init(|| Builtins(CHANNEL_COUNTS.iter().map(|&n| build_meter(n)).collect()))
}

fn build_meter(channels: usize) -> BuiltMeter {
    let label = CString::new(meter_label(channels)).unwrap_or_default();
    let name = CString::new(meter_name(channels)).unwrap_or_default();
    let maker = CString::new("ZestBay").unwrap_or_default();
    let copyright = CString::new("MIT").unwrap_or_default();

    // Ports: inputs, then outputs, then one level per channel.
    let mut port_descriptors = Vec::with_capacity(channels * 3);
    let mut port_names = Vec::with_capacity(channels * 3);
    let mut hints = Vec::with_capacity(channels * 3);
    for (flags, prefix) in [
        (PORT_INPUT | PORT_AUDIO, "In"),
        (PORT_OUTPUT | PORT_AUDIO, "Out"),
        (PORT_OUTPUT | PORT_CONTROL, "Level"),
    ] {
        for ch in 1..=channels {
            port_descriptors.push(flags);
            port_names.push(CString::new(format!("{prefix} {ch}")).unwrap_or_default());
            hints.push(PortRangeHint {
                hint: if flags & PORT_CONTROL != 0 { HINT_BOUNDED_BELOW | HINT_BOUNDED_ABOVE } else { 0 },
                lower: 0.0,
                upper: MAX_LEVEL,
            });
        }
    }
    let port_name_ptrs: Vec<*const std::ffi::c_char> = port_names.iter().map(|s| s.as_ptr()).collect();

    BuiltMeter {
        desc: Descriptor {
            unique_id: 0,
            label: label.as_ptr(),
            properties: 0,
            name: name.as_ptr(),
            maker: maker.as_ptr(),
            copyright: copyright.as_ptr(),
            port_count: port_descriptors.len() as c_ulong,
            port_descriptors: port_descriptors.as_ptr(),
            port_names: port_name_ptrs.as_ptr(),
            port_range_hints: hints.as_ptr(),
            implementation_data: channels as *mut std::ffi::c_void,
            instantiate: Some(meter_instantiate),
            connect_port: Some(meter_connect_port),
            activate: None,
            run: Some(meter_run),
            run_adding: None,
            set_run_adding_gain: None,
            deactivate: None,
            cleanup: Some(meter_cleanup),
        },
        _label: label,
        _name: name,
        _maker: maker,
        _copyright: copyright,
        _port_descriptors: port_descriptors,
        _port_names: port_names,
        _port_name_ptrs: port_name_ptrs,
        _hints: hints,
    }
}

fn meter_name(channels: usize) -> String {
    format!("Meter ({channels} ch)")
}

/// The `ladspa_descriptor` entry point of the built-in "library".
pub unsafe extern "C" fn descriptor(index: c_ulong) -> *const Descriptor {
    builtins()
        .0
        .get(index as usize)
        .map_or(std::ptr::null(), |m| &m.desc as *const Descriptor)
}

/// Catalog entry for a built-in URI (not listed in the browser).
pub fn info(uri: &str) -> Option<&'static PluginInfo> {
    static INFOS: OnceLock<Vec<PluginInfo>> = OnceLock::new();
    let infos = INFOS.get_or_init(|| {
        builtins()
            .0
            .iter()
            .filter_map(|m| unsafe { super::scanner::describe(Path::new(LIBRARY_PATH), &m.desc) })
            .map(|mut info| {
                info.category = crate::plugin::types::PluginCategory::Analyser;
                info
            })
            .collect()
    });
    infos.iter().find(|i| i.uri == uri)
}

// ---------------------------------------------------------------------------
// DSP
// ---------------------------------------------------------------------------

struct MeterState {
    channels: usize,
    sample_rate: f32,
    inputs: [*const f32; MAX_CHANNELS],
    outputs: [*mut f32; MAX_CHANNELS],
    levels: [*mut f32; MAX_CHANNELS],
    level: [f32; MAX_CHANNELS],
}

unsafe extern "C" fn meter_instantiate(desc: *const Descriptor, sample_rate: c_ulong) -> Handle {
    if desc.is_null() || sample_rate == 0 {
        return std::ptr::null_mut();
    }
    let channels = unsafe { (*desc).implementation_data } as usize;
    if !CHANNEL_COUNTS.contains(&channels) {
        return std::ptr::null_mut();
    }
    Box::into_raw(Box::new(MeterState {
        channels,
        sample_rate: sample_rate as f32,
        inputs: [std::ptr::null(); MAX_CHANNELS],
        outputs: [std::ptr::null_mut(); MAX_CHANNELS],
        levels: [std::ptr::null_mut(); MAX_CHANNELS],
        level: [0.0; MAX_CHANNELS],
    })) as Handle
}

unsafe extern "C" fn meter_connect_port(handle: Handle, port: c_ulong, data: *mut f32) {
    if handle.is_null() {
        return;
    }
    let s = unsafe { &mut *(handle as *mut MeterState) };
    let port = port as usize;
    let n = s.channels;
    match port / n {
        0 => s.inputs[port] = data,
        1 => s.outputs[port - n] = data,
        2 if port < 3 * n => s.levels[port - 2 * n] = data,
        _ => {}
    }
}

unsafe extern "C" fn meter_run(handle: Handle, frames: c_ulong) {
    if handle.is_null() {
        return;
    }
    let s = unsafe { &mut *(handle as *mut MeterState) };
    let frames = frames as usize;
    // 20 dB/s in amplitude: 10^(-20/20 * seconds).
    let fall = 10f32.powf(-FALL_DB_PER_SECOND / 20.0 * frames as f32 / s.sample_rate);
    for ch in 0..s.channels {
        let (input, output) = (s.inputs[ch], s.outputs[ch]);
        let mut peak = 0.0f32;
        if !input.is_null() && frames > 0 {
            let src = unsafe { std::slice::from_raw_parts(input, frames) };
            for &x in src {
                peak = peak.max(x.abs());
            }
            if !output.is_null() && !std::ptr::eq(output, input) {
                unsafe { std::ptr::copy_nonoverlapping(input, output, frames) };
            }
        } else if !output.is_null() && frames > 0 {
            unsafe { std::ptr::write_bytes(output, 0, frames) };
        }
        let level = peak.max(s.level[ch] * fall).min(MAX_LEVEL);
        s.level[ch] = if level.is_finite() { level } else { MAX_LEVEL };
        if !s.levels[ch].is_null() {
            unsafe { *s.levels[ch] = s.level[ch] };
        }
    }
}

unsafe extern "C" fn meter_cleanup(handle: Handle) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle as *mut MeterState) });
    }
}

// ---------------------------------------------------------------------------
// Levels for the UI
// ---------------------------------------------------------------------------

/// Running meters by instance id: their port slots, whose control outputs
/// the audio thread updates every cycle (atomics, no locking there).
static RUNNING: Mutex<Option<HashMap<u64, SharedPortUpdates>>> = Mutex::new(None);

/// Called on the PipeWire thread when a meter instance starts.
pub fn register(instance_id: u64, updates: SharedPortUpdates) {
    let mut guard = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
    guard.get_or_insert_with(HashMap::new).insert(instance_id, updates);
}

/// Called on the PipeWire thread when a meter instance goes away.
pub fn unregister(instance_id: u64) {
    let mut guard = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(map) = guard.as_mut() {
        map.remove(&instance_id);
    }
}

/// Every running meter's current levels (linear, one per channel).
pub fn levels() -> Vec<(u64, Vec<f32>)> {
    let guard = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .as_ref()
        .map(|map| {
            map.iter()
                .map(|(id, updates)| (*id, updates.control_outputs.iter().map(|s| s.value.load()).collect()))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris_round_trip() {
        for n in CHANNEL_COUNTS {
            let uri = meter_uri(n);
            assert_eq!(meter_channels(&uri), Some(n));
            assert!(is_builtin_uri(&uri));
            let info = info(&uri).expect("built-in info");
            assert_eq!((info.audio_inputs, info.audio_outputs, info.control_outputs), (n, n, n));
            assert_eq!(info.library_path, LIBRARY_PATH);
        }
        assert_eq!(meter_channels(&meter_uri(7)), None);
        assert_eq!(meter_channels(&meter_uri(9)), None);
        assert!(info(&meter_uri(8)).is_some());
        assert_eq!(meter_channels("ladspa:other.so:meter-2"), None);
    }

    #[test]
    fn host_runs_builtin_meter() {
        let uri = meter_uri(2);
        let info = info(&uri).expect("built-in info");
        let inst = unsafe {
            crate::ladspa::host::LadspaPluginInstance::new(LIBRARY_PATH, &uri, info, 48000.0, None)
        }
        .expect("host instantiates the built-in meter");
        assert_eq!((inst.audio_input_channels, inst.audio_output_channels), (2, 2));
        assert!(inst.params.is_empty());
        let a = [0.25f32; 64];
        let b = [-0.5f32; 64];
        let mut oa = [0.0f32; 64];
        let mut ob = [0.0f32; 64];
        unsafe { inst.process(&[&a, &b], &mut [&mut oa, &mut ob], 64, &[]) };
        assert_eq!(oa, a);
        assert_eq!(ob, b);
        let levels: Vec<f32> = inst.port_updates.control_outputs.iter().map(|s| s.value.load()).collect();
        assert_eq!(levels, vec![0.25, 0.5]);
    }

    #[test]
    fn meter_passes_audio_and_reports_peak_then_falls() {
        unsafe {
            let desc = descriptor(1); // 2 channels
            let d = &*desc;
            let h = (d.instantiate.unwrap())(desc, 48000);
            let input_l = [0.5f32, -0.8, 0.1, 0.0];
            let input_r = [0.0f32; 4];
            let mut out_l = [0.0f32; 4];
            let mut out_r = [1.0f32; 4];
            let (mut lvl_l, mut lvl_r) = (0.0f32, 0.0f32);
            let connect = d.connect_port.unwrap();
            connect(h, 0, input_l.as_ptr() as *mut f32);
            connect(h, 1, input_r.as_ptr() as *mut f32);
            connect(h, 2, out_l.as_mut_ptr());
            connect(h, 3, out_r.as_mut_ptr());
            connect(h, 4, &mut lvl_l);
            connect(h, 5, &mut lvl_r);
            (d.run.unwrap())(h, 4);
            assert_eq!(out_l, input_l);
            assert_eq!(out_r, input_r);
            assert_eq!(lvl_l, 0.8);
            assert_eq!(lvl_r, 0.0);

            // One second of silence: 20 dB down (x0.1).
            let silent = vec![0.0f32; 48000];
            let mut out = vec![0.0f32; 48000];
            connect(h, 0, silent.as_ptr() as *mut f32);
            connect(h, 2, out.as_mut_ptr());
            connect(h, 1, silent.as_ptr() as *mut f32);
            connect(h, 3, out.as_mut_ptr());
            (d.run.unwrap())(h, 48000);
            assert!((lvl_l - 0.08).abs() < 1e-4, "level after 1 s: {lvl_l}");
            (d.cleanup.unwrap())(h);
        }
    }
}
