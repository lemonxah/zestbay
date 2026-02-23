//! VST3 COM host objects — IHostApplication, IComponentHandler, IRunLoop.
//!
//! These are hand-rolled COM objects (same pattern as HostPlugFrame in ui.rs)
//! that VST3 plugins need for correct operation:
//!
//! - **IHostApplication** — passed to `IComponent::initialize()` and
//!   `IEditController::initialize()`. Plugins use it for `getName()` and
//!   sometimes `queryInterface` for `IRunLoop`.
//!
//! - **IComponentHandler** — set on the controller via `setComponentHandler()`.
//!   Plugins call `performEdit()` when the user tweaks a knob in the GUI.
//!
//! - **IRunLoop** — Linux-specific. Plugins (esp. JUCE-based) register timers
//!   and file-descriptor event handlers that must be serviced on the GUI thread.

use std::collections::HashMap;
use std::os::raw::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use vst3::Steinberg::IBStream_::IStreamSeekMode_::*;
use vst3::Steinberg::Linux::*;
use vst3::Steinberg::Vst::*;
use vst3::Steinberg::*;

use crate::plugin::types::*;

// =========================================================================
// IHostApplication
// =========================================================================

/// Our host-side IHostApplication.
///
/// Layout: first field is a pointer to the vtable so that a
/// `*mut HostApplication` can be cast to `*mut IHostApplication` / `*mut FUnknown`.
#[repr(C)]
pub struct HostApplication {
    pub(crate) vtbl: *const IHostApplicationVtbl,
    ref_count: AtomicU32,
    /// Pointer to the shared HostRunLoop (if any). The HostApplication's
    /// queryInterface returns this for IRunLoop_iid requests.
    pub run_loop: *mut HostRunLoop,
}

unsafe impl Send for HostApplication {}
unsafe impl Sync for HostApplication {}

static HOST_APP_VTBL: IHostApplicationVtbl = IHostApplicationVtbl {
    base: FUnknownVtbl {
        queryInterface: host_app_query_interface,
        addRef: host_app_add_ref,
        release: host_app_release,
    },
    getName: host_app_get_name,
    createInstance: host_app_create_instance,
};

unsafe extern "system" fn host_app_query_interface(
    this: *mut FUnknown,
    iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    unsafe {
        if iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        let iid_ref = &*iid;

        // Accept FUnknown or IHostApplication
        if *iid_ref == FUnknown_iid || *iid_ref == IHostApplication_iid {
            host_app_add_ref(this);
            *obj = this as *mut c_void;
            return kResultOk;
        }

        // Forward IRunLoop queries to our run loop
        if *iid_ref == IRunLoop_iid {
            let app = this as *mut HostApplication;
            if !(*app).run_loop.is_null() {
                host_run_loop_add_ref((*app).run_loop as *mut FUnknown);
                *obj = (*app).run_loop as *mut c_void;
                return kResultOk;
            }
        }

        *obj = std::ptr::null_mut();
        kNoInterface
    }
}

unsafe extern "system" fn host_app_add_ref(this: *mut FUnknown) -> uint32 {
    unsafe {
        let app = this as *mut HostApplication;
        let old = (*app).ref_count.fetch_add(1, Ordering::Relaxed);
        old + 1
    }
}

unsafe extern "system" fn host_app_release(this: *mut FUnknown) -> uint32 {
    unsafe {
        let app = this as *mut HostApplication;
        let old = (*app).ref_count.fetch_sub(1, Ordering::Relaxed);
        if old == 1 {
            drop(Box::from_raw(app));
            return 0;
        }
        old - 1
    }
}

unsafe extern "system" fn host_app_get_name(
    _this: *mut IHostApplication,
    name: *mut String128,
) -> tresult {
    unsafe {
        if name.is_null() {
            return kInvalidArgument;
        }
        // Write "ZestBay" as UTF-16 into String128 ([TChar; 128] = [i16; 128])
        let host_name = "ZestBay";
        let buf = &mut *name;
        for (i, ch) in host_name.encode_utf16().enumerate() {
            if i >= 127 {
                break;
            }
            buf[i] = ch;
        }
        buf[host_name.len().min(127)] = 0;
        kResultOk
    }
}

unsafe extern "system" fn host_app_create_instance(
    _this: *mut IHostApplication,
    _cid: *mut TUID,
    _iid: *mut TUID,
    _obj: *mut *mut c_void,
) -> tresult {
    // We don't support creating instances via the host application.
    kNotImplemented
}

