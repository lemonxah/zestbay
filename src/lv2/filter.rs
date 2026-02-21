use std::cell::RefCell;
use std::ffi::CString;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use pipewire::core::CoreRc;

use super::host::Lv2PluginInstance;
use super::types::*;

/// Represents a running PipeWire filter wrapping an LV2 plugin
pub struct Lv2FilterNode {
    /// Raw pw_filter pointer (we own this)
    filter: *mut pipewire::sys::pw_filter,
    /// Listener hook (must be kept alive)
    _hook: Box<libspa::sys::spa_hook>,
    /// The events struct (must be kept alive as long as the listener is active)
    _events: Box<pipewire::sys::pw_filter_events>,
    /// Boxed user data kept alive for the process callback
    _user_data: *mut FilterData,
    /// Keep the core alive so it outlives the filter
    _core: CoreRc,
    /// Instance ID
    pub instance_id: PluginInstanceId,
    /// Name for display
    pub display_name: String,
}

/// Configuration for creating a filter node
pub struct FilterConfig {
    pub instance_id: PluginInstanceId,
    pub display_name: String,
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    pub sample_rate: u32,
}

/// Per-port user data returned by pw_filter_add_port.
/// We store only a port index here; the actual audio data
/// is fetched via pw_filter_get_dsp_buffer during process().
#[repr(C)]
struct PortData {
    /// Index into our input_ports or output_ports vec
    index: u32,
}

/// User data passed to the filter callbacks.
///
/// The `instance_ptr` is a raw pointer to the Lv2PluginInstance, accessed
/// from the RT thread in on_process(). The instance itself is kept alive
/// by the PipeWire manager's `lv2_instances` map.
struct FilterData {
    instance_ptr: *mut Lv2PluginInstance,
    filter: *mut pipewire::sys::pw_filter,
    instance_id: PluginInstanceId,
    display_name: String,
    /// Event sender to notify the UI about the PW node ID.
    event_tx: std::sync::mpsc::Sender<crate::pipewire::PwEvent>,
    /// Whether we already sent the node ID
    node_id_sent: bool,
    /// Set to true before disconnect/destroy so the RT process callback
    /// can bail out immediately without touching the plugin instance.
    shutting_down: AtomicBool,
    input_port_ptrs: Vec<*mut std::ffi::c_void>,
    output_port_ptrs: Vec<*mut std::ffi::c_void>,
    n_audio_inputs: usize,
    n_audio_outputs: usize,
}

// FilterData is accessed from the RT thread via raw pointer.
// This is safe because PipeWire guarantees on_process is not called
// concurrently, and we only do atomic-width writes from the main thread.
unsafe impl Send for FilterData {}

