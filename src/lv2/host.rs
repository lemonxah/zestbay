use std::collections::HashMap;
use std::ffi::c_void;
use std::os::raw::c_char;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use lilv::World;

use super::log::{Lv2LogSetup, RtScope};
use super::options::{Lv2OptionsSetup, MAX_BLOCK_LENGTH, SEQUENCE_SIZE};
use super::state::{LV2_State_Interface, Lv2StatePathSetup, StateEntry, LV2_STATE__INTERFACE};
use super::types::*;
use super::urid::UridMapper;
use super::worker::{LV2_Worker_Interface, Lv2Worker, Lv2WorkerSetup, LV2_WORKER_INTERFACE_URI};

static NEXT_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

fn next_instance_id() -> PluginInstanceId {
    NEXT_INSTANCE_ID.fetch_add(1, Ordering::Relaxed)
}

/// Sample rate of each live instance, keyed by its LV2_Handle, so UI code
/// that only receives the handle can tell plugin UIs the real rate.
static HANDLE_SAMPLE_RATES: OnceLock<Mutex<HashMap<usize, f64>>> = OnceLock::new();

fn handle_sample_rates() -> &'static Mutex<HashMap<usize, f64>> {
    HANDLE_SAMPLE_RATES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Sample rate of the live instance owning `handle`, if any.
pub fn sample_rate_for_handle(handle: *mut c_void) -> Option<f64> {
    if handle.is_null() {
        return None;
    }
    handle_sample_rates()
        .lock()
        .ok()?
        .get(&(handle as usize))
        .copied()
}

/// Clamp a value to [min, max], tolerating NaN bounds (some LV2 plugins
/// have unset min/max ranges).  If min or max is NaN, the value is returned
/// as-is.
fn safe_clamp(value: f32, min: f32, max: f32) -> f32 {
    if min.is_nan() || max.is_nan() {
        return value;
    }
    value.clamp(min, max)
}

/// Capacity of each atom port buffer in bytes (matches `buf-size#sequenceSize`).
const ATOM_BUF_SIZE: usize = SEQUENCE_SIZE;
/// Size of dummy buffers for ports of unsupported classes.
const DUMMY_PORT_BYTES: usize = 64 * 1024;
const MAX_BLOCK: usize = MAX_BLOCK_LENGTH as usize;

const LV2_RESIZE_PORT_URI: &std::ffi::CStr = c"http://lv2plug.in/ns/ext/resize-port#resize";
const LV2_URI_MAP_URI: &std::ffi::CStr = c"http://lv2plug.in/ns/ext/uri-map";

// Data-less LV2 host capability flags
const LV2_HARD_RT_CAPABLE_URI: &std::ffi::CStr = c"http://lv2plug.in/ns/lv2core#hardRTCapable";
const LV2_IS_LIVE_URI: &std::ffi::CStr = c"http://lv2plug.in/ns/lv2core#isLive";
const LV2_IN_PLACE_BROKEN_URI: &std::ffi::CStr = c"http://lv2plug.in/ns/lv2core#inPlaceBroken";

const ATOM_SEQUENCE_URI: &str = "http://lv2plug.in/ns/ext/atom#Sequence";
const ATOM_CHUNK_URI: &str = "http://lv2plug.in/ns/ext/atom#Chunk";
const MIDI_EVENT_URI: &str = "http://lv2plug.in/ns/ext/midi#MidiEvent";

#[repr(C)]
#[allow(non_camel_case_types)]
struct LV2_Resize_Port_Resize {
    data: *mut c_void,
    resize: unsafe extern "C" fn(data: *mut c_void, index: u32, size: usize) -> i32,
}

const LV2_RESIZE_PORT_ERR_NO_SPACE: i32 = 2;

unsafe extern "C" fn resize_port_stub(
    _data: *mut c_void,
    _index: u32,
    _size: usize,
) -> i32 {
    LV2_RESIZE_PORT_ERR_NO_SPACE
}

#[repr(C)]
#[allow(non_camel_case_types)]
struct LV2_URI_Map_Feature {
    callback_data: *mut c_void,
    uri_to_id: unsafe extern "C" fn(
        callback_data: *mut c_void,
        map: *const c_char,
        uri: *const c_char,
    ) -> u32,
}

unsafe extern "C" fn uri_map_callback(
    callback_data: *mut c_void,
    _map: *const c_char,
    uri: *const c_char,
) -> u32 {
    if callback_data.is_null() || uri.is_null() {
        return 0;
    }
    unsafe {
        let mapper = &*(callback_data as *const UridMapper);
        let uri_str = std::ffi::CStr::from_ptr(uri);
        mapper.map(uri_str.to_str().unwrap_or(""))
    }
}

/// Feature data the plugin may keep pointers to for its whole lifetime.
struct HostFeatureData {
    resize_port: LV2_Resize_Port_Resize,
    uri_map: LV2_URI_Map_Feature,
}

fn write_u32(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_ne_bytes());
}

