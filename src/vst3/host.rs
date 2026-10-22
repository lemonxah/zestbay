//! VST3 plugin host — instantiation and real-time processing.
//!
//! Threading model: the PipeWire main-loop thread is the VST3 UI/main
//! thread (component setup, IEditController, IPlugView, IRunLoop, state).
//! `process()` runs on the PipeWire data thread and never allocates or locks.
//!
//! Parameters travel through the atomic slots in `port_updates`
//! (normalized values):
//! - host → processor: `process()` diffs slots against an RT-private copy
//!   of the last applied values and sends `inputParameterChanges`;
//! - processor → host: `outputParameterChanges` are written into the slots;
//! - slots → edit controller: `sync_controller()` (main thread) pushes any
//!   slot value the controller doesn't hold yet via `setParamNormalized`;
//! - plugin GUI → processor: `performEdit` writes the slot (see com_host).

use std::ffi::CString;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use vst3::Steinberg::Vst::*;
use vst3::Steinberg::*;

use super::com_host::{
    HostApplication, HostComponentHandler, Vst3SharedParams, new_host_application,
    new_host_component_handler,
};
use crate::clap::threading::{self, RtGate};

// VST3 crate defines these as u32 but the API expects i32 — cast once here.
const K_AUDIO: i32 = vst3::Steinberg::Vst::MediaTypes_::kAudio as i32;
const K_EVENT: i32 = vst3::Steinberg::Vst::MediaTypes_::kEvent as i32;
const K_INPUT: i32 = vst3::Steinberg::Vst::BusDirections_::kInput as i32;
const K_OUTPUT: i32 = vst3::Steinberg::Vst::BusDirections_::kOutput as i32;

/// Largest block `process()` is ever called with (K4).
pub const MAX_BLOCK: usize = 8192;

use crate::plugin::types::*;

// ---------------------------------------------------------------------------
// RT-safe IParameterChanges / IParamValueQueue inline COM objects
//
// These are pre-allocated per-instance and reused every process() call.
// No heap allocation happens during process().
// ---------------------------------------------------------------------------

/// Default number of input parameter changes (tests; instances size the
/// queue list to their parameter count).
#[cfg(test)]
const MAX_PARAM_CHANGES: usize = 128;

/// A single parameter value queue (one param, one point at sample offset 0).
#[repr(C)]
struct InlineParamValueQueue {
    vtbl: *const IParamValueQueueVtbl,
    param_id: ParamID,
    value: ParamValue,
    used: bool,
}

static INLINE_PVQ_VTBL: IParamValueQueueVtbl = IParamValueQueueVtbl {
    base: FUnknownVtbl {
        queryInterface: ipvq_query_interface,
        addRef: ipvq_add_ref,
        release: ipvq_release,
    },
    getParameterId: ipvq_get_parameter_id,
    getPointCount: ipvq_get_point_count,
    getPoint: ipvq_get_point,
    addPoint: ipvq_add_point,
};

unsafe extern "system" fn ipvq_query_interface(
    this: *mut FUnknown,
    iid: *const TUID,
    obj: *mut *mut std::ffi::c_void,
) -> tresult {
    unsafe {
        if iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        let iid_ref = &*iid;
        if *iid_ref == FUnknown_iid || *iid_ref == IParamValueQueue_iid {
            *obj = this as *mut std::ffi::c_void;
            return kResultOk;
        }
        *obj = std::ptr::null_mut();
        kNoInterface
    }
}

unsafe extern "system" fn ipvq_add_ref(_this: *mut FUnknown) -> uint32 {
    // Inline object — never deallocated via refcount.
    1
}

unsafe extern "system" fn ipvq_release(_this: *mut FUnknown) -> uint32 {
    1
}

unsafe extern "system" fn ipvq_get_parameter_id(this: *mut IParamValueQueue) -> ParamID {
    unsafe {
        let q = this as *mut InlineParamValueQueue;
        (*q).param_id
    }
}

unsafe extern "system" fn ipvq_get_point_count(this: *mut IParamValueQueue) -> int32 {
    unsafe {
        let q = this as *mut InlineParamValueQueue;
        if (*q).used { 1 } else { 0 }
    }
}

unsafe extern "system" fn ipvq_get_point(
    this: *mut IParamValueQueue,
    index: int32,
    sample_offset: *mut int32,
    value: *mut ParamValue,
) -> tresult {
    unsafe {
        let q = this as *mut InlineParamValueQueue;
        if index != 0 || !(*q).used {
            return kInvalidArgument;
        }
        if !sample_offset.is_null() {
            *sample_offset = 0;
        }
        if !value.is_null() {
            *value = (*q).value;
        }
        kResultOk
    }
}

unsafe extern "system" fn ipvq_add_point(
    _this: *mut IParamValueQueue,
    _sample_offset: int32,
    _value: ParamValue,
    _index: *mut int32,
) -> tresult {
    // Input queues are read-only for the plugin.
    kResultFalse
}

/// Pre-allocated IParameterChanges for input parameter changes.
#[repr(C)]
struct InlineParameterChanges {
    vtbl: *const IParameterChangesVtbl,
    queues: Vec<InlineParamValueQueue>,
    used_count: i32,
}

static INLINE_PC_VTBL: IParameterChangesVtbl = IParameterChangesVtbl {
    base: FUnknownVtbl {
        queryInterface: ipc_query_interface,
        addRef: ipc_add_ref,
        release: ipc_release,
    },
    getParameterCount: ipc_get_parameter_count,
    getParameterData: ipc_get_parameter_data,
    addParameterData: ipc_add_parameter_data,
};

unsafe extern "system" fn ipc_query_interface(
    this: *mut FUnknown,
    iid: *const TUID,
    obj: *mut *mut std::ffi::c_void,
) -> tresult {
    unsafe {
        if iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        let iid_ref = &*iid;
        if *iid_ref == FUnknown_iid || *iid_ref == IParameterChanges_iid {
            *obj = this as *mut std::ffi::c_void;
            return kResultOk;
        }
        *obj = std::ptr::null_mut();
        kNoInterface
    }
}

unsafe extern "system" fn ipc_add_ref(_this: *mut FUnknown) -> uint32 {
    1
}

unsafe extern "system" fn ipc_release(_this: *mut FUnknown) -> uint32 {
    1
}

unsafe extern "system" fn ipc_get_parameter_count(this: *mut IParameterChanges) -> int32 {
    unsafe {
        let pc = this as *mut InlineParameterChanges;
        (*pc).used_count
    }
}

unsafe extern "system" fn ipc_get_parameter_data(
    this: *mut IParameterChanges,
    index: int32,
) -> *mut IParamValueQueue {
    unsafe {
        let pc = this as *mut InlineParameterChanges;
        if index < 0 || index >= (*pc).used_count {
            return std::ptr::null_mut();
        }
        let queues_ptr = (*pc).queues.as_mut_ptr();
        queues_ptr.add(index as usize) as *mut IParamValueQueue
    }
}

unsafe extern "system" fn ipc_add_parameter_data(
    _this: *mut IParameterChanges,
    _id: *const ParamID,
    _index: *mut int32,
) -> *mut IParamValueQueue {
    // Input changes are read-only for the plugin.
    std::ptr::null_mut()
}

impl InlineParameterChanges {
    #[cfg(test)]
    fn new() -> Self {
        Self::with_capacity(MAX_PARAM_CHANGES)
    }

    fn with_capacity(n: usize) -> Self {
        let mut queues = Vec::with_capacity(n);
        for _ in 0..n {
            queues.push(InlineParamValueQueue {
                vtbl: &INLINE_PVQ_VTBL,
                param_id: 0,
                value: 0.0,
                used: false,
            });
        }
        Self {
            vtbl: &INLINE_PC_VTBL,
            queues,
            used_count: 0,
        }
    }

    /// Reset all queues for a new process() call.
    fn reset(&mut self) {
        for q in self.queues.iter_mut().take(self.used_count as usize) {
            q.used = false;
        }
        self.used_count = 0;
    }

    /// Add a parameter change. Returns false if full.
    fn add_change(&mut self, param_id: ParamID, value: ParamValue) -> bool {
        let idx = self.used_count as usize;
        if idx >= self.queues.len() {
            return false;
        }
        self.queues[idx].param_id = param_id;
        self.queues[idx].value = value;
        self.queues[idx].used = true;
        self.used_count += 1;
        true
    }
}

