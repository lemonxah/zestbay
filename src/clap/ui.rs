//! CLAP plugin GUI support — embedded X11 window mode.
//!
//! CLAP plugins that support the `clap.gui` extension are embedded in a
//! host-created X11 parent window.  Most Linux CLAP plugins (JUCE-based like
//! Surge XT) only support embedded mode (`is_api_supported(X11, false)`).
//!
//! The host creates an X11 top-level window, calls `gui.set_parent()` with
//! the window ID, and pumps X11 events on a dedicated thread.

use std::collections::HashMap;
use std::os::raw::{c_char, c_int, c_ulong, c_void};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use crate::plugin::types::PluginInstanceId;

// ---------------------------------------------------------------------------
// X11 FFI bindings (minimal set needed for host windows)
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

use std::os::raw::c_long;

const STRUCTURE_NOTIFY_MASK: c_long = 1 << 17;
const SUBSTRUCTURE_NOTIFY_MASK: c_long = 1 << 19;
const EXPOSURE_MASK: c_long = 1 << 15;



// ---------------------------------------------------------------------------
// GUI state registry
// ---------------------------------------------------------------------------

/// Per-instance state for an open CLAP GUI (always embedded X11).
struct ClapGuiState {
    plugin: *const clap_sys::plugin::clap_plugin,
    gui_ext: *const clap_sys::ext::gui::clap_plugin_gui,
    instance_id: PluginInstanceId,
    /// The X11 display connection we created for this GUI window.
    x11_display: *mut c_void,
    /// The X11 window hosting the plugin.
    x11_window: c_ulong,
    /// Signal to stop the event loop thread.
    running: std::sync::Arc<AtomicBool>,
}

unsafe impl Send for ClapGuiState {}

/// Global registry of open CLAP GUIs, keyed by instance_id.
static OPEN_GUIS: Mutex<Option<HashMap<PluginInstanceId, ClapGuiState>>> = Mutex::new(None);

fn with_guis<F, R>(f: F) -> R
where
    F: FnOnce(&mut HashMap<PluginInstanceId, ClapGuiState>) -> R,
{
    let mut lock = OPEN_GUIS.lock().unwrap();
    let map = lock.get_or_insert_with(HashMap::new);
    f(map)
}

// ---------------------------------------------------------------------------
// Timer support
// ---------------------------------------------------------------------------

/// A registered timer.
struct TimerEntry {
    plugin: *const clap_sys::plugin::clap_plugin,
    timer_ext: *const clap_sys::ext::timer_support::clap_plugin_timer_support,
    period_ms: u32,
}

unsafe impl Send for TimerEntry {}

/// Global timer registry.  Timer IDs are globally unique across all plugin instances.
static TIMERS: Mutex<Option<HashMap<u32, TimerEntry>>> = Mutex::new(None);
static NEXT_TIMER_ID: AtomicU64 = AtomicU64::new(1);
static TIMER_THREAD_RUNNING: AtomicBool = AtomicBool::new(false);

fn with_timers<F, R>(f: F) -> R
where
    F: FnOnce(&mut HashMap<u32, TimerEntry>) -> R,
{
    let mut lock = TIMERS.lock().unwrap();
    let map = lock.get_or_insert_with(HashMap::new);
    f(map)
}

fn ensure_timer_thread() {
    if TIMER_THREAD_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed)
        .is_ok()
    {
        std::thread::Builder::new()
            .name("zestbay-clap-timers".into())
            .spawn(timer_thread_main)
            .ok();
    }
}

fn timer_thread_main() {
    log::debug!("CLAP timer thread started");
    let tick = std::time::Duration::from_millis(10);

    // Track when each timer was last fired
    let mut last_fired: HashMap<u32, std::time::Instant> = HashMap::new();

    loop {
        std::thread::sleep(tick);

        // Collect timers that need to fire
        let to_fire: Vec<(
            u32,
            *const clap_sys::plugin::clap_plugin,
            *const clap_sys::ext::timer_support::clap_plugin_timer_support,
        )> = with_timers(|timers| {
            if timers.is_empty() {
                return Vec::new();
            }
            let now = std::time::Instant::now();
            let mut result = Vec::new();
            for (&id, entry) in timers.iter() {
                let period = std::time::Duration::from_millis(entry.period_ms as u64);
                let last = last_fired.entry(id).or_insert(now - period);
                if now.duration_since(*last) >= period {
                    result.push((id, entry.plugin, entry.timer_ext));
                    *last = now;
                }
            }
            result
        });

        for (timer_id, plugin, timer_ext) in to_fire {
            unsafe {
                let ext = &*timer_ext;
                if let Some(on_timer) = ext.on_timer {
                    on_timer(plugin, timer_id);
                }
            }
        }

        // Clean up last_fired entries for removed timers
        let active_ids: Vec<u32> = with_timers(|timers| timers.keys().copied().collect());
        last_fired.retain(|id, _| active_ids.contains(id));
    }
}

