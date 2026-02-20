//! LV2 Plugin Native UI Support
//!
//! Uses suil to instantiate native LV2 plugin UIs and embeds them in GTK3
//! windows.  A single persistent GTK thread handles all plugin UI windows —
//! `gtk_init()` is called exactly once, and `gtk_main()` runs for the entire
//! application lifetime.  Individual plugin windows are created and destroyed
//! via a command channel, allowing multiple plugin UIs to coexist.
//!
//! This avoids the crash-on-reopen bug caused by calling `gtk_init()` from
//! different threads and running multiple `gtk_main()` loops.

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_uint, c_ulong, c_void};
use std::ptr;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, OnceLock};

use crate::lv2::urid::UridMapper;
use crate::pipewire::{Lv2Event, PwCommand, PwEvent};

// ─── Suil FFI ─────────────────────────────────────────────────────────────────

#[link(name = "suil-0")]
unsafe extern "C" {
    fn suil_host_new(
        write_func: Option<SuilPortWriteFunc>,
        index_func: Option<SuilPortIndexFunc>,
        subscribe_func: Option<SuilPortSubscribeFunc>,
        unsubscribe_func: Option<SuilPortUnsubscribeFunc>,
    ) -> *mut c_void;

    fn suil_host_free(host: *mut c_void);

    fn suil_instance_new(
        host: *mut c_void,
        controller: *mut c_void,
        container_type_uri: *const c_char,
        plugin_uri: *const c_char,
        ui_uri: *const c_char,
        ui_type_uri: *const c_char,
        ui_bundle_path: *const c_char,
        ui_binary_path: *const c_char,
        features: *const *const lv2_raw::core::LV2Feature,
    ) -> *mut c_void;

    fn suil_instance_free(instance: *mut c_void);

    fn suil_instance_get_widget(instance: *mut c_void) -> *mut c_void;

    /// Get the raw LV2UI_Handle from the suil instance.
    fn suil_instance_get_handle(instance: *mut c_void) -> *mut c_void;

    fn suil_instance_port_event(
        instance: *mut c_void,
        port_index: c_uint,
        buffer_size: c_uint,
        format: c_uint,
        buffer: *const c_void,
    );

    /// Query the UI for extension data by URI.
    /// Returns a pointer to the extension struct, or null if unsupported.
    fn suil_instance_extension_data(
        instance: *mut c_void,
        uri: *const c_char,
    ) -> *const c_void;
}

type SuilPortWriteFunc = unsafe extern "C" fn(
    controller: *mut c_void,
    port_index: c_uint,
    buffer_size: c_uint,
    protocol: c_uint,
    buffer: *const c_void,
);

type SuilPortIndexFunc =
    unsafe extern "C" fn(controller: *mut c_void, port_symbol: *const c_char) -> c_uint;

type SuilPortSubscribeFunc = unsafe extern "C" fn(
    controller: *mut c_void,
    port_index: c_uint,
    protocol: c_uint,
    features: *const *const lv2_raw::core::LV2Feature,
) -> c_uint;

type SuilPortUnsubscribeFunc = unsafe extern "C" fn(
    controller: *mut c_void,
    port_index: c_uint,
    protocol: c_uint,
    features: *const *const lv2_raw::core::LV2Feature,
) -> c_uint;

// ─── GTK3 / GLib FFI ─────────────────────────────────────────────────────────

#[link(name = "gtk-3")]
unsafe extern "C" {
    fn gtk_init(argc: *mut c_int, argv: *mut *mut *mut c_char);
    fn gtk_main();
    fn gtk_main_quit();
    fn gtk_window_new(window_type: c_int) -> *mut c_void;
    fn gtk_window_set_title(window: *mut c_void, title: *const c_char);
    fn gtk_window_set_default_size(window: *mut c_void, width: c_int, height: c_int);
    fn gtk_container_add(container: *mut c_void, widget: *mut c_void);
    fn gtk_widget_show_all(widget: *mut c_void);
    fn gtk_widget_destroy(widget: *mut c_void);
    fn gtk_window_present(window: *mut c_void);
}

#[link(name = "gobject-2.0")]
unsafe extern "C" {
    fn g_signal_connect_data(
        instance: *mut c_void,
        detailed_signal: *const c_char,
        c_handler: Option<unsafe extern "C" fn()>,
        data: *mut c_void,
        destroy_data: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
        connect_flags: c_uint,
    ) -> c_ulong;
}

