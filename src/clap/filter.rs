//! PipeWire filter node that wraps a CLAP plugin instance.
//!
//! This is structurally identical to `lv2::filter::Lv2FilterNode`, but uses
//! `ClapPluginInstance` for the process callback.

use std::cell::{RefCell, UnsafeCell};
use std::ffi::CString;
use std::rc::Rc;
use std::sync::Arc;

use pipewire::core::CoreRc;

use super::host::ClapPluginInstance;
use crate::midi::filter::ResolvedMappings;
use crate::midi::rt::{FilterMain, FilterPorts, FilterRt, FilterShared, new_filter_halves};
use crate::plugin::cpu_stats::global_cpu_tracker;
use crate::plugin::types::PluginInstanceId;

pub struct ClapFilterNode {
    filter: *mut pipewire::sys::pw_filter,
    _hook: Box<libspa::sys::spa_hook>,
    _events: Box<pipewire::sys::pw_filter_events>,
    _user_data: *mut FilterData,
    _core: CoreRc,
    main: FilterMain,
    pub instance_id: PluginInstanceId,
    pub display_name: String,
}

pub struct FilterConfig {
    pub instance_id: PluginInstanceId,
    pub display_name: String,
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    pub has_midi_in: bool,
    pub has_midi_out: bool,
}

#[repr(C)]
struct PortData {
    index: u32,
}

/// User data of the pw_filter callbacks.
///
/// `shared` (atomics only) may be read from any thread.  `rt` is touched
/// exclusively by the RT `process` callback once the filter is connected
/// (before that only by `new`), hence the `UnsafeCell`.
struct FilterData {
    shared: Arc<FilterShared>,
    rt: UnsafeCell<FilterRt>,
    /// Plugin instance owned by an `Rc<RefCell<..>>` on the main thread.
    /// See `on_process` for the aliasing rules.
    instance_ptr: *mut ClapPluginInstance,
}