// ---------------------------------------------------------------------------
// Output parameter changes (plugin writes, host reads after process())
// ---------------------------------------------------------------------------

/// Output queue: keeps only the last point (the host only needs the final
/// value of each block).
#[repr(C)]
struct OutputParamQueue {
    vtbl: *const IParamValueQueueVtbl,
    param_id: ParamID,
    value: ParamValue,
    points: i32,
    /// Position in `OutputParameterChanges::touched`, or -1.
    slot: i32,
}

static OUTPUT_PVQ_VTBL: IParamValueQueueVtbl = IParamValueQueueVtbl {
    base: FUnknownVtbl {
        queryInterface: ipvq_query_interface,
        addRef: ipvq_add_ref,
        release: ipvq_release,
    },
    getParameterId: opvq_get_parameter_id,
    getPointCount: opvq_get_point_count,
    getPoint: opvq_get_point,
    addPoint: opvq_add_point,
};

unsafe extern "system" fn opvq_get_parameter_id(this: *mut IParamValueQueue) -> ParamID {
    unsafe { (*(this as *mut OutputParamQueue)).param_id }
}

unsafe extern "system" fn opvq_get_point_count(this: *mut IParamValueQueue) -> int32 {
    unsafe { (*(this as *mut OutputParamQueue)).points }
}

unsafe extern "system" fn opvq_get_point(
    this: *mut IParamValueQueue,
    index: int32,
    sample_offset: *mut int32,
    value: *mut ParamValue,
) -> tresult {
    unsafe {
        let q = &*(this as *mut OutputParamQueue);
        if index < 0 || index >= q.points {
            return kInvalidArgument;
        }
        if !sample_offset.is_null() {
            *sample_offset = 0;
        }
        if !value.is_null() {
            *value = q.value;
        }
        kResultOk
    }
}

unsafe extern "system" fn opvq_add_point(
    this: *mut IParamValueQueue,
    _sample_offset: int32,
    value: ParamValue,
    index: *mut int32,
) -> tresult {
    unsafe {
        let q = &mut *(this as *mut OutputParamQueue);
        q.value = value;
        if !index.is_null() {
            *index = q.points;
        }
        q.points = q.points.saturating_add(1);
        kResultOk
    }
}

/// Pre-allocated IParameterChanges for output parameter changes.  One queue
/// per known parameter (same index) plus a trailing sink for unknown ids.
#[repr(C)]
struct OutputParameterChanges {
    vtbl: *const IParameterChangesVtbl,
    queues: Vec<OutputParamQueue>,
    /// Indices of queues used in this block (capacity == queues.len()).
    touched: Vec<usize>,
    /// (ParamID, index), sorted by id.
    id_index: Vec<(ParamID, usize)>,
}

static OUTPUT_PC_VTBL: IParameterChangesVtbl = IParameterChangesVtbl {
    base: FUnknownVtbl {
        queryInterface: ipc_query_interface,
        addRef: ipc_add_ref,
        release: ipc_release,
    },
    getParameterCount: opc_get_parameter_count,
    getParameterData: opc_get_parameter_data,
    addParameterData: opc_add_parameter_data,
};

unsafe extern "system" fn opc_get_parameter_count(this: *mut IParameterChanges) -> int32 {
    unsafe { (*(this as *mut OutputParameterChanges)).touched.len() as int32 }
}

unsafe extern "system" fn opc_get_parameter_data(
    this: *mut IParameterChanges,
    index: int32,
) -> *mut IParamValueQueue {
    unsafe {
        let pc = &mut *(this as *mut OutputParameterChanges);
        match usize::try_from(index).ok().and_then(|i| pc.touched.get(i).copied()) {
            Some(q) => &mut pc.queues[q] as *mut OutputParamQueue as *mut IParamValueQueue,
            None => std::ptr::null_mut(),
        }
    }
}

unsafe extern "system" fn opc_add_parameter_data(
    this: *mut IParameterChanges,
    id: *const ParamID,
    index: *mut int32,
) -> *mut IParamValueQueue {
    unsafe {
        if id.is_null() {
            return std::ptr::null_mut();
        }
        let pc = &mut *(this as *mut OutputParameterChanges);
        match pc.queue_for(*id) {
            Some((q, slot)) => {
                if !index.is_null() {
                    *index = slot;
                }
                &mut pc.queues[q] as *mut OutputParamQueue as *mut IParamValueQueue
            }
            None => std::ptr::null_mut(),
        }
    }
}

impl OutputParameterChanges {
    fn new(ids: &[ParamID]) -> Self {
        let mut queues = Vec::with_capacity(ids.len() + 1);
        for &id in ids.iter().chain(std::iter::once(&0)) {
            queues.push(OutputParamQueue {
                vtbl: &OUTPUT_PVQ_VTBL,
                param_id: id,
                value: 0.0,
                points: 0,
                slot: -1,
            });
        }
        let mut id_index: Vec<(ParamID, usize)> =
            ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
        id_index.sort_unstable_by_key(|&(id, _)| id);
        Self {
            vtbl: &OUTPUT_PC_VTBL,
            touched: Vec::with_capacity(queues.len()),
            queues,
            id_index,
        }
    }

    fn sink(&self) -> usize {
        self.queues.len() - 1
    }

    /// Queue index and its position in `touched` for `id` (no allocation).
    fn queue_for(&mut self, id: ParamID) -> Option<(usize, i32)> {
        let q = match self.id_index.binary_search_by_key(&id, |&(pid, _)| pid) {
            Ok(pos) => self.id_index[pos].1,
            Err(_) => self.sink(),
        };
        if q == self.sink() {
            self.queues[q].param_id = id;
        }
        if self.queues[q].slot < 0 {
            if self.touched.len() == self.touched.capacity() {
                return None;
            }
            self.queues[q].slot = self.touched.len() as i32;
            self.touched.push(q);
        }
        Some((q, self.queues[q].slot))
    }

    fn reset(&mut self) {
        for &q in &self.touched {
            self.queues[q].points = 0;
            self.queues[q].slot = -1;
        }
        self.touched.clear();
    }
}

// ---------------------------------------------------------------------------
// RT-safe IEventList inline COM object for MIDI events
// ---------------------------------------------------------------------------

/// Max number of MIDI events per process call.
const MAX_MIDI_EVENTS: usize = 512;

/// Pre-allocated IEventList that holds note on/off and other MIDI events.
#[repr(C)]
struct InlineEventList {
    vtbl: *const IEventListVtbl,
    events: Vec<Event>,
    used_count: i32,
}

static INLINE_EL_VTBL: IEventListVtbl = IEventListVtbl {
    base: FUnknownVtbl {
        queryInterface: iel_query_interface,
        addRef: iel_add_ref,
        release: iel_release,
    },
    getEventCount: iel_get_event_count,
    getEvent: iel_get_event,
    addEvent: iel_add_event,
};

unsafe extern "system" fn iel_query_interface(
    this: *mut FUnknown,
    iid: *const TUID,
    obj: *mut *mut std::ffi::c_void,
) -> tresult {
    unsafe {
        if iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        let iid_ref = &*iid;
        if *iid_ref == FUnknown_iid || *iid_ref == IEventList_iid {
            *obj = this as *mut std::ffi::c_void;
            return kResultOk;
        }
        *obj = std::ptr::null_mut();
        kNoInterface
    }
}

unsafe extern "system" fn iel_add_ref(_this: *mut FUnknown) -> uint32 {
    1
}

unsafe extern "system" fn iel_release(_this: *mut FUnknown) -> uint32 {
    1
}

unsafe extern "system" fn iel_get_event_count(this: *mut IEventList) -> int32 {
    unsafe {
        let el = this as *mut InlineEventList;
        (*el).used_count
    }
}

unsafe extern "system" fn iel_get_event(
    this: *mut IEventList,
    index: int32,
    e: *mut Event,
) -> tresult {
    unsafe {
        let el = this as *mut InlineEventList;
        if index < 0 || index >= (*el).used_count || e.is_null() {
            return kInvalidArgument;
        }
        *e = (&(*el).events)[index as usize];
        kResultOk
    }
}

