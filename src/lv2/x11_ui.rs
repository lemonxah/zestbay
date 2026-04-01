//! Standalone X11 window for hosting LV2 X11 plugin UIs.
//!
//! This bypasses GTK and suil entirely for X11 UIs, matching Carla's approach:
//! - Opens its own X11 display connection
//! - Creates a top-level window as the plugin parent
//! - Pumps X11 events via a GLib timer (GTK thread integration)
//! - Handles resize, focus forwarding, and close

use std::ffi::{CString, c_void};
use std::os::raw::{c_char, c_int, c_uint, c_ulong};
use std::ptr;

#[link(name = "X11")]
unsafe extern "C" {
    fn XOpenDisplay(name: *const c_char) -> *mut c_void;
    fn XCloseDisplay(display: *mut c_void) -> c_int;
    fn XCreateWindow(
        display: *mut c_void,
        parent: c_ulong,
        x: c_int,
        y: c_int,
        width: c_uint,
        height: c_uint,
        border_width: c_uint,
        depth: c_int,
        class: c_uint,
        visual: *mut c_void,
        valuemask: c_ulong,
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
    fn XSetInputFocus(
        display: *mut c_void,
        window: c_ulong,
        revert_to: c_int,
        time: c_ulong,
    ) -> c_int;
    fn XQueryTree(
        display: *mut c_void,
        window: c_ulong,
        root: *mut c_ulong,
        parent: *mut c_ulong,
        children: *mut *mut c_ulong,
        nchildren: *mut c_uint,
    ) -> c_int;
    fn XFree(data: *mut c_void) -> c_int;
    fn XGetWindowAttributes(
        display: *mut c_void,
        window: c_ulong,
        attrs: *mut XWindowAttributes,
    ) -> c_int;
    fn XInternAtom(display: *mut c_void, name: *const c_char, only_if_exists: c_int) -> c_ulong;
    fn XSetWMProtocols(
        display: *mut c_void,
        window: c_ulong,
        protocols: *mut c_ulong,
        count: c_int,
    ) -> c_int;
    fn XChangeProperty(
        display: *mut c_void,
        window: c_ulong,
        property: c_ulong,
        type_: c_ulong,
        format: c_int,
        mode: c_int,
        data: *const u8,
        nelements: c_int,
    ) -> c_int;
    fn XStoreName(display: *mut c_void, window: c_ulong, name: *const c_char) -> c_int;
    fn XSetTransientForHint(display: *mut c_void, window: c_ulong, prop: c_ulong) -> c_int;
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
const REVERT_TO_POINTER_ROOT: c_int = 1;
const CURRENT_TIME: c_ulong = 0;

#[repr(C)]
struct XSetWindowAttributes {
    background_pixmap: c_ulong,
    background_pixel: c_ulong,
    border_pixmap: c_ulong,
    border_pixel: c_ulong,
    bit_gravity: c_int,
    win_gravity: c_int,
    backing_store: c_int,
    backing_planes: c_ulong,
    backing_pixel: c_ulong,
    save_under: c_int,
    event_mask: c_ulong,
    do_not_propagate_mask: c_ulong,
    override_redirect: c_int,
    colormap: c_ulong,
    cursor: c_ulong,
}

// XEvent is a large union — we only need enough bytes and specific fields
#[repr(C)]
struct XEvent {
    type_: c_int,
    _pad: [u8; 188], // XEvent is 192 bytes total on 64-bit
}

#[repr(C)]
struct XConfigureEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut c_void,
    event: c_ulong,
    window: c_ulong,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
    border_width: c_int,
    above: c_ulong,
    override_redirect: c_int,
}

#[repr(C)]
struct XClientMessageEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut c_void,
    window: c_ulong,
    message_type: c_ulong,
    format: c_int,
    data_l: [c_ulong; 5],
}

#[repr(C)]
struct XFocusChangeEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut c_void,
    window: c_ulong,
    mode: c_int,
    detail: c_int,
}

