//! Shared RT-safe MIDI processing functions.
//!
//! These functions are called from each plugin filter's `on_process` callback
//! to parse incoming MIDI messages from PipeWire DSP buffers and apply
//! CC/Note mappings to plugin parameters via `SharedPortUpdates`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::filter::{ResolvedMappingEntry, ResolvedMappings};
use super::types::{MappingMode, MidiMessageType};

/// State required for MIDI processing within a plugin filter's RT callback.
/// Each plugin filter embeds one of these in its `FilterData`.
pub struct MidiProcessingState {
    pub mappings: parking_lot::RwLock<Arc<ResolvedMappings>>,
    pub learn_mode: AtomicBool,
    pub learn_captured: AtomicBool,
    pub toggle_prev: [bool; 128],
}

impl MidiProcessingState {
    pub fn new() -> Self {
        Self {
            mappings: parking_lot::RwLock::new(Arc::new(ResolvedMappings::empty())),
            learn_mode: AtomicBool::new(false),
            learn_captured: AtomicBool::new(false),
            toggle_prev: [false; 128],
        }
    }
}

/// Result of processing a single MIDI message during learn mode.
pub struct LearnCapture {
    pub channel: u8,
    pub cc: u8,
    pub message_type: MidiMessageType,
}

/// # Safety
/// - `dsp_buf` must be a valid pointer returned by `pw_filter_get_dsp_buffer`
///   for a "8 bit raw midi" port
/// - Must be called from the PipeWire RT process callback
pub unsafe fn process_midi_buffer(
    dsp_buf: *mut std::ffi::c_void,
    state: &mut MidiProcessingState,
    event_tx: &std::sync::mpsc::Sender<crate::pipewire::PwEvent>,
    instance_id: u64,
) -> Option<LearnCapture> {
    if dsp_buf.is_null() {
        return None;
    }

    unsafe {
        let seq = dsp_buf as *const libspa::sys::spa_pod_sequence;
        let body = &(*seq).body;
        let body_size = (*seq).pod.size as u32;

        let mut learn_result: Option<LearnCapture> = None;

        let mut ctrl = libspa::sys::spa_pod_control_first(body);
        while libspa::sys::spa_pod_control_is_inside(body, body_size, ctrl) {
            if (*ctrl).type_ == libspa::sys::SPA_CONTROL_Midi {
                let midi_size = (*ctrl).value.size as usize;
                let midi_data = (&(*ctrl).value as *const libspa::sys::spa_pod as *const u8)
                    .add(std::mem::size_of::<libspa::sys::spa_pod>());

                if midi_size >= 3 {
                    let status = *midi_data;
                    let byte1 = *midi_data.add(1);
                    let byte2 = *midi_data.add(2);
                    let channel = status & 0x0F;
                    let msg_type = status & 0xF0;

                    if msg_type == 0xB0 {
                        if state.learn_mode.load(Ordering::Acquire) {
                            if !state.learn_captured.swap(true, Ordering::SeqCst) {
                                learn_result = Some(LearnCapture {
                                    channel,
                                    cc: byte1,
                                    message_type: MidiMessageType::Cc,
                                });
                            }
                        } else {
                            handle_cc(
                                state,
                                channel,
                                byte1,
                                byte2,
                                MidiMessageType::Cc,
                                event_tx,
                                instance_id,
                            );
                        }
                    } else if msg_type == 0x90 || msg_type == 0x80 {
                        let velocity = if msg_type == 0x80 { 0 } else { byte2 };
                        if state.learn_mode.load(Ordering::Acquire) {
                            if velocity > 0 && !state.learn_captured.swap(true, Ordering::SeqCst) {
                                learn_result = Some(LearnCapture {
                                    channel,
                                    cc: byte1,
                                    message_type: MidiMessageType::Note,
                                });
                            }
                        } else {
                            handle_cc(
                                state,
                                channel,
                                byte1,
                                velocity,
                                MidiMessageType::Note,
                                event_tx,
                                instance_id,
                            );
                        }
                    }
                }
            }
            ctrl = libspa::sys::spa_pod_control_next(ctrl);
        }

        learn_result
    }
}