fn read_u32(buf: &[u8], off: usize) -> u32 {
    u32::from_ne_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

/// Reset an input atom port to an empty `atom:Sequence` (header only).
fn init_atom_input(buf: &mut [u8], sequence_type_urid: u32) {
    assert!(buf.len() >= 16, "atom buffer too small");
    write_u32(buf, 0, 8);
    write_u32(buf, 4, sequence_type_urid);
    write_u32(buf, 8, 0);
    write_u32(buf, 12, 0);
}

/// Prepare an output atom port before `run()`: per the LV2 atom spec the
/// host sets it to an `atom:Chunk` whose size is the available capacity.
fn init_atom_output(buf: &mut [u8], chunk_type_urid: u32) {
    assert!(buf.len() >= 16, "atom buffer too small");
    write_u32(buf, 0, (buf.len() - 8) as u32);
    write_u32(buf, 4, chunk_type_urid);
}

/// Length of the atom sequence the plugin wrote to an output port, if it
/// contains at least one event.
fn produced_sequence_len(buf: &[u8], sequence_type_urid: u32) -> Option<usize> {
    if buf.len() < 16 {
        return None;
    }
    let size = read_u32(buf, 0) as usize;
    let type_ = read_u32(buf, 4);
    let total = 8 + size;
    if type_ == sequence_type_urid && size > 8 && total <= buf.len() {
        Some(total)
    } else {
        None
    }
}

/// Fill an input atom port with queued UI atoms (at frame 0) followed by
/// MIDI events.  RT-safe: no allocation, no blocking.  Returns the total
/// atom size written (header included).
fn fill_atom_input(
    buf: &mut [u8],
    sequence_type_urid: u32,
    midi_event_urid: u32,
    ui_atoms: &AtomPortBuffer,
    midi_events: &[crate::midi::processing::RawMidiEvent],
) -> usize {
    init_atom_input(buf, sequence_type_urid);
    let cap = buf.len();
    let mut write_pos = 16usize;

    // UI → plugin atoms (each an LV2_Atom: 8-byte header + body)
    while cap - write_pos >= 16 {
        match ui_atoms.pop_into_rt(&mut buf[write_pos + 8..]) {
            MsgPop::Data(len) => {
                if len < 8 {
                    continue;
                }
                let atom_total = 8 + read_u32(buf, write_pos + 8) as usize;
                if atom_total > len {
                    continue; // malformed: header claims more than was sent
                }
                buf[write_pos..write_pos + 8].copy_from_slice(&0i64.to_ne_bytes());
                let event_end = write_pos + 8 + atom_total;
                let padded_end = (write_pos + 8 + atom_total + 7) & !7;
                buf[event_end..padded_end.min(cap)].fill(0);
                write_pos = padded_end;
            }
            MsgPop::TooLarge(_) => {
                if write_pos == 16 {
                    // Can never fit this port; drop it rather than block the queue.
                    ui_atoms.discard_rt();
                    continue;
                }
                break;
            }
            MsgPop::Empty => break,
        }
    }

    // MIDI events: time_frames(8) + atom_size(4) + atom_type(4) + data(padded)
    for evt in midi_events {
        let midi_size = (evt.size as usize).min(evt.data.len());
        if midi_size == 0 {
            continue;
        }
        let padded_event_size = (16 + midi_size + 7) & !7;
        if write_pos + padded_event_size > cap {
            break;
        }
        buf[write_pos..write_pos + 8].copy_from_slice(&(evt.offset as i64).to_ne_bytes());
        write_u32(buf, write_pos + 8, midi_size as u32);
        write_u32(buf, write_pos + 12, midi_event_urid);
        buf[write_pos + 16..write_pos + 16 + midi_size].copy_from_slice(&evt.data[..midi_size]);
        buf[write_pos + 16 + midi_size..write_pos + padded_event_size].fill(0);
        write_pos += padded_event_size;
    }

    // Sequence body size = total - atom header(8)
    write_u32(buf, 0, (write_pos - 8) as u32);
    write_pos
}

/// 8-byte aligned buffer for one atom port.
pub struct AtomBuf {
    pub port_index: usize,
    data: Box<[u64]>,
}

impl AtomBuf {
    fn new(port_index: usize, bytes: usize) -> Self {
        Self {
            port_index,
            data: vec![0u64; bytes.div_ceil(8)].into_boxed_slice(),
        }
    }

    pub fn bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.data.as_ptr() as *const u8, self.data.len() * 8) }
    }

    pub fn bytes_mut(&mut self) -> &mut [u8] {
        unsafe {
            std::slice::from_raw_parts_mut(self.data.as_mut_ptr() as *mut u8, self.data.len() * 8)
        }
    }
}

pub struct Lv2PluginInstance {
    pub id: PluginInstanceId,
    // NOTE: field order is drop order.  `instance` must be dropped (plugin
    // deactivated + freed) before the feature data and port buffers it
    // points to; `Drop` stops the worker thread before any of this.
    instance: lilv::instance::ActiveInstance,
    /// Worker thread for plugins that require the worker#schedule feature
    pub worker: Option<Lv2Worker>,
    _unused_worker_setup: Option<Lv2WorkerSetup>,
    /// Keeps the lilv world alive for as long as the instance.
    _lilv_plugin: lilv::plugin::Plugin,
    _urid_map: Box<lv2_raw::urid::LV2UridMap>,
    _urid_unmap: Box<super::urid::LV2UridUnmap>,
    _log_setup: Lv2LogSetup,
    _options_setup: Lv2OptionsSetup,
    state_path_setup: Lv2StatePathSetup,
    _host_features: Box<HostFeatureData>,
    /// Port memory the plugin reads/writes (RT-private).
    control_in_mem: Box<[f32]>,
    control_out_mem: Box<[f32]>,
    dummy_audio_in: Box<[f32]>,
    dummy_audio_out: Box<[f32]>,
    _dummy_ports: Vec<Box<[u64]>>,
    pub plugin_uri: String,
    pub display_name: String,
    pub audio_input_indices: Vec<usize>,
    pub audio_output_indices: Vec<usize>,
    /// Control port metadata.  `value` is the value at instantiation; the
    /// live value is in `port_updates` (see `control_input_values()`).
    pub control_inputs: Vec<ControlPort>,
    pub control_outputs: Vec<ControlPort>,
    pub atom_in_bufs: Vec<AtomBuf>,
    pub atom_out_bufs: Vec<AtomBuf>,
    pub port_updates: SharedPortUpdates,
    atom_sequence_urid: u32,
    atom_chunk_urid: u32,
    midi_event_urid: u32,
    bypassed: AtomicBool,
    pub sample_rate: f64,
    /// Accumulated worker thread CPU time (ns) drained after each process() call
    pub last_worker_ns: u64,
    /// LV2_State_Interface pointer (if the plugin provides it)
    state_iface: Option<std::ptr::NonNull<LV2_State_Interface>>,
    /// Raw LV2_Handle for calling state save/restore
    lv2_handle: *mut c_void,
    /// Plugin descriptor's extension_data function pointer (for data-access UI feature)
    extension_data_fn: Option<unsafe extern "C" fn(*const c_char) -> *const c_void>,
    /// Shared reference to the URID mapper for state operations
    urid_mapper: Arc<UridMapper>,
}