// ---------------------------------------------------------------------------
// Thread ID tracking (for thread_check extension)
// ---------------------------------------------------------------------------

/// Thread ID of the CLAP GUI thread.
/// CLAP considers this the "main thread" for GUI operations.
static MAIN_THREAD_ID: Mutex<Option<std::thread::ThreadId>> = Mutex::new(None);

/// Register the calling thread as the CLAP "main thread".
/// This must be called from the thread where plugins are created/activated
/// (the PipeWire thread in ZestBay) before any param or GUI calls.
pub fn set_main_thread_id() {
    let mut lock = MAIN_THREAD_ID.lock().unwrap();
    *lock = Some(std::thread::current().id());
}

fn is_main_thread() -> bool {
    let lock = MAIN_THREAD_ID.lock().unwrap();
    lock.map_or(false, |id| id == std::thread::current().id())
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Open a CLAP plugin GUI in an embedded X11 window.
///
/// # Safety
/// `plugin_ptr` must be a valid, live CLAP plugin pointer.
pub unsafe fn open_clap_gui(
    plugin_ptr: *const clap_sys::plugin::clap_plugin,
    instance_id: PluginInstanceId,
    display_name: &str,
    event_tx: &std::sync::mpsc::Sender<crate::pipewire::PwEvent>,
    cmd_tx: &std::sync::mpsc::Sender<crate::pipewire::PwCommand>,
) {
    // Check if already open
    let already_open = with_guis(|m| m.contains_key(&instance_id));
    if already_open {
        log::warn!("CLAP GUI already open for instance {}", instance_id);
        return;
    }

    unsafe {
        let plugin_ref = &*plugin_ptr;

        // Get the gui extension from the plugin
        let gui_ext_ptr = match plugin_ref.get_extension {
            Some(get_ext) => get_ext(plugin_ptr, clap_sys::ext::gui::CLAP_EXT_GUI.as_ptr()),
            None => {
                log::warn!(
                    "CLAP plugin has no get_extension (instance {})",
                    instance_id
                );
                return;
            }
        };

        if gui_ext_ptr.is_null() {
            log::warn!(
                "CLAP plugin does not support gui extension (instance {})",
                instance_id
            );
            return;
        }

        let gui = &*(gui_ext_ptr as *const clap_sys::ext::gui::clap_plugin_gui);

        // Check that the plugin supports embedded X11
        let supported = match gui.is_api_supported {
            Some(is_supported) => is_supported(
                plugin_ptr,
                clap_sys::ext::gui::CLAP_WINDOW_API_X11.as_ptr(),
                false,
            ),
            None => false,
        };

        if !supported {
            log::warn!(
                "CLAP plugin does not support embedded X11 GUI (instance {})",
                instance_id
            );
            return;
        }

        open_embedded_x11_gui(
            plugin_ptr,
            instance_id,
            display_name,
            event_tx,
            cmd_tx,
            gui,
            gui_ext_ptr as *const clap_sys::ext::gui::clap_plugin_gui,
        );
    }
}

/// Open an embedded CLAP GUI in a host-created X11 window.
unsafe fn open_embedded_x11_gui(
    plugin_ptr: *const clap_sys::plugin::clap_plugin,
    instance_id: PluginInstanceId,
    display_name: &str,
    event_tx: &std::sync::mpsc::Sender<crate::pipewire::PwEvent>,
    cmd_tx: &std::sync::mpsc::Sender<crate::pipewire::PwCommand>,
    gui: &clap_sys::ext::gui::clap_plugin_gui,
    gui_ext: *const clap_sys::ext::gui::clap_plugin_gui,
) {
    unsafe {
        // Ensure main thread ID is set (normally already done in ClapPluginInstance::new)
        set_main_thread_id();

        // Ensure the timer thread is running
        ensure_timer_thread();

        // Create the embedded GUI (is_floating = false)
        let created = with_current_plugin(plugin_ptr, || match gui.create {
            Some(create_fn) => {
                create_fn(
                    plugin_ptr,
                    clap_sys::ext::gui::CLAP_WINDOW_API_X11.as_ptr(),
                    false,
                )
            }
            None => false,
        });

        if !created {
            log::warn!(
                "CLAP gui.create (embedded) returned false (instance {})",
                instance_id
            );
            remove_timers_for_plugin(plugin_ptr);
            return;
        }

        // Query the preferred size
        let mut width: u32 = 800;
        let mut height: u32 = 600;
        if let Some(get_size) = gui.get_size {
            get_size(plugin_ptr, &mut width, &mut height);
        }

        log::info!(
            "CLAP embedded GUI size: {}x{} (instance {})",
            width,
            height,
            instance_id
        );

        // Open our own X11 display connection for this GUI window
        let display = XOpenDisplay(std::ptr::null());
        if display.is_null() {
            log::error!("CLAP: cannot open X11 display for embedded GUI");
            if let Some(destroy_fn) = gui.destroy {
                destroy_fn(plugin_ptr);
            }
            return;
        }

        let screen = XDefaultScreen(display);
        let root = XDefaultRootWindow(display);

        // Create a top-level window to host the plugin
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
            log::error!("CLAP: XCreateSimpleWindow failed");
            XCloseDisplay(display);
            if let Some(destroy_fn) = gui.destroy {
                destroy_fn(plugin_ptr);
            }
            return;
        }

        // Set the window title
        let title = std::ffi::CString::new(display_name).unwrap_or_default();
        XStoreName(display, window, title.as_ptr());

        // Select events we care about
        XSelectInput(
            display,
            window,
            STRUCTURE_NOTIFY_MASK | SUBSTRUCTURE_NOTIFY_MASK | EXPOSURE_MASK,
        );

        // Set up WM_DELETE_WINDOW so we can handle window close
        let wm_delete = XInternAtom(display, c"WM_DELETE_WINDOW".as_ptr(), 0);
        let mut protocols = [wm_delete];
        XSetWMProtocols(display, window, protocols.as_mut_ptr(), 1);

        XMapWindow(display, window);
        XFlush(display);

        // Tell the plugin to embed in our window
        let clap_window = clap_sys::ext::gui::clap_window {
            api: clap_sys::ext::gui::CLAP_WINDOW_API_X11.as_ptr(),
            specific: clap_sys::ext::gui::clap_window_handle { x11: window },
        };

        let set_ok = if let Some(set_parent) = gui.set_parent {
            set_parent(plugin_ptr, &clap_window)
        } else {
            false
        };

        if !set_ok {
            log::error!(
                "CLAP gui.set_parent failed (instance {})",
                instance_id
            );
            XDestroyWindow(display, window);
            XCloseDisplay(display);
            if let Some(destroy_fn) = gui.destroy {
                destroy_fn(plugin_ptr);
            }
            return;
        }

        // Show the plugin GUI
        if let Some(show_fn) = gui.show {
            show_fn(plugin_ptr);
        }

        XFlush(display);

        let running = std::sync::Arc::new(AtomicBool::new(true));

        // Register the open GUI
        with_guis(|m| {
            m.insert(
                instance_id,
                ClapGuiState {
                    plugin: plugin_ptr,
                    gui_ext,
                    instance_id,
                    x11_display: display,
                    x11_window: window,
                    running: running.clone(),
                },
            );
        });

        // Spawn a thread to pump X11 events for this window.
        // Cast the raw display pointer to usize to cross the Send boundary.
        let display_addr = display as usize;
        let cmd_tx_clone = cmd_tx.clone();
        let running_clone = running.clone();
        std::thread::Builder::new()
            .name(format!("clap-gui-{}", instance_id))
            .spawn(move || {
                let display = display_addr as *mut c_void;
                x11_event_loop(display, window, wm_delete, instance_id, running_clone, cmd_tx_clone);
            })
            .ok();

        log::info!("CLAP embedded X11 GUI opened for instance {}", instance_id);
        let _ = event_tx.send(crate::pipewire::PwEvent::Plugin(
            crate::pipewire::PluginEvent::PluginUiOpened { instance_id },
        ));
    }
}

