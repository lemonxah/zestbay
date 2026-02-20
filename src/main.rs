//! ZestBay - A PipeWire patchbay application
//!
//! A visual audio routing manager for PipeWire, inspired by qpwgraph.

// Allow dead code during development - skeleton APIs will be used later
#![allow(dead_code)]

mod lv2;
mod patchbay;
mod pipewire;
mod tray;
mod ui;

fn main() -> eframe::Result<()> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Starting ZestBay");

    // Force the eframe/winit window to use X11 (XWayland) instead of native
    // Wayland.  This is necessary because:
    //   - _NET_WM_STATE_SKIP_TASKBAR only works on X11
    //   - close_requested() and Visible(true/false) viewport commands are
    //     unreliable on native Wayland (needed for tray show/hide/close)
    // The system tray (ksni) communicates via DBus, so it is unaffected.
    // GTK plugin UIs already force X11 via GDK_BACKEND=x11.
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        log::info!(
            "Unsetting WAYLAND_DISPLAY to force X11/XWayland backend for the main window"
        );
        // SAFETY: This is called at the very start of main(), before any
        // other threads are spawned, so no concurrent access to the env.
        unsafe { std::env::remove_var("WAYLAND_DISPLAY") };
    }

    // Spawn the system tray icon (KDE StatusNotifierItem)
    let tray_state = tray::spawn_tray();

    // Create shared graph state
    let graph = pipewire::GraphState::new();

    // Initialize LV2 plugin manager (scans installed plugins)
    let lv2_manager = lv2::Lv2Manager::new();

    // Start PipeWire manager thread
    let (event_rx, cmd_tx) = pipewire::start(graph.clone());

    // Run the UI — with X11 forced, with_taskbar(false) and our Xlib FFI
    // will correctly set _NET_WM_STATE_SKIP_TASKBAR.
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ZestBay")
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_taskbar(false)
            .with_app_id("zestbay"),
        ..Default::default()
    };

    let result = eframe::run_native(
        "ZestBay",
        options,
        Box::new(move |cc| {
            let app = ui::ZestBayApp::new(cc, graph, event_rx, cmd_tx, lv2_manager, tray_state);
            Ok(Box::new(app) as Box<dyn eframe::App>)
        }),
    );

    // Shut down the persistent GTK thread (if it was started)
    lv2::ui::shutdown_gtk_thread();

    result
}