impl Lv2FilterNode {
    /// Create a new PipeWire filter node that wraps an LV2 plugin.
    ///
    /// The filter gets explicit input and output audio ports so it can be
    /// wired inline between other nodes in the PipeWire graph.
    pub fn new(
        core: &CoreRc,
        config: FilterConfig,
        plugin_instance: Rc<RefCell<Lv2PluginInstance>>,
        event_tx: std::sync::mpsc::Sender<crate::pipewire::PwEvent>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let c_name = CString::new(config.display_name.as_str())
            .unwrap_or_else(|_| CString::new("LV2 Plugin").unwrap());
        let instance_id_str = config.instance_id.to_string();

        // Build properties
        let props = unsafe {
            let p = pipewire::sys::pw_properties_new(
                c_str(b"media.type\0"),
                c_str(b"Audio\0"),
                c_str(b"media.category\0"),
                c_str(b"Filter\0"),
                c_str(b"media.role\0"),
                c_str(b"DSP\0"),
                c_str(b"media.class\0"),
                c_str(b"Audio/Duplex\0"),
                c_str(b"node.virtual\0"),
                c_str(b"true\0"),
                std::ptr::null::<std::os::raw::c_char>(),
            );
            // Set additional properties with dynamic values
            let key = CString::new("node.name").unwrap();
            let val = CString::new(config.display_name.as_str()).unwrap();
            pipewire::sys::pw_properties_set(p, key.as_ptr(), val.as_ptr());
            let key = CString::new("node.description").unwrap();
            pipewire::sys::pw_properties_set(p, key.as_ptr(), val.as_ptr());
            let key = CString::new("zestbay.lv2.instance_id").unwrap();
            let val = CString::new(instance_id_str.as_str()).unwrap();
            pipewire::sys::pw_properties_set(p, key.as_ptr(), val.as_ptr());
            p
        };

        // Create the filter (takes ownership of props)
        let core_raw = core.as_raw_ptr();
        let filter = unsafe { pipewire::sys::pw_filter_new(core_raw, c_name.as_ptr(), props) };
        if filter.is_null() {
            return Err("Failed to create pw_filter".into());
        }

        // Get a raw pointer to the plugin instance for use in the RT callback.
        // The instance is kept alive by the caller (lv2_instances map in manager.rs).
        let instance_ptr = plugin_instance.as_ptr();

        // Allocate user data with a stable pointer using Box::into_raw
        // so we can safely mutate it during port setup.
        let user_data = Box::into_raw(Box::new(FilterData {
            instance_ptr,
            filter,
            instance_id: config.instance_id,
            display_name: config.display_name.clone(),
            event_tx,
            node_id_sent: false,
            shutting_down: AtomicBool::new(false),
            input_port_ptrs: Vec::with_capacity(config.audio_inputs),
            output_port_ptrs: Vec::with_capacity(config.audio_outputs),
            n_audio_inputs: config.audio_inputs,
            n_audio_outputs: config.audio_outputs,
        }));

        // Set up the events struct
        let events = Box::new(pipewire::sys::pw_filter_events {
            version: pipewire::sys::PW_VERSION_FILTER_EVENTS,
            destroy: None,
            state_changed: Some(on_state_changed),
            io_changed: None,
            param_changed: None,
            add_buffer: None,
            remove_buffer: None,
            process: Some(on_process),
            drained: None,
            command: None,
        });

        // Register the listener
        let mut hook = Box::new(unsafe { std::mem::zeroed::<libspa::sys::spa_hook>() });
        unsafe {
            pipewire::sys::pw_filter_add_listener(
                filter,
                hook.as_mut() as *mut libspa::sys::spa_hook,
                events.as_ref() as *const pipewire::sys::pw_filter_events,
                user_data as *mut std::ffi::c_void,
            );
        }

        // Add input ports
        for i in 0..config.audio_inputs {
            let port_name = CString::new(format!("input_{}", i)).unwrap();
            let port_props = unsafe {
                pipewire::sys::pw_properties_new(
                    c_str(b"port.name\0"),
                    port_name.as_ptr(),
                    c_str(b"format.dsp\0"),
                    c_str(b"32 bit float mono audio\0"),
                    std::ptr::null::<std::os::raw::c_char>(),
                )
            };
            let port_data = unsafe {
                pipewire::sys::pw_filter_add_port(
                    filter,
                    libspa::sys::SPA_DIRECTION_INPUT,
                    pipewire::sys::pw_filter_port_flags_PW_FILTER_PORT_FLAG_MAP_BUFFERS,
                    std::mem::size_of::<PortData>(),
                    port_props,
                    std::ptr::null_mut(),
                    0,
                )
            };
            if port_data.is_null() {
                log::error!("Failed to add input port {}", i);
            } else {
                // Initialize the PortData
                let pd = port_data as *mut PortData;
                unsafe {
                    (*pd).index = i as u32;
                    (*user_data).input_port_ptrs.push(port_data);
                }
            }
        }

        // Add output ports
        for i in 0..config.audio_outputs {
            let port_name = CString::new(format!("output_{}", i)).unwrap();
            let port_props = unsafe {
                pipewire::sys::pw_properties_new(
                    c_str(b"port.name\0"),
                    port_name.as_ptr(),
                    c_str(b"format.dsp\0"),
                    c_str(b"32 bit float mono audio\0"),
                    std::ptr::null::<std::os::raw::c_char>(),
                )
            };
            let port_data = unsafe {
                pipewire::sys::pw_filter_add_port(
                    filter,
                    libspa::sys::SPA_DIRECTION_OUTPUT,
                    pipewire::sys::pw_filter_port_flags_PW_FILTER_PORT_FLAG_MAP_BUFFERS,
                    std::mem::size_of::<PortData>(),
                    port_props,
                    std::ptr::null_mut(),
                    0,
                )
            };
            if port_data.is_null() {
                log::error!("Failed to add output port {}", i);
            } else {
                let pd = port_data as *mut PortData;
                unsafe {
                    (*pd).index = i as u32;
                    (*user_data).output_port_ptrs.push(port_data);
                }
            }
        }

        // Connect the filter with RT_PROCESS so on_process runs directly
        // on the data thread where pw_filter_get_dsp_buffer is valid.
        let flags = pipewire::sys::pw_filter_flags_PW_FILTER_FLAG_RT_PROCESS;
        let ret =
            unsafe { pipewire::sys::pw_filter_connect(filter, flags, std::ptr::null_mut(), 0) };
        if ret < 0 {
            // Clean up on failure
            unsafe {
                pipewire::sys::pw_filter_destroy(filter);
                drop(Box::from_raw(user_data));
            }
            return Err(format!("Failed to connect pw_filter: error {}", ret).into());
        }

        log::info!(
            "LV2 filter node created: {} (instance {}, {} in / {} out)",
            config.display_name,
            config.instance_id,
            config.audio_inputs,
            config.audio_outputs,
        );

        Ok(Self {
            filter,
            _hook: hook,
            _events: events,
            _user_data: user_data,
            _core: core.clone(),
            instance_id: config.instance_id,
            display_name: config.display_name,
        })
    }

