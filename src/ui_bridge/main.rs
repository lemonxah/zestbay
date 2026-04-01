//! ZestBay UI Bridge — standalone process for hosting LV2 plugin UIs.
//!
//! This binary runs in a separate process from the main ZestBay application.
//! It creates native X11 windows and loads LV2 UI plugins directly (no suil,
//! no GTK), matching the approach used by Carla. This avoids GLX/EGL conflicts
//! that occur when Qt6/Wayland and X11/GLX coexist in the same process.
//!
//! Communication with the host is via stdin/stdout JSON messages.

mod protocol;

use protocol::{BridgeMessage, HostMessage};
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_void};
use std::io::{BufRead, Write};
use std::os::raw::{c_char, c_int, c_uint, c_ulong};
use std::ptr;

// ---------------------------------------------------------------------------
// X11 FFI
// ---------------------------------------------------------------------------

#[link(name = "X11")]
unsafe extern "C" {
    fn XOpenDisplay(name: *const c_char) -> *mut c_void;
    fn XCloseDisplay(display: *mut c_void) -> c_int;
    fn XCreateWindow(
        display: *mut c_void, parent: c_ulong,
        x: c_int, y: c_int, width: c_uint, height: c_uint,
        border_width: c_uint, depth: c_int, class: c_uint,
        visual: *mut c_void, valuemask: c_ulong,
        attributes: *mut XSetWindowAttributes,
    ) -> c_ulong;
    fn XDestroyWindow(display: *mut c_void, window: c_ulong) -> c_int;
    fn XMapRaised(display: *mut c_void, window: c_ulong) -> c_int;
    fn XUnmapWindow(display: *mut c_void, window: c_ulong) -> c_int;
    fn XResizeWindow(display: *mut c_void, window: c_ulong, w: c_uint, h: c_uint) -> c_int;
    fn XSync(display: *mut c_void, discard: c_int) -> c_int;
    fn XFlush(display: *mut c_void) -> c_int;
    fn XPending(display: *mut c_void) -> c_int;
    fn XNextEvent(display: *mut c_void, event: *mut XEvent) -> c_int;
    fn XSetInputFocus(display: *mut c_void, window: c_ulong, revert: c_int, time: c_ulong) -> c_int;
    fn XQueryTree(
        display: *mut c_void, window: c_ulong,
        root: *mut c_ulong, parent: *mut c_ulong,
        children: *mut *mut c_ulong, nchildren: *mut c_uint,
    ) -> c_int;
    fn XFree(data: *mut c_void) -> c_int;
    fn XGetWindowAttributes(display: *mut c_void, window: c_ulong, attrs: *mut XWindowAttributes) -> c_int;
    fn XInternAtom(display: *mut c_void, name: *const c_char, only_if_exists: c_int) -> c_ulong;
    fn XSetWMProtocols(display: *mut c_void, window: c_ulong, protocols: *mut c_ulong, count: c_int) -> c_int;
    fn XChangeProperty(
        display: *mut c_void, window: c_ulong, property: c_ulong,
        type_: c_ulong, format: c_int, mode: c_int,
        data: *const u8, nelements: c_int,
    ) -> c_int;
    fn XStoreName(display: *mut c_void, window: c_ulong, name: *const c_char) -> c_int;
    fn XDefaultScreen(display: *mut c_void) -> c_int;
    fn XDefaultDepth(display: *mut c_void, screen: c_int) -> c_int;
    fn XDefaultVisual(display: *mut c_void, screen: c_int) -> *mut c_void;
    fn XDefaultRootWindow(display: *mut c_void) -> c_ulong;
}