impl ClapFilterNode {
    pub fn new(
        core: &CoreRc,
        config: FilterConfig,
        plugin_instance: Rc<RefCell<ClapPluginInstance>>,
        _event_tx: std::sync::mpsc::Sender<crate::pipewire::PwEvent>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let c_name = CString::new(config.display_name.as_str())
            .unwrap_or_else(|_| CString::new("CLAP Plugin").unwrap());
        let instance_id_str = config.instance_id.to_string();

        // Instrument plugins (audio_inputs == 0) must NOT be passive:
        // their only input is MIDI, and when the MIDI device disconnects
        // PipeWire would stop scheduling a passive node with no connected
        // inputs, killing the audio output chain.
        let is_passive = config.audio_inputs > 0;

        let props = unsafe {
            let p = pipewire::sys::pw_properties_new(
                c_str(b"media.type\0"),
                c_str(b"Audio\0"),
                c_str(b"media.category\0"),
                c_str(b"Filter\0"),
                c_str(b"media.role\0"),
                c_str(b"DSP\0"),
                c_str(b"node.virtual\0"),
                c_str(b"true\0"),
                std::ptr::null::<std::os::raw::c_char>(),
            );
            if is_passive {
                let key = CString::new("node.passive").unwrap();
                let val = CString::new("true").unwrap();
                pipewire::sys::pw_properties_set(p, key.as_ptr(), val.as_ptr());
            }
            let key = CString::new("node.name").unwrap();
            let val = CString::new(config.display_name.as_str()).unwrap();
            pipewire::sys::pw_properties_set(p, key.as_ptr(), val.as_ptr());
            let key = CString::new("node.description").unwrap();
            pipewire::sys::pw_properties_set(p, key.as_ptr(), val.as_ptr());
            let key = CString::new("zestbay.plugin.instance_id").unwrap();
            let val = CString::new(instance_id_str.as_str()).unwrap();
            pipewire::sys::pw_properties_set(p, key.as_ptr(), val.as_ptr());
            p
        };

        let core_raw = core.as_raw_ptr();
        let filter = unsafe { pipewire::sys::pw_filter_new(core_raw, c_name.as_ptr(), props) };
        if filter.is_null() {
            return Err("Failed to create pw_filter".into());
        }

        // Ports are created before the filter is connected, so the RT
        // callback cannot run yet.  A port that fails to be created keeps a
        // null entry and is fed from scratch memory.
        let mut ports = FilterPorts {
            inputs: Vec::with_capacity(config.audio_inputs),
            outputs: Vec::with_capacity(config.audio_outputs),
            midi_in: std::ptr::null_mut(),
            midi_out: std::ptr::null_mut(),
        };

        for i in 0..config.audio_inputs {
            let port_name = CString::new(format!("input_{}", i)).unwrap();
            let port_data = unsafe {
                add_port(
                    filter,
                    libspa::sys::SPA_DIRECTION_INPUT,
                    &port_name,
                    b"32 bit float mono audio\0",
                )
            };
            if port_data.is_null() {
                log::error!("Failed to add input port {}", i);
            } else {
                unsafe { (*(port_data as *mut PortData)).index = i as u32 };
            }
            ports.inputs.push(port_data);
        }

        for i in 0..config.audio_outputs {
            let port_name = CString::new(format!("output_{}", i)).unwrap();
            let port_data = unsafe {
                add_port(
                    filter,
                    libspa::sys::SPA_DIRECTION_OUTPUT,
                    &port_name,
                    b"32 bit float mono audio\0",
                )
            };
            if port_data.is_null() {
                log::error!("Failed to add output port {}", i);
            } else {
                unsafe { (*(port_data as *mut PortData)).index = i as u32 };
            }
            ports.outputs.push(port_data);
        }

        // MIDI input port
        let port_name = CString::new("midi_in").unwrap();
        ports.midi_in = unsafe {
            add_port(
                filter,
                libspa::sys::SPA_DIRECTION_INPUT,
                &port_name,
                b"8 bit raw midi\0",
            )
        };
        if ports.midi_in.is_null() {
            log::error!("Failed to add MIDI input port for {}", config.display_name);
        }

        // MIDI output port
        let port_name = CString::new("midi_out").unwrap();
        ports.midi_out = unsafe {
            add_port(
                filter,
                libspa::sys::SPA_DIRECTION_OUTPUT,
                &port_name,
                b"8 bit raw midi\0",
            )
        };
        if ports.midi_out.is_null() {
            log::error!("Failed to add MIDI output port for {}", config.display_name);
        }

        let cpu_slot =
            global_cpu_tracker().register(config.instance_id, config.display_name.clone());
        let (main, rt) = new_filter_halves(ports, config.has_midi_in, cpu_slot);

        let user_data = Box::into_raw(Box::new(FilterData {
            shared: main.shared.clone(),
            rt: UnsafeCell::new(rt),
            instance_ptr: plugin_instance.as_ptr(),
        }));

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

        let mut hook = Box::new(unsafe { std::mem::zeroed::<libspa::sys::spa_hook>() });
        unsafe {
            pipewire::sys::pw_filter_add_listener(
                filter,
                hook.as_mut() as *mut libspa::sys::spa_hook,
                events.as_ref() as *const pipewire::sys::pw_filter_events,
                user_data as *mut std::ffi::c_void,
            );
        }

        let flags = pipewire::sys::pw_filter_flags_PW_FILTER_FLAG_RT_PROCESS;
        let ret =
            unsafe { pipewire::sys::pw_filter_connect(filter, flags, std::ptr::null_mut(), 0) };
        if ret < 0 {
            unsafe {
                pipewire::sys::pw_filter_destroy(filter);
                drop(Box::from_raw(user_data));
            }
            global_cpu_tracker().unregister(config.instance_id);
            return Err(format!("Failed to connect pw_filter: error {}", ret).into());
        }

        log::info!(
            "CLAP filter node created: {} (instance {}, {} in / {} out)",
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
            main,
            instance_id: config.instance_id,
            display_name: config.display_name,
        })
    }

    pub fn node_id(&self) -> u32 {
        if self.filter.is_null() {
            return 0;
        }
        unsafe { pipewire::sys::pw_filter_get_node_id(self.filter) }
    }

    /// Sample rate the graph is currently driving this filter at
    /// (from the process clock); 0 until the first process cycle.
    pub fn graph_rate(&self) -> u32 {
        self.main.graph_rate()
    }

    /// Forward events queued by the RT callback (parameter changes from MIDI
    /// mappings, MIDI learn captures, PluginAdded once the node id is known)
    /// and release mapping tables the RT callback no longer uses.
    /// Must be called from the PipeWire main loop on every tick.
    pub fn drain_events(&self, event_tx: &std::sync::mpsc::Sender<crate::pipewire::PwEvent>) {
        self.main
            .drain_events(event_tx, self.instance_id, &self.display_name, || {
                self.node_id()
            });
    }

    pub fn update_mappings(&self, mappings: Arc<ResolvedMappings>) {
        self.main.update_mappings(mappings);
    }

    pub fn set_learn_mode(&self, enabled: bool) {
        self.main.set_learn_mode(enabled);
    }

    pub fn disconnect(&mut self) {
        self.main.set_shutting_down();
        if !self.filter.is_null() {
            unsafe {
                pipewire::sys::pw_filter_disconnect(self.filter);
            }
        }
    }
}

