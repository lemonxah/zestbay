//! PipeWire filter node that wraps a VST3 plugin instance.
//!
//! This is structurally identical to `clap::filter::ClapFilterNode`, but uses
//! `Vst3PluginInstance` for the process callback.

use std::cell::RefCell;
use std::ffi::CString;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use pipewire::core::CoreRc;

use super::host::Vst3PluginInstance;
use crate::plugin::cpu_stats::{global_cpu_tracker, PluginTimingSlot};
use crate::plugin::types::PluginInstanceId;

pub struct Vst3FilterNode {
    filter: *mut pipewire::sys::pw_filter,
    _hook: Box<libspa::sys::spa_hook>,
    _events: Box<pipewire::sys::pw_filter_events>,
    _user_data: *mut FilterData,
    _core: CoreRc,
    pub instance_id: PluginInstanceId,
    pub display_name: String,
}

pub struct FilterConfig {
    pub instance_id: PluginInstanceId,
    pub display_name: String,
    pub audio_inputs: usize,
    pub audio_outputs: usize,
}

#[repr(C)]
struct PortData {
    index: u32,
}

struct FilterData {
    instance_ptr: *mut Vst3PluginInstance,
    filter: *mut pipewire::sys::pw_filter,
    instance_id: PluginInstanceId,
    display_name: String,
    event_tx: std::sync::mpsc::Sender<crate::pipewire::PwEvent>,
    node_id_sent: bool,
    shutting_down: AtomicBool,
    input_port_ptrs: Vec<*mut std::ffi::c_void>,
    output_port_ptrs: Vec<*mut std::ffi::c_void>,
    n_audio_inputs: usize,
    n_audio_outputs: usize,
    cpu_slot: Arc<PluginTimingSlot>,
}

unsafe impl Send for FilterData {}

impl Vst3FilterNode {
    pub fn new(
        core: &CoreRc,
        config: FilterConfig,
        plugin_instance: Rc<RefCell<Vst3PluginInstance>>,
        event_tx: std::sync::mpsc::Sender<crate::pipewire::PwEvent>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let c_name = CString::new(config.display_name.as_str())
            .unwrap_or_else(|_| CString::new("VST3 Plugin").unwrap());
        let instance_id_str = config.instance_id.to_string();

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

        let instance_ptr = plugin_instance.as_ptr();
        let cpu_slot =
            global_cpu_tracker().register(config.instance_id, config.display_name.clone());

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
            cpu_slot,
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

        // Add audio input ports
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
            if !port_data.is_null() {
                let pd = port_data as *mut PortData;
                unsafe {
                    (*pd).index = i as u32;
                    (*user_data).input_port_ptrs.push(port_data);
                }
            }
        }

        // Add audio output ports
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
            if !port_data.is_null() {
                let pd = port_data as *mut PortData;
                unsafe {
                    (*pd).index = i as u32;
                    (*user_data).output_port_ptrs.push(port_data);
                }
            }
        }

        let flags = pipewire::sys::pw_filter_flags_PW_FILTER_FLAG_RT_PROCESS;
        let ret =
            unsafe { pipewire::sys::pw_filter_connect(filter, flags, std::ptr::null_mut(), 0) };
        if ret < 0 {
            unsafe {
                pipewire::sys::pw_filter_destroy(filter);
                drop(Box::from_raw(user_data));
            }
            return Err(format!("Failed to connect pw_filter: error {}", ret).into());
        }

        log::info!(
            "VST3 filter node created: {} (instance {}, {} in / {} out)",
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
}

impl Drop for Vst3FilterNode {
    fn drop(&mut self) {
        global_cpu_tracker().unregister(self.instance_id);

        if !self._user_data.is_null() {
            unsafe {
                (*self._user_data)
                    .shutting_down
                    .store(true, Ordering::SeqCst);
            }
        }

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

unsafe extern "C" fn on_state_changed(
    data: *mut std::ffi::c_void,
    _old: pipewire::sys::pw_filter_state,
    state: pipewire::sys::pw_filter_state,
    _error: *const std::os::raw::c_char,
) {
    if state == pipewire::sys::pw_filter_state_PW_FILTER_STATE_PAUSED
        || state == pipewire::sys::pw_filter_state_PW_FILTER_STATE_STREAMING
    {
        let fd = unsafe { &mut *(data as *mut FilterData) };
        if !fd.node_id_sent && !fd.filter.is_null() {
            let node_id = unsafe { pipewire::sys::pw_filter_get_node_id(fd.filter) };
            if node_id != 0 && node_id != u32::MAX {
                log::info!(
                    "VST3 filter node ID resolved: instance {} -> pw_node {}",
                    fd.instance_id,
                    node_id
                );
                let _ = fd.event_tx.send(crate::pipewire::PwEvent::Plugin(
                    crate::pipewire::PluginEvent::PluginAdded {
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

unsafe extern "C" fn on_process(
    data: *mut std::ffi::c_void,
    position: *mut libspa::sys::spa_io_position,
) {
    unsafe {
        let fd = &*(data as *const FilterData);

        if fd.shutting_down.load(Ordering::Acquire) {
            return;
        }

        let (n_samples, rate) = if !position.is_null() {
            (
                (*position).clock.duration as u32,
                (*position).clock.rate.denom as u32,
            )
        } else {
            return;
        };

        if n_samples == 0 || n_samples > 8192 {
            return;
        }

        let inst = &mut *fd.instance_ptr;

        let mut input_bufs: Vec<&[f32]> = Vec::with_capacity(fd.n_audio_inputs);
        for port_ptr in &fd.input_port_ptrs {
            let buf = pipewire::sys::pw_filter_get_dsp_buffer(*port_ptr, n_samples);
            if !buf.is_null() {
                input_bufs.push(std::slice::from_raw_parts(
                    buf as *const f32,
                    n_samples as usize,
                ));
            } else {
                static SILENCE: [f32; 8192] = [0.0; 8192];
                input_bufs.push(&SILENCE[..n_samples as usize]);
            }
        }

        let mut output_bufs: Vec<&mut [f32]> = Vec::with_capacity(fd.n_audio_outputs);
        for port_ptr in &fd.output_port_ptrs {
            let buf = pipewire::sys::pw_filter_get_dsp_buffer(*port_ptr, n_samples);
            if !buf.is_null() {
                output_bufs.push(std::slice::from_raw_parts_mut(
                    buf as *mut f32,
                    n_samples as usize,
                ));
            }
        }

        if output_bufs.len() < fd.n_audio_outputs {
            return;
        }

        let t0 = std::time::Instant::now();
        inst.process(&input_bufs, &mut output_bufs, n_samples as usize);
        let elapsed = t0.elapsed().as_nanos() as u64;
        fd.cpu_slot.record(elapsed, n_samples, rate);
    }
}
