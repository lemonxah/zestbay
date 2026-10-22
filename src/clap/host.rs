//! CLAP plugin host — instantiation and real-time processing.
//!
//! Threading model (see `threading.rs`): everything except `process()` runs
//! on the PipeWire main-loop thread, which is the CLAP main thread.
//! `process()` runs on the PipeWire data thread and never allocates or locks.
//! Host → plugin parameter changes travel through the atomic slots in
//! `port_updates`; `process()` diffs them against an RT-private copy of the
//! last applied values and emits `CLAP_EVENT_PARAM_VALUE` events.

use std::ffi::{CStr, CString, c_void};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap_sys::audio_buffer::clap_audio_buffer;
use clap_sys::events::{
    CLAP_CORE_EVENT_SPACE_ID, CLAP_EVENT_MIDI, CLAP_EVENT_PARAM_VALUE, clap_event_header,
    clap_event_midi, clap_event_param_value, clap_input_events, clap_output_events,
};
use clap_sys::ext::params::{
    CLAP_PARAM_RESCAN_ALL, CLAP_PARAM_RESCAN_INFO, CLAP_PARAM_RESCAN_VALUES, clap_host_params,
    clap_param_clear_flags, clap_param_rescan_flags, clap_plugin_params,
};
use clap_sys::ext::state::{clap_host_state, clap_plugin_state};
use clap_sys::host::clap_host;
use clap_sys::id::clap_id;
use clap_sys::plugin::clap_plugin;
use clap_sys::stream::{clap_istream, clap_ostream};

use super::threading::{self, RtGate};
use crate::plugin::types::*;

static NEXT_INSTANCE_ID: AtomicU64 = AtomicU64::new(1_000_000);

fn next_instance_id() -> PluginInstanceId {
    NEXT_INSTANCE_ID.fetch_add(1, Ordering::Relaxed)
}

/// Largest block `process()` is ever called with (K4).
pub const MAX_BLOCK: usize = 8192;
/// Max MIDI events forwarded per block.
const MAX_MIDI_EVENTS: usize = 512;
/// How long the RT thread must have been quiet before the main thread
/// flushes parameters itself.
const RT_IDLE_BEFORE_FLUSH: Duration = Duration::from_millis(500);

/// A loaded CLAP plugin library (keeps the dlopen handle alive).
struct ClapLibrary {
    /// dlopen handle — must NOT be closed while any plugin from this lib exists.
    _handle: *mut c_void,
    /// The entry point (lives inside the .so).
    entry: *const clap_sys::entry::clap_plugin_entry,
}

impl Drop for ClapLibrary {
    fn drop(&mut self) {
        let entry = unsafe { &*self.entry };
        if let Some(deinit) = entry.deinit {
            unsafe {
                deinit();
            }
        }
        // We intentionally do NOT dlclose.  See scanner.rs comment.
    }
}

/// Host-side state reachable from plugin callbacks via `clap_host.host_data`.
/// Boxed so its address is stable; callbacks may arrive on any thread.
pub(crate) struct HostData {
    /// The plugin pointer — set right after create_plugin(), before init().
    pub plugin: *const clap_plugin,
    callback_requested: AtomicBool,
    restart_requested: AtomicBool,
    flush_requested: AtomicBool,
    rescan_flags: AtomicU32,
}

/// A running CLAP plugin instance.
pub struct ClapPluginInstance {
    pub id: PluginInstanceId,
    plugin: *const clap_plugin,
    _host_box: Box<clap_host>,
    host_data: Box<HostData>,

    pub plugin_id: String,
    pub display_name: String,

    /// Audio port layout (flattened to mono channels, matching the LV2 approach)
    pub audio_input_channels: usize,
    pub audio_output_channels: usize,

    /// Whether this plugin accepts MIDI input (has note input ports)
    pub has_midi_in: bool,
    /// Whether this plugin produces MIDI output (has note output ports)
    pub has_midi_out: bool,

    /// Channel count of each CLAP audio port.
    input_port_channels: Vec<usize>,
    output_port_channels: Vec<usize>,

    /// Parameters (static info; values live in `port_updates`).
    pub params: Vec<ClapParam>,
    /// (param id, index into `params`), sorted by id.
    id_index: Vec<(clap_id, usize)>,
    params_ext: *const clap_plugin_params,
    state_ext: *const clap_plugin_state,

    /// Shared port updates (same pattern as LV2)
    pub port_updates: SharedPortUpdates,

    bypassed: AtomicBool,
    /// Fixed at instantiation; a graph rate change requires a reload (K3).
    pub sample_rate: f64,
    activated: bool,

    gate: RtGate,
    process_count: AtomicU64,
    /// Set by `set_parameter`; lets `idle` flush when audio is stopped.
    host_param_dirty: AtomicBool,
    /// Main-thread bookkeeping for RT idleness.
    last_seen_process_count: u64,
    last_process_change: Instant,

    /// Touched only by `process()` (or by the main thread while suspended).
    rt: RtState,

    /// Dropped last: deinit must run after the plugin is destroyed.
    _library: ClapLibrary,
}

/// Preallocated buffers used by `process()`.
struct RtState {
    /// start_processing() succeeded and stop_processing() is still due.
    started: bool,
    last_applied: Vec<f64>,
    param_events: Vec<clap_event_param_value>,
    midi_events: Vec<clap_event_midi>,
    in_ptrs: Vec<*mut f32>,
    out_ptrs: Vec<*mut f32>,
    in_bufs: Vec<clap_audio_buffer>,
    out_bufs: Vec<clap_audio_buffer>,
    silence: Vec<f32>,
    scratch: Vec<f32>,
}

