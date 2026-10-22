//! LV2 Worker extension implementation.
//!
//! Provides `LV2_Worker_Schedule` feature so plugins can offload
//! non-realtime work to a background thread.  The RT thread calls
//! `schedule_work` which copies the request into a preallocated lock-free
//! ring and wakes the worker thread; the worker calls the plugin's `work()`,
//! and responses travel back through a second ring for delivery after the
//! next `run()` via `work_response()`.  Nothing on the RT side allocates,
//! frees or blocks.
//!
//! Two-phase construction:
//!   1. `Lv2WorkerSetup::new()` — creates rings + schedule feature (pre-instantiation)
//!   2. `Lv2WorkerSetup::activate(handle, iface)` → `Lv2Worker` (post-instantiation)
//!
//! The owner must call `Lv2Worker::stop()` (joins the thread) before the
//! plugin instance is freed; dropping the worker also stops it.

use std::ffi::{CStr, c_void};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::plugin::types::{MsgConsumer, MsgPop, MsgProducer, msg_ring};

// ── C-compatible struct definitions matching lv2/worker/worker.h ──

pub const LV2_WORKER_SCHEDULE_URI: &CStr = c"http://lv2plug.in/ns/ext/worker#schedule";
pub const LV2_WORKER_INTERFACE_URI: &str = "http://lv2plug.in/ns/ext/worker#interface";

#[allow(non_camel_case_types)]
pub type LV2_Worker_Status = u32;
pub const LV2_WORKER_SUCCESS: LV2_Worker_Status = 0;
pub const LV2_WORKER_ERR_UNKNOWN: LV2_Worker_Status = 1;
pub const LV2_WORKER_ERR_NO_SPACE: LV2_Worker_Status = 2;

/// Capacity of the request and response rings, in bytes.
const RING_CAPACITY: usize = 256 * 1024;
/// Largest single response delivered to the plugin.
const MAX_RESPONSE_SIZE: usize = 64 * 1024;

#[allow(non_camel_case_types)]
pub type LV2_Worker_Respond_Handle = *mut c_void;
#[allow(non_camel_case_types)]
pub type LV2_Worker_Respond_Function = unsafe extern "C" fn(
    handle: LV2_Worker_Respond_Handle,
    size: u32,
    data: *const c_void,
) -> LV2_Worker_Status;

#[allow(non_camel_case_types)]
pub type LV2_Worker_Schedule_Handle = *mut c_void;

/// The host-provided feature struct passed to the plugin.
#[repr(C)]
#[allow(non_camel_case_types)]
pub struct LV2_Worker_Schedule {
    pub handle: LV2_Worker_Schedule_Handle,
    pub schedule_work: unsafe extern "C" fn(
        handle: LV2_Worker_Schedule_Handle,
        size: u32,
        data: *const c_void,
    ) -> LV2_Worker_Status,
}

/// Plugin-provided interface retrieved via extension_data.
#[repr(C)]
#[allow(non_camel_case_types)]
pub struct LV2_Worker_Interface {
    pub work: unsafe extern "C" fn(
        instance: *mut c_void, // LV2_Handle
        respond: LV2_Worker_Respond_Function,
        handle: LV2_Worker_Respond_Handle,
        size: u32,
        data: *const c_void,
    ) -> LV2_Worker_Status,
    pub work_response: unsafe extern "C" fn(
        instance: *mut c_void, // LV2_Handle
        size: u32,
        body: *const c_void,
    ) -> LV2_Worker_Status,
    pub end_run: Option<unsafe extern "C" fn(instance: *mut c_void) -> LV2_Worker_Status>,
}

// ── Context structs ──

/// Context passed as the `handle` in the `LV2_Worker_Schedule` C struct.
/// Lives on the heap (stable pointer) until the `Lv2Worker` is dropped,
/// which happens after the plugin instance has been freed.
struct ScheduleContext {
    /// The plugin normally schedules from `run()`, but some also schedule
    /// from `restore()`/`instantiate()` on another thread.  `try_lock` keeps
    /// the RT path non-blocking; contention reports `NO_SPACE`.
    request_tx: parking_lot::Mutex<MsgProducer>,
    /// Worker thread to wake (set on activation).
    worker_thread: parking_lot::Mutex<Option<std::thread::Thread>>,
}

