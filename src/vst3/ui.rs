//! VST3 plugin GUI support — embedded X11 window mode.
//!
//! VST3 plugins provide their GUI via `IEditController::createView()` which
//! returns an `IPlugView`.  We embed the view in a host-created X11 window,
//! similar to the CLAP GUI approach.

use std::collections::HashMap;
use std::os::raw::{c_char, c_int, c_long, c_ulong, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use vst3::Steinberg::*;
use vst3::Steinberg::Linux::IRunLoop_iid;
use vst3::Steinberg::Vst::{IEditController, ViewType};

use crate::plugin::types::PluginInstanceId;

use super::com_host::{HostRunLoop, run_loop_tick};

// ---------------------------------------------------------------------------
// X11 FFI bindings (shared with CLAP — ideally in a common module, but
// duplicated here to keep the VST3 module self-contained)
// ---------------------------------------------------------------------------

#[link(name = "X11")]
unsafe extern "C" {
    fn XOpenDisplay(display_name: *const c_char) -> *mut c_void;
    fn XCloseDisplay(display: *mut c_void) -> c_int;
    fn XCreateSimpleWindow(
        display: *mut c_void,
        parent: c_ulong,
        x: c_int,
        y: c_int,
        width: c_int,
        height: c_int,
        border_width: c_int,
        border: c_ulong,
        background: c_ulong,
    ) -> c_ulong;
    fn XMapWindow(display: *mut c_void, window: c_ulong) -> c_int;
    fn XDestroyWindow(display: *mut c_void, window: c_ulong) -> c_int;
    fn XDefaultRootWindow(display: *mut c_void) -> c_ulong;
    fn XDefaultScreen(display: *mut c_void) -> c_int;
    fn XBlackPixel(display: *mut c_void, screen: c_int) -> c_ulong;
    fn XWhitePixel(display: *mut c_void, screen: c_int) -> c_ulong;
    fn XPending(display: *mut c_void) -> c_int;
    fn XNextEvent(display: *mut c_void, event: *mut [u8; 192]) -> c_int;
    fn XFlush(display: *mut c_void) -> c_int;
    fn XStoreName(display: *mut c_void, window: c_ulong, name: *const c_char) -> c_int;
    fn XResizeWindow(
        display: *mut c_void,
        window: c_ulong,
        width: c_int,
        height: c_int,
    ) -> c_int;
    fn XSelectInput(display: *mut c_void, window: c_ulong, event_mask: c_long) -> c_int;
    fn XInternAtom(
        display: *mut c_void,
        atom_name: *const c_char,
        only_if_exists: c_int,
    ) -> c_ulong;
    fn XSetWMProtocols(
        display: *mut c_void,
        window: c_ulong,
        protocols: *mut c_ulong,
        count: c_int,
    ) -> c_int;
}

const STRUCTURE_NOTIFY_MASK: c_long = 1 << 17;
const SUBSTRUCTURE_NOTIFY_MASK: c_long = 1 << 19;
const EXPOSURE_MASK: c_long = 1 << 15;

// ---------------------------------------------------------------------------
// IPlugFrame implementation (COM object the plugin calls for resize)
// ---------------------------------------------------------------------------

/// Our host-side IPlugFrame.  Lives as long as the GUI is open.
///
/// Layout: the first field is a pointer to the vtable so that a
/// `*mut HostPlugFrame` can be safely cast to `*mut IPlugFrame`.
#[repr(C)]
struct HostPlugFrame {
    vtbl: *const IPlugFrameVtbl,
    ref_count: std::sync::atomic::AtomicU32,
    /// Instance id so we can find the matching Vst3GuiState for resize.
    instance_id: PluginInstanceId,
    /// Pointer to the shared HostRunLoop — returned by queryInterface for
    /// IRunLoop_iid.  Plugins (esp. JUCE-based) query IPlugFrame for IRunLoop.
    run_loop: *mut HostRunLoop,
}

// The static vtable for our HostPlugFrame.
static HOST_PLUG_FRAME_VTBL: IPlugFrameVtbl = IPlugFrameVtbl {
    base: FUnknownVtbl {
        queryInterface: host_plug_frame_query_interface,
        addRef: host_plug_frame_add_ref,
        release: host_plug_frame_release,
    },
    resizeView: host_plug_frame_resize_view,
};

unsafe extern "system" fn host_plug_frame_query_interface(
    this: *mut FUnknown,
    iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    unsafe {
        if iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        let iid_ref = &*iid;
        // Accept FUnknown or IPlugFrame
        if *iid_ref == FUnknown_iid || *iid_ref == IPlugFrame_iid {
            host_plug_frame_add_ref(this);
            *obj = this as *mut c_void;
            return kResultOk;
        }
        // Forward IRunLoop queries to our run loop
        if *iid_ref == IRunLoop_iid {
            let frame = this as *mut HostPlugFrame;
            if !(*frame).run_loop.is_null() {
                super::com_host::host_run_loop_add_ref((*frame).run_loop as *mut FUnknown);
                *obj = (*frame).run_loop as *mut c_void;
                return kResultOk;
            }
        }
        *obj = std::ptr::null_mut();
        kNoInterface
    }
}

unsafe extern "system" fn host_plug_frame_add_ref(this: *mut FUnknown) -> uint32 {
    unsafe {
        let frame = this as *mut HostPlugFrame;
        let old = (*frame)
            .ref_count
            .fetch_add(1, Ordering::Relaxed);
        old + 1
    }
}

unsafe extern "system" fn host_plug_frame_release(this: *mut FUnknown) -> uint32 {
    unsafe {
        let frame = this as *mut HostPlugFrame;
        let old = (*frame).ref_count.fetch_sub(1, Ordering::Relaxed);
        if old == 1 {
            // Last reference — deallocate
            drop(Box::from_raw(frame));
            return 0;
        }
        old - 1
    }
}

unsafe extern "system" fn host_plug_frame_resize_view(
    this: *mut IPlugFrame,
    view: *mut IPlugView,
    new_size: *mut ViewRect,
) -> tresult {
    unsafe {
        if new_size.is_null() {
            return kInvalidArgument;
        }
        let frame = this as *mut HostPlugFrame;
        let instance_id = (*frame).instance_id;
        let rect = &*new_size;
        let w = (rect.right - rect.left).max(1);
        let h = (rect.bottom - rect.top).max(1);

        log::debug!(
            "VST3 IPlugFrame::resizeView {}x{} (instance {})",
            w,
            h,
            instance_id
        );

        // Resize the X11 host window
        with_guis(|m| {
            if let Some(state) = m.get(&instance_id) {
                if !state.x11_display.is_null() && state.x11_window != 0 {
                    XResizeWindow(state.x11_display, state.x11_window, w, h);
                    XFlush(state.x11_display);
                }
            }
        });

        // Notify the plugin view of the new size
        if !view.is_null() {
            ((*(*view).vtbl).onSize)(view, new_size);
        }

        kResultOk
    }
}

/// Allocate a new HostPlugFrame (ref_count starts at 1).
fn new_host_plug_frame(instance_id: PluginInstanceId, run_loop: *mut HostRunLoop) -> *mut HostPlugFrame {
    let frame = Box::new(HostPlugFrame {
        vtbl: &HOST_PLUG_FRAME_VTBL,
        ref_count: std::sync::atomic::AtomicU32::new(1),
        instance_id,
        run_loop,
    });
    Box::into_raw(frame)
}

// ---------------------------------------------------------------------------
// GUI state registry
// ---------------------------------------------------------------------------

struct Vst3GuiState {
    #[allow(dead_code)]
    instance_id: PluginInstanceId,
    view: *mut IPlugView,
    plug_frame: *mut HostPlugFrame,
    x11_display: *mut c_void,
    x11_window: c_ulong,
    running: std::sync::Arc<AtomicBool>,
    run_loop: *mut HostRunLoop,
}

unsafe impl Send for Vst3GuiState {}

static OPEN_GUIS: Mutex<Option<HashMap<PluginInstanceId, Vst3GuiState>>> = Mutex::new(None);

fn with_guis<F, R>(f: F) -> R
where
    F: FnOnce(&mut HashMap<PluginInstanceId, Vst3GuiState>) -> R,
{
    let mut lock = OPEN_GUIS.lock().unwrap();
    let map = lock.get_or_insert_with(HashMap::new);
    f(map)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Open a VST3 plugin GUI in an embedded X11 window.
///
/// `controller_ptr` must be a valid `*mut IEditController` with a live refcount.
///
/// # Safety
/// Called from the PW thread.
pub unsafe fn open_vst3_gui(
    controller_ptr: *mut IEditController,
    instance_id: PluginInstanceId,
    display_name: &str,
    event_tx: &std::sync::mpsc::Sender<crate::pipewire::PwEvent>,
    cmd_tx: &std::sync::mpsc::Sender<crate::pipewire::PwCommand>,
) {
    // Already open?
    let already = with_guis(|m| m.contains_key(&instance_id));
    if already {
        log::warn!("VST3 GUI already open for instance {}", instance_id);
        return;
    }

    if controller_ptr.is_null() {
        log::warn!("VST3: no IEditController for instance {}", instance_id);
        return;
    }

    unsafe {
        // Call createView via vtable (raw pointer, no SmartPtr wrapper)
        let view = ((*(*controller_ptr).vtbl).createView)(controller_ptr, ViewType::kEditor);
        if view.is_null() {
            log::warn!(
                "VST3: createView returned null for instance {}",
                instance_id
            );
            return;
        }

        // Check X11 support
        let supported =
            ((*(*view).vtbl).isPlatformTypeSupported)(view, kPlatformTypeX11EmbedWindowID);
        if supported != kResultOk {
            log::warn!(
                "VST3: plugin does not support X11EmbedWindowID (instance {})",
                instance_id
            );
            ((*(*view).vtbl).base.release)(view as *mut FUnknown);
            return;
        }

        // Create a run loop for this GUI (for timer/fd registration)
        let run_loop = super::com_host::new_host_run_loop();

        // Create our IPlugFrame (with run_loop for IRunLoop QI)
        let plug_frame = new_host_plug_frame(instance_id, run_loop);

        // setFrame
        ((*(*view).vtbl).setFrame)(view, plug_frame as *mut IPlugFrame);

        // getSize
        let mut rect: ViewRect = std::mem::zeroed();
        let size_ok = ((*(*view).vtbl).getSize)(view, &mut rect);
        let (width, height) = if size_ok == kResultOk {
            (
                (rect.right - rect.left).max(100),
                (rect.bottom - rect.top).max(100),
            )
        } else {
            (800, 600)
        };

        log::info!(
            "VST3 GUI size: {}x{} (instance {})",
            width,
            height,
            instance_id
        );

        // Open X11 display
        let display = XOpenDisplay(std::ptr::null());
        if display.is_null() {
            log::error!("VST3: cannot open X11 display");
            ((*(*view).vtbl).setFrame)(view, std::ptr::null_mut());
            ((*(*view).vtbl).base.release)(view as *mut FUnknown);
            host_plug_frame_release(plug_frame as *mut FUnknown);
            return;
        }

        let screen = XDefaultScreen(display);
        let root = XDefaultRootWindow(display);

        let window = XCreateSimpleWindow(
            display,
            root,
            0,
            0,
            width as c_int,
            height as c_int,
            0,
            XBlackPixel(display, screen),
            XWhitePixel(display, screen),
        );

        if window == 0 {
            log::error!("VST3: XCreateSimpleWindow failed");
            XCloseDisplay(display);
            ((*(*view).vtbl).setFrame)(view, std::ptr::null_mut());
            ((*(*view).vtbl).base.release)(view as *mut FUnknown);
            host_plug_frame_release(plug_frame as *mut FUnknown);
            return;
        }

        let title = std::ffi::CString::new(display_name).unwrap_or_default();
        XStoreName(display, window, title.as_ptr());

        XSelectInput(
            display,
            window,
            STRUCTURE_NOTIFY_MASK | SUBSTRUCTURE_NOTIFY_MASK | EXPOSURE_MASK,
        );

        let wm_delete = XInternAtom(display, c"WM_DELETE_WINDOW".as_ptr(), 0);
        let mut protocols = [wm_delete];
        XSetWMProtocols(display, window, protocols.as_mut_ptr(), 1);

        XMapWindow(display, window);
        XFlush(display);

        // Attach the plugin view to our X11 window
        let attached = ((*(*view).vtbl).attached)(
            view,
            window as *mut c_void,
            kPlatformTypeX11EmbedWindowID,
        );
        if attached != kResultOk {
            log::error!(
                "VST3: IPlugView::attached failed (instance {})",
                instance_id
            );
            XDestroyWindow(display, window);
            XCloseDisplay(display);
            ((*(*view).vtbl).setFrame)(view, std::ptr::null_mut());
            ((*(*view).vtbl).base.release)(view as *mut FUnknown);
            host_plug_frame_release(plug_frame as *mut FUnknown);
            return;
        }

        let running = std::sync::Arc::new(AtomicBool::new(true));

        // Register state
        with_guis(|m| {
            m.insert(
                instance_id,
                Vst3GuiState {
                    instance_id,
                    view,
                    plug_frame,
                    x11_display: display,
                    x11_window: window,
                    running: running.clone(),
                    run_loop,
                },
            );
        });

        // Spawn X11 event loop thread
        let display_addr = display as usize;
        let run_loop_addr = run_loop as usize;
        let cmd_tx_clone = cmd_tx.clone();
        let running_clone = running.clone();
        std::thread::Builder::new()
            .name(format!("vst3-gui-{}", instance_id))
            .spawn(move || {
                let display = display_addr as *mut c_void;
                let run_loop = run_loop_addr as *mut HostRunLoop;
                x11_event_loop(
                    display,
                    window,
                    wm_delete,
                    instance_id,
                    running_clone,
                    cmd_tx_clone,
                    run_loop,
                );
            })
            .ok();

        log::info!(
            "VST3 embedded X11 GUI opened for instance {}",
            instance_id
        );
        let _ = event_tx.send(crate::pipewire::PwEvent::Plugin(
            crate::pipewire::PluginEvent::PluginUiOpened { instance_id },
        ));
    }
}

/// X11 event loop for a VST3 GUI window.
fn x11_event_loop(
    display: *mut c_void,
    _window: c_ulong,
    wm_delete_atom: c_ulong,
    instance_id: PluginInstanceId,
    running: std::sync::Arc<AtomicBool>,
    cmd_tx: std::sync::mpsc::Sender<crate::pipewire::PwCommand>,
    run_loop: *mut HostRunLoop,
) {
    let tick = std::time::Duration::from_millis(8);

    while running.load(Ordering::Acquire) {
        unsafe {
            // Service IRunLoop timers and fd event handlers
            run_loop_tick(run_loop);

            while XPending(display) > 0 {
                let mut event = std::mem::zeroed::<[u8; 192]>();
                XNextEvent(display, &mut event);

                let event_type = *(event.as_ptr() as *const i32);
                if event_type == 33 {
                    // ClientMessage
                    let atom = *((event.as_ptr() as *const u8).add(56) as *const c_ulong);
                    if atom == wm_delete_atom {
                        log::info!(
                            "VST3 GUI window close requested (instance {})",
                            instance_id
                        );
                        running.store(false, Ordering::Release);
                        let _ = cmd_tx.send(crate::pipewire::PwCommand::ClosePluginUI {
                            instance_id,
                        });
                        break;
                    }
                }
            }
        }
        std::thread::sleep(tick);
    }

    log::debug!("VST3 X11 event loop exiting for instance {}", instance_id);
}

/// Close a VST3 plugin GUI if open.
///
/// Must be called from the main (PW) thread.
pub fn close_vst3_gui(
    instance_id: PluginInstanceId,
    event_tx: &std::sync::mpsc::Sender<crate::pipewire::PwEvent>,
) {
    let state = with_guis(|m| m.remove(&instance_id));

    if let Some(state) = state {
        state.running.store(false, Ordering::Release);

        unsafe {
            // Detach the view
            ((*(*state.view).vtbl).removed)(state.view);

            // Clear the frame reference
            ((*(*state.view).vtbl).setFrame)(state.view, std::ptr::null_mut());

            // Release the view
            ((*(*state.view).vtbl).base.release)(state.view as *mut FUnknown);

            // Release our plug frame
            host_plug_frame_release(state.plug_frame as *mut FUnknown);

            // Release the run loop
            if !state.run_loop.is_null() {
                super::com_host::host_run_loop_release(state.run_loop as *mut FUnknown);
            }

            // Clean up X11
            if !state.x11_display.is_null() && state.x11_window != 0 {
                std::thread::sleep(std::time::Duration::from_millis(50));
                XDestroyWindow(state.x11_display, state.x11_window);
                XCloseDisplay(state.x11_display);
            }
        }

        let _ = event_tx.send(crate::pipewire::PwEvent::Plugin(
            crate::pipewire::PluginEvent::PluginUiClosed { instance_id },
        ));

        log::info!("VST3 GUI closed for instance {}", instance_id);
    }
}