impl RtState {
    fn new(input_ports: &[usize], output_ports: &[usize], last_applied: Vec<f64>) -> Self {
        let n_in: usize = input_ports.iter().sum();
        let n_out: usize = output_ports.iter().sum();
        let mut rt = Self {
            started: false,
            param_events: Vec::with_capacity(last_applied.len()),
            last_applied,
            midi_events: Vec::with_capacity(MAX_MIDI_EVENTS),
            in_ptrs: vec![std::ptr::null_mut(); n_in],
            out_ptrs: vec![std::ptr::null_mut(); n_out],
            in_bufs: Vec::with_capacity(input_ports.len()),
            out_bufs: Vec::with_capacity(output_ports.len()),
            silence: vec![0.0; MAX_BLOCK],
            scratch: vec![0.0; MAX_BLOCK],
        };
        // The pointer arrays never reallocate, so the per-port views into
        // them stay valid for the life of the instance.
        let mut off = 0;
        for &ch in input_ports {
            rt.in_bufs.push(audio_buffer(unsafe { rt.in_ptrs.as_mut_ptr().add(off) }, ch));
            off += ch;
        }
        let mut off = 0;
        for &ch in output_ports {
            rt.out_bufs.push(audio_buffer(unsafe { rt.out_ptrs.as_mut_ptr().add(off) }, ch));
            off += ch;
        }
        rt
    }
}

fn audio_buffer(data32: *mut *mut f32, channels: usize) -> clap_audio_buffer {
    clap_audio_buffer {
        data32,
        data64: std::ptr::null_mut(),
        channel_count: channels as u32,
        latency: 0,
        constant_mask: 0,
    }
}

#[derive(Debug, Clone)]
pub struct ClapParam {
    pub id: u32,
    pub port_index: usize,
    pub name: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    pub is_toggle: bool,
}

unsafe impl Send for ClapPluginInstance {}

impl ClapPluginInstance {
    /// Return the raw CLAP plugin pointer (for GUI extension access).
    pub fn plugin_ptr(&self) -> *const clap_plugin {
        self.plugin
    }

