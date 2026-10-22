//! Shared RT-safe MIDI processing functions.
//!
//! These functions are called from each plugin filter's `on_process` callback
//! to parse incoming MIDI messages from PipeWire DSP buffers and apply
//! CC/Note mappings to plugin parameters via `SharedPortUpdates`.
//!
//! Nothing here allocates, locks or sends on a channel: notifications for the
//! main thread are pushed into the filter's SPSC queue (`RtEvent`) and dropped
//! when the queue is full.

use std::sync::atomic::{AtomicBool, Ordering};

use super::filter::{ResolvedMappingEntry, ResolvedMappings};
use super::types::{MappingMode, MidiMessageType};

/// Event pushed from the RT callback to the main loop (see `FilterMain::drain_events`).
/// Must stay `Copy` and allocation-free.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RtEvent {
    /// A MIDI mapping changed a parameter.
    ParameterChanged {
        instance_id: u64,
        port_index: usize,
        value: f32,
    },
    /// MIDI learn captured a CC / note.
    LearnCaptured {
        channel: u8,
        number: u8,
        message_type: MidiMessageType,
    },
}

/// MIDI learn flags shared between the main thread (sets) and RT (captures).
pub struct LearnFlags {
    pub mode: AtomicBool,
    pub captured: AtomicBool,
}

impl LearnFlags {
    pub fn new() -> Self {
        Self {
            mode: AtomicBool::new(false),
            captured: AtomicBool::new(false),
        }
    }

    pub fn set(&self, enabled: bool) {
        self.captured.store(false, Ordering::SeqCst);
        self.mode.store(enabled, Ordering::SeqCst);
    }
}

const TOGGLE_SLOTS: usize = 2 * 16 * 128;

#[inline]
fn toggle_index(message_type: MidiMessageType, channel: u8, number: u8) -> usize {
    let t = match message_type {
        MidiMessageType::Cc => 0,
        MidiMessageType::Note => 1,
    };
    (t * 16 + (channel & 0x0F) as usize) * 128 + (number & 0x7F) as usize
}

/// RT-private state for MIDI mapping processing (toggle edge detection).
pub struct MidiProcessingState {
    toggle_prev: [bool; TOGGLE_SLOTS],
}

impl MidiProcessingState {
    pub fn new() -> Self {
        Self {
            toggle_prev: [false; TOGGLE_SLOTS],
        }
    }
}

#[inline]
fn push_event(events: &mut rtrb::Producer<RtEvent>, event: RtEvent) -> bool {
    events.push(event).is_ok()
}

/// Walk a "8 bit raw midi" DSP buffer and apply learn / mappings to every
/// CC and note message.
///
/// # Safety
/// - `dsp_buf` must be null or a valid pointer returned by `pw_filter_get_dsp_buffer`
///   for a "8 bit raw midi" port
/// - Must be called from the PipeWire RT process callback
pub unsafe fn process_midi_buffer(
    dsp_buf: *mut std::ffi::c_void,
    state: &mut MidiProcessingState,
    learn: &LearnFlags,
    mappings: &ResolvedMappings,
    events: &mut rtrb::Producer<RtEvent>,
) {
    if dsp_buf.is_null() {
        return;
    }

    unsafe {
        let seq = dsp_buf as *const libspa::sys::spa_pod_sequence;
        let body = &(*seq).body;
        let body_size = (*seq).pod.size;

        let mut ctrl = libspa::sys::spa_pod_control_first(body);
        while libspa::sys::spa_pod_control_is_inside(body, body_size, ctrl) {
            if (*ctrl).type_ == libspa::sys::SPA_CONTROL_Midi {
                let midi_size = (*ctrl).value.size as usize;
                let midi_data = (&(*ctrl).value as *const libspa::sys::spa_pod as *const u8)
                    .add(std::mem::size_of::<libspa::sys::spa_pod>());

                if midi_size >= 3 {
                    let msg = [*midi_data, *midi_data.add(1), *midi_data.add(2)];
                    handle_midi_message(state, learn, mappings, events, msg);
                }
            }
            ctrl = libspa::sys::spa_pod_control_next(ctrl);
        }
    }
}