#[derive(Debug, Clone)]
pub struct ControlPort {
    pub index: usize,
    pub symbol: String,
    pub name: String,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub is_toggle: bool,
    pub step_count: u32,
    pub is_logarithmic: bool,
}

impl Lv2PluginInstance {
    pub unsafe fn new(
        world: &lilv::World,
        plugin: &lilv::plugin::Plugin,
        plugin_info: &Lv2PluginInfo,
        sample_rate: f64,
        block_length: u32,
        urid_mapper: &Arc<UridMapper>,
    ) -> Option<Self> {
        let id = next_instance_id();
        let atom_sequence_urid = urid_mapper.map(ATOM_SEQUENCE_URI);
        let atom_chunk_urid = urid_mapper.map(ATOM_CHUNK_URI);
        let midi_event_urid = urid_mapper.map(MIDI_EVENT_URI);

        let mut urid_map = Box::new(urid_mapper.as_lv2_urid_map());
        let urid_feature = unsafe { UridMapper::make_feature(&mut *urid_map as *mut _) };
        let mut urid_unmap = Box::new(urid_mapper.as_lv2_urid_unmap());
        let urid_unmap_feature =
            unsafe { UridMapper::make_unmap_feature(&mut *urid_unmap as *mut _) };

        let log_setup = Lv2LogSetup::new(urid_mapper);
        let log_feature = log_setup.make_feature();

        let options_setup = Lv2OptionsSetup::new(urid_mapper, sample_rate, block_length);
        let options_feature = options_setup.make_feature();
        let buf_size_features = options_setup.make_buf_size_features();

        let needs_worker = plugin_info
            .required_features
            .iter()
            .any(|f| f == "http://lv2plug.in/ns/ext/worker#schedule");

        let worker_setup = if needs_worker {
            Some(Lv2WorkerSetup::new())
        } else {
            None
        };

        let state_path_setup = Lv2StatePathSetup::new(&plugin_info.uri, &format!("instance-{}", id));
        let path_features = state_path_setup.make_features();

        let mut host_features = Box::new(HostFeatureData {
            resize_port: LV2_Resize_Port_Resize {
                data: std::ptr::null_mut(),
                resize: resize_port_stub,
            },
            uri_map: LV2_URI_Map_Feature {
                callback_data: Arc::as_ptr(urid_mapper) as *mut c_void,
                uri_to_id: uri_map_callback,
            },
        });

        // lilv 0.2.4 depends on lv2_raw 0.2's LV2Feature while we use lv2_raw 0.3.
        // Both versions have an identical #[repr(C)] layout:
        //   { uri: *const c_char, data: *mut c_void }
        // The only difference is libc types vs std::os::raw types (same ABI).
        // We transmute the Vec to bridge the type boundary.
        let worker_feature;
        let resize_port_feature = lv2_raw::core::LV2Feature {
            uri: LV2_RESIZE_PORT_URI.as_ptr(),
            data: &mut host_features.resize_port as *mut _ as *mut c_void,
        };
        let uri_map_feature = lv2_raw::core::LV2Feature {
            uri: LV2_URI_MAP_URI.as_ptr(),
            data: &mut host_features.uri_map as *mut _ as *mut c_void,
        };
        let hard_rt_feature = lv2_raw::core::LV2Feature {
            uri: LV2_HARD_RT_CAPABLE_URI.as_ptr(),
            data: std::ptr::null_mut(),
        };
        let is_live_feature = lv2_raw::core::LV2Feature {
            uri: LV2_IS_LIVE_URI.as_ptr(),
            data: std::ptr::null_mut(),
        };
        let in_place_broken_feature = lv2_raw::core::LV2Feature {
            uri: LV2_IN_PLACE_BROKEN_URI.as_ptr(),
            data: std::ptr::null_mut(),
        };
        let mut features_v3: Vec<&lv2_raw::core::LV2Feature> = vec![
            &urid_feature,
            &urid_unmap_feature,
            &log_feature,
            &options_feature,
            &resize_port_feature,
            &uri_map_feature,
            &hard_rt_feature,
            &is_live_feature,
            &in_place_broken_feature,
        ];
        for f in &path_features {
            features_v3.push(f);
        }
        for f in &buf_size_features {
            features_v3.push(f);
        }
        if let Some(ref ws) = worker_setup {
            worker_feature = ws.make_feature();
            features_v3.push(&worker_feature);
        }
        let features = unsafe {
            // SAFETY: lv2_raw 0.2 and 0.3 LV2Feature are layout-identical #[repr(C)] structs.
            std::mem::transmute::<Vec<&lv2_raw::core::LV2Feature>, Vec<_>>(features_v3)
        };

        log::debug!("LV2 new: calling plugin.instantiate()");
        let mut instance = unsafe { plugin.instantiate(sample_rate, features) }?;
        log::debug!("LV2 new: instantiate() returned OK");

        let mut audio_input_indices = Vec::new();
        let mut audio_output_indices = Vec::new();
        let mut control_inputs = Vec::new();
        let mut control_outputs = Vec::new();
        let mut atom_in_bufs = Vec::new();
        let mut atom_out_bufs = Vec::new();

        for port_info in &plugin_info.ports {
            match port_info.port_type {
                Lv2PortType::AudioInput => {
                    audio_input_indices.push(port_info.index);
                }
                Lv2PortType::AudioOutput => {
                    audio_output_indices.push(port_info.index);
                }
                Lv2PortType::ControlInput => {
                    let default = super::scanner::resolve_control_default(
                        port_info.default_value,
                        port_info.min_value,
                        port_info.max_value,
                    );
                    control_inputs.push(ControlPort {
                        index: port_info.index,
                        symbol: port_info.symbol.clone(),
                        name: port_info.name.clone(),
                        value: default,
                        min: port_info.min_value,
                        max: port_info.max_value,
                        default,
                        is_toggle: port_info.is_toggle,
                        step_count: port_info.step_count,
                        is_logarithmic: port_info.is_logarithmic,
                    });
                }
                Lv2PortType::ControlOutput => {
                    control_outputs.push(ControlPort {
                        index: port_info.index,
                        symbol: port_info.symbol.clone(),
                        name: port_info.name.clone(),
                        value: 0.0,
                        min: port_info.min_value,
                        max: port_info.max_value,
                        default: port_info.default_value,
                        is_toggle: false,
                        step_count: 0,
                        is_logarithmic: false,
                    });
                }
                Lv2PortType::AtomInput => {
                    atom_in_bufs.push(AtomBuf::new(port_info.index, ATOM_BUF_SIZE));
                }
                Lv2PortType::AtomOutput => {
                    atom_out_bufs.push(AtomBuf::new(port_info.index, ATOM_BUF_SIZE));
                }
            }
        }

        let mut control_in_mem: Box<[f32]> = control_inputs.iter().map(|cp| cp.value).collect();
        let mut control_out_mem: Box<[f32]> = vec![0.0f32; control_outputs.len()].into();

        log::debug!(
            "LV2 new: connecting control ports ({} in, {} out)",
            control_inputs.len(),
            control_outputs.len()
        );
        for (cp, mem) in control_inputs.iter().zip(control_in_mem.iter_mut()) {
            unsafe {
                instance.connect_port_mut(cp.index, mem as *mut f32);
            }
        }
        for (cp, mem) in control_outputs.iter().zip(control_out_mem.iter_mut()) {
            unsafe {
                instance.connect_port_mut(cp.index, mem as *mut f32);
            }
        }

        // Connect audio ports to persistent dummy buffers so that ALL ports
        // are connected before activate(), as required by the LV2 spec.
        // process() reconnects them to real buffers each cycle and falls
        // back to these when the filter supplies fewer buffers.
        let mut dummy_audio_in: Box<[f32]> = vec![0.0f32; MAX_BLOCK].into();
        let mut dummy_audio_out: Box<[f32]> = vec![0.0f32; MAX_BLOCK].into();
        for &idx in &audio_input_indices {
            unsafe {
                instance.connect_port_mut(idx, dummy_audio_in.as_mut_ptr());
            }
        }
        for &idx in &audio_output_indices {
            unsafe {
                instance.connect_port_mut(idx, dummy_audio_out.as_mut_ptr());
            }
        }

        for ab in atom_in_bufs.iter_mut() {
            init_atom_input(ab.bytes_mut(), atom_sequence_urid);
            unsafe {
                instance.connect_port_mut(ab.port_index, ab.bytes_mut().as_mut_ptr());
            }
        }
        for ab in atom_out_bufs.iter_mut() {
            init_atom_output(ab.bytes_mut(), atom_chunk_urid);
            unsafe {
                instance.connect_port_mut(ab.port_index, ab.bytes_mut().as_mut_ptr());
            }
        }

        // Ports of classes we don't otherwise handle still get valid memory
        // because run() may touch every port.
        let known: std::collections::HashSet<usize> =
            plugin_info.ports.iter().map(|p| p.index).collect();
        let control_class = world.new_uri("http://lv2plug.in/ns/lv2core#ControlPort");
        let mut dummy_ports: Vec<Box<[u64]>> = Vec::new();
        for idx in 0..plugin.ports_count() {
            if known.contains(&idx) {
                continue;
            }
            let is_control = plugin
                .port_by_index(idx)
                .is_some_and(|p| p.is_a(&control_class));
            let mut buf: Box<[u64]> = if is_control {
                vec![0u64; 1].into()
            } else {
                vec![0u64; DUMMY_PORT_BYTES / 8].into()
            };
            log::info!(
                "LV2 '{}': connecting unsupported port {} to a dummy {} buffer",
                plugin_info.name,
                idx,
                if is_control { "control" } else { "data" }
            );
            unsafe {
                instance.connect_port_mut(idx, buf.as_mut_ptr());
            }
            dummy_ports.push(buf);
        }
        log::debug!("LV2 new: all ports connected");

        let port_updates = Arc::new(PortUpdates {
            control_inputs: control_inputs
                .iter()
                .map(|cp| PortSlot {
                    port_index: cp.index,
                    value: AtomicF32::new(cp.value),
                })
                .collect(),
            control_outputs: control_outputs
                .iter()
                .map(|cp| PortSlot {
                    port_index: cp.index,
                    value: AtomicF32::new(cp.value),
                })
                .collect(),
            atom_outputs: atom_out_bufs
                .iter()
                .map(|ab| AtomPortBuffer::new(ab.port_index))
                .collect(),
            atom_inputs: atom_in_bufs
                .iter()
                .map(|ab| AtomPortBuffer::new(ab.port_index))
                .collect(),
        });

        // Activate the worker if needed: get the instance handle and worker
        // interface BEFORE activation (extension_data is available on the
        // un-activated instance).
        let lv2_handle = instance.handle() as *mut c_void;

        // lv2_raw 0.2 declares `extension_data` as non-nullable
        // `extern "C" fn(...)`, but the LV2 C spec allows it to be NULL
        // ("If a plugin does not support any extension data, this field
        // may be NULL").  Reading a null through a non-nullable fn type
        // is UB, so we read the raw pointer value first.
        let has_extension_data = instance.descriptor().map_or(false, |d| {
            let raw: *const () =
                unsafe { *(std::ptr::addr_of!(d.extension_data) as *const *const ()) };
            !raw.is_null()
        });

        let extension_data_fn: Option<unsafe extern "C" fn(*const c_char) -> *const c_void> =
            if has_extension_data {
                instance
                    .descriptor()
                    .map(|d| unsafe { std::mem::transmute(d.extension_data) })
            } else {
                None
            };

        let mut unused_worker_setup = None;
        let worker = if let Some(ws) = worker_setup {
            let worker_iface_ptr: Option<std::ptr::NonNull<LV2_Worker_Interface>> =
                if has_extension_data {
                    unsafe {
                        instance
                            .extension_data::<LV2_Worker_Interface>(LV2_WORKER_INTERFACE_URI)
                    }
                } else {
                    None
                };
            match worker_iface_ptr {
                Some(iface_ptr) => {
                    log::info!("LV2 worker: activating for plugin '{}'", plugin_info.name);
                    Some(unsafe { ws.activate(lv2_handle, iface_ptr.as_ptr()) })
                }
                None => {
                    log::warn!(
                        "LV2 worker: plugin '{}' requires worker but provides no interface",
                        plugin_info.name
                    );
                    // Keep the schedule feature data alive: the plugin may
                    // have stored a pointer to it.  Requests are never processed.
                    unused_worker_setup = Some(ws);
                    None
                }
            }
        } else {
            None
        };

        let state_iface: Option<std::ptr::NonNull<LV2_State_Interface>> = if has_extension_data {
            unsafe { instance.extension_data::<LV2_State_Interface>(LV2_STATE__INTERFACE) }
        } else {
            None
        };
        if state_iface.is_some() {
            log::info!(
                "LV2 state: plugin '{}' provides state interface",
                plugin_info.name
            );
        }

        log::debug!("LV2 new: calling activate()");
        let active_instance = unsafe { instance.activate() };
        log::debug!("LV2 new: activate() completed");

        if let Ok(mut rates) = handle_sample_rates().lock() {
            rates.insert(lv2_handle as usize, sample_rate);
        }

        Some(Self {
            id,
            instance: active_instance,
            worker,
            _unused_worker_setup: unused_worker_setup,
            _lilv_plugin: plugin.clone(),
            _urid_map: urid_map,
            _urid_unmap: urid_unmap,
            _log_setup: log_setup,
            _options_setup: options_setup,
            state_path_setup,
            _host_features: host_features,
            control_in_mem,
            control_out_mem,
            dummy_audio_in,
            dummy_audio_out,
            _dummy_ports: dummy_ports,
            plugin_uri: plugin_info.uri.clone(),
            display_name: plugin_info.name.clone(),
            audio_input_indices,
            audio_output_indices,
            control_inputs,
            control_outputs,
            atom_in_bufs,
            atom_out_bufs,
            port_updates,
            atom_sequence_urid,
            atom_chunk_urid,
            midi_event_urid,
            bypassed: AtomicBool::new(false),
            sample_rate,
            last_worker_ns: 0,
            state_iface,
            lv2_handle,
            extension_data_fn,
            urid_mapper: urid_mapper.clone(),
        })
    }