    /// Sample rate the plugin was activated with.
    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    /// Load a CLAP plugin from a `.clap` file and instantiate it.
    ///
    /// # Safety
    /// Calls into C plugin code via dlopen/function pointers.  Must be called
    /// on the PipeWire thread, which becomes the CLAP main thread.
    pub unsafe fn new(
        clap_path: &str,
        plugin_id: &str,
        plugin_info: &PluginInfo,
        sample_rate: f64,
    ) -> Option<Self> {
        unsafe {
            threading::set_main_thread();

            let instance_id = next_instance_id();
            let c_path = CString::new(clap_path).ok()?;

            let handle = libc::dlopen(c_path.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL);
            if handle.is_null() {
                log::error!("CLAP: dlopen failed for {}", clap_path);
                return None;
            }

            let entry_ptr = libc::dlsym(handle, c"clap_entry".as_ptr());
            if entry_ptr.is_null() {
                log::error!("CLAP: no clap_entry in {}", clap_path);
                libc::dlclose(handle);
                return None;
            }

            let entry = entry_ptr as *const clap_sys::entry::clap_plugin_entry;
            let entry_ref = &*entry;

            if let Some(init_fn) = entry_ref.init {
                if !init_fn(c_path.as_ptr()) {
                    log::error!("CLAP: init failed for {}", clap_path);
                    libc::dlclose(handle);
                    return None;
                }
            }

            let library = ClapLibrary {
                _handle: handle,
                entry,
            };

            let factory_ptr = entry_ref.get_factory?(
                clap_sys::factory::plugin_factory::CLAP_PLUGIN_FACTORY_ID.as_ptr(),
            );
            if factory_ptr.is_null() {
                log::error!("CLAP: no plugin factory in {}", clap_path);
                return None;
            }
            let factory =
                &*(factory_ptr as *const clap_sys::factory::plugin_factory::clap_plugin_factory);

            let mut host_data = Box::new(HostData {
                plugin: std::ptr::null(),
                callback_requested: AtomicBool::new(false),
                restart_requested: AtomicBool::new(false),
                flush_requested: AtomicBool::new(false),
                rescan_flags: AtomicU32::new(0),
            });

            let host_box = Box::new(clap_host {
                clap_version: clap_sys::version::CLAP_VERSION,
                host_data: &mut *host_data as *mut HostData as *mut c_void,
                name: c"ZestBay".as_ptr(),
                vendor: c"ZestBay".as_ptr(),
                url: c"https://github.com/lemonxah/zestbay".as_ptr(),
                version: c"0.1.0".as_ptr(),
                get_extension: Some(host_get_extension),
                request_restart: Some(host_request_restart),
                request_process: Some(host_request_process),
                request_callback: Some(host_request_callback),
            });

            let c_id = CString::new(plugin_id).ok()?;
            let plugin_ptr = factory.create_plugin?(factory, &*host_box, c_id.as_ptr());
            if plugin_ptr.is_null() {
                log::error!("CLAP: create_plugin failed for {}", plugin_id);
                return None;
            }

            host_data.plugin = plugin_ptr;
            let plugin_ref = &*plugin_ptr;

            let destroy = || {
                super::ui::remove_plugin_callbacks(plugin_ptr);
                if let Some(destroy) = plugin_ref.destroy {
                    destroy(plugin_ptr);
                }
                super::ui::remove_plugin_callbacks(plugin_ptr);
            };

            if let Some(init_fn) = plugin_ref.init {
                if !init_fn(plugin_ptr) {
                    log::error!("CLAP: plugin init failed for {}", plugin_id);
                    destroy();
                    return None;
                }
            }

            let (input_port_channels, output_port_channels) = query_audio_ports(plugin_ptr);
            let audio_input_channels: usize = input_port_channels.iter().sum();
            let audio_output_channels: usize = output_port_channels.iter().sum();

            // Query note ports (MIDI capability)
            let mut has_midi_in = false;
            let mut has_midi_out = false;
            if let Some(get_ext) = plugin_ref.get_extension {
                let ext = get_ext(
                    plugin_ptr,
                    clap_sys::ext::note_ports::CLAP_EXT_NOTE_PORTS.as_ptr(),
                );
                if !ext.is_null() {
                    let note_ports =
                        &*(ext as *const clap_sys::ext::note_ports::clap_plugin_note_ports);
                    if let Some(count_fn) = note_ports.count {
                        has_midi_in = count_fn(plugin_ptr, true) > 0;
                        has_midi_out = count_fn(plugin_ptr, false) > 0;
                    }
                }
            }

            // If the plugin is categorised as an instrument, assume it accepts MIDI
            // even if it doesn't expose note ports (some plugins omit the extension).
            if !has_midi_in && plugin_info.category == PluginCategory::Instrument {
                has_midi_in = true;
            }

            log::info!(
                "CLAP: {} — audio {}/{}, midi_in={}, midi_out={}",
                plugin_info.name,
                audio_input_channels,
                audio_output_channels,
                has_midi_in,
                has_midi_out,
            );

            // Query params
            let mut params = Vec::new();
            let mut initial_values = Vec::new();
            let params_ext = get_plugin_ext::<clap_plugin_params>(
                plugin_ptr,
                clap_sys::ext::params::CLAP_EXT_PARAMS,
            );
            if !params_ext.is_null() {
                let pe = &*params_ext;
                if let (Some(count_fn), Some(get_info)) = (pe.count, pe.get_info) {
                    let n = count_fn(plugin_ptr);
                    for idx in 0..n {
                        let mut info: clap_sys::ext::params::clap_param_info = std::mem::zeroed();
                        if !get_info(plugin_ptr, idx, &mut info) {
                            continue;
                        }
                        let is_hidden =
                            info.flags & clap_sys::ext::params::CLAP_PARAM_IS_HIDDEN != 0;
                        let is_readonly =
                            info.flags & clap_sys::ext::params::CLAP_PARAM_IS_READONLY != 0;
                        if is_hidden || is_readonly {
                            continue;
                        }
                        let mut value = info.default_value;
                        if let Some(get_val) = pe.get_value {
                            get_val(plugin_ptr, info.id, &mut value);
                        }
                        let is_toggle = info.flags & clap_sys::ext::params::CLAP_PARAM_IS_STEPPED
                            != 0
                            && info.min_value == 0.0
                            && info.max_value == 1.0;
                        params.push(ClapParam {
                            id: info.id,
                            port_index: params.len(),
                            name: read_clap_name(&info.name),
                            min: info.min_value,
                            max: info.max_value,
                            default: info.default_value,
                            is_toggle,
                        });
                        initial_values.push(value);
                    }
                }
            }

            let mut id_index: Vec<(clap_id, usize)> =
                params.iter().enumerate().map(|(i, p)| (p.id, i)).collect();
            id_index.sort_unstable_by_key(|&(id, _)| id);

            let port_updates = Arc::new(PortUpdates {
                control_inputs: params
                    .iter()
                    .zip(&initial_values)
                    .map(|(p, &v)| PortSlot {
                        port_index: p.port_index,
                        value: AtomicF32::new(v as f32),
                    })
                    .collect(),
                control_outputs: Vec::new(),
                atom_outputs: Vec::new(),
                atom_inputs: Vec::new(),
            });
            // Compare against exactly what the slots hold.
            let last_applied: Vec<f64> = initial_values.iter().map(|&v| v as f32 as f64).collect();

            let state_ext =
                get_plugin_ext::<clap_plugin_state>(plugin_ptr, clap_sys::ext::state::CLAP_EXT_STATE);

            // Activate (K4: any block size from 1 to MAX_BLOCK).
            let activated = match plugin_ref.activate {
                Some(activate) => activate(plugin_ptr, sample_rate, 1, MAX_BLOCK as u32),
                None => true,
            };
            if !activated {
                log::error!("CLAP: activate failed for {}", plugin_id);
                destroy();
                return None;
            }

            // start_processing is an audio-thread call: process() does it lazily.
            let rt = RtState::new(&input_port_channels, &output_port_channels, last_applied);

            Some(Self {
                id: instance_id,
                plugin: plugin_ptr,
                _host_box: host_box,
                host_data,
                plugin_id: plugin_id.to_string(),
                display_name: plugin_info.name.clone(),
                audio_input_channels,
                audio_output_channels,
                has_midi_in,
                has_midi_out,
                input_port_channels,
                output_port_channels,
                params,
                id_index,
                params_ext,
                state_ext,
                port_updates,
                bypassed: AtomicBool::new(false),
                sample_rate,
                activated,
                gate: RtGate::new(),
                process_count: AtomicU64::new(0),
                host_param_dirty: AtomicBool::new(false),
                last_seen_process_count: 0,
                last_process_change: Instant::now(),
                rt,
                _library: library,
            })
        }
    }

    /// Process a block of audio.
    ///
    /// # Safety
    /// Called from the PipeWire RT thread.  The `inputs` and `outputs` slices
    /// must be valid for `sample_count` frames.  Never allocates or locks.
    pub unsafe fn process(
        &mut self,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        sample_count: usize,
        midi_events: &[crate::midi::processing::RawMidiEvent],
    ) {
        threading::mark_audio_thread();
        let n = sample_count.min(MAX_BLOCK);
        if n == 0 {
            return;
        }
        if !self.gate.enter() {
            threading::passthrough(inputs, outputs, n);
            return;
        }
        self.process_count.fetch_add(1, Ordering::Relaxed);
        self.host_data.flush_requested.store(false, Ordering::Relaxed);

        let ran = unsafe { self.process_locked(inputs, outputs, n, midi_events) };
        if !ran || self.bypassed.load(Ordering::Relaxed) {
            threading::passthrough(inputs, outputs, n);
        }
        self.gate.leave();
    }

