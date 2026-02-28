//! LV2 Worker extension implementation.
//!
//! Provides `LV2_Worker_Schedule` feature so plugins can offload
//! non-realtime work to a background thread.  The RT thread calls
//! `schedule_work` which enqueues a message; a dedicated worker thread
//! picks it up, calls the plugin's `work()`, and any responses are
//! queued back for delivery in the next `process()` cycle via
//! `work_response()`.
//!
//! Two-phase construction:
//!   1. `Lv2WorkerSetup::new()` — creates channels + schedule feature (pre-instantiation)
//!   2. `Lv2WorkerSetup::activate(handle, iface)` → `Lv2Worker` (post-instantiation)

use std::ffi::{CStr, c_void};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

// ── C-compatible struct definitions matching lv2/worker/worker.h ──

pub const LV2_WORKER_SCHEDULE_URI: &CStr = c"http://lv2plug.in/ns/ext/worker#schedule";
pub const LV2_WORKER_INTERFACE_URI: &str = "http://lv2plug.in/ns/ext/worker#interface";

#[allow(non_camel_case_types)]
pub type LV2_Worker_Status = u32;
pub const LV2_WORKER_SUCCESS: LV2_Worker_Status = 0;
#[allow(dead_code)]
pub const LV2_WORKER_ERR_UNKNOWN: LV2_Worker_Status = 1;

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

// ── Messages passed between RT thread and worker thread ──

struct WorkRequest {
    data: Vec<u8>,
}

struct WorkResponse {
    data: Vec<u8>,
}

// ── Context structs ──

/// Context passed as the `handle` in the `LV2_Worker_Schedule` C struct.
/// Lives on the heap and the pointer is stable.
struct ScheduleContext {
    request_tx: mpsc::Sender<WorkRequest>,
}

/// Context passed as the `LV2_Worker_Respond_Handle` to the `work()` callback.
/// Created on the stack of the worker thread for each work call.
struct RespondContext {
    response_tx: mpsc::Sender<WorkResponse>,
}

// ── Phase 1: Pre-instantiation setup ──

/// Holds the channels and schedule feature needed BEFORE plugin instantiation.
/// Call `make_feature()` to get the `LV2Feature` to pass to `plugin.instantiate()`,
/// then call `activate()` after instantiation to start the worker thread.
pub struct Lv2WorkerSetup {
    request_tx: mpsc::Sender<WorkRequest>,
    request_rx: Option<mpsc::Receiver<WorkRequest>>,
    response_tx: mpsc::Sender<WorkResponse>,
    response_rx: Option<mpsc::Receiver<WorkResponse>>,
    /// Heap-allocated schedule struct (stable pointer for the plugin)
    schedule: Box<LV2_Worker_Schedule>,
    /// Heap-allocated context (reclaimed on drop or transferred to Lv2Worker)
    sched_ctx_ptr: *mut ScheduleContext,
}

// SAFETY: The raw pointer (sched_ctx_ptr) is only used to build the feature
// and is either reclaimed in Drop or transferred to Lv2Worker.
unsafe impl Send for Lv2WorkerSetup {}

impl Lv2WorkerSetup {
    /// Create a new worker setup. This allocates the channels and schedule
    /// feature struct, ready to be passed to `plugin.instantiate()`.
    pub fn new() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<WorkRequest>();
        let (response_tx, response_rx) = mpsc::channel::<WorkResponse>();

        let sched_ctx = Box::new(ScheduleContext {
            request_tx: request_tx.clone(),
        });
        let sched_ctx_ptr = Box::into_raw(sched_ctx);

        let schedule = Box::new(LV2_Worker_Schedule {
            handle: sched_ctx_ptr as LV2_Worker_Schedule_Handle,
            schedule_work: schedule_work_callback,
        });