    pub unsafe fn reconnect_control_ports(&mut self) {
        for (cp, mem) in self.control_inputs.iter().zip(self.control_in_mem.iter_mut()) {
            unsafe {
                self.instance
                    .instance_mut()
                    .connect_port_mut(cp.index, mem as *mut f32);
            }
        }
        for (cp, mem) in self.control_outputs.iter().zip(self.control_out_mem.iter_mut()) {
            unsafe {
                self.instance
                    .instance_mut()
                    .connect_port_mut(cp.index, mem as *mut f32);
            }
        }
    }

    /// Run one block.  RT-safe: no allocation, locking or I/O.
    /// `sample_count` must not exceed `MAX_BLOCK_LENGTH` (it is clamped).
    pub unsafe fn process(
        &mut self,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        sample_count: usize,
        midi_events: &[crate::midi::processing::RawMidiEvent],
    ) {
        let sample_count = sample_count.min(MAX_BLOCK);

        // Connect audio ports (buffer pointers change each cycle).  Ports
        // without a (large enough) buffer use the persistent dummies.
        for (i, &port_idx) in self.audio_input_indices.iter().enumerate() {
            let ptr = match inputs.get(i) {
                Some(buf) if buf.len() >= sample_count => buf.as_ptr(),
                _ => self.dummy_audio_in.as_ptr(),
            };
            unsafe {
                self.instance.instance_mut().connect_port(port_idx, ptr);
            }
        }

        for (i, &port_idx) in self.audio_output_indices.iter().enumerate() {
            let ptr = match outputs.get_mut(i) {
                Some(buf) if buf.len() >= sample_count => buf.as_mut_ptr(),
                _ => self.dummy_audio_out.as_mut_ptr(),
            };
            unsafe {
                self.instance.instance_mut().connect_port_mut(port_idx, ptr);
            }
        }

        // Apply host-side parameter changes (set_parameter, MIDI CC, UI):
        // the shared slot is authoritative for control inputs.
        for (mem, slot) in self
            .control_in_mem
            .iter_mut()
            .zip(self.port_updates.control_inputs.iter())
        {
            *mem = slot.value.load();
        }

        // Prepare atom input buffers (UI → plugin communication + MIDI events)
        for (ab, shared) in self
            .atom_in_bufs
            .iter_mut()
            .zip(self.port_updates.atom_inputs.iter())
        {
            fill_atom_input(
                ab.bytes_mut(),
                self.atom_sequence_urid,
                self.midi_event_urid,
                shared,
                midi_events,
            );
        }

        for ab in &mut self.atom_out_bufs {
            init_atom_output(ab.bytes_mut(), self.atom_chunk_urid);
        }

        {
            let _rt = RtScope::enter();

            // Always call run() so the plugin keeps its internal state alive
            // (visualizers, worker threads, etc.). When bypassed we just
            // overwrite the audio output with a passthrough copy afterwards.
            unsafe {
                self.instance.run(sample_count);
            }

            // Deliver any pending worker responses after run()
            // and drain accumulated worker thread CPU time for stats
            self.last_worker_ns = 0;
            if let Some(worker) = self.worker.as_mut() {
                unsafe {
                    worker.deliver_responses();
                }
                self.last_worker_ns = worker.drain_worker_ns();
            }
        }

        // When bypassed, overwrite plugin audio output with passthrough
        if self.bypassed.load(Ordering::Relaxed) {
            for (i, output) in outputs.iter_mut().enumerate() {
                if i < inputs.len() {
                    let copy_len = output.len().min(inputs[i].len()).min(sample_count);
                    output[..copy_len].copy_from_slice(&inputs[i][..copy_len]);
                } else {
                    for sample in output.iter_mut().take(sample_count) {
                        *sample = 0.0;
                    }
                }
            }
        }

        // Only outputs are written back: the plugin owns their values.
        for (mem, slot) in self
            .control_out_mem
            .iter()
            .zip(self.port_updates.control_outputs.iter())
        {
            slot.value.store(*mem);
        }

        for (ab, shared) in self
            .atom_out_bufs
            .iter()
            .zip(self.port_updates.atom_outputs.iter())
        {
            if !shared.is_listening() {
                continue;
            }
            let bytes = ab.bytes();
            if let Some(total) = produced_sequence_len(bytes, self.atom_sequence_urid) {
                shared.write_rt(&bytes[..total]);
            }
        }
    }

