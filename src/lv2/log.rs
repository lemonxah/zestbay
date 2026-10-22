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
//!
//! Messages logged while the calling thread is inside the plugin's `run()`
//! (see [`RtScope`]) are formatted into a stack buffer and pushed into a
//! lock-free ring; a background thread drains the ring and does the actual
//! logging, so the RT thread never allocates, locks or does I/O.

use std::cell::Cell;
use std::ffi::{CStr, c_char, c_void};
use std::sync::{Arc, OnceLock};

use super::urid::UridMapper;
use crate::plugin::types::{MsgConsumer, MsgPop, MsgProducer, msg_ring};

// `va_list` ABI: on x86_64 SysV it is an array type, so a `va_list` parameter
// is passed as a pointer; on aarch64 Linux (AAPCS64) it is a 32-byte struct,
// which is passed by reference to a caller-made copy.  In both cases the
// callee receives a single pointer, and forwarding that pointer to
// `vsnprintf` is ABI-correct.  On other architectures we do not interpret
// the argument list and log the raw format string instead.
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
unsafe extern "C" {
    fn vsnprintf(s: *mut c_char, n: usize, format: *const c_char, ap: *mut c_void) -> i32;
}

pub const LV2_LOG_LOG_URI: &CStr = c"http://lv2plug.in/ns/ext/log#log";

const LOG_ERROR_URI: &str = "http://lv2plug.in/ns/ext/log#Error";
const LOG_WARNING_URI: &str = "http://lv2plug.in/ns/ext/log#Warning";
const LOG_NOTE_URI: &str = "http://lv2plug.in/ns/ext/log#Note";
const LOG_TRACE_URI: &str = "http://lv2plug.in/ns/ext/log#Trace";

const MSG_BUF_SIZE: usize = 1024;
const RT_LOG_RING_CAPACITY: usize = 64 * 1024;

type PrintfFn = unsafe extern "C" fn(handle: *mut c_void, type_: u32, fmt: *const c_char, ...) -> i32;
type PrintfPrefixFn = unsafe extern "C" fn(handle: *mut c_void, type_: u32, fmt: *const c_char) -> i32;

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct LV2_Log_Log {
    pub handle: *mut c_void,
    pub printf: Option<PrintfFn>,
    pub vprintf: Option<
        unsafe extern "C" fn(
            handle: *mut c_void,
            type_: u32,
            fmt: *const c_char,
            ap: *mut c_void,
        ) -> i32,
    >,
}

thread_local! {
    static IN_RT: Cell<bool> = const { Cell::new(false) };
}

/// Marks the current thread as running plugin RT code (`run()`, worker
/// responses) for as long as the guard lives.  Log calls made inside the
/// scope are deferred to the background log thread.
pub struct RtScope {
    prev: bool,
}

impl RtScope {
    pub fn enter() -> Self {
        let prev = IN_RT.with(|f| f.replace(true));
        RtScope { prev }
    }
}

impl Drop for RtScope {
    fn drop(&mut self) {
        IN_RT.with(|f| f.set(self.prev));
    }
}

fn in_rt() -> bool {
    IN_RT.with(|f| f.get())
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum Level {
    Error = 0,
    Warn = 1,
    Info = 2,
    Trace = 3,
    Debug = 4,
}

impl Level {
    fn from_u8(v: u8) -> Self {
        match v {
            0 => Level::Error,
            1 => Level::Warn,
            2 => Level::Info,
            3 => Level::Trace,
            _ => Level::Debug,
        }
    }
}

fn emit(level: Level, msg: &str) {
    match level {
        Level::Error => log::error!("[LV2] {}", msg),
        Level::Warn => log::warn!("[LV2] {}", msg),
        Level::Info => log::info!("[LV2] {}", msg),
        Level::Trace => log::trace!("[LV2] {}", msg),
        Level::Debug => log::debug!("[LV2] {}", msg),
    }
}

/// Process-wide ring for messages logged from RT context.
struct RtLogRing {
    producer: parking_lot::Mutex<MsgProducer>,
}

static RT_LOG_RING: OnceLock<RtLogRing> = OnceLock::new();

fn rt_log_ring() -> &'static RtLogRing {
    RT_LOG_RING.get_or_init(|| {
        let (producer, consumer) = msg_ring(RT_LOG_RING_CAPACITY);
        if let Err(e) = std::thread::Builder::new()
            .name("lv2-log".into())
            .spawn(move || rt_log_drain_thread(consumer))
        {
            log::error!("Failed to spawn LV2 log thread: {}", e);
        }
        RtLogRing {
            producer: parking_lot::Mutex::new(producer),
        }
    })
}

