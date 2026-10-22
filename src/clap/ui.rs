//! CLAP plugin GUI support — embedded X11 window mode.
//!
//! CLAP plugins that support the `clap.gui` extension are embedded in a
//! host-created X11 parent window.  Most Linux CLAP plugins (JUCE-based like
//! Surge XT) only support embedded mode (`is_api_supported(X11, false)`).
//!
//! Threading: the PipeWire thread is the CLAP main thread.  GUI calls,
//! timer callbacks (`clap.timer-support`), fd callbacks
//! (`clap.posix-fd-support`) and host-window X11 events are all serviced on
//! it from [`idle`], which the PipeWire manager calls on every timer tick.
//! There are no helper threads, so once [`close_clap_gui`] (or the plugin
//! instance's drop) returns no callback for that plugin can still run.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::plugin::types::PluginInstanceId;

use super::threading;
use super::x11::HostWindow;

type ClapPlugin = clap_sys::plugin::clap_plugin;

// ---------------------------------------------------------------------------
// GUI state registry
// ---------------------------------------------------------------------------

/// Per-instance state for an open CLAP GUI (always embedded X11).
struct ClapGuiState {
    plugin: *const ClapPlugin,
    gui_ext: *const clap_sys::ext::gui::clap_plugin_gui,
    window: HostWindow,
    event_tx: std::sync::mpsc::Sender<crate::pipewire::PwEvent>,
    /// Resize requested by the plugin, applied on the main thread.
    pending_size: Option<(u32, u32)>,
    /// The plugin reported its GUI closed (`clap_host_gui.closed`).
    close_requested: bool,
}

unsafe impl Send for ClapGuiState {}

/// Global registry of open CLAP GUIs, keyed by instance_id.
/// Never hold this lock while calling into plugin code.
static OPEN_GUIS: Mutex<Option<HashMap<PluginInstanceId, ClapGuiState>>> = Mutex::new(None);

fn with_guis<F, R>(f: F) -> R
where
    F: FnOnce(&mut HashMap<PluginInstanceId, ClapGuiState>) -> R,
{
    let mut lock = OPEN_GUIS.lock().unwrap_or_else(|e| e.into_inner());
    let map = lock.get_or_insert_with(HashMap::new);
    f(map)
}

// ---------------------------------------------------------------------------
// Timer / fd registries (serviced from `idle` on the main thread)
// ---------------------------------------------------------------------------

struct TimerEntry {
    plugin: *const ClapPlugin,
    period: Duration,
    next_due: Instant,
}

unsafe impl Send for TimerEntry {}

struct FdEntry {
    plugin: *const ClapPlugin,
    fd: i32,
    flags: u32,
}

unsafe impl Send for FdEntry {}

/// Timer IDs are globally unique across all plugin instances.
static TIMERS: Mutex<Option<HashMap<u32, TimerEntry>>> = Mutex::new(None);
static NEXT_TIMER_ID: Mutex<u32> = Mutex::new(1);
static FDS: Mutex<Vec<FdEntry>> = Mutex::new(Vec::new());

fn with_timers<F, R>(f: F) -> R
where
    F: FnOnce(&mut HashMap<u32, TimerEntry>) -> R,
{
    let mut lock = TIMERS.lock().unwrap_or_else(|e| e.into_inner());
    let map = lock.get_or_insert_with(HashMap::new);
    f(map)
}

fn with_fds<F, R>(f: F) -> R
where
    F: FnOnce(&mut Vec<FdEntry>) -> R,
{
    let mut lock = FDS.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut lock)
}

/// Remove every timer and fd registered by `plugin`.  After this returns no
/// timer/fd callback for the plugin will be invoked.
pub(crate) fn remove_plugin_callbacks(plugin: *const ClapPlugin) {
    with_timers(|t| t.retain(|_, e| !std::ptr::eq(e.plugin, plugin)));
    with_fds(|f| f.retain(|e| !std::ptr::eq(e.plugin, plugin)));
}