/// Context passed as the `LV2_Worker_Respond_Handle` to the `work()` callback.
/// Lives on the worker thread's stack; only the worker thread produces responses.
struct RespondContext {
    response_tx: MsgProducer,
}

// ── Phase 1: Pre-instantiation setup ──

/// Holds the rings and schedule feature needed BEFORE plugin instantiation.
/// Call `make_feature()` to get the `LV2Feature` to pass to `plugin.instantiate()`,
/// then call `activate()` after instantiation to start the worker thread.
pub struct Lv2WorkerSetup {
    request_rx: MsgConsumer,
    response_tx: MsgProducer,
    response_rx: MsgConsumer,
    /// Heap-allocated schedule struct (stable pointer for the plugin)
    schedule: Box<LV2_Worker_Schedule>,
    sched_ctx: Box<ScheduleContext>,
}

// SAFETY: the raw pointer inside `schedule` points into `sched_ctx`, which
// moves with the setup.
unsafe impl Send for Lv2WorkerSetup {}

impl Lv2WorkerSetup {
    /// Create a new worker setup. This allocates the rings and schedule
    /// feature struct, ready to be passed to `plugin.instantiate()`.
    pub fn new() -> Self {
        let (request_tx, request_rx) = msg_ring(RING_CAPACITY);
        let (response_tx, response_rx) = msg_ring(RING_CAPACITY);

        let sched_ctx = Box::new(ScheduleContext {
            request_tx: parking_lot::Mutex::new(request_tx),
            worker_thread: parking_lot::Mutex::new(None),
        });

        let schedule = Box::new(LV2_Worker_Schedule {
            handle: &*sched_ctx as *const ScheduleContext as LV2_Worker_Schedule_Handle,
            schedule_work: schedule_work_callback,
        });

        Lv2WorkerSetup {
            request_rx,
            response_tx,
            response_rx,
            schedule,
            sched_ctx,
        }
    }

    /// Build an `LV2Feature` for the worker schedule.
    /// Pass this (via transmute) to `plugin.instantiate()`.
    pub fn make_feature(&self) -> lv2_raw::core::LV2Feature {
        lv2_raw::core::LV2Feature {
            uri: LV2_WORKER_SCHEDULE_URI.as_ptr(),
            data: &*self.schedule as *const LV2_Worker_Schedule as *mut c_void,
        }
    }

    /// Activate the worker: spawn the background thread and return the
    /// active `Lv2Worker`.
    ///
    /// # Safety
    /// `lv2_handle` and `worker_iface` must stay valid until `Lv2Worker::stop()`
    /// has returned (or the worker is dropped).
    pub unsafe fn activate(
        self,
        lv2_handle: *mut c_void,
        worker_iface: *const LV2_Worker_Interface,
    ) -> Lv2Worker {
        let Lv2WorkerSetup {
            request_rx,
            response_tx,
            response_rx,
            schedule,
            sched_ctx,
        } = self;

        let stop = Arc::new(AtomicBool::new(false));
        let worker_ns = Arc::new(AtomicU64::new(0));

        let thread_handle = lv2_handle as usize;
        let thread_iface = worker_iface as usize;
        let thread_worker_ns = worker_ns.clone();
        let thread_stop = stop.clone();

        let thread = std::thread::Builder::new()
            .name("lv2-worker".to_string())
            .spawn(move || {
                worker_thread_main(
                    thread_handle as *mut c_void,
                    thread_iface as *const LV2_Worker_Interface,
                    request_rx,
                    response_tx,
                    thread_stop,
                    thread_worker_ns,
                );
            })
            .expect("Failed to spawn LV2 worker thread");

        *sched_ctx.worker_thread.lock() = Some(thread.thread().clone());

        Lv2Worker {
            response_rx,
            response_buf: vec![0u8; MAX_RESPONSE_SIZE].into_boxed_slice(),
            _schedule: schedule,
            sched_ctx,
            worker_iface,
            lv2_handle,
            thread: Some(thread),
            stop,
            worker_ns,
        }
    }
}

// ── Phase 2: Active worker ──

