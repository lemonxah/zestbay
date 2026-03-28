use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use pipewire::core::CoreRc;

use super::types::{MappingMode, MidiCcSource, MidiMessageType};
use crate::plugin::types::SharedPortUpdates;

pub struct MidiFilterNode {
    filter: *mut pipewire::sys::pw_filter,
    _hook: Box<libspa::sys::spa_hook>,
    _events: Box<pipewire::sys::pw_filter_events>,
    _user_data: *mut FilterData,
    _core: CoreRc,
}

pub struct ResolvedMappingEntry {
    pub port_updates: SharedPortUpdates,
    pub port_index: usize,
    pub instance_id: u64,
    pub min: f32,
    pub max: f32,
    pub mode: MappingMode,
    pub source: MidiCcSource,
    pub is_logarithmic: bool,
    pub is_toggle: bool,
}

/// Immutable snapshot of all resolved mappings.
/// The manager builds a new `Arc<ResolvedMappings>` and swaps it into the
/// filter's `RwLock`.  The RT callback uses `try_read()` to access it
/// without blocking.
pub struct ResolvedMappings {
    entries: Vec<ResolvedMappingEntry>,
}

impl ResolvedMappings {
    pub fn new(entries: Vec<ResolvedMappingEntry>) -> Self {
        Self { entries }
    }

    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    fn find(
        &self,
        device_name: &str,
        channel: u8,
        cc: u8,
        message_type: MidiMessageType,
    ) -> Option<&ResolvedMappingEntry> {
        let exact = self.entries.iter().find(|e| {
            e.source.device_name == device_name
                && e.source.channel == Some(channel)
                && e.source.cc == cc
                && e.source.message_type == message_type
        });
        if exact.is_some() {
            return exact;
        }
        self.entries.iter().find(|e| {
            e.source.device_name == device_name
                && e.source.channel.is_none()
                && e.source.cc == cc
                && e.source.message_type == message_type
        })
    }

    /// Match by channel + cc only (ignoring device_name).
    /// Used for per-plugin MIDI ports where routing determines the device.
    pub fn find_any_device(
        &self,
        channel: u8,
        cc: u8,
        message_type: MidiMessageType,
    ) -> Option<&ResolvedMappingEntry> {
        let exact = self.entries.iter().find(|e| {
            e.source.channel == Some(channel)
                && e.source.cc == cc
                && e.source.message_type == message_type
        });
        if exact.is_some() {
            return exact;
        }
        self.entries.iter().find(|e| {
            e.source.channel.is_none() && e.source.cc == cc && e.source.message_type == message_type
        })
    }
}

unsafe impl Send for ResolvedMappings {}
unsafe impl Sync for ResolvedMappings {}

struct FilterData {
    filter: *mut pipewire::sys::pw_filter,
    event_tx: std::sync::mpsc::Sender<crate::pipewire::PwEvent>,
    node_id_sent: bool,
    shutting_down: AtomicBool,
    midi_port_ptr: *mut std::ffi::c_void,
    mappings: parking_lot::RwLock<Arc<ResolvedMappings>>,
    device_name: String,
    toggle_prev: [bool; 128],
    learn_mode: AtomicBool,
    learn_captured: AtomicBool,
}

unsafe impl Send for FilterData {}

#[repr(C)]
struct PortData {
    index: u32,
}