#[link(name = "glib-2.0")]
unsafe extern "C" {
    fn g_timeout_add(
        interval: c_uint,
        function: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
        data: *mut c_void,
    ) -> c_uint;

    fn g_idle_add(
        function: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
        data: *mut c_void,
    ) -> c_uint;

    fn g_source_remove(tag: c_uint) -> c_int;
}

/// GTK_WINDOW_TOPLEVEL = 0
const GTK_WINDOW_TOPLEVEL: c_int = 0;

// ─── LV2 UI idle interface ────────────────────────────────────────────────────

/// URI for the LV2 UI idle interface extension.
const LV2_UI_IDLE_INTERFACE: &CStr = c"http://lv2plug.in/ns/extensions/ui#idleInterface";

/// The LV2 UI idle interface — plugins that support it expose an `idle`
/// function that the host must call periodically for internal housekeeping
/// (redraws, animations, etc.).
#[repr(C)]
struct Lv2UiIdleInterface {
    /// Returns 0 if the UI is still alive, non-zero if it wants to close.
    idle: Option<unsafe extern "C" fn(ui_handle: *mut c_void) -> c_int>,
}

// ─── LV2 UI type URIs ────────────────────────────────────────────────────────

const LV2_UI_GTK2: &str = "http://lv2plug.in/ns/extensions/ui#GtkUI";
const LV2_UI_GTK3: &str = "http://lv2plug.in/ns/extensions/ui#Gtk3UI";
const LV2_UI_GTK4: &str = "http://lv2plug.in/ns/extensions/ui#Gtk4UI";
const LV2_UI_X11: &str = "http://lv2plug.in/ns/extensions/ui#X11UI";
const LV2_UI_QT5: &str = "http://lv2plug.in/ns/extensions/ui#Qt5UI";

/// Host container type — we embed into a GTK3 window, so tell suil to
/// wrap any non-GTK3 UI (X11, Qt5, GTK2) into a GTK3 widget.
const HOST_TYPE_URI: &str = "http://lv2plug.in/ns/extensions/ui#Gtk3UI";

// ─── Controller ───────────────────────────────────────────────────────────────

/// Passed as `controller` pointer to suil callbacks.  Gives the port-write
/// callback enough context to send parameter changes back to the PW thread.
struct UiController {
    instance_id: u64,
    cmd_tx: Sender<PwCommand>,
    /// Port symbol → port index mapping (for port_index_callback)
    symbol_to_index: Vec<(String, usize)>,
    /// Shared port updates for atom input forwarding (UI → DSP).
    /// None if the plugin has no atom input ports.
    port_updates: Option<super::types::SharedPortUpdates>,
}

// ─── Suil callbacks ───────────────────────────────────────────────────────────

/// Called by the plugin UI when it writes a control value.
unsafe extern "C" fn port_write_callback(
    controller: *mut c_void,
    port_index: c_uint,
    buffer_size: c_uint,
    protocol: c_uint,
    buffer: *const c_void,
) {
    if controller.is_null() || buffer.is_null() {
        return;
    }
    let ctrl = unsafe { &*(controller as *const UiController) };

    if protocol == 0 && buffer_size == 4 {
        // Protocol 0 means float control port
        let value = unsafe { *(buffer as *const f32) };
        log::debug!(
            "UI port write: instance={}, index={}, value={}",
            ctrl.instance_id,
            port_index,
            value
        );
        let _ = ctrl.cmd_tx.send(PwCommand::SetPluginParameter {
            instance_id: ctrl.instance_id,
            port_index: port_index as usize,
            value,
        });
    } else if protocol != 0 && buffer_size > 0 {
        // Non-zero protocol = atom data (e.g. atom:eventTransfer).
        // Forward the atom data to the DSP thread's atom input buffer
        // via the shared PortUpdates.
        if let Some(ref port_updates) = ctrl.port_updates {
            let data =
                unsafe { std::slice::from_raw_parts(buffer as *const u8, buffer_size as usize) };
            for atom_in in port_updates.atom_inputs.iter() {
                if atom_in.port_index == port_index as usize {
                    atom_in.write(data);
                    break;
                }
            }
        }
    }
}

/// Called by suil to resolve a port symbol to its index.
unsafe extern "C" fn port_index_callback(
    controller: *mut c_void,
    port_symbol: *const c_char,
) -> c_uint {
    if controller.is_null() || port_symbol.is_null() {
        return u32::MAX;
    }
    let ctrl = unsafe { &*(controller as *const UiController) };
    let symbol = unsafe { CStr::from_ptr(port_symbol) };
    let symbol = match symbol.to_str() {
        Ok(s) => s,
        Err(_) => return u32::MAX,
    };
    for (sym, idx) in &ctrl.symbol_to_index {
        if sym == symbol {
            return *idx as c_uint;
        }
    }
    u32::MAX
}