    fn control_input_slot(&self, idx: usize) -> Option<&PortSlot> {
        self.port_updates.control_inputs.get(idx)
    }

    /// Set a control input.  Only stores into the shared atomic slot; the
    /// RT thread applies it at the start of the next block.
    pub fn set_parameter(&self, port_index: usize, value: f32) {
        if value.is_nan() {
            return;
        }
        if let Some(idx) = self
            .control_inputs
            .iter()
            .position(|cp| cp.index == port_index)
        {
            let cp = &self.control_inputs[idx];
            let clamped = safe_clamp(value, cp.min, cp.max);
            if let Some(slot) = self.control_input_slot(idx) {
                slot.value.store(clamped);
            }
        }
    }

    pub fn set_parameter_by_symbol(&self, symbol: &str, value: f32) {
        if value.is_nan() {
            return;
        }
        if let Some(idx) = self
            .control_inputs
            .iter()
            .position(|cp| cp.symbol == symbol)
        {
            let cp = &self.control_inputs[idx];
            let clamped = safe_clamp(value, cp.min, cp.max);
            if let Some(slot) = self.control_input_slot(idx) {
                slot.value.store(clamped);
            }
        }
    }

    /// Current control input values `(port_index, value)` from the shared slots.
    pub fn control_input_values(&self) -> Vec<(usize, f32)> {
        self.port_updates
            .control_inputs
            .iter()
            .map(|s| (s.port_index, s.value.load()))
            .collect()
    }

