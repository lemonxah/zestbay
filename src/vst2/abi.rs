//! Minimal C ABI declarations for VST 2.4 interoperability.
//!
//! No Steinberg SDK headers are included. The public RustAudio bindings are
//! a reference for the binary layout and opcode numbers:
//! https://github.com/RustAudio/vst2-sys/blob/master/src/lib.rs
//! Nullable callbacks are represented with Option, including older plugins
//! which leave the VST 2.4 processing callbacks unset.

use std::ffi::c_void;

pub const MAGIC: i32 = 0x5673_7450;
pub const HAS_EDITOR: i32 = 1;
pub const PROGRAM_CHUNKS: i32 = 1 << 5;
pub const IS_SYNTH: i32 = 1 << 8;
pub const MAX_CHANNELS: i32 = 256;
pub const MAX_PARAMS: i32 = 16384;

pub type Callback = unsafe extern "C" fn(*mut Effect, i32, i32, isize, *mut c_void, f32) -> isize;
pub type Process = unsafe extern "C" fn(*mut Effect, *mut *mut f32, *mut *mut f32, i32);
pub type Entry = unsafe extern "C" fn(Callback) -> *mut Effect;

#[repr(C)]
pub struct Effect {
    pub magic: i32,
    pub dispatcher: Option<Callback>,
    pub process: Option<Process>,
    pub set_parameter: Option<unsafe extern "C" fn(*mut Effect, i32, f32)>,
    pub get_parameter: Option<unsafe extern "C" fn(*mut Effect, i32) -> f32>,
    pub num_programs: i32,
    pub num_params: i32,
    pub num_inputs: i32,
    pub num_outputs: i32,
    pub flags: i32,
    pub reserved1: isize,
    pub reserved2: isize,
    pub initial_delay: i32,
    pub quality: [i32; 2],
    pub io_ratio: f32,
    pub object: *mut c_void,
    pub user: *mut c_void,
    pub unique_id: i32,
    pub version: i32,
    pub process_replacing: Option<Process>,
    pub process_double_replacing:
        Option<unsafe extern "C" fn(*mut Effect, *mut *mut f64, *mut *mut f64, i32)>,
    pub future: [u8; 56],
}

pub mod eff {
    pub const OPEN: i32 = 0;
    pub const CLOSE: i32 = 1;
    pub const SET_PROGRAM: i32 = 2;
    pub const GET_PROGRAM: i32 = 3;
    pub const PARAM_NAME: i32 = 8;
    pub const SAMPLE_RATE: i32 = 10;
    pub const BLOCK_SIZE: i32 = 11;
    pub const MAINS: i32 = 12;
    pub const EDIT_RECT: i32 = 13;
    pub const EDIT_OPEN: i32 = 14;
    pub const EDIT_CLOSE: i32 = 15;
    pub const EDIT_IDLE: i32 = 19;
    pub const GET_CHUNK: i32 = 23;
    pub const SET_CHUNK: i32 = 24;
    pub const EVENTS: i32 = 25;
    pub const CATEGORY: i32 = 35;
    pub const NAME: i32 = 45;
    pub const VENDOR: i32 = 47;
    pub const PRODUCT: i32 = 48;
    pub const CAN_DO: i32 = 51;
    pub const PARAM_PROPERTIES: i32 = 56;
    pub const SHELL_NEXT: i32 = 70;
    pub const START: i32 = 71;
    pub const STOP: i32 = 72;
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct MidiEvent {
    pub kind: i32,
    pub byte_size: i32,
    pub delta_frames: i32,
    pub flags: i32,
    pub note_length: i32,
    pub note_offset: i32,
    pub data: [u8; 4],
    pub detune: i8,
    pub note_off_velocity: u8,
    pub reserved: [u8; 2],
}

/// VstEvents ends in a variable-length array of event pointers.
#[repr(C)]
pub struct Events<const N: usize> {
    pub count: i32,
    pub reserved: isize,
    pub events: [*const MidiEvent; N],
}

#[repr(C)]
#[derive(Default)]
pub struct Rect {
    pub top: i16,
    pub left: i16,
    pub bottom: i16,
    pub right: i16,
}

#[repr(C)]
#[derive(Default)]
pub struct TimeInfo {
    pub sample_pos: f64,
    pub sample_rate: f64,
    pub nanos: f64,
    pub ppq: f64,
    pub tempo: f64,
    pub bar_start: f64,
    pub cycle_start: f64,
    pub cycle_end: f64,
    pub signature_numerator: i32,
    pub signature_denominator: i32,
    pub smpte_offset: i32,
    pub smpte_rate: i32,
    pub samples_to_next_clock: i32,
    pub flags: i32,
}

#[repr(C)]
pub struct ParameterProperties {
    pub step: f32,
    pub small_step: f32,
    pub large_step: f32,
    pub label: [u8; 64],
    pub flags: i32,
    pub min_integer: i32,
    pub max_integer: i32,
    pub step_integer: i32,
    pub large_step_integer: i32,
    pub short_label: [u8; 8],
    pub display_index: i16,
    pub category: i16,
    pub category_count: i16,
    pub reserved: i16,
    pub category_label: [u8; 24],
    pub future: [u8; 16],
}

/// The instance must remain live and this call must obey its thread rules.
pub unsafe fn dispatch(
    effect: *mut Effect,
    opcode: i32,
    index: i32,
    value: isize,
    ptr: *mut c_void,
    opt: f32,
) -> isize {
    unsafe {
        (*effect)
            .dispatcher
            .map_or(0, |f| f(effect, opcode, index, value, ptr, opt))
    }
}

pub unsafe fn command(effect: *mut Effect, opcode: i32, value: isize) -> isize {
    unsafe { dispatch(effect, opcode, 0, value, std::ptr::null_mut(), 0.0) }
}

pub unsafe fn text(effect: *mut Effect, opcode: i32, index: i32) -> String {
    let mut buf = [0u8; 256];
    unsafe { dispatch(effect, opcode, index, 0, buf.as_mut_ptr().cast(), 0.0) };
    let len = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..len]).trim().to_owned()
}