/// Apply one 3-byte MIDI message (CC / note on / note off) to learn mode or
/// the resolved mappings.  Other message types are ignored.
pub fn handle_midi_message(
    state: &mut MidiProcessingState,
    learn: &LearnFlags,
    mappings: &ResolvedMappings,
    events: &mut rtrb::Producer<RtEvent>,
    msg: [u8; 3],
) {
    let status = msg[0];
    let channel = status & 0x0F;
    let (message_type, number, value) = match status & 0xF0 {
        0xB0 => (MidiMessageType::Cc, msg[1], msg[2]),
        0x90 => (MidiMessageType::Note, msg[1], msg[2]),
        0x80 => (MidiMessageType::Note, msg[1], 0),
        _ => return,
    };

    if learn.mode.load(Ordering::Acquire) {
        // Notes are learned on note-on only
        if message_type == MidiMessageType::Note && value == 0 {
            return;
        }
        if !learn.captured.swap(true, Ordering::SeqCst) {
            let ok = push_event(
                events,
                RtEvent::LearnCaptured {
                    channel,
                    number,
                    message_type,
                },
            );
            if !ok {
                // Queue full: let the next message try again
                learn.captured.store(false, Ordering::SeqCst);
            }
        }
        return;
    }

    // For per-plugin MIDI, we match on channel + number only (no device_name
    // filtering, since the MIDI is already routed to this plugin's port).
    let Some(entry) = mappings.find_any_device(channel, number, message_type) else {
        return;
    };

    let Some(new_value) = compute_mapped_value(entry, state, message_type, channel, number, value)
    else {
        return;
    };

    if let Some(slot) = entry
        .port_updates
        .control_inputs
        .iter()
        .find(|s| s.port_index == entry.port_index)
    {
        slot.value.store(new_value);
    }

    push_event(
        events,
        RtEvent::ParameterChanged {
            instance_id: entry.instance_id,
            port_index: entry.port_index,
            value: new_value,
        },
    );
}

/// Whether a message counts as "pressed" for toggle/momentary/switch handling.
/// Notes: note-on with velocity > 0 is pressed; note-off / velocity 0 is released.
/// CCs: values >= 64 are pressed (MIDI switch convention).
#[inline]
pub(crate) fn is_pressed(message_type: MidiMessageType, value: u8) -> bool {
    match message_type {
        MidiMessageType::Note => value > 0,
        MidiMessageType::Cc => value > 63,
    }
}

/// Returns `None` if the value should not be applied (e.g. toggle not triggered).
#[inline]
pub(crate) fn compute_mapped_value(
    entry: &ResolvedMappingEntry,
    state: &mut MidiProcessingState,
    message_type: MidiMessageType,
    channel: u8,
    number: u8,
    value: u8,
) -> Option<f32> {
    let pressed = is_pressed(message_type, value);
    match entry.mode {
        MappingMode::Continuous => {
            if entry.is_toggle {
                // Boolean parameter: snap instead of producing in-between values
                return Some(if pressed { entry.max } else { entry.min });
            }
            let t = value as f32 / 127.0;
            if entry.is_logarithmic {
                Some(entry.min * (entry.max / entry.min).powf(t))
            } else {
                Some(entry.min + t * (entry.max - entry.min))
            }
        }
        MappingMode::Toggle => {
            let idx = toggle_index(message_type, channel, number);
            let prev = state.toggle_prev[idx];
            state.toggle_prev[idx] = pressed;

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
            if pressed {
                Some(entry.max)
            } else {
                Some(entry.min)
            }
        }
    }
}

