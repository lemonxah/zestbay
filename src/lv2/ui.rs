use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_uint, c_ulong, c_void};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

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

const LV2_URID_MAP_URI: &CStr = c"http://lv2plug.in/ns/ext/urid#map";
const LV2_DATA_ACCESS_URI: &CStr = c"http://lv2plug.in/ns/ext/data-access";
const LV2_INSTANCE_ACCESS_URI: &CStr = c"http://lv2plug.in/ns/ext/instance-access";
const LV2_OPTIONS_URI: &CStr = c"http://lv2plug.in/ns/ext/options#options";

/// How long `close_plugin_ui` waits for the GTK thread to destroy a UI.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(2);
/// Upper bound on atom output messages forwarded to a UI per timer tick.
const MAX_ATOM_MESSAGES_PER_TICK: usize = 64;

#[repr(C)]
struct LV2ExtensionDataFeature {
    data_access: unsafe extern "C" fn(*const c_char) -> *const c_void,
}

#[repr(C)]
struct Lv2UiIdleInterface {
    idle: Option<unsafe extern "C" fn(ui_handle: *mut c_void) -> c_int>,
}

#[repr(C)]
struct Lv2Option {
    context: u32,
    subject: u32,
    key: u32,
    size: u32,
    type_: u32,
    value: *const c_void,
}

const LV2_UI_GTK2: &str = "http://lv2plug.in/ns/extensions/ui#GtkUI";
const LV2_UI_GTK3: &str = "http://lv2plug.in/ns/extensions/ui#Gtk3UI";
const LV2_UI_GTK4: &str = "http://lv2plug.in/ns/extensions/ui#Gtk4UI";
const LV2_UI_X11: &str = "http://lv2plug.in/ns/extensions/ui#X11UI";
const LV2_UI_QT5: &str = "http://lv2plug.in/ns/extensions/ui#Qt5UI";

const HOST_TYPE_URI: &str = "http://lv2plug.in/ns/extensions/ui#Gtk3UI";

/// Feature data handed to an in-process plugin UI.  The UI may keep
/// pointers into it for its whole lifetime, so it is heap-allocated and
/// freed only after the suil instance has been destroyed.
struct UiHostFeatures {
    _urid_mapper: Arc<UridMapper>,
    urid_map: lv2_raw::urid::LV2UridMap,
    data_access: Option<LV2ExtensionDataFeature>,
    sample_rate: f32,
    options: [Lv2Option; 2],
    features: Vec<lv2_raw::core::LV2Feature>,
    feature_ptrs: Vec<*const lv2_raw::core::LV2Feature>,
}

impl UiHostFeatures {
    fn new(
        urid_mapper: &Arc<UridMapper>,
        extension_data_fn: Option<unsafe extern "C" fn(*const c_char) -> *const c_void>,
        lv2_handle: *mut c_void,
        sample_rate: f32,
    ) -> Box<Self> {
        let sample_rate_urid = urid_mapper.map("http://lv2plug.in/ns/ext/parameters#sampleRate");
        let atom_float_urid = urid_mapper.map("http://lv2plug.in/ns/ext/atom#Float");

        let mut b = Box::new(UiHostFeatures {
            _urid_mapper: urid_mapper.clone(),
            urid_map: urid_mapper.as_lv2_urid_map(),
            data_access: extension_data_fn.map(|f| LV2ExtensionDataFeature { data_access: f }),
            sample_rate,
            options: [
                Lv2Option {
                    context: 0, // LV2_OPTIONS_INSTANCE
                    subject: 0,
                    key: sample_rate_urid,
                    size: std::mem::size_of::<f32>() as u32,
                    type_: atom_float_urid,
                    value: ptr::null(),
                },
                Lv2Option {
                    context: 0,
                    subject: 0,
                    key: 0,
                    size: 0,
                    type_: 0,
                    value: ptr::null(),
                },
            ],
            features: Vec::new(),
            feature_ptrs: Vec::new(),
        });

        // All pointers below point into the box, which never moves.
        b.options[0].value = &b.sample_rate as *const f32 as *const c_void;

        let mut features = Vec::with_capacity(4);
        features.push(lv2_raw::core::LV2Feature {
            uri: LV2_URID_MAP_URI.as_ptr(),
            data: &mut b.urid_map as *mut _ as *mut c_void,
        });
        if let Some(ref mut da) = b.data_access {
            features.push(lv2_raw::core::LV2Feature {
                uri: LV2_DATA_ACCESS_URI.as_ptr(),
                data: da as *mut _ as *mut c_void,
            });
        }
        if !lv2_handle.is_null() {
            features.push(lv2_raw::core::LV2Feature {
                uri: LV2_INSTANCE_ACCESS_URI.as_ptr(),
                data: lv2_handle,
            });
        }
        features.push(lv2_raw::core::LV2Feature {
            uri: LV2_OPTIONS_URI.as_ptr(),
            data: b.options.as_ptr() as *mut c_void,
        });
        b.features = features;
        b.feature_ptrs = b
            .features
            .iter()
            .map(|f| f as *const _)
            .chain(std::iter::once(ptr::null()))
            .collect();
        b
    }