// ─── Per-window state ─────────────────────────────────────────────────────────

/// State for a single plugin UI window managed by the GTK thread.
struct WindowState {
    instance_id: u64,
    gtk_window: *mut c_void,
    suil_instance: *mut c_void,
    suil_host: *mut c_void,
    controller_ptr: *mut c_void,
    /// Timer data for DSP→UI sync (heap-allocated, freed on window close)
    timer_data: *mut UiTimerData,
    /// GLib source ID for the DSP→UI sync timer, used to remove it on close
    timer_source_id: c_uint,
}

// ─── Periodic DSP → UI sync ──────────────────────────────────────────────────

/// Data held by the GLib timeout that syncs DSP control port values to the UI.
struct UiTimerData {
    suil_instance: *mut c_void,
    port_updates: super::types::SharedPortUpdates,
    /// URID of "http://lv2plug.in/ns/ext/atom#eventTransfer", used as the
    /// `format` parameter when pushing atom data to the UI via
    /// `suil_instance_port_event`.
    atom_event_transfer_urid: u32,
    /// LV2 UI idle interface, if the plugin supports it.
    idle_iface: Option<&'static Lv2UiIdleInterface>,
    /// The raw LV2UI_Handle (from suil_instance_get_handle), passed to
    /// the idle callback.
    ui_handle: *mut c_void,
    /// Set to true when the window is closing — tells the timer callback
    /// to stop (return 0) so it doesn't touch the suil instance after
    /// it has been freed.
    closing: std::sync::atomic::AtomicBool,
    /// The instance_id, used to close the correct window from the idle callback
    /// when the plugin UI requests close via the idle interface.
    instance_id: u64,
}

/// GLib timeout callback — reads control port values from the DSP (via
/// the lock-free `SharedPortUpdates`) and pushes them to the suil UI
/// instance via `suil_instance_port_event`.
///
/// Returns 1 (G_SOURCE_CONTINUE) to keep the timer running, or 0 to stop.
unsafe extern "C" fn ui_timer_callback(data: *mut c_void) -> c_int {
    if data.is_null() {
        return 0; // stop
    }
    let td = unsafe { &*(data as *const UiTimerData) };
    if td.suil_instance.is_null()
        || td.closing.load(std::sync::atomic::Ordering::Acquire)
    {
        return 0; // stop the timer
    }

    // Push control OUTPUT values (meters, gain reduction, etc.) to the UI.
    for slot in td.port_updates.control_outputs.iter() {
        let val = slot.value.load();
        unsafe {
            suil_instance_port_event(
                td.suil_instance,
                slot.port_index as c_uint,
                std::mem::size_of::<f32>() as c_uint,
                0, // protocol 0 = float
                &val as *const f32 as *const c_void,
            );
        }
    }

    // Push control INPUT values back to the UI.
    for slot in td.port_updates.control_inputs.iter() {
        let val = slot.value.load();
        unsafe {
            suil_instance_port_event(
                td.suil_instance,
                slot.port_index as c_uint,
                std::mem::size_of::<f32>() as c_uint,
                0, // protocol 0 = float
                &val as *const f32 as *const c_void,
            );
        }
    }

    // Forward atom output data (visualization buffers, waveforms, etc.)
    // from the DSP thread to the plugin UI.
    if td.atom_event_transfer_urid != 0 {
        for atom_buf in td.port_updates.atom_outputs.iter() {
            if let Some(data) = atom_buf.read() {
                if data.len() < 16 {
                    continue;
                }
                let seq_body_size = u32::from_ne_bytes(
                    [data[0], data[1], data[2], data[3]]
                ) as usize;
                let events_end = (8 + seq_body_size).min(data.len());
                let mut offset = 16usize;

                while offset + 16 <= events_end {
                    let event_atom_offset = offset + 8;
                    let ev_size = u32::from_ne_bytes([
                        data[event_atom_offset],
                        data[event_atom_offset + 1],
                        data[event_atom_offset + 2],
                        data[event_atom_offset + 3],
                    ]) as usize;
                    let atom_total = 8 + ev_size;
                    if event_atom_offset + atom_total > events_end {
                        break;
                    }

                    unsafe {
                        suil_instance_port_event(
                            td.suil_instance,
                            atom_buf.port_index as c_uint,
                            atom_total as c_uint,
                            td.atom_event_transfer_urid as c_uint,
                            data[event_atom_offset..].as_ptr() as *const c_void,
                        );
                    }

                    let padded_size = (atom_total + 7) & !7;
                    offset += 8 + padded_size;
                }
            }
        }
    }

    // Call the plugin UI's idle interface if it supports one.
    if let Some(idle_iface) = td.idle_iface {
        if let Some(idle_fn) = idle_iface.idle {
            let result = unsafe { idle_fn(td.ui_handle) };
            if result != 0 {
                // Plugin wants to close — schedule a close via g_idle_add
                // instead of directly manipulating the window here.
                let instance_id = td.instance_id;
                let close_data = Box::into_raw(Box::new(instance_id));
                unsafe {
                    g_idle_add(
                        Some(close_window_idle_callback),
                        close_data as *mut c_void,
                    );
                }
                return 0; // stop the timer
            }
        }
    }

    1 // G_SOURCE_CONTINUE
}

