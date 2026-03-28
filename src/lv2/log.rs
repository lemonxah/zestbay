//! LV2 Log extension implementation.
//!
//! Provides the `LV2_Log_Log` feature so plugins can emit log messages
//! that are routed to Rust's `log` crate (env_logger).  The host maps
//! LV2 log‐type URIDs to the appropriate `log` level:
//!
//! - `log#Error`   → `log::error!`
//! - `log#Warning` → `log::warn!`
//! - `log#Note`    → `log::info!`
//! - `log#Trace`   → `log::trace!`

use std::ffi::{CStr, c_char, c_void};
use std::sync::Arc;

use super::urid::UridMapper;

unsafe extern "C" {
    fn vsnprintf(
        s: *mut c_char,
        n: usize,
        format: *const c_char,
        ap: *mut c_void,
    ) -> i32;
}

pub const LV2_LOG_LOG_URI: &CStr = c"http://lv2plug.in/ns/ext/log#log";

const LOG_ERROR_URI: &str = "http://lv2plug.in/ns/ext/log#Error";
const LOG_WARNING_URI: &str = "http://lv2plug.in/ns/ext/log#Warning";
const LOG_NOTE_URI: &str = "http://lv2plug.in/ns/ext/log#Note";
const LOG_TRACE_URI: &str = "http://lv2plug.in/ns/ext/log#Trace";

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct LV2_Log_Log {
    pub handle: *mut c_void,
    pub printf: Option<
        unsafe extern "C" fn(handle: *mut c_void, type_: u32, fmt: *const c_char, ...) -> i32,
    >,
    pub vprintf: Option<
        unsafe extern "C" fn(
            handle: *mut c_void,
            type_: u32,
            fmt: *const c_char,
            ap: *mut c_void,
        ) -> i32,
    >,
}

struct LogContext {
    mapper: Arc<UridMapper>,
    urid_error: u32,
    urid_warning: u32,
    urid_note: u32,
    urid_trace: u32,
}

pub struct Lv2LogSetup {
    log_struct: Box<LV2_Log_Log>,
    log_ctx_ptr: *mut LogContext,
}

// SAFETY: log_ctx_ptr is heap-allocated in new(), only dereferenced in the
// vprintf callback, and reclaimed in Drop.
unsafe impl Send for Lv2LogSetup {}

impl Lv2LogSetup {
    pub fn new(mapper: &Arc<UridMapper>) -> Self {
        let ctx = Box::new(LogContext {
            mapper: Arc::clone(mapper),
            urid_error: mapper.map(LOG_ERROR_URI),
            urid_warning: mapper.map(LOG_WARNING_URI),
            urid_note: mapper.map(LOG_NOTE_URI),
            urid_trace: mapper.map(LOG_TRACE_URI),
        });
        let ctx_ptr = Box::into_raw(ctx);

        let log_struct = Box::new(LV2_Log_Log {
            handle: ctx_ptr as *mut c_void,
            printf: None,
            vprintf: Some(log_vprintf_callback),
        });

        Lv2LogSetup {
            log_struct,
            log_ctx_ptr: ctx_ptr,
        }
    }

    pub fn make_feature(&self) -> lv2_raw::core::LV2Feature {
        lv2_raw::core::LV2Feature {
            uri: LV2_LOG_LOG_URI.as_ptr(),
            data: &*self.log_struct as *const LV2_Log_Log as *mut c_void,
        }
    }
}

impl Drop for Lv2LogSetup {
    fn drop(&mut self) {
        if !self.log_ctx_ptr.is_null() {
            unsafe {
                drop(Box::from_raw(self.log_ctx_ptr));
            }
            self.log_ctx_ptr = std::ptr::null_mut();
        }
    }
}

/// # Safety
/// `ap` must be a valid C `va_list` pointer from the calling plugin.
unsafe extern "C" fn log_vprintf_callback(
    handle: *mut c_void,
    type_: u32,
    fmt: *const c_char,
    ap: *mut c_void,
) -> i32 {
    if handle.is_null() || fmt.is_null() {
        return -1;
    }

    let ctx = unsafe { &*(handle as *const LogContext) };

    // Single-pass with fixed buffer — va_list is consumed by one vsnprintf call.
    let mut buf = [0u8; 2048];
    let written = unsafe {
        vsnprintf(
            buf.as_mut_ptr() as *mut c_char,
            buf.len(),
            fmt,
            ap,
        )
    };

    let msg = if written >= 0 {
        let actual_len = std::cmp::min(written as usize, buf.len() - 1);
        String::from_utf8_lossy(&buf[..actual_len])
    } else {
        unsafe { CStr::from_ptr(fmt) }.to_string_lossy()
    };

    if type_ == ctx.urid_error {
        log::error!("[LV2] {}", msg);
    } else if type_ == ctx.urid_warning {
        log::warn!("[LV2] {}", msg);
    } else if type_ == ctx.urid_note {
        log::info!("[LV2] {}", msg);
    } else if type_ == ctx.urid_trace {
        log::trace!("[LV2] {}", msg);
    } else {
        let uri = ctx.mapper.unmap(type_).unwrap_or_else(|| format!("URID:{}", type_));
        log::debug!("[LV2] [{}] {}", uri, msg);
    }

    written
}
