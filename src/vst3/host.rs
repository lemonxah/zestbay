//! VST3 plugin host — instantiation and real-time processing.

use std::collections::HashMap;
use std::ffi::CString;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use vst3::Steinberg::Vst::*;
use vst3::Steinberg::*;

use super::com_host::{
    HostApplication, HostComponentHandler,
    new_host_application, new_host_component_handler,
};

// VST3 crate defines these as u32 but the API expects i32 — cast once here.
const K_AUDIO: i32 = vst3::Steinberg::Vst::MediaTypes_::kAudio as i32;
const K_INPUT: i32 = vst3::Steinberg::Vst::BusDirections_::kInput as i32;
const K_OUTPUT: i32 = vst3::Steinberg::Vst::BusDirections_::kOutput as i32;

use crate::plugin::types::*;

// ---------------------------------------------------------------------------
// RT-safe IParameterChanges / IParamValueQueue inline COM objects
//
// These are pre-allocated per-instance and reused every process() call.
// No heap allocation happens during process().
// ---------------------------------------------------------------------------

/// Max number of parameter changes per process call.
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
    // Output parameter changes from the plugin — we don't write to this.
    kResultOk
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
    // Used for output parameter changes — we don't support adding from plugin side.
    std::ptr::null_mut()
}

impl InlineParameterChanges {
    fn new() -> Self {
        let mut queues = Vec::with_capacity(MAX_PARAM_CHANGES);
        for _ in 0..MAX_PARAM_CHANGES {
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

/// Empty IParameterChanges for output parameter changes (plugin writes to this).
#[repr(C)]
struct EmptyParameterChanges {
    vtbl: *const IParameterChangesVtbl,
}

static EMPTY_PC_VTBL: IParameterChangesVtbl = IParameterChangesVtbl {
    base: FUnknownVtbl {
        queryInterface: ipc_query_interface,
        addRef: ipc_add_ref,
        release: ipc_release,
    },
    getParameterCount: empty_pc_get_parameter_count,
    getParameterData: empty_pc_get_parameter_data,
    addParameterData: empty_pc_add_parameter_data,
};

unsafe extern "system" fn empty_pc_get_parameter_count(
    _this: *mut IParameterChanges,
) -> int32 {
    0
}

unsafe extern "system" fn empty_pc_get_parameter_data(
    _this: *mut IParameterChanges,
    _index: int32,
) -> *mut IParamValueQueue {
    std::ptr::null_mut()
}

unsafe extern "system" fn empty_pc_add_parameter_data(
    _this: *mut IParameterChanges,
    _id: *const ParamID,
    _index: *mut int32,
) -> *mut IParamValueQueue {
    // We could capture output parameter changes here, but for now discard.
    std::ptr::null_mut()
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

unsafe impl Send for Vst3Library {}

impl Drop for Vst3Library {
    fn drop(&mut self) {
        unsafe {
            // Call ModuleExit if available
            let sym = libc::dlsym(self._handle, c"ModuleExit".as_ptr());
            if !sym.is_null() {
                let module_exit: unsafe extern "system" fn() -> bool =
                    std::mem::transmute(sym);
                module_exit();
            }
            // Intentionally do NOT dlclose — same rationale as CLAP
        }
    }
}

/// Describes one VST3 audio bus (input or output).
struct Vst3AudioBusDesc {
    channel_count: usize,
}

/// Per-parameter info stored on the instance.
#[derive(Debug, Clone)]
pub struct Vst3Param {
    pub id: u32,
    pub port_index: usize,
    pub name: String,
    /// Current value in normalized [0, 1] range.
    pub value: f64,
    pub default: f64,
    /// Is this the bypass parameter?
    pub is_bypass: bool,
}

/// A running VST3 plugin instance.
pub struct Vst3PluginInstance {
    pub id: PluginInstanceId,
    component: vst3::ComPtr<IComponent>,
    processor: vst3::ComPtr<IAudioProcessor>,
    controller: Option<vst3::ComPtr<IEditController>>,
    _library: Arc<Vst3Library>,

    pub plugin_id: String,
    pub display_name: String,

    pub audio_input_channels: usize,
    pub audio_output_channels: usize,

    input_bus_descs: Vec<Vst3AudioBusDesc>,
    output_bus_descs: Vec<Vst3AudioBusDesc>,

    pub params: Vec<Vst3Param>,
    bypass_param_id: Option<u32>,

    pub port_updates: SharedPortUpdates,

    pub bypassed: bool,
    pub sample_rate: f64,
    active: bool,
    processing: bool,

    /// Host application COM object — kept alive for the plugin's lifetime.
    host_app: *mut HostApplication,
    /// Component handler COM object — kept alive for the plugin's lifetime.
    component_handler: *mut HostComponentHandler,

    /// Pre-allocated input parameter changes for process().
    input_param_changes: InlineParameterChanges,
    /// Pre-allocated output parameter changes for process().
    output_param_changes: EmptyParameterChanges,
}

unsafe impl Send for Vst3PluginInstance {}

impl Vst3PluginInstance {
    /// Load a VST3 plugin from a `.vst3` bundle and instantiate it.
    ///
    /// `plugin_id` is the hex-encoded TUID (from the scanner).
    ///
    /// # Safety
    /// Calls into C plugin code via dlopen / COM vtable pointers.
    pub unsafe fn new(
        bundle_path: &str,
        plugin_id: &str,
        plugin_info: &PluginInfo,
        sample_rate: f64,
    ) -> Option<Self> {
        unsafe {
            let instance_id = next_instance_id();

            // Find the .so inside the bundle
            let so_path = super::scanner::find_bundle_binary(std::path::Path::new(bundle_path))?;
            let so_str = so_path.to_str()?;
            let c_path = CString::new(so_str).ok()?;

            // dlopen
            let handle = libc::dlopen(c_path.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL);
            if handle.is_null() {
                log::error!("VST3: dlopen failed for {}", so_str);
                return None;
            }

            // Call ModuleEntry
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

            let library = Arc::new(Vst3Library { _handle: handle });

            // Get factory
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

            // Parse the CID from hex
            let cid = super::scanner::hex_to_tuid(plugin_id)?;

            // Create IComponent
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

            // Create our host application COM object (provides IHostApplication + IRunLoop QI)
            let host_app = new_host_application(std::ptr::null_mut());

            // Initialize component with our host context
            if component.initialize(host_app as *mut FUnknown) != kResultOk {
                log::error!("VST3: initialize failed for {}", plugin_id);
                super::com_host::release_host_application(host_app);
                return None;
            }

            // Get IAudioProcessor
            let processor = match component.cast::<IAudioProcessor>() {
                Some(p) => p,
                None => {
                    log::error!("VST3: no IAudioProcessor for {}", plugin_id);
                    component.terminate();
                    return None;
                }
            };

            // Get IEditController (may be same object or separate)
            let mut controller_cid: TUID = std::mem::zeroed();
            let has_separate_controller =
                component.getControllerClassId(&mut controller_cid) == kResultOk;

            let controller: Option<vst3::ComPtr<IEditController>> =
                if let Some(ec) = component.cast::<IEditController>() {
                    Some(ec)
                } else if has_separate_controller {
                    let mut ctrl_obj: *mut std::ffi::c_void = std::ptr::null_mut();
                    let r = factory.createInstance(
                        controller_cid.as_ptr() as FIDString,
                        IEditController_iid.as_ptr() as FIDString,
                        &mut ctrl_obj,
                    );
                    if r == kResultOk && !ctrl_obj.is_null() {
                        let ec = vst3::ComPtr::<IEditController>::from_raw(
                            ctrl_obj as *mut IEditController,
                        );
                        if let Some(ref ec) = ec {
                            ec.initialize(host_app as *mut FUnknown);
                        }
                        ec
                    } else {
                        None
                    }
                } else {
                    None
                };

            // Query audio bus info
            let mut input_bus_descs = Vec::new();
            let mut output_bus_descs = Vec::new();
            let mut audio_input_channels = 0usize;
            let mut audio_output_channels = 0usize;

            let in_bus_count = component.getBusCount(K_AUDIO, K_INPUT);
            for idx in 0..in_bus_count {
                let mut bus_info: BusInfo = std::mem::zeroed();
                if component.getBusInfo(K_AUDIO, K_INPUT, idx, &mut bus_info) == kResultOk {
                    let ch = bus_info.channelCount as usize;
                    audio_input_channels += ch;
                    // Activate the bus
                    component.activateBus(K_AUDIO, K_INPUT, idx, 1);
                    input_bus_descs.push(Vst3AudioBusDesc {
                        channel_count: ch,
                    });
                }
            }

            let out_bus_count = component.getBusCount(K_AUDIO, K_OUTPUT);
            for idx in 0..out_bus_count {
                let mut bus_info: BusInfo = std::mem::zeroed();
                if component.getBusInfo(K_AUDIO, K_OUTPUT, idx, &mut bus_info) == kResultOk {
                    let ch = bus_info.channelCount as usize;
                    audio_output_channels += ch;
                    component.activateBus(K_AUDIO, K_OUTPUT, idx, 1);
                    output_bus_descs.push(Vst3AudioBusDesc {
                        channel_count: ch,
                    });
                }
            }

            // Query parameters
            let mut params = Vec::new();
            let mut bypass_param_id: Option<u32> = None;
            let mut port_idx = 0usize;

            if let Some(ref ctrl) = controller {
                let param_count = ctrl.getParameterCount();
                for idx in 0..param_count {
                    let mut pinfo: ParameterInfo = std::mem::zeroed();
                    if ctrl.getParameterInfo(idx, &mut pinfo) == kResultOk {
                        let is_hidden =
                            pinfo.flags & ParameterInfo_::ParameterFlags_::kIsHidden != 0;
                        let is_readonly =
                            pinfo.flags & ParameterInfo_::ParameterFlags_::kIsReadOnly != 0;
                        let is_bypass_param =
                            pinfo.flags & ParameterInfo_::ParameterFlags_::kIsBypass != 0;

                        if is_bypass_param {
                            bypass_param_id = Some(pinfo.id);
                        }

                        if !is_hidden && !is_readonly && !is_bypass_param {
                            let name = read_string128(&pinfo.title);
                            let value = ctrl.getParamNormalized(pinfo.id);
                            params.push(Vst3Param {
                                id: pinfo.id,
                                port_index: port_idx,
                                name,
                                value,
                                default: pinfo.defaultNormalizedValue,
                                is_bypass: false,
                            });
                            port_idx += 1;
                        }
                    }
                }
            }

            // Build shared port updates
            let port_updates = Arc::new(PortUpdates {
                control_inputs: params
                    .iter()
                    .map(|p| PortSlot {
                        port_index: p.port_index,
                        value: AtomicF32::new(p.value as f32),
                    })
                    .collect(),
                control_outputs: Vec::new(),
                atom_outputs: Vec::new(),
                atom_inputs: Vec::new(),
            });

            // Build ParamID → port_index mapping for the component handler
            let param_map: HashMap<u32, usize> = params
                .iter()
                .map(|p| (p.id, p.port_index))
                .collect();

            // Create IComponentHandler and set it on the controller
            let component_handler = new_host_component_handler(
                instance_id,
                param_map,
                port_updates.clone(),
            );

            if let Some(ref ctrl) = controller {
                let ch_ptr = component_handler as *mut IComponentHandler;
                let result = ((*(*ctrl.as_ptr()).vtbl).setComponentHandler)(ctrl.as_ptr(), ch_ptr);
                if result != kResultOk {
                    log::warn!(
                        "VST3: setComponentHandler returned {} for {}",
                        result,
                        plugin_id
                    );
                }
            }

            // Setup processing
            let mut setup = ProcessSetup {
                processMode: ProcessModes_::kRealtime as i32,
                symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
                maxSamplesPerBlock: 8192,
                sampleRate: sample_rate,
            };

            if processor.setupProcessing(&mut setup) != kResultOk {
                log::error!("VST3: setupProcessing failed for {}", plugin_id);
                component.terminate();
                return None;
            }

            // Activate
            if component.setActive(1) != kResultOk {
                log::error!("VST3: setActive failed for {}", plugin_id);
                component.terminate();
                return None;
            }

            // Start processing
            let processing = processor.setProcessing(1) == kResultOk;
            if !processing {
                log::warn!("VST3: setProcessing returned error for {} (continuing anyway)", plugin_id);
            }

            Some(Self {
                id: instance_id,
                component,
                processor,
                controller,
                _library: library,
                plugin_id: plugin_id.to_string(),
                display_name: plugin_info.name.clone(),
                audio_input_channels,
                audio_output_channels,
                input_bus_descs,
                output_bus_descs,
                params,
                bypass_param_id,
                port_updates,
                bypassed: false,
                sample_rate,
                host_app,
                component_handler,
                input_param_changes: InlineParameterChanges::new(),
                output_param_changes: EmptyParameterChanges { vtbl: &EMPTY_PC_VTBL },
                active: true,
                processing,
            })
        }
    }

    /// Process a block of audio.
    ///
    /// # Safety
    /// Called from the PipeWire RT thread. The `inputs` and `outputs` slices
    /// must be valid for `sample_count` frames.
    pub unsafe fn process(
        &mut self,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        sample_count: usize,
    ) {
        unsafe {
            if self.bypassed {
                // Pass-through
                for (i, output) in outputs.iter_mut().enumerate() {
                    if i < inputs.len() {
                        let n = output.len().min(inputs[i].len()).min(sample_count);
                        output[..n].copy_from_slice(&inputs[i][..n]);
                    } else {
                        for s in output.iter_mut().take(sample_count) {
                            *s = 0.0;
                        }
                    }
                }
                return;
            }

            // Read parameter changes from shared port_updates and build
            // IParameterChanges for the process call.
            self.input_param_changes.reset();

            if let Some(ref controller) = self.controller {
                for (i, p) in self.params.iter_mut().enumerate() {
                    if let Some(slot) = self.port_updates.control_inputs.get(i) {
                        let new_val = slot.value.load() as f64;
                        if (new_val - p.value).abs() > 1e-7 {
                            p.value = new_val;
                            controller.setParamNormalized(p.id, new_val);
                            self.input_param_changes.add_change(p.id, new_val);
                        }
                    }
                }
            }

            // Build audio buffers
            // VST3 uses per-bus buffers with multiple channels.
            let mut in_channel_ptrs: Vec<Vec<*mut f32>> = Vec::new();
            let mut in_audio_bufs: Vec<AudioBusBuffers> = Vec::new();
            let mut ch_offset = 0usize;

            for bus_desc in &self.input_bus_descs {
                let mut channel_ptrs = Vec::new();
                for ch in 0..bus_desc.channel_count {
                    let idx = ch_offset + ch;
                    if idx < inputs.len() {
                        channel_ptrs.push(inputs[idx].as_ptr() as *mut f32);
                    } else {
                        channel_ptrs.push(std::ptr::null_mut());
                    }
                }
                ch_offset += bus_desc.channel_count;
                in_channel_ptrs.push(channel_ptrs);
            }

            for ptrs in &mut in_channel_ptrs {
                let mut buf: AudioBusBuffers = std::mem::zeroed();
                buf.numChannels = ptrs.len() as i32;
                buf.silenceFlags = 0;
                buf.__field0.channelBuffers32 = ptrs.as_mut_ptr();
                in_audio_bufs.push(buf);
            }

            let mut out_channel_ptrs: Vec<Vec<*mut f32>> = Vec::new();
            let mut out_audio_bufs: Vec<AudioBusBuffers> = Vec::new();
            let mut ch_offset = 0usize;

            for bus_desc in &self.output_bus_descs {
                let mut channel_ptrs = Vec::new();
                for ch in 0..bus_desc.channel_count {
                    let idx = ch_offset + ch;
                    if idx < outputs.len() {
                        channel_ptrs.push(outputs[idx].as_mut_ptr());
                    } else {
                        channel_ptrs.push(std::ptr::null_mut());
                    }
                }
                ch_offset += bus_desc.channel_count;
                out_channel_ptrs.push(channel_ptrs);
            }

            for ptrs in &mut out_channel_ptrs {
                let mut buf: AudioBusBuffers = std::mem::zeroed();
                buf.numChannels = ptrs.len() as i32;
                buf.silenceFlags = 0;
                buf.__field0.channelBuffers32 = ptrs.as_mut_ptr();
                out_audio_bufs.push(buf);
            }

            let mut process_data: ProcessData = std::mem::zeroed();
            process_data.processMode = ProcessModes_::kRealtime as i32;
            process_data.symbolicSampleSize = SymbolicSampleSizes_::kSample32 as i32;
            process_data.numSamples = sample_count as i32;
            process_data.numInputs = in_audio_bufs.len() as i32;
            process_data.numOutputs = out_audio_bufs.len() as i32;
            process_data.inputs = if in_audio_bufs.is_empty() {
                std::ptr::null_mut()
            } else {
                in_audio_bufs.as_mut_ptr()
            };
            process_data.outputs = if out_audio_bufs.is_empty() {
                std::ptr::null_mut()
            } else {
                out_audio_bufs.as_mut_ptr()
            };
            // Wire up parameter changes (input + output)
            process_data.inputParameterChanges =
                &mut self.input_param_changes as *mut InlineParameterChanges
                    as *mut IParameterChanges;
            process_data.outputParameterChanges =
                &mut self.output_param_changes as *mut EmptyParameterChanges
                    as *mut IParameterChanges;
            process_data.inputEvents = std::ptr::null_mut();
            process_data.outputEvents = std::ptr::null_mut();
            process_data.processContext = std::ptr::null_mut();

            self.processor.process(&mut process_data);

            // Sync param values back to port_updates
            for (i, p) in self.params.iter().enumerate() {
                if let Some(slot) = self.port_updates.control_inputs.get(i) {
                    slot.value.store(p.value as f32);
                }
            }
        }
    }

    pub fn set_parameter(&mut self, port_index: usize, value: f32) {
        if let Some(p) = self.params.iter_mut().find(|p| p.port_index == port_index) {
            let clamped = (value as f64).clamp(0.0, 1.0);
            p.value = clamped;

            if let Some(ref controller) = self.controller {
                unsafe {
                    controller.setParamNormalized(p.id, clamped);
                }
            }

            if let Some(slot) = self
                .port_updates
                .control_inputs
                .iter()
                .find(|s| s.port_index == port_index)
            {
                slot.value.store(clamped as f32);
            }
        }
    }

    /// Return a raw pointer to the IEditController (for GUI access).
    /// Returns null if no controller is available.
    pub fn controller_ptr(&self) -> *mut IEditController {
        match self.controller {
            Some(ref ec) => ec.as_ptr(),
            None => std::ptr::null_mut(),
        }
    }

    pub fn get_parameters(&self) -> Vec<ParameterValue> {
        self.params
            .iter()
            .map(|p| ParameterValue {
                port_index: p.port_index,
                symbol: format!("param_{}", p.id),
                name: p.name.clone(),
                value: p.value as f32,
                min: 0.0,
                max: 1.0,
                default: p.default as f32,
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
            bypassed: self.bypassed,
        }
    }

    /// Get the full plugin state as a byte vector.
    ///
    /// This calls `IComponent::getState()` followed by `IEditController::getState()`
    /// and concatenates the results with a length header so both can be restored.
    ///
    /// Returns `None` if getting state fails.
    pub fn get_state(&self) -> Option<Vec<u8>> {
        unsafe {
            // Get component state
            let comp_stream = super::com_host::new_memory_stream();
            let comp_result = self.component.getState(comp_stream as *mut IBStream);
            let comp_data = if comp_result == kResultOk {
                (*comp_stream).data.clone()
            } else {
                Vec::new()
            };
            super::com_host::release_memory_stream(comp_stream);

            // Get controller state (if separate)
            let ctrl_data = if let Some(ref controller) = self.controller {
                let ctrl_stream = super::com_host::new_memory_stream();
                let ctrl_result = controller.getState(ctrl_stream as *mut IBStream);
                let data = if ctrl_result == kResultOk {
                    (*ctrl_stream).data.clone()
                } else {
                    Vec::new()
                };
                super::com_host::release_memory_stream(ctrl_stream);
                data
            } else {
                Vec::new()
            };

            // Format: [comp_len: u32 LE][comp_data][ctrl_data]
            let comp_len = comp_data.len() as u32;
            let mut blob = Vec::with_capacity(4 + comp_data.len() + ctrl_data.len());
            blob.extend_from_slice(&comp_len.to_le_bytes());
            blob.extend_from_slice(&comp_data);
            blob.extend_from_slice(&ctrl_data);

            Some(blob)
        }
    }

    /// Restore the full plugin state from a byte vector (from `get_state`).
    ///
    /// # Safety
    /// Calls into plugin code via COM vtable pointers.
    pub unsafe fn set_state(&mut self, blob: &[u8]) -> bool {
        unsafe {
            if blob.len() < 4 {
                return false;
            }
            let comp_len = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]) as usize;
            if blob.len() < 4 + comp_len {
                return false;
            }
            let comp_data = &blob[4..4 + comp_len];
            let ctrl_data = &blob[4 + comp_len..];

            // Set component state
            if !comp_data.is_empty() {
                let stream = super::com_host::new_memory_stream_from_data(comp_data.to_vec());
                let result = self.component.setState(stream as *mut IBStream);
                if result != kResultOk {
                    log::warn!("VST3: IComponent::setState failed ({})", result);
                }

                // Also pass component state to controller via setComponentState
                if let Some(ref controller) = self.controller {
                    // Reset stream position for the controller to re-read
                    (*stream).pos = 0;
                    let _ = controller.setComponentState(stream as *mut IBStream);
                }

                super::com_host::release_memory_stream(stream);
            }

            // Set controller state
            if !ctrl_data.is_empty() {
                if let Some(ref controller) = self.controller {
                    let stream = super::com_host::new_memory_stream_from_data(ctrl_data.to_vec());
                    let result = controller.setState(stream as *mut IBStream);
                    if result != kResultOk {
                        log::warn!("VST3: IEditController::setState failed ({})", result);
                    }
                    super::com_host::release_memory_stream(stream);
                }
            }

            // Re-read parameter values from the controller after state restore
            if let Some(ref controller) = self.controller {
                for p in self.params.iter_mut() {
                    let val = controller.getParamNormalized(p.id);
                    p.value = val;
                    if let Some(slot) = self.port_updates.control_inputs.iter().find(|s| s.port_index == p.port_index) {
                        slot.value.store(val as f32);
                    }
                }
            }

            true
        }
    }
}

impl Drop for Vst3PluginInstance {
    fn drop(&mut self) {
        unsafe {
            if self.processing {
                self.processor.setProcessing(0);
            }
            if self.active {
                self.component.setActive(0);
            }

            // Clear the component handler on the controller before terminating
            if let Some(ref controller) = self.controller {
                ((*(*controller.as_ptr()).vtbl).setComponentHandler)(
                    controller.as_ptr(),
                    std::ptr::null_mut(),
                );
            }

            // Terminate controller if separate
            if let Some(ref controller) = self.controller {
                if let Some(base) = controller.cast::<IPluginBase>() {
                    base.terminate();
                }
            }

            self.component.terminate();

            // Release our COM objects
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
