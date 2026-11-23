mod clap;
mod color_scheme;
mod layout;
mod lv2;
mod midi;
mod osc;
mod patchbay;
mod pipewire;
mod plugin;
mod presets;
mod session;
mod subgraph;
mod theme_css;
mod tray;
mod ui;
pub mod ui_bridge;
mod vst2;
mod ladspa;
mod vst3;

use cxx_qt::casting::Upcast;
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQmlEngine, QString, QUrl};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};

pub static SAFE_MODE: AtomicBool = AtomicBool::new(false);

pub static PLUGINS_FROZEN: AtomicBool = AtomicBool::new(false);

pub static NO_PROBE: AtomicBool = AtomicBool::new(false);

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
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--scan-plugin") {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
        plugin::sandbox::run_scan_main(&args[2..]);
    }
    if unsafe { XInitThreads() } == 0 {
        eprintln!("XInitThreads failed; X11 plugin UIs may be unstable");
    }
    unsafe { std::env::set_var("GDK_BACKEND", "x11") };
    unsafe { std::env::set_var("QT_QUICK_CONTROLS_STYLE", "Basic") };

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    color_scheme::start();

    let args: Vec<String> = std::env::args().collect();

    if let Some(pos) = args.iter().position(|a| a == "--probe-plugin") {
        let probe_args: Vec<String> = args[pos + 1..].to_vec();
        plugin::sandbox::run_probe_main(&probe_args);
    }

    match session::parse_args(&args[1..]).and_then(session::init) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("zestbay: {e}");
            eprintln!("Usage: zestbay [--session <name> | --session_path <folder>]");
            std::process::exit(2);
        }
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
    exit_now(0);
}