impl Drop for ClapFilterNode {
    fn drop(&mut self) {
        global_cpu_tracker().unregister(self.instance_id);

        self.main.set_shutting_down();

        if !self.filter.is_null() {
            unsafe {
                pipewire::sys::pw_filter_destroy(self.filter);
            }
            self.filter = std::ptr::null_mut();
        }

        if !self._user_data.is_null() {
            unsafe {
                drop(Box::from_raw(self._user_data));
            }
            self._user_data = std::ptr::null_mut();
        }
    }
}

#[inline]
fn c_str(bytes: &[u8]) -> *const std::os::raw::c_char {
    bytes.as_ptr() as *const std::os::raw::c_char
}

/// # Safety
/// `filter` must be a valid, not yet connected pw_filter; `format` must be NUL-terminated.
unsafe fn add_port(
    filter: *mut pipewire::sys::pw_filter,
    direction: libspa::sys::spa_direction,
    name: &CString,
    format: &[u8],
) -> *mut std::ffi::c_void {
    unsafe {
        let port_props = pipewire::sys::pw_properties_new(
            c_str(b"port.name\0"),
            name.as_ptr(),
            c_str(b"format.dsp\0"),
            c_str(format),
            std::ptr::null::<std::os::raw::c_char>(),
        );
        pipewire::sys::pw_filter_add_port(
            filter,
            direction,
            pipewire::sys::pw_filter_port_flags_PW_FILTER_PORT_FLAG_MAP_BUFFERS,
            std::mem::size_of::<PortData>(),
            port_props,
            std::ptr::null_mut(),
            0,
        )
    }
}

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
        log::info!("CLAP filter state: {} ({})", state_str, err);
    } else {
        log::info!("CLAP filter state: {}", state_str);
    }

    if state == pipewire::sys::pw_filter_state_PW_FILTER_STATE_PAUSED
        || state == pipewire::sys::pw_filter_state_PW_FILTER_STATE_STREAMING
    {
        // Only the shared atomics are touched here; PluginAdded is emitted
        // by `drain_events` on the next tick.
        let fd = data as *const FilterData;
        let shared = unsafe { &(*fd).shared };
        shared
            .ready
            .store(true, std::sync::atomic::Ordering::Release);
    }
}

unsafe extern "C" fn on_process(
    data: *mut std::ffi::c_void,
    position: *mut libspa::sys::spa_io_position,
) {
    unsafe {
        let fd = &*(data as *const FilterData);
        // SAFETY: only this callback accesses `rt` while the filter is connected.
        let rt = &mut *fd.rt.get();
        let instance_ptr = fd.instance_ptr;

        rt.run_cycle(position, |inputs, outputs, n_samples, midi_events| {
            // Aliasing invariant: the main thread owns the instance through an
            // Rc<RefCell<..>> and may borrow it while we run, so it must only
            // call methods that touch atomics (set_parameter / set_bypassed /
            // get_parameters, contract K1) or anything else only while this
            // filter is disconnected.  The `&mut` here is created per block
            // and never outlives the `process` call.
            let inst = &mut *instance_ptr;
            inst.process(inputs, outputs, n_samples, midi_events);
            0
        });
    }
}