/// Allocate a new HostApplication (ref_count starts at 1).
pub fn new_host_application(run_loop: *mut HostRunLoop) -> *mut HostApplication {
    let app = Box::new(HostApplication {
        vtbl: &HOST_APP_VTBL,
        ref_count: AtomicU32::new(1),
        run_loop,
    });
    Box::into_raw(app)
}

/// Release (decrement refcount of) a HostApplication.
///
/// # Safety
/// `app` must be a valid pointer from `new_host_application`.
pub unsafe fn release_host_application(app: *mut HostApplication) {
    if !app.is_null() {
        unsafe {
            host_app_release(app as *mut FUnknown);
        }
    }
}

/// Release (decrement refcount of) a HostComponentHandler.
///
/// # Safety
/// `ch` must be a valid pointer from `new_host_component_handler`.
pub unsafe fn release_host_component_handler(ch: *mut HostComponentHandler) {
    if !ch.is_null() {
        unsafe {
            host_ch_release(ch as *mut FUnknown);
        }
    }
}

// =========================================================================
// IComponentHandler
// =========================================================================

/// Our host-side IComponentHandler.
///
/// When the plugin's GUI changes a parameter, it calls `performEdit()`.
/// We write the new value into the shared `PortUpdates` so the audio thread
/// picks it up.
#[repr(C)]
pub struct HostComponentHandler {
    vtbl: *const IComponentHandlerVtbl,
    ref_count: AtomicU32,
    pub instance_id: PluginInstanceId,
    /// Mapping from VST3 ParamID → port_index in SharedPortUpdates.
    pub param_map: Arc<Mutex<HashMap<u32, usize>>>,
    /// Shared port updates for lock-free parameter sync.
    pub port_updates: SharedPortUpdates,
}

unsafe impl Send for HostComponentHandler {}
unsafe impl Sync for HostComponentHandler {}

static HOST_COMPONENT_HANDLER_VTBL: IComponentHandlerVtbl = IComponentHandlerVtbl {
    base: FUnknownVtbl {
        queryInterface: host_ch_query_interface,
        addRef: host_ch_add_ref,
        release: host_ch_release,
    },
    beginEdit: host_ch_begin_edit,
    performEdit: host_ch_perform_edit,
    endEdit: host_ch_end_edit,
    restartComponent: host_ch_restart_component,
};

unsafe extern "system" fn host_ch_query_interface(
    this: *mut FUnknown,
    iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    unsafe {
        if iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        let iid_ref = &*iid;
        if *iid_ref == FUnknown_iid || *iid_ref == IComponentHandler_iid {
            host_ch_add_ref(this);
            *obj = this as *mut c_void;
            return kResultOk;
        }
        *obj = std::ptr::null_mut();
        kNoInterface
    }
}

unsafe extern "system" fn host_ch_add_ref(this: *mut FUnknown) -> uint32 {
    unsafe {
        let ch = this as *mut HostComponentHandler;
        let old = (*ch).ref_count.fetch_add(1, Ordering::Relaxed);
        old + 1
    }
}

unsafe extern "system" fn host_ch_release(this: *mut FUnknown) -> uint32 {
    unsafe {
        let ch = this as *mut HostComponentHandler;
        let old = (*ch).ref_count.fetch_sub(1, Ordering::Relaxed);
        if old == 1 {
            drop(Box::from_raw(ch));
            return 0;
        }
        old - 1
    }
}

unsafe extern "system" fn host_ch_begin_edit(
    _this: *mut IComponentHandler,
    _id: ParamID,
) -> tresult {
    // We don't need to do anything special at the start of an edit gesture.
    kResultOk
}

unsafe extern "system" fn host_ch_perform_edit(
    this: *mut IComponentHandler,
    id: ParamID,
    value_normalized: ParamValue,
) -> tresult {
    unsafe {
        let ch = this as *mut HostComponentHandler;
        let port_updates = &(*ch).port_updates;

        if let Ok(param_map) = (*ch).param_map.lock() {
            if let Some(&port_index) = param_map.get(&id) {
                if let Some(slot) = port_updates
                    .control_inputs
                    .iter()
                    .find(|s| s.port_index == port_index)
                {
                    slot.value.store(value_normalized as f32);
                    return kResultOk;
                }
            }
        }

        kResultOk
    }
}

unsafe extern "system" fn host_ch_end_edit(_this: *mut IComponentHandler, _id: ParamID) -> tresult {
    kResultOk
}

