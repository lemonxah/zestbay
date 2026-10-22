//! ZestBay UI Bridge — standalone process for hosting LV2 plugin UIs.
//!
//! This binary runs in a separate process from the main ZestBay application.
//! It creates native X11 windows and loads LV2 UI plugins directly (no suil,
//! no GTK), matching the approach used by Carla. This avoids GLX/EGL conflicts
//! that occur when Qt6/Wayland and X11/GLX coexist in the same process.
//!
//! Communication with the host is via stdin/stdout newline-delimited JSON.
//! When stdin reaches EOF (host exited or crashed) all windows are closed
//! and the bridge exits.

mod protocol;

use protocol::{BridgeMessage, HostMessage};
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_void};
use std::io::Write;
use std::os::raw::{c_char, c_int, c_long, c_uint, c_ulong};
use std::ptr;
use std::sync::Mutex;

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

/// XEvent is a 24-long union; only `type_` and casts to specific events are used.
#[repr(C)]
struct XEvent { pad: [c_long; 24] }

impl XEvent {
    fn type_(&self) -> c_int {
        unsafe { *(self as *const XEvent as *const c_int) }
    }
}

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
// URID mapping
// ---------------------------------------------------------------------------

/// URIDs minted by the bridge itself start here so they never collide with
/// the host's (which are small and dense).
const LOCAL_URID_BASE: u32 = 1 << 30;

struct UridMapper {
    map: HashMap<String, u32>,
    next_id: u32,
}

impl UridMapper {
    fn new() -> Self {
        Self { map: HashMap::new(), next_id: LOCAL_URID_BASE }
    }

    /// Merge the host's mappings (authoritative).
    fn merge(&mut self, entries: Vec<(String, u32)>) {
        for (uri, id) in entries {
            self.map.insert(uri, id);
        }
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

/// One process-wide mapper shared by all UIs.
static URID_MAPPER: Mutex<Option<UridMapper>> = Mutex::new(None);

fn map_uri(uri: &str) -> u32 {
    let mut guard = URID_MAPPER.lock().unwrap_or_else(|e| e.into_inner());
    guard.get_or_insert_with(UridMapper::new).map_uri(uri)
}

unsafe extern "C" fn urid_map_callback(_handle: *mut c_void, uri: *const c_char) -> u32 {
    if uri.is_null() {
        return 0;
    }
    let uri_str = unsafe { CStr::from_ptr(uri) }.to_str().unwrap_or("");
    map_uri(uri_str)
}

// ---------------------------------------------------------------------------
// Host communication
// ---------------------------------------------------------------------------

fn send_message(msg: &BridgeMessage) {
    if let Ok(mut line) = serde_json::to_vec(msg) {
        line.push(b'\n');
        let mut stdout = std::io::stdout().lock();
        let _ = stdout.write_all(&line);
        let _ = stdout.flush();
    }
}

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { TABLE[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[n as usize & 63] as char } else { '=' });
    }
    out
}

/// Remove and return the next complete line (without '\n') from `buf`.
fn take_line(buf: &mut Vec<u8>) -> Option<Vec<u8>> {
    let pos = buf.iter().position(|&b| b == b'\n')?;
    let mut line: Vec<u8> = buf.drain(..=pos).collect();
    line.pop();
    Some(line)
}

// ---------------------------------------------------------------------------
// Plugin UI windows
// ---------------------------------------------------------------------------

/// The `LV2UI_Controller` handed to each UI: identifies which instance a
/// port write belongs to.
struct UiController {
    instance_id: u64,
}

