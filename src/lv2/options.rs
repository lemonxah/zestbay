//! LV2 Options and Buf-Size extension implementation.
//!
//! Provides `LV2_Options_Option` and `Lv2OptionsSetup` so that plugins
//! receive sample rate, block length, and sequence size at instantiation
//! time.  Also provides buf-size feature URIs (`boundedBlockLength`,
//! `fixedBlockLength`) that plugins can check for host capability
//! discovery.
//!
//! Two features are produced:
//!   - `options#options` — an array of `LV2_Options_Option` with the
//!     actual values (sample rate, block lengths, sequence size)
//!   - Buf-size features (`boundedBlockLength`, `fixedBlockLength`) —
//!     data-less features that advertise host guarantees about block sizes

use std::ffi::{CStr, c_void};
use std::sync::Arc;

use super::urid::UridMapper;

// ── URI constants ──

pub const LV2_OPTIONS__OPTIONS: &CStr = c"http://lv2plug.in/ns/ext/options#options";
pub const LV2_BUF_SIZE__BOUNDED_BLOCK_LENGTH: &CStr =
    c"http://lv2plug.in/ns/ext/buf-size#boundedBlockLength";
pub const LV2_BUF_SIZE__FIXED_BLOCK_LENGTH: &CStr =
    c"http://lv2plug.in/ns/ext/buf-size#fixedBlockLength";

// ── Option context values ──

pub const LV2_OPTIONS_INSTANCE: u32 = 0;
pub const LV2_OPTIONS_RESOURCE: u32 = 1;
pub const LV2_OPTIONS_BLANK: u32 = 2;
pub const LV2_OPTIONS_PORT: u32 = 3;

// ── C-compatible option struct ──

/// A single LV2 option, matching the C `LV2_Options_Option` layout.
/// The options array passed to the plugin is NULL-terminated: the last
/// element has `key = 0` and `value = null`.
#[repr(C)]
#[derive(Clone, Copy)]
#[allow(non_camel_case_types)]
pub struct LV2_Options_Option {
    pub context: u32,
    pub subject: u32,
    pub key: u32,
    pub size: u32,
    pub type_: u32,
    pub value: *const c_void,
}

// SAFETY: The value pointer points to heap-allocated data owned by
// Lv2OptionsSetup.  The struct is only read by the plugin from the
// same thread that holds the setup, and the data lifetime is tied to
// the setup struct.
unsafe impl Send for LV2_Options_Option {}
unsafe impl Sync for LV2_Options_Option {}

// ── Setup struct ──

/// Holds the options array and the stable heap-allocated values needed
/// BEFORE plugin instantiation.  Call `make_feature()` to get the
/// `LV2Feature` to pass to `plugin.instantiate()`.
pub struct Lv2OptionsSetup {
    /// The options array (NULL-terminated).
    options: Vec<LV2_Options_Option>,
    /// Stable heap storage for the block length value (i32).
    _block_length: Box<i32>,
    /// Stable heap storage for the min block length value (i32).
    _min_block_length: Box<i32>,
    /// Stable heap storage for the max block length value (i32).
    _max_block_length: Box<i32>,
    /// Stable heap storage for the sample rate value (f32).
    _sample_rate: Box<f32>,
    /// Stable heap storage for the sequence size value (i32).
    _sequence_size: Box<i32>,
    /// Keep the mapper alive so URIDs remain valid.
    _urid_mapper: Arc<UridMapper>,
}

// SAFETY: All raw pointers in the options Vec point to Box-owned heap
// data that lives as long as this struct.
unsafe impl Send for Lv2OptionsSetup {}