// ─── GTK thread global state ─────────────────────────────────────────────────

/// Command sent to the persistent GTK thread.
enum GtkCommand {
    /// Open a plugin UI window.
    Open(OpenUiRequest),
    /// Close a plugin UI window by instance_id.
    /// `destroyed_by_gtk` is true when this comes from the GTK "destroy"
    /// signal (window already being destroyed — don't call gtk_widget_destroy).
    Close { instance_id: u64, destroyed_by_gtk: bool },
    /// Shut down the GTK thread (quit gtk_main).
    Shutdown,
}

/// All the data needed to open a plugin UI window on the GTK thread.
struct OpenUiRequest {
    plugin_uri: String,
    instance_id: u64,
    cmd_tx: Sender<PwCommand>,
    event_tx: Sender<PwEvent>,
    control_values: Vec<(usize, f32)>,
    port_updates: super::types::SharedPortUpdates,
    urid_mapper: Arc<UridMapper>,
}

/// Sender half of the GTK command channel.  Initialized once when the
/// persistent GTK thread is spawned.
static GTK_CMD_TX: OnceLock<Mutex<Sender<GtkCommand>>> = OnceLock::new();

/// Set of instance IDs that currently have their UI open.
/// Shared between the GTK thread and the rest of the app.
static OPEN_UI_SET: OnceLock<Arc<Mutex<std::collections::HashSet<u64>>>> = OnceLock::new();

fn open_ui_set() -> &'static Arc<Mutex<std::collections::HashSet<u64>>> {
    OPEN_UI_SET.get_or_init(|| Arc::new(Mutex::new(std::collections::HashSet::new())))
}

/// Returns true if the given instance already has its native UI open.
pub fn is_ui_open(instance_id: u64) -> bool {
    open_ui_set().lock().unwrap().contains(&instance_id)
}

/// Ensure the persistent GTK thread is running.  Safe to call multiple times;
/// the thread is only spawned on the first call.
fn ensure_gtk_thread() -> &'static Mutex<Sender<GtkCommand>> {
    GTK_CMD_TX.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<GtkCommand>();

        std::thread::Builder::new()
            .name("zestbay-gtk".into())
            .spawn(move || {
                gtk_thread_main(rx);
            })
            .expect("Failed to spawn GTK thread");

        Mutex::new(tx)
    })
}

/// Data stored per command-channel poll callback.  We use a raw pointer to
/// the receiver because GLib callbacks need stable pointers.
struct GtkThreadState {
    cmd_rx: std::sync::mpsc::Receiver<GtkCommand>,
    windows: HashMap<u64, WindowState>,
    event_tx_cache: Option<Sender<PwEvent>>,
}

/// GLib idle callback that drains the command channel and processes requests.
/// Registered once when the GTK thread starts.  Returns 1 to keep running.
unsafe extern "C" fn gtk_poll_commands(data: *mut c_void) -> c_int {
    if data.is_null() {
        return 0;
    }
    let state = unsafe { &mut *(data as *mut GtkThreadState) };

    // Drain all pending commands (non-blocking)
    while let Ok(cmd) = state.cmd_rx.try_recv() {
        match cmd {
            GtkCommand::Open(req) => {
                // Cache event_tx for future close notifications
                if state.event_tx_cache.is_none() {
                    state.event_tx_cache = Some(req.event_tx.clone());
                }
                handle_open_window(state, req);
            }
            GtkCommand::Close { instance_id, destroyed_by_gtk } => {
                handle_close_window(state, instance_id, destroyed_by_gtk);
            }
            GtkCommand::Shutdown => {
                // Close all open windows, then quit
                let ids: Vec<u64> = state.windows.keys().copied().collect();
                for id in ids {
                    handle_close_window(state, id, false);
                }
                unsafe { gtk_main_quit(); }
                return 0; // stop polling
            }
        }
    }

    1 // G_SOURCE_CONTINUE — keep polling
}