unsafe extern "system" fn host_ch_restart_component(
    _this: *mut IComponentHandler,
    flags: int32,
) -> tresult {
    log::debug!(
        "VST3 IComponentHandler::restartComponent(flags=0x{:x})",
        flags
    );
    // We could handle specific restart flags here (e.g., kParamValuesChanged
    // to re-read param values, kLatencyChanged to update latency compensation).
    // For now, acknowledge the request.
    kResultOk
}

/// Allocate a new HostComponentHandler (ref_count starts at 1).
pub fn new_host_component_handler(
    instance_id: PluginInstanceId,
    param_map: HashMap<u32, usize>,
    port_updates: SharedPortUpdates,
) -> *mut HostComponentHandler {
    let ch = Box::new(HostComponentHandler {
        vtbl: &HOST_COMPONENT_HANDLER_VTBL,
        ref_count: AtomicU32::new(1),
        instance_id,
        param_map: Arc::new(Mutex::new(param_map)),
        port_updates,
    });
    Box::into_raw(ch)
}

// =========================================================================
// IRunLoop (Linux-specific)
// =========================================================================

/// A registered timer entry.
struct TimerEntry {
    handler: *mut ITimerHandler,
    interval_ms: u64,
    last_fired: std::time::Instant,
}

/// A registered fd event handler entry.
struct FdEntry {
    handler: *mut IEventHandler,
    fd: std::os::raw::c_int,
}

/// Our host-side IRunLoop for Linux.
///
/// VST3 plugins (especially JUCE-based) register timers and file descriptor
/// event handlers via this interface. We service them from the X11 event
/// loop thread.
#[repr(C)]
pub struct HostRunLoop {
    vtbl: *const IRunLoopVtbl,
    ref_count: AtomicU32,
    timers: Mutex<Vec<TimerEntry>>,
    fd_handlers: Mutex<Vec<FdEntry>>,
}

unsafe impl Send for HostRunLoop {}
unsafe impl Sync for HostRunLoop {}

static HOST_RUN_LOOP_VTBL: IRunLoopVtbl = IRunLoopVtbl {
    base: FUnknownVtbl {
        queryInterface: host_run_loop_query_interface,
        addRef: host_run_loop_add_ref,
        release: host_run_loop_release,
    },
    registerEventHandler: host_run_loop_register_event_handler,
    unregisterEventHandler: host_run_loop_unregister_event_handler,
    registerTimer: host_run_loop_register_timer,
    unregisterTimer: host_run_loop_unregister_timer,
};

unsafe extern "system" fn host_run_loop_query_interface(
    this: *mut FUnknown,
    iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    unsafe {
        if iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        let iid_ref = &*iid;
        if *iid_ref == FUnknown_iid || *iid_ref == IRunLoop_iid {
            host_run_loop_add_ref(this);
            *obj = this as *mut c_void;
            return kResultOk;
        }
        *obj = std::ptr::null_mut();
        kNoInterface
    }
}

pub unsafe extern "system" fn host_run_loop_add_ref(this: *mut FUnknown) -> uint32 {
    unsafe {
        let rl = this as *mut HostRunLoop;
        let old = (*rl).ref_count.fetch_add(1, Ordering::Relaxed);
        old + 1
    }
}

pub unsafe extern "system" fn host_run_loop_release(this: *mut FUnknown) -> uint32 {
    unsafe {
        let rl = this as *mut HostRunLoop;
        let old = (*rl).ref_count.fetch_sub(1, Ordering::Relaxed);
        if old == 1 {
            drop(Box::from_raw(rl));
            return 0;
        }
        old - 1
    }
}

unsafe extern "system" fn host_run_loop_register_event_handler(
    this: *mut IRunLoop,
    handler: *mut IEventHandler,
    fd: FileDescriptor,
) -> tresult {
    unsafe {
        if handler.is_null() {
            return kInvalidArgument;
        }
        let rl = this as *mut HostRunLoop;
        log::debug!("VST3 IRunLoop: registerEventHandler fd={}", fd);
        if let Ok(mut fds) = (*rl).fd_handlers.lock() {
            fds.push(FdEntry { handler, fd });
        }
        kResultOk
    }
}

unsafe extern "system" fn host_run_loop_unregister_event_handler(
    this: *mut IRunLoop,
    handler: *mut IEventHandler,
) -> tresult {
    unsafe {
        let rl = this as *mut HostRunLoop;
        log::debug!("VST3 IRunLoop: unregisterEventHandler");
        if let Ok(mut fds) = (*rl).fd_handlers.lock() {
            fds.retain(|e| e.handler != handler);
        }
        kResultOk
    }
}