impl Lv2OptionsSetup {
    /// Create a new options setup with the given parameters.
    ///
    /// `urid_mapper` — shared URID mapper for resolving option URIs.
    /// `sample_rate` — the host sample rate (e.g. 48000.0).
    /// `block_length` — the nominal/fixed block size (e.g. 1024).
    pub fn new(urid_mapper: &Arc<UridMapper>, sample_rate: f64, block_length: u32) -> Self {
        // Map all the URIDs we need
        let nominal_block_length_key =
            urid_mapper.map("http://lv2plug.in/ns/ext/buf-size#nominalBlockLength");
        let max_block_length_key =
            urid_mapper.map("http://lv2plug.in/ns/ext/buf-size#maxBlockLength");
        let min_block_length_key =
            urid_mapper.map("http://lv2plug.in/ns/ext/buf-size#minBlockLength");
        let sequence_size_key =
            urid_mapper.map("http://lv2plug.in/ns/ext/buf-size#sequenceSize");
        let sample_rate_key =
            urid_mapper.map("http://lv2plug.in/ns/ext/parameters#sampleRate");

        let atom_int_type = urid_mapper.map("http://lv2plug.in/ns/ext/atom#Int");
        let atom_float_type = urid_mapper.map("http://lv2plug.in/ns/ext/atom#Float");

        // Allocate values on the heap so pointers remain stable
        let block_length_val = Box::new(block_length as i32);
        let min_block_length_val = Box::new(block_length as i32);
        let max_block_length_val = Box::new(block_length as i32);
        let sample_rate_val = Box::new(sample_rate as f32);
        let sequence_size_val = Box::new(65536_i32);

        let options = vec![
            // nominalBlockLength — Int
            LV2_Options_Option {
                context: LV2_OPTIONS_INSTANCE,
                subject: 0,
                key: nominal_block_length_key,
                size: std::mem::size_of::<i32>() as u32,
                type_: atom_int_type,
                value: &*block_length_val as *const i32 as *const c_void,
            },
            // maxBlockLength — Int
            LV2_Options_Option {
                context: LV2_OPTIONS_INSTANCE,
                subject: 0,
                key: max_block_length_key,
                size: std::mem::size_of::<i32>() as u32,
                type_: atom_int_type,
                value: &*max_block_length_val as *const i32 as *const c_void,
            },
            // minBlockLength — Int
            LV2_Options_Option {
                context: LV2_OPTIONS_INSTANCE,
                subject: 0,
                key: min_block_length_key,
                size: std::mem::size_of::<i32>() as u32,
                type_: atom_int_type,
                value: &*min_block_length_val as *const i32 as *const c_void,
            },
            // sequenceSize — Int
            LV2_Options_Option {
                context: LV2_OPTIONS_INSTANCE,
                subject: 0,
                key: sequence_size_key,
                size: std::mem::size_of::<i32>() as u32,
                type_: atom_int_type,
                value: &*sequence_size_val as *const i32 as *const c_void,
            },
            // sampleRate — Float
            LV2_Options_Option {
                context: LV2_OPTIONS_INSTANCE,
                subject: 0,
                key: sample_rate_key,
                size: std::mem::size_of::<f32>() as u32,
                type_: atom_float_type,
                value: &*sample_rate_val as *const f32 as *const c_void,
            },
            // NULL terminator
            LV2_Options_Option {
                context: 0,
                subject: 0,
                key: 0,
                size: 0,
                type_: 0,
                value: std::ptr::null(),
            },
        ];

        Lv2OptionsSetup {
            options,
            _block_length: block_length_val,
            _min_block_length: min_block_length_val,
            _max_block_length: max_block_length_val,
            _sample_rate: sample_rate_val,
            _sequence_size: sequence_size_val,
            _urid_mapper: urid_mapper.clone(),
        }
    }

    /// Build an `LV2Feature` for the options extension.
    /// The feature data points to the first element of the options array.
    pub fn make_feature(&self) -> lv2_raw::core::LV2Feature {
        lv2_raw::core::LV2Feature {
            uri: LV2_OPTIONS__OPTIONS.as_ptr(),
            data: self.options.as_ptr() as *mut c_void,
        }
    }

    /// Build an `LV2Feature` for `buf-size#boundedBlockLength`.
    /// This is a data-less feature: the host guarantees that the block
    /// size will never exceed `maxBlockLength`.
    pub fn make_bounded_block_length_feature(&self) -> lv2_raw::core::LV2Feature {
        lv2_raw::core::LV2Feature {
            uri: LV2_BUF_SIZE__BOUNDED_BLOCK_LENGTH.as_ptr(),
            data: std::ptr::null_mut(),
        }
    }

    /// Build an `LV2Feature` for `buf-size#fixedBlockLength`.
    /// This is a data-less feature: the host guarantees that the block
    /// size is always exactly `nominalBlockLength` (PipeWire default).
    pub fn make_fixed_block_length_feature(&self) -> lv2_raw::core::LV2Feature {
        lv2_raw::core::LV2Feature {
            uri: LV2_BUF_SIZE__FIXED_BLOCK_LENGTH.as_ptr(),
            data: std::ptr::null_mut(),
        }
    }

    /// Convenience: return all buf-size features as a Vec.
    /// These are data-less features that plugins check for host capability.
    pub fn make_buf_size_features(&self) -> Vec<lv2_raw::core::LV2Feature> {
        vec![
            self.make_bounded_block_length_feature(),
            self.make_fixed_block_length_feature(),
        ]
    }
}
