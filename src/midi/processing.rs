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

/// A single raw MIDI event extracted from a PipeWire DSP buffer.
#[derive(Clone, Copy)]
pub struct RawMidiEvent {
    pub offset: u32,
    pub data: [u8; 3],
    pub size: u8,
}

/// Maximum number of MIDI events we collect per process cycle (RT-safe, no heap).
pub const MAX_MIDI_EVENTS: usize = 256;

/// Extract raw MIDI events from a PipeWire DSP buffer into a fixed-size array.
///
/// # Safety
/// - `dsp_buf` must be a valid pointer from `pw_filter_get_dsp_buffer` for a MIDI port
/// - Must be called from the PipeWire RT process callback
pub unsafe fn extract_midi_events(
    dsp_buf: *mut std::ffi::c_void,
    out: &mut [RawMidiEvent; MAX_MIDI_EVENTS],
) -> usize {
    if dsp_buf.is_null() {
        return 0;
    }

    unsafe {
        let seq = dsp_buf as *const libspa::sys::spa_pod_sequence;
        let body = &(*seq).body;
        let body_size = (*seq).pod.size as u32;

        let mut count = 0usize;
        let mut ctrl = libspa::sys::spa_pod_control_first(body);
        while libspa::sys::spa_pod_control_is_inside(body, body_size, ctrl) {
            if (*ctrl).type_ == libspa::sys::SPA_CONTROL_Midi {
                let midi_size = (*ctrl).value.size as usize;
                let midi_data = (&(*ctrl).value as *const libspa::sys::spa_pod as *const u8)
                    .add(std::mem::size_of::<libspa::sys::spa_pod>());

                if midi_size >= 1 && midi_size <= 3 && count < MAX_MIDI_EVENTS {
                    let mut evt = RawMidiEvent {
                        offset: (*ctrl).offset,
                        data: [0; 3],
                        size: midi_size as u8,
                    };
                    std::ptr::copy_nonoverlapping(midi_data, evt.data.as_mut_ptr(), midi_size);
                    out[count] = evt;
                    count += 1;
                }
            }
            ctrl = libspa::sys::spa_pod_control_next(ctrl);
        }
        count
    }
}