unsafe extern "system" fn iel_add_event(_this: *mut IEventList, _e: *mut Event) -> tresult {
    // Output events from plugin — ignored (no MIDI output routing yet).
    kResultOk
}

impl InlineEventList {
    fn new() -> Self {
        Self {
            vtbl: &INLINE_EL_VTBL,
            events: vec![unsafe { std::mem::zeroed() }; MAX_MIDI_EVENTS],
            used_count: 0,
        }
    }

    fn reset(&mut self) {
        self.used_count = 0;
    }

    /// Next free event slot, or None if full (never grows).
    fn next_slot(&mut self) -> Option<&mut Event> {
        let idx = self.used_count as usize;
        let evt = self.events.get_mut(idx)?;
        self.used_count += 1;
        Some(evt)
    }

    /// Add a note-on event. Returns false if full.
    fn add_note_on(&mut self, sample_offset: i32, channel: i16, pitch: i16, velocity: f32) -> bool {
        let Some(evt) = self.next_slot() else {
            return false;
        };
        evt.busIndex = 0;
        evt.sampleOffset = sample_offset;
        evt.ppqPosition = 0.0;
        evt.flags = 0;
        evt.r#type = Event_::EventTypes_::kNoteOnEvent as u16;
        evt.__field0.noteOn = NoteOnEvent {
            channel,
            pitch,
            tuning: 0.0,
            velocity,
            length: 0,
            noteId: -1,
        };
        true
    }

    /// Add a note-off event. Returns false if full.
    fn add_note_off(&mut self, sample_offset: i32, channel: i16, pitch: i16, velocity: f32) -> bool {
        let Some(evt) = self.next_slot() else {
            return false;
        };
        evt.busIndex = 0;
        evt.sampleOffset = sample_offset;
        evt.ppqPosition = 0.0;
        evt.flags = 0;
        evt.r#type = Event_::EventTypes_::kNoteOffEvent as u16;
        evt.__field0.noteOff = NoteOffEvent {
            channel,
            pitch,
            velocity,
            noteId: -1,
            tuning: 0.0,
        };
        true
    }

    /// Populate from raw PipeWire MIDI events.
    fn fill_from_raw(&mut self, midi_events: &[crate::midi::processing::RawMidiEvent]) {
        self.reset();
        for evt in midi_events {
            if evt.size < 3 {
                continue;
            }
            let status = evt.data[0];
            let msg_type = status & 0xF0;
            let channel = (status & 0x0F) as i16;
            let pitch = evt.data[1] as i16;
            let velocity_raw = evt.data[2];
            let offset = evt.offset as i32;

            match msg_type {
                0x90 => {
                    if velocity_raw == 0 {
                        // Note-on with velocity 0 = note-off
                        self.add_note_off(offset, channel, pitch, 0.0);
                    } else {
                        self.add_note_on(offset, channel, pitch, velocity_raw as f32 / 127.0);
                    }
                }
                0x80 => {
                    self.add_note_off(offset, channel, pitch, velocity_raw as f32 / 127.0);
                }
                _ => {
                    // Other MIDI messages (CC, etc.) — not handled as VST3 events
                    // CC mapping is handled by the filter's process_midi_buffer
                }
            }
        }
    }
}

static NEXT_INSTANCE_ID: AtomicU64 = AtomicU64::new(2_000_000);

fn next_instance_id() -> PluginInstanceId {
    NEXT_INSTANCE_ID.fetch_add(1, Ordering::Relaxed)
}

/// Keeps the dlopen handle alive. Must NOT be closed while any plugin from
/// this library exists.
struct Vst3Library {
    _handle: *mut std::ffi::c_void,
}

impl Drop for Vst3Library {
    fn drop(&mut self) {
        unsafe {
            // Call ModuleExit if available
            let sym = libc::dlsym(self._handle, c"ModuleExit".as_ptr());
            if !sym.is_null() {
                let module_exit: unsafe extern "system" fn() -> bool = std::mem::transmute(sym);
                module_exit();
            }
            // Intentionally do NOT dlclose — same rationale as CLAP
        }
    }
}

/// Per-parameter info stored on the instance (values live in the slots).
#[derive(Debug, Clone)]
pub struct Vst3Param {
    pub id: u32,
    pub port_index: usize,
    pub name: String,
    /// Default in normalized [0, 1] range.
    pub default: f64,
    /// Is this the bypass parameter?
    pub is_bypass: bool,
    pub is_toggle: bool,
}

/// Preallocated state used by `process()` (or by the main thread while the
/// RT gate is suspended).
struct RtState {
    last_applied: Vec<f64>,
    input_param_changes: InlineParameterChanges,
    output_param_changes: OutputParameterChanges,
    input_event_list: InlineEventList,
    output_event_list: InlineEventList,
    in_ptrs: Vec<*mut f32>,
    out_ptrs: Vec<*mut f32>,
    in_bufs: Vec<AudioBusBuffers>,
    out_bufs: Vec<AudioBusBuffers>,
    silence: Vec<f32>,
    scratch: Vec<f32>,
}

impl RtState {
    fn new(input_buses: &[usize], output_buses: &[usize], ids: &[ParamID], initial: Vec<f64>) -> Self {
        let n_in: usize = input_buses.iter().sum();
        let n_out: usize = output_buses.iter().sum();
        let mut rt = Self {
            last_applied: initial,
            input_param_changes: InlineParameterChanges::with_capacity(ids.len().max(1)),
            output_param_changes: OutputParameterChanges::new(ids),
            input_event_list: InlineEventList::new(),
            output_event_list: InlineEventList::new(),
            in_ptrs: vec![std::ptr::null_mut(); n_in],
            out_ptrs: vec![std::ptr::null_mut(); n_out],
            in_bufs: Vec::with_capacity(input_buses.len()),
            out_bufs: Vec::with_capacity(output_buses.len()),
            silence: vec![0.0; MAX_BLOCK],
            scratch: vec![0.0; MAX_BLOCK],
        };
        // The pointer arrays never reallocate, so the per-bus views into
        // them stay valid for the life of the instance.
        let mut off = 0;
        for &ch in input_buses {
            rt.in_bufs.push(bus_buffers(unsafe { rt.in_ptrs.as_mut_ptr().add(off) }, ch));
            off += ch;
        }
        let mut off = 0;
        for &ch in output_buses {
            rt.out_bufs.push(bus_buffers(unsafe { rt.out_ptrs.as_mut_ptr().add(off) }, ch));
            off += ch;
        }
        rt
    }
}

fn bus_buffers(ptrs: *mut *mut f32, channels: usize) -> AudioBusBuffers {
    let mut buf: AudioBusBuffers = unsafe { std::mem::zeroed() };
    buf.numChannels = channels as i32;
    buf.silenceFlags = 0;
    buf.__field0.channelBuffers32 = ptrs;
    buf
}

/// A running VST3 plugin instance.
pub struct Vst3PluginInstance {
    pub id: PluginInstanceId,
    component: vst3::ComPtr<IComponent>,
    processor: vst3::ComPtr<IAudioProcessor>,
    controller: Option<vst3::ComPtr<IEditController>>,
    /// Component and edit controller are the same object.
    single_component: bool,
    /// (component side, controller side) of a separate controller's
    /// IConnectionPoint link.
    connection: Option<(vst3::ComPtr<IConnectionPoint>, vst3::ComPtr<IConnectionPoint>)>,

    pub plugin_id: String,
    pub display_name: String,

    pub audio_input_channels: usize,
    pub audio_output_channels: usize,

    /// Whether this plugin accepts MIDI input (has event input bus)
    pub has_midi_in: bool,
    /// Whether this plugin produces MIDI output (has event output bus)
    pub has_midi_out: bool,

    input_bus_channels: Vec<usize>,
    output_bus_channels: Vec<usize>,

    pub params: Vec<Vst3Param>,
    #[allow(dead_code)]
    bypass_param_id: Option<u32>,

    pub port_updates: SharedPortUpdates,
    shared: Arc<Vst3SharedParams>,

    bypassed: AtomicBool,
    /// Fixed at instantiation; a graph rate change requires a reload (K3).
    pub sample_rate: f64,
    active: bool,
    processing: bool,
    needs_restart: bool,

