use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_uint, c_ulong, c_void};
use std::ptr;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, OnceLock};

use crate::lv2::urid::UridMapper;
use crate::pipewire::{PluginEvent, PwCommand, PwEvent};

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

    fn suil_instance_get_handle(instance: *mut c_void) -> *mut c_void;

    fn suil_instance_port_event(
        instance: *mut c_void,
        port_index: c_uint,
        buffer_size: c_uint,
        format: c_uint,
        buffer: *const c_void,
    );

    fn suil_instance_extension_data(instance: *mut c_void, uri: *const c_char) -> *const c_void;
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
    fn gtk_widget_realize(widget: *mut c_void);
    fn gtk_widget_get_window(widget: *mut c_void) -> *mut c_void;
    fn gtk_window_present(window: *mut c_void);
    fn gtk_socket_new() -> *mut c_void;
    fn gtk_socket_get_id(socket: *mut c_void) -> c_ulong;
    fn gtk_socket_add_id(socket: *mut c_void, window_id: c_ulong);
    fn gtk_drawing_area_new() -> *mut c_void;
    fn gtk_widget_set_size_request(widget: *mut c_void, width: c_int, height: c_int);
    fn gtk_widget_set_can_focus(widget: *mut c_void, can_focus: c_int);
}

#[link(name = "gdk-3")]
unsafe extern "C" {
    fn gdk_x11_window_get_xid(gdk_window: *mut c_void) -> c_ulong;
    fn gdk_display_get_default() -> *mut c_void;
    fn gdk_x11_display_get_xdisplay(gdk_display: *mut c_void) -> *mut c_void;
}