/// Returns `None` if the value should not be applied (e.g. toggle not triggered).
#[inline]
pub(crate) fn compute_mapped_value(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::types::MidiCcSource;
    use crate::plugin::types::*;
    use std::sync::Arc;

    fn make_entry(min: f32, max: f32, mode: MappingMode, is_log: bool) -> (ResolvedMappingEntry, SharedPortUpdates) {
        let port_updates = Arc::new(PortUpdates {
            control_inputs: vec![PortSlot {
                port_index: 0,
                value: AtomicF32::new(min),
            }],
            control_outputs: Vec::new(),
            atom_outputs: Vec::new(),
            atom_inputs: Vec::new(),
        });
        let entry = ResolvedMappingEntry {
            port_updates: port_updates.clone(),
            port_index: 0,
            instance_id: 1,
            min,
            max,
            mode,
            source: MidiCcSource {
                device_name: "test".to_string(),
                channel: Some(0),
                cc: 1,
                message_type: MidiMessageType::Cc,
            },
            is_logarithmic: is_log,
            is_toggle: mode == MappingMode::Toggle,
        };
        (entry, port_updates)
    }

    // ---- Continuous mode ----

    #[test]
    fn continuous_min_value() {
        let (entry, _pu) = make_entry(0.0, 1.0, MappingMode::Continuous, false);
        let mut state = MidiProcessingState::new();
        let result = compute_mapped_value(&entry, &mut state, 1, 0);
        assert!(result.is_some());
        assert!((result.unwrap() - 0.0).abs() < 1e-5);
    }

    #[test]
    fn continuous_max_value() {
        let (entry, _pu) = make_entry(0.0, 1.0, MappingMode::Continuous, false);
        let mut state = MidiProcessingState::new();
        let result = compute_mapped_value(&entry, &mut state, 1, 127);
        assert!(result.is_some());
        assert!((result.unwrap() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn continuous_mid_value() {
        let (entry, _pu) = make_entry(0.0, 1.0, MappingMode::Continuous, false);
        let mut state = MidiProcessingState::new();
        let result = compute_mapped_value(&entry, &mut state, 1, 64);
        let expected = 64.0 / 127.0;
        assert!((result.unwrap() - expected).abs() < 1e-5);
    }

    #[test]
    fn continuous_custom_range() {
        let (entry, _pu) = make_entry(20.0, 20000.0, MappingMode::Continuous, false);
        let mut state = MidiProcessingState::new();

        let at_zero = compute_mapped_value(&entry, &mut state, 1, 0).unwrap();
        assert!((at_zero - 20.0).abs() < 1e-3);

        let at_max = compute_mapped_value(&entry, &mut state, 1, 127).unwrap();
        assert!((at_max - 20000.0).abs() < 1e-3);
    }

    #[test]
    fn continuous_logarithmic() {
        let (entry, _pu) = make_entry(20.0, 20000.0, MappingMode::Continuous, true);
        let mut state = MidiProcessingState::new();

        let at_zero = compute_mapped_value(&entry, &mut state, 1, 0).unwrap();
        assert!((at_zero - 20.0).abs() < 1e-3);

        let at_max = compute_mapped_value(&entry, &mut state, 1, 127).unwrap();
        assert!((at_max - 20000.0).abs() < 1.0);

        // Logarithmic midpoint should be geometric mean, not arithmetic
        let at_mid = compute_mapped_value(&entry, &mut state, 1, 64).unwrap();
        let linear_mid = (20.0 + 20000.0) / 2.0;
        assert!(at_mid < linear_mid); // log curve is below linear at midpoint
    }

    // ---- Momentary mode ----

    #[test]
    fn momentary_above_threshold() {
        let (entry, _pu) = make_entry(0.0, 1.0, MappingMode::Momentary, false);
        let mut state = MidiProcessingState::new();
        let result = compute_mapped_value(&entry, &mut state, 1, 64);
        assert_eq!(result, Some(1.0));
    }

    #[test]
    fn momentary_at_threshold() {
        let (entry, _pu) = make_entry(0.0, 1.0, MappingMode::Momentary, false);
        let mut state = MidiProcessingState::new();
        let result = compute_mapped_value(&entry, &mut state, 1, 63);
        assert_eq!(result, Some(0.0));
    }

    #[test]
    fn momentary_zero() {
        let (entry, _pu) = make_entry(0.0, 1.0, MappingMode::Momentary, false);
        let mut state = MidiProcessingState::new();
        let result = compute_mapped_value(&entry, &mut state, 1, 0);
        assert_eq!(result, Some(0.0));
    }

    #[test]
    fn momentary_max() {
        let (entry, _pu) = make_entry(0.0, 1.0, MappingMode::Momentary, false);
        let mut state = MidiProcessingState::new();
        let result = compute_mapped_value(&entry, &mut state, 1, 127);
        assert_eq!(result, Some(1.0));
    }

    // ---- Toggle mode ----

    #[test]
    fn toggle_first_press_turns_on() {
        let (entry, pu) = make_entry(0.0, 1.0, MappingMode::Toggle, false);
        // Start at min
        pu.control_inputs[0].value.store(0.0);
        let mut state = MidiProcessingState::new();

        // Press (value > 63, previous was false)
        let result = compute_mapped_value(&entry, &mut state, 1, 127);
        assert_eq!(result, Some(1.0)); // should toggle to max
    }

    #[test]
    fn toggle_second_press_turns_off() {
        let (entry, pu) = make_entry(0.0, 1.0, MappingMode::Toggle, false);
        pu.control_inputs[0].value.store(1.0);
        let mut state = MidiProcessingState::new();

        // Press
        let result = compute_mapped_value(&entry, &mut state, 1, 127);
        assert_eq!(result, Some(0.0)); // current > mid, toggle to min
    }

    #[test]
    fn toggle_hold_returns_none() {
        let (entry, _pu) = make_entry(0.0, 1.0, MappingMode::Toggle, false);
        let mut state = MidiProcessingState::new();

        // First press
        compute_mapped_value(&entry, &mut state, 1, 127);
        // Continued hold (value > 63, prev was already true)
        let result = compute_mapped_value(&entry, &mut state, 1, 127);
        assert_eq!(result, None);
    }

    #[test]
    fn toggle_release_returns_none() {
        let (entry, _pu) = make_entry(0.0, 1.0, MappingMode::Toggle, false);
        let mut state = MidiProcessingState::new();

        // Release (value <= 63, no transition)
        let result = compute_mapped_value(&entry, &mut state, 1, 0);
        assert_eq!(result, None);
    }

    #[test]
    fn toggle_press_release_press_toggles_twice() {
        let (entry, pu) = make_entry(0.0, 1.0, MappingMode::Toggle, false);
        pu.control_inputs[0].value.store(0.0);
        let mut state = MidiProcessingState::new();

        // First press → turns on
        let r1 = compute_mapped_value(&entry, &mut state, 1, 127);
        assert_eq!(r1, Some(1.0));
        pu.control_inputs[0].value.store(1.0);

        // Release
        let r2 = compute_mapped_value(&entry, &mut state, 1, 0);
        assert_eq!(r2, None);

        // Second press → turns off
        let r3 = compute_mapped_value(&entry, &mut state, 1, 127);
        assert_eq!(r3, Some(0.0));
    }

    // ---- RawMidiEvent ----

    #[test]
    fn raw_midi_event_copy() {
        let evt = RawMidiEvent {
            offset: 42,
            data: [0x90, 60, 100],
            size: 3,
        };
        let copy = evt;
        assert_eq!(copy.offset, 42);
        assert_eq!(copy.data, [0x90, 60, 100]);
        assert_eq!(copy.size, 3);
    }
}