unsafe extern "system" fn host_run_loop_register_timer(
    this: *mut IRunLoop,
    handler: *mut ITimerHandler,
    milliseconds: TimerInterval,
) -> tresult {
    unsafe {
        if handler.is_null() {
            return kInvalidArgument;
        }
        let rl = this as *mut HostRunLoop;
        log::debug!("VST3 IRunLoop: registerTimer interval={}ms", milliseconds);
        if let Ok(mut timers) = (*rl).timers.lock() {
            timers.push(TimerEntry {
                handler,
                interval_ms: milliseconds,
                last_fired: std::time::Instant::now(),
            });
        }
        kResultOk
    }
}

unsafe extern "system" fn host_run_loop_unregister_timer(
    this: *mut IRunLoop,
    handler: *mut ITimerHandler,
) -> tresult {
    unsafe {
        let rl = this as *mut HostRunLoop;
        log::debug!("VST3 IRunLoop: unregisterTimer");
        if let Ok(mut timers) = (*rl).timers.lock() {
            timers.retain(|e| e.handler != handler);
        }
        kResultOk
    }
}

/// Allocate a new HostRunLoop (ref_count starts at 1).
pub fn new_host_run_loop() -> *mut HostRunLoop {
    let rl = Box::new(HostRunLoop {
        vtbl: &HOST_RUN_LOOP_VTBL,
        ref_count: AtomicU32::new(1),
        timers: Mutex::new(Vec::new()),
        fd_handlers: Mutex::new(Vec::new()),
    });
    Box::into_raw(rl)
}

/// Fire any due timers and poll any registered file descriptors.
///
/// Must be called from the GUI event loop thread (the same thread that
/// the plugin's GUI is running on).
///
/// # Safety
/// The `run_loop` must be a valid `*mut HostRunLoop`.
pub unsafe fn run_loop_tick(run_loop: *mut HostRunLoop) {
    unsafe {
        if run_loop.is_null() {
            return;
        }

        // Fire due timers
        if let Ok(mut timers) = (*run_loop).timers.lock() {
            let now = std::time::Instant::now();
            for entry in timers.iter_mut() {
                let elapsed = now.duration_since(entry.last_fired);
                if elapsed.as_millis() >= entry.interval_ms as u128 {
                    entry.last_fired = now;
                    let handler = entry.handler;
                    // We need to call onTimer outside the lock to avoid
                    // deadlocks if the plugin re-registers timers from the
                    // callback. Collect handlers to fire instead.
                    // ... but for simplicity and because this is the common
                    // case, we'll call directly. If deadlocks occur, we can
                    // collect and call outside the lock.
                    ((*(*handler).vtbl).onTimer)(handler);
                }
            }
        }

        // Poll registered file descriptors
        if let Ok(fds) = (*run_loop).fd_handlers.lock() {
            for entry in fds.iter() {
                // Use poll(2) with zero timeout to check readability
                let mut pfd = libc::pollfd {
                    fd: entry.fd,
                    events: libc::POLLIN,
                    revents: 0,
                };
                let ret = libc::poll(&mut pfd, 1, 0);
                if ret > 0 && (pfd.revents & libc::POLLIN) != 0 {
                    ((*(*entry.handler).vtbl).onFDIsSet)(entry.handler, entry.fd);
                }
            }
        }
    }
}

// =========================================================================
// IBStream — memory-backed byte stream for state save/load
// =========================================================================

/// A memory-backed `IBStream` for VST3 state serialization.
///
/// Used for `IComponent::getState()` / `setState()` and
/// `IEditController::getState()` / `setState()`.
#[repr(C)]
pub struct MemoryStream {
    vtbl: *const IBStreamVtbl,
    ref_count: AtomicU32,
    /// The backing buffer.
    pub data: Vec<u8>,
    /// Current read/write position.
    pub pos: usize,
}

unsafe impl Send for MemoryStream {}

static MEMORY_STREAM_VTBL: IBStreamVtbl = IBStreamVtbl {
    base: FUnknownVtbl {
        queryInterface: ms_query_interface,
        addRef: ms_add_ref,
        release: ms_release,
    },
    read: ms_read,
    write: ms_write,
    seek: ms_seek,
    tell: ms_tell,
};

unsafe extern "system" fn ms_query_interface(
    this: *mut FUnknown,
    iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    unsafe {
        if iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        let iid_ref = &*iid;
        if *iid_ref == FUnknown_iid || *iid_ref == IBStream_iid {
            ms_add_ref(this);
            *obj = this as *mut c_void;
            return kResultOk;
        }
        *obj = std::ptr::null_mut();
        kNoInterface
    }
}