#[repr(C)]
struct XWindowAttributes {
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
    border_width: c_int,
    depth: c_int,
    visual: *mut c_void,
    root: c_ulong,
    class: c_int,
    bit_gravity: c_int,
    win_gravity: c_int,
    backing_store: c_int,
    backing_planes: c_ulong,
    backing_pixel: c_ulong,
    save_under: c_int,
    colormap: c_ulong,
    map_installed: c_int,
    map_state: c_int,
    all_event_masks: c_ulong,
    your_event_mask: c_ulong,
    do_not_propagate_mask: c_ulong,
    override_redirect: c_int,
    screen: *mut c_void,
}

const IS_VIEWABLE: c_int = 2;

pub struct X11PluginWindow {
    display: *mut c_void,
    pub host_window: c_ulong,
    child_window: c_ulong,
    wm_delete: c_ulong,
    pub closed: bool,
}

impl X11PluginWindow {
    /// Create a new X11 host window for embedding a plugin UI.
    pub fn new(title: &str) -> Option<Self> {
        unsafe {
            let display = XOpenDisplay(ptr::null());
            if display.is_null() {
                log::error!("X11PluginWindow: XOpenDisplay failed");
                return None;
            }

            let screen = XDefaultScreen(display);
            let depth = XDefaultDepth(display, screen);
            let visual = XDefaultVisual(display, screen);
            let root = XDefaultRootWindow(display);

            let mut attr: XSetWindowAttributes = std::mem::zeroed();
            attr.border_pixel = 0;
            attr.event_mask = KEY_PRESS_MASK
                | KEY_RELEASE_MASK
                | FOCUS_CHANGE_MASK
                | STRUCTURE_NOTIFY_MASK
                | SUBSTRUCTURE_NOTIFY_MASK;

            let host_window = XCreateWindow(
                display,
                root,
                0,
                0,
                300,
                300,
                0,
                depth,
                INPUT_OUTPUT,
                visual,
                CW_BORDER_PIXEL | CW_EVENT_MASK,
                &mut attr,
            );

            if host_window == 0 {
                XCloseDisplay(display);
                log::error!("X11PluginWindow: XCreateWindow failed");
                return None;
            }

            // Set window title
            let c_title = CString::new(title).unwrap_or_else(|_| c"ZestBay Plugin".to_owned());
            XStoreName(display, host_window, c_title.as_ptr());

            // Handle WM close
            let wm_delete = XInternAtom(display, c"WM_DELETE_WINDOW".as_ptr(), 0);
            let mut protocols = [wm_delete];
            XSetWMProtocols(display, host_window, protocols.as_mut_ptr(), 1);

            // Window type: dialog + normal (decorated floating)
            let wt = XInternAtom(display, c"_NET_WM_WINDOW_TYPE".as_ptr(), 0);
            let wt_dialog = XInternAtom(display, c"_NET_WM_WINDOW_TYPE_DIALOG".as_ptr(), 0);
            let wt_normal = XInternAtom(display, c"_NET_WM_WINDOW_TYPE_NORMAL".as_ptr(), 0);
            let wt_values = [wt_dialog, wt_normal];
            XChangeProperty(
                display,
                host_window,
                wt,
                XA_ATOM,
                32,
                PROP_MODE_REPLACE,
                wt_values.as_ptr() as *const u8,
                2,
            );

            // Set PID
            let pid = libc::getpid() as c_ulong;
            let nwp = XInternAtom(display, c"_NET_WM_PID".as_ptr(), 0);
            XChangeProperty(
                display,
                host_window,
                nwp,
                XA_CARDINAL,
                32,
                PROP_MODE_REPLACE,
                &pid as *const c_ulong as *const u8,
                1,
            );

            XFlush(display);

            Some(Self {
                display,
                host_window,
                child_window: 0,
                wm_delete,
                closed: false,
            })
        }
    }

    /// Get the X11 Window ID for use as LV2 ui:parent.
    pub fn parent_id(&self) -> usize {
        self.host_window as usize
    }

