//! CLAP plugin host — instantiation and real-time processing.

use std::ffi::{CStr, CString, c_void};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::plugin::types::*;

static NEXT_INSTANCE_ID: AtomicU64 = AtomicU64::new(1_000_000);

fn next_instance_id() -> PluginInstanceId {
    NEXT_INSTANCE_ID.fetch_add(1, Ordering::Relaxed)
}

/// A loaded CLAP plugin library (keeps the dlopen handle alive).
struct ClapLibrary {
    /// dlopen handle — must NOT be closed while any plugin from this lib exists.
    _handle: *mut c_void,
    /// The entry point (lives inside the .so).
    entry: *const clap_sys::entry::clap_plugin_entry,
}

unsafe impl Send for ClapLibrary {}

impl Drop for ClapLibrary {
    fn drop(&mut self) {
        // Call deinit on the entry
        let entry = unsafe { &*self.entry };
        if let Some(deinit) = entry.deinit {
            unsafe { deinit(); }
        }
        // We intentionally do NOT dlclose.  See scanner.rs comment.
    }
}

/// Holds the live CLAP host struct that the plugin calls back into.
/// Must be pinned and kept alive for the lifetime of the plugin.
#[repr(C)]
pub(crate) struct HostData {
    /// The plugin pointer — set after create_plugin(), used by host callbacks
    /// (timer registration, etc.) to find the correct plugin instance.
    pub plugin: *const clap_sys::plugin::clap_plugin,
}

/// A running CLAP plugin instance.
pub struct ClapPluginInstance {
    pub id: PluginInstanceId,
    plugin: *const clap_sys::plugin::clap_plugin,
    _library: Arc<ClapLibrary>,
    host_box: Box<clap_sys::host::clap_host>,
    _host_data: Box<HostData>,

    pub plugin_id: String,
    pub display_name: String,

    /// Audio port layout (flattened to mono channels, matching the LV2 approach)
    pub audio_input_channels: usize,
    pub audio_output_channels: usize,

    /// CLAP audio port info for process()
    input_port_infos: Vec<ClapAudioPortDesc>,
    output_port_infos: Vec<ClapAudioPortDesc>,

    /// Parameters
    pub params: Vec<ClapParam>,
    params_ext: *const clap_sys::ext::params::clap_plugin_params,

    /// Shared port updates (same pattern as LV2)
    pub port_updates: SharedPortUpdates,

    pub bypassed: bool,
    pub sample_rate: f64,
    activated: bool,
    processing: bool,
}

/// Describes a single CLAP audio port (may have multiple channels).
struct ClapAudioPortDesc {
    channel_count: usize,
}

#[derive(Debug, Clone)]
pub struct ClapParam {
    pub id: u32,
    pub port_index: usize,
    pub name: String,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub default: f64,
}

unsafe impl Send for ClapPluginInstance {}

impl ClapPluginInstance {
    /// Return the raw CLAP plugin pointer (for GUI extension access).
    pub fn plugin_ptr(&self) -> *const clap_sys::plugin::clap_plugin {
        self.plugin
    }