    /// Scope makePath/mapPath files to this persistent instance identity.
    /// Must be called before `restore_state`.
    pub fn set_state_scope(&mut self, stable_id: &str) {
        if stable_id.is_empty() {
            return;
        }
        self.state_path_setup.set_scope(stable_id);
    }

    pub fn set_bypassed(&self, bypassed: bool) {
        self.bypassed.store(bypassed, Ordering::Relaxed);
    }

    pub fn is_bypassed(&self) -> bool {
        self.bypassed.load(Ordering::Relaxed)
    }

    pub fn get_parameters(&self) -> Vec<Lv2ParameterValue> {
        self.control_inputs
            .iter()
            .enumerate()
            .map(|(i, cp)| Lv2ParameterValue {
                port_index: cp.index,
                symbol: cp.symbol.clone(),
                name: cp.name.clone(),
                value: self
                    .control_input_slot(i)
                    .map_or(cp.value, |s| s.value.load()),
                min: cp.min,
                max: cp.max,
                default: cp.default,
                is_toggle: cp.is_toggle,
            })
            .collect()
    }

    pub fn get_info(&self, pw_node_id: Option<u32>) -> Lv2InstanceInfo {
        Lv2InstanceInfo {
            id: self.id,
            stable_id: String::new(),
            plugin_uri: self.plugin_uri.clone(),
            format: PluginFormat::Lv2,
            display_name: self.display_name.clone(),
            pw_node_id,
            parameters: self.get_parameters(),
            active: true,
            bypassed: self.is_bypassed(),
            lv2_state: Vec::new(),
            plugin_state: None,
        }
    }