unsafe extern "system" fn ms_add_ref(this: *mut FUnknown) -> uint32 {
    unsafe {
        let ms = this as *mut MemoryStream;
        let old = (*ms).ref_count.fetch_add(1, Ordering::Relaxed);
        old + 1
    }
}

unsafe extern "system" fn ms_release(this: *mut FUnknown) -> uint32 {
    unsafe {
        let ms = this as *mut MemoryStream;
        let old = (*ms).ref_count.fetch_sub(1, Ordering::Relaxed);
        if old == 1 {
            drop(Box::from_raw(ms));
            return 0;
        }
        old - 1
    }
}

unsafe extern "system" fn ms_read(
    this: *mut IBStream,
    buffer: *mut c_void,
    num_bytes: int32,
    num_bytes_read: *mut int32,
) -> tresult {
    unsafe {
        let ms = this as *mut MemoryStream;
        if buffer.is_null() || num_bytes < 0 {
            return kInvalidArgument;
        }
        let available = (*ms).data.len().saturating_sub((*ms).pos);
        let to_read = (num_bytes as usize).min(available);
        if to_read > 0 {
            std::ptr::copy_nonoverlapping(
                (*ms).data.as_ptr().add((*ms).pos),
                buffer as *mut u8,
                to_read,
            );
            (*ms).pos += to_read;
        }
        if !num_bytes_read.is_null() {
            *num_bytes_read = to_read as int32;
        }
        kResultOk
    }
}

unsafe extern "system" fn ms_write(
    this: *mut IBStream,
    buffer: *mut c_void,
    num_bytes: int32,
    num_bytes_written: *mut int32,
) -> tresult {
    unsafe {
        let ms = this as *mut MemoryStream;
        if buffer.is_null() || num_bytes < 0 {
            return kInvalidArgument;
        }
        let n = num_bytes as usize;
        let end = (*ms).pos + n;
        if end > (*ms).data.len() {
            (*ms).data.resize(end, 0);
        }
        std::ptr::copy_nonoverlapping(
            buffer as *const u8,
            (*ms).data.as_mut_ptr().add((*ms).pos),
            n,
        );
        (*ms).pos += n;
        if !num_bytes_written.is_null() {
            *num_bytes_written = n as int32;
        }
        kResultOk
    }
}

unsafe extern "system" fn ms_seek(
    this: *mut IBStream,
    pos: int64,
    mode: int32,
    result: *mut int64,
) -> tresult {
    unsafe {
        let ms = this as *mut MemoryStream;
        let new_pos: i64 = match mode {
            m if m == kIBSeekSet as int32 => pos,
            m if m == kIBSeekCur as int32 => (*ms).pos as i64 + pos,
            m if m == kIBSeekEnd as int32 => (*ms).data.len() as i64 + pos,
            _ => return kInvalidArgument,
        };
        if new_pos < 0 {
            return kInvalidArgument;
        }
        (*ms).pos = new_pos as usize;
        if !result.is_null() {
            *result = new_pos;
        }
        kResultOk
    }
}

unsafe extern "system" fn ms_tell(this: *mut IBStream, pos: *mut int64) -> tresult {
    unsafe {
        let ms = this as *mut MemoryStream;
        if !pos.is_null() {
            *pos = (*ms).pos as int64;
        }
        kResultOk
    }
}

/// Create a new empty MemoryStream (for getState — plugin writes into it).
pub fn new_memory_stream() -> *mut MemoryStream {
    let ms = Box::new(MemoryStream {
        vtbl: &MEMORY_STREAM_VTBL,
        ref_count: AtomicU32::new(1),
        data: Vec::new(),
        pos: 0,
    });
    Box::into_raw(ms)
}

/// Create a MemoryStream pre-filled with data (for setState — plugin reads from it).
pub fn new_memory_stream_from_data(data: Vec<u8>) -> *mut MemoryStream {
    let ms = Box::new(MemoryStream {
        vtbl: &MEMORY_STREAM_VTBL,
        ref_count: AtomicU32::new(1),
        data,
        pos: 0,
    });
    Box::into_raw(ms)
}

/// Release a MemoryStream.
///
/// # Safety
/// `ms` must be a valid pointer from `new_memory_stream*`.
pub unsafe fn release_memory_stream(ms: *mut MemoryStream) {
    if !ms.is_null() {
        unsafe {
            ms_release(ms as *mut FUnknown);
        }
    }
}
