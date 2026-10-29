mod clap;
mod layout;
mod lv2;
mod midi;
mod patchbay;
mod pipewire;
mod plugin;
mod tray;
mod ui;
pub mod ui_bridge;
mod vst3;

use cxx_qt::casting::Upcast;
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQmlEngine, QString, QUrl};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};

/// Global flag: when true, skip restoring saved plugins on startup.
pub static SAFE_MODE: AtomicBool = AtomicBool::new(false);

/// Global flag: when true, `persist_active_plugins()` becomes a no-op.
/// Set during safe mode to prevent writing an empty plugin list over the
/// user's saved plugins.json.
pub static PLUGINS_FROZEN: AtomicBool = AtomicBool::new(false);

/// Global flag: when true, skip the sandbox probe before plugin instantiation.
/// Dangerous — a crashing plugin will take down the entire process.
pub static NO_PROBE: AtomicBool = AtomicBool::new(false);

/// End the process once everything is saved, without running atexit handlers
/// or the static destructors of loaded libraries. Hosted plugins (Pianoteq,
/// for one) can hit internal errors and hang in their own destructors while
/// PipeWire and Qt threads are still running, which froze ZestBay on quit.
pub fn exit_now(code: i32) -> ! {
    use std::io::Write;
    log::logger().flush();
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    unsafe { libc::_exit(code) }
}

#[link(name = "X11")]
unsafe extern "C" {
    fn XInitThreads() -> std::os::raw::c_int;
}

fn main() {
    // Plugin UIs (LV2/CLAP/VST3 X11 editors) call Xlib from several threads;
    // Xlib must be put into thread-safe mode before any other Xlib call and
    // before any thread is spawned.
    if unsafe { XInitThreads() } == 0 {
        eprintln!("XInitThreads failed; X11 plugin UIs may be unstable");
    }
    // In-process LV2 GTK UIs are X11-embedded.  Set before any thread is
    // spawned: setenv races with getenv on other threads.
    unsafe { std::env::set_var("GDK_BACKEND", "x11") };
    // Every control is custom-drawn from the Theme tokens; the Basic style
    // draws nothing of its own, so the UI looks the same on every desktop.
    unsafe { std::env::set_var("QT_QUICK_CONTROLS_STYLE", "Basic") };
    // lilv only searches its compiled-in LV2 path; in a Flatpak point it at
    // the user, extension and host plugin directories.
    if let Some(lv2_path) = plugin::paths::flatpak_lv2_path() {
        unsafe { std::env::set_var("LV2_PATH", lv2_path) };
    }

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = std::env::args().collect();

    // Handle --probe-plugin subcommand (used by exec_probe for clean-process probing)
    if let Some(pos) = args.iter().position(|a| a == "--probe-plugin") {
        let probe_args: Vec<String> = args[pos + 1..].to_vec();
        plugin::sandbox::run_probe_main(&probe_args);
        // run_probe_main never returns
    }

    if args.iter().any(|a| a == "--safe-mode") {
        log::warn!("Safe mode enabled via --safe-mode flag: skipping plugin restoration");
        SAFE_MODE.store(true, Ordering::SeqCst);
    }

    if args.iter().any(|a| a == "--no-probe") {
        log::warn!("Plugin probing disabled via --no-probe flag: crashing plugins will take down ZestBay");
        NO_PROBE.store(true, Ordering::SeqCst);
    }

    log::info!("Starting ZestBay");

    let mut app = QGuiApplication::new();

    // Must match the installed .desktop file (Wayland app id, taskbar icon).
    let desktop_file_name =
        plugin::paths::flatpak_app_id().unwrap_or_else(|| "zestbay".to_string());
    QGuiApplication::set_desktop_file_name(&QString::from(&desktop_file_name));

    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine
            .on_object_created(|_, obj, url| {
                if obj.is_null() {
                    log::error!("QML object creation FAILED for: {:?}", url.to_string());
                } else {
                    log::info!("QML object created successfully for: {:?}", url.to_string());
                }
            })
            .release();
    }

    if let Some(engine) = engine.as_mut() {
        engine
            .on_object_creation_failed(|_, url| {
                log::error!("QML creation failed signal for: {:?}", url.to_string());
            })
            .release();
    }

    let url = QUrl::from("qrc:/qt/qml/ZestBay/qml/main.qml");
    log::info!("Loading QML from: {:?}", url.to_string());
    if let Some(engine) = engine.as_mut() {
        engine.load(&url);
    }

    if let Some(engine) = engine.as_mut() {
        let engine: Pin<&mut QQmlEngine> = engine.upcast_pin();
        engine
            .on_quit(|_| {
                log::info!("QML engine quit signal received");
            })
            .release();
    }

    log::info!("Starting Qt event loop");

    if let Some(app) = app.as_mut() {
        app.exec();
    }

    log::info!("Qt event loop exited");

    lv2::ui::shutdown_gtk_thread();
    exit_now(0);
}