    pub fn lv2_handle_ptr(&self) -> *mut c_void {
        self.lv2_handle
    }

    pub fn extension_data_fn(
        &self,
    ) -> Option<unsafe extern "C" fn(*const c_char) -> *const c_void> {
        self.extension_data_fn
    }

    pub fn has_state_interface(&self) -> bool {
        self.state_iface.is_some()
    }

    pub unsafe fn save_state(&self) -> Option<Vec<StateEntry>> {
        let iface = self.state_iface?;
        unsafe {
            super::state::save_plugin_state(
                self.lv2_handle,
                iface.as_ptr(),
                &self.urid_mapper,
                &self.state_path_setup,
            )
        }
    }

    pub unsafe fn restore_state(&self, entries: &[StateEntry]) {
        let Some(iface) = self.state_iface else {
            return;
        };
        unsafe {
            super::state::restore_plugin_state(
                self.lv2_handle,
                iface.as_ptr(),
                &self.urid_mapper,
                entries,
                &self.state_path_setup,
            );
        }
    }
}

impl Drop for Lv2PluginInstance {
    fn drop(&mut self) {
        // The worker thread calls into the plugin; it must be gone before
        // the instance is deactivated and freed (fields drop after this).
        if let Some(worker) = self.worker.as_mut() {
            worker.stop();
        }
        if let Ok(mut rates) = handle_sample_rates().lock() {
            rates.remove(&(self.lv2_handle as usize));
        }
    }
}

pub struct Lv2Manager {
    world: World,
    available_plugins: Vec<Lv2PluginInfo>,
    active_instances: HashMap<PluginInstanceId, Lv2InstanceInfo>,
    pub sample_rate: f64,
}

impl Lv2Manager {
    pub fn new() -> Self {
        let world = World::with_load_all();
        let available_plugins = super::scanner::scan_plugins_with_world(&world);

        log::info!("LV2: Found {} plugins", available_plugins.len());

        Self {
            world,
            available_plugins,
            active_instances: HashMap::new(),
            sample_rate: 48000.0,
        }
    }

    pub fn available_plugins(&self) -> &[Lv2PluginInfo] {
        &self.available_plugins
    }

    pub fn rescan(&mut self) {
        self.available_plugins = super::scanner::scan_plugins_with_world(&self.world);
        log::info!(
            "LV2: Rescanned, found {} plugins",
            self.available_plugins.len()
        );
    }