/// Idle callback to close a window from within the GTK main loop
/// (e.g., when the idle interface requests close).
unsafe extern "C" fn close_window_idle_callback(data: *mut c_void) -> c_int {
    if !data.is_null() {
        let instance_id = unsafe { *Box::from_raw(data as *mut u64) };
        // We can't directly access GtkThreadState from here, so we send
        // a close command through the channel.
        if let Some(tx) = GTK_CMD_TX.get() {
            if let Ok(tx) = tx.lock() {
                let _ = tx.send(GtkCommand::Close { instance_id, destroyed_by_gtk: false });
            }
        }
    }
    0 // G_SOURCE_REMOVE — run once
}

/// GTK "destroy" signal handler for individual plugin windows.
/// Called when the user closes the window via the X button.
/// The `data` pointer is a heap-allocated `u64` instance_id.
unsafe extern "C" fn on_window_destroy_multi(_widget: *mut c_void, data: *mut c_void) {
    if data.is_null() {
        return;
    }
    let instance_id = unsafe { *(data as *const u64) };
    log::info!("Plugin UI window destroyed for instance {}", instance_id);

    // Send a close command to clean up resources.  The GTK window widget
    // itself is already being destroyed by GTK, so handle_close_window
    // will skip gtk_widget_destroy but still free suil resources.
    if let Some(tx) = GTK_CMD_TX.get() {
        if let Ok(tx) = tx.lock() {
            let _ = tx.send(GtkCommand::Close { instance_id, destroyed_by_gtk: true });
        }
    }
}

/// GLib destroy-notify for the signal data — frees the heap-allocated instance_id.
unsafe extern "C" fn destroy_instance_id_data(data: *mut c_void, _closure: *mut c_void) {
    if !data.is_null() {
        let _ = unsafe { Box::from_raw(data as *mut u64) };
    }
}

// ─── GTK thread main function ─────────────────────────────────────────────────

/// Main function for the persistent GTK thread.  Initializes GTK, sets up
/// a command poller, and runs `gtk_main()` indefinitely.
fn gtk_thread_main(cmd_rx: std::sync::mpsc::Receiver<GtkCommand>) {
    unsafe {
        // Force X11 backend for GTK — suil's X11-in-GTK3 wrapper uses
        // GtkPlug/GtkSocket which only work under X11.  On Wayland this
        // would crash.  Setting GDK_BACKEND=x11 makes GTK use XWayland.
        std::env::set_var("GDK_BACKEND", "x11");

        gtk_init(ptr::null_mut(), ptr::null_mut());
    }

    let state = Box::into_raw(Box::new(GtkThreadState {
        cmd_rx,
        windows: HashMap::new(),
        event_tx_cache: None,
    }));

    // Register an idle callback to poll the command channel.
    // Using g_timeout_add at a short interval is more CPU-friendly than
    // a true idle source (which would spin).  16ms ≈ 60Hz responsiveness.
    unsafe {
        g_timeout_add(16, Some(gtk_poll_commands), state as *mut c_void);
    }

    log::info!("Persistent GTK thread started — running gtk_main()");
    unsafe {
        gtk_main();
    }
    log::info!("Persistent GTK thread exiting");

    // Cleanup
    let _ = unsafe { Box::from_raw(state) };
}

// ─── Window open/close helpers (called on GTK thread) ─────────────────────────