const SEQ_HEADER_SIZE: usize = 16; // spa_pod + spa_pod_sequence_body
const CONTROL_HEADER_SIZE: usize = 16; // spa_pod_control without value body

#[inline]
fn read_u32(buf: &[u8], at: usize) -> u32 {
    u32::from_ne_bytes([buf[at], buf[at + 1], buf[at + 2], buf[at + 3]])
}

#[inline]
fn write_u32(buf: &mut [u8], at: usize, v: u32) {
    buf[at..at + 4].copy_from_slice(&v.to_ne_bytes());
}

/// Copy the spa_pod_sequence in `input` into `out`, keeping only whole controls
/// that fit.  `out` always ends up holding a valid (possibly empty) sequence
/// when it is at least 16 bytes long.  Returns the number of bytes written.
pub fn copy_sequence_bounded(input: &[u8], out: &mut [u8]) -> usize {
    if out.len() < SEQ_HEADER_SIZE {
        return 0;
    }
    let seq_type = libspa::sys::SPA_TYPE_Sequence as u32;
    // Empty sequence: pod.size = body header only
    write_u32(out, 0, 8);
    write_u32(out, 4, seq_type);
    write_u32(out, 8, 0);
    write_u32(out, 12, 0);

    if input.len() < SEQ_HEADER_SIZE || read_u32(input, 4) != seq_type {
        return SEQ_HEADER_SIZE;
    }
    // Keep the unit field
    write_u32(out, 8, read_u32(input, 8));

    let end = (8 + read_u32(input, 0) as usize).min(input.len());
    let mut pos = SEQ_HEADER_SIZE;
    let mut written = SEQ_HEADER_SIZE;
    while pos + CONTROL_HEADER_SIZE <= end {
        let value_size = read_u32(input, pos + 8) as usize;
        let size = CONTROL_HEADER_SIZE + value_size;
        if size > end - pos {
            break;
        }
        let padded = (size + 7) & !7;
        if padded > out.len() - written {
            break;
        }
        out[written..written + size].copy_from_slice(&input[pos..pos + size]);
        out[written + size..written + padded].fill(0);
        written += padded;
        pos += padded;
    }
    write_u32(out, 0, (written - 8) as u32);
    written
}

/// Write the MIDI thru output for this cycle: the output port always gets a
/// valid sequence (empty when there is no input), never larger than the
/// output buffer.
///
/// # Safety
/// - `out_port` must be null or a port_data pointer of a "8 bit raw midi" output port
/// - `in_buf` must be null or a valid pointer returned by `pw_filter_get_dsp_buffer`
///   for a "8 bit raw midi" input port
/// - Must be called from the PipeWire RT process callback
pub unsafe fn write_midi_output(out_port: *mut std::ffi::c_void, in_buf: *const std::ffi::c_void) {
    if out_port.is_null() {
        return;
    }
    unsafe {
        let b = pipewire::sys::pw_filter_dequeue_buffer(out_port);
        if b.is_null() {
            return;
        }
        let sb = (*b).buffer;
        if !sb.is_null() && (*sb).n_datas > 0 && !(*sb).datas.is_null() {
            let d = &mut *(*sb).datas;
            if !d.data.is_null() {
                let out = std::slice::from_raw_parts_mut(d.data as *mut u8, d.maxsize as usize);
                let input: &[u8] = if in_buf.is_null() {
                    &[]
                } else {
                    let pod = in_buf as *const libspa::sys::spa_pod;
                    std::slice::from_raw_parts(in_buf as *const u8, 8 + (*pod).size as usize)
                };
                let n = copy_sequence_bounded(input, out);
                if !d.chunk.is_null() {
                    (*d.chunk).offset = 0;
                    (*d.chunk).size = n as u32;
                    (*d.chunk).stride = 1;
                    (*d.chunk).flags = 0;
                }
            }
        }
        pipewire::sys::pw_filter_queue_buffer(out_port, b);
    }
}