    unsafe fn process_locked(
        &mut self,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        n: usize,
        midi_events: &[crate::midi::processing::RawMidiEvent],
    ) -> bool {
        unsafe {
            let plugin = self.plugin;
            let plugin_ref = &*plugin;
            let Self {
                rt,
                params,
                id_index,
                port_updates,
                ..
            } = self;

            if !rt.started {
                let ok = match plugin_ref.start_processing {
                    Some(start) => start(plugin),
                    None => true,
                };
                if !ok {
                    return false;
                }
                rt.started = true;
            }

            collect_param_events(params, &port_updates.control_inputs, rt);

            rt.midi_events.clear();
            for evt in midi_events.iter().take(MAX_MIDI_EVENTS) {
                rt.midi_events.push(clap_event_midi {
                    header: event_header::<clap_event_midi>(
                        CLAP_EVENT_MIDI,
                        evt.offset.min(n as u32 - 1),
                    ),
                    port_index: 0,
                    data: evt.data,
                });
            }

            for (k, ptr) in rt.in_ptrs.iter_mut().enumerate() {
                *ptr = match inputs.get(k) {
                    Some(s) if s.len() >= n => s.as_ptr() as *mut f32,
                    _ => rt.silence.as_mut_ptr(),
                };
            }
            for (k, ptr) in rt.out_ptrs.iter_mut().enumerate() {
                *ptr = match outputs.get_mut(k) {
                    Some(s) if s.len() >= n => s.as_mut_ptr(),
                    _ => rt.scratch.as_mut_ptr(),
                };
            }
            for b in rt.out_bufs.iter_mut() {
                b.constant_mask = 0;
            }

            let in_data = InputEvents {
                params: &rt.param_events,
                midi: &rt.midi_events,
            };
            let in_events = in_data.as_clap();
            let out_ctx = OutputEvents {
                id_index,
                slots: &port_updates.control_inputs,
                last_applied: rt.last_applied.as_mut_ptr(),
            };
            let out_events = out_ctx.as_clap();

            let process = clap_sys::process::clap_process {
                steady_time: -1,
                frames_count: n as u32,
                transport: std::ptr::null(),
                audio_inputs: if rt.in_bufs.is_empty() {
                    std::ptr::null()
                } else {
                    rt.in_bufs.as_ptr()
                },
                audio_outputs: if rt.out_bufs.is_empty() {
                    std::ptr::null_mut()
                } else {
                    rt.out_bufs.as_mut_ptr()
                },
                audio_inputs_count: rt.in_bufs.len() as u32,
                audio_outputs_count: rt.out_bufs.len() as u32,
                in_events: &in_events,
                out_events: &out_events,
            };

            if let Some(process_fn) = plugin_ref.process {
                process_fn(plugin, &process);
            }
            true
        }
    }

    /// Set a parameter (plain value).  Only stores into the atomic slot;
    /// `process()` forwards it to the plugin.
    pub fn set_parameter(&self, port_index: usize, value: f32) {
        if let Some(p) = self.params.iter().find(|p| p.port_index == port_index) {
            let clamped = (value as f64).clamp(p.min, p.max);
            if let Some(slot) = self
                .port_updates
                .control_inputs
                .iter()
                .find(|s| s.port_index == port_index)
            {
                slot.value.store(clamped as f32);
                self.host_param_dirty.store(true, Ordering::Release);
            }
        }
    }

    pub fn set_bypassed(&self, bypassed: bool) {
        self.bypassed.store(bypassed, Ordering::Relaxed);
    }

    pub fn is_bypassed(&self) -> bool {
        self.bypassed.load(Ordering::Relaxed)
    }

    /// Serialize plugin state via the CLAP state extension (main thread).
    pub fn get_state(&self) -> Option<Vec<u8>> {
        if self.state_ext.is_null() {
            return None;
        }
        unsafe {
            let save = (*self.state_ext).save?;
            let mut buf: Vec<u8> = Vec::new();
            let stream = clap_ostream {
                ctx: &mut buf as *mut Vec<u8> as *mut c_void,
                write: Some(ostream_write),
            };
            if save(self.plugin, &stream) {
                Some(buf)
            } else {
                log::warn!("CLAP: state.save failed for {}", self.display_name);
                None
            }
        }
    }

    /// Restore plugin state via the CLAP state extension (main thread).
    ///
    /// # Safety
    /// Calls into plugin code.
    pub unsafe fn set_state(&mut self, blob: &[u8]) -> bool {
        if self.state_ext.is_null() {
            return false;
        }
        unsafe {
            let Some(load) = (*self.state_ext).load else {
                return false;
            };
            let mut reader = StreamReader { data: blob, pos: 0 };
            let stream = clap_istream {
                ctx: &mut reader as *mut StreamReader as *mut c_void,
                read: Some(istream_read),
            };
            let ok = load(self.plugin, &stream);
            if !ok {
                log::warn!("CLAP: state.load failed for {}", self.display_name);
            }
            self.refresh_param_values();
            ok
        }
    }

    /// Re-read parameter values from the plugin into the slots (main thread).
    fn refresh_param_values(&self) {
        if self.params_ext.is_null() {
            return;
        }
        unsafe {
            let Some(get_value) = (*self.params_ext).get_value else {
                return;
            };
            for (p, slot) in self.params.iter().zip(&self.port_updates.control_inputs) {
                let mut v = 0.0;
                if get_value(self.plugin, p.id, &mut v) {
                    slot.value.store(v as f32);
                }
            }
        }
    }