// ---------------------------------------------------------------------------
// Main-thread pump
// ---------------------------------------------------------------------------

/// Service all CLAP GUIs, timers and fd handlers.  Must be called on the
/// PipeWire (CLAP main) thread on every manager tick.
pub fn idle() {
    if !threading::is_main_thread() {
        return;
    }
    pump_windows();
    fire_timers();
    poll_fds();
}

fn pump_windows() {
    let ids: Vec<PluginInstanceId> = with_guis(|m| m.keys().copied().collect());
    let mut to_close = Vec::new();
    for id in ids {
        let close = with_guis(|m| {
            let Some(s) = m.get_mut(&id) else {
                return false;
            };
            unsafe {
                if let Some((w, h)) = s.pending_size.take() {
                    s.window.resize(w, h);
                }
                let user_close = s.window.pump();
                if user_close {
                    log::info!("CLAP GUI window close requested (instance {})", id);
                }
                user_close || s.close_requested
            }
        });
        if close {
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
            log::info!("CLAP GUI closed for instance {}", id);
        }
    }
}

fn fire_timers() {
    let now = Instant::now();
    let due: Vec<u32> = with_timers(|timers| {
        let mut due = Vec::new();
        for (&id, e) in timers.iter_mut() {
            if now >= e.next_due {
                e.next_due = now + e.period;
                due.push(id);
            }
        }
        due
    });
    for id in due {
        // Re-check: an earlier callback may have unregistered this timer.
        let Some(plugin) = with_timers(|t| t.get(&id).map(|e| e.plugin)) else {
            continue;
        };
        unsafe {
            let ext = plugin_ext::<clap_sys::ext::timer_support::clap_plugin_timer_support>(
                plugin,
                clap_sys::ext::timer_support::CLAP_EXT_TIMER_SUPPORT,
            );
            if let Some(on_timer) = ext.and_then(|e| e.on_timer) {
                on_timer(plugin, id);
            }
        }
    }
}

fn poll_fds() {
    use clap_sys::ext::posix_fd_support::*;

    let entries: Vec<(*const ClapPlugin, i32, u32)> =
        with_fds(|f| f.iter().map(|e| (e.plugin, e.fd, e.flags)).collect());
    if entries.is_empty() {
        return;
    }
    let mut pfds: Vec<libc::pollfd> = entries
        .iter()
        .map(|&(_, fd, flags)| {
            let mut events: libc::c_short = 0;
            if flags & CLAP_POSIX_FD_READ != 0 {
                events |= libc::POLLIN;
            }
            if flags & CLAP_POSIX_FD_WRITE != 0 {
                events |= libc::POLLOUT;
            }
            libc::pollfd {
                fd,
                events,
                revents: 0,
            }
        })
        .collect();
    let ready = unsafe { libc::poll(pfds.as_mut_ptr(), pfds.len() as libc::nfds_t, 0) };
    if ready <= 0 {
        return;
    }
    for (pfd, &(plugin, fd, _)) in pfds.iter().zip(entries.iter()) {
        if pfd.revents == 0 {
            continue;
        }
        let still_registered =
            with_fds(|f| f.iter().any(|e| std::ptr::eq(e.plugin, plugin) && e.fd == fd));
        if !still_registered {
            continue;
        }
        let mut flags = 0;
        if pfd.revents & libc::POLLIN != 0 {
            flags |= CLAP_POSIX_FD_READ;
        }
        if pfd.revents & libc::POLLOUT != 0 {
            flags |= CLAP_POSIX_FD_WRITE;
        }
        if pfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            flags |= CLAP_POSIX_FD_ERROR;
        }
        unsafe {
            let ext = plugin_ext::<clap_plugin_posix_fd_support>(plugin, CLAP_EXT_POSIX_FD_SUPPORT);
            if let Some(on_fd) = ext.and_then(|e| e.on_fd) {
                on_fd(plugin, fd, flags);
            }
        }
    }
}