    /// Get the PipeWire node ID for this filter.
    /// Returns 0 if the node hasn't been registered yet.
    pub fn node_id(&self) -> u32 {
        if self.filter.is_null() {
            return 0;
        }
        unsafe { pipewire::sys::pw_filter_get_node_id(self.filter) }
    }

    /// Disconnect the filter without destroying it.
    ///
    /// Sets the `shutting_down` flag first so the RT process callback
    /// stops touching the plugin instance before we tear things down.
    /// Normally you should just drop the `Lv2FilterNode` instead of
    /// calling this — Drop handles everything.
    pub fn disconnect(&mut self) {
        // Signal the RT callback to stop processing
        if !self._user_data.is_null() {
            unsafe {
                (*self._user_data)
                    .shutting_down
                    .store(true, Ordering::SeqCst);
            }
        }
        if !self.filter.is_null() {
            unsafe {
                pipewire::sys::pw_filter_disconnect(self.filter);
            }
        }
    }
}

impl Drop for Lv2FilterNode {
    fn drop(&mut self) {
        // 1) Signal shutdown with a full fence so the RT thread sees it
        //    before we touch anything else.
        if !self._user_data.is_null() {
            unsafe {
                (*self._user_data)
                    .shutting_down
                    .store(true, Ordering::SeqCst);
            }
        }
        // 2) Destroy the filter. pw_filter_destroy disconnects AND
        //    guarantees no more callbacks will fire after it returns.
        if !self.filter.is_null() {
            unsafe {
                pipewire::sys::pw_filter_destroy(self.filter);
            }
            self.filter = std::ptr::null_mut();
        }
        // 3) Now it's safe to reclaim the user data — no callbacks can
        //    access it after pw_filter_destroy returned.
        if !self._user_data.is_null() {
            unsafe {
                drop(Box::from_raw(self._user_data));
            }
            self._user_data = std::ptr::null_mut();
        }
    }
}

/// Helper to get a `*const c_char` from a byte literal with null terminator
#[inline]
fn c_str(bytes: &[u8]) -> *const std::os::raw::c_char {
    bytes.as_ptr() as *const std::os::raw::c_char
}

// ─── Filter callbacks ──────────────────────────────────────────────────────────