        Lv2WorkerSetup {
            request_tx,
            request_rx: Some(request_rx),
            response_tx,
            response_rx: Some(response_rx),
            schedule,
            sched_ctx_ptr,
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
    /// `lv2_handle` — the raw LV2 instance handle from `instance.handle()`.
    /// `worker_iface` — the plugin's `LV2_Worker_Interface` from `extension_data`.
    ///
    /// # Safety
    /// `lv2_handle` and `worker_iface` must be valid for the lifetime of
    /// the returned `Lv2Worker`.
    pub unsafe fn activate(
        mut self,
        lv2_handle: *mut c_void,
        worker_iface: *const LV2_Worker_Interface,
    ) -> Lv2Worker {
        let request_rx = self.request_rx.take().expect("activate called twice");
        let response_rx = self.response_rx.take().expect("activate called twice");
        let response_tx = self.response_tx.clone();

        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let worker_ns = Arc::new(AtomicU64::new(0));

        let thread_handle = lv2_handle as usize;
        let thread_iface = worker_iface as usize;
        let thread_worker_ns = worker_ns.clone();

        let thread = std::thread::Builder::new()
            .name("lv2-worker".to_string())
            .spawn(move || {
                worker_thread_main(
                    thread_handle as *mut c_void,
                    thread_iface as *const LV2_Worker_Interface,
                    request_rx,
                    response_tx,
                    stop_rx,
                    thread_worker_ns,
                );
            })
            .expect("Failed to spawn LV2 worker thread");

        // Transfer ownership of the schedule + context to Lv2Worker.
        // Prevent our Drop from reclaiming the ScheduleContext.
        let sched_ctx_ptr = self.sched_ctx_ptr;
        self.sched_ctx_ptr = std::ptr::null_mut();

        let worker = Lv2Worker {
            _request_tx: self.request_tx.clone(),
            response_rx,
            schedule: std::mem::replace(
                &mut self.schedule,
                Box::new(LV2_Worker_Schedule {
                    handle: std::ptr::null_mut(),
                    schedule_work: schedule_work_callback,
                }),
            ),
            worker_iface,
            lv2_handle,
            _thread: thread,
            _stop_tx: stop_tx,
            sched_ctx_ptr,
            worker_ns,
        };

        // Forget self so Drop doesn't reclaim the context
        std::mem::forget(self);

        worker
    }
}

impl Drop for Lv2WorkerSetup {
    fn drop(&mut self) {
        // Reclaim the ScheduleContext if we still own it (activate was never called)
        if !self.sched_ctx_ptr.is_null() {
            unsafe {
                drop(Box::from_raw(self.sched_ctx_ptr));
            }
            self.sched_ctx_ptr = std::ptr::null_mut();
        }
    }
}

// ── Phase 2: Active worker ──

/// Per-instance worker handle.
/// Constructed via `Lv2WorkerSetup::activate()`, dropped when the plugin is destroyed.
pub struct Lv2Worker {
    /// Keep a sender so the channel stays alive
    _request_tx: mpsc::Sender<WorkRequest>,
    /// Channel for worker thread → RT (responses)
    response_rx: mpsc::Receiver<WorkResponse>,
    /// The C struct whose pointer is given to the plugin as the feature
    schedule: Box<LV2_Worker_Schedule>,
    /// Worker interface retrieved from the plugin
    worker_iface: *const LV2_Worker_Interface,
    /// LV2_Handle (the raw instance pointer for calling work/work_response)
    lv2_handle: *mut c_void,
    /// Keep the thread handle alive
    _thread: std::thread::JoinHandle<()>,
    /// Signal the worker thread to stop
    _stop_tx: mpsc::Sender<()>,
    /// ScheduleContext pointer (reclaimed on drop)
    sched_ctx_ptr: *mut ScheduleContext,
    /// Accumulated worker thread CPU time in nanoseconds (written by worker thread, drained by RT thread)
    worker_ns: Arc<AtomicU64>,
}

// SAFETY: The raw pointers (lv2_handle, worker_iface) are only accessed from
// the PipeWire filter thread (same thread that calls process).
unsafe impl Send for Lv2Worker {}

impl Lv2Worker {
    /// Deliver pending worker responses to the plugin.
    /// Must be called from the process (RT) thread, after `run()`.
    ///
    /// # Safety
    /// Must only be called from the same thread that calls `run()`.
    /// Drain and return accumulated worker thread CPU time in nanoseconds.
    /// Called from the process (RT) thread to include worker time in DSP stats.
    pub fn drain_worker_ns(&self) -> u64 {
        self.worker_ns.swap(0, Ordering::Relaxed)
    }