#[link(name = "X11")]
unsafe extern "C" {
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
    fn XFlush(display: *mut c_void) -> c_int;
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

const GTK_WINDOW_TOPLEVEL: c_int = 0;

const LV2_UI_IDLE_INTERFACE: &CStr = c"http://lv2plug.in/ns/extensions/ui#idleInterface";
const LV2_UI_RESIZE_URI: &CStr = c"http://lv2plug.in/ns/extensions/ui#resize";

/// LV2UI_Descriptor for direct UI instantiation (bypassing suil).
#[repr(C)]
#[allow(non_camel_case_types)]
struct LV2UI_Descriptor {
    uri: *const c_char,
    instantiate: Option<
        unsafe extern "C" fn(
            descriptor: *const LV2UI_Descriptor,
            plugin_uri: *const c_char,
            bundle_path: *const c_char,
            write_function: SuilPortWriteFunc,
            controller: *mut c_void,
            widget: *mut *mut c_void,
            features: *const *const lv2_raw::core::LV2Feature,
        ) -> *mut c_void,
    >,
    cleanup: Option<unsafe extern "C" fn(handle: *mut c_void)>,
    port_event: Option<
        unsafe extern "C" fn(
            handle: *mut c_void,
            port_index: c_uint,
            buffer_size: c_uint,
            format: c_uint,
            buffer: *const c_void,
        ),
    >,
    extension_data: Option<unsafe extern "C" fn(uri: *const c_char) -> *const c_void>,
}

/// LV2 UI resize feature — host provides this so the plugin can request resize.
#[repr(C)]
struct LV2UIResize {
    handle: *mut c_void,
    ui_resize: unsafe extern "C" fn(handle: *mut c_void, width: c_int, height: c_int) -> c_int,
}

unsafe extern "C" fn ui_resize_callback(
    handle: *mut c_void,
    width: c_int,
    height: c_int,
) -> c_int {
    unsafe {
        if !handle.is_null() {
            let window = handle as *mut c_void;
            gtk_window_set_default_size(window, width, height);
        }
    }
    0
}

unsafe extern "C" fn ui_resize_x11_callback(
    handle: *mut c_void,
    width: c_int,
    height: c_int,
) -> c_int {
    unsafe {
        if !handle.is_null() {
            let win = &mut *(handle as *mut super::x11_ui::X11PluginWindow);
            win.set_size(width as u32, height as u32);
        }
    }
    0
}

const LV2_DATA_ACCESS_URI: &CStr = c"http://lv2plug.in/ns/ext/data-access";
const LV2_INSTANCE_ACCESS_URI: &CStr = c"http://lv2plug.in/ns/ext/instance-access";
const LV2_UI_PARENT_URI: &CStr = c"http://lv2plug.in/ns/extensions/ui#parent";

#[repr(C)]
struct LV2ExtensionDataFeature {
    data_access: unsafe extern "C" fn(*const c_char) -> *const c_void,
}

#[repr(C)]
struct Lv2UiIdleInterface {
    idle: Option<unsafe extern "C" fn(ui_handle: *mut c_void) -> c_int>,
}

const LV2_UI_GTK2: &str = "http://lv2plug.in/ns/extensions/ui#GtkUI";
const LV2_UI_GTK3: &str = "http://lv2plug.in/ns/extensions/ui#Gtk3UI";
const LV2_UI_GTK4: &str = "http://lv2plug.in/ns/extensions/ui#Gtk4UI";
const LV2_UI_X11: &str = "http://lv2plug.in/ns/extensions/ui#X11UI";
const LV2_UI_QT5: &str = "http://lv2plug.in/ns/extensions/ui#Qt5UI";

const HOST_TYPE_URI: &str = "http://lv2plug.in/ns/extensions/ui#Gtk3UI";

struct UiController {
    instance_id: u64,
    cmd_tx: Sender<PwCommand>,
    symbol_to_index: Vec<(String, usize)>,
    port_updates: Option<super::types::SharedPortUpdates>,
}

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

struct WindowState {
    instance_id: u64,
    gtk_window: *mut c_void,
    suil_instance: *mut c_void,
    suil_host: *mut c_void,
    controller_ptr: *mut c_void,
    timer_data: *mut UiTimerData,
    timer_source_id: c_uint,
}

struct UiTimerData {
    suil_instance: *mut c_void,
    port_updates: super::types::SharedPortUpdates,
    atom_event_transfer_urid: u32,
    idle_iface: Option<&'static Lv2UiIdleInterface>,
    ui_handle: *mut c_void,
    closing: Arc<std::sync::atomic::AtomicBool>,
    instance_id: u64,
    /// Set to true when the timer callback returns 0 (auto-removed by GLib),
    /// so handle_close_window knows not to call g_source_remove again.
    timer_removed: std::sync::atomic::AtomicBool,
    /// Cached control output values for change detection — only forward to
    /// the UI when a value actually changed, to avoid overwhelming plugin UIs.
    prev_control_outputs: Vec<f32>,
    /// Cached control input values for change detection.
    prev_control_inputs: Vec<f32>,
    /// X11 window for direct X11 UIs (null for GTK/suil UIs).
    x11_window: *mut super::x11_ui::X11PluginWindow,
}

unsafe extern "C" fn ui_timer_callback(data: *mut c_void) -> c_int {
    if data.is_null() {
        return 0;
    }
    // SAFETY: we need mutable access for the prev_control caches
    let td = unsafe { &mut *(data as *mut UiTimerData) };
    if td.closing.load(std::sync::atomic::Ordering::Acquire) {
        td.timer_removed.store(true, std::sync::atomic::Ordering::Release);
        return 0;
    }

    // Helper: forward a port event to the UI via either suil or direct descriptor
    let send_port_event = |port_index: usize, size: u32, format: u32, buf: *const c_void| {
        unsafe {
            if !td.suil_instance.is_null() {
                suil_instance_port_event(
                    td.suil_instance,
                    port_index as c_uint,
                    size,
                    format,
                    buf,
                );
            } else if !td.ui_handle.is_null() {
                // Direct X11 UI — call the descriptor's port_event via the handle
                // The port_event function pointer is stored at a known offset in the descriptor.
                // We use a trampoline approach: the ui_handle's descriptor is accessible.
                // For now, we skip port forwarding for direct UIs — the timer idle call
                // is the critical part. Port events for direct UIs need the descriptor pointer.
                // TODO: store descriptor port_event fn pointer in UiTimerData
            }
        }
    };

    // Forward control outputs to UI, but only when the value changed
    for (i, slot) in td.port_updates.control_outputs.iter().enumerate() {
        let val = slot.value.load();
        let prev = td.prev_control_outputs.get(i).copied().unwrap_or(f32::NAN);
        if val.to_bits() != prev.to_bits() {
            if i < td.prev_control_outputs.len() {
                td.prev_control_outputs[i] = val;
            }
            send_port_event(
                slot.port_index,
                std::mem::size_of::<f32>() as u32,
                0,
                &val as *const f32 as *const c_void,
            );
        }
    }

    // Forward control inputs to UI, but only when the value changed
    for (i, slot) in td.port_updates.control_inputs.iter().enumerate() {
        let val = slot.value.load();
        let prev = td.prev_control_inputs.get(i).copied().unwrap_or(f32::NAN);
        if val.to_bits() != prev.to_bits() {
            if i < td.prev_control_inputs.len() {
                td.prev_control_inputs[i] = val;
            }
            send_port_event(
                slot.port_index,
                std::mem::size_of::<f32>() as u32,
                0,
                &val as *const f32 as *const c_void,
            );
        }
    }

    if td.atom_event_transfer_urid != 0 {
        for atom_buf in td.port_updates.atom_outputs.iter() {
            if let Some(data) = atom_buf.read() {
                if data.len() < 16 {
                    continue;
                }
                let seq_body_size =
                    u32::from_ne_bytes([data[0], data[1], data[2], data[3]]) as usize;
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

    // Pump X11 events for direct X11 UIs
    if !td.x11_window.is_null() {
        let x11_win = unsafe { &mut *td.x11_window };
        x11_win.idle();
        if x11_win.closed {
            td.timer_removed.store(true, std::sync::atomic::Ordering::Release);
            let instance_id = td.instance_id;
            let close_data = Box::into_raw(Box::new(instance_id));
            unsafe {
                g_idle_add(Some(close_window_idle_callback), close_data as *mut c_void);
            }
            return 0;
        }
    }

    if let Some(idle_iface) = td.idle_iface
        && let Some(idle_fn) = idle_iface.idle
    {
        let result = unsafe { idle_fn(td.ui_handle) };
        if result != 0 {
            td.timer_removed.store(true, std::sync::atomic::Ordering::Release);
            let instance_id = td.instance_id;
            let close_data = Box::into_raw(Box::new(instance_id));
            unsafe {
                g_idle_add(Some(close_window_idle_callback), close_data as *mut c_void);
            }
            return 0;
        }
    }

    1
}

enum GtkCommand {
    Open(OpenUiRequest),
    Close {
        instance_id: u64,
        destroyed_by_gtk: bool,
    },
    Shutdown,
}

struct OpenUiRequest {
    plugin_uri: String,
    instance_id: u64,
    cmd_tx: Sender<PwCommand>,
    event_tx: Sender<PwEvent>,
    control_values: Vec<(usize, f32)>,
    port_updates: super::types::SharedPortUpdates,
    urid_mapper: Arc<UridMapper>,
    lv2_handle: *mut c_void,
    extension_data_fn: Option<unsafe extern "C" fn(*const c_char) -> *const c_void>,
}

// SAFETY: lv2_handle is a raw LV2 plugin handle that must cross from the PW
// thread to the GTK thread.  The GTK thread uses it read-only for data-access
// and instance-access UI features.  The plugin instance outlives the UI.
unsafe impl Send for OpenUiRequest {}
type OnceArcMutex<A> = OnceLock<Arc<Mutex<A>>>;

static GTK_CMD_TX: OnceLock<Mutex<Sender<GtkCommand>>> = OnceLock::new();

static OPEN_UI_SET: OnceArcMutex<std::collections::HashSet<u64>> = OnceLock::new();

static CLOSING_FLAGS: OnceArcMutex<HashMap<u64, Arc<std::sync::atomic::AtomicBool>>> =
    OnceLock::new();

fn closing_flags() -> &'static Arc<Mutex<HashMap<u64, Arc<std::sync::atomic::AtomicBool>>>> {
    CLOSING_FLAGS.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

fn open_ui_set() -> &'static Arc<Mutex<std::collections::HashSet<u64>>> {
    OPEN_UI_SET.get_or_init(|| Arc::new(Mutex::new(std::collections::HashSet::new())))
}

pub fn is_ui_open(instance_id: u64) -> bool {
    open_ui_set().lock().unwrap().contains(&instance_id)
}

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

struct GtkThreadState {
    cmd_rx: std::sync::mpsc::Receiver<GtkCommand>,
    windows: HashMap<u64, WindowState>,
    event_tx_cache: Option<Sender<PwEvent>>,
}

unsafe extern "C" fn gtk_poll_commands(data: *mut c_void) -> c_int {
    if data.is_null() {
        return 0;
    }
    let state = unsafe { &mut *(data as *mut GtkThreadState) };

    while let Ok(cmd) = state.cmd_rx.try_recv() {
        match cmd {
            GtkCommand::Open(req) => {
                if state.event_tx_cache.is_none() {
                    state.event_tx_cache = Some(req.event_tx.clone());
                }
                handle_open_window(state, req);
            }
            GtkCommand::Close {
                instance_id,
                destroyed_by_gtk,
            } => {
                handle_close_window(state, instance_id, destroyed_by_gtk);
            }
            GtkCommand::Shutdown => {
                let ids: Vec<u64> = state.windows.keys().copied().collect();
                for id in ids {
                    handle_close_window(state, id, false);
                }
                unsafe {
                    gtk_main_quit();
                }
                return 0;
            }
        }
    }

    1
}

unsafe extern "C" fn close_window_idle_callback(data: *mut c_void) -> c_int {
    if !data.is_null() {
        let instance_id = unsafe { *Box::from_raw(data as *mut u64) };
        if let Ok(flags) = closing_flags().lock()
            && let Some(flag) = flags.get(&instance_id)
        {
            flag.store(true, std::sync::atomic::Ordering::Release);
        }
        if let Some(tx) = GTK_CMD_TX.get()
            && let Ok(tx) = tx.lock()
        {
            let _ = tx.send(GtkCommand::Close {
                instance_id,
                destroyed_by_gtk: false,
            });
        }
    }
    0
}

unsafe extern "C" fn on_window_destroy_multi(_widget: *mut c_void, data: *mut c_void) {
    if data.is_null() {
        return;
    }
    let instance_id = unsafe { *(data as *const u64) };
    log::info!("Plugin UI window destroyed for instance {}", instance_id);

    if let Ok(flags) = closing_flags().lock()
        && let Some(flag) = flags.get(&instance_id)
    {
        flag.store(true, std::sync::atomic::Ordering::Release);
    }

    if let Some(tx) = GTK_CMD_TX.get()
        && let Ok(tx) = tx.lock()
    {
        let _ = tx.send(GtkCommand::Close {
            instance_id,
            destroyed_by_gtk: true,
        });
    }
}

unsafe extern "C" fn destroy_instance_id_data(data: *mut c_void, _closure: *mut c_void) {
    if !data.is_null() {
        let _ = unsafe { Box::from_raw(data as *mut u64) };
    }
}

fn gtk_thread_main(cmd_rx: std::sync::mpsc::Receiver<GtkCommand>) {
    unsafe {
        std::env::set_var("GDK_BACKEND", "x11");

        gtk_init(ptr::null_mut(), ptr::null_mut());
    }

    let state = Box::into_raw(Box::new(GtkThreadState {
        cmd_rx,
        windows: HashMap::new(),
        event_tx_cache: None,
    }));

    unsafe {
        g_timeout_add(16, Some(gtk_poll_commands), state as *mut c_void);
    }

    log::info!("Persistent GTK thread started — running gtk_main()");
    unsafe {
        gtk_main();
    }
    log::info!("Persistent GTK thread exiting");

    let _ = unsafe { Box::from_raw(state) };
}

fn handle_open_window(state: &mut GtkThreadState, req: OpenUiRequest) {
    let instance_id = req.instance_id;

    if let Some(ws) = state.windows.get(&instance_id) {
        log::info!(
            "Plugin UI already open for instance {} — focusing",
            instance_id
        );
        unsafe {
            gtk_window_present(ws.gtk_window);
        }
        return;
    }

    let world = lilv::World::with_load_all();
    let uri_node = world.new_uri(&req.plugin_uri);

    let plugin = match world
        .plugins()
        .iter()
        .find(|p| p.uri().as_uri() == uri_node.as_uri())
    {
        Some(p) => p,
        None => {
            log::error!("Plugin not found: {}", req.plugin_uri);
            let _ = req.event_tx.send(PwEvent::Plugin(PluginEvent::PluginError {
                instance_id: Some(instance_id),
                message: format!("Plugin not found: {}", req.plugin_uri),
                fatal: false,
            }));
            return;
        }
    };

    let plugin_name = plugin
        .name()
        .as_str()
        .map(String::from)
        .unwrap_or_else(|| req.plugin_uri.clone());

    let mut symbol_to_index: Vec<(String, usize)> = Vec::new();
    let port_ranges = plugin.port_ranges_float();
    for (i, _) in port_ranges.iter().enumerate() {
        if let Some(port) = plugin.port_by_index(i)
            && let Some(sym_node) = port.symbol()
            && let Some(sym) = sym_node.as_str()
        {
            symbol_to_index.push((sym.to_string(), i));
        }
    }

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
            let _ = req.event_tx.send(PwEvent::Plugin(PluginEvent::PluginError {
                instance_id: Some(instance_id),
                message: format!("No supported UI found for plugin: {}", req.plugin_uri),
                fatal: false,
            }));
            return;
        }
    };

    log::info!(
        "Opening UI: uri={}, type={}, bundle={}, binary={}",
        ui_uri,
        ui_type_uri,
        bundle_path,
        binary_path
    );

    let host_type = match CString::new(HOST_TYPE_URI) {
        Ok(s) => s,
        Err(e) => {
            log::error!("CString error: {}", e);
            return;
        }
    };
    let c_plugin_uri = match CString::new(req.plugin_uri.as_str()) {
        Ok(s) => s,
        Err(e) => {
            log::error!("CString error: {}", e);
            return;
        }
    };
    let c_ui_uri = match CString::new(ui_uri) {
        Ok(s) => s,
        Err(e) => {
            log::error!("CString error: {}", e);
            return;
        }
    };
    let c_ui_type_uri = match CString::new(ui_type_uri) {
        Ok(s) => s,
        Err(e) => {
            log::error!("CString error: {}", e);
            return;
        }
    };
    let c_bundle_path = match CString::new(bundle_path) {
        Ok(s) => s,
        Err(e) => {
            log::error!("CString error: {}", e);
            return;
        }
    };
    let c_binary_path = match CString::new(binary_path) {
        Ok(s) => s,
        Err(e) => {
            log::error!("CString error: {}", e);
            return;
        }
    };

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

    let mut urid_map_struct = req.urid_mapper.as_lv2_urid_map();
    let urid_feature = unsafe { UridMapper::make_feature(&mut urid_map_struct) };

    let mut data_access_feature_data;
    let data_access_feature;
    let instance_access_feature;

    let mut feature_list: Vec<*const lv2_raw::core::LV2Feature> = vec![&urid_feature as *const _];

    if let Some(ext_data_fn) = req.extension_data_fn {
        data_access_feature_data = LV2ExtensionDataFeature {
            data_access: ext_data_fn,
        };
        data_access_feature = lv2_raw::core::LV2Feature {
            uri: LV2_DATA_ACCESS_URI.as_ptr(),
            data: &mut data_access_feature_data as *mut _ as *mut c_void,
        };
        feature_list.push(&data_access_feature as *const _);
    }

    if !req.lv2_handle.is_null() {
        instance_access_feature = lv2_raw::core::LV2Feature {
            uri: LV2_INSTANCE_ACCESS_URI.as_ptr(),
            data: req.lv2_handle,
        };
        feature_list.push(&instance_access_feature as *const _);
    }

    let is_x11_ui = c_ui_type_uri.to_str() == Ok(LV2_UI_X11);

    // Provide LV2 options with sample rate for UIs that need it (e.g. DPF/Pugl)
    let sample_rate_urid = req.urid_mapper.map("http://lv2plug.in/ns/ext/parameters#sampleRate");
    let atom_float_urid = req.urid_mapper.map("http://lv2plug.in/ns/ext/atom#Float");
    let options_uri: &CStr = c"http://lv2plug.in/ns/ext/options#options";
    let sample_rate_value: f32 = 48000.0;

    #[repr(C)]
    struct Lv2Option {
        context: u32,
        subject: u32,
        key: u32,
        size: u32,
        type_: u32,
        value: *const c_void,
    }

    let options = [
        Lv2Option {
            context: 0, // LV2_OPTIONS_INSTANCE
            subject: 0,
            key: sample_rate_urid,
            size: std::mem::size_of::<f32>() as u32,
            type_: atom_float_urid,
            value: &sample_rate_value as *const f32 as *const c_void,
        },
        Lv2Option {
            context: 0,
            subject: 0,
            key: 0,
            size: 0,
            type_: 0,
            value: ptr::null(),
        },
    ];

    let options_feature = lv2_raw::core::LV2Feature {
        uri: options_uri.as_ptr(),
        data: options.as_ptr() as *mut c_void,
    };
    feature_list.push(&options_feature as *const _);

    unsafe {
        // Create GTK window first
        let window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
        if window.is_null() {
            let _ = Box::from_raw(controller_ptr as *mut UiController);
            log::error!("Failed to create GTK window for instance {}", instance_id);
            return;
        }

        let title = CString::new(format!("ZestBay — {}", plugin_name))
            .unwrap_or_else(|_| c"ZestBay — Plugin UI".to_owned());
        gtk_window_set_title(window, title.as_ptr());
        gtk_window_set_default_size(window, -1, -1);

        // For X11 UIs: create a GtkSocket, realize it, and use its XID as ui:parent.
        // This bypasses suil's GtkPlug wrapping which is broken for DPF/Pugl plugins.
        let socket_widget: *mut c_void;
        let parent_feature;
        let actual_host_type;
        let c_x11_host_type;

        #[allow(unreachable_code, unused)]
        if false {
            // === Direct X11 UI instantiation (Carla-style) ===
            // Currently disabled — suil/GTK is tried first. Direct X11 is used
            // as fallback when suil crashes (see SUIL_CRASHED handler below).
            let window = window; // suppress unused warning
            gtk_widget_destroy(window);

            let x11_title = format!("ZestBay — {}", plugin_name);
            let mut x11_window = match super::x11_ui::X11PluginWindow::new(&x11_title) {
                Some(w) => w,
                None => {
                    let _ = Box::from_raw(controller_ptr as *mut UiController);
                    log::error!("Failed to create X11 window for plugin UI");
                    return;
                }
            };

            let parent_xid = x11_window.parent_id();

            parent_feature = lv2_raw::core::LV2Feature {
                uri: LV2_UI_PARENT_URI.as_ptr(),
                data: parent_xid as *mut c_void,
            };
            feature_list.push(&parent_feature as *const _);

            // UI resize feature — lets plugin resize the window
            // Store x11_window pointer for the resize callback
            let x11_window_ptr = &mut x11_window as *mut super::x11_ui::X11PluginWindow;
            let mut resize_data = LV2UIResize {
                handle: x11_window_ptr as *mut c_void,
                ui_resize: ui_resize_x11_callback,
            };
            let resize_feature = lv2_raw::core::LV2Feature {
                uri: LV2_UI_RESIZE_URI.as_ptr(),
                data: &mut resize_data as *mut _ as *mut c_void,
            };
            feature_list.push(&resize_feature as *const _);
            feature_list.push(ptr::null());

            // Load UI shared library directly
            let lib = libc::dlopen(c_binary_path.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL);
            if lib.is_null() {
                gtk_widget_destroy(window);
                let _ = Box::from_raw(controller_ptr as *mut UiController);
                log::error!("Failed to dlopen UI binary: {:?}", c_binary_path);
                return;
            }

            // Find the lv2ui_descriptor function
            let desc_sym = libc::dlsym(lib, c"lv2ui_descriptor".as_ptr());
            if desc_sym.is_null() {
                libc::dlclose(lib);
                gtk_widget_destroy(window);
                let _ = Box::from_raw(controller_ptr as *mut UiController);
                log::error!("No lv2ui_descriptor in {:?}", c_binary_path);
                return;
            }

            let lv2ui_descriptor_fn: unsafe extern "C" fn(c_uint) -> *const LV2UI_Descriptor =
                std::mem::transmute(desc_sym);

            // Find the UI descriptor matching our UI URI
            let mut ui_descriptor: *const LV2UI_Descriptor = ptr::null();
            for idx in 0..100 {
                let desc = lv2ui_descriptor_fn(idx);
                if desc.is_null() {
                    break;
                }
                let desc_uri = CStr::from_ptr((*desc).uri);
                if desc_uri == c_ui_uri.as_c_str() {
                    ui_descriptor = desc;
                    break;
                }
            }

            if ui_descriptor.is_null() {
                libc::dlclose(lib);
                gtk_widget_destroy(window);
                let _ = Box::from_raw(controller_ptr as *mut UiController);
                log::error!("UI descriptor not found for {}", req.plugin_uri);
                return;
            }

            // Instantiate the UI directly — with SIGSEGV protection
            use std::sync::atomic::{AtomicBool, Ordering as AtomOrd};
            static X11_CRASHED: AtomicBool = AtomicBool::new(false);
            X11_CRASHED.store(false, AtomOrd::SeqCst);

            static mut X11_SAVED_HANDLER: libc::sigaction = unsafe { std::mem::zeroed() };
            static mut X11_JUMP_BUF: [u8; 256] = [0u8; 256];

            unsafe extern "C" fn x11_crash_handler(_sig: c_int) {
                X11_CRASHED.store(true, AtomOrd::SeqCst);
                unsafe { siglongjmp((&raw mut X11_JUMP_BUF) as *mut u8, 1) };
            }

            unsafe extern "C" {
                #[link_name = "__sigsetjmp"]
                fn x11_sigsetjmp(env: *mut u8, savemask: c_int) -> c_int;
                fn siglongjmp(env: *mut u8, val: c_int) -> !;
            }

            let mut x11_sa: libc::sigaction = std::mem::zeroed();
            x11_sa.sa_sigaction = x11_crash_handler as *const () as usize;
            x11_sa.sa_flags = libc::SA_NODEFER;
            libc::sigaction(libc::SIGSEGV, &x11_sa, (&raw mut X11_SAVED_HANDLER) as *mut _);

            let mut ui_widget: *mut c_void = ptr::null_mut();
            let ui_handle = if x11_sigsetjmp((&raw mut X11_JUMP_BUF) as *mut u8, 1) == 0 {
                if let Some(instantiate_fn) = (*ui_descriptor).instantiate {
                    instantiate_fn(
                        ui_descriptor,
                        c_plugin_uri.as_ptr(),
                        c_bundle_path.as_ptr(),
                        port_write_callback,
                        controller_ptr,
                        &mut ui_widget,
                        feature_list.as_ptr(),
                    )
                } else {
                    ptr::null_mut()
                }
            } else {
                ptr::null_mut()
            };

            libc::sigaction(libc::SIGSEGV, std::ptr::addr_of!(X11_SAVED_HANDLER), ptr::null_mut());

            if ui_handle.is_null() || X11_CRASHED.load(AtomOrd::SeqCst) {
                let method = if X11_CRASHED.load(AtomOrd::SeqCst) { "crashed" } else { "failed" };
                log::warn!(
                    "Direct X11 UI instantiation {} for {} — trying suil/GTK fallback",
                    method, req.plugin_uri,
                );
                libc::dlclose(lib);
                drop(x11_window);

                if X11_CRASHED.load(AtomOrd::SeqCst) {
                    // After a SIGSEGV, the plugin .so state is corrupted.
                    // Suil would try to load the same .so and crash again.
                    // Report error and bail.
                    let _ = Box::from_raw(controller_ptr as *mut UiController);
                    let _ = req.event_tx.send(PwEvent::Plugin(PluginEvent::PluginError {
                        instance_id: Some(instance_id),
                        message: "Plugin UI crashed during instantiation. This plugin may require OpenGL/GLX which is not available in the current display environment.".into(),
                        fatal: false,
                    }));
                    return;
                }
                // Non-crash failure (returned null) — fall through to suil
            } else {

            log::info!(
                "X11 UI: direct instantiation OK, parent XID={}, widget={:?}",
                parent_xid,
                ui_widget,
            );

            // Create a shim suil instance/host (null — we manage the UI lifecycle directly)
            // We store ui_handle and ui_descriptor for cleanup and port_event calls.
            // For now, wrap in the same WindowState structure using suil_instance = null
            // and store the direct handles separately.
            c_x11_host_type = CString::default();
            actual_host_type = ptr::null();

            // Send initial port values
            if let Some(port_event_fn) = (*ui_descriptor).port_event {
                for &(port_index, value) in &req.control_values {
                    let val = value;
                    port_event_fn(
                        ui_handle,
                        port_index as c_uint,
                        std::mem::size_of::<f32>() as c_uint,
                        0,
                        &val as *const f32 as *const c_void,
                    );
                }
            }

            let atom_event_transfer_urid = req
                .urid_mapper
                .map("http://lv2plug.in/ns/ext/atom#eventTransfer");

            // Check for idle interface
            let idle_iface: Option<&'static Lv2UiIdleInterface> =
                if let Some(ext_data) = (*ui_descriptor).extension_data {
                    let ext = ext_data(LV2_UI_IDLE_INTERFACE.as_ptr());
                    if ext.is_null() {
                        None
                    } else {
                        Some(&*(ext as *const Lv2UiIdleInterface))
                    }
                } else {
                    None
                };

            // Show the X11 window
            x11_window.show();

            // Store x11_window on the heap so we can reference it from the timer
            let x11_window_box = Box::into_raw(Box::new(x11_window));

            let closing = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let timer_data = Box::into_raw(Box::new(UiTimerData {
                suil_instance: ptr::null_mut(), // not using suil
                port_updates: req.port_updates.clone(),
                atom_event_transfer_urid,
                idle_iface,
                ui_handle,
                closing: closing.clone(),
                instance_id,
                timer_removed: std::sync::atomic::AtomicBool::new(false),
                prev_control_outputs: vec![f32::NAN; req.port_updates.control_outputs.len()],
                prev_control_inputs: vec![f32::NAN; req.port_updates.control_inputs.len()],
                x11_window: x11_window_box,
            }));

            let timer_id = g_timeout_add(30, Some(ui_timer_callback), timer_data as *mut c_void);

            state.windows.insert(
                instance_id,
                WindowState {
                    instance_id,
                    gtk_window: ptr::null_mut(), // no GTK window for X11 UIs
                    suil_instance: ptr::null_mut(),
                    suil_host: ptr::null_mut(),
                    controller_ptr,
                    timer_data,
                    timer_source_id: timer_id,
                },
            );

            open_ui_set().lock().unwrap().insert(instance_id);
            return;
            } // end of direct X11 success path

            // Direct X11 failed — fall through to suil/GTK below.
            // Recreate the GTK window that was destroyed at the start of the X11 path.
            let window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
            if window.is_null() {
                let _ = Box::from_raw(controller_ptr as *mut UiController);
                log::error!("Failed to create GTK window for suil fallback");
                return;
            }
            let title = CString::new(format!("ZestBay — {}", plugin_name))
                .unwrap_or_else(|_| c"ZestBay — Plugin UI".to_owned());
            gtk_window_set_title(window, title.as_ptr());
            gtk_window_set_default_size(window, -1, -1);

            actual_host_type = host_type.as_ptr();
        } else {
            socket_widget = ptr::null_mut();
            parent_feature = std::mem::zeroed();
            c_x11_host_type = CString::default();
            actual_host_type = host_type.as_ptr();
        }

        // === GTK/suil UI path ===
        feature_list.push(ptr::null());

        let host = suil_host_new(
            Some(port_write_callback),
            Some(port_index_callback),
            None,
            None,
        );
        if host.is_null() {
            gtk_widget_destroy(window);
            let _ = Box::from_raw(controller_ptr as *mut UiController);
            log::error!("Failed to create suil host for instance {}", instance_id);
            return;
        }

        // Install a temporary SIGSEGV handler to catch DPF/Pugl crashes
        // during suil_instance_new. If the plugin crashes, we recover
        // gracefully instead of taking down the whole application.
        use std::sync::atomic::{AtomicBool, Ordering as AtomOrd};
        static SUIL_CRASHED: AtomicBool = AtomicBool::new(false);
        SUIL_CRASHED.store(false, AtomOrd::SeqCst);

        // sigjmp_buf is typically 200 bytes on x86_64 Linux, use 256 for safety
        static mut SAVED_HANDLER: libc::sigaction = unsafe { std::mem::zeroed() };
        static mut JUMP_BUF: [u8; 256] = [0u8; 256];

        unsafe extern "C" {
            #[link_name = "__sigsetjmp"]
            fn sigsetjmp(env: *mut u8, savemask: c_int) -> c_int;
            fn siglongjmp(env: *mut u8, val: c_int) -> !;
        }

        unsafe extern "C" fn crash_handler(_sig: c_int) {
            SUIL_CRASHED.store(true, AtomOrd::SeqCst);
            unsafe { siglongjmp((&raw mut JUMP_BUF) as *mut u8, 1) };
        }

        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = crash_handler as *const () as usize;
        sa.sa_flags = libc::SA_NODEFER;
        libc::sigaction(libc::SIGSEGV, &sa, (&raw mut SAVED_HANDLER) as *mut _);

        let instance = if sigsetjmp((&raw mut JUMP_BUF) as *mut u8, 1) == 0 {
            suil_instance_new(
                host,
                controller_ptr,
                actual_host_type,
                c_plugin_uri.as_ptr(),
                c_ui_uri.as_ptr(),
                c_ui_type_uri.as_ptr(),
                c_bundle_path.as_ptr(),
                c_binary_path.as_ptr(),
                feature_list.as_ptr(),
            )
        } else {
            // Returned here from siglongjmp after a crash
            ptr::null_mut()
        };

        // Restore original signal handler
        libc::sigaction(libc::SIGSEGV, std::ptr::addr_of!(SAVED_HANDLER), ptr::null_mut());

        if SUIL_CRASHED.load(AtomOrd::SeqCst) {
            log::error!(
                "Plugin UI crashed during instantiation (SIGSEGV caught) for instance {}. \
                 This plugin's UI uses a framework (e.g. DPF/Pugl) that is not yet fully supported.",
                instance_id
            );
            suil_host_free(host);
            gtk_widget_destroy(window);
            let _ = Box::from_raw(controller_ptr as *mut UiController);
            let _ = req.event_tx.send(PwEvent::Plugin(PluginEvent::PluginError {
                instance_id: Some(instance_id),
                message: "Plugin UI crashed during instantiation. This plugin's UI framework (DPF/Pugl) is not yet fully supported.".into(),
                fatal: false,
            }));
            return;

            // === Fallback: direct X11 instantiation (Carla-style) ===
            // Reserved for future use — currently disabled because retrying
            // after SIGSEGV corrupts the plugin's shared library state.
            let x11_title = format!("ZestBay — {}", plugin_name);
            let mut x11_window = match super::x11_ui::X11PluginWindow::new(&x11_title) {
                Some(w) => w,
                None => {
                    let _ = Box::from_raw(controller_ptr as *mut UiController);
                    log::error!("Failed to create X11 fallback window");
                    return;
                }
            };

            let parent_xid = x11_window.parent_id();

            // Rebuild feature list for direct instantiation with ui:parent
            // Re-use the existing feature_list entries (minus the null terminator)
            // and add the parent + resize features.
            let mut direct_features: Vec<*const lv2_raw::core::LV2Feature> = Vec::new();
            for &f in &feature_list {
                if f.is_null() { break; }
                direct_features.push(f);
            }

            let direct_parent_feature = lv2_raw::core::LV2Feature {
                uri: LV2_UI_PARENT_URI.as_ptr(),
                data: parent_xid as *mut c_void,
            };
            direct_features.push(&direct_parent_feature as *const _);

            let mut direct_resize_data = LV2UIResize {
                handle: ptr::null_mut(), // updated after Box
                ui_resize: ui_resize_x11_callback,
            };
            let direct_resize_feature = lv2_raw::core::LV2Feature {
                uri: LV2_UI_RESIZE_URI.as_ptr(),
                data: &mut direct_resize_data as *mut _ as *mut c_void,
            };
            direct_features.push(&direct_resize_feature as *const _);
            direct_features.push(ptr::null());

            // Load UI binary directly
            let lib = libc::dlopen(c_binary_path.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL);
            if lib.is_null() {
                drop(x11_window);
                let _ = Box::from_raw(controller_ptr as *mut UiController);
                log::error!("Failed to dlopen UI binary: {:?}", c_binary_path);
                return;
            }

            let desc_sym = libc::dlsym(lib, c"lv2ui_descriptor".as_ptr());
            if desc_sym.is_null() {
                libc::dlclose(lib);
                drop(x11_window);
                let _ = Box::from_raw(controller_ptr as *mut UiController);
                log::error!("No lv2ui_descriptor in {:?}", c_binary_path);
                return;
            }

            let lv2ui_descriptor_fn: unsafe extern "C" fn(c_uint) -> *const LV2UI_Descriptor =
                std::mem::transmute(desc_sym);

            let mut ui_descriptor: *const LV2UI_Descriptor = ptr::null();
            for idx in 0..100u32 {
                let desc = lv2ui_descriptor_fn(idx);
                if desc.is_null() { break; }
                let desc_uri = CStr::from_ptr((*desc).uri);
                if desc_uri == c_ui_uri.as_c_str() {
                    ui_descriptor = desc;
                    break;
                }
            }

            if ui_descriptor.is_null() {
                libc::dlclose(lib);
                drop(x11_window);
                let _ = Box::from_raw(controller_ptr as *mut UiController);
                log::error!("UI descriptor not found for {}", req.plugin_uri);
                return;
            }

            // Protect direct instantiation with SIGSEGV handler too
            SUIL_CRASHED.store(false, AtomOrd::SeqCst);
            libc::sigaction(libc::SIGSEGV, &sa, (&raw mut SAVED_HANDLER) as *mut _);

            let mut ui_widget: *mut c_void = ptr::null_mut();
            let ui_handle = if sigsetjmp((&raw mut JUMP_BUF) as *mut u8, 1) == 0 {
                if let Some(instantiate_fn) = (*ui_descriptor).instantiate {
                    instantiate_fn(
                        ui_descriptor,
                        c_plugin_uri.as_ptr(),
                        c_bundle_path.as_ptr(),
                        port_write_callback,
                        controller_ptr,
                        &mut ui_widget,
                        direct_features.as_ptr(),
                    )
                } else {
                    ptr::null_mut()
                }
            } else {
                ptr::null_mut()
            };

            libc::sigaction(libc::SIGSEGV, std::ptr::addr_of!(SAVED_HANDLER), ptr::null_mut());

            if ui_handle.is_null() || SUIL_CRASHED.load(AtomOrd::SeqCst) {
                libc::dlclose(lib);
                drop(x11_window);
                let _ = Box::from_raw(controller_ptr as *mut UiController);
                let method = if SUIL_CRASHED.load(AtomOrd::SeqCst) { "crashed (SIGSEGV)" } else { "returned null" };
                log::error!("Direct X11 UI instantiation {} for {}", method, req.plugin_uri);
                let _ = req.event_tx.send(PwEvent::Plugin(PluginEvent::PluginError {
                    instance_id: Some(instance_id),
                    message: format!("Plugin UI failed to open (suil and direct X11 both failed). This plugin's UI may require features not yet supported."),
                    fatal: false,
                }));
                return;
            }

            log::info!("X11 UI fallback: direct instantiation OK for instance {}", instance_id);

            // Show the X11 window
            x11_window.show();
            let x11_window_box = Box::into_raw(Box::new(x11_window));

            // Send initial port values
            if let Some(port_event_fn) = (*ui_descriptor).port_event {
                for &(port_index, value) in &req.control_values {
                    let val = value;
                    port_event_fn(
                        ui_handle,
                        port_index as c_uint,
                        std::mem::size_of::<f32>() as c_uint,
                        0,
                        &val as *const f32 as *const c_void,
                    );
                }
            }

            let atom_event_transfer_urid = req
                .urid_mapper
                .map("http://lv2plug.in/ns/ext/atom#eventTransfer");

            let idle_iface: Option<&'static Lv2UiIdleInterface> =
                if let Some(ext_data) = (*ui_descriptor).extension_data {
                    let ext = ext_data(LV2_UI_IDLE_INTERFACE.as_ptr());
                    if ext.is_null() { None } else { Some(&*(ext as *const Lv2UiIdleInterface)) }
                } else { None };

            let closing = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let timer_data = Box::into_raw(Box::new(UiTimerData {
                suil_instance: ptr::null_mut(),
                port_updates: req.port_updates.clone(),
                atom_event_transfer_urid,
                idle_iface,
                ui_handle,
                closing: closing.clone(),
                instance_id,
                timer_removed: std::sync::atomic::AtomicBool::new(false),
                prev_control_outputs: vec![f32::NAN; req.port_updates.control_outputs.len()],
                prev_control_inputs: vec![f32::NAN; req.port_updates.control_inputs.len()],
                x11_window: x11_window_box,
            }));

            let timer_id = g_timeout_add(30, Some(ui_timer_callback), timer_data as *mut c_void);

            state.windows.insert(instance_id, WindowState {
                instance_id,
                gtk_window: ptr::null_mut(),
                suil_instance: ptr::null_mut(),
                suil_host: ptr::null_mut(),
                controller_ptr,
                timer_data,
                timer_source_id: timer_id,
            });

            open_ui_set().lock().unwrap().insert(instance_id);
            return;
        }

        if instance.is_null() {
            suil_host_free(host);
            gtk_widget_destroy(window);
            let _ = Box::from_raw(controller_ptr as *mut UiController);
            log::error!(
                "Failed to create suil instance for instance {}",
                instance_id
            );
            let _ = req.event_tx.send(PwEvent::Plugin(PluginEvent::PluginError {
                instance_id: Some(instance_id),
                message: "Failed to create suil instance".into(),
                fatal: false,
            }));
            return;
        }

        let widget = suil_instance_get_widget(instance);
        if widget.is_null() {
            suil_instance_free(instance);
            suil_host_free(host);
            gtk_widget_destroy(window);
            let _ = Box::from_raw(controller_ptr as *mut UiController);
            log::error!("Failed to get UI widget for instance {}", instance_id);
            return;
        }

        gtk_container_add(window, widget);

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

        let destroy_signal = c"destroy";
        let id_data = Box::into_raw(Box::new(instance_id));
        g_signal_connect_data(
            window,
            destroy_signal.as_ptr(),
            Some(std::mem::transmute::<
                unsafe extern "C" fn(*mut c_void, *mut c_void),
                unsafe extern "C" fn(),
            >(on_window_destroy_multi)),
            id_data as *mut c_void,
            Some(destroy_instance_id_data),
            0,
        );

        gtk_widget_show_all(window);

        let atom_event_transfer_urid = req
            .urid_mapper
            .map("http://lv2plug.in/ns/ext/atom#eventTransfer");

        let idle_iface: Option<&'static Lv2UiIdleInterface> = {
            let ext = suil_instance_extension_data(instance, LV2_UI_IDLE_INTERFACE.as_ptr());
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

        let closing_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        if let Ok(mut flags) = closing_flags().lock() {
            flags.insert(instance_id, closing_flag.clone());
        }

        let n_control_outputs = req.port_updates.control_outputs.len();
        let n_control_inputs = req.port_updates.control_inputs.len();
        let timer_data = Box::into_raw(Box::new(UiTimerData {
            suil_instance: instance,
            port_updates: req.port_updates,
            atom_event_transfer_urid,
            idle_iface,
            ui_handle,
            closing: closing_flag,
            instance_id,
            timer_removed: std::sync::atomic::AtomicBool::new(false),
            // Initialize with NaN so the first tick always sends all values
            prev_control_outputs: vec![f32::NAN; n_control_outputs],
            prev_control_inputs: vec![f32::NAN; n_control_inputs],
            x11_window: ptr::null_mut(),
        }));
        let timer_source_id = g_timeout_add(33, Some(ui_timer_callback), timer_data as *mut c_void);

        state.windows.insert(
            instance_id,
            WindowState {
                instance_id,
                gtk_window: window,
                suil_instance: instance,
                suil_host: host,
                controller_ptr,
                timer_data,
                timer_source_id,
            },
        );

        open_ui_set().lock().unwrap().insert(instance_id);

        log::info!("Plugin UI window opened for instance {}", instance_id);

        let _ = req
            .event_tx
            .send(PwEvent::Plugin(PluginEvent::PluginUiOpened { instance_id }));
    }
}

fn handle_close_window(state: &mut GtkThreadState, instance_id: u64, destroyed_by_gtk: bool) {
    let ws = match state.windows.remove(&instance_id) {
        Some(ws) => ws,
        None => return,
    };

    unsafe {
        (*ws.timer_data)
            .closing
            .store(true, std::sync::atomic::Ordering::Release);

        // Only remove the timer source if it hasn't already been auto-removed
        // by GLib (when the callback returned 0).
        if !(*ws.timer_data).timer_removed.load(std::sync::atomic::Ordering::Acquire) {
            g_source_remove(ws.timer_source_id);
        }

        if !destroyed_by_gtk {
            gtk_widget_destroy(ws.gtk_window);
        }

        (*ws.timer_data).suil_instance = ptr::null_mut();

        suil_instance_free(ws.suil_instance);
        suil_host_free(ws.suil_host);

        let _ = Box::from_raw(ws.timer_data);
        let _ = Box::from_raw(ws.controller_ptr as *mut UiController);
    }

    if let Ok(mut flags) = closing_flags().lock() {
        flags.remove(&instance_id);
    }

    open_ui_set().lock().unwrap().remove(&instance_id);

    log::info!(
        "Plugin UI window closed and resources freed for instance {}",
        instance_id
    );

    if let Some(ref event_tx) = state.event_tx_cache {
        let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginUiClosed { instance_id }));
    }
}