    /// Show the window.
    pub fn show(&mut self) {
        unsafe {
            // Auto-detect child window
            if self.child_window == 0 {
                self.child_window = self.find_child_window();
            }

            // If child has a size, resize host to match
            if self.child_window != 0 {
                let mut wa: XWindowAttributes = std::mem::zeroed();
                if XGetWindowAttributes(self.display, self.child_window, &mut wa) != 0
                    && wa.width > 0
                    && wa.height > 0
                {
                    XResizeWindow(
                        self.display,
                        self.host_window,
                        wa.width as c_uint,
                        wa.height as c_uint,
                    );
                }
            }

            XMapRaised(self.display, self.host_window);
            XSync(self.display, 0);
        }
    }

    /// Hide the window.
    pub fn hide(&mut self) {
        unsafe {
            XUnmapWindow(self.display, self.host_window);
            XFlush(self.display);
        }
    }

    /// Process pending X11 events. Call this from a timer.
    pub fn idle(&mut self) {
        unsafe {
            let mut next_child_w: c_uint = 0;
            let mut next_child_h: c_uint = 0;

            while XPending(self.display) > 0 {
                let mut event: XEvent = std::mem::zeroed();
                XNextEvent(self.display, &mut event);

                match event.type_ {
                    CONFIGURE_NOTIFY => {
                        let cfg = &*(&event as *const XEvent as *const XConfigureEvent);
                        if cfg.window == self.child_window && cfg.width > 0 && cfg.height > 0 {
                            next_child_w = cfg.width as c_uint;
                            next_child_h = cfg.height as c_uint;
                        }
                    }
                    CLIENT_MESSAGE => {
                        let cm = &*(&event as *const XEvent as *const XClientMessageEvent);
                        if cm.data_l[0] == self.wm_delete {
                            self.closed = true;
                            self.hide();
                        }
                    }
                    FOCUS_IN => {
                        // Forward focus to child
                        if self.child_window == 0 {
                            self.child_window = self.find_child_window();
                        }
                        if self.child_window != 0 {
                            let mut wa: XWindowAttributes = std::mem::zeroed();
                            if XGetWindowAttributes(self.display, self.child_window, &mut wa) != 0
                                && wa.map_state == IS_VIEWABLE
                            {
                                XSetInputFocus(
                                    self.display,
                                    self.child_window,
                                    REVERT_TO_POINTER_ROOT,
                                    CURRENT_TIME,
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }

            // Resize host to match child
            if next_child_w > 0 && next_child_h > 0 {
                XResizeWindow(self.display, self.host_window, next_child_w, next_child_h);
                XFlush(self.display);
            }
        }
    }

    /// Set the window size.
    pub fn set_size(&mut self, width: u32, height: u32) {
        unsafe {
            XResizeWindow(self.display, self.host_window, width as c_uint, height as c_uint);
            if self.child_window != 0 {
                XResizeWindow(
                    self.display,
                    self.child_window,
                    width as c_uint,
                    height as c_uint,
                );
            }
            XSync(self.display, 0);
        }
    }

    fn find_child_window(&self) -> c_ulong {
        unsafe {
            let mut root: c_ulong = 0;
            let mut parent: c_ulong = 0;
            let mut children: *mut c_ulong = ptr::null_mut();
            let mut nchildren: c_uint = 0;

            if XQueryTree(
                self.display,
                self.host_window,
                &mut root,
                &mut parent,
                &mut children,
                &mut nchildren,
            ) != 0
                && nchildren > 0
                && !children.is_null()
            {
                let child = *children;
                XFree(children as *mut c_void);
                child
            } else {
                if !children.is_null() {
                    XFree(children as *mut c_void);
                }
                0
            }
        }
    }
}

impl Drop for X11PluginWindow {
    fn drop(&mut self) {
        unsafe {
            if self.host_window != 0 {
                XDestroyWindow(self.display, self.host_window);
            }
            if !self.display.is_null() {
                XCloseDisplay(self.display);
            }
        }
    }
}