fn rt_log_drain_thread(mut consumer: MsgConsumer) {
    let mut buf = vec![0u8; MSG_BUF_SIZE + 1];
    loop {
        loop {
            match consumer.pop_into(&mut buf) {
                MsgPop::Data(len) if len >= 1 => {
                    let level = Level::from_u8(buf[0]);
                    emit(level, &String::from_utf8_lossy(&buf[1..len]));
                }
                MsgPop::Data(_) => {}
                MsgPop::TooLarge(_) => {
                    consumer.discard();
                }
                MsgPop::Empty => break,
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// RT-safe: copies the message into the ring, dropping it if the ring is
/// full or momentarily contended.
fn push_rt(level: Level, msg: &[u8]) {
    let Some(ring) = RT_LOG_RING.get() else {
        return;
    };
    if let Some(mut p) = ring.producer.try_lock() {
        let msg = &msg[..msg.len().min(MSG_BUF_SIZE)];
        let _ = p.push_parts(&[&[level as u8], msg]);
    }
}

struct LogContext {
    mapper: Arc<UridMapper>,
    urid_error: u32,
    urid_warning: u32,
    urid_note: u32,
    urid_trace: u32,
}

impl LogContext {
    fn level(&self, type_: u32) -> Level {
        if type_ == self.urid_error {
            Level::Error
        } else if type_ == self.urid_warning {
            Level::Warn
        } else if type_ == self.urid_note {
            Level::Info
        } else if type_ == self.urid_trace {
            Level::Trace
        } else {
            Level::Debug
        }
    }

    fn log(&self, type_: u32, msg: &[u8]) {
        let level = self.level(type_);
        if in_rt() {
            push_rt(level, msg);
            return;
        }
        let text = String::from_utf8_lossy(msg);
        if matches!(level, Level::Debug) {
            let uri = self
                .mapper
                .unmap(type_)
                .unwrap_or_else(|| format!("URID:{}", type_));
            log::debug!("[LV2] [{}] {}", uri, text);
        } else {
            emit(level, &text);
        }
    }
}

pub struct Lv2LogSetup {
    log_struct: Box<LV2_Log_Log>,
    log_ctx_ptr: *mut LogContext,
}

// SAFETY: log_ctx_ptr is heap-allocated in new(), only dereferenced in the
// log callbacks, and reclaimed in Drop.
unsafe impl Send for Lv2LogSetup {}

impl Lv2LogSetup {
    pub fn new(mapper: &Arc<UridMapper>) -> Self {
        // Make sure the RT drain thread exists before any plugin can log
        // from run().
        rt_log_ring();

        let ctx = Box::new(LogContext {
            mapper: Arc::clone(mapper),
            urid_error: mapper.map(LOG_ERROR_URI),
            urid_warning: mapper.map(LOG_WARNING_URI),
            urid_note: mapper.map(LOG_NOTE_URI),
            urid_trace: mapper.map(LOG_TRACE_URI),
        });
        let ctx_ptr = Box::into_raw(ctx);

        // Stable Rust cannot define C-variadic functions.  `printf` points at
        // a function taking only the fixed (handle, type, fmt) prefix.  On the
        // supported SysV/AAPCS64 targets the fixed arguments of a variadic
        // call are passed exactly like a non-variadic call, and the callee
        // never touches the variadic part, so this is sound in practice.  The
        // format string is logged without argument substitution; plugins using
        // the standard lv2_log_* helpers go through `vprintf` and get full
        // formatting.
        let printf: PrintfFn = unsafe {
            std::mem::transmute::<PrintfPrefixFn, PrintfFn>(log_printf_callback)
        };

        let log_struct = Box::new(LV2_Log_Log {
            handle: ctx_ptr as *mut c_void,
            printf: Some(printf),
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

/// `printf` entry point: logs the raw format string (arguments are not
/// substituted, see `Lv2LogSetup::new`).
unsafe extern "C" fn log_printf_callback(
    handle: *mut c_void,
    type_: u32,
    fmt: *const c_char,
) -> i32 {
    if handle.is_null() || fmt.is_null() {
        return -1;
    }
    let ctx = unsafe { &*(handle as *const LogContext) };
    let bytes = unsafe { CStr::from_ptr(fmt) }.to_bytes();
    let bytes = &bytes[..bytes.len().min(MSG_BUF_SIZE)];
    ctx.log(type_, trim_newline(bytes));
    bytes.len() as i32
}

/// # Safety
/// `ap` must be the `va_list` argument as received from the calling plugin.
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

    // Single pass into a fixed stack buffer: no allocation, RT-safe.
    let mut buf = [0u8; MSG_BUF_SIZE];
    let written = unsafe { format_va(&mut buf, fmt, ap) };

    if written >= 0 {
        let len = (written as usize).min(buf.len() - 1);
        ctx.log(type_, trim_newline(&buf[..len]));
    } else {
        let bytes = unsafe { CStr::from_ptr(fmt) }.to_bytes();
        ctx.log(type_, trim_newline(&bytes[..bytes.len().min(MSG_BUF_SIZE)]));
    }

    written
}

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
unsafe fn format_va(buf: &mut [u8], fmt: *const c_char, ap: *mut c_void) -> i32 {
    unsafe { vsnprintf(buf.as_mut_ptr() as *mut c_char, buf.len(), fmt, ap) }
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
unsafe fn format_va(buf: &mut [u8], fmt: *const c_char, _ap: *mut c_void) -> i32 {
    let bytes = unsafe { CStr::from_ptr(fmt) }.to_bytes();
    let n = bytes.len().min(buf.len() - 1);
    buf[..n].copy_from_slice(&bytes[..n]);
    n as i32
}

fn trim_newline(msg: &[u8]) -> &[u8] {
    let mut end = msg.len();
    while end > 0 && (msg[end - 1] == b'\n' || msg[end - 1] == b'\r') {
        end -= 1;
    }
    &msg[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rt_scope_nests() {
        assert!(!in_rt());
        {
            let _a = RtScope::enter();
            assert!(in_rt());
            {
                let _b = RtScope::enter();
                assert!(in_rt());
            }
            assert!(in_rt());
        }
        assert!(!in_rt());
    }

    #[test]
    fn trim_newline_strips_trailing() {
        assert_eq!(trim_newline(b"abc\n"), b"abc");
        assert_eq!(trim_newline(b"abc\r\n\n"), b"abc");
        assert_eq!(trim_newline(b"abc"), b"abc");
        assert_eq!(trim_newline(b"\n"), b"");
    }

    #[test]
    fn printf_callback_logs_raw_format() {
        let mapper = Arc::new(UridMapper::new());
        let setup = Lv2LogSetup::new(&mapper);
        let printf = setup.log_struct.printf.unwrap();
        let note = mapper.map(LOG_NOTE_URI);
        let n = unsafe { printf(setup.log_struct.handle, note, c"hello %d\n".as_ptr(), 5i32) };
        assert_eq!(n, 9);
    }
}