/// Everything one UI references.  Always boxed and never moved while the
/// UI lives: the UI holds pointers to the controller and feature data.
struct PluginUiWindow {
    display: *mut c_void,
    host_window: c_ulong,
    child_window: c_ulong,
    wm_delete: c_ulong,
    ui_handle: *mut c_void,
    descriptor: *const LV2UI_Descriptor,
    idle_iface: Option<*const LV2UiIdleInterface>,
    closed: bool,
    _lib: *mut c_void,
    controller: UiController,
    urid_map: LV2UridMap,
    resize: LV2UIResize,
    sample_rate: f32,
    options: [LV2Option; 2],
    features: Vec<LV2Feature>,
    feature_ptrs: Vec<*const LV2Feature>,
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
        send_message(&BridgeMessage::PortWrite {
            instance_id: ctrl.instance_id,
            port_index: port_index as usize,
            value,
        });
    } else if protocol != 0 && buffer_size > 0 {
        let data = unsafe { std::slice::from_raw_parts(buffer as *const u8, buffer_size as usize) };
        send_message(&BridgeMessage::AtomWrite {
            instance_id: ctrl.instance_id,
            port_index: port_index as usize,
            data_b64: base64_encode(data),
        });
    }
}

unsafe extern "C" fn ui_resize_callback(
    handle: *mut c_void,
    width: c_int,
    height: c_int,
) -> c_int {
    unsafe {
        if !handle.is_null() && width > 0 && height > 0 {
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

fn open_ui(msg: HostMessage, windows: &mut HashMap<u64, Box<PluginUiWindow>>) -> BridgeMessage {
    let HostMessage::Open {
        instance_id, plugin_uri, ui_uri, bundle_path, binary_path,
        title, control_values, urid_map, sample_rate, ..
    } = msg else {
        unreachable!()
    };

    // A re-open replaces the old window.
    if let Some(mut old) = windows.remove(&instance_id) {
        close_window(&mut old);
    }

    URID_MAPPER
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(UridMapper::new)
        .merge(urid_map);

    let failed = |error: String| BridgeMessage::OpenFailed { instance_id, error };

    let (Ok(c_binary), Ok(c_ui_uri), Ok(c_plugin_uri), Ok(c_bundle)) = (
        CString::new(binary_path.as_str()),
        CString::new(ui_uri.as_str()),
        CString::new(plugin_uri.as_str()),
        CString::new(bundle_path.as_str()),
    ) else {
        return failed("invalid path or URI".into());
    };

    unsafe {
        // Open our own X11 display (clean, no GTK/Qt/Wayland interference)
        let display = XOpenDisplay(ptr::null());
        if display.is_null() {
            return failed("XOpenDisplay failed".into());
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
            return failed("XCreateWindow failed".into());
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

        let destroy_display = || {
            XDestroyWindow(display, host_window);
            XCloseDisplay(display);
        };

        // All UI-visible state lives in this box; pointers into it stay
        // valid because the box itself is stored in `windows`.
        let sr_urid = map_uri("http://lv2plug.in/ns/ext/parameters#sampleRate");
        let float_urid = map_uri("http://lv2plug.in/ns/ext/atom#Float");
        let mut win = Box::new(PluginUiWindow {
            display,
            host_window,
            child_window: 0,
            wm_delete,
            ui_handle: ptr::null_mut(),
            descriptor: ptr::null(),
            idle_iface: None,
            closed: false,
            _lib: ptr::null_mut(),
            controller: UiController { instance_id },
            urid_map: LV2UridMap { handle: ptr::null_mut(), map: urid_map_callback },
            resize: LV2UIResize { handle: ptr::null_mut(), ui_resize: ui_resize_callback },
            sample_rate,
            options: [
                LV2Option { context: 0, subject: 0, key: sr_urid, size: 4, type_: float_urid,
                            value: ptr::null() },
                LV2Option { context: 0, subject: 0, key: 0, size: 0, type_: 0, value: ptr::null() },
            ],
            features: Vec::new(),
            feature_ptrs: Vec::new(),
        });
        let win_ptr: *mut PluginUiWindow = &mut *win;
        win.resize.handle = win_ptr as *mut c_void;
        win.options[0].value = &win.sample_rate as *const f32 as *const c_void;
        win.features = vec![
            LV2Feature { uri: LV2_URID_MAP_URI.as_ptr(), data: &mut win.urid_map as *mut _ as *mut c_void },
            LV2Feature { uri: LV2_UI_PARENT_URI.as_ptr(), data: host_window as *mut c_void },
            LV2Feature { uri: LV2_UI_RESIZE_URI.as_ptr(), data: &mut win.resize as *mut _ as *mut c_void },
            LV2Feature { uri: LV2_OPTIONS_URI.as_ptr(), data: win.options.as_ptr() as *mut c_void },
        ];
        win.feature_ptrs = win
            .features
            .iter()
            .map(|f| f as *const LV2Feature)
            .chain(std::iter::once(ptr::null()))
            .collect();

        // Load UI binary
        let lib = libc::dlopen(c_binary.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL);
        if lib.is_null() {
            destroy_display();
            return failed(format!("dlopen failed: {:?}", c_binary));
        }

        let desc_sym = libc::dlsym(lib, c"lv2ui_descriptor".as_ptr());
        if desc_sym.is_null() {
            libc::dlclose(lib);
            destroy_display();
            return failed("No lv2ui_descriptor symbol".into());
        }

        let lv2ui_descriptor_fn: unsafe extern "C" fn(c_uint) -> *const LV2UI_Descriptor =
            std::mem::transmute(desc_sym);

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
            destroy_display();
            return failed("UI descriptor not found".into());
        }

        // Instantiate
        let mut widget: *mut c_void = ptr::null_mut();
        let controller_ptr = &mut win.controller as *mut UiController as *mut c_void;
        let ui_handle = if let Some(instantiate_fn) = (*ui_descriptor).instantiate {
            instantiate_fn(
                ui_descriptor,
                c_plugin_uri.as_ptr(),
                c_bundle.as_ptr(),
                port_write_callback,
                controller_ptr,
                &mut widget,
                win.feature_ptrs.as_ptr(),
            )
        } else {
            ptr::null_mut()
        };

        if ui_handle.is_null() {
            libc::dlclose(lib);
            destroy_display();
            return failed("UI instantiate returned null".into());
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

        win.ui_handle = ui_handle;
        win.descriptor = ui_descriptor;
        win.idle_iface = idle_iface;
        win.child_window = child;
        win._lib = lib;

        windows.insert(instance_id, win);

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

            match event.type_() {
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
        if let Some(iface) = win.idle_iface
            && let Some(idle_fn) = (*iface).idle
            && idle_fn(win.ui_handle) != 0
        {
            win.closed = true;
        }
    }
}

fn close_window(win: &mut PluginUiWindow) {
    unsafe {
        if !win.ui_handle.is_null()
            && !win.descriptor.is_null()
            && let Some(cleanup) = (*win.descriptor).cleanup
        {
            cleanup(win.ui_handle);
        }
        win.ui_handle = ptr::null_mut();
        if !win.display.is_null() {
            XDestroyWindow(win.display, win.host_window);
            win.host_window = 0;
            // Don't dlclose — plugin may have statics
            XCloseDisplay(win.display);
            win.display = ptr::null_mut();
        }
    }
}

fn port_event(win: &PluginUiWindow, port_index: usize, value: f32) {
    unsafe {
        if win.ui_handle.is_null() || win.descriptor.is_null() {
            return;
        }
        if let Some(pe) = (*win.descriptor).port_event {
            pe(win.ui_handle, port_index as c_uint,
               std::mem::size_of::<f32>() as c_uint,
               0, &value as *const f32 as *const c_void);
        }
    }
}

// ---------------------------------------------------------------------------
// Main loop
// ---------------------------------------------------------------------------

enum Flow {
    Continue,
    Quit,
}

fn handle_message(msg: HostMessage, windows: &mut HashMap<u64, Box<PluginUiWindow>>) -> Flow {
    match msg {
        HostMessage::Quit => return Flow::Quit,
        HostMessage::Open { .. } => {
            let result = open_ui(msg, windows);
            send_message(&result);
        }
        HostMessage::PortEvent { instance_id, port_index, value } => {
            if let Some(win) = windows.get(&instance_id) {
                port_event(win, port_index, value);
            }
        }
        HostMessage::Close { instance_id } => {
            if let Some(mut win) = windows.remove(&instance_id) {
                close_window(&mut win);
                send_message(&BridgeMessage::Closed { instance_id });
            }
        }
    }
    Flow::Continue
}

/// Read whatever is available on (non-blocking) stdin into `buf`.
/// Returns false on EOF or a fatal read error.
fn read_stdin(buf: &mut Vec<u8>) -> bool {
    let mut chunk = [0u8; 4096];
    loop {
        let n = unsafe { libc::read(libc::STDIN_FILENO, chunk.as_mut_ptr() as *mut c_void, chunk.len()) };
        if n > 0 {
            buf.extend_from_slice(&chunk[..n as usize]);
            continue;
        }
        if n == 0 {
            return false;
        }
        let err = std::io::Error::last_os_error();
        match err.kind() {
            std::io::ErrorKind::WouldBlock => return true,
            std::io::ErrorKind::Interrupted => continue,
            _ => {
                eprintln!("zestbay-ui-bridge: stdin read error: {}", err);
                return false;
            }
        }
    }
}

fn main() {
    let mut windows: HashMap<u64, Box<PluginUiWindow>> = HashMap::new();

    // Set stdin to non-blocking so we can interleave with X11 events
    unsafe {
        let fd = libc::STDIN_FILENO;
        let flags = libc::fcntl(fd, libc::F_GETFL);
        libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
    }

    send_message(&BridgeMessage::Ready);

    let mut inbuf: Vec<u8> = Vec::new();
    loop {
        let stdin_open = read_stdin(&mut inbuf);

        let mut quit = false;
        while let Some(line) = take_line(&mut inbuf) {
            if line.is_empty() {
                continue;
            }
            match serde_json::from_slice::<HostMessage>(&line) {
                Ok(msg) => {
                    if let Flow::Quit = handle_message(msg, &mut windows) {
                        quit = true;
                        break;
                    }
                }
                Err(e) => eprintln!("zestbay-ui-bridge: invalid message: {}", e),
            }
        }

        if quit || !stdin_open {
            break;
        }

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
                send_message(&BridgeMessage::Closed { instance_id: id });
            }
        }

        // Sleep briefly to avoid busy-waiting (~60 fps UI refresh)
        std::thread::sleep(std::time::Duration::from_millis(16));
    }

    // Host quit or went away: close everything.
    for (_, mut win) in windows.drain() {
        close_window(&mut win);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_line_handles_partial_input() {
        let mut buf = b"{\"a\":1}\n{\"b\"".to_vec();
        assert_eq!(take_line(&mut buf), Some(b"{\"a\":1}".to_vec()));
        assert_eq!(take_line(&mut buf), None);
        buf.extend_from_slice(b":2}\n");
        assert_eq!(take_line(&mut buf), Some(b"{\"b\":2}".to_vec()));
        assert!(buf.is_empty());
    }

    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_encode(&[0xff, 0xfe, 0x00]), "//4A");
    }

    #[test]
    fn local_urids_do_not_collide_with_host() {
        let mut m = UridMapper::new();
        m.merge(vec![("urn:a".into(), 1), ("urn:b".into(), 2)]);
        assert_eq!(m.map_uri("urn:a"), 1);
        assert!(m.map_uri("urn:new") >= LOCAL_URID_BASE);
    }

    #[test]
    fn xevent_is_xlib_sized() {
        assert_eq!(std::mem::size_of::<XEvent>(), 24 * std::mem::size_of::<c_long>());
    }
}
