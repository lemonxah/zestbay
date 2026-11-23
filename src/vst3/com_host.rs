use std::os::raw::c_void;
use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use vst3::Steinberg::IBStream_::IStreamSeekMode_::*;
use vst3::Steinberg::Linux::*;
use vst3::Steinberg::Vst::*;
use vst3::Steinberg::*;

use crate::plugin::types::*;

#[repr(C)]
pub struct HostApplication {
    pub(crate) vtbl: *const IHostApplicationVtbl,
    ref_count: AtomicU32,
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

        if *iid_ref == FUnknown_iid || *iid_ref == IHostApplication_iid {
            host_app_add_ref(this);
            *obj = this as *mut c_void;
            return kResultOk;
        }

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
    cid: *mut TUID,
    iid: *mut TUID,
    obj: *mut *mut c_void,
) -> tresult {
    unsafe {
        if obj.is_null() {
            return kInvalidArgument;
        }
        match super::message::create_instance(cid, iid) {
            Some(instance) => {
                *obj = instance;
                kResultOk
            }
            None => {
                *obj = std::ptr::null_mut();
                kNotImplemented
            }
        }
    }
}

pub fn new_host_application(run_loop: *mut HostRunLoop) -> *mut HostApplication {
    let app = Box::new(HostApplication {
        vtbl: &HOST_APP_VTBL,
        ref_count: AtomicU32::new(1),
        run_loop,
    });
    Box::into_raw(app)
}

pub unsafe fn release_host_application(app: *mut HostApplication) {
    if !app.is_null() {
        unsafe {
            host_app_release(app as *mut FUnknown);
        }
    }
}

pub unsafe fn release_host_component_handler(ch: *mut HostComponentHandler) {
    if !ch.is_null() {
        unsafe {
            host_ch_release(ch as *mut FUnknown);
        }
    }
}

pub struct Vst3SharedParams {
    pub port_updates: SharedPortUpdates,
    pub ids: Vec<ParamID>,
    pub id_index: Vec<(ParamID, usize)>,
    pub controller_bits: Vec<AtomicU32>,
    pub restart_flags: AtomicI32,
}

impl Vst3SharedParams {
    pub fn new(port_updates: SharedPortUpdates, ids: Vec<ParamID>) -> Self {
        let mut id_index: Vec<(ParamID, usize)> =
            ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
        id_index.sort_unstable_by_key(|&(id, _)| id);
        let controller_bits = port_updates
            .control_inputs
            .iter()
            .map(|s| AtomicU32::new(s.value.load().to_bits()))
            .collect();
        Self {
            port_updates,
            ids,
            id_index,
            controller_bits,
            restart_flags: AtomicI32::new(0),
        }
    }

    pub fn index_of(&self, id: ParamID) -> Option<usize> {
        self.id_index
            .binary_search_by_key(&id, |&(pid, _)| pid)
            .ok()
            .map(|pos| self.id_index[pos].1)
    }
}

#[repr(C)]
pub struct HostComponentHandler {
    vtbl: *const IComponentHandlerVtbl,
    ref_count: AtomicU32,
    pub instance_id: PluginInstanceId,
    pub shared: Arc<Vst3SharedParams>,
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
    kResultOk
}

unsafe extern "system" fn host_ch_perform_edit(
    this: *mut IComponentHandler,
    id: ParamID,
    value_normalized: ParamValue,
) -> tresult {
    unsafe {
        let shared = &(*(this as *mut HostComponentHandler)).shared;
        if let Some(idx) = shared.index_of(id)
            && let Some(slot) = shared.port_updates.control_inputs.get(idx)
        {
            let v = value_normalized.clamp(0.0, 1.0) as f32;
            shared.controller_bits[idx].store(v.to_bits(), Ordering::Relaxed);
            slot.value.store(v);
        }
        kResultOk
    }
}

unsafe extern "system" fn host_ch_end_edit(_this: *mut IComponentHandler, _id: ParamID) -> tresult {
    kResultOk
}

unsafe extern "system" fn host_ch_restart_component(
    this: *mut IComponentHandler,
    flags: int32,
) -> tresult {
    log::debug!(
        "VST3 IComponentHandler::restartComponent(flags=0x{:x})",
        flags
    );
    unsafe {
        let shared = &(*(this as *mut HostComponentHandler)).shared;
        shared.restart_flags.fetch_or(flags, Ordering::AcqRel);
    }
    kResultOk
}