    pub fn get_parameters(&self) -> Vec<ParameterValue> {
        self.params
            .iter()
            .zip(&self.port_updates.control_inputs)
            .map(|(p, slot)| ParameterValue {
                port_index: p.port_index,
                symbol: format!("param_{}", p.id),
                name: p.name.clone(),
                value: slot.value.load(),
                min: p.min as f32,
                max: p.max as f32,
                default: p.default as f32,
                is_toggle: p.is_toggle,
            })
            .collect()
    }

    pub fn get_info(&self, pw_node_id: Option<u32>) -> PluginInstanceInfo {
        PluginInstanceInfo {
            id: self.id,
            stable_id: String::new(),
            plugin_uri: self.plugin_id.clone(),
            format: PluginFormat::Clap,
            display_name: self.display_name.clone(),
            pw_node_id,
            parameters: self.get_parameters(),
            active: true,
            bypassed: self.is_bypassed(),
            lv2_state: Vec::new(),
            plugin_state: None,
        }
    }

    /// Main-thread housekeeping; call on every manager tick.
    ///
    /// Runs `on_main_thread` when the plugin asked for it, applies
    /// `params.rescan` requests, and flushes parameters through
    /// `params.flush` while the audio thread is not processing the plugin.
    pub fn idle(&mut self) {
        unsafe {
            let plugin_ref = &*self.plugin;
            if self.host_data.callback_requested.swap(false, Ordering::AcqRel) {
                if let Some(on_main) = plugin_ref.on_main_thread {
                    on_main(self.plugin);
                }
            }
        }

        let rescan = self.host_data.rescan_flags.swap(0, Ordering::AcqRel);
        if rescan & (CLAP_PARAM_RESCAN_VALUES | CLAP_PARAM_RESCAN_ALL) != 0 {
            self.refresh_param_values();
        }
        if rescan & (CLAP_PARAM_RESCAN_INFO | CLAP_PARAM_RESCAN_ALL) != 0 {
            log::warn!(
                "CLAP: {} changed its parameter list; reload the plugin to pick it up",
                self.display_name
            );
        }

        let count = self.process_count.load(Ordering::Relaxed);
        if count != self.last_seen_process_count {
            self.last_seen_process_count = count;
            self.last_process_change = Instant::now();
            return;
        }
        if self.last_process_change.elapsed() < RT_IDLE_BEFORE_FLUSH {
            return;
        }
        let flush = self.host_data.flush_requested.swap(false, Ordering::AcqRel);
        let dirty = self.host_param_dirty.swap(false, Ordering::AcqRel);
        if flush || dirty {
            self.flush_params();
        }
    }

    /// Deliver pending host parameter changes (and collect plugin output
    /// events) via `params.flush` while the audio thread is excluded.
    fn flush_params(&mut self) {
        if !self.activated || self.params_ext.is_null() {
            return;
        }
        let Some(flush) = (unsafe { (*self.params_ext).flush }) else {
            return;
        };
        if !self.gate.suspend(Duration::from_millis(5)) {
            self.host_param_dirty.store(true, Ordering::Release);
            return;
        }
        let Self {
            rt,
            params,
            id_index,
            port_updates,
            plugin,
            ..
        } = self;
        collect_param_events(params, &port_updates.control_inputs, rt);
        rt.midi_events.clear();
        let in_data = InputEvents {
            params: &rt.param_events,
            midi: &rt.midi_events,
        };
        let in_events = in_data.as_clap();
        let out_ctx = OutputEvents {
            id_index,
            slots: &port_updates.control_inputs,
            last_applied: rt.last_applied.as_mut_ptr(),
        };
        let out_events = out_ctx.as_clap();
        // While active, flush is an audio-thread call; the gate guarantees
        // the real audio thread is not inside the plugin.
        threading::with_audio_thread(|| unsafe { flush(*plugin, &in_events, &out_events) });
        self.gate.resume();
    }

    /// True after the plugin called `host.request_restart`.
    pub fn needs_restart(&self) -> bool {
        self.host_data.restart_requested.load(Ordering::Acquire)
    }

    /// Deactivate and reactivate the plugin in place (main thread).
    ///
    /// Returns false if the plugin could not be brought back with the same
    /// audio port layout; the caller must then reload the instance (K3
    /// path).  If the audio thread could not be excluded in time nothing is
    /// changed, the request stays pending and true is returned.
    pub fn restart(&mut self) -> bool {
        if !self.host_data.restart_requested.swap(false, Ordering::AcqRel) && self.activated {
            return true;
        }
        if !self.gate.suspend(Duration::from_secs(1)) {
            self.host_data.restart_requested.store(true, Ordering::Release);
            return true;
        }
        unsafe {
            self.stop_and_deactivate();
            let (ins, outs) = query_audio_ports(self.plugin);
            let same_layout = ins == self.input_port_channels && outs == self.output_port_channels;
            let plugin_ref = &*self.plugin;
            self.activated = match plugin_ref.activate {
                Some(activate) => activate(self.plugin, self.sample_rate, 1, MAX_BLOCK as u32),
                None => true,
            };
            self.gate.resume();
            if !self.activated {
                log::error!("CLAP: re-activate failed for {}", self.display_name);
            }
            log::info!("CLAP: {} restarted", self.display_name);
            self.activated && same_layout
        }
    }