    /// Load a CLAP plugin from a `.clap` file and instantiate it.
    ///
    /// # Safety
    /// Calls into C plugin code via dlopen/function pointers.
    pub unsafe fn new(
        clap_path: &str,
        plugin_id: &str,
        plugin_info: &PluginInfo,
        sample_rate: f64,
    ) -> Option<Self> { unsafe {
        // Register this thread (PW thread) as the CLAP "main thread" so that
        // thread_check.is_main_thread() returns true for param/GUI calls.
        super::ui::set_main_thread_id();

        let instance_id = next_instance_id();
        let c_path = CString::new(clap_path).ok()?;

        // dlopen
        let handle = libc::dlopen(c_path.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL);
        if handle.is_null() {
            log::error!("CLAP: dlopen failed for {}", clap_path);
            return None;
        }

        // Find clap_entry
        let sym_name = c"clap_entry";
        let entry_ptr = libc::dlsym(handle, sym_name.as_ptr());
        if entry_ptr.is_null() {
            log::error!("CLAP: no clap_entry in {}", clap_path);
            libc::dlclose(handle);
            return None;
        }

        let entry = entry_ptr as *const clap_sys::entry::clap_plugin_entry;
        let entry_ref = &*entry;

        // Call init
        if let Some(init_fn) = entry_ref.init {
            if !init_fn(c_path.as_ptr()) {
                log::error!("CLAP: init failed for {}", clap_path);
                libc::dlclose(handle);
                return None;
            }
        }

        let library = Arc::new(ClapLibrary {
            _handle: handle,
            entry,
        });

        // Get factory
        let factory_ptr = entry_ref.get_factory?(
            clap_sys::factory::plugin_factory::CLAP_PLUGIN_FACTORY_ID.as_ptr(),
        );
        if factory_ptr.is_null() {
            log::error!("CLAP: no plugin factory in {}", clap_path);
            return None;
        }
        let factory =
            &*(factory_ptr as *const clap_sys::factory::plugin_factory::clap_plugin_factory);

        // Build host
        let mut host_data = Box::new(HostData {
            plugin: std::ptr::null(),
        });

        let host_name = c"ZestBay";
        let host_vendor = c"ZestBay";
        let host_url = c"https://github.com/lemonxah/zestbay";
        let host_version = c"0.1.0";

        let host_box = Box::new(clap_sys::host::clap_host {
            clap_version: clap_sys::version::clap_version {
                major: 1,
                minor: 2,
                revision: 2,
            },
            host_data: &*host_data as *const HostData as *mut c_void,
            name: host_name.as_ptr(),
            vendor: host_vendor.as_ptr(),
            url: host_url.as_ptr(),
            version: host_version.as_ptr(),
            get_extension: Some(host_get_extension),
            request_restart: Some(host_request_restart),
            request_process: Some(host_request_process),
            request_callback: Some(host_request_callback),
        });

        // Create plugin
        let c_id = CString::new(plugin_id).ok()?;
        let plugin_ptr = factory.create_plugin?(&*factory, &*host_box, c_id.as_ptr());
        if plugin_ptr.is_null() {
            log::error!("CLAP: create_plugin failed for {}", plugin_id);
            return None;
        }

        // Set the plugin pointer in host_data so host callbacks can find it
        host_data.plugin = plugin_ptr;

        let plugin_ref = &*plugin_ptr;

        // Init
        if let Some(init_fn) = plugin_ref.init {
            if !init_fn(plugin_ptr) {
                log::error!("CLAP: plugin init failed for {}", plugin_id);
                if let Some(destroy) = plugin_ref.destroy {
                    destroy(plugin_ptr);
                }
                return None;
            }
        }

        // Query audio ports
        let mut input_port_infos = Vec::new();
        let mut output_port_infos = Vec::new();
        let mut audio_input_channels = 0usize;
        let mut audio_output_channels = 0usize;

        if let Some(get_ext) = plugin_ref.get_extension {
            let ext = get_ext(
                plugin_ptr,
                clap_sys::ext::audio_ports::CLAP_EXT_AUDIO_PORTS.as_ptr(),
            );
            if !ext.is_null() {
                let audio_ports =
                    &*(ext as *const clap_sys::ext::audio_ports::clap_plugin_audio_ports);
                if let Some(count_fn) = audio_ports.count {
                    let in_count = count_fn(plugin_ptr, true);
                    for idx in 0..in_count {
                        let mut info: clap_sys::ext::audio_ports::clap_audio_port_info =
                            std::mem::zeroed();
                        if let Some(get_fn) = audio_ports.get {
                            if get_fn(plugin_ptr, idx, true, &mut info) {
                                let ch = info.channel_count as usize;
                                audio_input_channels += ch;
                                input_port_infos.push(ClapAudioPortDesc {
                                    channel_count: ch,
                                });
                            }
                        }
                    }
                    let out_count = count_fn(plugin_ptr, false);
                    for idx in 0..out_count {
                        let mut info: clap_sys::ext::audio_ports::clap_audio_port_info =
                            std::mem::zeroed();
                        if let Some(get_fn) = audio_ports.get {
                            if get_fn(plugin_ptr, idx, false, &mut info) {
                                let ch = info.channel_count as usize;
                                audio_output_channels += ch;
                                output_port_infos.push(ClapAudioPortDesc {
                                    channel_count: ch,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Query params
        let mut params = Vec::new();
        let mut params_ext: *const clap_sys::ext::params::clap_plugin_params = std::ptr::null();

        if let Some(get_ext) = plugin_ref.get_extension {
            let ext = get_ext(
                plugin_ptr,
                clap_sys::ext::params::CLAP_EXT_PARAMS.as_ptr(),
            );
            if !ext.is_null() {
                params_ext = ext as *const clap_sys::ext::params::clap_plugin_params;
                let pe = &*params_ext;
                if let Some(count_fn) = pe.count {
                    let n = count_fn(plugin_ptr);
                    for idx in 0..n {
                        let mut info: clap_sys::ext::params::clap_param_info = std::mem::zeroed();
                        if let Some(get_info) = pe.get_info {
                            if get_info(plugin_ptr, idx, &mut info) {
                                let is_hidden =
                                    info.flags & clap_sys::ext::params::CLAP_PARAM_IS_HIDDEN != 0;
                                let is_readonly =
                                    info.flags & clap_sys::ext::params::CLAP_PARAM_IS_READONLY != 0;

                                if !is_hidden && !is_readonly {
                                    let name = read_clap_name(&info.name);
                                    let mut value = info.default_value;
                                    if let Some(get_val) = pe.get_value {
                                        get_val(plugin_ptr, info.id, &mut value);
                                    }
                                    params.push(ClapParam {
                                        id: info.id,
                                        port_index: params.len(),
                                        name,
                                        value,
                                        min: info.min_value,
                                        max: info.max_value,
                                        default: info.default_value,
                                    });
                                }
                            }
                        }
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

        // Activate
        let max_frames: u32 = 8192;
        let activated = if let Some(activate) = plugin_ref.activate {
            activate(plugin_ptr, sample_rate, 1, max_frames)
        } else {
            true
        };

        if !activated {
            log::error!("CLAP: activate failed for {}", plugin_id);
            if let Some(destroy) = plugin_ref.destroy {
                destroy(plugin_ptr);
            }
            return None;
        }

        // Start processing
        let processing = if let Some(start) = plugin_ref.start_processing {
            start(plugin_ptr)
        } else {
            true
        };

        let mut inst = Self {
            id: instance_id,
            plugin: plugin_ptr,
            _library: library,
            host_box,
            _host_data: host_data,
            plugin_id: plugin_id.to_string(),
            display_name: plugin_info.name.clone(),
            audio_input_channels,
            audio_output_channels,
            input_port_infos,
            output_port_infos,
            params,
            params_ext,
            port_updates,
            bypassed: false,
            sample_rate,
            activated,
            processing,
        };

        // Fix up the host_data back-pointer now that inst has its final address
        // Note: this is set after construction; must happen before any callback
        // Actually, since we Box the host_data separately, we need to update
        // the instance pointer.  We'll do that when the caller puts this into
        // an Rc<RefCell<>>.
        let _ = &mut inst;

        Some(inst)
    }}

    /// Process a block of audio.
    ///
    /// # Safety
    /// Called from the PipeWire RT thread.  The `inputs` and `outputs` slices
    /// must be valid for `sample_count` frames.
    pub unsafe fn process(
        &mut self,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        sample_count: usize,
    ) { unsafe {
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

        // Read parameter changes from the shared port_updates
        // and build CLAP input events
        let mut param_events: Vec<clap_sys::events::clap_event_param_value> = Vec::new();
        for (i, p) in self.params.iter_mut().enumerate() {
            if let Some(slot) = self.port_updates.control_inputs.get(i) {
                let new_val = slot.value.load() as f64;
                if (new_val - p.value).abs() > 1e-7 {
                    p.value = new_val;
                    param_events.push(clap_sys::events::clap_event_param_value {
                        header: clap_sys::events::clap_event_header {
                            size: std::mem::size_of::<clap_sys::events::clap_event_param_value>()
                                as u32,
                            time: 0,
                            space_id: clap_sys::events::CLAP_CORE_EVENT_SPACE_ID,
                            type_: clap_sys::events::CLAP_EVENT_PARAM_VALUE,
                            flags: 0,
                        },
                        param_id: p.id,
                        cookie: std::ptr::null_mut(),
                        note_id: -1,
                        port_index: -1,
                        channel: -1,
                        key: -1,
                        value: new_val,
                    });
                }
            }
        }

        // Build input events list
        let in_events_data = InputEventsData {
            events: &param_events,
        };
        let in_events = clap_sys::events::clap_input_events {
            ctx: &in_events_data as *const InputEventsData as *mut c_void,
            size: Some(input_events_size),
            get: Some(input_events_get),
        };

        // Build output events list (we'll collect param output changes)
        let out_events = clap_sys::events::clap_output_events {
            ctx: std::ptr::null_mut(),
            try_push: Some(output_events_try_push),
        };

        // Build audio buffers
        // CLAP uses per-port buffers where each port may have multiple channels.
        // We need to map our flattened mono channel arrays to CLAP's port structure.
        let mut in_buf_ptrs: Vec<Vec<*mut f32>> = Vec::new();
        let mut in_audio_bufs: Vec<clap_sys::audio_buffer::clap_audio_buffer> = Vec::new();
        let mut ch_offset = 0;
        for port_desc in &self.input_port_infos {
            let mut channel_ptrs = Vec::new();
            for ch in 0..port_desc.channel_count {
                let idx = ch_offset + ch;
                if idx < inputs.len() {
                    channel_ptrs.push(inputs[idx].as_ptr() as *mut f32);
                } else {
                    // Null — plugin should handle gracefully
                    channel_ptrs.push(std::ptr::null_mut());
                }
            }
            ch_offset += port_desc.channel_count;
            in_buf_ptrs.push(channel_ptrs);
        }
        for buf_ptrs in &mut in_buf_ptrs {
            in_audio_bufs.push(clap_sys::audio_buffer::clap_audio_buffer {
                data32: buf_ptrs.as_mut_ptr(),
                data64: std::ptr::null_mut(),
                channel_count: buf_ptrs.len() as u32,
                latency: 0,
                constant_mask: 0,
            });
        }

        let mut out_buf_ptrs: Vec<Vec<*mut f32>> = Vec::new();
        let mut out_audio_bufs: Vec<clap_sys::audio_buffer::clap_audio_buffer> = Vec::new();
        let mut ch_offset = 0;
        for port_desc in &self.output_port_infos {
            let mut channel_ptrs = Vec::new();
            for ch in 0..port_desc.channel_count {
                let idx = ch_offset + ch;
                if idx < outputs.len() {
                    channel_ptrs.push(outputs[idx].as_mut_ptr());
                } else {
                    channel_ptrs.push(std::ptr::null_mut());
                }
            }
            ch_offset += port_desc.channel_count;
            out_buf_ptrs.push(channel_ptrs);
        }
        for buf_ptrs in &mut out_buf_ptrs {
            out_audio_bufs.push(clap_sys::audio_buffer::clap_audio_buffer {
                data32: buf_ptrs.as_mut_ptr(),
                data64: std::ptr::null_mut(),
                channel_count: buf_ptrs.len() as u32,
                latency: 0,
                constant_mask: 0,
            });
        }

        let process = clap_sys::process::clap_process {
            steady_time: -1,
            frames_count: sample_count as u32,
            transport: std::ptr::null(),
            audio_inputs: if in_audio_bufs.is_empty() {
                std::ptr::null()
            } else {
                in_audio_bufs.as_ptr()
            },
            audio_outputs: if out_audio_bufs.is_empty() {
                std::ptr::null_mut()
            } else {
                out_audio_bufs.as_mut_ptr()
            },
            audio_inputs_count: in_audio_bufs.len() as u32,
            audio_outputs_count: out_audio_bufs.len() as u32,
            in_events: &in_events,
            out_events: &out_events,
        };

        let plugin_ref = &*self.plugin;
        if let Some(process_fn) = plugin_ref.process {
            process_fn(self.plugin, &process);
        }

        // Update port_updates with current param values
        for (i, p) in self.params.iter().enumerate() {
            if let Some(slot) = self.port_updates.control_inputs.get(i) {
                slot.value.store(p.value as f32);
            }
        }
    }}

    pub fn set_parameter(&mut self, port_index: usize, value: f32) {
        if let Some(p) = self.params.iter_mut().find(|p| p.port_index == port_index) {
            let clamped = (value as f64).clamp(p.min, p.max);
            p.value = clamped;

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

    pub fn get_parameters(&self) -> Vec<ParameterValue> {
        self.params
            .iter()
            .map(|p| ParameterValue {
                port_index: p.port_index,
                symbol: format!("param_{}", p.id),
                name: p.name.clone(),
                value: p.value as f32,
                min: p.min as f32,
                max: p.max as f32,
                default: p.default as f32,
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
            bypassed: self.bypassed,
        }
    }
}

impl Drop for ClapPluginInstance {
    fn drop(&mut self) {
        unsafe {
            let plugin_ref = &*self.plugin;
            if self.processing {
                if let Some(stop) = plugin_ref.stop_processing {
                    stop(self.plugin);
                }
            }
            if self.activated {
                if let Some(deactivate) = plugin_ref.deactivate {
                    deactivate(self.plugin);
                }
            }
            if let Some(destroy) = plugin_ref.destroy {
                destroy(self.plugin);
            }
        }
    }
}

// ---- Input events vtable ----

struct InputEventsData<'a> {
    events: &'a [clap_sys::events::clap_event_param_value],
}

unsafe extern "C" fn input_events_size(list: *const clap_sys::events::clap_input_events) -> u32 {
    unsafe {
        let data = &*((*list).ctx as *const InputEventsData);
        data.events.len() as u32
    }
}

unsafe extern "C" fn input_events_get(
    list: *const clap_sys::events::clap_input_events,
    index: u32,
) -> *const clap_sys::events::clap_event_header {
    unsafe {
        let data = &*((*list).ctx as *const InputEventsData);
        if (index as usize) < data.events.len() {
            &data.events[index as usize].header as *const clap_sys::events::clap_event_header
        } else {
            std::ptr::null()
        }
    }
}

// ---- Output events vtable (no-op for now) ----

unsafe extern "C" fn output_events_try_push(
    _list: *const clap_sys::events::clap_output_events,
    _event: *const clap_sys::events::clap_event_header,
) -> bool {
    // TODO: Capture output parameter changes for UI feedback
    true
}

// ---- Host callbacks ----

unsafe extern "C" fn host_get_extension(
    _host: *const clap_sys::host::clap_host,
    extension_id: *const std::ffi::c_char,
) -> *const c_void {
    unsafe {
        if extension_id.is_null() {
            return std::ptr::null();
        }
        let ext_id = CStr::from_ptr(extension_id);
        if ext_id == clap_sys::ext::gui::CLAP_EXT_GUI {
            return &super::ui::CLAP_HOST_GUI as *const clap_sys::ext::gui::clap_host_gui
                as *const c_void;
        }
        if ext_id == clap_sys::ext::timer_support::CLAP_EXT_TIMER_SUPPORT {
            return &super::ui::CLAP_HOST_TIMER_SUPPORT
                as *const clap_sys::ext::timer_support::clap_host_timer_support
                as *const c_void;
        }
        if ext_id == clap_sys::ext::thread_check::CLAP_EXT_THREAD_CHECK {
            return &super::ui::CLAP_HOST_THREAD_CHECK
                as *const clap_sys::ext::thread_check::clap_host_thread_check
                as *const c_void;
        }
        std::ptr::null()
    }
}

unsafe extern "C" fn host_request_restart(_host: *const clap_sys::host::clap_host) {
    log::debug!("CLAP: host_request_restart");
}

unsafe extern "C" fn host_request_process(_host: *const clap_sys::host::clap_host) {
    log::debug!("CLAP: host_request_process");
}

unsafe extern "C" fn host_request_callback(_host: *const clap_sys::host::clap_host) {
    log::debug!("CLAP: host_request_callback");
}

fn read_clap_name(name: &[std::ffi::c_char]) -> String {
    let bytes: Vec<u8> = name.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8(bytes).unwrap_or_else(|_| "?".to_string())
}
