use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_uint, c_ulong, c_void};
use std::ptr;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, OnceLock};

use crate::lv2::urid::UridMapper;
use crate::pipewire::{Lv2Event, PwCommand, PwEvent};

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

const GTK_WINDOW_TOPLEVEL: c_int = 0;

const LV2_UI_IDLE_INTERFACE: &CStr = c"http://lv2plug.in/ns/extensions/ui#idleInterface";

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
}

unsafe extern "C" fn ui_timer_callback(data: *mut c_void) -> c_int {
    if data.is_null() {
        return 0;
    }
    let td = unsafe { &*(data as *const UiTimerData) };
    if td.suil_instance.is_null() || td.closing.load(std::sync::atomic::Ordering::Acquire) {
        return 0;
    }

    for slot in td.port_updates.control_outputs.iter() {
        let val = slot.value.load();
        unsafe {
            suil_instance_port_event(
                td.suil_instance,
                slot.port_index as c_uint,
                std::mem::size_of::<f32>() as c_uint,
                0,
                &val as *const f32 as *const c_void,
            );
        }
    }

    for slot in td.port_updates.control_inputs.iter() {
        let val = slot.value.load();
        unsafe {
            suil_instance_port_event(
                td.suil_instance,
                slot.port_index as c_uint,
                std::mem::size_of::<f32>() as c_uint,
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

    if let Some(idle_iface) = td.idle_iface
        && let Some(idle_fn) = idle_iface.idle
    {
        let result = unsafe { idle_fn(td.ui_handle) };
        if result != 0 {
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
}
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
            let _ = req.event_tx.send(PwEvent::Lv2(Lv2Event::PluginError {
                instance_id: Some(instance_id),
                message: format!("No supported UI found for plugin: {}", req.plugin_uri),
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

    unsafe {
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
            log::error!(
                "Failed to create suil instance for instance {}",
                instance_id
            );
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

        gtk_container_add(window, widget);
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

        let timer_data = Box::into_raw(Box::new(UiTimerData {
            suil_instance: instance,
            port_updates: req.port_updates,
            atom_event_transfer_urid,
            idle_iface,
            ui_handle,
            closing: closing_flag,
            instance_id,
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
            .send(PwEvent::Lv2(Lv2Event::PluginUiOpened { instance_id }));
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

        g_source_remove(ws.timer_source_id);

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
        let _ = event_tx.send(PwEvent::Lv2(Lv2Event::PluginUiClosed { instance_id }));
    }
}

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