    fn as_ptr(&self) -> *const *const lv2_raw::core::LV2Feature {
        self.feature_ptrs.as_ptr()
    }
}

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
                    if !atom_in.write(data) {
                        log::warn!(
                            "UI atom queue full for instance {} port {}; message dropped",
                            ctrl.instance_id,
                            port_index
                        );
                    }
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
    gtk_window: *mut c_void,
    suil_instance: *mut c_void,
    suil_host: *mut c_void,
    controller_ptr: *mut c_void,
    timer_data: *mut UiTimerData,
    timer_source_id: c_uint,
    /// Feature data the UI may reference; freed after the suil instance.
    _features: Box<UiHostFeatures>,
}

struct UiTimerData {
    suil_instance: *mut c_void,
    port_updates: super::types::SharedPortUpdates,
    atom_event_transfer_urid: u32,
    idle_iface: Option<&'static Lv2UiIdleInterface>,
    ui_handle: *mut c_void,
    closing: Arc<AtomicBool>,
    instance_id: u64,
    /// Set to true when the timer callback returns 0 (auto-removed by GLib),
    /// so handle_close_window knows not to call g_source_remove again.
    timer_removed: AtomicBool,
    /// Cached control output values for change detection — only forward to
    /// the UI when a value actually changed, to avoid overwhelming plugin UIs.
    prev_control_outputs: Vec<f32>,
    /// Cached control input values for change detection.
    prev_control_inputs: Vec<f32>,
}

/// Forward each event of an `atom:Sequence` (as written by the plugin) to the UI.
fn forward_sequence_events(td: &UiTimerData, port_index: usize, data: &[u8]) {
    if data.len() < 16 {
        return;
    }
    let seq_body_size = u32::from_ne_bytes([data[0], data[1], data[2], data[3]]) as usize;
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
                port_index as c_uint,
                atom_total as c_uint,
                td.atom_event_transfer_urid as c_uint,
                data[event_atom_offset..].as_ptr() as *const c_void,
            );
        }

        let padded_size = (atom_total + 7) & !7;
        offset += 8 + padded_size;
    }
}

