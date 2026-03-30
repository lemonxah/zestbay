use std::collections::HashMap;
use std::ffi::c_void;
use std::os::raw::c_char;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use lilv::World;

use super::log::Lv2LogSetup;
use super::options::Lv2OptionsSetup;
use super::state::{LV2_State_Interface, Lv2StatePathSetup, StateEntry, LV2_STATE__INTERFACE};
use super::types::*;
use super::urid::UridMapper;
use super::worker::{LV2_Worker_Interface, Lv2Worker, Lv2WorkerSetup, LV2_WORKER_INTERFACE_URI};

static NEXT_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

fn next_instance_id() -> PluginInstanceId {
    NEXT_INSTANCE_ID.fetch_add(1, Ordering::Relaxed)
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

const ATOM_BUF_SIZE: usize = 65536;

const LV2_RESIZE_PORT_URI: &std::ffi::CStr = c"http://lv2plug.in/ns/ext/resize-port#resize";
const LV2_URI_MAP_URI: &std::ffi::CStr = c"http://lv2plug.in/ns/ext/uri-map";

// Data-less LV2 host capability flags
const LV2_HARD_RT_CAPABLE_URI: &std::ffi::CStr = c"http://lv2plug.in/ns/lv2core#hardRTCapable";
const LV2_IS_LIVE_URI: &std::ffi::CStr = c"http://lv2plug.in/ns/lv2core#isLive";
const LV2_IN_PLACE_BROKEN_URI: &std::ffi::CStr = c"http://lv2plug.in/ns/lv2core#inPlaceBroken";

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

fn init_atom_sequence(buf: &mut [u8], capacity: usize, is_output: bool, sequence_type_urid: u32) {
    assert!(capacity >= 16, "atom buffer too small");
    buf[..capacity].fill(0);

    let size: u32 = if is_output { (capacity - 8) as u32 } else { 8 };

    buf[0..4].copy_from_slice(&size.to_ne_bytes());
    buf[4..8].copy_from_slice(&sequence_type_urid.to_ne_bytes());
    buf[8..12].copy_from_slice(&0u32.to_ne_bytes());
    buf[12..16].copy_from_slice(&0u32.to_ne_bytes());
}

pub struct Lv2PluginInstance {
    pub id: PluginInstanceId,
    instance: lilv::instance::ActiveInstance,
    _world: lilv::World,
    _urid_map: Box<lv2_raw::urid::LV2UridMap>,
    _urid_unmap: Box<super::urid::LV2UridUnmap>,
    _log_setup: Lv2LogSetup,
    _options_setup: Lv2OptionsSetup,
    _state_path_setup: Lv2StatePathSetup,
    pub plugin_uri: String,
    pub display_name: String,
    pub audio_input_indices: Vec<usize>,
    pub audio_output_indices: Vec<usize>,
    pub control_inputs: Vec<ControlPort>,
    pub control_outputs: Vec<ControlPort>,
    pub atom_in_bufs: Vec<AtomBuf>,
    pub atom_out_bufs: Vec<AtomBuf>,
    pub port_updates: SharedPortUpdates,
    atom_sequence_urid: u32,
    pub bypassed: bool,
    pub sample_rate: f64,
    /// Worker thread for plugins that require the worker#schedule feature
    pub worker: Option<Lv2Worker>,
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

pub struct AtomBuf {
    pub port_index: usize,
    pub data: Vec<u8>,
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
}

impl Lv2PluginInstance {
    pub unsafe fn new(
        world: lilv::World,
        plugin: &lilv::plugin::Plugin,
        plugin_info: &Lv2PluginInfo,
        sample_rate: f64,
        block_length: u32,
        urid_mapper: &Arc<UridMapper>,
    ) -> Option<Self> {
        let id = next_instance_id();
        let atom_sequence_urid = urid_mapper.map("http://lv2plug.in/ns/ext/atom#Sequence");

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

        let state_path_setup = Lv2StatePathSetup::new(&plugin_info.uri);
        let make_path_feature = state_path_setup.make_make_path_feature();
        let free_path_feature = state_path_setup.make_free_path_feature();
        let map_path_feature = state_path_setup.make_map_path_feature();

        // lilv 0.2.4 depends on lv2_raw 0.2's LV2Feature while we use lv2_raw 0.3.
        // Both versions have an identical #[repr(C)] layout:
        //   { uri: *const c_char, data: *mut c_void }
        // The only difference is libc types vs std::os::raw types (same ABI).
        // We transmute the Vec to bridge the type boundary.
        let worker_feature;
        let mut resize_port_data = LV2_Resize_Port_Resize {
            data: std::ptr::null_mut(),
            resize: resize_port_stub,
        };
        let resize_port_feature = lv2_raw::core::LV2Feature {
            uri: LV2_RESIZE_PORT_URI.as_ptr(),
            data: &mut resize_port_data as *mut _ as *mut c_void,
        };
        let mut uri_map_data = LV2_URI_Map_Feature {
            callback_data: Arc::as_ptr(urid_mapper) as *mut c_void,
            uri_to_id: uri_map_callback,
        };
        let uri_map_feature = lv2_raw::core::LV2Feature {
            uri: LV2_URI_MAP_URI.as_ptr(),
            data: &mut uri_map_data as *mut _ as *mut c_void,
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
            &make_path_feature,
            &free_path_feature,
            &map_path_feature,
        ];
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

        let mut instance = unsafe { plugin.instantiate(sample_rate, features) }?;

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
                    control_inputs.push(ControlPort {
                        index: port_info.index,
                        symbol: port_info.symbol.clone(),
                        name: port_info.name.clone(),
                        value: port_info.default_value,
                        min: port_info.min_value,
                        max: port_info.max_value,
                        default: port_info.default_value,
                        is_toggle: port_info.is_toggle,
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
                    });
                }
                Lv2PortType::AtomInput => {
                    atom_in_bufs.push(AtomBuf {
                        port_index: port_info.index,
                        data: vec![0u8; ATOM_BUF_SIZE],
                    });
                }
                Lv2PortType::AtomOutput => {
                    atom_out_bufs.push(AtomBuf {
                        port_index: port_info.index,
                        data: vec![0u8; ATOM_BUF_SIZE],
                    });
                }
            }
        }

        for cp in &mut control_inputs {
            unsafe {
                instance.connect_port_mut(cp.index, &mut cp.value as *mut f32);
            }
        }
        for cp in &mut control_outputs {
            unsafe {
                instance.connect_port_mut(cp.index, &mut cp.value as *mut f32);
            }
        }

        // Connect audio ports to a dummy buffer so that ALL ports are
        // connected before activate(), as required by the LV2 spec.
        // process() will reconnect them to real buffers each cycle.
        let mut dummy_audio_buf = vec![0.0f32; block_length as usize];
        for &idx in audio_input_indices.iter().chain(audio_output_indices.iter()) {
            unsafe {
                instance.connect_port_mut(idx, dummy_audio_buf.as_mut_ptr());
            }
        }

        for ab in atom_in_bufs.iter_mut() {
            init_atom_sequence(&mut ab.data, ATOM_BUF_SIZE, false, atom_sequence_urid);
            unsafe {
                instance.connect_port_mut(ab.port_index, ab.data.as_mut_ptr());
            }
        }
        for ab in atom_out_bufs.iter_mut() {
            init_atom_sequence(&mut ab.data, ATOM_BUF_SIZE, true, atom_sequence_urid);
            unsafe {
                instance.connect_port_mut(ab.port_index, ab.data.as_mut_ptr());
            }
        }

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
        // SAFETY: lv2_raw 0.2 uses `extern "C" fn(*const u8) -> *const libc::c_void`
        // while the LV2 data-access spec uses `extern "C" fn(*const c_char) -> *const c_void`.
        // These are ABI-identical (u8 vs i8 pointer, libc::c_void = core::ffi::c_void).
        let extension_data_fn: Option<unsafe extern "C" fn(*const c_char) -> *const c_void> =
            instance
                .descriptor()
                .map(|d| unsafe { std::mem::transmute(d.extension_data) });

        let worker = if let Some(ws) = worker_setup {
            let worker_iface_ptr: Option<std::ptr::NonNull<LV2_Worker_Interface>> = unsafe {
                // extension_data returns lv2_raw 0.2 types (via lilv), but
                // LV2_Worker_Interface is our own #[repr(C)] definition which
                // matches the C layout exactly.
                instance.extension_data::<LV2_Worker_Interface>(LV2_WORKER_INTERFACE_URI)
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
                    drop(ws);
                    None
                }
            }
        } else {
            None
        };

        let state_iface: Option<std::ptr::NonNull<LV2_State_Interface>> =
            unsafe { instance.extension_data::<LV2_State_Interface>(LV2_STATE__INTERFACE) };
        if state_iface.is_some() {
            log::info!(
                "LV2 state: plugin '{}' provides state interface",
                plugin_info.name
            );
        }

        let active_instance = unsafe { instance.activate() };

        Some(Self {
            id,
            instance: active_instance,
            _world: world,
            _urid_map: urid_map,
            _urid_unmap: urid_unmap,
            _log_setup: log_setup,
            _options_setup: options_setup,
            _state_path_setup: state_path_setup,
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
            bypassed: false,
            sample_rate,
            worker,
            last_worker_ns: 0,
            state_iface,
            lv2_handle,
            extension_data_fn,
            urid_mapper: urid_mapper.clone(),
        })
    }

    pub unsafe fn reconnect_control_ports(&mut self) {
        for cp in &mut self.control_inputs {
            unsafe {
                self.instance
                    .instance_mut()
                    .connect_port_mut(cp.index, &mut cp.value as *mut f32);
            }
        }
        for cp in &mut self.control_outputs {
            unsafe {
                self.instance
                    .instance_mut()
                    .connect_port_mut(cp.index, &mut cp.value as *mut f32);
            }
        }
    }

    pub unsafe fn process(
        &mut self,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        sample_count: usize,
    ) {
        // Connect audio ports (buffer pointers change each cycle)
        for (i, &port_idx) in self.audio_input_indices.iter().enumerate() {
            if i < inputs.len() {
                unsafe {
                    self.instance
                        .instance_mut()
                        .connect_port(port_idx, inputs[i].as_ptr());
                }
            }
        }

        for (i, &port_idx) in self.audio_output_indices.iter().enumerate() {
            if i < outputs.len() {
                unsafe {
                    self.instance
                        .instance_mut()
                        .connect_port_mut(port_idx, outputs[i].as_mut_ptr());
                }
            }
        }

        // Read external parameter changes (e.g. from MIDI RT callback) into
        // the plugin's control input ports.  The LV2 plugin reads `cp.value`
        // directly via `connect_port_mut`, so updating it here ensures the
        // plugin processes with the latest value.  After `run()` we write
        // `cp.value` back to the atomic (which is now the same value).
        for (cp, slot) in self
            .control_inputs
            .iter_mut()
            .zip(self.port_updates.control_inputs.iter())
        {
            cp.value = slot.value.load();
        }

        // Prepare atom input buffers (UI → plugin communication)
        for (ab, shared) in self
            .atom_in_bufs
            .iter_mut()
            .zip(self.port_updates.atom_inputs.iter())
        {
            init_atom_sequence(&mut ab.data, ATOM_BUF_SIZE, false, self.atom_sequence_urid);
            let Some(ui_atom) = shared.read() else {
                continue;
            };
            if !ui_atom.is_empty() && ui_atom.len() >= 8 {
                let event_size = 8 + ui_atom.len();
                let padded_event_size = (event_size + 7) & !7;
                if 16 + padded_event_size <= ab.data.len() {
                    ab.data[16..24].copy_from_slice(&0i64.to_ne_bytes());
                    ab.data[24..24 + ui_atom.len()].copy_from_slice(&ui_atom);
                    let body_size = 8u32 + padded_event_size as u32;
                    ab.data[0..4].copy_from_slice(&body_size.to_ne_bytes());
                }
            }
        }

        for ab in &mut self.atom_out_bufs {
            init_atom_sequence(&mut ab.data, ATOM_BUF_SIZE, true, self.atom_sequence_urid);
        }

        // Always call run() so the plugin keeps its internal state alive
        // (visualizers, worker threads, etc.). When bypassed we just
        // overwrite the audio output with a passthrough copy afterwards.
        unsafe {
            self.instance.run(sample_count);
        }

        // Deliver any pending worker responses after run()
        // and drain accumulated worker thread CPU time for stats
        self.last_worker_ns = 0;
        if let Some(ref worker) = self.worker {
            unsafe {
                worker.deliver_responses();
            }
            self.last_worker_ns = worker.drain_worker_ns();
        }

        // When bypassed, overwrite plugin audio output with passthrough
        if self.bypassed {
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

        for (cp, slot) in self
            .control_outputs
            .iter()
            .zip(self.port_updates.control_outputs.iter())
        {
            slot.value.store(cp.value);
        }

        for (cp, slot) in self
            .control_inputs
            .iter()
            .zip(self.port_updates.control_inputs.iter())
        {
            slot.value.store(cp.value);
        }

        for (ab, shared) in self
            .atom_out_bufs
            .iter()
            .zip(self.port_updates.atom_outputs.iter())
        {
            if ab.data.len() >= 16 {
                let atom_size =
                    u32::from_ne_bytes([ab.data[0], ab.data[1], ab.data[2], ab.data[3]]);
                let total = 8 + atom_size as usize;
                if atom_size > 8 && total <= ab.data.len() {
                    shared.write(&ab.data[..total]);
                }
            }
        }
    }

    pub fn set_parameter(&mut self, port_index: usize, value: f32) {
        if let Some(cp) = self
            .control_inputs
            .iter_mut()
            .find(|cp| cp.index == port_index)
        {
            let clamped = safe_clamp(value, cp.min, cp.max);
            cp.value = clamped;

            if let Some(slot) = self
                .port_updates
                .control_inputs
                .iter()
                .find(|s| s.port_index == port_index)
            {
                slot.value.store(clamped);
            }
        }
    }

    pub fn set_parameter_by_symbol(&mut self, symbol: &str, value: f32) {
        if let Some(idx) = self
            .control_inputs
            .iter()
            .position(|cp| cp.symbol == symbol)
        {
            let cp = &mut self.control_inputs[idx];
            let clamped = safe_clamp(value, cp.min, cp.max);
            cp.value = clamped;
            let port_index = cp.index;

            if let Some(slot) = self
                .port_updates
                .control_inputs
                .iter()
                .find(|s| s.port_index == port_index)
            {
                slot.value.store(clamped);
            }
        }
    }

    pub fn get_parameters(&self) -> Vec<Lv2ParameterValue> {
        self.control_inputs
            .iter()
            .map(|cp| Lv2ParameterValue {
                port_index: cp.index,
                symbol: cp.symbol.clone(),
                name: cp.name.clone(),
                value: cp.value,
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
            bypassed: self.bypassed,
            lv2_state: Vec::new(),
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
            super::state::save_plugin_state(self.lv2_handle, iface.as_ptr(), &self.urid_mapper)
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
            );
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
