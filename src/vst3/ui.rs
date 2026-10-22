//! VST3 plugin GUI support — embedded X11 window mode.
//!
//! VST3 plugins provide their GUI via `IEditController::createView()` which
//! returns an `IPlugView`.  We embed the view in a host-created X11 window,
//! similar to the CLAP GUI approach.
//!
//! Threading: the PipeWire thread is the VST3 UI thread.  The view is
//! created, attached, resized and removed there, and the per-view IRunLoop
//! (timers + fd handlers) and host-window X11 events are serviced there from
//! [`idle`], which the PipeWire manager calls on every timer tick.  There is
//! no GUI helper thread, so when [`close_vst3_gui`] returns nothing for that
//! view can still run.

use std::collections::HashMap;
use std::os::raw::c_void;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use vst3::Steinberg::Linux::IRunLoop_iid;
use vst3::Steinberg::Vst::{IEditController, ViewType};
use vst3::Steinberg::*;

use crate::clap::threading;
use crate::clap::x11::HostWindow;
use crate::plugin::types::PluginInstanceId;

use super::com_host::{HostRunLoop, run_loop_clear, run_loop_tick};

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
        let old = (*frame).ref_count.fetch_add(1, Ordering::Relaxed);
        old + 1
    }
}

unsafe extern "system" fn host_plug_frame_release(this: *mut FUnknown) -> uint32 {
    unsafe {
        let frame = this as *mut HostPlugFrame;
        let old = (*frame).ref_count.fetch_sub(1, Ordering::AcqRel);
        if old == 1 {
            // Last reference — drop our run loop ref and deallocate
            let frame = Box::from_raw(frame);
            if !frame.run_loop.is_null() {
                super::com_host::host_run_loop_release(frame.run_loop as *mut FUnknown);
            }
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
        let rect = *new_size;
        let w = (rect.right - rect.left).max(1) as u32;
        let h = (rect.bottom - rect.top).max(1) as u32;

        log::debug!(
            "VST3 IPlugFrame::resizeView {}x{} (instance {})",
            w,
            h,
            instance_id
        );

        if !threading::is_main_thread() {
            // Applied (with onSize) by the next `idle` on the thread that
            // owns the Display.
            let known = with_guis(|m| match m.get_mut(&instance_id) {
                Some(s) => {
                    s.pending_size = Some(rect);
                    true
                }
                None => false,
            });
            return if known { kResultOk } else { kResultFalse };
        }

        let known = with_guis(|m| match m.get(&instance_id) {
            Some(s) => {
                s.window.resize(w, h);
                true
            }
            None => false,
        });
        if !known {
            return kResultFalse;
        }
        // Notify the plugin view of the new size (outside the registry lock).
        if !view.is_null() {
            let mut r = rect;
            ((*(*view).vtbl).onSize)(view, &mut r);
        }
        kResultOk
    }
}

/// Allocate a new HostPlugFrame (ref_count starts at 1).  Takes its own
/// reference on `run_loop`.
fn new_host_plug_frame(instance_id: PluginInstanceId, run_loop: *mut HostRunLoop) -> *mut HostPlugFrame {
    if !run_loop.is_null() {
        unsafe {
            super::com_host::host_run_loop_add_ref(run_loop as *mut FUnknown);
        }
    }
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
    controller: *mut IEditController,
    view: *mut IPlugView,
    plug_frame: *mut HostPlugFrame,
    run_loop: *mut HostRunLoop,
    window: HostWindow,
    event_tx: std::sync::mpsc::Sender<crate::pipewire::PwEvent>,
    /// Resize requested off the main thread, applied by `idle`.
    pending_size: Option<ViewRect>,
}

unsafe impl Send for Vst3GuiState {}

/// Never hold this lock while calling into plugin code.
static OPEN_GUIS: Mutex<Option<HashMap<PluginInstanceId, Vst3GuiState>>> = Mutex::new(None);

fn with_guis<F, R>(f: F) -> R
where
    F: FnOnce(&mut HashMap<PluginInstanceId, Vst3GuiState>) -> R,
{
    let mut lock = OPEN_GUIS.lock().unwrap_or_else(|e| e.into_inner());
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
/// Called from the PW thread (the VST3 UI thread).
pub unsafe fn open_vst3_gui(
    controller_ptr: *mut IEditController,
    instance_id: PluginInstanceId,
    display_name: &str,
    event_tx: &std::sync::mpsc::Sender<crate::pipewire::PwEvent>,
    _cmd_tx: &std::sync::mpsc::Sender<crate::pipewire::PwCommand>,
) {
    threading::set_main_thread();

    if with_guis(|m| m.contains_key(&instance_id)) {
        log::warn!("VST3 GUI already open for instance {}", instance_id);
        return;
    }

    if controller_ptr.is_null() {
        log::warn!("VST3: no IEditController for instance {}", instance_id);
        return;
    }

    unsafe {
        let view = ((*(*controller_ptr).vtbl).createView)(controller_ptr, ViewType::kEditor);
        if view.is_null() {
            log::warn!(
                "VST3: createView returned null for '{}' (instance {}). \
                 This plugin may not support embedded UI.",
                display_name,
                instance_id,
            );
            return;
        }

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

        // Run loop for this GUI (timers/fds), handed out via IPlugFrame.
        let run_loop = super::com_host::new_host_run_loop();
        let plug_frame = new_host_plug_frame(instance_id, run_loop);
        ((*(*view).vtbl).setFrame)(view, plug_frame as *mut IPlugFrame);

        let mut rect: ViewRect = std::mem::zeroed();
        let (width, height) = if ((*(*view).vtbl).getSize)(view, &mut rect) == kResultOk {
            (
                (rect.right - rect.left).max(100) as u32,
                (rect.bottom - rect.top).max(100) as u32,
            )
        } else {
            (800, 600)
        };
        log::info!("VST3 GUI size: {}x{} (instance {})", width, height, instance_id);

        let Some(window) = HostWindow::open(display_name, width, height) else {
            release_view(view, plug_frame, run_loop, false);
            return;
        };
        let x11_window = window.window;

        // Register before attached() so resizeView during attach finds us.
        with_guis(|m| {
            m.insert(
                instance_id,
                Vst3GuiState {
                    controller: controller_ptr,
                    view,
                    plug_frame,
                    run_loop,
                    window,
                    event_tx: event_tx.clone(),
                    pending_size: None,
                },
            );
        });

        let attached = ((*(*view).vtbl).attached)(
            view,
            x11_window as *mut c_void,
            kPlatformTypeX11EmbedWindowID,
        );
        if attached != kResultOk {
            log::error!("VST3: IPlugView::attached failed (instance {})", instance_id);
            if let Some(state) = with_guis(|m| m.remove(&instance_id)) {
                let window = state.window;
                release_view(state.view, state.plug_frame, state.run_loop, false);
                drop(window);
            }
            return;
        }

        with_guis(|m| {
            if let Some(s) = m.get(&instance_id) {
                s.window.flush();
            }
        });

        log::info!("VST3 embedded X11 GUI opened for instance {}", instance_id);
        let _ = event_tx.send(crate::pipewire::PwEvent::Plugin(
            crate::pipewire::PluginEvent::PluginUiOpened { instance_id },
        ));
    }
}

/// removed() (if attached), clear the frame, release the view, the frame
/// and our run loop reference.  Leftover run-loop handlers are dropped
/// without being called.
unsafe fn release_view(
    view: *mut IPlugView,
    plug_frame: *mut HostPlugFrame,
    run_loop: *mut HostRunLoop,
    attached: bool,
) {
    unsafe {
        if attached {
            ((*(*view).vtbl).removed)(view);
        }
        ((*(*view).vtbl).setFrame)(view, std::ptr::null_mut());
        ((*(*view).vtbl).base.release)(view as *mut FUnknown);
        run_loop_clear(run_loop);
        host_plug_frame_release(plug_frame as *mut FUnknown);
        if !run_loop.is_null() {
            super::com_host::host_run_loop_release(run_loop as *mut FUnknown);
        }
    }
}

/// Tear down a registered GUI: view first, then the X11 window.
unsafe fn teardown(state: Vst3GuiState) {
    unsafe {
        release_view(state.view, state.plug_frame, state.run_loop, true);
    }
    drop(state.window);
}

/// Service all VST3 GUIs: run-loop timers/fds, pending resizes and host
/// window events.  Must be called on the PipeWire thread on every manager
/// tick.
pub fn idle() {
    if !threading::is_main_thread() {
        return;
    }
    let ids: Vec<PluginInstanceId> = with_guis(|m| m.keys().copied().collect());
    let mut to_close = Vec::new();
    for id in ids {
        let Some((view, run_loop, pending)) = with_guis(|m| {
            m.get_mut(&id)
                .map(|s| (s.view, s.run_loop, s.pending_size.take()))
        }) else {
            continue;
        };

        unsafe {
            run_loop_tick(run_loop);

            if let Some(mut rect) = pending {
                let w = (rect.right - rect.left).max(1) as u32;
                let h = (rect.bottom - rect.top).max(1) as u32;
                with_guis(|m| {
                    if let Some(s) = m.get(&id) {
                        s.window.resize(w, h);
                    }
                });
                ((*(*view).vtbl).onSize)(view, &mut rect);
            }
        }

        let close = with_guis(|m| match m.get_mut(&id) {
            Some(s) => unsafe { s.window.pump() },
            None => false,
        });
        if close {
            log::info!("VST3 GUI window close requested (instance {})", id);
            to_close.push(id);
        }
    }
    for id in to_close {
        if let Some(state) = with_guis(|m| m.remove(&id)) {
            let tx = state.event_tx.clone();
            unsafe { teardown(state) };
            let _ = tx.send(crate::pipewire::PwEvent::Plugin(
                crate::pipewire::PluginEvent::PluginUiClosed { instance_id: id },
            ));
            log::info!("VST3 GUI closed for instance {}", id);
        }
    }
}

/// Close a VST3 plugin GUI if open.
///
/// Must be called from the main (PW) thread.  When this returns the view is
/// removed and released and no run-loop callback for it can still run.
pub fn close_vst3_gui(
    instance_id: PluginInstanceId,
    event_tx: &std::sync::mpsc::Sender<crate::pipewire::PwEvent>,
) {
    if let Some(state) = with_guis(|m| m.remove(&instance_id)) {
        unsafe { teardown(state) };
        let _ = event_tx.send(crate::pipewire::PwEvent::Plugin(
            crate::pipewire::PluginEvent::PluginUiClosed { instance_id },
        ));
        log::info!("VST3 GUI closed for instance {}", instance_id);
    }
}

/// Close any GUI still open for `controller` (used when the instance is dropped).
pub(crate) fn force_close_for_controller(controller: *mut IEditController) {
    if controller.is_null() {
        return;
    }
    let found = with_guis(|m| {
        let id = m
            .iter()
            .find(|(_, s)| std::ptr::eq(s.controller, controller))
            .map(|(id, _)| *id)?;
        m.remove(&id).map(|s| (id, s))
    });
    if let Some((id, state)) = found {
        let tx = state.event_tx.clone();
        unsafe { teardown(state) };
        let _ = tx.send(crate::pipewire::PwEvent::Plugin(
            crate::pipewire::PluginEvent::PluginUiClosed { instance_id: id },
        ));
        log::info!("VST3 GUI closed for instance {} (plugin dropped)", id);
    }
}
