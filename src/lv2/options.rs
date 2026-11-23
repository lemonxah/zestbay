use std::ffi::{CStr, c_void};
use std::sync::Arc;

use super::urid::UridMapper;

pub const LV2_OPTIONS__OPTIONS: &CStr = c"http://lv2plug.in/ns/ext/options#options";
pub const LV2_BUF_SIZE__BOUNDED_BLOCK_LENGTH: &CStr =
    c"http://lv2plug.in/ns/ext/buf-size#boundedBlockLength";

pub const MAX_BLOCK_LENGTH: u32 = 8192;
pub const MIN_BLOCK_LENGTH: u32 = 1;
pub const SEQUENCE_SIZE: usize = 65536;

pub const LV2_OPTIONS_INSTANCE: u32 = 0;

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

unsafe impl Send for LV2_Options_Option {}
unsafe impl Sync for LV2_Options_Option {}

pub struct Lv2OptionsSetup {
    options: Vec<LV2_Options_Option>,
    _block_length: Box<i32>,
    _min_block_length: Box<i32>,
    _max_block_length: Box<i32>,
    _sample_rate: Box<f32>,
    _sequence_size: Box<i32>,
    _urid_mapper: Arc<UridMapper>,
}

unsafe impl Send for Lv2OptionsSetup {}

impl Lv2OptionsSetup {
    pub fn new(urid_mapper: &Arc<UridMapper>, sample_rate: f64, block_length: u32) -> Self {
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

        let nominal = block_length.clamp(MIN_BLOCK_LENGTH, MAX_BLOCK_LENGTH);
        let block_length_val = Box::new(nominal as i32);
        let min_block_length_val = Box::new(MIN_BLOCK_LENGTH as i32);
        let max_block_length_val = Box::new(MAX_BLOCK_LENGTH as i32);
        let sample_rate_val = Box::new(sample_rate as f32);
        let sequence_size_val = Box::new(SEQUENCE_SIZE as i32);

        let options = vec![
            LV2_Options_Option {
                context: LV2_OPTIONS_INSTANCE,
                subject: 0,
                key: nominal_block_length_key,
                size: std::mem::size_of::<i32>() as u32,
                type_: atom_int_type,
                value: &*block_length_val as *const i32 as *const c_void,
            },
            LV2_Options_Option {
                context: LV2_OPTIONS_INSTANCE,
                subject: 0,
                key: max_block_length_key,
                size: std::mem::size_of::<i32>() as u32,
                type_: atom_int_type,
                value: &*max_block_length_val as *const i32 as *const c_void,
            },
            LV2_Options_Option {
                context: LV2_OPTIONS_INSTANCE,
                subject: 0,
                key: min_block_length_key,
                size: std::mem::size_of::<i32>() as u32,
                type_: atom_int_type,
                value: &*min_block_length_val as *const i32 as *const c_void,
            },
            LV2_Options_Option {
                context: LV2_OPTIONS_INSTANCE,
                subject: 0,
                key: sequence_size_key,
                size: std::mem::size_of::<i32>() as u32,
                type_: atom_int_type,
                value: &*sequence_size_val as *const i32 as *const c_void,
            },
            LV2_Options_Option {
                context: LV2_OPTIONS_INSTANCE,
                subject: 0,
                key: sample_rate_key,
                size: std::mem::size_of::<f32>() as u32,
                type_: atom_float_type,
                value: &*sample_rate_val as *const f32 as *const c_void,
            },
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

    pub fn make_feature(&self) -> lv2_raw::core::LV2Feature {
        lv2_raw::core::LV2Feature {
            uri: LV2_OPTIONS__OPTIONS.as_ptr(),
            data: self.options.as_ptr() as *mut c_void,
        }
    }

    pub fn make_bounded_block_length_feature(&self) -> lv2_raw::core::LV2Feature {
        lv2_raw::core::LV2Feature {
            uri: LV2_BUF_SIZE__BOUNDED_BLOCK_LENGTH.as_ptr(),
            data: std::ptr::null_mut(),
        }
    }

    pub fn make_buf_size_features(&self) -> Vec<lv2_raw::core::LV2Feature> {
        vec![self.make_bounded_block_length_feature()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int_option(setup: &Lv2OptionsSetup, mapper: &UridMapper, uri: &str) -> i32 {
        let key = mapper.map(uri);
        let opt = setup
            .options
            .iter()
            .find(|o| o.key == key)
            .expect("option present");
        unsafe { *(opt.value as *const i32) }
    }

    #[test]
    fn block_length_bounds() {
        let mapper = Arc::new(UridMapper::new());
        let setup = Lv2OptionsSetup::new(&mapper, 48000.0, 256);
        assert_eq!(
            int_option(&setup, &mapper, "http://lv2plug.in/ns/ext/buf-size#nominalBlockLength"),
            256
        );
        assert_eq!(
            int_option(&setup, &mapper, "http://lv2plug.in/ns/ext/buf-size#minBlockLength"),
            1
        );
        assert_eq!(
            int_option(&setup, &mapper, "http://lv2plug.in/ns/ext/buf-size#maxBlockLength"),
            8192
        );
        assert_eq!(setup.options.last().unwrap().key, 0);
    }

    #[test]
    fn no_fixed_or_power_of_two_feature() {
        let mapper = Arc::new(UridMapper::new());
        let setup = Lv2OptionsSetup::new(&mapper, 48000.0, 1024);
        let features = setup.make_buf_size_features();
        assert_eq!(features.len(), 1);
        let uri = unsafe { CStr::from_ptr(features[0].uri) };
        assert_eq!(uri, LV2_BUF_SIZE__BOUNDED_BLOCK_LENGTH);
    }
}