    gate: RtGate,
    rt: RtState,

    /// Host application COM object — kept alive for the plugin's lifetime.
    host_app: *mut HostApplication,
    /// Component handler COM object — kept alive for the plugin's lifetime.
    component_handler: *mut HostComponentHandler,

    /// Dropped last: ModuleExit must run after all plugin objects are released.
    _library: Vst3Library,
}

unsafe impl Send for Vst3PluginInstance {}

impl Vst3PluginInstance {
    /// Load a VST3 plugin from a `.vst3` bundle and instantiate it.
    ///
    /// `plugin_id` is the hex-encoded TUID (from the scanner).
    ///
    /// # Safety
    /// Calls into C plugin code via dlopen / COM vtable pointers.  Must be
    /// called on the PipeWire thread (the VST3 main thread).
    pub unsafe fn new(
        bundle_path: &str,
        plugin_id: &str,
        plugin_info: &PluginInfo,
        sample_rate: f64,
    ) -> Option<Self> {
        unsafe {
            threading::set_main_thread();
            let instance_id = next_instance_id();

            // Find the .so inside the bundle
            let so_path = super::scanner::find_bundle_binary(std::path::Path::new(bundle_path))?;
            let so_str = so_path.to_str()?;
            let c_path = CString::new(so_str).ok()?;

            let handle = libc::dlopen(c_path.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL);
            if handle.is_null() {
                log::error!("VST3: dlopen failed for {}", so_str);
                return None;
            }

            let module_entry_sym = libc::dlsym(handle, c"ModuleEntry".as_ptr());
            if !module_entry_sym.is_null() {
                let module_entry: unsafe extern "system" fn(*mut std::ffi::c_void) -> bool =
                    std::mem::transmute(module_entry_sym);
                if !module_entry(handle) {
                    log::error!("VST3: ModuleEntry failed for {}", so_str);
                    libc::dlclose(handle);
                    return None;
                }
            }

            let library = Vst3Library { _handle: handle };

            let get_factory_sym = libc::dlsym(handle, c"GetPluginFactory".as_ptr());
            if get_factory_sym.is_null() {
                log::error!("VST3: no GetPluginFactory in {}", so_str);
                return None;
            }

            let get_factory: unsafe extern "system" fn() -> *mut IPluginFactory =
                std::mem::transmute(get_factory_sym);
            let factory_raw = get_factory();
            if factory_raw.is_null() {
                log::error!("VST3: GetPluginFactory returned null for {}", so_str);
                return None;
            }

            let factory = vst3::ComPtr::<IPluginFactory>::from_raw(factory_raw)?;
            let cid = super::scanner::hex_to_tuid(plugin_id)?;

            let mut obj: *mut std::ffi::c_void = std::ptr::null_mut();
            let result = factory.createInstance(
                cid.as_ptr() as FIDString,
                IComponent_iid.as_ptr() as FIDString,
                &mut obj,
            );
            if result != kResultOk || obj.is_null() {
                log::error!("VST3: createInstance failed for {}", plugin_id);
                return None;
            }

            let component = vst3::ComPtr::<IComponent>::from_raw(obj as *mut IComponent)?;

            // Host application COM object (IHostApplication)
            let host_app = new_host_application(std::ptr::null_mut());

            if component.initialize(host_app as *mut FUnknown) != kResultOk {
                log::error!("VST3: initialize failed for {}", plugin_id);
                drop(component);
                super::com_host::release_host_application(host_app);
                return None;
            }

            let processor = match component.cast::<IAudioProcessor>() {
                Some(p) => p,
                None => {
                    log::error!("VST3: no IAudioProcessor for {}", plugin_id);
                    component.terminate();
                    drop(component);
                    super::com_host::release_host_application(host_app);
                    return None;
                }
            };

            // IEditController: same object (single component) or separate.
            let mut controller_cid: TUID = std::mem::zeroed();
            let has_separate_controller =
                component.getControllerClassId(&mut controller_cid) == kResultOk;

            let mut single_component = false;
            let controller: Option<vst3::ComPtr<IEditController>> =
                if let Some(ec) = component.cast::<IEditController>() {
                    log::info!("VST3: {} — controller is same object as component", plugin_info.name);
                    single_component = true;
                    Some(ec)
                } else if has_separate_controller {
                    let mut ctrl_obj: *mut std::ffi::c_void = std::ptr::null_mut();
                    let r = factory.createInstance(
                        controller_cid.as_ptr() as FIDString,
                        IEditController_iid.as_ptr() as FIDString,
                        &mut ctrl_obj,
                    );
                    if r == kResultOk && !ctrl_obj.is_null() {
                        vst3::ComPtr::<IEditController>::from_raw(ctrl_obj as *mut IEditController)
                            .and_then(|ec| {
                                let init_r = ec.initialize(host_app as *mut FUnknown);
                                log::info!(
                                    "VST3: {} — separate controller initialized (result={})",
                                    plugin_info.name,
                                    init_r,
                                );
                                (init_r == kResultOk).then_some(ec)
                            })
                    } else {
                        None
                    }
                } else {
                    None
                };

            // Connect a separate component ↔ controller via IConnectionPoint
            // (required by e.g. Vital to expose params / create views).
            let mut connection = None;
            if let (Some(ctrl), false) = (&controller, single_component) {
                if let (Some(comp_cp), Some(ctrl_cp)) =
                    (component.cast::<IConnectionPoint>(), ctrl.cast::<IConnectionPoint>())
                {
                    comp_cp.connect(ctrl_cp.as_ptr());
                    ctrl_cp.connect(comp_cp.as_ptr());
                    log::info!(
                        "VST3: {} — connected component ↔ controller via IConnectionPoint",
                        plugin_info.name
                    );
                    connection = Some((comp_cp, ctrl_cp));
                }
            }

            // Pass component state to controller
            if let Some(ctrl) = &controller {
                let comp_stream = super::com_host::new_memory_stream();
                if component.getState(comp_stream as *mut IBStream) == kResultOk
                    && !(*comp_stream).data.is_empty()
                {
                    (*comp_stream).pos = 0;
                    let _ = ctrl.setComponentState(comp_stream as *mut IBStream);
                }
                super::com_host::release_memory_stream(comp_stream);
                log::info!(
                    "VST3: {} — controller has {} params after init",
                    plugin_info.name,
                    ctrl.getParameterCount(),
                );
            }

            // Audio buses (activate all)
            let input_bus_channels = bus_channels(&component, K_INPUT);
            let output_bus_channels = bus_channels(&component, K_OUTPUT);
            for (dir, n) in [(K_INPUT, input_bus_channels.len()), (K_OUTPUT, output_bus_channels.len())] {
                for idx in 0..n as i32 {
                    component.activateBus(K_AUDIO, dir, idx, 1);
                }
            }
            let audio_input_channels: usize = input_bus_channels.iter().sum();
            let audio_output_channels: usize = output_bus_channels.iter().sum();

            // Event (MIDI) buses
            let n_event_in = component.getBusCount(K_EVENT, K_INPUT);
            let n_event_out = component.getBusCount(K_EVENT, K_OUTPUT);
            let has_midi_in = n_event_in > 0;
            let has_midi_out = n_event_out > 0;
            for idx in 0..n_event_in {
                component.activateBus(K_EVENT, K_INPUT, idx, 1);
            }
            for idx in 0..n_event_out {
                component.activateBus(K_EVENT, K_OUTPUT, idx, 1);
            }

            log::info!(
                "VST3: {} — audio {}/{}, midi_in={}, midi_out={}",
                plugin_info.name,
                audio_input_channels,
                audio_output_channels,
                has_midi_in,
                has_midi_out,
            );

            // Parameters
            let mut params = Vec::new();
            let mut initial_values = Vec::new();
            let mut bypass_param_id: Option<u32> = None;

            if let Some(ctrl) = &controller {
                for idx in 0..ctrl.getParameterCount() {
                    let mut pinfo: ParameterInfo = std::mem::zeroed();
                    if ctrl.getParameterInfo(idx, &mut pinfo) != kResultOk {
                        continue;
                    }
                    let is_hidden = pinfo.flags & ParameterInfo_::ParameterFlags_::kIsHidden != 0;
                    let is_readonly =
                        pinfo.flags & ParameterInfo_::ParameterFlags_::kIsReadOnly != 0;
                    let is_bypass_param =
                        pinfo.flags & ParameterInfo_::ParameterFlags_::kIsBypass != 0;
                    if is_bypass_param {
                        bypass_param_id = Some(pinfo.id);
                    }
                    if is_hidden || is_readonly || is_bypass_param {
                        continue;
                    }
                    initial_values.push(ctrl.getParamNormalized(pinfo.id));
                    params.push(Vst3Param {
                        id: pinfo.id,
                        port_index: params.len(),
                        name: read_string128(&pinfo.title),
                        default: pinfo.defaultNormalizedValue,
                        is_bypass: false,
                        is_toggle: pinfo.stepCount == 1,
                    });
                }
            }

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
            let ids: Vec<ParamID> = params.iter().map(|p| p.id).collect();
            let shared = Arc::new(Vst3SharedParams::new(port_updates.clone(), ids.clone()));

            let component_handler = new_host_component_handler(instance_id, shared.clone());
            if let Some(ctrl) = &controller {
                let ch_ptr = component_handler as *mut IComponentHandler;
                let result = ((*(*ctrl.as_ptr()).vtbl).setComponentHandler)(ctrl.as_ptr(), ch_ptr);
                if result != kResultOk {
                    log::warn!("VST3: setComponentHandler returned {} for {}", result, plugin_id);
                }
            }

            let last_applied: Vec<f64> = initial_values.iter().map(|&v| v as f32 as f64).collect();
            let rt = RtState::new(&input_bus_channels, &output_bus_channels, &ids, last_applied);

            // From here on Drop performs the full teardown.
            let mut inst = Self {
                id: instance_id,
                component,
                processor,
                controller,
                single_component,
                connection,
                plugin_id: plugin_id.to_string(),
                display_name: plugin_info.name.clone(),
                audio_input_channels,
                audio_output_channels,
                has_midi_in,
                has_midi_out,
                input_bus_channels,
                output_bus_channels,
                params,
                bypass_param_id,
                port_updates,
                shared,
                bypassed: AtomicBool::new(false),
                sample_rate,
                active: false,
                processing: false,
                needs_restart: false,
                gate: RtGate::new(),
                rt,
                host_app,
                component_handler,
                _library: library,
            };

            if !inst.activate() {
                return None;
            }
            Some(inst)
        }
    }