const INPUT_OUTPUT: c_uint = 1;
const CW_BORDER_PIXEL: c_ulong = 1 << 3;
const CW_EVENT_MASK: c_ulong = 1 << 11;
const KEY_PRESS_MASK: c_ulong = 1 << 0;
const KEY_RELEASE_MASK: c_ulong = 1 << 1;
const STRUCTURE_NOTIFY_MASK: c_ulong = 1 << 17;
const SUBSTRUCTURE_NOTIFY_MASK: c_ulong = 1 << 19;
const FOCUS_CHANGE_MASK: c_ulong = 1 << 21;
const CONFIGURE_NOTIFY: c_int = 22;
const CLIENT_MESSAGE: c_int = 33;
const FOCUS_IN: c_int = 9;
const PROP_MODE_REPLACE: c_int = 0;
const XA_ATOM: c_ulong = 4;
const XA_CARDINAL: c_ulong = 6;
const IS_VIEWABLE: c_int = 2;

#[repr(C)]
struct XSetWindowAttributes {
    background_pixmap: c_ulong, background_pixel: c_ulong,
    border_pixmap: c_ulong, border_pixel: c_ulong,
    bit_gravity: c_int, win_gravity: c_int,
    backing_store: c_int, backing_planes: c_ulong, backing_pixel: c_ulong,
    save_under: c_int, event_mask: c_ulong, do_not_propagate_mask: c_ulong,
    override_redirect: c_int, colormap: c_ulong, cursor: c_ulong,
}

#[repr(C)]
struct XEvent { type_: c_int, _pad: [u8; 188] }

#[repr(C)]
struct XConfigureEvent {
    type_: c_int, serial: c_ulong, send_event: c_int, display: *mut c_void,
    event: c_ulong, window: c_ulong,
    x: c_int, y: c_int, width: c_int, height: c_int,
    border_width: c_int, above: c_ulong, override_redirect: c_int,
}

#[repr(C)]
struct XClientMessageEvent {
    type_: c_int, serial: c_ulong, send_event: c_int, display: *mut c_void,
    window: c_ulong, message_type: c_ulong, format: c_int,
    data_l: [c_ulong; 5],
}

#[repr(C)]
struct XWindowAttributes {
    x: c_int, y: c_int, width: c_int, height: c_int,
    border_width: c_int, depth: c_int, visual: *mut c_void,
    root: c_ulong, class: c_int, bit_gravity: c_int, win_gravity: c_int,
    backing_store: c_int, backing_planes: c_ulong, backing_pixel: c_ulong,
    save_under: c_int, colormap: c_ulong, map_installed: c_int,
    map_state: c_int, all_event_masks: c_ulong, your_event_mask: c_ulong,
    do_not_propagate_mask: c_ulong, override_redirect: c_int, screen: *mut c_void,
}

// ---------------------------------------------------------------------------
// LV2 UI types
// ---------------------------------------------------------------------------

const LV2_UI_PARENT_URI: &CStr = c"http://lv2plug.in/ns/extensions/ui#parent";
const LV2_UI_RESIZE_URI: &CStr = c"http://lv2plug.in/ns/extensions/ui#resize";
const LV2_UI_IDLE_INTERFACE: &CStr = c"http://lv2plug.in/ns/extensions/ui#idleInterface";
const LV2_URID_MAP_URI: &CStr = c"http://lv2plug.in/ns/ext/urid#map";
const LV2_OPTIONS_URI: &CStr = c"http://lv2plug.in/ns/ext/options#options";

#[repr(C)]
struct LV2Feature {
    uri: *const c_char,
    data: *mut c_void,
}

#[repr(C)]
struct LV2UI_Descriptor {
    uri: *const c_char,
    instantiate: Option<unsafe extern "C" fn(
        descriptor: *const LV2UI_Descriptor,
        plugin_uri: *const c_char,
        bundle_path: *const c_char,
        write_function: unsafe extern "C" fn(*mut c_void, c_uint, c_uint, c_uint, *const c_void),
        controller: *mut c_void,
        widget: *mut *mut c_void,
        features: *const *const LV2Feature,
    ) -> *mut c_void>,
    cleanup: Option<unsafe extern "C" fn(handle: *mut c_void)>,
    port_event: Option<unsafe extern "C" fn(*mut c_void, c_uint, c_uint, c_uint, *const c_void)>,
    extension_data: Option<unsafe extern "C" fn(*const c_char) -> *const c_void>,
}