impl MidiFilterNode {
    pub fn new(
        core: &CoreRc,
        event_tx: std::sync::mpsc::Sender<crate::pipewire::PwEvent>,
        device_name: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let node_name = format!("ZestBay MIDI In ({})", device_name);
        let c_name = CString::new(node_name.as_str())
            .unwrap_or_else(|_| CString::new("ZestBay MIDI In").unwrap());

        let props = unsafe {
            let p = pipewire::sys::pw_properties_new(
                c_str(b"media.type\0"),
                c_str(b"Midi\0"),
                c_str(b"media.category\0"),
                c_str(b"Filter\0"),
                c_str(b"media.role\0"),
                c_str(b"DSP\0"),
                std::ptr::null::<std::os::raw::c_char>(),
            );
            let key = CString::new("node.name").unwrap();
            let val = CString::new(node_name.as_str()).unwrap();
            pipewire::sys::pw_properties_set(p, key.as_ptr(), val.as_ptr());
            let key = CString::new("node.description").unwrap();
            pipewire::sys::pw_properties_set(p, key.as_ptr(), val.as_ptr());
            let key = CString::new("zestbay.midi.listener").unwrap();
            let val = CString::new("true").unwrap();
            pipewire::sys::pw_properties_set(p, key.as_ptr(), val.as_ptr());
            p
        };

        let core_raw = core.as_raw_ptr();
        let filter = unsafe { pipewire::sys::pw_filter_new(core_raw, c_name.as_ptr(), props) };
        if filter.is_null() {
            return Err("Failed to create MIDI pw_filter".into());
        }

        let user_data = Box::into_raw(Box::new(FilterData {
            filter,
            event_tx,
            node_id_sent: false,
            shutting_down: AtomicBool::new(false),
            midi_port_ptr: std::ptr::null_mut(),
            mappings: parking_lot::RwLock::new(Arc::new(ResolvedMappings::empty())),
            device_name: device_name.to_string(),
            toggle_prev: [false; 128],
            learn_mode: AtomicBool::new(false),
            learn_captured: AtomicBool::new(false),
        }));

        let port_name = CString::new("midi_in").unwrap();
        let port_props = unsafe {
            pipewire::sys::pw_properties_new(
                c_str(b"port.name\0"),
                port_name.as_ptr(),
                c_str(b"format.dsp\0"),
                c_str(b"8 bit raw midi\0"),
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
            log::error!("Failed to add MIDI input port");
        } else {
            unsafe {
                (*user_data).midi_port_ptr = port_data;
            }
        }

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
            return Err(format!("Failed to connect MIDI pw_filter: error {}", ret).into());
        }

        log::info!("MIDI filter node created for device: {}", device_name);

        Ok(Self {
            filter,
            _hook: hook,
            _events: events,
            _user_data: user_data,
            _core: core.clone(),
        })
    }

    pub fn node_id(&self) -> u32 {
        if self.filter.is_null() {
            return 0;
        }
        unsafe { pipewire::sys::pw_filter_get_node_id(self.filter) }
    }

    pub fn update_mappings(&self, mappings: Arc<ResolvedMappings>) {
        if !self._user_data.is_null() {
            unsafe {
                *(*self._user_data).mappings.write() = mappings;
            }
        }
    }

    pub fn set_learn_mode(&self, enabled: bool) {
        if !self._user_data.is_null() {
            unsafe {
                (*self._user_data)
                    .learn_captured
                    .store(false, Ordering::SeqCst);
                (*self._user_data)
                    .learn_mode
                    .store(enabled, Ordering::SeqCst);
            }
        }
    }

    pub fn device_name(&self) -> &str {
        if !self._user_data.is_null() {
            unsafe { &(*self._user_data).device_name }
        } else {
            ""
        }
    }