    /// setupProcessing (K4: max block 8192) + setActive + setProcessing.
    /// Main thread; the audio thread must not be processing.
    unsafe fn activate(&mut self) -> bool {
        unsafe {
            let mut setup = ProcessSetup {
                processMode: ProcessModes_::kRealtime as i32,
                symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
                maxSamplesPerBlock: MAX_BLOCK as i32,
                sampleRate: self.sample_rate,
            };
            if self.processor.setupProcessing(&mut setup) != kResultOk {
                log::error!("VST3: setupProcessing failed for {}", self.plugin_id);
                return false;
            }
            if self.component.setActive(1) != kResultOk {
                log::error!("VST3: setActive failed for {}", self.plugin_id);
                return false;
            }
            self.active = true;
            self.processing = self.processor.setProcessing(1) == kResultOk;
            if !self.processing {
                log::warn!(
                    "VST3: setProcessing returned error for {} (continuing anyway)",
                    self.plugin_id
                );
            }
            true
        }
    }

    /// setProcessing(false) + setActive(false).  Main thread; the audio
    /// thread must not be processing.
    unsafe fn deactivate(&mut self) {
        unsafe {
            if self.processing {
                self.processor.setProcessing(0);
                self.processing = false;
            }
            if self.active {
                self.component.setActive(0);
                self.active = false;
            }
        }
    }