/// X11 event loop for an embedded CLAP GUI window.
///
/// When the user clicks the window close button (WM_DELETE_WINDOW), we send
/// `PwCommand::ClosePluginUI` back to the PipeWire thread so it can do the
/// proper teardown (gui.hide, gui.destroy, X11 cleanup) on the main thread.
fn x11_event_loop(
    display: *mut c_void,
    _window: c_ulong,
    wm_delete_atom: c_ulong,
    instance_id: PluginInstanceId,
    running: std::sync::Arc<AtomicBool>,
    cmd_tx: std::sync::mpsc::Sender<crate::pipewire::PwCommand>,
) {
    let tick = std::time::Duration::from_millis(16); // ~60fps

    while running.load(Ordering::Acquire) {
        // Process all pending X11 events
        unsafe {
            while XPending(display) > 0 {
                let mut event = std::mem::zeroed::<[u8; 192]>();
                XNextEvent(display, &mut event);

                // Check for WM_DELETE_WINDOW (ClientMessage type = 33)
                // XEvent.type is the first i32 in the union
                let event_type = *(event.as_ptr() as *const i32);
                if event_type == 33 {
                    // ClientMessage
                    // The atom is at offset 56 in XClientMessageEvent (data.l[0])
                    let atom = *((event.as_ptr() as *const u8).add(56) as *const c_ulong);
                    if atom == wm_delete_atom {
                        log::info!(
                            "CLAP GUI window close requested (instance {})",
                            instance_id
                        );
                        running.store(false, Ordering::Release);
                        // Ask the PW thread to do proper CLAP teardown
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

    log::debug!("CLAP X11 event loop exiting for instance {}", instance_id);
}

/// Close a CLAP plugin GUI if it is open.
///
/// Must be called from the main (PW) thread so that CLAP plugin methods
/// (`gui.hide`, `gui.destroy`) are invoked on the correct thread.
pub fn close_clap_gui(
    instance_id: PluginInstanceId,
    event_tx: &std::sync::mpsc::Sender<crate::pipewire::PwEvent>,
) {
    let state = with_guis(|m| m.remove(&instance_id));

    if let Some(state) = state {
        // Signal the event loop thread to stop
        state.running.store(false, Ordering::Release);

        // Remove all timers for this plugin
        remove_timers_for_plugin(state.plugin);

        unsafe {
            let gui = &*state.gui_ext;

            if let Some(hide_fn) = gui.hide {
                hide_fn(state.plugin);
            }

            if let Some(destroy_fn) = gui.destroy {
                destroy_fn(state.plugin);
            }

            // Clean up X11 resources
            if !state.x11_display.is_null() && state.x11_window != 0 {
                // Give the event loop thread a moment to exit
                std::thread::sleep(std::time::Duration::from_millis(50));
                XDestroyWindow(state.x11_display, state.x11_window);
                XCloseDisplay(state.x11_display);
            }
        }

        let _ = event_tx.send(crate::pipewire::PwEvent::Plugin(
            crate::pipewire::PluginEvent::PluginUiClosed { instance_id },
        ));

        log::info!("CLAP GUI closed for instance {}", instance_id);
    }
}

/// Called when the plugin notifies us that it closed its own GUI (via clap_host_gui::closed).
#[allow(dead_code)]
pub fn on_plugin_gui_closed(instance_id: PluginInstanceId) {
    let state = with_guis(|m| m.remove(&instance_id));
    if let Some(state) = state {
        state.running.store(false, Ordering::Release);
        remove_timers_for_plugin(state.plugin);
        log::info!("CLAP plugin self-closed GUI for instance {}", instance_id);
    }
}

/// Check if a CLAP GUI is currently open for this instance.
#[allow(dead_code)]
pub fn is_gui_open(instance_id: PluginInstanceId) -> bool {
    with_guis(|m| m.contains_key(&instance_id))
}

/// Remove all registered timers for a given plugin pointer.
fn remove_timers_for_plugin(plugin: *const clap_sys::plugin::clap_plugin) {
    with_timers(|timers| {
        timers.retain(|_id, entry| !std::ptr::eq(entry.plugin, plugin));
    });
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
    _host: *const clap_sys::host::clap_host,
    width: u32,
    height: u32,
) -> bool {
    log::debug!("CLAP host_gui: request_resize {}x{}", width, height);

    // Resize the X11 window if we have one for this plugin
    let host_ref = unsafe { &*_host };
    if !host_ref.host_data.is_null() {
        let hd = unsafe { &*(host_ref.host_data as *const super::host::HostData) };
        if !hd.plugin.is_null() {
            with_guis(|m| {
                for state in m.values() {
                    if std::ptr::eq(state.plugin, hd.plugin)
                        && !state.x11_display.is_null()
                        && state.x11_window != 0
                    {
                        unsafe {
                            XResizeWindow(
                                state.x11_display,
                                state.x11_window,
                                width as c_int,
                                height as c_int,
                            );
                            XFlush(state.x11_display);
                        }
                        break;
                    }
                }
            });
        }
    }

    true
}

unsafe extern "C" fn host_gui_request_show(_host: *const clap_sys::host::clap_host) -> bool {
    log::debug!("CLAP host_gui: request_show");
    true
}

unsafe extern "C" fn host_gui_request_hide(_host: *const clap_sys::host::clap_host) -> bool {
    log::debug!("CLAP host_gui: request_hide");
    true
}

unsafe extern "C" fn host_gui_closed(
    _host: *const clap_sys::host::clap_host,
    was_destroyed: bool,
) {
    log::debug!("CLAP host_gui: closed (was_destroyed={})", was_destroyed);
}

// ---- clap_host_timer_support ----

pub static CLAP_HOST_TIMER_SUPPORT: clap_sys::ext::timer_support::clap_host_timer_support =
    clap_sys::ext::timer_support::clap_host_timer_support {
        register_timer: Some(host_timer_register),
        unregister_timer: Some(host_timer_unregister),
    };

unsafe extern "C" fn host_timer_register(
    host: *const clap_sys::host::clap_host,
    period_ms: u32,
    timer_id: *mut u32,
) -> bool {
    unsafe {
        if host.is_null() || timer_id.is_null() {
            return false;
        }

        // Get the plugin pointer from host->host_data.
        let host_ref = &*host;
        if !host_ref.host_data.is_null() {
            let hd = &*(host_ref.host_data as *const super::host::HostData);
            if !hd.plugin.is_null() {
                return register_timer_for_plugin(hd.plugin, period_ms, timer_id);
            }
        }

        // Fallback: try thread-local (set during gui.create())
        let plugin_ptr = CURRENT_PLUGIN_PTR.with(|cell| cell.get());
        if !plugin_ptr.is_null() {
            return register_timer_for_plugin(plugin_ptr, period_ms, timer_id);
        }

        // Fallback: try open GUIs
        let found = with_guis(|m| m.values().next().map(|s| s.plugin));
        if let Some(p) = found {
            return register_timer_for_plugin(p, period_ms, timer_id);
        }

        log::warn!("CLAP timer: cannot find plugin for host {:?}", host);
        false
    }
}

fn register_timer_for_plugin(
    plugin: *const clap_sys::plugin::clap_plugin,
    period_ms: u32,
    timer_id: *mut u32,
) -> bool {
    let id = NEXT_TIMER_ID.fetch_add(1, Ordering::Relaxed) as u32;

    // Get the timer_support extension from the plugin
    let timer_ext = unsafe {
        let plugin_ref = &*plugin;
        match plugin_ref.get_extension {
            Some(get_ext) => {
                let ext = get_ext(
                    plugin,
                    clap_sys::ext::timer_support::CLAP_EXT_TIMER_SUPPORT.as_ptr(),
                );
                if ext.is_null() {
                    log::warn!("CLAP timer: plugin has no timer_support extension");
                    return false;
                }
                ext as *const clap_sys::ext::timer_support::clap_plugin_timer_support
            }
            None => {
                log::warn!("CLAP timer: plugin has no get_extension");
                return false;
            }
        }
    };

    with_timers(|timers| {
        timers.insert(
            id,
            TimerEntry {
                plugin,
                timer_ext,
                period_ms,
            },
        );
    });

    unsafe {
        *timer_id = id;
    }

    ensure_timer_thread();
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

// ---- clap_host_thread_check ----

pub static CLAP_HOST_THREAD_CHECK: clap_sys::ext::thread_check::clap_host_thread_check =
    clap_sys::ext::thread_check::clap_host_thread_check {
        is_main_thread: Some(host_is_main_thread),
        is_audio_thread: Some(host_is_audio_thread),
    };

unsafe extern "C" fn host_is_main_thread(_host: *const clap_sys::host::clap_host) -> bool {
    is_main_thread()
}

unsafe extern "C" fn host_is_audio_thread(_host: *const clap_sys::host::clap_host) -> bool {
    !is_main_thread()
}

// ---- Thread-local for current plugin being opened ----

thread_local! {
    static CURRENT_PLUGIN_PTR: std::cell::Cell<*const clap_sys::plugin::clap_plugin> =
        const { std::cell::Cell::new(std::ptr::null()) };
}

/// Set the current plugin pointer for the duration of GUI creation.
/// This allows timer registration during gui.create() to find the plugin.
pub fn with_current_plugin<F, R>(plugin: *const clap_sys::plugin::clap_plugin, f: F) -> R
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