unsafe extern "C" fn ui_timer_callback(data: *mut c_void) -> c_int {
    if data.is_null() {
        return 0;
    }
    // SAFETY: we need mutable access for the prev_control caches
    let td = unsafe { &mut *(data as *mut UiTimerData) };
    if td.closing.load(Ordering::Acquire) || td.suil_instance.is_null() {
        td.timer_removed.store(true, Ordering::Release);
        return 0;
    }

    let suil_instance = td.suil_instance;
    let send_port_event = |port_index: usize, value: f32| unsafe {
        suil_instance_port_event(
            suil_instance,
            port_index as c_uint,
            std::mem::size_of::<f32>() as c_uint,
            0,
            &value as *const f32 as *const c_void,
        );
    };

    // Forward control outputs to UI, but only when the value changed
    for (i, slot) in td.port_updates.control_outputs.iter().enumerate() {
        let val = slot.value.load();
        let prev = td.prev_control_outputs.get(i).copied().unwrap_or(f32::NAN);
        if val.to_bits() != prev.to_bits() {
            if i < td.prev_control_outputs.len() {
                td.prev_control_outputs[i] = val;
            }
            send_port_event(slot.port_index, val);
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
            send_port_event(slot.port_index, val);
        }
    }

    if td.atom_event_transfer_urid != 0 {
        for atom_buf in td.port_updates.atom_outputs.iter() {
            for _ in 0..MAX_ATOM_MESSAGES_PER_TICK {
                let Some(data) = atom_buf.read() else {
                    break;
                };
                forward_sequence_events(td, atom_buf.port_index, &data);
            }
        }
    }

    if let Some(idle_iface) = td.idle_iface
        && let Some(idle_fn) = idle_iface.idle
    {
        let result = unsafe { idle_fn(td.ui_handle) };
        if result != 0 {
            td.timer_removed.store(true, Ordering::Release);
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
        /// Dropped (or sent to) once the window is gone.
        ack: Option<std::sync::mpsc::Sender<()>>,
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
    sample_rate: f32,
}

// SAFETY: lv2_handle is a raw LV2 plugin handle that must cross from the PW
// thread to the GTK thread.  The GTK thread uses it read-only for data-access
// and instance-access UI features.  The plugin instance outlives the UI
// (close_plugin_ui blocks until the UI is destroyed).
unsafe impl Send for OpenUiRequest {}
type OnceArcMutex<A> = OnceLock<Arc<Mutex<A>>>;

static GTK_CMD_TX: OnceLock<Mutex<Sender<GtkCommand>>> = OnceLock::new();
static GTK_THREAD_ID: OnceLock<std::thread::ThreadId> = OnceLock::new();

static OPEN_UI_SET: OnceArcMutex<std::collections::HashSet<u64>> = OnceLock::new();

static CLOSING_FLAGS: OnceArcMutex<HashMap<u64, Arc<AtomicBool>>> = OnceLock::new();

/// Instances whose GtkWindow has already been destroyed by GTK (user closed
/// it) but whose Close command may not have been processed yet.  Prevents a
/// second gtk_widget_destroy when a host-initiated Close is handled first.
static GTK_DESTROYED: Mutex<Vec<u64>> = Mutex::new(Vec::new());

fn take_gtk_destroyed(instance_id: u64) -> bool {
    match GTK_DESTROYED.lock() {
        Ok(mut v) => match v.iter().position(|&id| id == instance_id) {
            Some(pos) => {
                v.swap_remove(pos);
                true
            }
            None => false,
        },
        Err(_) => false,
    }
}

fn closing_flags() -> &'static Arc<Mutex<HashMap<u64, Arc<AtomicBool>>>> {
    CLOSING_FLAGS.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

fn open_ui_set() -> &'static Arc<Mutex<std::collections::HashSet<u64>>> {
    OPEN_UI_SET.get_or_init(|| Arc::new(Mutex::new(std::collections::HashSet::new())))
}

pub fn is_ui_open(instance_id: u64) -> bool {
    if open_ui_set().lock().unwrap().contains(&instance_id) {
        return true;
    }
    match UI_BRIDGE.lock() {
        Ok(guard) => guard.as_ref().is_some_and(|b| b.is_open(instance_id)),
        Err(_) => false,
    }
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

fn send_gtk_command(cmd: GtkCommand) -> bool {
    match GTK_CMD_TX.get() {
        Some(tx) => match tx.lock() {
            Ok(tx) => tx.send(cmd).is_ok(),
            Err(_) => false,
        },
        None => false,
    }
}

fn on_gtk_thread() -> bool {
    GTK_THREAD_ID
        .get()
        .is_some_and(|id| *id == std::thread::current().id())
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
                ack,
            } => {
                handle_close_window(state, instance_id, destroyed_by_gtk);
                if let Some(ack) = ack {
                    let _ = ack.send(());
                }
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
            flag.store(true, Ordering::Release);
        }
        send_gtk_command(GtkCommand::Close {
            instance_id,
            destroyed_by_gtk: false,
            ack: None,
        });
    }
    0
}

unsafe extern "C" fn on_window_destroy_multi(_widget: *mut c_void, data: *mut c_void) {
    if data.is_null() {
        return;
    }
    let instance_id = unsafe { *(data as *const u64) };
    log::info!("Plugin UI window destroyed for instance {}", instance_id);

    if let Ok(mut v) = GTK_DESTROYED.lock() {
        v.push(instance_id);
    }

    if let Ok(flags) = closing_flags().lock()
        && let Some(flag) = flags.get(&instance_id)
    {
        flag.store(true, Ordering::Release);
    }

    send_gtk_command(GtkCommand::Close {
        instance_id,
        destroyed_by_gtk: true,
        ack: None,
    });
}