#[repr(C)]
struct LV2UridMap {
    handle: *mut c_void,
    map: unsafe extern "C" fn(handle: *mut c_void, uri: *const c_char) -> u32,
}

#[repr(C)]
struct LV2UIResize {
    handle: *mut c_void,
    ui_resize: unsafe extern "C" fn(handle: *mut c_void, width: c_int, height: c_int) -> c_int,
}

#[repr(C)]
struct LV2UiIdleInterface {
    idle: Option<unsafe extern "C" fn(handle: *mut c_void) -> c_int>,
}

#[repr(C)]
struct LV2Option {
    context: u32, subject: u32, key: u32, size: u32, type_: u32, value: *const c_void,
}

// ---------------------------------------------------------------------------
// Bridge state
// ---------------------------------------------------------------------------

struct UridMapper {
    map: HashMap<String, u32>,
    next_id: u32,
}

impl UridMapper {
    fn new(initial: Vec<(String, u32)>) -> Self {
        let mut map = HashMap::new();
        let mut max_id = 1u32;
        for (uri, id) in initial {
            if id > max_id { max_id = id; }
            map.insert(uri, id);
        }
        Self { map, next_id: max_id + 1 }
    }

    fn map_uri(&mut self, uri: &str) -> u32 {
        if let Some(&id) = self.map.get(uri) {
            return id;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.map.insert(uri.to_string(), id);
        id
    }
}

static mut URID_MAPPER: *mut UridMapper = ptr::null_mut();

unsafe extern "C" fn urid_map_callback(handle: *mut c_void, uri: *const c_char) -> u32 {
    unsafe {
        if uri.is_null() { return 0; }
        let mapper = &mut *URID_MAPPER;
        let uri_str = CStr::from_ptr(uri).to_str().unwrap_or("");
        mapper.map_uri(uri_str)
    }
}

struct PluginUiWindow {
    instance_id: u64,
    display: *mut c_void,
    host_window: c_ulong,
    child_window: c_ulong,
    wm_delete: c_ulong,
    ui_handle: *mut c_void,
    descriptor: *const LV2UI_Descriptor,
    idle_iface: Option<*const LV2UiIdleInterface>,
    closed: bool,
    _lib: *mut c_void,
}

// Global for the port write callback to find
static mut BRIDGE_STDOUT: *mut std::io::Stdout = ptr::null_mut();
static mut CURRENT_INSTANCE_ID: u64 = 0;

unsafe extern "C" fn port_write_callback(
    _controller: *mut c_void,
    port_index: c_uint,
    buffer_size: c_uint,
    protocol: c_uint,
    buffer: *const c_void,
) {
    unsafe {
        if protocol == 0 && buffer_size == 4 && !buffer.is_null() {
            let value = *(buffer as *const f32);
            let msg = BridgeMessage::PortWrite {
                instance_id: CURRENT_INSTANCE_ID,
                port_index: port_index as usize,
                value,
            };
            if let Ok(json) = serde_json::to_string(&msg) {
                let stdout = &mut *BRIDGE_STDOUT;
                let _ = writeln!(stdout, "{}", json);
                let _ = stdout.flush();
            }
        }
    }
}

unsafe extern "C" fn ui_resize_callback(
    handle: *mut c_void,
    width: c_int,
    height: c_int,
) -> c_int {
    unsafe {
        if !handle.is_null() {
            let win = handle as *mut PluginUiWindow;
            XResizeWindow((*win).display, (*win).host_window, width as c_uint, height as c_uint);
            if (*win).child_window != 0 {
                XResizeWindow((*win).display, (*win).child_window, width as c_uint, height as c_uint);
            }
            XFlush((*win).display);
        }
    }
    0
}

// ---------------------------------------------------------------------------
// Window management
// ---------------------------------------------------------------------------

fn open_ui(msg: HostMessage, windows: &mut HashMap<u64, PluginUiWindow>) -> BridgeMessage {
    let (instance_id, plugin_uri, ui_uri, bundle_path, binary_path, title,
         control_values, urid_map_initial, sample_rate) = match msg {
        HostMessage::Open {
            instance_id, plugin_uri, ui_uri, bundle_path, binary_path,
            title, control_values, urid_map, sample_rate, ..
        } => (instance_id, plugin_uri, ui_uri, bundle_path, binary_path,
              title, control_values, urid_map, sample_rate),
        _ => unreachable!(),
    };

    unsafe {
        // Initialize URID mapper with host's mappings
        let mapper = Box::new(UridMapper::new(urid_map_initial));
        URID_MAPPER = Box::into_raw(mapper);

        // Open our own X11 display (clean, no GTK/Qt/Wayland interference)
        let display = XOpenDisplay(ptr::null());
        if display.is_null() {
            return BridgeMessage::OpenFailed {
                instance_id,
                error: "XOpenDisplay failed".into(),
            };
        }

        let screen = XDefaultScreen(display);
        let depth = XDefaultDepth(display, screen);
        let visual = XDefaultVisual(display, screen);
        let root = XDefaultRootWindow(display);

        let mut attr: XSetWindowAttributes = std::mem::zeroed();
        attr.border_pixel = 0;
        attr.event_mask = KEY_PRESS_MASK | KEY_RELEASE_MASK | FOCUS_CHANGE_MASK
            | STRUCTURE_NOTIFY_MASK | SUBSTRUCTURE_NOTIFY_MASK;

        let host_window = XCreateWindow(
            display, root, 0, 0, 300, 300, 0,
            depth, INPUT_OUTPUT, visual,
            CW_BORDER_PIXEL | CW_EVENT_MASK, &mut attr,
        );

        if host_window == 0 {
            XCloseDisplay(display);
            return BridgeMessage::OpenFailed {
                instance_id,
                error: "XCreateWindow failed".into(),
            };
        }

        // Window properties
        let c_title = CString::new(title).unwrap_or_else(|_| c"ZestBay Plugin".to_owned());
        XStoreName(display, host_window, c_title.as_ptr());

        let wm_delete = XInternAtom(display, c"WM_DELETE_WINDOW".as_ptr(), 0);
        let mut protocols = [wm_delete];
        XSetWMProtocols(display, host_window, protocols.as_mut_ptr(), 1);

        let wt = XInternAtom(display, c"_NET_WM_WINDOW_TYPE".as_ptr(), 0);
        let wt_dialog = XInternAtom(display, c"_NET_WM_WINDOW_TYPE_DIALOG".as_ptr(), 0);
        let wt_normal = XInternAtom(display, c"_NET_WM_WINDOW_TYPE_NORMAL".as_ptr(), 0);
        let wt_values = [wt_dialog, wt_normal];
        XChangeProperty(display, host_window, wt, XA_ATOM, 32, PROP_MODE_REPLACE,
                        wt_values.as_ptr() as *const u8, 2);

        let pid = libc::getpid() as c_ulong;
        let nwp = XInternAtom(display, c"_NET_WM_PID".as_ptr(), 0);
        XChangeProperty(display, host_window, nwp, XA_CARDINAL, 32, PROP_MODE_REPLACE,
                        &pid as *const c_ulong as *const u8, 1);

        // Build LV2 features
        let mut urid_map_struct = LV2UridMap {
            handle: ptr::null_mut(),
            map: urid_map_callback,
        };
        let urid_feature = LV2Feature {
            uri: LV2_URID_MAP_URI.as_ptr(),
            data: &mut urid_map_struct as *mut _ as *mut c_void,
        };

        let parent_feature = LV2Feature {
            uri: LV2_UI_PARENT_URI.as_ptr(),
            data: host_window as *mut c_void,
        };

        // Allocate PluginUiWindow early so resize callback can reference it
        let mut plugin_win = Box::new(PluginUiWindow {
            instance_id,
            display,
            host_window,
            child_window: 0,
            wm_delete,
            ui_handle: ptr::null_mut(),
            descriptor: ptr::null(),
            idle_iface: None,
            closed: false,
            _lib: ptr::null_mut(),
        });

        let mut resize_data = LV2UIResize {
            handle: &mut *plugin_win as *mut _ as *mut c_void,
            ui_resize: ui_resize_callback,
        };
        let resize_feature = LV2Feature {
            uri: LV2_UI_RESIZE_URI.as_ptr(),
            data: &mut resize_data as *mut _ as *mut c_void,
        };

        // Options with sample rate
        let sr_urid = (*URID_MAPPER).map_uri("http://lv2plug.in/ns/ext/parameters#sampleRate");
        let float_urid = (*URID_MAPPER).map_uri("http://lv2plug.in/ns/ext/atom#Float");
        let options = [
            LV2Option { context: 0, subject: 0, key: sr_urid, size: 4, type_: float_urid,
                         value: &sample_rate as *const f32 as *const c_void },
            LV2Option { context: 0, subject: 0, key: 0, size: 0, type_: 0, value: ptr::null() },
        ];
        let options_feature = LV2Feature {
            uri: LV2_OPTIONS_URI.as_ptr(),
            data: options.as_ptr() as *mut c_void,
        };

        let features: Vec<*const LV2Feature> = vec![
            &urid_feature as *const _,
            &parent_feature as *const _,
            &resize_feature as *const _,
            &options_feature as *const _,
            ptr::null(),
        ];

        // Load UI binary
        let c_binary = CString::new(binary_path.as_str()).unwrap();
        let lib = libc::dlopen(c_binary.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL);
        if lib.is_null() {
            XDestroyWindow(display, host_window);
            XCloseDisplay(display);
            return BridgeMessage::OpenFailed {
                instance_id,
                error: format!("dlopen failed: {:?}", c_binary),
            };
        }

        let desc_sym = libc::dlsym(lib, c"lv2ui_descriptor".as_ptr());
        if desc_sym.is_null() {
            libc::dlclose(lib);
            XDestroyWindow(display, host_window);
            XCloseDisplay(display);
            return BridgeMessage::OpenFailed {
                instance_id,
                error: "No lv2ui_descriptor symbol".into(),
            };
        }

        let lv2ui_descriptor_fn: unsafe extern "C" fn(c_uint) -> *const LV2UI_Descriptor =
            std::mem::transmute(desc_sym);

        let c_ui_uri = CString::new(ui_uri.as_str()).unwrap();
        let mut ui_descriptor: *const LV2UI_Descriptor = ptr::null();
        for idx in 0..100u32 {
            let desc = lv2ui_descriptor_fn(idx);
            if desc.is_null() { break; }
            if CStr::from_ptr((*desc).uri) == c_ui_uri.as_c_str() {
                ui_descriptor = desc;
                break;
            }
        }

        if ui_descriptor.is_null() {
            libc::dlclose(lib);
            XDestroyWindow(display, host_window);
            XCloseDisplay(display);
            return BridgeMessage::OpenFailed {
                instance_id,
                error: "UI descriptor not found".into(),
            };
        }

        // Instantiate
        CURRENT_INSTANCE_ID = instance_id;
        let c_plugin_uri = CString::new(plugin_uri.as_str()).unwrap();
        let c_bundle = CString::new(bundle_path.as_str()).unwrap();
        let mut widget: *mut c_void = ptr::null_mut();

        let ui_handle = if let Some(instantiate_fn) = (*ui_descriptor).instantiate {
            instantiate_fn(
                ui_descriptor,
                c_plugin_uri.as_ptr(),
                c_bundle.as_ptr(),
                port_write_callback,
                ptr::null_mut(),
                &mut widget,
                features.as_ptr(),
            )
        } else {
            ptr::null_mut()
        };

        if ui_handle.is_null() {
            libc::dlclose(lib);
            XDestroyWindow(display, host_window);
            XCloseDisplay(display);
            return BridgeMessage::OpenFailed {
                instance_id,
                error: "UI instantiate returned null".into(),
            };
        }

        // Check for idle interface
        let idle_iface = if let Some(ext_data) = (*ui_descriptor).extension_data {
            let ext = ext_data(LV2_UI_IDLE_INTERFACE.as_ptr());
            if ext.is_null() { None } else { Some(ext as *const LV2UiIdleInterface) }
        } else {
            None
        };

        // Send initial port values
        if let Some(port_event_fn) = (*ui_descriptor).port_event {
            for &(port_index, value) in &control_values {
                let val = value;
                port_event_fn(
                    ui_handle, port_index as c_uint,
                    std::mem::size_of::<f32>() as c_uint,
                    0, &val as *const f32 as *const c_void,
                );
            }
        }

        // Show window — auto-detect child size
        let child = find_child_window(display, host_window);
        if child != 0 {
            let mut wa: XWindowAttributes = std::mem::zeroed();
            if XGetWindowAttributes(display, child, &mut wa) != 0 && wa.width > 0 && wa.height > 0 {
                XResizeWindow(display, host_window, wa.width as c_uint, wa.height as c_uint);
            }
        }

        XMapRaised(display, host_window);
        XSync(display, 0);

        plugin_win.ui_handle = ui_handle;
        plugin_win.descriptor = ui_descriptor;
        plugin_win.idle_iface = idle_iface;
        plugin_win.child_window = child;
        plugin_win._lib = lib;

        windows.insert(instance_id, *plugin_win);

        BridgeMessage::Opened { instance_id }
    }
}

fn find_child_window(display: *mut c_void, parent: c_ulong) -> c_ulong {
    unsafe {
        let mut root: c_ulong = 0;
        let mut par: c_ulong = 0;
        let mut children: *mut c_ulong = ptr::null_mut();
        let mut nchildren: c_uint = 0;

        if XQueryTree(display, parent, &mut root, &mut par, &mut children, &mut nchildren) != 0
            && nchildren > 0 && !children.is_null()
        {
            let child = *children;
            XFree(children as *mut c_void);
            child
        } else {
            if !children.is_null() { XFree(children as *mut c_void); }
            0
        }
    }
}

fn idle_window(win: &mut PluginUiWindow) {
    unsafe {
        while XPending(win.display) > 0 {
            let mut event: XEvent = std::mem::zeroed();
            XNextEvent(win.display, &mut event);

            match event.type_ {
                CONFIGURE_NOTIFY => {
                    let cfg = &*(&event as *const XEvent as *const XConfigureEvent);
                    if cfg.window != win.host_window && cfg.width > 0 && cfg.height > 0 {
                        if win.child_window == 0 { win.child_window = cfg.window; }
                        XResizeWindow(win.display, win.host_window, cfg.width as c_uint, cfg.height as c_uint);
                    }
                }
                CLIENT_MESSAGE => {
                    let cm = &*(&event as *const XEvent as *const XClientMessageEvent);
                    if cm.data_l[0] == win.wm_delete {
                        win.closed = true;
                        XUnmapWindow(win.display, win.host_window);
                    }
                }
                FOCUS_IN => {
                    if win.child_window == 0 {
                        win.child_window = find_child_window(win.display, win.host_window);
                    }
                    if win.child_window != 0 {
                        let mut wa: XWindowAttributes = std::mem::zeroed();
                        if XGetWindowAttributes(win.display, win.child_window, &mut wa) != 0
                            && wa.map_state == IS_VIEWABLE
                        {
                            XSetInputFocus(win.display, win.child_window, 1, 0);
                        }
                    }
                }
                _ => {}
            }
        }

        // Call idle interface
        if let Some(iface) = win.idle_iface {
            if let Some(idle_fn) = (*iface).idle {
                if idle_fn(win.ui_handle) != 0 {
                    win.closed = true;
                }
            }
        }
    }
}

fn close_window(win: &mut PluginUiWindow) {
    unsafe {
        if let Some(cleanup) = (*win.descriptor).cleanup {
            cleanup(win.ui_handle);
        }
        win.ui_handle = ptr::null_mut();
        XDestroyWindow(win.display, win.host_window);
        win.host_window = 0;
        if !win._lib.is_null() {
            // Don't dlclose — plugin may have statics
        }
        XCloseDisplay(win.display);
        win.display = ptr::null_mut();
    }
}

// ---------------------------------------------------------------------------
// Main loop
// ---------------------------------------------------------------------------

fn send_message(stdout: &mut std::io::Stdout, msg: &BridgeMessage) {
    if let Ok(json) = serde_json::to_string(msg) {
        let _ = writeln!(stdout, "{}", json);
        let _ = stdout.flush();
    }
}

fn main() {
    let mut stdout = std::io::stdout();
    unsafe { BRIDGE_STDOUT = &mut stdout as *mut _; }

    let stdin = std::io::stdin();
    let mut stdin_lines = stdin.lock().lines();
    let mut windows: HashMap<u64, PluginUiWindow> = HashMap::new();

    // Set stdin to non-blocking so we can interleave with X11 events
    unsafe {
        let fd = libc::STDIN_FILENO;
        let flags = libc::fcntl(fd, libc::F_GETFL);
        libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
    }

    send_message(&mut stdout, &BridgeMessage::Ready);

    loop {
        // Read pending messages from host (non-blocking)
        let mut got_quit = false;
        loop {
            match stdin_lines.next() {
                Some(Ok(line)) if !line.is_empty() => {
                    if let Ok(msg) = serde_json::from_str::<HostMessage>(&line) {
                        match msg {
                            HostMessage::Quit => { got_quit = true; break; }
                            HostMessage::Open { .. } => {
                                let result = open_ui(msg, &mut windows);
                                send_message(&mut stdout, &result);
                            }
                            HostMessage::PortEvent { instance_id, port_index, value } => {
                                if let Some(win) = windows.get(&instance_id) {
                                    unsafe {
                                        if let Some(pe) = (*win.descriptor).port_event {
                                            let val = value;
                                            pe(win.ui_handle, port_index as c_uint,
                                               std::mem::size_of::<f32>() as c_uint,
                                               0, &val as *const f32 as *const c_void);
                                        }
                                    }
                                }
                            }
                            HostMessage::Close { instance_id } => {
                                if let Some(mut win) = windows.remove(&instance_id) {
                                    close_window(&mut win);
                                    send_message(&mut stdout, &BridgeMessage::Closed { instance_id });
                                }
                            }
                        }
                    }
                }
                _ => break, // No more data or would block
            }
        }

        if got_quit { break; }

        // Idle all windows
        let mut closed_ids = Vec::new();
        for (id, win) in windows.iter_mut() {
            idle_window(win);
            if win.closed {
                closed_ids.push(*id);
            }
        }

        for id in closed_ids {
            if let Some(mut win) = windows.remove(&id) {
                close_window(&mut win);
                send_message(&mut stdout, &BridgeMessage::Closed { instance_id: id });
            }
        }

        if windows.is_empty() && got_quit {
            break;
        }

        // Sleep briefly to avoid busy-waiting (30fps UI refresh)
        std::thread::sleep(std::time::Duration::from_millis(16));
    }

    // Cleanup remaining windows
    for (_, mut win) in windows.drain() {
        close_window(&mut win);
    }
}
