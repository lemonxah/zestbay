use std::collections::HashMap;
use std::ffi::c_void;
use std::os::raw::c_char;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

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

static HANDLE_SAMPLE_RATES: OnceLock<Mutex<HashMap<usize, f64>>> = OnceLock::new();

fn handle_sample_rates() -> &'static Mutex<HashMap<usize, f64>> {
    HANDLE_SAMPLE_RATES.get_or_init(|| Mutex::new(HashMap::new()))
}

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

fn safe_clamp(value: f32, min: f32, max: f32) -> f32 {
    if min.is_nan() || max.is_nan() {
        return value;
    }
    value.clamp(min, max)
}

const ATOM_BUF_SIZE: usize = SEQUENCE_SIZE;
const DUMMY_PORT_BYTES: usize = 64 * 1024;
const MAX_BLOCK: usize = MAX_BLOCK_LENGTH as usize;

const LV2_RESIZE_PORT_URI: &std::ffi::CStr = c"http://lv2plug.in/ns/ext/resize-port#resize";
const LV2_URI_MAP_URI: &std::ffi::CStr = c"http://lv2plug.in/ns/ext/uri-map";

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

fn init_atom_input(buf: &mut [u8], sequence_type_urid: u32) {
    assert!(buf.len() >= 16, "atom buffer too small");
    write_u32(buf, 0, 8);
    write_u32(buf, 4, sequence_type_urid);
    write_u32(buf, 8, 0);
    write_u32(buf, 12, 0);
}

fn init_atom_output(buf: &mut [u8], chunk_type_urid: u32) {
    assert!(buf.len() >= 16, "atom buffer too small");
    write_u32(buf, 0, (buf.len() - 8) as u32);
    write_u32(buf, 4, chunk_type_urid);
}

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