unsafe fn plugin_ext<'a, T>(plugin: *const ClapPlugin, id: &std::ffi::CStr) -> Option<&'a T> {
    unsafe {
        let get_ext = (*plugin).get_extension?;
        let ext = get_ext(plugin, id.as_ptr());
        if ext.is_null() {
            None
        } else {
            Some(&*(ext as *const T))
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Open a CLAP plugin GUI in an embedded X11 window.
///
/// # Safety
/// `plugin_ptr` must be a valid, live CLAP plugin pointer.  Must be called
/// on the PipeWire (CLAP main) thread.
pub unsafe fn open_clap_gui(
    plugin_ptr: *const ClapPlugin,
    instance_id: PluginInstanceId,
    display_name: &str,
    event_tx: &std::sync::mpsc::Sender<crate::pipewire::PwEvent>,
    _cmd_tx: &std::sync::mpsc::Sender<crate::pipewire::PwCommand>,
) {
    threading::set_main_thread();

    if with_guis(|m| m.contains_key(&instance_id)) {
        log::warn!("CLAP GUI already open for instance {}", instance_id);
        return;
    }

    unsafe {
        let Some(gui) = plugin_ext::<clap_sys::ext::gui::clap_plugin_gui>(
            plugin_ptr,
            clap_sys::ext::gui::CLAP_EXT_GUI,
        ) else {
            log::warn!(
                "CLAP plugin does not support gui extension (instance {})",
                instance_id
            );
            return;
        };

        let supported = match gui.is_api_supported {
            Some(f) => f(plugin_ptr, clap_sys::ext::gui::CLAP_WINDOW_API_X11.as_ptr(), false),
            None => false,
        };
        if !supported {
            log::warn!(
                "CLAP plugin does not support embedded X11 GUI (instance {})",
                instance_id
            );
            return;
        }

        let created = with_current_plugin(plugin_ptr, || match gui.create {
            Some(create_fn) => create_fn(
                plugin_ptr,
                clap_sys::ext::gui::CLAP_WINDOW_API_X11.as_ptr(),
                false,
            ),
            None => false,
        });
        if !created {
            log::warn!(
                "CLAP gui.create (embedded) returned false (instance {})",
                instance_id
            );
            destroy_plugin_gui(plugin_ptr, gui, false);
            return;
        }

        let mut width: u32 = 800;
        let mut height: u32 = 600;
        if let Some(get_size) = gui.get_size {
            if !get_size(plugin_ptr, &mut width, &mut height) {
                width = 800;
                height = 600;
            }
        }
        log::info!(
            "CLAP embedded GUI size: {}x{} (instance {})",
            width,
            height,
            instance_id
        );

        let Some(window) = HostWindow::open(display_name, width, height) else {
            destroy_plugin_gui(plugin_ptr, gui, true);
            return;
        };
        let x11_window = window.window;

        // Register before set_parent/show so resize requests made during
        // embedding find the window.
        with_guis(|m| {
            m.insert(
                instance_id,
                ClapGuiState {
                    plugin: plugin_ptr,
                    gui_ext: gui as *const _,
                    window,
                    event_tx: event_tx.clone(),
                    pending_size: None,
                    close_requested: false,
                },
            );
        });

        let clap_window = clap_sys::ext::gui::clap_window {
            api: clap_sys::ext::gui::CLAP_WINDOW_API_X11.as_ptr(),
            specific: clap_sys::ext::gui::clap_window_handle { x11: x11_window },
        };
        let set_ok = match gui.set_parent {
            Some(set_parent) => set_parent(plugin_ptr, &clap_window),
            None => false,
        };
        if !set_ok {
            log::error!("CLAP gui.set_parent failed (instance {})", instance_id);
            if let Some(state) = with_guis(|m| m.remove(&instance_id)) {
                teardown(state);
            }
            return;
        }

        if let Some(show_fn) = gui.show {
            show_fn(plugin_ptr);
        }
        with_guis(|m| {
            if let Some(s) = m.get(&instance_id) {
                s.window.flush();
            }
        });

        log::info!("CLAP embedded X11 GUI opened for instance {}", instance_id);
        let _ = event_tx.send(crate::pipewire::PwEvent::Plugin(
            crate::pipewire::PluginEvent::PluginUiOpened { instance_id },
        ));
    }
}

/// Hide/destroy the plugin GUI.  Timers and fds the plugin registered stay:
/// it may use them for non-GUI work and must unregister GUI ones itself.
/// They are force-removed when the instance is dropped.
unsafe fn destroy_plugin_gui(
    plugin: *const ClapPlugin,
    gui: &clap_sys::ext::gui::clap_plugin_gui,
    hide: bool,
) {
    unsafe {
        if hide {
            if let Some(hide_fn) = gui.hide {
                hide_fn(plugin);
            }
        }
        if let Some(destroy_fn) = gui.destroy {
            destroy_fn(plugin);
        }
    }
}

/// Destroy the plugin GUI, then the host window.  The state must already
/// be removed from the registry.
unsafe fn teardown(state: ClapGuiState) {
    unsafe {
        destroy_plugin_gui(state.plugin, &*state.gui_ext, true);
    }
    drop(state.window);
}

/// Close a CLAP plugin GUI if it is open.
///
/// Must be called from the main (PW) thread.  When this returns the GUI is
/// destroyed and no GUI callback for the plugin can still run.  Timers/fds
/// the plugin still has registered stay until the instance is dropped.
pub fn close_clap_gui(
    instance_id: PluginInstanceId,
    event_tx: &std::sync::mpsc::Sender<crate::pipewire::PwEvent>,
) {
    if let Some(state) = with_guis(|m| m.remove(&instance_id)) {
        unsafe { teardown(state) };
        let _ = event_tx.send(crate::pipewire::PwEvent::Plugin(
            crate::pipewire::PluginEvent::PluginUiClosed { instance_id },
        ));
        log::info!("CLAP GUI closed for instance {}", instance_id);
    }
}

/// Close any GUI still open for `plugin` (used when the instance is dropped).
pub(crate) fn force_close_for_plugin(plugin: *const ClapPlugin) {
    let found = with_guis(|m| {
        let id = m
            .iter()
            .find(|(_, s)| std::ptr::eq(s.plugin, plugin))
            .map(|(id, _)| *id)?;
        m.remove(&id).map(|s| (id, s))
    });
    if let Some((id, state)) = found {
        let tx = state.event_tx.clone();
        unsafe { teardown(state) };
        let _ = tx.send(crate::pipewire::PwEvent::Plugin(
            crate::pipewire::PluginEvent::PluginUiClosed { instance_id: id },
        ));
        log::info!("CLAP GUI closed for instance {} (plugin dropped)", id);
    }
}

/// Check if a CLAP GUI is currently open for this instance.
#[allow(dead_code)]
pub fn is_gui_open(instance_id: PluginInstanceId) -> bool {
    with_guis(|m| m.contains_key(&instance_id))
}

unsafe fn host_plugin(host: *const clap_sys::host::clap_host) -> *const ClapPlugin {
    unsafe {
        if host.is_null() || (*host).host_data.is_null() {
            return std::ptr::null();
        }
        let hd = &*((*host).host_data as *const super::host::HostData);
        hd.plugin
    }
}

// ===========================================================================
// Host extension statics — returned via host_get_extension
// ===========================================================================

// ---- clap_host_gui ----

pub static CLAP_HOST_GUI: clap_sys::ext::gui::clap_host_gui = clap_sys::ext::gui::clap_host_gui {
    resize_hints_changed: Some(host_gui_resize_hints_changed),
    request_resize: Some(host_gui_request_resize),
    request_show: Some(host_gui_request_show),
    request_hide: Some(host_gui_request_hide),
    closed: Some(host_gui_closed),
};

unsafe extern "C" fn host_gui_resize_hints_changed(_host: *const clap_sys::host::clap_host) {
    log::debug!("CLAP host_gui: resize_hints_changed");
}

unsafe extern "C" fn host_gui_request_resize(
    host: *const clap_sys::host::clap_host,
    width: u32,
    height: u32,
) -> bool {
    log::debug!("CLAP host_gui: request_resize {}x{}", width, height);
    let plugin = unsafe { host_plugin(host) };
    if plugin.is_null() {
        return false;
    }
    let on_main = threading::is_main_thread();
    with_guis(|m| {
        let Some(s) = m.values_mut().find(|s| std::ptr::eq(s.plugin, plugin)) else {
            return false;
        };
        if on_main {
            unsafe { s.window.resize(width, height) };
        } else {
            // Applied by the next `idle` on the thread that owns the Display.
            s.pending_size = Some((width, height));
        }
        true
    })
}

unsafe extern "C" fn host_gui_request_show(_host: *const clap_sys::host::clap_host) -> bool {
    log::debug!("CLAP host_gui: request_show");
    true
}

unsafe extern "C" fn host_gui_request_hide(_host: *const clap_sys::host::clap_host) -> bool {
    log::debug!("CLAP host_gui: request_hide");
    true
}

unsafe extern "C" fn host_gui_closed(host: *const clap_sys::host::clap_host, was_destroyed: bool) {
    log::debug!("CLAP host_gui: closed (was_destroyed={})", was_destroyed);
    let plugin = unsafe { host_plugin(host) };
    with_guis(|m| {
        if let Some(s) = m.values_mut().find(|s| std::ptr::eq(s.plugin, plugin)) {
            s.close_requested = true;
        }
    });
}

// ---- clap_host_timer_support ----

pub static CLAP_HOST_TIMER_SUPPORT: clap_sys::ext::timer_support::clap_host_timer_support =
    clap_sys::ext::timer_support::clap_host_timer_support {
        register_timer: Some(host_timer_register),
        unregister_timer: Some(host_timer_unregister),
    };

/// Resolve the plugin for a host callback (host_data, or the plugin whose
/// GUI is being created on this thread).
unsafe fn callback_plugin(host: *const clap_sys::host::clap_host) -> *const ClapPlugin {
    let p = unsafe { host_plugin(host) };
    if !p.is_null() {
        return p;
    }
    CURRENT_PLUGIN_PTR.with(|cell| cell.get())
}

unsafe extern "C" fn host_timer_register(
    host: *const clap_sys::host::clap_host,
    period_ms: u32,
    timer_id: *mut u32,
) -> bool {
    if timer_id.is_null() {
        return false;
    }
    let plugin = unsafe { callback_plugin(host) };
    if plugin.is_null() {
        log::warn!("CLAP timer: cannot find plugin for host {:?}", host);
        return false;
    }
    let id = {
        let mut next = NEXT_TIMER_ID.lock().unwrap_or_else(|e| e.into_inner());
        let id = *next;
        *next = next.wrapping_add(1).max(1);
        id
    };
    let period = Duration::from_millis(period_ms.max(1) as u64);
    with_timers(|t| {
        t.insert(
            id,
            TimerEntry {
                plugin,
                period,
                next_due: Instant::now() + period,
            },
        );
    });
    unsafe {
        *timer_id = id;
    }
    log::debug!("CLAP timer registered: id={} period={}ms", id, period_ms);
    true
}

unsafe extern "C" fn host_timer_unregister(
    _host: *const clap_sys::host::clap_host,
    timer_id: u32,
) -> bool {
    let removed = with_timers(|timers| timers.remove(&timer_id).is_some());
    if removed {
        log::debug!("CLAP timer unregistered: id={}", timer_id);
    }
    removed
}

// ---- clap_host_posix_fd_support ----

pub static CLAP_HOST_POSIX_FD_SUPPORT: clap_sys::ext::posix_fd_support::clap_host_posix_fd_support =
    clap_sys::ext::posix_fd_support::clap_host_posix_fd_support {
        register_fd: Some(host_register_fd),
        modify_fd: Some(host_modify_fd),
        unregister_fd: Some(host_unregister_fd),
    };

unsafe extern "C" fn host_register_fd(
    host: *const clap_sys::host::clap_host,
    fd: i32,
    flags: u32,
) -> bool {
    let plugin = unsafe { callback_plugin(host) };
    if plugin.is_null() || fd < 0 {
        return false;
    }
    with_fds(|f| {
        if f.iter().any(|e| std::ptr::eq(e.plugin, plugin) && e.fd == fd) {
            return false;
        }
        f.push(FdEntry { plugin, fd, flags });
        true
    })
}

unsafe extern "C" fn host_modify_fd(
    host: *const clap_sys::host::clap_host,
    fd: i32,
    flags: u32,
) -> bool {
    let plugin = unsafe { callback_plugin(host) };
    with_fds(|f| {
        match f.iter_mut().find(|e| std::ptr::eq(e.plugin, plugin) && e.fd == fd) {
            Some(e) => {
                e.flags = flags;
                true
            }
            None => false,
        }
    })
}

unsafe extern "C" fn host_unregister_fd(host: *const clap_sys::host::clap_host, fd: i32) -> bool {
    let plugin = unsafe { callback_plugin(host) };
    with_fds(|f| {
        let before = f.len();
        f.retain(|e| !(std::ptr::eq(e.plugin, plugin) && e.fd == fd));
        f.len() != before
    })
}

// ---- clap_host_thread_check ----

pub static CLAP_HOST_THREAD_CHECK: clap_sys::ext::thread_check::clap_host_thread_check =
    clap_sys::ext::thread_check::clap_host_thread_check {
        is_main_thread: Some(host_is_main_thread),
        is_audio_thread: Some(host_is_audio_thread),
    };

unsafe extern "C" fn host_is_main_thread(_host: *const clap_sys::host::clap_host) -> bool {
    threading::is_main_thread()
}

unsafe extern "C" fn host_is_audio_thread(_host: *const clap_sys::host::clap_host) -> bool {
    threading::is_audio_thread()
}

// ---- Thread-local for current plugin being opened ----

thread_local! {
    static CURRENT_PLUGIN_PTR: std::cell::Cell<*const ClapPlugin> =
        const { std::cell::Cell::new(std::ptr::null()) };
}

/// Set the current plugin pointer for the duration of GUI creation.
/// This allows timer registration during gui.create() to find the plugin.
pub fn with_current_plugin<F, R>(plugin: *const ClapPlugin, f: F) -> R
where
    F: FnOnce() -> R,
{
    CURRENT_PLUGIN_PTR.with(|cell| {
        let old = cell.get();
        cell.set(plugin);
        let result = f();
        cell.set(old);
        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_plugin_callbacks_only_touches_that_plugin() {
        let a = 0x1000usize as *const ClapPlugin;
        let b = 0x2000usize as *const ClapPlugin;
        let far = Instant::now() + Duration::from_secs(3600);
        with_timers(|t| {
            t.insert(9_000_001, TimerEntry { plugin: a, period: Duration::from_secs(1), next_due: far });
            t.insert(9_000_002, TimerEntry { plugin: b, period: Duration::from_secs(1), next_due: far });
        });
        with_fds(|f| {
            f.push(FdEntry { plugin: a, fd: 1001, flags: 1 });
            f.push(FdEntry { plugin: b, fd: 1002, flags: 1 });
        });

        remove_plugin_callbacks(a);

        assert!(with_timers(|t| !t.contains_key(&9_000_001) && t.contains_key(&9_000_002)));
        assert!(with_fds(|f| !f.iter().any(|e| e.fd == 1001) && f.iter().any(|e| e.fd == 1002)));

        remove_plugin_callbacks(b);
        assert!(with_timers(|t| !t.contains_key(&9_000_002)));
    }
}
