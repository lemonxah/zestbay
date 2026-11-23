use std::os::raw::{c_char, c_int, c_long, c_uint, c_ulong, c_void};

unsafe extern "C" fn ignore_x_error(_display: *mut c_void, _event: *mut c_void) -> c_int {
    0
}

#[link(name = "X11")]
unsafe extern "C" {
    fn XOpenDisplay(name: *const c_char) -> *mut c_void;
    fn XCloseDisplay(display: *mut c_void) -> c_int;
    fn XCreateSimpleWindow(
        display: *mut c_void,
        parent: c_ulong,
        x: c_int,
        y: c_int,
        width: c_uint,
        height: c_uint,
        border_width: c_uint,
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
    fn XNextEvent(display: *mut c_void, event: *mut XEvent) -> c_int;
    fn XFlush(display: *mut c_void) -> c_int;
    fn XSync(display: *mut c_void, discard: c_int) -> c_int;
    fn XSetErrorHandler(
        handler: Option<unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int>,
    ) -> Option<unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int>;
    fn XStoreName(display: *mut c_void, window: c_ulong, name: *const c_char) -> c_int;
    fn XResizeWindow(display: *mut c_void, window: c_ulong, w: c_uint, h: c_uint) -> c_int;
    fn XSelectInput(display: *mut c_void, window: c_ulong, event_mask: c_long) -> c_int;
    fn XInternAtom(display: *mut c_void, name: *const c_char, only_if_exists: c_int) -> c_ulong;
    fn XSetWMProtocols(
        display: *mut c_void,
        window: c_ulong,
        protocols: *mut c_ulong,
        count: c_int,
    ) -> c_int;
    fn XQueryTree(
        display: *mut c_void,
        window: c_ulong,
        root: *mut c_ulong,
        parent: *mut c_ulong,
        children: *mut *mut c_ulong,
        nchildren: *mut c_uint,
    ) -> c_int;
    fn XSetInputFocus(
        display: *mut c_void,
        window: c_ulong,
        revert_to: c_int,
        time: c_ulong,
    ) -> c_int;
    fn XFree(data: *mut c_void) -> c_int;
}

#[repr(C)]
struct XEvent {
    type_: c_int,
    _pad: [u8; 188],
}

const STRUCTURE_NOTIFY_MASK: c_long = 1 << 17;
const SUBSTRUCTURE_NOTIFY_MASK: c_long = 1 << 19;
const EXPOSURE_MASK: c_long = 1 << 15;
const FOCUS_CHANGE_MASK: c_long = 1 << 21;

const FOCUS_IN: c_int = 9;
const MAP_NOTIFY: c_int = 19;
const REPARENT_NOTIFY: c_int = 21;
const CLIENT_MESSAGE: c_int = 33;

const REVERT_TO_PARENT: c_int = 2;
const CURRENT_TIME: c_ulong = 0;

const CLIENT_MESSAGE_DATA_OFFSET: usize = 56;

pub struct HostWindow {
    display: *mut c_void,
    pub window: c_ulong,
    wm_delete: c_ulong,
    child: c_ulong,
}

unsafe impl Send for HostWindow {}

impl HostWindow {
    pub unsafe fn open(title: &str, width: u32, height: u32) -> Option<Self> {
        unsafe {
            let display = XOpenDisplay(std::ptr::null());
            if display.is_null() {
                log::error!("X11: cannot open display for plugin GUI");
                return None;
            }
            let screen = XDefaultScreen(display);
            let root = XDefaultRootWindow(display);
            let window = XCreateSimpleWindow(
                display,
                root,
                0,
                0,
                width.max(1),
                height.max(1),
                0,
                XBlackPixel(display, screen),
                XWhitePixel(display, screen),
            );
            if window == 0 {
                log::error!("X11: XCreateSimpleWindow failed");
                XCloseDisplay(display);
                return None;
            }

            let title = std::ffi::CString::new(title).unwrap_or_default();
            XStoreName(display, window, title.as_ptr());
            XSelectInput(
                display,
                window,
                STRUCTURE_NOTIFY_MASK | SUBSTRUCTURE_NOTIFY_MASK | EXPOSURE_MASK | FOCUS_CHANGE_MASK,
            );
            let wm_delete = XInternAtom(display, c"WM_DELETE_WINDOW".as_ptr(), 0);
            let mut protocols = [wm_delete];
            XSetWMProtocols(display, window, protocols.as_mut_ptr(), 1);

            XMapWindow(display, window);
            XSync(display, 0);

            Some(Self {
                display,
                window,
                wm_delete,
                child: 0,
            })
        }
    }

    pub unsafe fn resize(&self, width: u32, height: u32) {
        unsafe {
            XResizeWindow(self.display, self.window, width.max(1), height.max(1));
            XFlush(self.display);
        }
    }

    pub unsafe fn flush(&self) {
        unsafe {
            XFlush(self.display);
        }
    }

    pub unsafe fn pump(&mut self) -> bool {
        unsafe {
            let mut close = false;
            while XPending(self.display) > 0 {
                let mut event: XEvent = std::mem::zeroed();
                XNextEvent(self.display, &mut event);
                match event.type_ {
                    CLIENT_MESSAGE => {
                        let atom = std::ptr::read_unaligned(
                            (&event as *const XEvent as *const u8).add(CLIENT_MESSAGE_DATA_OFFSET)
                                as *const c_ulong,
                        );
                        if atom == self.wm_delete {
                            close = true;
                        }
                    }
                    REPARENT_NOTIFY | MAP_NOTIFY => {
                        let child = self.first_child();
                        if child != 0 && child != self.child {
                            self.child = child;
                            log::debug!("X11: plugin child window 0x{:x} detected", child);
                        }
                    }
                    FOCUS_IN if self.child != 0 => {
                        let previous = XSetErrorHandler(Some(ignore_x_error));
                        XSetInputFocus(self.display, self.child, REVERT_TO_PARENT, CURRENT_TIME);
                        XSync(self.display, 0);
                        XSetErrorHandler(previous);
                    }
                    _ => {}
                }
            }
            close
        }
    }

    unsafe fn first_child(&self) -> c_ulong {
        unsafe {
            let mut root_ret: c_ulong = 0;
            let mut parent_ret: c_ulong = 0;
            let mut children: *mut c_ulong = std::ptr::null_mut();
            let mut n: c_uint = 0;
            let status = XQueryTree(
                self.display,
                self.window,
                &mut root_ret,
                &mut parent_ret,
                &mut children,
                &mut n,
            );
            let child = if status != 0 && n > 0 && !children.is_null() {
                *children
            } else {
                0
            };
            if !children.is_null() {
                XFree(children as *mut c_void);
            }
            child
        }
    }
}

impl Drop for HostWindow {
    fn drop(&mut self) {
        unsafe {
            XDestroyWindow(self.display, self.window);
            XSync(self.display, 0);
            XCloseDisplay(self.display);
        }
    }
}