#[inline]
unsafe fn handle_cc(
    state: &mut MidiProcessingState,
    channel: u8,
    cc: u8,
    value: u8,
    message_type: MidiMessageType,
    event_tx: &std::sync::mpsc::Sender<crate::pipewire::PwEvent>,
    instance_id: u64,
) {
    let mappings_guard = match state.mappings.try_read() {
        Some(g) => g,
        None => return,
    };
    let mappings = mappings_guard.clone();
    drop(mappings_guard);

    // For per-plugin MIDI, we match on channel + cc only (no device_name filtering,
    // since the MIDI is already routed to this specific plugin's port).
    let Some(entry) = mappings.find_any_device(channel, cc, message_type) else {
        return;
    };

    let new_value = compute_mapped_value(entry, state, cc, value);
    let new_value = match new_value {
        Some(v) => v,
        None => return,
    };

    if let Some(slot) = entry
        .port_updates
        .control_inputs
        .iter()
        .find(|s| s.port_index == entry.port_index)
    {
        slot.value.store(new_value);
    }

    let _ = event_tx.send(crate::pipewire::PwEvent::Plugin(
        crate::pipewire::PluginEvent::ParameterChanged {
            instance_id: entry.instance_id,
            port_index: entry.port_index,
            value: new_value,
        },
    ));
}

/// # Safety
/// - `out_buf` must be a valid pointer returned by `pw_filter_get_dsp_buffer`
///   for a "8 bit raw midi" output port
/// - Must be called from the PipeWire RT process callback
pub unsafe fn clear_midi_buffer(out_buf: *mut std::ffi::c_void) {
    if out_buf.is_null() {
        return;
    }

    unsafe {
        let out_seq = out_buf as *mut libspa::sys::spa_pod_sequence;
        (*out_seq).pod.size = std::mem::size_of::<libspa::sys::spa_pod_sequence_body>() as u32;
        (*out_seq).pod.type_ = libspa::sys::SPA_TYPE_Sequence;
        (*out_seq).body.unit = 0;
        (*out_seq).body.pad = 0;
    }
}

/// # Safety
/// - Both pointers must be valid DSP buffer pointers from `pw_filter_get_dsp_buffer`
///   for "8 bit raw midi" ports
/// - Must be called from the PipeWire RT process callback
pub unsafe fn forward_midi_buffer(in_buf: *mut std::ffi::c_void, out_buf: *mut std::ffi::c_void) {
    if in_buf.is_null() || out_buf.is_null() {
        return;
    }

    unsafe {
        let in_seq = in_buf as *const libspa::sys::spa_pod_sequence;
        let total_size = std::mem::size_of::<libspa::sys::spa_pod>() + (*in_seq).pod.size as usize;

        std::ptr::copy_nonoverlapping(in_buf as *const u8, out_buf as *mut u8, total_size);
    }
}

/// Returns `None` if the value should not be applied (e.g. toggle not triggered).
#[inline]
fn compute_mapped_value(
    entry: &ResolvedMappingEntry,
    state: &mut MidiProcessingState,
    cc: u8,
    value: u8,
) -> Option<f32> {
    match entry.mode {
        MappingMode::Continuous => {
            let t = value as f32 / 127.0;
            if entry.is_logarithmic {
                Some(entry.min * (entry.max / entry.min).powf(t))
            } else {
                Some(entry.min + t * (entry.max - entry.min))
            }
        }
        MappingMode::Toggle => {
            let pressed = value > 63;
            let prev = state.toggle_prev[cc as usize];
            state.toggle_prev[cc as usize] = pressed;

            if pressed && !prev {
                let slot = entry
                    .port_updates
                    .control_inputs
                    .iter()
                    .find(|s| s.port_index == entry.port_index)?;
                let current = slot.value.load();
                let mid = (entry.min + entry.max) / 2.0;
                if current > mid {
                    Some(entry.min)
                } else {
                    Some(entry.max)
                }
            } else {
                None
            }
        }
        MappingMode::Momentary => {
            if value > 63 {
                Some(entry.max)
            } else {
                Some(entry.min)
            }
        }
    }
}