    /// stop_processing (as the audio thread) + deactivate.  The gate must be
    /// suspended or the filter already destroyed.
    unsafe fn stop_and_deactivate(&mut self) {
        unsafe {
            let plugin_ref = &*self.plugin;
            if self.rt.started {
                if let Some(stop) = plugin_ref.stop_processing {
                    threading::with_audio_thread(|| stop(self.plugin));
                }
                self.rt.started = false;
            }
            if self.activated {
                if let Some(deactivate) = plugin_ref.deactivate {
                    deactivate(self.plugin);
                }
                self.activated = false;
            }
        }
    }
}

impl Drop for ClapPluginInstance {
    fn drop(&mut self) {
        // The filter is destroyed before the instance, so the audio thread
        // can no longer enter process(); the gate just makes sure of it.
        super::ui::force_close_for_plugin(self.plugin);
        super::ui::remove_plugin_callbacks(self.plugin);
        self.gate.suspend(Duration::from_secs(2));
        unsafe {
            self.stop_and_deactivate();
            let plugin_ref = &*self.plugin;
            if let Some(destroy) = plugin_ref.destroy {
                destroy(self.plugin);
            }
        }
        super::ui::remove_plugin_callbacks(self.plugin);
    }
}

unsafe fn get_plugin_ext<T>(plugin: *const clap_plugin, id: &CStr) -> *const T {
    unsafe {
        match (*plugin).get_extension {
            Some(get_ext) => get_ext(plugin, id.as_ptr()) as *const T,
            None => std::ptr::null(),
        }
    }
}

/// Channel counts of the plugin's input and output audio ports.
unsafe fn query_audio_ports(plugin: *const clap_plugin) -> (Vec<usize>, Vec<usize>) {
    unsafe {
        let mut ins = Vec::new();
        let mut outs = Vec::new();
        let ext = get_plugin_ext::<clap_sys::ext::audio_ports::clap_plugin_audio_ports>(
            plugin,
            clap_sys::ext::audio_ports::CLAP_EXT_AUDIO_PORTS,
        );
        if ext.is_null() {
            return (ins, outs);
        }
        let ap = &*ext;
        let (Some(count_fn), Some(get_fn)) = (ap.count, ap.get) else {
            return (ins, outs);
        };
        for (is_input, list) in [(true, &mut ins), (false, &mut outs)] {
            for idx in 0..count_fn(plugin, is_input) {
                let mut info: clap_sys::ext::audio_ports::clap_audio_port_info = std::mem::zeroed();
                if get_fn(plugin, idx, is_input, &mut info) {
                    list.push(info.channel_count as usize);
                }
            }
        }
        (ins, outs)
    }
}

/// Diff slots against `rt.last_applied` and fill `rt.param_events`.
/// Preallocated: never pushes beyond capacity.
fn collect_param_events(params: &[ClapParam], slots: &[PortSlot], rt: &mut RtState) {
    rt.param_events.clear();
    for ((p, slot), last) in params.iter().zip(slots).zip(rt.last_applied.iter_mut()) {
        let v = slot.value.load() as f64;
        if v != *last {
            if rt.param_events.len() == rt.param_events.capacity() {
                break;
            }
            *last = v;
            rt.param_events.push(clap_event_param_value {
                header: event_header::<clap_event_param_value>(CLAP_EVENT_PARAM_VALUE, 0),
                param_id: p.id,
                cookie: std::ptr::null_mut(),
                note_id: -1,
                port_index: -1,
                channel: -1,
                key: -1,
                value: v,
            });
        }
    }
}

fn event_header<T>(type_: u16, time: u32) -> clap_event_header {
    clap_event_header {
        size: std::mem::size_of::<T>() as u32,
        time,
        space_id: CLAP_CORE_EVENT_SPACE_ID,
        type_,
        flags: 0,
    }
}

// ---- Input events (params first at time 0, then MIDI in time order) ----

struct InputEvents<'a> {
    params: &'a [clap_event_param_value],
    midi: &'a [clap_event_midi],
}

impl InputEvents<'_> {
    fn as_clap(&self) -> clap_input_events {
        clap_input_events {
            ctx: self as *const Self as *mut c_void,
            size: Some(input_events_size),
            get: Some(input_events_get),
        }
    }
}

unsafe extern "C" fn input_events_size(list: *const clap_input_events) -> u32 {
    unsafe {
        let data = &*((*list).ctx as *const InputEvents);
        (data.params.len() + data.midi.len()) as u32
    }
}

unsafe extern "C" fn input_events_get(
    list: *const clap_input_events,
    index: u32,
) -> *const clap_event_header {
    unsafe {
        let data = &*((*list).ctx as *const InputEvents);
        let idx = index as usize;
        if let Some(e) = data.params.get(idx) {
            return &e.header;
        }
        match data.midi.get(idx - data.params.len()) {
            Some(e) => &e.header,
            None => std::ptr::null(),
        }
    }
}

// ---- Output events: plugin-originated parameter changes → slots ----

struct OutputEvents<'a> {
    id_index: &'a [(clap_id, usize)],
    slots: &'a [PortSlot],
    /// RT-private last-applied values (so the change isn't echoed back).
    last_applied: *mut f64,
}

impl OutputEvents<'_> {
    fn as_clap(&self) -> clap_output_events {
        clap_output_events {
            ctx: self as *const Self as *mut c_void,
            try_push: Some(output_events_try_push),
        }
    }
}