pub fn new_host_component_handler(
    instance_id: PluginInstanceId,
    shared: Arc<Vst3SharedParams>,
) -> *mut HostComponentHandler {
    let ch = Box::new(HostComponentHandler {
        vtbl: &HOST_COMPONENT_HANDLER_VTBL,
        ref_count: AtomicU32::new(1),
        instance_id,
        shared,
    });
    Box::into_raw(ch)
}

struct TimerEntry {
    handler: *mut ITimerHandler,
    interval_ms: u64,
    last_fired: std::time::Instant,
}

struct FdEntry {
    handler: *mut IEventHandler,
    fd: std::os::raw::c_int,
}

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

pub fn new_host_run_loop() -> *mut HostRunLoop {
    let rl = Box::new(HostRunLoop {
        vtbl: &HOST_RUN_LOOP_VTBL,
        ref_count: AtomicU32::new(1),
        timers: Mutex::new(Vec::new()),
        fd_handlers: Mutex::new(Vec::new()),
    });
    Box::into_raw(rl)
}

pub unsafe fn run_loop_tick(run_loop: *mut HostRunLoop) {
    unsafe {
        if run_loop.is_null() {
            return;
        }
        let rl = &*run_loop;

        let now = std::time::Instant::now();
        let due: Vec<*mut ITimerHandler> = {
            let mut timers = rl.timers.lock().unwrap_or_else(|e| e.into_inner());
            timers
                .iter_mut()
                .filter(|e| now.duration_since(e.last_fired).as_millis() >= e.interval_ms as u128)
                .map(|e| {
                    e.last_fired = now;
                    e.handler
                })
                .collect()
        };
        for handler in due {
            let alive = {
                let timers = rl.timers.lock().unwrap_or_else(|e| e.into_inner());
                let alive = timers.iter().any(|e| e.handler == handler);
                if alive {
                    funknown_add_ref(handler as *mut FUnknown);
                }
                alive
            };
            if alive {
                ((*(*handler).vtbl).onTimer)(handler);
                funknown_release(handler as *mut FUnknown);
            }
        }

        let fds: Vec<(*mut IEventHandler, std::os::raw::c_int)> = {
            let fds = rl.fd_handlers.lock().unwrap_or_else(|e| e.into_inner());
            fds.iter().map(|e| (e.handler, e.fd)).collect()
        };
        for (handler, fd) in fds {
            let mut pfd = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            if libc::poll(&mut pfd, 1, 0) <= 0 || pfd.revents == 0 {
                continue;
            }
            let alive = {
                let fds = rl.fd_handlers.lock().unwrap_or_else(|e| e.into_inner());
                let alive = fds.iter().any(|e| e.handler == handler && e.fd == fd);
                if alive {
                    funknown_add_ref(handler as *mut FUnknown);
                }
                alive
            };
            if alive {
                ((*(*handler).vtbl).onFDIsSet)(handler, fd);
                funknown_release(handler as *mut FUnknown);
            }
        }
    }
}