fn for_each_midi_event(
    buf: &[u8],
    sequence_type_urid: u32,
    midi_event_urid: u32,
    mut f: impl FnMut(u32, &[u8]),
) {
    let Some(total) = produced_sequence_len(buf, sequence_type_urid) else {
        return;
    };
    let mut pos = 16;
    while pos + 16 <= total {
        let frames = i64::from_ne_bytes(buf[pos..pos + 8].try_into().unwrap());
        let size = read_u32(buf, pos + 8) as usize;
        let type_ = read_u32(buf, pos + 12);
        let body = pos + 16;
        if size > total - body {
            break;
        }
        if type_ == midi_event_urid && size > 0 {
            f(frames.clamp(0, u32::MAX as i64) as u32, &buf[body..body + size]);
        }
        pos = (body + size + 7) & !7;
    }
}

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

    while cap - write_pos >= 16 {
        match ui_atoms.pop_into_rt(&mut buf[write_pos + 8..]) {
            MsgPop::Data(len) => {
                if len < 8 {
                    continue;
                }
                let atom_total = 8 + read_u32(buf, write_pos + 8) as usize;
                if atom_total > len {
                    continue;
                }
                buf[write_pos..write_pos + 8].copy_from_slice(&0i64.to_ne_bytes());
                let event_end = write_pos + 8 + atom_total;
                let padded_end = (write_pos + 8 + atom_total + 7) & !7;
                buf[event_end..padded_end.min(cap)].fill(0);
                write_pos = padded_end;
            }
            MsgPop::TooLarge(_) => {
                if write_pos == 16 {
                    ui_atoms.discard_rt();
                    continue;
                }
                break;
            }
            MsgPop::Empty => break,
        }
    }

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

    write_u32(buf, 0, (write_pos - 8) as u32);
    write_pos
}

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
    #[allow(dead_code)]
    pub id: PluginInstanceId,
    instance: lilv::instance::ActiveInstance,
    pub worker: Option<Lv2Worker>,
    _unused_worker_setup: Option<Lv2WorkerSetup>,
    _lilv_plugin: lilv::plugin::Plugin,
    _urid_map: Box<lv2_raw::urid::LV2UridMap>,
    _urid_unmap: Box<super::urid::LV2UridUnmap>,
    _log_setup: Lv2LogSetup,
    _options_setup: Lv2OptionsSetup,
    state_path_setup: Lv2StatePathSetup,
    _host_features: Box<HostFeatureData>,
    control_in_mem: Box<[f32]>,
    control_out_mem: Box<[f32]>,
    dummy_audio_in: Box<[f32]>,
    dummy_audio_out: Box<[f32]>,
    _dummy_ports: Vec<Box<[u64]>>,
    pub plugin_uri: String,
    #[allow(dead_code)]
    pub display_name: String,
    pub audio_input_indices: Vec<usize>,
    pub audio_output_indices: Vec<usize>,
    pub control_inputs: Vec<ControlPort>,
    #[allow(dead_code)]
    pub control_outputs: Vec<ControlPort>,
    pub atom_in_bufs: Vec<AtomBuf>,
    pub atom_out_bufs: Vec<AtomBuf>,
    pub midi_output_names: Vec<String>,
    midi_out_slots: Vec<usize>,
    pub port_updates: SharedPortUpdates,
    atom_sequence_urid: u32,
    atom_chunk_urid: u32,
    midi_event_urid: u32,
    bypassed: AtomicBool,
    pub sample_rate: f64,
    pub last_worker_ns: u64,
    state_iface: Option<std::ptr::NonNull<LV2_State_Interface>>,
    lv2_handle: *mut c_void,
    extension_data_fn: Option<unsafe extern "C" fn(*const c_char) -> *const c_void>,
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

        let worker_required = plugin_info
            .required_features
            .iter()
            .any(|f| f == "http://lv2plug.in/ns/ext/worker#schedule");

        let worker_setup = Some(Lv2WorkerSetup::new());

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
        let mut midi_output_names = Vec::new();
        let mut midi_out_slots = Vec::new();
        let midi_event_node = world.new_uri(MIDI_EVENT_URI);

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
                    let carries_midi = plugin
                        .port_by_index(port_info.index)
                        .is_some_and(|port| port.supports_event(&midi_event_node));
                    if carries_midi {
                        midi_out_slots.push(atom_out_bufs.len());
                        midi_output_names.push(port_info.name.clone());
                    }
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

        let lv2_handle = instance.handle();

        let has_extension_data = instance.descriptor().is_some_and(|d| {
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
                    if worker_required {
                        log::warn!(
                            "LV2 worker: plugin '{}' requires worker but provides no interface",
                            plugin_info.name
                        );
                    }
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
            midi_output_names,
            midi_out_slots,
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

    pub unsafe fn process(
        &mut self,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        sample_count: usize,
        midi_events: &[crate::midi::processing::RawMidiEvent],
        midi_out: &mut crate::midi::processing::MidiSink,
    ) {
        let sample_count = sample_count.min(MAX_BLOCK);

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

        for (mem, slot) in self
            .control_in_mem
            .iter_mut()
            .zip(self.port_updates.control_inputs.iter())
        {
            let value = slot.value.load();
            if value.is_finite() {
                *mem = value;
            }
        }

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

            unsafe {
                self.instance.run(sample_count);
            }

            self.last_worker_ns = 0;
            if let Some(worker) = self.worker.as_mut() {
                unsafe {
                    worker.deliver_responses();
                }
                self.last_worker_ns = worker.drain_worker_ns();
            }
        }

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

        if self.bypassed.load(Ordering::Relaxed) {
            midi_out.thru(midi_events);
        } else {
            for (port, &slot) in self.midi_out_slots.iter().enumerate() {
                for_each_midi_event(
                    self.atom_out_bufs[slot].bytes(),
                    self.atom_sequence_urid,
                    self.midi_event_urid,
                    |frame, data| midi_out.push(port, frame, data),
                );
            }
        }

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
        if let Some(worker) = self.worker.as_mut() {
            worker.stop();
        }
        if let Ok(mut rates) = handle_sample_rates().lock() {
            rates.remove(&(self.lv2_handle as usize));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::processing::RawMidiEvent;

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
        init_atom_output(&mut buf, 7);
        assert_eq!(produced_sequence_len(&buf, 5), None);
        init_atom_input(&mut buf, 5);
        assert_eq!(produced_sequence_len(&buf, 5), None);
        write_u32(&mut buf, 0, 8 + 24);
        assert_eq!(produced_sequence_len(&buf, 5), Some(40));
        write_u32(&mut buf, 0, 1000);
        assert_eq!(produced_sequence_len(&buf, 5), None);
    }

    #[test]
    fn midi_events_are_read_from_output_sequence() {
        let mut buf = vec![0u8; 128];
        init_atom_input(&mut buf, 5);
        let mut pos = 16;
        for (frames, type_, body) in [
            (3i64, 2u32, &[0x90u8, 60, 100][..]),
            (10, 9, &[1, 2, 3, 4, 5, 6, 7, 8, 9][..]),
            (40, 2, &[0xC1, 4][..]),
        ] {
            buf[pos..pos + 8].copy_from_slice(&frames.to_ne_bytes());
            write_u32(&mut buf, pos + 8, body.len() as u32);
            write_u32(&mut buf, pos + 12, type_);
            buf[pos + 16..pos + 16 + body.len()].copy_from_slice(body);
            pos = (pos + 16 + body.len() + 7) & !7;
        }
        write_u32(&mut buf, 0, (pos - 8) as u32);

        let mut seen = Vec::new();
        for_each_midi_event(&buf, 5, 2, |frame, data| seen.push((frame, data.to_vec())));
        assert_eq!(seen, vec![(3, vec![0x90, 60, 100]), (40, vec![0xC1, 4])]);

        init_atom_output(&mut buf, 7);
        let mut count = 0;
        for_each_midi_event(&buf, 5, 2, |_, _| count += 1);
        assert_eq!(count, 0);
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

        assert_eq!(total, 16 + 24 + 24 + 24);
        assert_eq!(read_u32(&buf, 0) as usize, total - 8);
        assert_eq!(read_u32(&buf, 4), 1);
        assert_eq!(read_u32(&buf, 24), 3);
        assert_eq!(read_u32(&buf, 28), 77);
        assert_eq!(&buf[32..35], &[1, 2, 3]);
        assert_eq!(&buf[35..40], &[0, 0, 0, 0, 0]);
        assert_eq!(read_u32(&buf, 48), 8);
        assert_eq!(read_u32(&buf, 52), 78);
        assert_eq!(i64::from_ne_bytes(buf[64..72].try_into().unwrap()), 5);
        assert_eq!(read_u32(&buf, 72), 3);
        assert_eq!(read_u32(&buf, 76), 2);
        assert_eq!(&buf[80..83], &[0x90, 60, 100]);
        assert_eq!(ui.read(), None);
    }

    #[test]
    fn fill_atom_input_leaves_overflow_queued() {
        let ui = AtomPortBuffer::new(0);
        ui.write(&make_atom(1, &[0u8; 16]));
        ui.write(&make_atom(1, &[0u8; 16]));
        let mut buf = vec![0u8; 16 + 32 + 8];
        fill_atom_input(&mut buf, 1, 2, &ui, &[]);
        assert_eq!(read_u32(&buf, 0), 8 + 32);
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