/// Per-instance worker handle.
/// Constructed via `Lv2WorkerSetup::activate()`.
pub struct Lv2Worker {
    /// Ring for worker thread → RT (responses)
    response_rx: MsgConsumer,
    /// Preallocated buffer a response is copied into before `work_response()`
    response_buf: Box<[u8]>,
    /// The C struct whose pointer is given to the plugin as the feature
    _schedule: Box<LV2_Worker_Schedule>,
    /// Pointed to by `_schedule.handle`
    sched_ctx: Box<ScheduleContext>,
    /// Worker interface retrieved from the plugin
    worker_iface: *const LV2_Worker_Interface,
    /// LV2_Handle (the raw instance pointer for calling work/work_response)
    lv2_handle: *mut c_void,
    thread: Option<std::thread::JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    /// Accumulated worker thread CPU time in nanoseconds (written by worker thread, drained by RT thread)
    worker_ns: Arc<AtomicU64>,
}

// SAFETY: The raw pointers (lv2_handle, worker_iface) are only used from the
// PipeWire filter thread (same thread that calls process) and from the worker
// thread, which is joined before the instance is freed.
unsafe impl Send for Lv2Worker {}

impl Lv2Worker {
    /// Drain and return accumulated worker thread CPU time in nanoseconds.
    /// Called from the process (RT) thread to include worker time in DSP stats.
    pub fn drain_worker_ns(&self) -> u64 {
        self.worker_ns.swap(0, Ordering::Relaxed)
    }

    /// Deliver pending worker responses to the plugin.
    /// Must be called from the process (RT) thread, after `run()`.
    /// Never allocates or blocks.
    ///
    /// # Safety
    /// Must only be called from the same thread that calls `run()`.
    pub unsafe fn deliver_responses(&mut self) {
        unsafe {
            let iface = &*self.worker_iface;
            loop {
                match self.response_rx.pop_into(&mut self.response_buf) {
                    MsgPop::Data(len) => {
                        (iface.work_response)(
                            self.lv2_handle,
                            len as u32,
                            self.response_buf.as_ptr() as *const c_void,
                        );
                    }
                    MsgPop::TooLarge(_) => {
                        // respond() rejects these; never expected here.
                        self.response_rx.discard();
                    }
                    MsgPop::Empty => break,
                }
            }
            if let Some(end_run) = iface.end_run {
                end_run(self.lv2_handle);
            }
        }
    }

    /// Stop the worker thread and wait for it to exit.  After this returns
    /// the plugin's `work()` is no longer called and the instance may be
    /// freed.  Idempotent.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread.thread().unpark();
            if thread.join().is_err() {
                log::error!("LV2 worker thread panicked");
            }
        }
    }
}

impl Drop for Lv2Worker {
    fn drop(&mut self) {
        self.stop();
        *self.sched_ctx.worker_thread.lock() = None;
    }
}

// ── Callbacks ──

/// Called by the plugin from `run()` (RT thread) to schedule work.
/// Copies the request into the preallocated ring; never allocates or blocks.
unsafe extern "C" fn schedule_work_callback(
    handle: LV2_Worker_Schedule_Handle,
    size: u32,
    data: *const c_void,
) -> LV2_Worker_Status {
    if handle.is_null() {
        return LV2_WORKER_ERR_UNKNOWN;
    }
    let ctx = unsafe { &*(handle as *const ScheduleContext) };
    let payload: &[u8] = if size > 0 && !data.is_null() {
        unsafe { std::slice::from_raw_parts(data as *const u8, size as usize) }
    } else {
        &[]
    };
    let pushed = match ctx.request_tx.try_lock() {
        Some(mut tx) => tx.push(payload),
        None => false,
    };
    if !pushed {
        return LV2_WORKER_ERR_NO_SPACE;
    }
    if let Some(guard) = ctx.worker_thread.try_lock()
        && let Some(ref thread) = *guard
    {
        thread.unpark();
    }
    LV2_WORKER_SUCCESS
}