    pub unsafe fn deliver_responses(&self) {
        unsafe {
            let iface = &*self.worker_iface;
            while let Ok(resp) = self.response_rx.try_recv() {
                (iface.work_response)(
                    self.lv2_handle,
                    resp.data.len() as u32,
                    resp.data.as_ptr() as *const c_void,
                );
            }
            if let Some(end_run) = iface.end_run {
                end_run(self.lv2_handle);
            }
        }
    }
}

impl Drop for Lv2Worker {
    fn drop(&mut self) {
        // _stop_tx and _request_tx are dropped, which causes the worker thread
        // to exit via Disconnected on request_rx or stop_rx.
        // Reclaim the ScheduleContext.
        if !self.sched_ctx_ptr.is_null() {
            unsafe {
                drop(Box::from_raw(self.sched_ctx_ptr));
            }
            self.sched_ctx_ptr = std::ptr::null_mut();
        }
    }
}

// ── Callbacks ──

/// Called by the plugin from `run()` (RT thread) to schedule work.
unsafe extern "C" fn schedule_work_callback(
    handle: LV2_Worker_Schedule_Handle,
    size: u32,
    data: *const c_void,
) -> LV2_Worker_Status {
    if handle.is_null() {
        return LV2_WORKER_ERR_UNKNOWN;
    }
    let ctx = unsafe { &*(handle as *const ScheduleContext) };
    let payload = if size > 0 && !data.is_null() {
        unsafe { std::slice::from_raw_parts(data as *const u8, size as usize) }.to_vec()
    } else {
        Vec::new()
    };
    match ctx.request_tx.send(WorkRequest { data: payload }) {
        Ok(_) => LV2_WORKER_SUCCESS,
        Err(_) => LV2_WORKER_ERR_UNKNOWN,
    }
}

/// Called by the plugin's `work()` to send a response back to the RT thread.
unsafe extern "C" fn respond_callback(
    handle: LV2_Worker_Respond_Handle,
    size: u32,
    data: *const c_void,
) -> LV2_Worker_Status {
    if handle.is_null() {
        return LV2_WORKER_ERR_UNKNOWN;
    }
    let ctx = unsafe { &*(handle as *const RespondContext) };
    let payload = if size > 0 && !data.is_null() {
        unsafe { std::slice::from_raw_parts(data as *const u8, size as usize) }.to_vec()
    } else {
        Vec::new()
    };
    match ctx.response_tx.send(WorkResponse { data: payload }) {
        Ok(_) => LV2_WORKER_SUCCESS,
        Err(_) => LV2_WORKER_ERR_UNKNOWN,
    }
}

// ── Worker thread ──

fn worker_thread_main(
    lv2_handle: *mut c_void,
    worker_iface: *const LV2_Worker_Interface,
    request_rx: mpsc::Receiver<WorkRequest>,
    response_tx: mpsc::Sender<WorkResponse>,
    stop_rx: mpsc::Receiver<()>,
    worker_ns: Arc<AtomicU64>,
) {
    loop {
        // Wait for a work request or stop signal
        // Use a small timeout so we can check stop_rx
        match request_rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(req) => {
                let respond_ctx = RespondContext {
                    response_tx: response_tx.clone(),
                };
                let t0 = std::time::Instant::now();
                unsafe {
                    let iface = &*worker_iface;
                    (iface.work)(
                        lv2_handle,
                        respond_callback,
                        &respond_ctx as *const RespondContext as LV2_Worker_Respond_Handle,
                        req.data.len() as u32,
                        if req.data.is_empty() {
                            std::ptr::null()
                        } else {
                            req.data.as_ptr() as *const c_void
                        },
                    );
                }
                let elapsed = t0.elapsed().as_nanos() as u64;
                worker_ns.fetch_add(elapsed, Ordering::Relaxed);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Check if we should stop
                if stop_rx.try_recv().is_ok() {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break;
            }
        }
    }
}