unsafe extern "C" fn destroy_instance_id_data(data: *mut c_void, _closure: *mut c_void) {
    if !data.is_null() {
        let _ = unsafe { Box::from_raw(data as *mut u64) };
    }
}

fn gtk_thread_main(cmd_rx: std::sync::mpsc::Receiver<GtkCommand>) {
    let _ = GTK_THREAD_ID.set(std::thread::current().id());
    // GDK_BACKEND=x11 is set in main() before any thread is spawned.
    unsafe {
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

    // Dropping the state drops the command receiver: later senders (and
    // close_plugin_ui waiters) fail fast instead of hanging.
    let _ = unsafe { Box::from_raw(state) };
}

// ── SIGSEGV guard around UI instantiation ──
//
// Some UI toolkits (DPF/Pugl without GLX, ...) crash inside
// suil_instance_new.  We catch SIGSEGV on the GTK thread only for the
// duration of that call and siglongjmp back.  Faults on any other thread
// (e.g. the RT thread) must never be redirected onto the GTK thread's jump
// buffer: the handler restores the previous disposition and returns, so the
// faulting instruction re-executes and crashes (or is handled) normally.

/// Large enough for glibc's sigjmp_buf on x86_64 (200 B) and aarch64 (312 B).
#[repr(C, align(16))]
struct SigJmpBuf([u8; 512]);

static CRASH_ARMED_THREAD: AtomicUsize = AtomicUsize::new(0);
static CRASH_CAUGHT: AtomicBool = AtomicBool::new(false);
static mut CRASH_SAVED_HANDLER: libc::sigaction = unsafe { std::mem::zeroed() };
static mut CRASH_JUMP_BUF: SigJmpBuf = SigJmpBuf([0u8; 512]);

unsafe extern "C" {
    #[link_name = "__sigsetjmp"]
    fn sigsetjmp(env: *mut SigJmpBuf, savemask: c_int) -> c_int;
    fn siglongjmp(env: *mut SigJmpBuf, val: c_int) -> !;
}

unsafe extern "C" fn crash_handler(_sig: c_int) {
    let me = unsafe { libc::pthread_self() } as usize;
    if me != 0 && CRASH_ARMED_THREAD.load(Ordering::SeqCst) == me {
        CRASH_ARMED_THREAD.store(0, Ordering::SeqCst);
        CRASH_CAUGHT.store(true, Ordering::SeqCst);
        unsafe { siglongjmp(&raw mut CRASH_JUMP_BUF, 1) };
    }
    // Not the armed thread: hand the fault back to the previous handler.
    unsafe {
        libc::sigaction(libc::SIGSEGV, &raw const CRASH_SAVED_HANDLER, ptr::null_mut());
    }
}

/// Arm the guard for the calling thread.  Must be paired with `disarm_crash_guard`.
unsafe fn arm_crash_guard() {
    CRASH_CAUGHT.store(false, Ordering::SeqCst);
    CRASH_ARMED_THREAD.store(unsafe { libc::pthread_self() } as usize, Ordering::SeqCst);
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = crash_handler as *const () as usize;
        sa.sa_flags = libc::SA_NODEFER;
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(libc::SIGSEGV, &sa, &raw mut CRASH_SAVED_HANDLER);
    }
}

unsafe fn disarm_crash_guard() {
    CRASH_ARMED_THREAD.store(0, Ordering::SeqCst);
    unsafe {
        libc::sigaction(libc::SIGSEGV, &raw const CRASH_SAVED_HANDLER, ptr::null_mut());
    }
}