/// A single raw MIDI event extracted from a PipeWire DSP buffer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RawMidiEvent {
    pub offset: u32,
    pub data: [u8; 3],
    pub size: u8,
}

impl RawMidiEvent {
    pub const EMPTY: RawMidiEvent = RawMidiEvent {
        offset: 0,
        data: [0; 3],
        size: 0,
    };
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
        let body_size = (*seq).pod.size;

        let mut count = 0usize;
        let mut ctrl = libspa::sys::spa_pod_control_first(body);
        while libspa::sys::spa_pod_control_is_inside(body, body_size, ctrl) {
            if (*ctrl).type_ == libspa::sys::SPA_CONTROL_Midi {
                let midi_size = (*ctrl).value.size as usize;
                let midi_data = (&(*ctrl).value as *const libspa::sys::spa_pod as *const u8)
                    .add(std::mem::size_of::<libspa::sys::spa_pod>());

                if (1..=3).contains(&midi_size) && count < MAX_MIDI_EVENTS {
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

/// Select the events that fall into the block `[start, start + len)` of a
/// cycle of `total` frames, rebasing their offsets to the block start.
/// Offsets at or beyond `total` are clamped into the last frame.
/// Returns the number of events written to `out`.
pub fn chunk_midi_events(
    events: &[RawMidiEvent],
    start: usize,
    len: usize,
    total: usize,
    out: &mut [RawMidiEvent],
) -> usize {
    if len == 0 || total == 0 {
        return 0;
    }
    let end = start + len;
    let mut n = 0;
    for ev in events {
        let off = (ev.offset as usize).min(total - 1);
        if off >= start && off < end && n < out.len() {
            out[n] = RawMidiEvent {
                offset: (off - start) as u32,
                ..*ev
            };
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::types::MidiCcSource;
    use crate::plugin::types::*;
    use std::sync::Arc;

    fn make_entry_typed(
        min: f32,
        max: f32,
        mode: MappingMode,
        is_log: bool,
        message_type: MidiMessageType,
    ) -> (ResolvedMappingEntry, SharedPortUpdates) {
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
                message_type,
            },
            is_logarithmic: is_log,
            is_toggle: mode == MappingMode::Toggle,
        };
        (entry, port_updates)
    }

    fn make_entry(
        min: f32,
        max: f32,
        mode: MappingMode,
        is_log: bool,
    ) -> (ResolvedMappingEntry, SharedPortUpdates) {
        make_entry_typed(min, max, mode, is_log, MidiMessageType::Cc)
    }

    fn cc(entry: &ResolvedMappingEntry, state: &mut MidiProcessingState, value: u8) -> Option<f32> {
        compute_mapped_value(entry, state, MidiMessageType::Cc, 0, 1, value)
    }

    fn note(entry: &ResolvedMappingEntry, state: &mut MidiProcessingState, vel: u8) -> Option<f32> {
        compute_mapped_value(entry, state, MidiMessageType::Note, 0, 1, vel)
    }

    // ---- Continuous mode ----

    #[test]
    fn continuous_min_value() {
        let (entry, _pu) = make_entry(0.0, 1.0, MappingMode::Continuous, false);
        let mut state = MidiProcessingState::new();
        let result = cc(&entry, &mut state, 0);
        assert!((result.unwrap() - 0.0).abs() < 1e-5);
    }

    #[test]
    fn continuous_max_value() {
        let (entry, _pu) = make_entry(0.0, 1.0, MappingMode::Continuous, false);
        let mut state = MidiProcessingState::new();
        let result = cc(&entry, &mut state, 127);
        assert!((result.unwrap() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn continuous_mid_value() {
        let (entry, _pu) = make_entry(0.0, 1.0, MappingMode::Continuous, false);
        let mut state = MidiProcessingState::new();
        let result = cc(&entry, &mut state, 64);
        let expected = 64.0 / 127.0;
        assert!((result.unwrap() - expected).abs() < 1e-5);
    }

    #[test]
    fn continuous_custom_range() {
        let (entry, _pu) = make_entry(20.0, 20000.0, MappingMode::Continuous, false);
        let mut state = MidiProcessingState::new();

        let at_zero = cc(&entry, &mut state, 0).unwrap();
        assert!((at_zero - 20.0).abs() < 1e-3);

        let at_max = cc(&entry, &mut state, 127).unwrap();
        assert!((at_max - 20000.0).abs() < 1e-3);
    }

    #[test]
    fn continuous_logarithmic() {
        let (entry, _pu) = make_entry(20.0, 20000.0, MappingMode::Continuous, true);
        let mut state = MidiProcessingState::new();

        let at_zero = cc(&entry, &mut state, 0).unwrap();
        assert!((at_zero - 20.0).abs() < 1e-3);

        let at_max = cc(&entry, &mut state, 127).unwrap();
        assert!((at_max - 20000.0).abs() < 1.0);

        // Logarithmic midpoint should be geometric mean, not arithmetic
        let at_mid = cc(&entry, &mut state, 64).unwrap();
        let linear_mid = (20.0 + 20000.0) / 2.0;
        assert!(at_mid < linear_mid); // log curve is below linear at midpoint
    }

    #[test]
    fn continuous_toggle_param_snaps() {
        let (mut entry, _pu) = make_entry(0.0, 1.0, MappingMode::Continuous, false);
        entry.is_toggle = true;
        let mut state = MidiProcessingState::new();
        assert_eq!(cc(&entry, &mut state, 30), Some(0.0));
        assert_eq!(cc(&entry, &mut state, 100), Some(1.0));
        // Soft note still switches on
        assert_eq!(note(&entry, &mut state, 5), Some(1.0));
        assert_eq!(note(&entry, &mut state, 0), Some(0.0));
    }

    // ---- Momentary mode ----

    #[test]
    fn momentary_above_threshold() {
        let (entry, _pu) = make_entry(0.0, 1.0, MappingMode::Momentary, false);
        let mut state = MidiProcessingState::new();
        assert_eq!(cc(&entry, &mut state, 64), Some(1.0));
    }

    #[test]
    fn momentary_at_threshold() {
        let (entry, _pu) = make_entry(0.0, 1.0, MappingMode::Momentary, false);
        let mut state = MidiProcessingState::new();
        assert_eq!(cc(&entry, &mut state, 63), Some(0.0));
    }

    #[test]
    fn momentary_zero() {
        let (entry, _pu) = make_entry(0.0, 1.0, MappingMode::Momentary, false);
        let mut state = MidiProcessingState::new();
        assert_eq!(cc(&entry, &mut state, 0), Some(0.0));
    }

    #[test]
    fn momentary_max() {
        let (entry, _pu) = make_entry(0.0, 1.0, MappingMode::Momentary, false);
        let mut state = MidiProcessingState::new();
        assert_eq!(cc(&entry, &mut state, 127), Some(1.0));
    }

    #[test]
    fn momentary_note_soft_hit_is_on() {
        let (entry, _pu) = make_entry_typed(
            0.0,
            1.0,
            MappingMode::Momentary,
            false,
            MidiMessageType::Note,
        );
        let mut state = MidiProcessingState::new();
        assert_eq!(note(&entry, &mut state, 1), Some(1.0));
        assert_eq!(note(&entry, &mut state, 40), Some(1.0));
        assert_eq!(note(&entry, &mut state, 0), Some(0.0));
    }

    // ---- Toggle mode ----

    #[test]
    fn toggle_first_press_turns_on() {
        let (entry, pu) = make_entry(0.0, 1.0, MappingMode::Toggle, false);
        pu.control_inputs[0].value.store(0.0);
        let mut state = MidiProcessingState::new();
        assert_eq!(cc(&entry, &mut state, 127), Some(1.0));
    }

    #[test]
    fn toggle_second_press_turns_off() {
        let (entry, pu) = make_entry(0.0, 1.0, MappingMode::Toggle, false);
        pu.control_inputs[0].value.store(1.0);
        let mut state = MidiProcessingState::new();
        assert_eq!(cc(&entry, &mut state, 127), Some(0.0));
    }

    #[test]
    fn toggle_hold_returns_none() {
        let (entry, _pu) = make_entry(0.0, 1.0, MappingMode::Toggle, false);
        let mut state = MidiProcessingState::new();
        cc(&entry, &mut state, 127);
        assert_eq!(cc(&entry, &mut state, 127), None);
    }

    #[test]
    fn toggle_release_returns_none() {
        let (entry, _pu) = make_entry(0.0, 1.0, MappingMode::Toggle, false);
        let mut state = MidiProcessingState::new();
        assert_eq!(cc(&entry, &mut state, 0), None);
    }

    #[test]
    fn toggle_press_release_press_toggles_twice() {
        let (entry, pu) = make_entry(0.0, 1.0, MappingMode::Toggle, false);
        pu.control_inputs[0].value.store(0.0);
        let mut state = MidiProcessingState::new();

        assert_eq!(cc(&entry, &mut state, 127), Some(1.0));
        pu.control_inputs[0].value.store(1.0);
        assert_eq!(cc(&entry, &mut state, 0), None);
        assert_eq!(cc(&entry, &mut state, 127), Some(0.0));
    }

    #[test]
    fn toggle_note_soft_hit_toggles() {
        let (entry, pu) =
            make_entry_typed(0.0, 1.0, MappingMode::Toggle, false, MidiMessageType::Note);
        let mut state = MidiProcessingState::new();
        assert_eq!(note(&entry, &mut state, 10), Some(1.0));
        pu.control_inputs[0].value.store(1.0);
        assert_eq!(note(&entry, &mut state, 0), None);
        assert_eq!(note(&entry, &mut state, 3), Some(0.0));
    }

    #[test]
    fn toggle_state_is_per_type_channel_number() {
        let (entry, pu) = make_entry(0.0, 1.0, MappingMode::Toggle, false);
        let mut state = MidiProcessingState::new();
        // CC 1 on channel 0 held down
        assert!(compute_mapped_value(&entry, &mut state, MidiMessageType::Cc, 0, 1, 127).is_some());
        pu.control_inputs[0].value.store(1.0);
        // Same number on another channel / as a note is an independent button
        assert!(compute_mapped_value(&entry, &mut state, MidiMessageType::Cc, 1, 1, 127).is_some());
        assert!(
            compute_mapped_value(&entry, &mut state, MidiMessageType::Note, 0, 1, 127).is_some()
        );
        // While the original is still considered held
        assert!(compute_mapped_value(&entry, &mut state, MidiMessageType::Cc, 0, 1, 127).is_none());
    }

    // ---- handle_midi_message ----

    #[test]
    fn handle_message_pushes_parameter_event() {
        let (entry, pu) = make_entry(0.0, 1.0, MappingMode::Continuous, false);
        let mappings = ResolvedMappings::new(vec![entry]);
        let mut state = MidiProcessingState::new();
        let learn = LearnFlags::new();
        let (mut tx, mut rx) = rtrb::RingBuffer::new(4);

        handle_midi_message(&mut state, &learn, &mappings, &mut tx, [0xB0, 1, 127]);
        assert!((pu.control_inputs[0].value.load() - 1.0).abs() < 1e-6);
        assert_eq!(
            rx.pop(),
            Ok(RtEvent::ParameterChanged {
                instance_id: 1,
                port_index: 0,
                value: 1.0
            })
        );
        // Unmapped CC: nothing
        handle_midi_message(&mut state, &learn, &mappings, &mut tx, [0xB0, 2, 127]);
        assert!(rx.pop().is_err());
    }

    #[test]
    fn handle_message_drops_when_queue_full() {
        let (entry, pu) = make_entry(0.0, 1.0, MappingMode::Continuous, false);
        let mappings = ResolvedMappings::new(vec![entry]);
        let mut state = MidiProcessingState::new();
        let learn = LearnFlags::new();
        let (mut tx, rx) = rtrb::RingBuffer::new(1);

        handle_midi_message(&mut state, &learn, &mappings, &mut tx, [0xB0, 1, 0]);
        handle_midi_message(&mut state, &learn, &mappings, &mut tx, [0xB0, 1, 127]);
        // Parameter still applied even though the notification was dropped
        assert!((pu.control_inputs[0].value.load() - 1.0).abs() < 1e-6);
        assert_eq!(rx.slots(), 1);
    }

    #[test]
    fn note_off_releases_momentary() {
        let (entry, pu) = make_entry_typed(
            0.0,
            1.0,
            MappingMode::Momentary,
            false,
            MidiMessageType::Note,
        );
        let mappings = ResolvedMappings::new(vec![entry]);
        let mut state = MidiProcessingState::new();
        let learn = LearnFlags::new();
        let (mut tx, _rx) = rtrb::RingBuffer::new(8);

        handle_midi_message(&mut state, &learn, &mappings, &mut tx, [0x90, 1, 20]);
        assert_eq!(pu.control_inputs[0].value.load(), 1.0);
        // Note-off with a non-zero release velocity is still a release
        handle_midi_message(&mut state, &learn, &mappings, &mut tx, [0x80, 1, 100]);
        assert_eq!(pu.control_inputs[0].value.load(), 0.0);
    }

    #[test]
    fn learn_captures_once() {
        let mappings = ResolvedMappings::empty();
        let mut state = MidiProcessingState::new();
        let learn = LearnFlags::new();
        learn.set(true);
        let (mut tx, mut rx) = rtrb::RingBuffer::new(8);

        // Note-off is not learned
        handle_midi_message(&mut state, &learn, &mappings, &mut tx, [0x83, 60, 0]);
        assert!(rx.pop().is_err());
        handle_midi_message(&mut state, &learn, &mappings, &mut tx, [0x93, 60, 5]);
        handle_midi_message(&mut state, &learn, &mappings, &mut tx, [0xB0, 7, 5]);
        assert_eq!(
            rx.pop(),
            Ok(RtEvent::LearnCaptured {
                channel: 3,
                number: 60,
                message_type: MidiMessageType::Note
            })
        );
        assert!(rx.pop().is_err());
    }

    #[test]
    fn learn_retries_when_queue_full() {
        let mappings = ResolvedMappings::empty();
        let mut state = MidiProcessingState::new();
        let learn = LearnFlags::new();
        learn.set(true);
        let (mut tx, mut rx) = rtrb::RingBuffer::new(1);
        tx.push(RtEvent::ParameterChanged {
            instance_id: 0,
            port_index: 0,
            value: 0.0,
        })
        .unwrap();

        handle_midi_message(&mut state, &learn, &mappings, &mut tx, [0xB0, 7, 5]);
        assert!(!learn.captured.load(Ordering::SeqCst));
        rx.pop().unwrap();
        handle_midi_message(&mut state, &learn, &mappings, &mut tx, [0xB0, 7, 5]);
        assert!(matches!(
            rx.pop(),
            Ok(RtEvent::LearnCaptured { number: 7, .. })
        ));
    }

    // ---- MIDI sequence forwarding ----

    fn build_sequence(events: &[(u32, &[u8])]) -> Vec<u8> {
        let mut v = vec![0u8; 16];
        v[4..8].copy_from_slice(&(libspa::sys::SPA_TYPE_Sequence as u32).to_ne_bytes());
        for (offset, data) in events {
            let start = v.len();
            v.extend_from_slice(&offset.to_ne_bytes());
            v.extend_from_slice(&(libspa::sys::SPA_CONTROL_Midi as u32).to_ne_bytes());
            v.extend_from_slice(&(data.len() as u32).to_ne_bytes());
            v.extend_from_slice(&(libspa::sys::SPA_TYPE_Bytes as u32).to_ne_bytes());
            v.extend_from_slice(data);
            let padded = (v.len() - start + 7) & !7;
            v.resize(start + padded, 0);
        }
        let size = (v.len() - 8) as u32;
        v[0..4].copy_from_slice(&size.to_ne_bytes());
        v
    }

    #[test]
    fn copy_sequence_empty_input_clears_output() {
        let mut out = [0xAAu8; 64];
        let n = copy_sequence_bounded(&[], &mut out);
        assert_eq!(n, 16);
        assert_eq!(read_u32(&out, 0), 8);
        assert_eq!(read_u32(&out, 4), libspa::sys::SPA_TYPE_Sequence as u32);
    }

    #[test]
    fn copy_sequence_full_copy() {
        let input = build_sequence(&[(0, &[0x90, 60, 100]), (10, &[0x80, 60, 0])]);
        let mut out = vec![0u8; 256];
        let n = copy_sequence_bounded(&input, &mut out);
        assert_eq!(n, input.len());
        assert_eq!(&out[..n], &input[..]);
    }

    #[test]
    fn copy_sequence_truncates_to_capacity() {
        let input = build_sequence(&[(0, &[0x90, 60, 100]), (10, &[0x80, 60, 0])]);
        // Header (16) + one control (16 + 3 -> 24 padded) = 40 bytes
        let mut out = vec![0u8; 50];
        let n = copy_sequence_bounded(&input, &mut out);
        assert_eq!(n, 40);
        assert_eq!(read_u32(&out, 0), 32);
        assert_eq!(&out[16..40], &input[16..40]);
    }

    #[test]
    fn copy_sequence_tiny_output() {
        let input = build_sequence(&[(0, &[0x90, 60, 100])]);
        let mut out = [0u8; 8];
        assert_eq!(copy_sequence_bounded(&input, &mut out), 0);
    }

    #[test]
    fn copy_sequence_rejects_truncated_input_control() {
        let mut input = build_sequence(&[(0, &[0x90, 60, 100])]);
        // Claim a value size larger than the pod
        input[24..28].copy_from_slice(&100u32.to_ne_bytes());
        let mut out = vec![0u8; 256];
        assert_eq!(copy_sequence_bounded(&input, &mut out), 16);
    }

    // ---- Block chunking of MIDI events ----

    fn ev(offset: u32) -> RawMidiEvent {
        RawMidiEvent {
            offset,
            data: [0x90, 60, 100],
            size: 3,
        }
    }

    #[test]
    fn chunk_events_split_and_rebase() {
        let events = [ev(0), ev(8191), ev(8192), ev(10000), ev(20000)];
        let mut out = [RawMidiEvent::EMPTY; MAX_MIDI_EVENTS];

        let n = chunk_midi_events(&events, 0, 8192, 12000, &mut out);
        assert_eq!(n, 2);
        assert_eq!(out[0].offset, 0);
        assert_eq!(out[1].offset, 8191);

        let n = chunk_midi_events(&events, 8192, 12000 - 8192, 12000, &mut out);
        assert_eq!(n, 3);
        assert_eq!(out[0].offset, 0);
        assert_eq!(out[1].offset, 10000 - 8192);
        // Out-of-range offset is clamped to the last frame
        assert_eq!(out[2].offset, 12000 - 1 - 8192);
    }

    #[test]
    fn chunk_events_respects_output_capacity() {
        let events = [ev(0), ev(1), ev(2)];
        let mut out = [RawMidiEvent::EMPTY; 2];
        assert_eq!(chunk_midi_events(&events, 0, 16, 16, &mut out), 2);
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