/// Open a new plugin UI window.  Called on the GTK thread.
fn handle_open_window(state: &mut GtkThreadState, req: OpenUiRequest) {
    let instance_id = req.instance_id;

    // If already open, just present (focus) the existing window
    if let Some(ws) = state.windows.get(&instance_id) {
        log::info!("Plugin UI already open for instance {} — focusing", instance_id);
        unsafe { gtk_window_present(ws.gtk_window); }
        return;
    }

    // Discover plugin & UI via lilv
    let world = lilv::World::with_load_all();
    let uri_node = world.new_uri(&req.plugin_uri);

    let plugin = match world.plugins().iter().find(|p| p.uri().as_uri() == uri_node.as_uri()) {
        Some(p) => p,
        None => {
            log::error!("Plugin not found: {}", req.plugin_uri);
            let _ = req.event_tx.send(PwEvent::Lv2(Lv2Event::PluginError {
                instance_id: Some(instance_id),
                message: format!("Plugin not found: {}", req.plugin_uri),
            }));
            return;
        }
    };

    let plugin_name = plugin
        .name()
        .as_str()
        .map(String::from)
        .unwrap_or_else(|| req.plugin_uri.clone());

    // Build symbol→index mapping
    let mut symbol_to_index: Vec<(String, usize)> = Vec::new();
    let port_ranges = plugin.port_ranges_float();
    for (i, _) in port_ranges.iter().enumerate() {
        if let Some(port) = plugin.port_by_index(i) {
            if let Some(sym_node) = port.symbol() {
                if let Some(sym) = sym_node.as_str() {
                    symbol_to_index.push((sym.to_string(), i));
                }
            }
        }
    }

    // Find a suitable UI
    let ui_class_x11 = world.new_uri(LV2_UI_X11);
    let ui_class_gtk2 = world.new_uri(LV2_UI_GTK2);
    let ui_class_gtk3 = world.new_uri(LV2_UI_GTK3);
    let ui_class_gtk4 = world.new_uri(LV2_UI_GTK4);
    let ui_class_qt5 = world.new_uri(LV2_UI_QT5);

    let preferred_types = [
        (&ui_class_gtk3, LV2_UI_GTK3),
        (&ui_class_x11, LV2_UI_X11),
        (&ui_class_qt5, LV2_UI_QT5),
        (&ui_class_gtk2, LV2_UI_GTK2),
        (&ui_class_gtk4, LV2_UI_GTK4),
    ];

    let mut found_ui: Option<(String, String, String, String)> = None;

    if let Some(uis) = plugin.uis() {
        for ui in uis.iter() {
            for (class_node, type_uri) in &preferred_types {
                if ui.is_a(class_node) {
                    let ui_uri = match ui.uri().as_uri() {
                        Some(u) => u.to_string(),
                        None => continue,
                    };
                    let bundle_path = match ui.bundle_uri() {
                        Some(node) => match node.path() {
                            Some((_host, path)) => path,
                            None => continue,
                        },
                        None => continue,
                    };
                    let binary_path = match ui.binary_uri() {
                        Some(node) => match node.path() {
                            Some((_host, path)) => path,
                            None => continue,
                        },
                        None => continue,
                    };
                    found_ui = Some((ui_uri, type_uri.to_string(), bundle_path, binary_path));
                    break;
                }
            }
            if found_ui.is_some() {
                break;
            }
        }
    }

    let (ui_uri, ui_type_uri, bundle_path, binary_path) = match found_ui {
        Some(f) => f,
        None => {
            log::error!("No supported UI found for plugin: {}", req.plugin_uri);
            let _ = req.event_tx.send(PwEvent::Lv2(Lv2Event::PluginError {
                instance_id: Some(instance_id),
                message: format!("No supported UI found for plugin: {}", req.plugin_uri),
            }));
            return;
        }
    };

    log::info!(
        "Opening UI: uri={}, type={}, bundle={}, binary={}",
        ui_uri, ui_type_uri, bundle_path, binary_path
    );

    // Build CStrings
    let host_type = match CString::new(HOST_TYPE_URI) {
        Ok(s) => s,
        Err(e) => { log::error!("CString error: {}", e); return; }
    };
    let c_plugin_uri = match CString::new(req.plugin_uri.as_str()) {
        Ok(s) => s,
        Err(e) => { log::error!("CString error: {}", e); return; }
    };
    let c_ui_uri = match CString::new(ui_uri) {
        Ok(s) => s,
        Err(e) => { log::error!("CString error: {}", e); return; }
    };
    let c_ui_type_uri = match CString::new(ui_type_uri) {
        Ok(s) => s,
        Err(e) => { log::error!("CString error: {}", e); return; }
    };
    let c_bundle_path = match CString::new(bundle_path) {
        Ok(s) => s,
        Err(e) => { log::error!("CString error: {}", e); return; }
    };
    let c_binary_path = match CString::new(binary_path) {
        Ok(s) => s,
        Err(e) => { log::error!("CString error: {}", e); return; }
    };

    // Create controller
    let has_atom_inputs = !req.port_updates.atom_inputs.is_empty();
    let controller = Box::new(UiController {
        instance_id,
        cmd_tx: req.cmd_tx,
        symbol_to_index,
        port_updates: if has_atom_inputs {
            Some(req.port_updates.clone())
        } else {
            None
        },
    });
    let controller_ptr = Box::into_raw(controller) as *mut c_void;

    // Build LV2 features for the UI
    let mut urid_map_struct = req.urid_mapper.as_lv2_urid_map();
    let urid_feature = unsafe { UridMapper::make_feature(&mut urid_map_struct) };

    unsafe {
        // Create suil host & instance
        let host = suil_host_new(
            Some(port_write_callback),
            Some(port_index_callback),
            None,
            None,
        );
        if host.is_null() {
            let _ = Box::from_raw(controller_ptr as *mut UiController);
            log::error!("Failed to create suil host for instance {}", instance_id);
            return;
        }

        let features: [*const lv2_raw::core::LV2Feature; 2] =
            [&urid_feature as *const _, ptr::null()];

        let instance = suil_instance_new(
            host,
            controller_ptr,
            host_type.as_ptr(),
            c_plugin_uri.as_ptr(),
            c_ui_uri.as_ptr(),
            c_ui_type_uri.as_ptr(),
            c_bundle_path.as_ptr(),
            c_binary_path.as_ptr(),
            features.as_ptr(),
        );
        if instance.is_null() {
            suil_host_free(host);
            let _ = Box::from_raw(controller_ptr as *mut UiController);
            log::error!("Failed to create suil instance for instance {}", instance_id);
            let _ = req.event_tx.send(PwEvent::Lv2(Lv2Event::PluginError {
                instance_id: Some(instance_id),
                message: "Failed to create suil instance".into(),
            }));
            return;
        }

        let widget = suil_instance_get_widget(instance);
        if widget.is_null() {
            suil_instance_free(instance);
            suil_host_free(host);
            let _ = Box::from_raw(controller_ptr as *mut UiController);
            log::error!("Failed to get UI widget for instance {}", instance_id);
            return;
        }

        // Push current DSP parameter values into the UI
        for &(port_index, value) in &req.control_values {
            let val = value;
            suil_instance_port_event(
                instance,
                port_index as c_uint,
                std::mem::size_of::<f32>() as c_uint,
                0,
                &val as *const f32 as *const c_void,
            );
        }

        // Create GTK window and embed the widget
        let window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
        if window.is_null() {
            suil_instance_free(instance);
            suil_host_free(host);
            let _ = Box::from_raw(controller_ptr as *mut UiController);
            log::error!("Failed to create GTK window for instance {}", instance_id);
            return;
        }

        let title = CString::new(format!("ZestBay — {}", plugin_name))
            .unwrap_or_else(|_| c"ZestBay — Plugin UI".to_owned());
        gtk_window_set_title(window, title.as_ptr());
        gtk_window_set_default_size(window, 640, 480);

        // Connect "destroy" signal — when user closes window via X button.
        // We pass a heap-allocated instance_id as data so the callback knows
        // which window was closed.
        let destroy_signal = c"destroy";
        let id_data = Box::into_raw(Box::new(instance_id));
        g_signal_connect_data(
            window,
            destroy_signal.as_ptr(),
            // Cast the two-arg handler to the no-arg fn pointer type that
            // g_signal_connect_data expects.  GLib actually passes the args
            // at the ABI level regardless; this is standard GLib FFI practice.
            Some(std::mem::transmute::<
                unsafe extern "C" fn(*mut c_void, *mut c_void),
                unsafe extern "C" fn(),
            >(on_window_destroy_multi)),
            id_data as *mut c_void,
            Some(destroy_instance_id_data),
            0,
        );

        gtk_container_add(window, widget);
        gtk_widget_show_all(window);

        // Set up periodic DSP → UI sync
        let atom_event_transfer_urid =
            req.urid_mapper.map("http://lv2plug.in/ns/ext/atom#eventTransfer");

        let idle_iface: Option<&'static Lv2UiIdleInterface> = {
            let ext = suil_instance_extension_data(
                instance,
                LV2_UI_IDLE_INTERFACE.as_ptr(),
            );
            if ext.is_null() {
                None
            } else {
                Some(&*(ext as *const Lv2UiIdleInterface))
            }
        };
        let ui_handle = suil_instance_get_handle(instance);

        if idle_iface.is_some() {
            log::info!("Plugin supports ui:idleInterface");
        }

        let timer_data = Box::into_raw(Box::new(UiTimerData {
            suil_instance: instance,
            port_updates: req.port_updates,
            atom_event_transfer_urid,
            idle_iface,
            ui_handle,
            closing: std::sync::atomic::AtomicBool::new(false),
            instance_id,
        }));
        let timer_source_id = g_timeout_add(33, Some(ui_timer_callback), timer_data as *mut c_void);

        // Track the window
        state.windows.insert(instance_id, WindowState {
            instance_id,
            gtk_window: window,
            suil_instance: instance,
            suil_host: host,
            controller_ptr,
            timer_data,
            timer_source_id,
        });

        // Update the shared open-UI set
        open_ui_set().lock().unwrap().insert(instance_id);

        log::info!("Plugin UI window opened for instance {}", instance_id);

        // Notify the UI that a plugin window opened
        let _ = req.event_tx.send(PwEvent::Lv2(Lv2Event::PluginUiOpened { instance_id }));
    }
}

