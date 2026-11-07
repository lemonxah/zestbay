//! VST2 lifecycle and audio processing. The main thread owns the instance;
//! audio processing uses shared references and an UnsafeCell protected by
//! RtGate. Host callbacks only access immutable data and atomics.

use std::cell::{Cell, UnsafeCell};
use std::ffi::{CStr, CString, c_void};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use super::abi::{self, eff, *};
use crate::clap::threading::{self, RtGate};
use crate::midi::processing::{MAX_MIDI_EVENTS, RawMidiEvent};
use crate::midi::rt::MAX_BLOCK;
use crate::plugin::types::*;

const MAX_STATE: usize = 64 * 1024 * 1024;

thread_local! {
    // During VSTPluginMain the plugin can call the host with a null effect.
    static CONSTRUCTING: Cell<*const HostData> = const { Cell::new(std::ptr::null()) };
    // Each caller gets its own time record; editor queries cannot race audio.
    static TIME: UnsafeCell<TimeInfo> = UnsafeCell::new(TimeInfo::default());
}

pub(super) struct HostData {
    sample_rate: f64,
    current_id: i32,
    directory: CString,
    slots: OnceLock<SharedPortUpdates>,
    applied: OnceLock<Vec<AtomicF32>>,
    restart: AtomicBool,
    refresh: AtomicBool,
    pub resize: AtomicU64,
    position: AtomicU64,
}

unsafe extern "C" fn host_callback(
    effect: *mut Effect,
    opcode: i32,
    index: i32,
    value: isize,
    ptr: *mut c_void,
    opt: f32,
) -> isize {
    // These queries are valid even before the plugin has returned AEffect.
    match opcode {
        1 => return 2400,
        32 | 33 => {
            if !ptr.is_null() {
                unsafe { std::ptr::copy_nonoverlapping(c"ZestBay".as_ptr(), ptr.cast(), 8) };
                return 1;
            }
            return 0;
        }
        34 => return 1200,
        37 => {
            if ptr.is_null() {
                return 0;
            }
            let name = unsafe { CStr::from_ptr(ptr.cast()) }.to_bytes();
            return match name {
                b"sendVstEvents" | b"sendVstMidiEvent" | b"sendVstTimeInfo" | b"sizeWindow"
                | b"startStopProcess" | b"shellCategory" | b"supportShell" => 1,
                _ => 0,
            };
        }
        38 => return 1, // English
        _ => {}
    }
    let data = CONSTRUCTING.with(|slot| slot.get());
    let data = if !data.is_null() {
        data
    } else if !effect.is_null() {
        unsafe { (*effect).reserved1 as *const HostData }
    } else {
        std::ptr::null()
    };
    let Some(data) = (unsafe { data.as_ref() }) else {
        return 0;
    };
    match opcode {
        0 => {
            // audioMasterAutomate, including edits from the native GUI
            if opt.is_finite() && index >= 0 {
                if let Some(slot) = data
                    .slots
                    .get()
                    .and_then(|s| s.control_inputs.get(index as usize))
                {
                    let value = opt.clamp(0.0, 1.0);
                    if let Some(applied) = data.applied.get().and_then(|a| a.get(index as usize)) {
                        applied.store(value);
                    }
                    slot.value.store(value);
                }
            }
            0
        }
        2 => data.current_id as isize,
        6 => 1, // legacy wantMidi
        7 => TIME.with(|time| unsafe {
            let info = &mut *time.get();
            *info = TimeInfo {
                sample_pos: data.position.load(Ordering::Relaxed) as f64,
                sample_rate: data.sample_rate,
                // ZestBay has no DAW transport; no tempo/PPQ validity flags.
                ..TimeInfo::default()
            };
            time.get() as isize
        }),
        13 => {
            data.restart.store(true, Ordering::Release);
            1
        }
        15 if index > 0 && index <= 16384 && value > 0 && value <= 16384 => {
            data.resize
                .store(((index as u64) << 32) | value as u64, Ordering::Release);
            1
        }
        16 => data.sample_rate as isize,
        17 => MAX_BLOCK as isize,
        22 => 1, // replacing mode
        23 => {
            if threading::is_audio_thread() {
                2
            } else {
                1
            }
        }
        24 => 1, // automation off
        41 => data.directory.as_ptr() as isize,
        42 => {
            data.refresh.store(true, Ordering::Release);
            1
        }
        43 | 44 => 1, // begin/end parameter edit
        // MIDI output, file selectors and offline processing are unsupported.
        _ => 0,
    }
}