    /// Sample rate the plugin was set up with.
    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    /// Process a block of audio.
    ///
    /// # Safety
    /// Called from the PipeWire RT thread. The `inputs` and `outputs` slices
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
        unsafe { self.process_locked(inputs, outputs, n, midi_events) };
        if !self.active || self.bypassed.load(Ordering::Relaxed) {
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
    ) {
        if !self.active {
            return;
        }
        let Self {
            rt,
            shared,
            processor,
            ..
        } = self;
        let slots = &shared.port_updates.control_inputs;

        // Host-side parameter changes → inputParameterChanges
        rt.input_param_changes.reset();
        for (i, (slot, last)) in slots.iter().zip(rt.last_applied.iter_mut()).enumerate() {
            let v = slot.value.load() as f64;
            if v != *last {
                if !rt.input_param_changes.add_change(shared.ids[i], v) {
                    break;
                }
                *last = v;
            }
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
        for b in rt.in_bufs.iter_mut().chain(rt.out_bufs.iter_mut()) {
            b.silenceFlags = 0;
        }

        rt.input_event_list.fill_from_raw(midi_events);
        rt.output_event_list.reset();
        rt.output_param_changes.reset();

        let mut process_data: ProcessData = unsafe { std::mem::zeroed() };
        process_data.processMode = ProcessModes_::kRealtime as i32;
        process_data.symbolicSampleSize = SymbolicSampleSizes_::kSample32 as i32;
        process_data.numSamples = n as i32;
        process_data.numInputs = rt.in_bufs.len() as i32;
        process_data.numOutputs = rt.out_bufs.len() as i32;
        process_data.inputs = if rt.in_bufs.is_empty() {
            std::ptr::null_mut()
        } else {
            rt.in_bufs.as_mut_ptr()
        };
        process_data.outputs = if rt.out_bufs.is_empty() {
            std::ptr::null_mut()
        } else {
            rt.out_bufs.as_mut_ptr()
        };
        process_data.inputParameterChanges =
            &mut rt.input_param_changes as *mut InlineParameterChanges as *mut IParameterChanges;
        process_data.outputParameterChanges =
            &mut rt.output_param_changes as *mut OutputParameterChanges as *mut IParameterChanges;
        process_data.inputEvents = if rt.input_event_list.used_count > 0 {
            &mut rt.input_event_list as *mut InlineEventList as *mut IEventList
        } else {
            std::ptr::null_mut()
        };
        process_data.outputEvents =
            &mut rt.output_event_list as *mut InlineEventList as *mut IEventList;
        process_data.processContext = std::ptr::null_mut();

        unsafe {
            processor.process(&mut process_data);
        }

        // Plugin-originated changes → slots (synced to the controller by
        // `sync_controller` on the main thread).
        let out = &rt.output_param_changes;
        for &q in &out.touched {
            let queue = &out.queues[q];
            if q >= slots.len() || queue.points == 0 {
                continue;
            }
            let v = queue.value.clamp(0.0, 1.0) as f32;
            slots[q].value.store(v);
            rt.last_applied[q] = v as f64;
        }
    }

    /// Set a parameter (normalized).  Only stores into the atomic slot;
    /// `process()` forwards it to the processor and `sync_controller()` to
    /// the edit controller.
    pub fn set_parameter(&self, port_index: usize, value: f32) {
        if self.params.iter().any(|p| p.port_index == port_index) {
            if let Some(slot) = self
                .port_updates
                .control_inputs
                .iter()
                .find(|s| s.port_index == port_index)
            {
                slot.value.store(value.clamp(0.0, 1.0));
            }
        }
    }

    /// Push slot values the edit controller doesn't hold yet via
    /// `setParamNormalized`.  Main thread only.
    pub fn sync_controller(&self) {
        let Some(ctrl) = &self.controller else {
            return;
        };
        for (i, slot) in self.port_updates.control_inputs.iter().enumerate() {
            let bits = slot.value.load().to_bits();
            let known = &self.shared.controller_bits[i];
            if known.load(Ordering::Relaxed) != bits {
                known.store(bits, Ordering::Relaxed);
                unsafe {
                    ctrl.setParamNormalized(self.shared.ids[i], f32::from_bits(bits) as f64);
                }
            }
        }
    }

    /// Main-thread housekeeping; call on every manager tick.
    ///
    /// Syncs the edit controller and acts on `restartComponent` flags.
    pub fn idle(&mut self) {
        use vst3::Steinberg::Vst::RestartFlags_::*;

        self.sync_controller();
        let flags = self.shared.restart_flags.swap(0, Ordering::AcqRel);
        if flags == 0 {
            return;
        }
        if flags & kParamValuesChanged != 0 {
            self.refresh_param_values();
        }
        if flags & kLatencyChanged != 0 {
            let latency = unsafe { self.processor.getLatencySamples() };
            log::info!("VST3: {} latency changed to {} samples", self.display_name, latency);
        }
        if flags & (kIoChanged | kReloadComponent) != 0 {
            log::info!("VST3: {} requested a component restart (0x{:x})", self.display_name, flags);
            self.needs_restart = true;
        }
    }

    /// Re-read all parameter values from the edit controller into the slots.
    fn refresh_param_values(&self) {
        let Some(ctrl) = &self.controller else {
            return;
        };
        for (i, slot) in self.port_updates.control_inputs.iter().enumerate() {
            let v = unsafe { ctrl.getParamNormalized(self.shared.ids[i]) } as f32;
            self.shared.controller_bits[i].store(v.to_bits(), Ordering::Relaxed);
            slot.value.store(v);
        }
    }

    /// True after the plugin asked for kIoChanged / kReloadComponent.
    pub fn needs_restart(&self) -> bool {
        self.needs_restart
    }

    /// Deactivate and reactivate the plugin in place (main thread).
    ///
    /// Returns false if the plugin could not be brought back with the same
    /// audio bus layout; the caller must then reload the instance (K3
    /// path).  If the audio thread could not be excluded in time nothing is
    /// changed, the request stays pending and true is returned.
    pub fn restart(&mut self) -> bool {
        if !self.gate.suspend(Duration::from_secs(1)) {
            return true;
        }
        self.needs_restart = false;
        let ok = unsafe {
            self.deactivate();
            let same_layout = bus_channels(&self.component, K_INPUT) == self.input_bus_channels
                && bus_channels(&self.component, K_OUTPUT) == self.output_bus_channels;
            self.activate() && same_layout
        };
        self.gate.resume();
        log::info!("VST3: {} restarted (ok={})", self.display_name, ok);
        ok
    }

    /// Return a raw pointer to the IEditController (for GUI access).
    /// Returns null if no controller is available.
    pub fn controller_ptr(&self) -> *mut IEditController {
        match self.controller {
            Some(ref ec) => ec.as_ptr(),
            None => std::ptr::null_mut(),
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
                symbol: format!("param_{}", p.id),
                name: p.name.clone(),
                value: slot.value.load(),
                min: 0.0,
                max: 1.0,
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
            format: PluginFormat::Vst3,
            display_name: self.display_name.clone(),
            pw_node_id,
            parameters: self.get_parameters(),
            active: true,
            bypassed: self.is_bypassed(),
            lv2_state: Vec::new(),
            plugin_state: None,
        }
    }

    /// Get the full plugin state as a byte vector (main thread).
    ///
    /// Format: `[comp_len: u32 LE][IComponent::getState][IEditController::getState]`.
    /// Returns `None` if neither part could be saved.
    pub fn get_state(&self) -> Option<Vec<u8>> {
        unsafe {
            let comp_data = read_state(|s| self.component.getState(s));
            let ctrl_data = match &self.controller {
                Some(ctrl) => read_state(|s| ctrl.getState(s)),
                None => None,
            };
            if comp_data.is_none() && ctrl_data.is_none() {
                return None;
            }
            Some(encode_state_blob(
                comp_data.as_deref().unwrap_or(&[]),
                ctrl_data.as_deref().unwrap_or(&[]),
            ))
        }
    }

    /// Restore the full plugin state from a byte vector (from `get_state`).
    ///
    /// Order follows the VST3 spec: `IComponent::setState`, then
    /// `IEditController::setComponentState` with the same data, then
    /// `IEditController::setState`.  Slots are refreshed from the controller.
    ///
    /// # Safety
    /// Calls into plugin code via COM vtable pointers (main thread).
    pub unsafe fn set_state(&mut self, blob: &[u8]) -> bool {
        unsafe {
            let Some((comp_data, ctrl_data)) = decode_state_blob(blob) else {
                log::warn!("VST3: malformed state blob for {}", self.display_name);
                return false;
            };
            let mut ok = true;

            if !comp_data.is_empty() {
                let result = write_state(comp_data, |s| self.component.setState(s));
                if result != kResultOk {
                    log::warn!("VST3: IComponent::setState failed ({})", result);
                    ok = false;
                }
                if let Some(ctrl) = &self.controller {
                    let _ = write_state(comp_data, |s| ctrl.setComponentState(s));
                }
            }

            if !ctrl_data.is_empty() {
                if let Some(ctrl) = &self.controller {
                    let result = write_state(ctrl_data, |s| ctrl.setState(s));
                    if result != kResultOk {
                        log::warn!("VST3: IEditController::setState failed ({})", result);
                        ok = false;
                    }
                }
            }

            self.refresh_param_values();
            ok
        }
    }
}

/// Run a `getState`-style call against a fresh memory stream.
unsafe fn read_state(f: impl FnOnce(*mut IBStream) -> tresult) -> Option<Vec<u8>> {
    unsafe {
        let stream = super::com_host::new_memory_stream();
        let result = f(stream as *mut IBStream);
        let data = (result == kResultOk).then(|| std::mem::take(&mut (*stream).data));
        super::com_host::release_memory_stream(stream);
        data
    }
}

/// Run a `setState`-style call against a fresh stream holding `data`.
unsafe fn write_state(data: &[u8], f: impl FnOnce(*mut IBStream) -> tresult) -> tresult {
    unsafe {
        let stream = super::com_host::new_memory_stream_from_data(data.to_vec());
        let result = f(stream as *mut IBStream);
        super::com_host::release_memory_stream(stream);
        result
    }
}

fn encode_state_blob(comp: &[u8], ctrl: &[u8]) -> Vec<u8> {
    let mut blob = Vec::with_capacity(4 + comp.len() + ctrl.len());
    blob.extend_from_slice(&(comp.len() as u32).to_le_bytes());
    blob.extend_from_slice(comp);
    blob.extend_from_slice(ctrl);
    blob
}

fn decode_state_blob(blob: &[u8]) -> Option<(&[u8], &[u8])> {
    let len_bytes: [u8; 4] = blob.get(..4)?.try_into().ok()?;
    let comp_len = u32::from_le_bytes(len_bytes) as usize;
    let comp = blob.get(4..4usize.checked_add(comp_len)?)?;
    Some((comp, &blob[4 + comp_len..]))
}

/// Channel count of every audio bus in one direction.
unsafe fn bus_channels(component: &vst3::ComPtr<IComponent>, dir: i32) -> Vec<usize> {
    unsafe {
        let mut out = Vec::new();
        for idx in 0..component.getBusCount(K_AUDIO, dir) {
            let mut info: BusInfo = std::mem::zeroed();
            if component.getBusInfo(K_AUDIO, dir, idx, &mut info) == kResultOk {
                out.push(info.channelCount.max(0) as usize);
            }
        }
        out
    }
}

impl Drop for Vst3PluginInstance {
    fn drop(&mut self) {
        // The filter is destroyed before the instance, so the audio thread
        // can no longer enter process(); the gate just makes sure of it.
        super::ui::force_close_for_controller(self.controller_ptr());
        self.gate.suspend(Duration::from_secs(2));
        unsafe {
            self.deactivate();

            if let Some((comp_cp, ctrl_cp)) = self.connection.take() {
                comp_cp.disconnect(ctrl_cp.as_ptr());
                ctrl_cp.disconnect(comp_cp.as_ptr());
            }

            if let Some(controller) = &self.controller {
                ((*(*controller.as_ptr()).vtbl).setComponentHandler)(
                    controller.as_ptr(),
                    std::ptr::null_mut(),
                );
                // A single-component plugin is terminated once, below.
                if !self.single_component {
                    controller.terminate();
                }
            }
            self.controller = None;

            self.component.terminate();

            super::com_host::release_host_component_handler(self.component_handler);
            super::com_host::release_host_application(self.host_app);
        }
    }
}

/// Read a null-terminated UTF-16 string from a String128 ([u16; 128]).
fn read_string128(buf: &[u16]) -> String {
    let chars: Vec<u16> = buf.iter().take_while(|&&c| c != 0).copied().collect();
    String::from_utf16(&chars).unwrap_or_else(|_| "?".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- InlineParameterChanges ----

    #[test]
    fn param_changes_new_is_empty() {
        let pc = InlineParameterChanges::new();
        assert_eq!(pc.used_count, 0);
        assert_eq!(pc.queues.len(), MAX_PARAM_CHANGES);
    }

    #[test]
    fn param_changes_add_and_count() {
        let mut pc = InlineParameterChanges::new();

        assert!(pc.add_change(100, 0.5));
        assert_eq!(pc.used_count, 1);
        assert_eq!(pc.queues[0].param_id, 100);
        assert!((pc.queues[0].value - 0.5).abs() < 1e-9);
        assert!(pc.queues[0].used);

        assert!(pc.add_change(200, 0.75));
        assert_eq!(pc.used_count, 2);
        assert_eq!(pc.queues[1].param_id, 200);
    }

    #[test]
    fn param_changes_reset_clears() {
        let mut pc = InlineParameterChanges::new();
        pc.add_change(100, 0.5);
        pc.add_change(200, 0.75);
        assert_eq!(pc.used_count, 2);

        pc.reset();
        assert_eq!(pc.used_count, 0);
        assert!(!pc.queues[0].used);
        assert!(!pc.queues[1].used);
    }

    #[test]
    fn param_changes_full_returns_false() {
        let mut pc = InlineParameterChanges::new();
        for i in 0..MAX_PARAM_CHANGES {
            assert!(pc.add_change(i as u32, 0.0));
        }
        assert_eq!(pc.used_count, MAX_PARAM_CHANGES as i32);
        // Should fail at capacity
        assert!(!pc.add_change(999, 0.0));
        assert_eq!(pc.used_count, MAX_PARAM_CHANGES as i32);
    }

    #[test]
    fn param_changes_reset_then_reuse() {
        let mut pc = InlineParameterChanges::new();
        pc.add_change(1, 0.1);
        pc.add_change(2, 0.2);
        pc.reset();

        pc.add_change(3, 0.3);
        assert_eq!(pc.used_count, 1);
        assert_eq!(pc.queues[0].param_id, 3);
        assert!((pc.queues[0].value - 0.3).abs() < 1e-9);
    }

    // ---- InlineEventList ----

    #[test]
    fn event_list_new_is_empty() {
        let el = InlineEventList::new();
        assert_eq!(el.used_count, 0);
    }

    #[test]
    fn event_list_add_note_on() {
        let mut el = InlineEventList::new();
        assert!(el.add_note_on(0, 1, 60, 0.8));
        assert_eq!(el.used_count, 1);

        let evt = &el.events[0];
        assert_eq!(evt.r#type, Event_::EventTypes_::kNoteOnEvent as u16);
        assert_eq!(evt.sampleOffset, 0);
        unsafe {
            assert_eq!(evt.__field0.noteOn.channel, 1);
            assert_eq!(evt.__field0.noteOn.pitch, 60);
            assert!((evt.__field0.noteOn.velocity - 0.8).abs() < 1e-5);
        }
    }

    #[test]
    fn event_list_add_note_off() {
        let mut el = InlineEventList::new();
        assert!(el.add_note_off(128, 0, 72, 0.5));
        assert_eq!(el.used_count, 1);

        let evt = &el.events[0];
        assert_eq!(evt.r#type, Event_::EventTypes_::kNoteOffEvent as u16);
        assert_eq!(evt.sampleOffset, 128);
        unsafe {
            assert_eq!(evt.__field0.noteOff.channel, 0);
            assert_eq!(evt.__field0.noteOff.pitch, 72);
            assert!((evt.__field0.noteOff.velocity - 0.5).abs() < 1e-5);
        }
    }

    #[test]
    fn event_list_reset() {
        let mut el = InlineEventList::new();
        el.add_note_on(0, 0, 60, 0.5);
        el.add_note_off(0, 0, 60, 0.0);
        assert_eq!(el.used_count, 2);

        el.reset();
        assert_eq!(el.used_count, 0);
    }

    #[test]
    fn event_list_capacity_limit() {
        let mut el = InlineEventList::new();
        for i in 0..MAX_MIDI_EVENTS {
            assert!(el.add_note_on(i as i32, 0, 60, 0.5));
        }
        assert_eq!(el.used_count, MAX_MIDI_EVENTS as i32);
        // Should fail at capacity
        assert!(!el.add_note_on(0, 0, 60, 0.5));
    }

    // ---- InlineEventList: fill_from_raw ----

    #[test]
    fn event_list_fill_from_raw_note_on() {
        let mut el = InlineEventList::new();
        let events = [crate::midi::processing::RawMidiEvent {
            offset: 10,
            data: [0x90, 60, 100], // Note on, channel 0, pitch 60, velocity 100
            size: 3,
        }];
        el.fill_from_raw(&events);
        assert_eq!(el.used_count, 1);

        let evt = &el.events[0];
        assert_eq!(evt.r#type, Event_::EventTypes_::kNoteOnEvent as u16);
        assert_eq!(evt.sampleOffset, 10);
        unsafe {
            assert_eq!(evt.__field0.noteOn.channel, 0);
            assert_eq!(evt.__field0.noteOn.pitch, 60);
            assert!((evt.__field0.noteOn.velocity - 100.0 / 127.0).abs() < 1e-5);
        }
    }

    #[test]
    fn event_list_fill_from_raw_note_on_velocity_zero_is_note_off() {
        let mut el = InlineEventList::new();
        let events = [crate::midi::processing::RawMidiEvent {
            offset: 0,
            data: [0x90, 64, 0], // Note on with velocity 0 = note off
            size: 3,
        }];
        el.fill_from_raw(&events);
        assert_eq!(el.used_count, 1);

        let evt = &el.events[0];
        assert_eq!(evt.r#type, Event_::EventTypes_::kNoteOffEvent as u16);
        unsafe {
            assert_eq!(evt.__field0.noteOff.pitch, 64);
            assert!((evt.__field0.noteOff.velocity - 0.0).abs() < 1e-5);
        }
    }

    #[test]
    fn event_list_fill_from_raw_note_off() {
        let mut el = InlineEventList::new();
        let events = [crate::midi::processing::RawMidiEvent {
            offset: 20,
            data: [0x80, 72, 64], // Note off, channel 0, pitch 72, velocity 64
            size: 3,
        }];
        el.fill_from_raw(&events);
        assert_eq!(el.used_count, 1);

        let evt = &el.events[0];
        assert_eq!(evt.r#type, Event_::EventTypes_::kNoteOffEvent as u16);
        unsafe {
            assert_eq!(evt.__field0.noteOff.pitch, 72);
            assert!((evt.__field0.noteOff.velocity - 64.0 / 127.0).abs() < 1e-5);
        }
    }

    #[test]
    fn event_list_fill_from_raw_channel_extraction() {
        let mut el = InlineEventList::new();
        let events = [crate::midi::processing::RawMidiEvent {
            offset: 0,
            data: [0x95, 60, 80], // Note on, channel 5
            size: 3,
        }];
        el.fill_from_raw(&events);
        unsafe {
            assert_eq!(el.events[0].__field0.noteOn.channel, 5);
        }
    }

    #[test]
    fn event_list_fill_from_raw_cc_ignored() {
        let mut el = InlineEventList::new();
        let events = [crate::midi::processing::RawMidiEvent {
            offset: 0,
            data: [0xB0, 1, 64], // CC, channel 0, CC#1, value 64
            size: 3,
        }];
        el.fill_from_raw(&events);
        assert_eq!(el.used_count, 0); // CC messages not converted to VST3 events
    }

    #[test]
    fn event_list_fill_from_raw_short_message_skipped() {
        let mut el = InlineEventList::new();
        let events = [crate::midi::processing::RawMidiEvent {
            offset: 0,
            data: [0x90, 60, 0],
            size: 2, // Too short (needs 3)
        }];
        el.fill_from_raw(&events);
        assert_eq!(el.used_count, 0);
    }

    #[test]
    fn event_list_fill_from_raw_mixed_events() {
        let mut el = InlineEventList::new();
        let events = [
            crate::midi::processing::RawMidiEvent {
                offset: 0,
                data: [0x90, 60, 100], // Note on
                size: 3,
            },
            crate::midi::processing::RawMidiEvent {
                offset: 48,
                data: [0xB0, 1, 64], // CC (ignored)
                size: 3,
            },
            crate::midi::processing::RawMidiEvent {
                offset: 96,
                data: [0x80, 60, 0], // Note off
                size: 3,
            },
        ];
        el.fill_from_raw(&events);
        assert_eq!(el.used_count, 2); // Only note on + note off
    }

    #[test]
    fn event_list_fill_from_raw_resets_first() {
        let mut el = InlineEventList::new();
        // Fill with something first
        el.add_note_on(0, 0, 60, 0.5);
        el.add_note_on(0, 0, 64, 0.5);
        assert_eq!(el.used_count, 2);

        // fill_from_raw should reset before filling
        el.fill_from_raw(&[]);
        assert_eq!(el.used_count, 0);
    }

    // ---- read_string128 ----

    #[test]
    fn read_string128_basic() {
        let mut buf = [0u16; 128];
        for (i, ch) in "Gain".encode_utf16().enumerate() {
            buf[i] = ch;
        }
        assert_eq!(read_string128(&buf), "Gain");
    }

    #[test]
    fn read_string128_empty() {
        let buf = [0u16; 128];
        assert_eq!(read_string128(&buf), "");
    }

    #[test]
    fn read_string128_full_buffer() {
        let mut buf = [0u16; 128];
        let text = "A".repeat(127);
        for (i, ch) in text.encode_utf16().enumerate() {
            buf[i] = ch;
        }
        buf[127] = 0;
        assert_eq!(read_string128(&buf), text);
    }

    #[test]
    fn read_string128_unicode() {
        let mut buf = [0u16; 128];
        for (i, ch) in "Über".encode_utf16().enumerate() {
            buf[i] = ch;
        }
        assert_eq!(read_string128(&buf), "Über");
    }

    // ---- State blob format ----

    #[test]
    fn state_blob_format_roundtrip() {
        // Simulate the format: [comp_len: u32 LE][comp_data][ctrl_data]
        let comp_data = vec![1, 2, 3, 4, 5];
        let ctrl_data = vec![10, 20, 30];

        let comp_len = comp_data.len() as u32;
        let mut blob = Vec::new();
        blob.extend_from_slice(&comp_len.to_le_bytes());
        blob.extend_from_slice(&comp_data);
        blob.extend_from_slice(&ctrl_data);

        // Parse it back (same logic as set_state)
        assert!(blob.len() >= 4);
        let parsed_comp_len = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]) as usize;
        assert_eq!(parsed_comp_len, 5);
        assert!(blob.len() >= 4 + parsed_comp_len);

        let parsed_comp = &blob[4..4 + parsed_comp_len];
        let parsed_ctrl = &blob[4 + parsed_comp_len..];

        assert_eq!(parsed_comp, &comp_data);
        assert_eq!(parsed_ctrl, &ctrl_data);
    }

    #[test]
    fn state_blob_empty_controller() {
        let comp_data = vec![42, 43];
        let ctrl_data: Vec<u8> = vec![];

        let comp_len = comp_data.len() as u32;
        let mut blob = Vec::new();
        blob.extend_from_slice(&comp_len.to_le_bytes());
        blob.extend_from_slice(&comp_data);
        blob.extend_from_slice(&ctrl_data);

        let parsed_comp_len = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]) as usize;
        let parsed_ctrl = &blob[4 + parsed_comp_len..];
        assert!(parsed_ctrl.is_empty());
    }

    #[test]
    fn state_blob_too_short_rejected() {
        // Less than 4 bytes
        let blob = vec![1, 2, 3];
        assert!(blob.len() < 4); // would fail set_state's first check

        // 4 bytes but comp_len says more data than exists
        let mut blob = Vec::new();
        blob.extend_from_slice(&100u32.to_le_bytes()); // says 100 bytes of comp data
        assert!(blob.len() < 4 + 100); // would fail set_state's second check
    }

    #[test]
    fn state_blob_empty_component() {
        let comp_data: Vec<u8> = vec![];
        let ctrl_data = vec![1, 2, 3];

        let comp_len = comp_data.len() as u32;
        let mut blob = Vec::new();
        blob.extend_from_slice(&comp_len.to_le_bytes());
        blob.extend_from_slice(&comp_data);
        blob.extend_from_slice(&ctrl_data);

        let parsed_comp_len = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]) as usize;
        assert_eq!(parsed_comp_len, 0);
        let parsed_comp = &blob[4..4 + parsed_comp_len];
        let parsed_ctrl = &blob[4 + parsed_comp_len..];
        assert!(parsed_comp.is_empty());
        assert_eq!(parsed_ctrl, &ctrl_data);
    }

    #[test]
    fn state_blob_encode_decode() {
        let blob = encode_state_blob(&[1, 2, 3], &[9]);
        let (comp, ctrl) = decode_state_blob(&blob).unwrap();
        assert_eq!(comp, &[1, 2, 3]);
        assert_eq!(ctrl, &[9]);
        assert!(decode_state_blob(&[1, 2]).is_none());
        let mut bad = Vec::new();
        bad.extend_from_slice(&100u32.to_le_bytes());
        bad.push(1);
        assert!(decode_state_blob(&bad).is_none());
        let mut huge = Vec::new();
        huge.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_state_blob(&huge).is_none());
    }