    pub fn disconnect(&mut self) {
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

impl Drop for MidiFilterNode {
    fn drop(&mut self) {
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
    let state_str = match state {
        pipewire::sys::pw_filter_state_PW_FILTER_STATE_ERROR => "Error",
        pipewire::sys::pw_filter_state_PW_FILTER_STATE_UNCONNECTED => "Unconnected",
        pipewire::sys::pw_filter_state_PW_FILTER_STATE_CONNECTING => "Connecting",
        pipewire::sys::pw_filter_state_PW_FILTER_STATE_PAUSED => "Paused",
        pipewire::sys::pw_filter_state_PW_FILTER_STATE_STREAMING => "Streaming",
        _ => "Unknown",
    };
    log::info!("MIDI filter state: {}", state_str);

    let fd = unsafe { &mut *(data as *mut FilterData) };
    if !fd.node_id_sent && !fd.filter.is_null() {
        let node_id = unsafe { pipewire::sys::pw_filter_get_node_id(fd.filter) };
        if node_id != 0 && node_id != u32::MAX {
            log::info!("MIDI filter node ID resolved: pw_node {}", node_id);
            fd.node_id_sent = true;
        }
    }
}

/// RT callback: parse incoming MIDI CC messages and write to mapped parameter atomics.
unsafe extern "C" fn on_process(
    data: *mut std::ffi::c_void,
    _position: *mut libspa::sys::spa_io_position,
) {
    unsafe {
        let fd = &mut *(data as *mut FilterData);

        if fd.shutting_down.load(Ordering::Acquire) {
            return;
        }

        if fd.midi_port_ptr.is_null() {
            return;
        }

        // pw_filter_get_dsp_buffer on a "8 bit raw midi" port returns
        // a *spa_pod_sequence of SPA_CONTROL_Midi events.
        let buf = pipewire::sys::pw_filter_get_dsp_buffer(fd.midi_port_ptr, 0);
        if buf.is_null() {
            return;
        }

        let seq = buf as *const libspa::sys::spa_pod_sequence;
        let body = &(*seq).body;
        let body_size = (*seq).pod.size as u32;

        let mut ctrl = libspa::sys::spa_pod_control_first(body);
        while libspa::sys::spa_pod_control_is_inside(body, body_size, ctrl) {
            if (*ctrl).type_ == libspa::sys::SPA_CONTROL_Midi {
                let midi_size = (*ctrl).value.size as usize;
                // MIDI data follows immediately after the spa_pod header inside the control.
                let midi_data = (&(*ctrl).value as *const libspa::sys::spa_pod as *const u8)
                    .add(std::mem::size_of::<libspa::sys::spa_pod>());

                // CC messages: 3 bytes: status (0xBn), CC number, value.
                // Note-on: 3 bytes: status (0x9n), note number, velocity.
                // Note-off: 3 bytes: status (0x8n), note number, velocity.
                if midi_size >= 3 {
                    let status = *midi_data;
                    let byte1 = *midi_data.add(1);
                    let byte2 = *midi_data.add(2);
                    let channel = status & 0x0F;
                    let msg_type = status & 0xF0;

                    if msg_type == 0xB0 {
                        if fd.learn_mode.load(Ordering::Acquire) {
                            if !fd.learn_captured.swap(true, Ordering::SeqCst) {
                                let _ = fd.event_tx.send(crate::pipewire::PwEvent::Plugin(
                                    crate::pipewire::PluginEvent::MidiCcReceived {
                                        device_name: fd.device_name.clone(),
                                        channel,
                                        cc: byte1,
                                        message_type: MidiMessageType::Cc,
                                    },
                                ));
                            }
                        } else {
                            handle_cc(fd, channel, byte1, byte2, MidiMessageType::Cc);
                        }
                    } else if msg_type == 0x90 || msg_type == 0x80 {
                        let velocity = if msg_type == 0x80 { 0 } else { byte2 };
                        if fd.learn_mode.load(Ordering::Acquire) {
                            if velocity > 0 && !fd.learn_captured.swap(true, Ordering::SeqCst) {
                                let _ = fd.event_tx.send(crate::pipewire::PwEvent::Plugin(
                                    crate::pipewire::PluginEvent::MidiCcReceived {
                                        device_name: fd.device_name.clone(),
                                        channel,
                                        cc: byte1,
                                        message_type: MidiMessageType::Note,
                                    },
                                ));
                            }
                        } else {
                            handle_cc(fd, channel, byte1, velocity, MidiMessageType::Note);
                        }
                    }
                }
            }
            ctrl = libspa::sys::spa_pod_control_next(ctrl);
        }
    }
}

/// Apply a CC value to the mapped parameter, handling continuous/toggle/momentary modes.
#[inline]
unsafe fn handle_cc(
    fd: &mut FilterData,
    channel: u8,
    cc: u8,
    value: u8,
    message_type: MidiMessageType,
) {
    let mappings_guard = match fd.mappings.try_read() {
        Some(g) => g,
        None => return,
    };
    let mappings = mappings_guard.clone();
    drop(mappings_guard);

    let Some(entry) = mappings.find(&fd.device_name, channel, cc, message_type) else {
        return;
    };

    let new_value = match entry.mode {
        MappingMode::Continuous => {
            let t = value as f32 / 127.0;
            if entry.is_logarithmic {
                entry.min * (entry.max / entry.min).powf(t)
            } else {
                entry.min + t * (entry.max - entry.min)
            }
        }
        MappingMode::Toggle => {
            let pressed = value > 63;
            let prev = fd.toggle_prev[cc as usize];
            fd.toggle_prev[cc as usize] = pressed;

            if pressed && !prev {
                if let Some(slot) = entry
                    .port_updates
                    .control_inputs
                    .iter()
                    .find(|s| s.port_index == entry.port_index)
                {
                    let current = slot.value.load();
                    let mid = (entry.min + entry.max) / 2.0;
                    if current > mid {
                        entry.min
                    } else {
                        entry.max
                    }
                } else {
                    return;
                }
            } else {
                return;
            }
        }
        MappingMode::Momentary => {
            if value > 63 {
                entry.max
            } else {
                entry.min
            }
        }
    };

    if let Some(slot) = entry
        .port_updates
        .control_inputs
        .iter()
        .find(|s| s.port_index == entry.port_index)
    {
        slot.value.store(new_value);
    }

    let _ = fd.event_tx.send(crate::pipewire::PwEvent::Plugin(
        crate::pipewire::PluginEvent::ParameterChanged {
            instance_id: entry.instance_id,
            port_index: entry.port_index,
            value: new_value,
        },
    ));
}