pub fn run_loop_clear(run_loop: *mut HostRunLoop) {
    if run_loop.is_null() {
        return;
    }
    unsafe {
        let rl = &*run_loop;
        rl.timers.lock().unwrap_or_else(|e| e.into_inner()).clear();
        rl.fd_handlers.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

unsafe fn funknown_add_ref(obj: *mut FUnknown) {
    unsafe {
        ((*(*obj).vtbl).addRef)(obj);
    }
}

unsafe fn funknown_release(obj: *mut FUnknown) {
    unsafe {
        ((*(*obj).vtbl).release)(obj);
    }
}

#[repr(C)]
pub struct MemoryStream {
    vtbl: *const IBStreamVtbl,
    ref_count: AtomicU32,
    pub data: Vec<u8>,
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

pub fn new_memory_stream() -> *mut MemoryStream {
    let ms = Box::new(MemoryStream {
        vtbl: &MEMORY_STREAM_VTBL,
        ref_count: AtomicU32::new(1),
        data: Vec::new(),
        pos: 0,
    });
    Box::into_raw(ms)
}

pub fn new_memory_stream_from_data(data: Vec<u8>) -> *mut MemoryStream {
    let ms = Box::new(MemoryStream {
        vtbl: &MEMORY_STREAM_VTBL,
        ref_count: AtomicU32::new(1),
        data,
        pos: 0,
    });
    Box::into_raw(ms)
}

pub unsafe fn release_memory_stream(ms: *mut MemoryStream) {
    if !ms.is_null() {
        unsafe {
            ms_release(ms as *mut FUnknown);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_stream_write_then_read() {
        unsafe {
            let ms = new_memory_stream();
            let data = b"Hello, VST3!";
            let mut written: int32 = 0;
            let result = ms_write(
                ms as *mut IBStream,
                data.as_ptr() as *mut c_void,
                data.len() as int32,
                &mut written,
            );
            assert_eq!(result, kResultOk);
            assert_eq!(written, data.len() as int32);
            assert_eq!((*ms).data, data);
            assert_eq!((*ms).pos, data.len());

            let mut new_pos: int64 = 0;
            let result = ms_seek(
                ms as *mut IBStream,
                0,
                kIBSeekSet as int32,
                &mut new_pos,
            );
            assert_eq!(result, kResultOk);
            assert_eq!(new_pos, 0);

            let mut buf = [0u8; 32];
            let mut bytes_read: int32 = 0;
            let result = ms_read(
                ms as *mut IBStream,
                buf.as_mut_ptr() as *mut c_void,
                buf.len() as int32,
                &mut bytes_read,
            );
            assert_eq!(result, kResultOk);
            assert_eq!(bytes_read, data.len() as int32);
            assert_eq!(&buf[..data.len()], data);

            release_memory_stream(ms);
        }
    }

    #[test]
    fn memory_stream_from_data() {
        unsafe {
            let data = vec![1, 2, 3, 4, 5];
            let ms = new_memory_stream_from_data(data.clone());
            assert_eq!((*ms).pos, 0);
            assert_eq!((*ms).data, data);

            let mut buf = [0u8; 8];
            let mut bytes_read: int32 = 0;
            let result = ms_read(
                ms as *mut IBStream,
                buf.as_mut_ptr() as *mut c_void,
                buf.len() as int32,
                &mut bytes_read,
            );
            assert_eq!(result, kResultOk);
            assert_eq!(bytes_read, 5);
            assert_eq!(&buf[..5], &[1, 2, 3, 4, 5]);
            assert_eq!((*ms).pos, 5);

            release_memory_stream(ms);
        }
    }

    #[test]
    fn memory_stream_read_past_end() {
        unsafe {
            let ms = new_memory_stream_from_data(vec![10, 20]);
            (*ms).pos = 2;

            let mut buf = [0u8; 4];
            let mut bytes_read: int32 = 0;
            let result = ms_read(
                ms as *mut IBStream,
                buf.as_mut_ptr() as *mut c_void,
                4,
                &mut bytes_read,
            );
            assert_eq!(result, kResultOk);
            assert_eq!(bytes_read, 0);

            release_memory_stream(ms);
        }
    }

    #[test]
    fn memory_stream_partial_read() {
        unsafe {
            let ms = new_memory_stream_from_data(vec![1, 2, 3, 4, 5]);
            let mut buf = [0u8; 3];
            let mut bytes_read: int32 = 0;
            ms_read(
                ms as *mut IBStream,
                buf.as_mut_ptr() as *mut c_void,
                3,
                &mut bytes_read,
            );
            assert_eq!(bytes_read, 3);
            assert_eq!(&buf, &[1, 2, 3]);
            assert_eq!((*ms).pos, 3);

            ms_read(
                ms as *mut IBStream,
                buf.as_mut_ptr() as *mut c_void,
                3,
                &mut bytes_read,
            );
            assert_eq!(bytes_read, 2);
            assert_eq!(&buf[..2], &[4, 5]);

            release_memory_stream(ms);
        }
    }

    #[test]
    fn memory_stream_seek_set() {
        unsafe {
            let ms = new_memory_stream_from_data(vec![0; 100]);
            let mut result_pos: int64 = 0;

            let r = ms_seek(ms as *mut IBStream, 50, kIBSeekSet as int32, &mut result_pos);
            assert_eq!(r, kResultOk);
            assert_eq!(result_pos, 50);
            assert_eq!((*ms).pos, 50);

            release_memory_stream(ms);
        }
    }

    #[test]
    fn memory_stream_seek_cur() {
        unsafe {
            let ms = new_memory_stream_from_data(vec![0; 100]);
            (*ms).pos = 30;
            let mut result_pos: int64 = 0;

            let r = ms_seek(ms as *mut IBStream, 20, kIBSeekCur as int32, &mut result_pos);
            assert_eq!(r, kResultOk);
            assert_eq!(result_pos, 50);
            assert_eq!((*ms).pos, 50);

            let r = ms_seek(ms as *mut IBStream, -10, kIBSeekCur as int32, &mut result_pos);
            assert_eq!(r, kResultOk);
            assert_eq!(result_pos, 40);

            release_memory_stream(ms);
        }
    }

    #[test]
    fn memory_stream_seek_end() {
        unsafe {
            let ms = new_memory_stream_from_data(vec![0; 100]);
            let mut result_pos: int64 = 0;

            let r = ms_seek(ms as *mut IBStream, 0, kIBSeekEnd as int32, &mut result_pos);
            assert_eq!(r, kResultOk);
            assert_eq!(result_pos, 100);

            let r = ms_seek(ms as *mut IBStream, -10, kIBSeekEnd as int32, &mut result_pos);
            assert_eq!(r, kResultOk);
            assert_eq!(result_pos, 90);

            release_memory_stream(ms);
        }
    }

    #[test]
    fn memory_stream_seek_negative_result_fails() {
        unsafe {
            let ms = new_memory_stream_from_data(vec![0; 10]);
            let mut result_pos: int64 = 0;

            let r = ms_seek(ms as *mut IBStream, -1, kIBSeekSet as int32, &mut result_pos);
            assert_eq!(r, kInvalidArgument);

            release_memory_stream(ms);
        }
    }

    #[test]
    fn memory_stream_seek_invalid_mode() {
        unsafe {
            let ms = new_memory_stream_from_data(vec![0; 10]);
            let mut result_pos: int64 = 0;

            let r = ms_seek(ms as *mut IBStream, 0, 99, &mut result_pos);
            assert_eq!(r, kInvalidArgument);

            release_memory_stream(ms);
        }
    }

    #[test]
    fn memory_stream_tell() {
        unsafe {
            let ms = new_memory_stream_from_data(vec![0; 50]);
            (*ms).pos = 25;
            let mut pos: int64 = 0;

            let r = ms_tell(ms as *mut IBStream, &mut pos);
            assert_eq!(r, kResultOk);
            assert_eq!(pos, 25);

            release_memory_stream(ms);
        }
    }

    #[test]
    fn memory_stream_write_extends_buffer() {
        unsafe {
            let ms = new_memory_stream();
            let data1 = b"ABC";
            let mut written: int32 = 0;
            ms_write(
                ms as *mut IBStream,
                data1.as_ptr() as *mut c_void,
                3,
                &mut written,
            );
            assert_eq!((*ms).data.len(), 3);
            assert_eq!((*ms).pos, 3);

            (*ms).pos = 10;
            let data2 = b"XY";
            ms_write(
                ms as *mut IBStream,
                data2.as_ptr() as *mut c_void,
                2,
                &mut written,
            );
            assert_eq!((*ms).data.len(), 12);
            assert_eq!((&(*ms).data)[10], b'X');
            assert_eq!((&(*ms).data)[11], b'Y');
            assert_eq!((&(*ms).data)[3], 0);
            assert_eq!((&(*ms).data)[9], 0);

            release_memory_stream(ms);
        }
    }

    #[test]
    fn memory_stream_read_null_buffer() {
        unsafe {
            let ms = new_memory_stream_from_data(vec![1, 2, 3]);
            let r = ms_read(ms as *mut IBStream, std::ptr::null_mut(), 3, std::ptr::null_mut());
            assert_eq!(r, kInvalidArgument);
            release_memory_stream(ms);
        }
    }

    #[test]
    fn memory_stream_write_null_buffer() {
        unsafe {
            let ms = new_memory_stream();
            let r = ms_write(ms as *mut IBStream, std::ptr::null_mut(), 3, std::ptr::null_mut());
            assert_eq!(r, kInvalidArgument);
            release_memory_stream(ms);
        }
    }

    #[test]
    fn memory_stream_read_negative_bytes() {
        unsafe {
            let ms = new_memory_stream_from_data(vec![1, 2, 3]);
            let mut buf = [0u8; 4];
            let r = ms_read(
                ms as *mut IBStream,
                buf.as_mut_ptr() as *mut c_void,
                -1,
                std::ptr::null_mut(),
            );
            assert_eq!(r, kInvalidArgument);
            release_memory_stream(ms);
        }
    }

    #[test]
    fn host_app_get_name_returns_zestbay() {
        unsafe {
            let app = new_host_application(std::ptr::null_mut());
            let mut name: String128 = [0u16; 128];

            let r = host_app_get_name(app as *mut IHostApplication, &mut name);
            assert_eq!(r, kResultOk);

            let len = name.iter().position(|&c| c == 0).unwrap_or(128);
            let s = String::from_utf16(&name[..len]).unwrap();
            assert_eq!(s, "ZestBay");

            release_host_application(app);
        }
    }

    #[test]
    fn host_app_create_instance_returns_not_implemented() {
        unsafe {
            let app = new_host_application(std::ptr::null_mut());
            let mut obj: *mut c_void = std::ptr::null_mut();
            let r = host_app_create_instance(
                app as *mut IHostApplication,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut obj,
            );
            assert_eq!(r, kNotImplemented);
            release_host_application(app);
        }
    }

    #[test]
    fn component_handler_perform_edit() {
        unsafe {
            let port_updates = Arc::new(PortUpdates {
                control_inputs: vec![
                    PortSlot { port_index: 0, value: AtomicF32::new(0.0) },
                    PortSlot { port_index: 1, value: AtomicF32::new(0.5) },
                ],
                control_outputs: Vec::new(),
                atom_outputs: Vec::new(),
                atom_inputs: Vec::new(),
            });

            let shared = Arc::new(Vst3SharedParams::new(port_updates.clone(), vec![200, 100]));
            let ch = new_host_component_handler(42, shared.clone());

            let r = host_ch_perform_edit(ch as *mut IComponentHandler, 100, 0.75);
            assert_eq!(r, kResultOk);
            assert!((port_updates.control_inputs[1].value.load() - 0.75).abs() < 1e-5);
            assert_eq!(
                shared.controller_bits[1].load(Ordering::Relaxed),
                0.75f32.to_bits()
            );

            let r = host_ch_perform_edit(ch as *mut IComponentHandler, 200, 0.25);
            assert_eq!(r, kResultOk);
            assert!((port_updates.control_inputs[0].value.load() - 0.25).abs() < 1e-5);

            let r = host_ch_perform_edit(ch as *mut IComponentHandler, 999, 0.5);
            assert_eq!(r, kResultOk);

            release_host_component_handler(ch);
        }
    }

    #[test]
    fn component_handler_begin_end_edit_are_noop() {
        unsafe {
            let port_updates = Arc::new(PortUpdates {
                control_inputs: Vec::new(),
                control_outputs: Vec::new(),
                atom_outputs: Vec::new(),
                atom_inputs: Vec::new(),
            });
            let shared = Arc::new(Vst3SharedParams::new(port_updates, Vec::new()));
            let ch = new_host_component_handler(1, shared);

            assert_eq!(host_ch_begin_edit(ch as *mut IComponentHandler, 0), kResultOk);
            assert_eq!(host_ch_end_edit(ch as *mut IComponentHandler, 0), kResultOk);

            release_host_component_handler(ch);
        }
    }

    #[test]
    fn component_handler_restart_flags_accumulate() {
        unsafe {
            let port_updates = Arc::new(PortUpdates {
                control_inputs: Vec::new(),
                control_outputs: Vec::new(),
                atom_outputs: Vec::new(),
                atom_inputs: Vec::new(),
            });
            let shared = Arc::new(Vst3SharedParams::new(port_updates, Vec::new()));
            let ch = new_host_component_handler(1, shared.clone());
            let ich = ch as *mut IComponentHandler;
            assert_eq!(host_ch_restart_component(ich, RestartFlags_::kParamValuesChanged), kResultOk);
            assert_eq!(host_ch_restart_component(ich, RestartFlags_::kIoChanged), kResultOk);
            assert_eq!(
                shared.restart_flags.swap(0, Ordering::AcqRel),
                RestartFlags_::kParamValuesChanged | RestartFlags_::kIoChanged
            );
            release_host_component_handler(ch);
        }
    }

    #[test]
    fn run_loop_register_unregister_timer() {
        unsafe {
            let rl = new_host_run_loop();

            let fake_handler1 = 0x1000usize as *mut ITimerHandler;
            let fake_handler2 = 0x2000usize as *mut ITimerHandler;

            let r = host_run_loop_register_timer(rl as *mut IRunLoop, fake_handler1, 100);
            assert_eq!(r, kResultOk);

            let r = host_run_loop_register_timer(rl as *mut IRunLoop, fake_handler2, 200);
            assert_eq!(r, kResultOk);

            assert_eq!((*rl).timers.lock().unwrap().len(), 2);

            let r = host_run_loop_unregister_timer(rl as *mut IRunLoop, fake_handler1);
            assert_eq!(r, kResultOk);
            assert_eq!((*rl).timers.lock().unwrap().len(), 1);

            let r = host_run_loop_unregister_timer(rl as *mut IRunLoop, fake_handler2);
            assert_eq!(r, kResultOk);
            assert_eq!((*rl).timers.lock().unwrap().len(), 0);

            host_run_loop_release(rl as *mut FUnknown);
        }
    }

    #[test]
    fn run_loop_register_unregister_event_handler() {
        unsafe {
            let rl = new_host_run_loop();

            let fake_handler = 0x3000usize as *mut IEventHandler;

            let r = host_run_loop_register_event_handler(rl as *mut IRunLoop, fake_handler, 42);
            assert_eq!(r, kResultOk);
            assert_eq!((*rl).fd_handlers.lock().unwrap().len(), 1);

            let r = host_run_loop_unregister_event_handler(rl as *mut IRunLoop, fake_handler);
            assert_eq!(r, kResultOk);
            assert_eq!((*rl).fd_handlers.lock().unwrap().len(), 0);

            host_run_loop_release(rl as *mut FUnknown);
        }
    }

    #[test]
    fn run_loop_null_timer_handler_rejected() {
        unsafe {
            let rl = new_host_run_loop();
            let r = host_run_loop_register_timer(rl as *mut IRunLoop, std::ptr::null_mut(), 100);
            assert_eq!(r, kInvalidArgument);
            assert_eq!((*rl).timers.lock().unwrap().len(), 0);
            host_run_loop_release(rl as *mut FUnknown);
        }
    }

    #[test]
    fn run_loop_null_event_handler_rejected() {
        unsafe {
            let rl = new_host_run_loop();
            let r = host_run_loop_register_event_handler(rl as *mut IRunLoop, std::ptr::null_mut(), 0);
            assert_eq!(r, kInvalidArgument);
            assert_eq!((*rl).fd_handlers.lock().unwrap().len(), 0);
            host_run_loop_release(rl as *mut FUnknown);
        }
    }

    #[test]
    fn host_application_ref_counting() {
        unsafe {
            let app = new_host_application(std::ptr::null_mut());
            let count = host_app_add_ref(app as *mut FUnknown);
            assert_eq!(count, 2);

            let count = host_app_add_ref(app as *mut FUnknown);
            assert_eq!(count, 3);

            let count = host_app_release(app as *mut FUnknown);
            assert_eq!(count, 2);

            let count = host_app_release(app as *mut FUnknown);
            assert_eq!(count, 1);

            let count = host_app_release(app as *mut FUnknown);
            assert_eq!(count, 0);
        }
    }

    #[test]
    fn memory_stream_ref_counting() {
        unsafe {
            let ms = new_memory_stream();
            let count = ms_add_ref(ms as *mut FUnknown);
            assert_eq!(count, 2);

            let count = ms_release(ms as *mut FUnknown);
            assert_eq!(count, 1);

            let count = ms_release(ms as *mut FUnknown);
            assert_eq!(count, 0);
        }
    }
}