    pub fn find_plugin(&self, uri: &str) -> Option<&Lv2PluginInfo> {
        self.available_plugins.iter().find(|p| p.uri == uri)
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn register_instance(&mut self, info: Lv2InstanceInfo) {
        self.active_instances.insert(info.id, info);
    }

    pub fn set_instance_pw_node_id(&mut self, instance_id: PluginInstanceId, pw_node_id: u32) {
        if let Some(info) = self.active_instances.get_mut(&instance_id) {
            info.pw_node_id = Some(pw_node_id);
        }
    }

    pub fn remove_instance(&mut self, instance_id: PluginInstanceId) {
        self.active_instances.remove(&instance_id);
    }

    pub fn update_parameter(
        &mut self,
        instance_id: PluginInstanceId,
        port_index: usize,
        value: f32,
    ) {
        if let Some(info) = self.active_instances.get_mut(&instance_id) {
            if let Some(param) = info
                .parameters
                .iter_mut()
                .find(|p| p.port_index == port_index)
            {
                param.value = value;
            } else {
                info.parameters.push(super::types::Lv2ParameterValue {
                    port_index,
                    symbol: String::new(),
                    name: String::new(),
                    value,
                    min: 0.0,
                    max: 1.0,
                    default: 0.0,
                    is_toggle: false,
                });
            }
        }
    }

    pub fn active_instances(&self) -> &HashMap<PluginInstanceId, Lv2InstanceInfo> {
        &self.active_instances
    }

    pub fn get_instance(&self, id: PluginInstanceId) -> Option<&Lv2InstanceInfo> {
        self.active_instances.get(&id)
    }

    pub fn get_instance_mut(&mut self, id: PluginInstanceId) -> Option<&mut Lv2InstanceInfo> {
        self.active_instances.get_mut(&id)
    }

    pub fn find_by_stable_id(&self, stable_id: &str) -> Option<&Lv2InstanceInfo> {
        self.active_instances
            .values()
            .find(|info| info.stable_id == stable_id)
    }

    pub fn find_by_stable_id_mut(&mut self, stable_id: &str) -> Option<&mut Lv2InstanceInfo> {
        self.active_instances
            .values_mut()
            .find(|info| info.stable_id == stable_id)
    }

    pub fn instance_id_for_stable_id(&self, stable_id: &str) -> Option<PluginInstanceId> {
        self.active_instances
            .iter()
            .find(|(_, info)| info.stable_id == stable_id)
            .map(|(id, _)| *id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::processing::RawMidiEvent;

    // ---- safe_clamp ----

    #[test]
    fn safe_clamp_normal() {
        assert_eq!(safe_clamp(0.5, 0.0, 1.0), 0.5);
        assert_eq!(safe_clamp(-1.0, 0.0, 1.0), 0.0);
        assert_eq!(safe_clamp(2.0, 0.0, 1.0), 1.0);
    }

    #[test]
    fn safe_clamp_at_bounds() {
        assert_eq!(safe_clamp(0.0, 0.0, 1.0), 0.0);
        assert_eq!(safe_clamp(1.0, 0.0, 1.0), 1.0);
    }

    #[test]
    fn safe_clamp_nan_min_returns_value() {
        let result = safe_clamp(0.5, f32::NAN, 1.0);
        assert_eq!(result, 0.5);
    }

    #[test]
    fn safe_clamp_nan_max_returns_value() {
        let result = safe_clamp(0.5, 0.0, f32::NAN);
        assert_eq!(result, 0.5);
    }

    #[test]
    fn safe_clamp_both_nan_returns_value() {
        let result = safe_clamp(42.0, f32::NAN, f32::NAN);
        assert_eq!(result, 42.0);
    }

    #[test]
    fn safe_clamp_negative_range() {
        assert_eq!(safe_clamp(-5.0, -10.0, -1.0), -5.0);
        assert_eq!(safe_clamp(-15.0, -10.0, -1.0), -10.0);
        assert_eq!(safe_clamp(0.0, -10.0, -1.0), -1.0);
    }

    // ---- atom port helpers ----

    #[test]
    fn init_atom_input_is_empty_sequence() {
        let mut buf = vec![0xFFu8; 64];
        init_atom_input(&mut buf, 42);
        assert_eq!(read_u32(&buf, 0), 8);
        assert_eq!(read_u32(&buf, 4), 42);
        assert_eq!(read_u32(&buf, 8), 0);
        assert_eq!(read_u32(&buf, 12), 0);
    }

    #[test]
    fn init_atom_output_is_full_chunk() {
        let mut buf = vec![0xFFu8; 64];
        init_atom_output(&mut buf, 99);
        assert_eq!(read_u32(&buf, 0), 56);
        assert_eq!(read_u32(&buf, 4), 99);
    }

    #[test]
    #[should_panic(expected = "atom buffer too small")]
    fn init_atom_input_too_small() {
        let mut buf = vec![0u8; 8];
        init_atom_input(&mut buf, 1);
    }

    #[test]
    fn produced_sequence_requires_events() {
        let mut buf = vec![0u8; 64];
        // Untouched chunk → nothing produced
        init_atom_output(&mut buf, 7);
        assert_eq!(produced_sequence_len(&buf, 5), None);
        // Empty sequence → nothing produced
        init_atom_input(&mut buf, 5);
        assert_eq!(produced_sequence_len(&buf, 5), None);
        // Sequence with one 8-byte event body
        write_u32(&mut buf, 0, 8 + 24);
        assert_eq!(produced_sequence_len(&buf, 5), Some(40));
        // Size larger than the buffer → rejected
        write_u32(&mut buf, 0, 1000);
        assert_eq!(produced_sequence_len(&buf, 5), None);
    }

    fn make_atom(type_: u32, body: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&(body.len() as u32).to_ne_bytes());
        v.extend_from_slice(&type_.to_ne_bytes());
        v.extend_from_slice(body);
        v
    }

    #[test]
    fn fill_atom_input_queues_all_ui_atoms_then_midi() {
        let ui = AtomPortBuffer::new(0);
        ui.write(&make_atom(77, &[1, 2, 3]));
        ui.write(&make_atom(78, &[4, 5, 6, 7, 8, 9, 10, 11]));
        let midi = [RawMidiEvent {
            offset: 5,
            data: [0x90, 60, 100],
            size: 3,
        }];
        let mut buf = vec![0u8; 256];
        let total = fill_atom_input(&mut buf, 1, 2, &ui, &midi);

        // header(16) + ev1(8+8+3 → 24) + ev2(8+8+8 → 24) + midi(8+8+3 → 24)
        assert_eq!(total, 16 + 24 + 24 + 24);
        assert_eq!(read_u32(&buf, 0) as usize, total - 8);
        assert_eq!(read_u32(&buf, 4), 1);
        // event 1
        assert_eq!(read_u32(&buf, 24), 3);
        assert_eq!(read_u32(&buf, 28), 77);
        assert_eq!(&buf[32..35], &[1, 2, 3]);
        assert_eq!(&buf[35..40], &[0, 0, 0, 0, 0]);
        // event 2
        assert_eq!(read_u32(&buf, 48), 8);
        assert_eq!(read_u32(&buf, 52), 78);
        // midi event
        assert_eq!(i64::from_ne_bytes(buf[64..72].try_into().unwrap()), 5);
        assert_eq!(read_u32(&buf, 72), 3);
        assert_eq!(read_u32(&buf, 76), 2);
        assert_eq!(&buf[80..83], &[0x90, 60, 100]);
        // queue drained
        assert_eq!(ui.read(), None);
    }

    #[test]
    fn fill_atom_input_leaves_overflow_queued() {
        let ui = AtomPortBuffer::new(0);
        ui.write(&make_atom(1, &[0u8; 16]));
        ui.write(&make_atom(1, &[0u8; 16]));
        // room for header + one 32-byte event only
        let mut buf = vec![0u8; 16 + 32 + 8];
        fill_atom_input(&mut buf, 1, 2, &ui, &[]);
        assert_eq!(read_u32(&buf, 0), 8 + 32);
        // second message still queued for the next cycle
        assert!(ui.read().is_some());
    }

    #[test]
    fn fill_atom_input_drops_message_that_can_never_fit() {
        let ui = AtomPortBuffer::new(0);
        ui.write(&make_atom(1, &[0u8; 200]));
        ui.write(&make_atom(1, &[9u8; 4]));
        let mut buf = vec![0u8; 64];
        fill_atom_input(&mut buf, 1, 2, &ui, &[]);
        assert_eq!(read_u32(&buf, 0), 8 + 24);
        assert_eq!(&buf[32..36], &[9, 9, 9, 9]);
    }

    #[test]
    fn atom_buf_is_aligned() {
        let ab = AtomBuf::new(0, 100);
        assert_eq!(ab.bytes().as_ptr() as usize % 8, 0);
        assert_eq!(ab.bytes().len(), 104);
    }
}