/// Close a plugin UI window and free its resources.  Called on the GTK thread.
///
/// `destroyed_by_gtk` — true when called from the GTK "destroy" signal (the
/// window is already being destroyed by GTK; we must NOT call
/// `gtk_widget_destroy` again).  false for programmatic close (we need to
/// destroy the window ourselves).
fn handle_close_window(state: &mut GtkThreadState, instance_id: u64, destroyed_by_gtk: bool) {
    let ws = match state.windows.remove(&instance_id) {
        Some(ws) => ws,
        None => return, // already closed or not open
    };

    unsafe {
        // Remove the GLib timer source first — this guarantees the timer
        // callback won't fire again, so it's safe to free timer_data.
        // g_source_remove returns TRUE if the source was found and removed.
        g_source_remove(ws.timer_source_id);

        // Signal closing as well (belt-and-suspenders with g_source_remove)
        (*ws.timer_data)
            .closing
            .store(true, std::sync::atomic::Ordering::Release);

        // Only destroy the GTK window if this is a programmatic close.
        // When called from the "destroy" signal, the window is already
        // mid-destruction — calling gtk_widget_destroy would be a double-free.
        if !destroyed_by_gtk {
            gtk_widget_destroy(ws.gtk_window);
        }

        // Free suil instance and host
        suil_instance_free(ws.suil_instance);
        suil_host_free(ws.suil_host);

        // Free timer data and controller
        let _ = Box::from_raw(ws.timer_data);
        let _ = Box::from_raw(ws.controller_ptr as *mut UiController);
    }

    // Update the shared open-UI set
    open_ui_set().lock().unwrap().remove(&instance_id);

    log::info!("Plugin UI window closed and resources freed for instance {}", instance_id);

    // Notify the UI
    if let Some(ref event_tx) = state.event_tx_cache {
        let _ = event_tx.send(PwEvent::Lv2(Lv2Event::PluginUiClosed { instance_id }));
    }
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Request to open a native LV2 plugin UI.
///
/// This is non-blocking — it sends a command to the persistent GTK thread
/// which creates the window asynchronously.  If the UI is already open for
/// this instance, the existing window is focused instead.
///
/// `control_values` is a snapshot of current control input port values
/// (port_index, value) from the running DSP instance.
pub fn open_plugin_ui(
    plugin_uri: &str,
    instance_id: u64,
    cmd_tx: Sender<PwCommand>,
    event_tx: Sender<PwEvent>,
    control_values: Vec<(usize, f32)>,
    port_updates: super::types::SharedPortUpdates,
    urid_mapper: Arc<UridMapper>,
) {
    let gtk_tx = ensure_gtk_thread();
    let tx = gtk_tx.lock().unwrap();
    let _ = tx.send(GtkCommand::Open(OpenUiRequest {
        plugin_uri: plugin_uri.to_string(),
        instance_id,
        cmd_tx,
        event_tx,
        control_values,
        port_updates,
        urid_mapper,
    }));
}

/// Request to close a native LV2 plugin UI window.
///
/// Non-blocking — sends a close command to the GTK thread.
pub fn close_plugin_ui(instance_id: u64) {
    if let Some(tx) = GTK_CMD_TX.get() {
        if let Ok(tx) = tx.lock() {
            let _ = tx.send(GtkCommand::Close { instance_id, destroyed_by_gtk: false });
        }
    }
}

/// Shut down the persistent GTK thread.  Call this during application exit.
pub fn shutdown_gtk_thread() {
    if let Some(tx) = GTK_CMD_TX.get() {
        if let Ok(tx) = tx.lock() {
            let _ = tx.send(GtkCommand::Shutdown);
        }
    }
}