struct Library(*mut c_void);

impl Drop for Library {
    fn drop(&mut self) {
        unsafe {
            libc::dlclose(self.0);
        }
    }
}

pub(super) struct LoadedEffect {
    pub raw: *mut Effect,
    pub host: Box<HostData>,
    _library: Library,
}

impl LoadedEffect {
    /// Main thread, or an isolated scanner process. The module must match
    /// this process's architecture. `id` selects a child of a shell plugin.
    pub unsafe fn open(path: &Path, id: i32, sample_rate: f64) -> Option<Self> {
        use std::os::unix::ffi::OsStrExt;
        let path_c = CString::new(path.as_os_str().as_bytes()).ok()?;
        let handle = unsafe { libc::dlopen(path_c.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if handle.is_null() {
            log::debug!("VST2: cannot load {}", path.display());
            return None;
        }
        let library = Library(handle);
        let mut entry = unsafe { libc::dlsym(handle, c"VSTPluginMain".as_ptr()) };
        if entry.is_null() {
            entry = unsafe { libc::dlsym(handle, c"main".as_ptr()) };
        }
        if entry.is_null() {
            return None;
        }
        let entry: Entry = unsafe { std::mem::transmute(entry) };
        let host = Box::new(HostData {
            sample_rate,
            current_id: id,
            directory: CString::new(path.parent()?.as_os_str().as_bytes()).ok()?,
            slots: OnceLock::new(),
            applied: OnceLock::new(),
            restart: AtomicBool::new(false),
            refresh: AtomicBool::new(false),
            resize: AtomicU64::new(0),
            position: AtomicU64::new(0),
        });
        let raw = CONSTRUCTING.with(|slot| {
            let previous = slot.replace(&*host);
            let raw = unsafe { entry(host_callback) };
            slot.set(previous);
            raw
        });
        if raw.is_null() || unsafe { (*raw).magic != MAGIC || (*raw).dispatcher.is_none() } {
            return None;
        }
        unsafe {
            (*raw).reserved1 = (&*host as *const HostData) as isize;
        }
        let loaded = Self {
            raw,
            host,
            _library: library,
        };
        unsafe {
            command(raw, eff::OPEN, 0);
            dispatch(
                raw,
                eff::SAMPLE_RATE,
                0,
                0,
                std::ptr::null_mut(),
                sample_rate as f32,
            );
            command(raw, eff::BLOCK_SIZE, MAX_BLOCK as isize);
        }
        if !loaded.valid_counts() {
            return None;
        }
        Some(loaded)
    }

    pub fn valid_counts(&self) -> bool {
        unsafe {
            (0..=MAX_CHANNELS).contains(&(*self.raw).num_inputs)
                && (0..=MAX_CHANNELS).contains(&(*self.raw).num_outputs)
                && (0..=MAX_PARAMS).contains(&(*self.raw).num_params)
                && (0..=65536).contains(&(*self.raw).num_programs)
        }
    }

    pub fn can_process(&self) -> bool {
        unsafe { (*self.raw).process_replacing.is_some() || (*self.raw).process.is_some() }
    }

    pub fn accepts_midi(&self) -> bool {
        unsafe {
            (*self.raw).flags & IS_SYNTH != 0
                || dispatch(
                    self.raw,
                    eff::CAN_DO,
                    0,
                    0,
                    c"receiveVstMidiEvent".as_ptr().cast_mut().cast(),
                    0.0,
                ) > 0
                || dispatch(
                    self.raw,
                    eff::CAN_DO,
                    0,
                    0,
                    c"receiveVstEvents".as_ptr().cast_mut().cast(),
                    0.0,
                ) > 0
        }
    }

    pub fn parameters(&self) -> Vec<Vst2Param> {
        let mut params = Vec::new();
        unsafe {
            for i in 0..(*self.raw).num_params {
                let mut name = abi::text(self.raw, eff::PARAM_NAME, i);
                if name.is_empty() {
                    name = format!("Parameter {}", i + 1);
                }
                let mut properties: ParameterProperties = std::mem::zeroed();
                let has = dispatch(
                    self.raw,
                    eff::PARAM_PROPERTIES,
                    i,
                    0,
                    (&mut properties as *mut ParameterProperties).cast(),
                    0.0,
                ) != 0;
                let is_toggle = has && properties.flags & 1 != 0;
                let steps = if is_toggle {
                    1
                } else if has && properties.flags & 2 != 0 {
                    let span =
                        i64::from(properties.max_integer) - i64::from(properties.min_integer);
                    let step = if properties.flags & 8 != 0 {
                        properties.step_integer.max(1)
                    } else {
                        1
                    };
                    (span.max(0) / i64::from(step)).min(u32::MAX as i64) as u32
                } else {
                    0
                };
                params.push(Vst2Param {
                    port_index: i as usize,
                    name,
                    default: self.parameter(i),
                    is_toggle,
                    step_count: steps,
                });
            }
        }
        params
    }

    fn parameter(&self, index: i32) -> f32 {
        let v = unsafe {
            (*self.raw)
                .get_parameter
                .map_or(0.0, |get| get(self.raw, index))
        };
        if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

impl Drop for LoadedEffect {
    fn drop(&mut self) {
        // CLOSE may delete AEffect itself. Never dereference it afterwards.
        unsafe {
            command(self.raw, eff::CLOSE, 0);
        }
    }
}

#[derive(Clone)]
pub struct Vst2Param {
    pub port_index: usize,
    pub name: String,
    pub default: f32,
    pub is_toggle: bool,
    pub step_count: u32,
}

struct AudioState {
    inputs: Vec<f32>,
    outputs: Vec<f32>,
    input_ptrs: Vec<*mut f32>,
    output_ptrs: Vec<*mut f32>,
    midi: Box<[MidiEvent; MAX_MIDI_EVENTS]>,
    events: Events<MAX_MIDI_EVENTS>,
}

impl AudioState {
    fn new(inputs: usize, outputs: usize) -> Self {
        Self {
            inputs: vec![0.0; inputs * MAX_BLOCK],
            outputs: vec![0.0; outputs * MAX_BLOCK],
            input_ptrs: vec![std::ptr::null_mut(); inputs],
            output_ptrs: vec![std::ptr::null_mut(); outputs],
            midi: Box::new([MidiEvent::default(); MAX_MIDI_EVENTS]),
            events: Events {
                count: 0,
                reserved: 0,
                events: [std::ptr::null(); MAX_MIDI_EVENTS],
            },
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SavedState {
    version: u32,
    unique_id: i32,
    program: Option<i32>,
    chunk: Option<Vec<u8>>,
    parameters: Vec<f32>,
}

pub struct Vst2PluginInstance {
    loaded: LoadedEffect,
    pub display_name: String,
    pub audio_input_channels: usize,
    pub audio_output_channels: usize,
    pub has_midi_in: bool,
    pub has_midi_out: bool,
    pub params: Vec<Vst2Param>,
    pub port_updates: SharedPortUpdates,
    pub sample_rate: f64,
    gate: RtGate,
    bypassed: AtomicBool,
    params_dirty: AtomicBool,
    parked: Cell<bool>,
    // Accessed only by the audio callback, or main while the gate is suspended.
    audio: UnsafeCell<AudioState>,
}

impl Vst2PluginInstance {
    /// Calls untrusted plugin code. Must run on the plugin main thread.
    pub unsafe fn new(
        library_path: &str,
        plugin_id: &str,
        info: &PluginInfo,
        sample_rate: f64,
        state: Option<&[u8]>,
    ) -> Option<Self> {
        threading::set_main_thread();
        let id = super::scanner::parse_id(plugin_id)?;
        if !sample_rate.is_finite() || sample_rate <= 0.0 {
            return None;
        }
        let loaded = unsafe { LoadedEffect::open(Path::new(library_path), id, sample_rate) }?;
        if unsafe { (*loaded.raw).unique_id != id } || !loaded.can_process() {
            return None;
        }
        let defaults = loaded.parameters();
        if let Some(blob) = state {
            if !restore_state(&loaded, blob) {
                return None;
            }
        }
        if !loaded.valid_counts() {
            return None;
        }
        let mut params = loaded.parameters();
        for (p, default) in params.iter_mut().zip(&defaults) {
            p.default = default.default;
        }
        let values: Vec<f32> = (0..params.len())
            .map(|i| loaded.parameter(i as i32))
            .collect();
        let port_updates = Arc::new(PortUpdates {
            control_inputs: values
                .iter()
                .enumerate()
                .map(|(i, v)| PortSlot {
                    port_index: i,
                    value: AtomicF32::new(*v),
                })
                .collect(),
            control_outputs: Vec::new(),
            atom_inputs: Vec::new(),
            atom_outputs: Vec::new(),
        });
        let _ = loaded
            .host
            .applied
            .set(values.iter().map(|v| AtomicF32::new(*v)).collect());
        let _ = loaded.host.slots.set(port_updates.clone());
        let inputs = unsafe { (*loaded.raw).num_inputs as usize };
        let outputs = unsafe { (*loaded.raw).num_outputs as usize };
        let has_midi_in = loaded.accepts_midi();
        loaded.host.restart.store(false, Ordering::Release);
        unsafe {
            command(loaded.raw, eff::MAINS, 1);
            command(loaded.raw, eff::START, 0);
        }
        Some(Self {
            loaded,
            display_name: info.name.clone(),
            audio_input_channels: inputs,
            audio_output_channels: outputs,
            has_midi_in,
            has_midi_out: false,
            params,
            port_updates,
            sample_rate,
            gate: RtGate::new(),
            bypassed: AtomicBool::new(false),
            params_dirty: AtomicBool::new(false),
            parked: Cell::new(false),
            audio: UnsafeCell::new(AudioState::new(inputs, outputs)),
        })
    }

    pub fn effect_ptr(&self) -> *mut Effect {
        self.loaded.raw
    }

    pub fn set_parameter(&self, port_index: usize, value: f32) {
        if value.is_finite() {
            if let Some(slot) = self.port_updates.control_inputs.get(port_index) {
                slot.value.store(value.clamp(0.0, 1.0));
                self.params_dirty.store(true, Ordering::Release);
            }
        }
    }

    pub fn set_bypassed(&self, bypassed: bool) {
        self.bypassed.store(bypassed, Ordering::Relaxed);
    }
    pub fn is_bypassed(&self) -> bool {
        self.bypassed.load(Ordering::Relaxed)
    }

    pub fn get_parameters(&self) -> Vec<ParameterValue> {
        self.params
            .iter()
            .zip(&self.port_updates.control_inputs)
            .map(|(p, slot)| ParameterValue {
                port_index: p.port_index,
                symbol: format!("param_{}", p.port_index),
                name: p.name.clone(),
                value: slot.value.load(),
                min: 0.0,
                max: 1.0,
                default: p.default,
                is_toggle: p.is_toggle,
            })
            .collect()
    }

    /// Audio thread only. Input/output slices are valid for `n` frames.
    /// All scratch and event storage is allocated during construction.
    pub unsafe fn process(
        &self,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        n: usize,
        midi: &[RawMidiEvent],
    ) {
        threading::mark_audio_thread();
        let n = n.min(MAX_BLOCK);
        if n == 0 {
            return;
        }
        if !self.gate.enter() {
            threading::passthrough(inputs, outputs, n);
            return;
        }
        if self.needs_restart() {
            threading::passthrough(inputs, outputs, n);
            self.gate.leave();
            return;
        }
        let audio = unsafe { &mut *self.audio.get() };
        self.params_dirty.store(false, Ordering::Release);
        self.apply_parameters();
        let raw = self.loaded.raw;
        // IOChanged can arrive during parameter changes. Avoid calling into
        // an incompatible layout until the manager has reloaded the instance.
        if self.needs_restart() {
            threading::passthrough(inputs, outputs, n);
            self.gate.leave();
            return;
        }
        for i in 0..self.audio_input_channels {
            let buf = &mut audio.inputs[i * MAX_BLOCK..i * MAX_BLOCK + n];
            buf.fill(0.0);
            if let Some(input) = inputs.get(i) {
                let count = input.len().min(n);
                buf[..count].copy_from_slice(&input[..count]);
            }
            audio.input_ptrs[i] = buf.as_mut_ptr();
        }
        for i in 0..self.audio_output_channels {
            let buf = &mut audio.outputs[i * MAX_BLOCK..i * MAX_BLOCK + n];
            buf.fill(0.0); // also supports pre-2.4 accumulating process()
            audio.output_ptrs[i] = buf.as_mut_ptr();
        }
        if self.has_midi_in {
            audio.events.count = 0;
            for event in midi
                .iter()
                .filter(|e| (1..=3).contains(&e.size))
                .take(MAX_MIDI_EVENTS)
            {
                let i = audio.events.count as usize;
                audio.midi[i] = MidiEvent {
                    kind: 1,
                    byte_size: std::mem::size_of::<MidiEvent>() as i32,
                    delta_frames: event.offset.min(n as u32 - 1) as i32,
                    flags: 1,
                    data: [event.data[0], event.data[1], event.data[2], 0],
                    ..MidiEvent::default()
                };
                audio.events.events[i] = &audio.midi[i];
                audio.events.count += 1;
            }
            if audio.events.count > 0 {
                unsafe {
                    dispatch(
                        raw,
                        eff::EVENTS,
                        0,
                        0,
                        (&mut audio.events as *mut Events<MAX_MIDI_EVENTS>).cast(),
                        0.0,
                    );
                }
            }
        }
        if !self.needs_restart() {
            unsafe {
                if let Some(process) = (*raw).process_replacing.or((*raw).process) {
                    process(
                        raw,
                        audio.input_ptrs.as_mut_ptr(),
                        audio.output_ptrs.as_mut_ptr(),
                        n as i32,
                    );
                }
            }
        }
        if self.is_bypassed() || self.needs_restart() {
            threading::passthrough(inputs, outputs, n);
        } else {
            for (i, output) in outputs.iter_mut().enumerate() {
                let count = output.len().min(n);
                if i < self.audio_output_channels {
                    output[..count]
                        .copy_from_slice(&audio.outputs[i * MAX_BLOCK..i * MAX_BLOCK + count]);
                } else {
                    output[..count].fill(0.0);
                }
            }
        }
        self.loaded
            .host
            .position
            .fetch_add(n as u64, Ordering::Relaxed);
        self.gate.leave();
    }

    fn apply_parameters(&self) {
        let applied = self
            .loaded
            .host
            .applied
            .get()
            .expect("initialized before activation");
        for (i, slot) in self.port_updates.control_inputs.iter().enumerate() {
            let value = slot.value.load();
            if value.is_finite() && value != applied[i].load() {
                // Publish before calling the plugin: a synchronous automate
                // callback can report a quantized value and must win.
                applied[i].store(value);
                unsafe {
                    if let Some(set) = (*self.loaded.raw).set_parameter {
                        set(self.loaded.raw, i as i32, value.clamp(0.0, 1.0));
                    }
                }
            }
        }
    }

    /// Main-thread flushing also handles knobs when PipeWire is idle.
    pub fn idle(&self) {
        if self.parked.get() || self.needs_restart() {
            return;
        }
        if !self.params_dirty.load(Ordering::Acquire)
            && !self.loaded.host.refresh.load(Ordering::Acquire)
        {
            return;
        }
        if !self.gate.suspend(Duration::from_millis(2)) {
            return;
        }
        self.params_dirty.store(false, Ordering::Release);
        self.apply_parameters();
        if self.loaded.host.refresh.swap(false, Ordering::AcqRel) {
            for (i, slot) in self.port_updates.control_inputs.iter().enumerate() {
                let value = self.loaded.parameter(i as i32);
                slot.value.store(value);
                self.loaded.host.applied.get().unwrap()[i].store(value);
            }
        }
        self.gate.resume();
    }

    pub fn needs_restart(&self) -> bool {
        self.loaded.host.restart.load(Ordering::Acquire)
    }
    pub fn is_parked(&self) -> bool {
        self.parked.get()
    }

    pub fn restart(&self) -> bool {
        if self.parked.get() {
            return false;
        }
        if !self.gate.suspend(Duration::from_secs(1)) {
            return true;
        }
        self.loaded.host.restart.store(false, Ordering::Release);
        let raw = self.loaded.raw;
        unsafe {
            command(raw, eff::STOP, 0);
            command(raw, eff::MAINS, 0);
            if !self.loaded.valid_counts()
                || (*raw).num_inputs as usize != self.audio_input_channels
                || (*raw).num_outputs as usize != self.audio_output_channels
                || (*raw).num_params as usize != self.params.len()
            {
                self.parked.set(true);
                return false;
            }
            command(raw, eff::MAINS, 1);
            command(raw, eff::START, 0);
        }
        self.gate.resume();
        true
    }

    /// State calls are serialized with audio; no plugin state methods run
    /// concurrently with processReplacing or setParameter.
    pub fn get_state(&self) -> Option<Vec<u8>> {
        let already_suspended = self.parked.get();
        if !already_suspended && !self.gate.suspend(Duration::from_secs(1)) {
            return None;
        }
        let result = self.save_suspended();
        if !already_suspended {
            self.gate.resume();
        }
        result
    }

    fn save_suspended(&self) -> Option<Vec<u8>> {
        if !self.loaded.valid_counts() {
            return None;
        }
        if !self.needs_restart()
            && unsafe { (*self.loaded.raw).num_params as usize == self.params.len() }
        {
            self.apply_parameters();
        }
        let raw = self.loaded.raw;
        unsafe {
            let chunk = if (*raw).flags & PROGRAM_CHUNKS != 0 {
                let mut ptr: *mut c_void = std::ptr::null_mut();
                let size = dispatch(
                    raw,
                    eff::GET_CHUNK,
                    0,
                    0,
                    (&mut ptr as *mut *mut c_void).cast(),
                    0.0,
                );
                if size <= 0 || size as usize > MAX_STATE || ptr.is_null() {
                    return None;
                }
                Some(std::slice::from_raw_parts(ptr.cast::<u8>(), size as usize).to_vec())
            } else {
                None
            };
            let program = if (*raw).num_programs > 0 {
                Some(command(raw, eff::GET_PROGRAM, 0) as i32)
            } else {
                None
            };
            serde_json::to_vec(&SavedState {
                version: 1,
                unique_id: (*raw).unique_id,
                program,
                chunk,
                parameters: (0..(*raw).num_params)
                    .map(|i| self.loaded.parameter(i))
                    .collect(),
            })
            .ok()
        }
    }
}

impl Drop for Vst2PluginInstance {
    fn drop(&mut self) {
        // The filter is destroyed before its instance. All GUI work is on
        // this same thread, and must finish before CLOSE unloads its code.
        super::ui::force_close_for_plugin(self.loaded.raw);
        unsafe {
            command(self.loaded.raw, eff::STOP, 0);
            command(self.loaded.raw, eff::MAINS, 0);
        }
    }
}

fn restore_state(loaded: &LoadedEffect, blob: &[u8]) -> bool {
    if blob.len() > MAX_STATE * 5 {
        return false;
    }
    let Ok(state) = serde_json::from_slice::<SavedState>(blob) else {
        return false;
    };
    let raw = loaded.raw;
    unsafe {
        if state.version != 1 || state.unique_id != (*raw).unique_id {
            return false;
        }
        if let Some(chunk) = state.chunk {
            if (*raw).flags & PROGRAM_CHUNKS == 0 || chunk.is_empty() || chunk.len() > MAX_STATE {
                return false;
            }
            // Many plugins return 0 even after successfully restoring a chunk.
            dispatch(
                raw,
                eff::SET_CHUNK,
                0,
                chunk.len() as isize,
                chunk.as_ptr().cast_mut().cast(),
                0.0,
            );
        } else {
            if let Some(program) = state.program {
                if program >= 0 && program < (*raw).num_programs {
                    command(raw, eff::SET_PROGRAM, program as isize);
                }
            }
            if let Some(set) = (*raw).set_parameter {
                for (i, value) in state
                    .parameters
                    .iter()
                    .take((*raw).num_params.max(0) as usize)
                    .enumerate()
                {
                    if value.is_finite() {
                        set(raw, i as i32, value.clamp(0.0, 1.0));
                    }
                }
            }
        }
    }
    true
}

pub(super) unsafe fn host_data(effect: *mut Effect) -> &'static HostData {
    unsafe { &*((*effect).reserved1 as *const HostData) }
}