/// Called by the plugin's `work()` (worker thread) to send a response back
/// to the RT thread.
unsafe extern "C" fn respond_callback(
    handle: LV2_Worker_Respond_Handle,
    size: u32,
    data: *const c_void,
) -> LV2_Worker_Status {
    if handle.is_null() {
        return LV2_WORKER_ERR_UNKNOWN;
    }
    let ctx = unsafe { &mut *(handle as *mut RespondContext) };
    if size as usize > MAX_RESPONSE_SIZE {
        log::warn!("LV2 worker: response of {} bytes too large, dropped", size);
        return LV2_WORKER_ERR_NO_SPACE;
    }
    let payload: &[u8] = if size > 0 && !data.is_null() {
        unsafe { std::slice::from_raw_parts(data as *const u8, size as usize) }
    } else {
        &[]
    };
    if ctx.response_tx.push(payload) {
        LV2_WORKER_SUCCESS
    } else {
        LV2_WORKER_ERR_NO_SPACE
    }
}

// ── Worker thread ──

fn worker_thread_main(
    lv2_handle: *mut c_void,
    worker_iface: *const LV2_Worker_Interface,
    mut request_rx: MsgConsumer,
    response_tx: MsgProducer,
    stop: Arc<AtomicBool>,
    worker_ns: Arc<AtomicU64>,
) {
    let mut respond_ctx = RespondContext { response_tx };
    let mut buf: Vec<u8> = Vec::new();
    while !stop.load(Ordering::Acquire) {
        let Some(len) = request_rx.peek_len() else {
            std::thread::park_timeout(std::time::Duration::from_millis(100));
            continue;
        };
        if buf.len() < len {
            buf.resize(len, 0);
        }
        let MsgPop::Data(len) = request_rx.pop_into(&mut buf) else {
            continue;
        };
        let t0 = std::time::Instant::now();
        unsafe {
            let iface = &*worker_iface;
            (iface.work)(
                lv2_handle,
                respond_callback,
                &mut respond_ctx as *mut RespondContext as LV2_Worker_Respond_Handle,
                len as u32,
                if len == 0 {
                    std::ptr::null()
                } else {
                    buf.as_ptr() as *const c_void
                },
            );
        }
        let elapsed = t0.elapsed().as_nanos() as u64;
        worker_ns.fetch_add(elapsed, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;

    static WORK_CALLS: AtomicUsize = AtomicUsize::new(0);
    static RESPONSES: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

    unsafe extern "C" fn test_work(
        _instance: *mut c_void,
        respond: LV2_Worker_Respond_Function,
        handle: LV2_Worker_Respond_Handle,
        size: u32,
        data: *const c_void,
    ) -> LV2_Worker_Status {
        WORK_CALLS.fetch_add(1, Ordering::SeqCst);
        unsafe { respond(handle, size, data) }
    }

    unsafe extern "C" fn test_work_response(
        _instance: *mut c_void,
        size: u32,
        body: *const c_void,
    ) -> LV2_Worker_Status {
        let data = unsafe { std::slice::from_raw_parts(body as *const u8, size as usize) };
        RESPONSES.lock().unwrap().push(data.to_vec());
        LV2_WORKER_SUCCESS
    }

    #[test]
    fn worker_round_trip_and_join() {
        let iface = Box::new(LV2_Worker_Interface {
            work: test_work,
            work_response: test_work_response,
            end_run: None,
        });
        let setup = Lv2WorkerSetup::new();
        let sched: *const LV2_Worker_Schedule = &*setup.schedule;
        let mut worker = unsafe { setup.activate(std::ptr::null_mut(), &*iface) };

        let msg = *b"hello worker";
        let status = unsafe {
            ((*sched).schedule_work)(
                (*sched).handle,
                msg.len() as u32,
                msg.as_ptr() as *const c_void,
            )
        };
        assert_eq!(status, LV2_WORKER_SUCCESS);

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while RESPONSES.lock().unwrap().is_empty() && std::time::Instant::now() < deadline {
            unsafe { worker.deliver_responses() };
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(RESPONSES.lock().unwrap().as_slice(), &[msg.to_vec()]);
        assert_eq!(WORK_CALLS.load(Ordering::SeqCst), 1);

        let t0 = std::time::Instant::now();
        worker.stop();
        assert!(worker.thread.is_none());
        assert!(t0.elapsed() < std::time::Duration::from_secs(1));
    }
}
