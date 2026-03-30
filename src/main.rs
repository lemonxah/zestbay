mod clap;
mod layout;
mod lv2;
mod midi;
mod patchbay;
mod pipewire;
mod plugin;
mod tray;
mod ui;
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

fn main() {
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

    QGuiApplication::set_desktop_file_name(&QString::from("zestbay"));

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
}