unsafe extern "C" fn output_events_try_push(
    list: *const clap_output_events,
    event: *const clap_event_header,
) -> bool {
    unsafe {
        if list.is_null() || event.is_null() || (*list).ctx.is_null() {
            return false;
        }
        let ctx = &*((*list).ctx as *const OutputEvents);
        let hdr = &*event;
        if hdr.space_id == CLAP_CORE_EVENT_SPACE_ID && hdr.type_ == CLAP_EVENT_PARAM_VALUE {
            let ev = &*(event as *const clap_event_param_value);
            if let Ok(pos) = ctx.id_index.binary_search_by_key(&ev.param_id, |&(id, _)| id) {
                let idx = ctx.id_index[pos].1;
                if let Some(slot) = ctx.slots.get(idx) {
                    let v = ev.value as f32;
                    slot.value.store(v);
                    *ctx.last_applied.add(idx) = v as f64;
                }
            }
        }
        true
    }
}

// ---- State streams ----

unsafe extern "C" fn ostream_write(
    stream: *const clap_ostream,
    buffer: *const c_void,
    size: u64,
) -> i64 {
    unsafe {
        if stream.is_null() || (buffer.is_null() && size > 0) {
            return -1;
        }
        let buf = &mut *((*stream).ctx as *mut Vec<u8>);
        let n = size.min(i64::MAX as u64) as usize;
        if n > 0 {
            buf.extend_from_slice(std::slice::from_raw_parts(buffer as *const u8, n));
        }
        n as i64
    }
}

struct StreamReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl StreamReader<'_> {
    /// Copy up to `out.len()` bytes; returns the count (0 at end of stream).
    fn read_into(&mut self, out: &mut [u8]) -> usize {
        let n = out.len().min(self.data.len() - self.pos);
        out[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        n
    }
}

unsafe extern "C" fn istream_read(
    stream: *const clap_istream,
    buffer: *mut c_void,
    size: u64,
) -> i64 {
    unsafe {
        if stream.is_null() || (buffer.is_null() && size > 0) {
            return -1;
        }
        let reader = &mut *((*stream).ctx as *mut StreamReader);
        let n = size.min(i64::MAX as u64) as usize;
        if n == 0 {
            return 0;
        }
        let out = std::slice::from_raw_parts_mut(buffer as *mut u8, n);
        reader.read_into(out) as i64
    }
}

// ---- Host callbacks ----

unsafe fn host_data<'a>(host: *const clap_host) -> Option<&'a HostData> {
    unsafe {
        if host.is_null() || (*host).host_data.is_null() {
            None
        } else {
            Some(&*((*host).host_data as *const HostData))
        }
    }
}

static CLAP_HOST_PARAMS: clap_host_params = clap_host_params {
    rescan: Some(host_params_rescan),
    clear: Some(host_params_clear),
    request_flush: Some(host_params_request_flush),
};

static CLAP_HOST_STATE: clap_host_state = clap_host_state {
    mark_dirty: Some(host_state_mark_dirty),
};

static CLAP_HOST_LOG: clap_sys::ext::log::clap_host_log = clap_sys::ext::log::clap_host_log {
    log: Some(host_log),
};

unsafe extern "C" fn host_get_extension(
    _host: *const clap_host,
    extension_id: *const std::ffi::c_char,
) -> *const c_void {
    unsafe {
        if extension_id.is_null() {
            return std::ptr::null();
        }
        let ext_id = CStr::from_ptr(extension_id);
        if ext_id == clap_sys::ext::gui::CLAP_EXT_GUI {
            return &super::ui::CLAP_HOST_GUI as *const _ as *const c_void;
        }
        if ext_id == clap_sys::ext::timer_support::CLAP_EXT_TIMER_SUPPORT {
            return &super::ui::CLAP_HOST_TIMER_SUPPORT as *const _ as *const c_void;
        }
        if ext_id == clap_sys::ext::posix_fd_support::CLAP_EXT_POSIX_FD_SUPPORT {
            return &super::ui::CLAP_HOST_POSIX_FD_SUPPORT as *const _ as *const c_void;
        }
        if ext_id == clap_sys::ext::thread_check::CLAP_EXT_THREAD_CHECK {
            return &super::ui::CLAP_HOST_THREAD_CHECK as *const _ as *const c_void;
        }
        if ext_id == clap_sys::ext::params::CLAP_EXT_PARAMS {
            return &CLAP_HOST_PARAMS as *const _ as *const c_void;
        }
        if ext_id == clap_sys::ext::state::CLAP_EXT_STATE {
            return &CLAP_HOST_STATE as *const _ as *const c_void;
        }
        if ext_id == clap_sys::ext::log::CLAP_EXT_LOG {
            return &CLAP_HOST_LOG as *const _ as *const c_void;
        }
        std::ptr::null()
    }
}

unsafe extern "C" fn host_request_restart(host: *const clap_host) {
    log::debug!("CLAP: host_request_restart");
    if let Some(hd) = unsafe { host_data(host) } {
        hd.restart_requested.store(true, Ordering::Release);
    }
}

unsafe extern "C" fn host_request_process(_host: *const clap_host) {
    // The PipeWire graph drives processing continuously.
}

unsafe extern "C" fn host_request_callback(host: *const clap_host) {
    if let Some(hd) = unsafe { host_data(host) } {
        hd.callback_requested.store(true, Ordering::Release);
    }
}

unsafe extern "C" fn host_params_rescan(host: *const clap_host, flags: clap_param_rescan_flags) {
    if let Some(hd) = unsafe { host_data(host) } {
        hd.rescan_flags.fetch_or(flags, Ordering::AcqRel);
    }
}

unsafe extern "C" fn host_params_clear(
    _host: *const clap_host,
    _param_id: clap_id,
    _flags: clap_param_clear_flags,
) {
    // No automation or modulation is stored by the host.
}

unsafe extern "C" fn host_params_request_flush(host: *const clap_host) {
    if let Some(hd) = unsafe { host_data(host) } {
        hd.flush_requested.store(true, Ordering::Release);
    }
}

unsafe extern "C" fn host_state_mark_dirty(_host: *const clap_host) {
    // State is snapshotted on demand (K2); nothing to track.
}