/// Global UI bridge client — lazily spawned on first use.
static UI_BRIDGE: OnceLock<Mutex<Option<super::ui_bridge_client::UiBridgeClient>>> = OnceLock::new();

fn get_or_spawn_bridge(
    event_tx: &Sender<PwEvent>,
    cmd_tx: &Sender<PwCommand>,
) -> bool {
    let bridge_lock = UI_BRIDGE.get_or_init(|| {
        match super::ui_bridge_client::UiBridgeClient::spawn(event_tx.clone(), cmd_tx.clone()) {
            Ok(client) => {
                log::info!("UI bridge process spawned");
                Mutex::new(Some(client))
            }
            Err(e) => {
                log::error!("Failed to spawn UI bridge: {}", e);
                Mutex::new(None)
            }
        }
    });
    bridge_lock.lock().unwrap().is_some()
}

pub fn open_plugin_ui(
    plugin_uri: &str,
    instance_id: u64,
    cmd_tx: Sender<PwCommand>,
    event_tx: Sender<PwEvent>,
    control_values: Vec<(usize, f32)>,
    port_updates: super::types::SharedPortUpdates,
    urid_mapper: Arc<UridMapper>,
    lv2_handle: *mut c_void,
    extension_data_fn: Option<unsafe extern "C" fn(*const c_char) -> *const c_void>,
) {
    // Try to find the UI info to determine if it's X11
    let world = lilv::World::with_load_all();
    let uri_node = world.new_uri(plugin_uri);
    let plugin = world.plugins().iter().find(|p| p.uri().as_uri() == uri_node.as_uri());

    let mut ui_info: Option<(String, String, String, String)> = None;
    if let Some(plugin) = plugin {
        let ui_class_x11 = world.new_uri(LV2_UI_X11);
        let ui_class_gtk3 = world.new_uri(LV2_UI_GTK3);
        let ui_class_gtk2 = world.new_uri(LV2_UI_GTK2);

        if let Some(uis) = plugin.uis() {
            // Check for any UI type
            for ui in uis.iter() {
                for (class_node, type_uri) in [
                    (&ui_class_x11, LV2_UI_X11),
                    (&ui_class_gtk3, LV2_UI_GTK3),
                    (&ui_class_gtk2, LV2_UI_GTK2),
                ] {
                    if ui.is_a(class_node) {
                        let ui_uri = ui.uri().as_uri().map(String::from).unwrap_or_default();
                        let bundle_path = ui.bundle_uri()
                            .and_then(|n| n.path().map(|(_, p)| p))
                            .unwrap_or_default();
                        let binary_path = ui.binary_uri()
                            .and_then(|n| n.path().map(|(_, p)| p))
                            .unwrap_or_default();
                        ui_info = Some((ui_uri, type_uri.to_string(), bundle_path, binary_path));
                        break;
                    }
                }
                if ui_info.is_some() { break; }
            }
        }
    }

    let is_x11_ui = ui_info.as_ref().map_or(false, |(_, t, _, _)| t == LV2_UI_X11);

    // Check if the UI requires instance-access (can only work in-process).
    // We check by scanning the bundle's TTL files for the instance-access URI
    // in a requiredFeature context.
    let needs_instance_access = ui_info.as_ref().map_or(false, |(_, _, bundle, _)| {
        let bundle_dir = std::path::Path::new(bundle);
        if let Ok(entries) = std::fs::read_dir(bundle_dir) {
            for entry in entries.flatten() {
                if entry.path().extension().is_some_and(|e| e == "ttl") {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        if content.contains("instance-access")
                            && content.contains("requiredFeature")
                        {
                            return true;
                        }
                    }
                }
            }
        }
        false
    });

    // For X11 UIs that DON'T need instance-access, use the bridge process
    // (avoids GLX/EGL conflicts on Wayland). Plugins that need instance-access
    // must run in-process via suil/GTK.
    if is_x11_ui && !needs_instance_access {
        if let Some((ref ui_uri, ref ui_type_uri, ref bundle_path, ref binary_path)) = ui_info {
            if get_or_spawn_bridge(&event_tx, &cmd_tx) {
                let bridge_lock = UI_BRIDGE.get().unwrap();
                let guard = bridge_lock.lock().unwrap();
                if let Some(ref client) = *guard {
                    let display_name = plugin_uri.rsplit('/').next().unwrap_or(plugin_uri);
                    log::info!("Using UI bridge for X11 plugin: {}", plugin_uri);
                    client.open_ui(
                        instance_id,
                        plugin_uri,
                        ui_uri,
                        ui_type_uri,
                        bundle_path,
                        binary_path,
                        display_name,
                        control_values.clone(),
                        &urid_mapper,
                        48000.0, // TODO: get actual sample rate
                    );
                    return;
                }
            }
            log::warn!("UI bridge not available, falling back to in-process for X11 UI");
        }
    }

    // For GTK UIs (or X11 fallback), use the old suil/GTK path
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
        lv2_handle,
        extension_data_fn,
    }));
}

pub fn close_plugin_ui(instance_id: u64) {
    if let Some(tx) = GTK_CMD_TX.get()
        && let Ok(tx) = tx.lock()
    {
        let _ = tx.send(GtkCommand::Close {
            instance_id,
            destroyed_by_gtk: false,
        });
    }
}

pub fn shutdown_gtk_thread() {
    if let Some(tx) = GTK_CMD_TX.get()
        && let Ok(tx) = tx.lock()
    {
        let _ = tx.send(GtkCommand::Shutdown);
    }
}