    // ---- OutputParameterChanges ----

    #[test]
    fn output_changes_capture_last_point_per_param() {
        unsafe {
            let mut out = OutputParameterChanges::new(&[30, 10, 20]);
            let pc = &mut out as *mut OutputParameterChanges as *mut IParameterChanges;
            let cap = out.touched.capacity();

            let mut index = -1;
            let q = opc_add_parameter_data(pc, &10, &mut index);
            assert!(!q.is_null());
            assert_eq!(index, 0);
            let mut pidx = -1;
            assert_eq!(opvq_add_point(q, 0, 0.25, &mut pidx), kResultOk);
            assert_eq!(opvq_add_point(q, 16, 0.75, &mut pidx), kResultOk);
            assert_eq!(pidx, 1);

            // Same id returns the same queue and index.
            let mut index2 = -1;
            assert_eq!(opc_add_parameter_data(pc, &10, &mut index2), q);
            assert_eq!(index2, 0);

            // Unknown id goes to the sink, still a valid queue.
            let mut index3 = -1;
            let sink = opc_add_parameter_data(pc, &999, &mut index3);
            assert!(!sink.is_null());
            assert_eq!(opvq_get_parameter_id(sink), 999);

            assert_eq!(opc_get_parameter_count(pc), 2);
            assert_eq!(out.touched[0], 1); // ParamID 10 is parameter index 1
            assert_eq!(out.queues[1].value, 0.75);
            assert_eq!(out.queues[1].points, 2);
            assert_eq!(out.touched.capacity(), cap);

            out.reset();
            assert_eq!(opc_get_parameter_count(pc), 0);
            assert_eq!(out.queues[1].points, 0);
            assert_eq!(out.queues[1].slot, -1);
        }
    }

    #[test]
    fn input_changes_with_capacity() {
        let mut pc = InlineParameterChanges::with_capacity(2);
        assert!(pc.add_change(1, 0.1));
        assert!(pc.add_change(2, 0.2));
        assert!(!pc.add_change(3, 0.3));
    }

    #[test]
    fn rt_state_bus_views_point_into_flat_arrays() {
        let rt = RtState::new(&[2, 2], &[1], &[1, 2], vec![0.0, 0.0]);
        assert_eq!(rt.in_bufs.len(), 2);
        assert_eq!(rt.in_bufs[1].numChannels, 2);
        unsafe {
            assert_eq!(
                rt.in_bufs[1].__field0.channelBuffers32,
                rt.in_ptrs.as_ptr().add(2) as *mut *mut f32
            );
        }
        assert_eq!(rt.output_param_changes.queues.len(), 3);
        assert_eq!(rt.input_event_list.events.len(), MAX_MIDI_EVENTS);
    }
}