unsafe extern "C" fn on_state_changed(
    data: *mut std::ffi::c_void,
    _old: pipewire::sys::pw_filter_state,
    state: pipewire::sys::pw_filter_state,
    error: *const std::os::raw::c_char,
) {
    let state_str = match state {
        pipewire::sys::pw_filter_state_PW_FILTER_STATE_ERROR => "Error",
        pipewire::sys::pw_filter_state_PW_FILTER_STATE_UNCONNECTED => "Unconnected",
        pipewire::sys::pw_filter_state_PW_FILTER_STATE_CONNECTING => "Connecting",
        pipewire::sys::pw_filter_state_PW_FILTER_STATE_PAUSED => "Paused",
        pipewire::sys::pw_filter_state_PW_FILTER_STATE_STREAMING => "Streaming",
        _ => "Unknown",
    };
    if !error.is_null() {
        let err = unsafe { std::ffi::CStr::from_ptr(error) }.to_string_lossy();
        log::info!("LV2 filter state: {} ({})", state_str, err);
    } else {
        log::info!("LV2 filter state: {}", state_str);
    }

    // When the filter reaches Paused state, the PW node is fully registered
    // and we can query its node ID reliably.
    if state == pipewire::sys::pw_filter_state_PW_FILTER_STATE_PAUSED
        || state == pipewire::sys::pw_filter_state_PW_FILTER_STATE_STREAMING
    {
        let fd = unsafe { &mut *(data as *mut FilterData) };
        if !fd.node_id_sent && !fd.filter.is_null() {
            let node_id = unsafe { pipewire::sys::pw_filter_get_node_id(fd.filter) };
            if node_id != 0 && node_id != u32::MAX {
                log::info!(
                    "LV2 filter node ID resolved: instance {} -> pw_node {}",
                    fd.instance_id,
                    node_id
                );
                let _ = fd.event_tx.send(crate::pipewire::PwEvent::Lv2(
                    crate::pipewire::Lv2Event::PluginAdded {
                        instance_id: fd.instance_id,
                        pw_node_id: node_id,
                        display_name: fd.display_name.clone(),
                    },
                ));
                fd.node_id_sent = true;
            }
        }
    }
}

/// Process callback — runs on the PipeWire real-time data thread.
/// Reads from input port buffers, runs the LV2 plugin, writes to output port buffers.
///
/// # Safety
/// - Called from the RT data thread (PW_FILTER_FLAG_RT_PROCESS).
/// - `data` is a valid `*mut FilterData` allocated with Box::into_raw.
/// - `instance_ptr` points to a live Lv2PluginInstance (kept alive by manager).
/// - PipeWire guarantees this is not called concurrently with itself.
unsafe extern "C" fn on_process(
    data: *mut std::ffi::c_void,
    position: *mut libspa::sys::spa_io_position,
) {
    unsafe {
        let fd = &*(data as *const FilterData);

        // Bail out immediately if we're shutting down — the instance may
        // already be freed or about to be freed on the main thread.
        if fd.shutting_down.load(Ordering::Acquire) {
            return;
        }

        // Get sample count from the position clock
        let n_samples = if !position.is_null() {
            (*position).clock.duration as u32
        } else {
            return; // No position info, can't process
        };

        if n_samples == 0 || n_samples > 8192 {
            return; // Sanity check
        }

        let inst = &mut *fd.instance_ptr;

        let n_in = fd.n_audio_inputs;
        let n_out = fd.n_audio_outputs;

        // Get DSP buffers for input ports
        let mut input_bufs: Vec<&[f32]> = Vec::with_capacity(n_in);
        for port_ptr in &fd.input_port_ptrs {
            let buf = pipewire::sys::pw_filter_get_dsp_buffer(*port_ptr, n_samples);
            if !buf.is_null() {
                input_bufs.push(std::slice::from_raw_parts(
                    buf as *const f32,
                    n_samples as usize,
                ));
            } else {
                // No buffer available — use silence (static to avoid allocation)
                static SILENCE: [f32; 8192] = [0.0; 8192];
                input_bufs.push(&SILENCE[..n_samples as usize]);
            }
        }

        // Get DSP buffers for output ports
        let mut output_bufs: Vec<&mut [f32]> = Vec::with_capacity(n_out);
        for port_ptr in &fd.output_port_ptrs {
            let buf = pipewire::sys::pw_filter_get_dsp_buffer(*port_ptr, n_samples);
            if !buf.is_null() {
                output_bufs.push(std::slice::from_raw_parts_mut(
                    buf as *mut f32,
                    n_samples as usize,
                ));
            }
        }

        // If we didn't get all output buffers, skip processing
        if output_bufs.len() < n_out {
            return;
        }

        // Run the LV2 plugin
        inst.process(&input_bufs, &mut output_bufs, n_samples as usize);
    }
}