unsafe extern "C" fn host_log(
    _host: *const clap_host,
    severity: clap_sys::ext::log::clap_log_severity,
    msg: *const std::ffi::c_char,
) {
    use clap_sys::ext::log::*;
    // Logging may allocate; don't do it from the audio thread.
    if msg.is_null() || (threading::is_audio_thread() && !threading::is_main_thread()) {
        return;
    }
    let text = unsafe { CStr::from_ptr(msg) }.to_string_lossy();
    match severity {
        CLAP_LOG_DEBUG => log::debug!("CLAP plugin: {}", text),
        CLAP_LOG_INFO => log::info!("CLAP plugin: {}", text),
        CLAP_LOG_WARNING => log::warn!("CLAP plugin: {}", text),
        _ => log::error!("CLAP plugin: {}", text),
    }
}

fn read_clap_name(name: &[std::ffi::c_char]) -> String {
    let bytes: Vec<u8> = name.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8(bytes).unwrap_or_else(|_| "?".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param(id: u32, idx: usize) -> ClapParam {
        ClapParam {
            id,
            port_index: idx,
            name: String::new(),
            min: 0.0,
            max: 1.0,
            default: 0.0,
            is_toggle: false,
        }
    }

    fn slots(vals: &[f32]) -> Vec<PortSlot> {
        vals.iter()
            .enumerate()
            .map(|(i, &v)| PortSlot {
                port_index: i,
                value: AtomicF32::new(v),
            })
            .collect()
    }

    #[test]
    fn param_events_only_for_changed_slots() {
        let params = vec![param(10, 0), param(20, 1)];
        let s = slots(&[0.25, 0.5]);
        let mut rt = RtState::new(&[], &[], vec![0.25, 0.5]);

        collect_param_events(&params, &s, &mut rt);
        assert!(rt.param_events.is_empty());

        s[1].value.store(0.75);
        collect_param_events(&params, &s, &mut rt);
        assert_eq!(rt.param_events.len(), 1);
        assert_eq!(rt.param_events[0].param_id, 20);
        assert_eq!(rt.param_events[0].value, 0.75);

        // Applied once; no repeat on the next block.
        collect_param_events(&params, &s, &mut rt);
        assert!(rt.param_events.is_empty());
    }

    #[test]
    fn param_events_never_exceed_capacity() {
        let params = vec![param(1, 0), param(2, 1)];
        let s = slots(&[1.0, 1.0]);
        let mut rt = RtState::new(&[], &[], vec![0.0, 0.0]);
        let cap = rt.param_events.capacity();
        collect_param_events(&params, &s, &mut rt);
        assert_eq!(rt.param_events.len(), 2);
        assert_eq!(rt.param_events.capacity(), cap);
    }

    #[test]
    fn output_param_event_updates_slot_and_last_applied() {
        let s = slots(&[0.0, 0.0]);
        let mut last = vec![0.0f64, 0.0];
        let index = vec![(5u32, 1usize), (7u32, 0usize)];
        let ctx = OutputEvents {
            id_index: &index,
            slots: &s,
            last_applied: last.as_mut_ptr(),
        };
        let out = ctx.as_clap();
        let ev = clap_event_param_value {
            header: event_header::<clap_event_param_value>(CLAP_EVENT_PARAM_VALUE, 0),
            param_id: 5,
            cookie: std::ptr::null_mut(),
            note_id: -1,
            port_index: -1,
            channel: -1,
            key: -1,
            value: 0.5,
        };
        assert!(unsafe { output_events_try_push(&out, &ev.header) });
        assert_eq!(s[1].value.load(), 0.5);
        assert_eq!(last[1], 0.5);
        assert_eq!(s[0].value.load(), 0.0);
    }

    #[test]
    fn rt_buffers_point_into_flat_channel_arrays() {
        let rt = RtState::new(&[2, 1], &[2], Vec::new());
        assert_eq!(rt.in_bufs.len(), 2);
        assert_eq!(rt.in_bufs[0].channel_count, 2);
        assert_eq!(rt.in_bufs[1].data32, unsafe { rt.in_ptrs.as_ptr().add(2) as *mut _ });
        assert_eq!(rt.out_bufs[0].data32, rt.out_ptrs.as_ptr() as *mut _);
        assert_eq!(rt.silence.len(), MAX_BLOCK);
    }

    #[test]
    fn state_streams_roundtrip_with_partial_reads() {
        let mut buf: Vec<u8> = Vec::new();
        let os = clap_ostream {
            ctx: &mut buf as *mut Vec<u8> as *mut c_void,
            write: Some(ostream_write),
        };
        let data = b"hello clap state";
        unsafe {
            assert_eq!(ostream_write(&os, data.as_ptr() as *const c_void, 5), 5);
            assert_eq!(
                ostream_write(&os, data[5..].as_ptr() as *const c_void, (data.len() - 5) as u64),
                (data.len() - 5) as i64
            );
        }
        assert_eq!(&buf, data);

        let mut reader = StreamReader { data: &buf, pos: 0 };
        let is = clap_istream {
            ctx: &mut reader as *mut StreamReader as *mut c_void,
            read: Some(istream_read),
        };
        let mut out = [0u8; 10];
        unsafe {
            assert_eq!(istream_read(&is, out.as_mut_ptr() as *mut c_void, 10), 10);
            assert_eq!(&out, &data[..10]);
            assert_eq!(istream_read(&is, out.as_mut_ptr() as *mut c_void, 10), 6);
            assert_eq!(&out[..6], &data[10..]);
            assert_eq!(istream_read(&is, out.as_mut_ptr() as *mut c_void, 10), 0);
        }
    }
}