fn send_ui_error(req: &OpenUiRequest, message: String) {
    let _ = req.event_tx.send(PwEvent::Plugin(PluginEvent::PluginError {
        instance_id: Some(req.instance_id),
        message,
        fatal: false,
    }));
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

    take_gtk_destroyed(instance_id);

    // The GTK thread's own cached world.
    let (world, plugin) = match super::find_in_thread_world(&req.plugin_uri) {
        Some(found) => found,
        None => {
            log::error!("Plugin not found: {}", req.plugin_uri);
            send_ui_error(&req, format!("Plugin not found: {}", req.plugin_uri));
            return;
        }
    };

    let plugin_name = plugin
        .name()
        .as_str()
        .map(String::from)
        .unwrap_or_else(|| req.plugin_uri.clone());

    let mut symbol_to_index: Vec<(String, usize)> = Vec::new();
    for i in 0..plugin.ports_count() {
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
            send_ui_error(
                &req,
                format!("No supported UI found for plugin: {}", req.plugin_uri),
            );
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

    let cstrings = (
        CString::new(HOST_TYPE_URI),
        CString::new(req.plugin_uri.as_str()),
        CString::new(ui_uri),
        CString::new(ui_type_uri),
        CString::new(bundle_path),
        CString::new(binary_path),
    );
    let (host_type, c_plugin_uri, c_ui_uri, c_ui_type_uri, c_bundle_path, c_binary_path) =
        match cstrings {
            (Ok(a), Ok(b), Ok(c), Ok(d), Ok(e), Ok(f)) => (a, b, c, d, e, f),
            _ => {
                log::error!("CString error while opening UI for {}", req.plugin_uri);
                return;
            }
        };

    let has_atom_inputs = !req.port_updates.atom_inputs.is_empty();
    let controller = Box::new(UiController {
        instance_id,
        cmd_tx: req.cmd_tx.clone(),
        symbol_to_index,
        port_updates: if has_atom_inputs {
            Some(req.port_updates.clone())
        } else {
            None
        },
    });
    let controller_ptr = Box::into_raw(controller) as *mut c_void;
    let free_controller = || unsafe {
        drop(Box::from_raw(controller_ptr as *mut UiController));
    };

    let features = UiHostFeatures::new(
        &req.urid_mapper,
        req.extension_data_fn,
        req.lv2_handle,
        req.sample_rate,
    );

    unsafe {
        let window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
        if window.is_null() {
            free_controller();
            log::error!("Failed to create GTK window for instance {}", instance_id);
            return;
        }

        let title = CString::new(format!("ZestBay — {}", plugin_name))
            .unwrap_or_else(|_| c"ZestBay — Plugin UI".to_owned());
        gtk_window_set_title(window, title.as_ptr());
        gtk_window_set_default_size(window, -1, -1);

        let host = suil_host_new(
            Some(port_write_callback),
            Some(port_index_callback),
            None,
            None,
        );
        if host.is_null() {
            gtk_widget_destroy(window);
            free_controller();
            log::error!("Failed to create suil host for instance {}", instance_id);
            return;
        }

        // Catch DPF/Pugl crashes during suil_instance_new (GTK thread only).
        arm_crash_guard();
        let instance = if sigsetjmp(&raw mut CRASH_JUMP_BUF, 1) == 0 {
            suil_instance_new(
                host,
                controller_ptr,
                host_type.as_ptr(),
                c_plugin_uri.as_ptr(),
                c_ui_uri.as_ptr(),
                c_ui_type_uri.as_ptr(),
                c_bundle_path.as_ptr(),
                c_binary_path.as_ptr(),
                features.as_ptr(),
            )
        } else {
            // Returned here from siglongjmp after a crash
            ptr::null_mut()
        };
        disarm_crash_guard();

        if CRASH_CAUGHT.load(Ordering::SeqCst) {
            log::error!(
                "Plugin UI crashed during instantiation (SIGSEGV caught) for instance {}. \
                 This plugin's UI uses a framework (e.g. DPF/Pugl) that is not yet fully supported.",
                instance_id
            );
            suil_host_free(host);
            gtk_widget_destroy(window);
            free_controller();
            send_ui_error(
                &req,
                "Plugin UI crashed during instantiation. This plugin's UI framework (DPF/Pugl) is not yet fully supported.".into(),
            );
            return;
        }

        if instance.is_null() {
            suil_host_free(host);
            gtk_widget_destroy(window);
            free_controller();
            log::error!(
                "Failed to create suil instance for instance {}",
                instance_id
            );
            send_ui_error(&req, "Failed to create suil instance".into());
            return;
        }

        let widget = suil_instance_get_widget(instance);
        if widget.is_null() {
            suil_instance_free(instance);
            suil_host_free(host);
            gtk_widget_destroy(window);
            free_controller();
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

        let closing_flag = Arc::new(AtomicBool::new(false));
        if let Ok(mut flags) = closing_flags().lock() {
            flags.insert(instance_id, closing_flag.clone());
        }

        // Start receiving plugin → UI atom output.
        for atom_out in req.port_updates.atom_outputs.iter() {
            atom_out.set_listening(true);
        }

        let n_control_outputs = req.port_updates.control_outputs.len();
        let n_control_inputs = req.port_updates.control_inputs.len();
        let timer_data = Box::into_raw(Box::new(UiTimerData {
            suil_instance: instance,
            port_updates: req.port_updates.clone(),
            atom_event_transfer_urid,
            idle_iface,
            ui_handle,
            closing: closing_flag,
            instance_id,
            timer_removed: AtomicBool::new(false),
            // Initialize with NaN so the first tick always sends all values
            prev_control_outputs: vec![f32::NAN; n_control_outputs],
            prev_control_inputs: vec![f32::NAN; n_control_inputs],
        }));
        let timer_source_id = g_timeout_add(33, Some(ui_timer_callback), timer_data as *mut c_void);

        state.windows.insert(
            instance_id,
            WindowState {
                gtk_window: window,
                suil_instance: instance,
                suil_host: host,
                controller_ptr,
                timer_data,
                timer_source_id,
                _features: features,
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
    let destroyed_by_gtk = take_gtk_destroyed(instance_id) || destroyed_by_gtk;
    let ws = match state.windows.remove(&instance_id) {
        Some(ws) => ws,
        None => return,
    };

    unsafe {
        (*ws.timer_data).closing.store(true, Ordering::Release);

        // Only remove the timer source if it hasn't already been auto-removed
        // by GLib (when the callback returned 0).
        if !(*ws.timer_data).timer_removed.load(Ordering::Acquire) {
            g_source_remove(ws.timer_source_id);
        }

        if !destroyed_by_gtk {
            gtk_widget_destroy(ws.gtk_window);
        }

        (*ws.timer_data).suil_instance = ptr::null_mut();

        suil_instance_free(ws.suil_instance);
        suil_host_free(ws.suil_host);

        let timer_data = Box::from_raw(ws.timer_data);
        for atom_out in timer_data.port_updates.atom_outputs.iter() {
            atom_out.set_listening(false);
        }
        drop(timer_data);
        drop(Box::from_raw(ws.controller_ptr as *mut UiController));
    }
    // The UI is gone; its feature data can go too.
    drop(ws._features);

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

// ── Out-of-process UI bridge ──

/// UI bridge client — spawned on first use, respawned if the child died,
/// shut down by `shutdown_gtk_thread`.
static UI_BRIDGE: Mutex<Option<super::ui_bridge_client::UiBridgeClient>> = Mutex::new(None);
static UI_BRIDGE_SHUT_DOWN: AtomicBool = AtomicBool::new(false);

/// Run `f` with a live bridge client, spawning (or respawning) it if needed.
fn with_bridge<R>(
    event_tx: &Sender<PwEvent>,
    cmd_tx: &Sender<PwCommand>,
    f: impl FnOnce(&mut super::ui_bridge_client::UiBridgeClient) -> R,
) -> Option<R> {
    if UI_BRIDGE_SHUT_DOWN.load(Ordering::Acquire) {
        return None;
    }
    let mut guard = UI_BRIDGE.lock().ok()?;
    if let Some(client) = guard.as_mut()
        && !client.is_alive()
    {
        log::warn!("UI bridge process is no longer running — respawning on demand");
        if let Some(mut dead) = guard.take() {
            dead.shutdown();
        }
    }
    if guard.is_none() {
        match super::ui_bridge_client::UiBridgeClient::spawn(event_tx.clone(), cmd_tx.clone()) {
            Ok(client) => {
                log::info!("UI bridge process spawned");
                *guard = Some(client);
            }
            Err(e) => {
                log::error!("Failed to spawn UI bridge: {}", e);
                return None;
            }
        }
    }
    guard.as_mut().map(f)
}

fn shutdown_bridge() {
    UI_BRIDGE_SHUT_DOWN.store(true, Ordering::Release);
    let client = UI_BRIDGE.lock().ok().and_then(|mut g| g.take());
    if let Some(mut client) = client {
        client.shutdown();
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
    lv2_handle: *mut c_void,
    extension_data_fn: Option<unsafe extern "C" fn(*const c_char) -> *const c_void>,
) {
    // The shared slots hold the live values; prefer them over the caller's
    // snapshot.
    let mut control_values = control_values;
    for slot in port_updates
        .control_inputs
        .iter()
        .chain(port_updates.control_outputs.iter())
    {
        let value = slot.value.load();
        match control_values.iter_mut().find(|(idx, _)| *idx == slot.port_index) {
            Some(entry) => entry.1 = value,
            None => control_values.push((slot.port_index, value)),
        }
    }

    let sample_rate = super::host::sample_rate_for_handle(lv2_handle).unwrap_or_else(|| {
        log::warn!("LV2 UI: unknown sample rate for instance {}, assuming 48000", instance_id);
        48000.0
    }) as f32;

    // Try to find the UI info to determine if it's X11
    let world = super::thread_world();
    let uri_node = world.new_uri(plugin_uri);
    let plugin = world.plugins().iter().find(|p| p.uri().as_uri() == uri_node.as_uri());

    let mut ui_info: Option<(String, String, String, String)> = None;
    let mut display_name = plugin_uri.rsplit('/').next().unwrap_or(plugin_uri).to_string();
    if let Some(plugin) = plugin {
        if let Some(name) = plugin.name().as_str() {
            display_name = name.to_string();
        }
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
            let opened = with_bridge(&event_tx, &cmd_tx, |client| {
                log::info!("Using UI bridge for X11 plugin: {}", plugin_uri);
                client.open_ui(
                    instance_id,
                    plugin_uri,
                    ui_uri,
                    ui_type_uri,
                    bundle_path,
                    binary_path,
                    &display_name,
                    control_values.clone(),
                    &urid_mapper,
                    sample_rate,
                    port_updates.clone(),
                )
            });
            if opened == Some(true) {
                return;
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
        sample_rate,
    }));
}

/// Result of [`close_plugin_ui`].
pub enum UiCloseStatus {
    /// No in-process UI can touch the plugin instance any more.
    Closed,
    /// The GTK thread did not confirm in time: an in-process UI may still
    /// use the instance (instance-access / data-access) until the close is
    /// acknowledged, so the instance must be kept alive until then.
    Pending(PendingUiClose),
}

/// A close the GTK thread has not acknowledged yet.
pub struct PendingUiClose(std::sync::mpsc::Receiver<()>);

impl PendingUiClose {
    /// True once the GTK thread destroyed the UI (or is gone).
    pub fn is_done(&self) -> bool {
        !matches!(self.0.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty))
    }
}

/// Close the plugin UI for `instance_id` (in-process or bridge).
///
/// For in-process UIs this blocks (up to ~2 s) until the GTK thread has
/// destroyed the UI, so the caller may free the plugin instance afterwards.
/// If the GTK thread does not confirm in time, `Pending` is returned and the
/// instance must not be freed yet.  It does not wait when called on the GTK
/// thread itself or when the GTK thread is not running.
pub fn close_plugin_ui(instance_id: u64) -> UiCloseStatus {
    // Bridge UIs live in another process and never touch the instance's
    // memory; closing them is fire-and-forget.
    if let Ok(mut guard) = UI_BRIDGE.lock()
        && let Some(client) = guard.as_mut()
    {
        client.close_ui(instance_id);
    }

    if GTK_CMD_TX.get().is_none() {
        return UiCloseStatus::Closed;
    }

    if on_gtk_thread() {
        send_gtk_command(GtkCommand::Close {
            instance_id,
            destroyed_by_gtk: false,
            ack: None,
        });
        return UiCloseStatus::Closed;
    }

    let (ack_tx, ack_rx) = std::sync::mpsc::channel::<()>();
    if !send_gtk_command(GtkCommand::Close {
        instance_id,
        destroyed_by_gtk: false,
        ack: Some(ack_tx),
    }) {
        return UiCloseStatus::Closed; // GTK thread gone
    }
    match ack_rx.recv_timeout(CLOSE_TIMEOUT) {
        // Ok: closed.  Disconnected: the command was dropped (thread exiting).
        Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => UiCloseStatus::Closed,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            log::warn!(
                "Timed out waiting for the GTK thread to close the UI of instance {}",
                instance_id
            );
            UiCloseStatus::Pending(PendingUiClose(ack_rx))
        }
    }
}

/// Close all plugin UIs, stop the GTK thread and shut down the UI bridge
/// process.  Called on application quit.
pub fn shutdown_gtk_thread() {
    send_gtk_command(GtkCommand::Shutdown);
    shutdown_bridge();
}
