//! System tray icon using KDE StatusNotifierItem protocol.
//!
//! Provides a tray icon with "Show" and "Quit" context menu items.
//! Left-clicking the tray icon toggles window visibility.
//! The tray runs on its own background thread via ksni's blocking API.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ksni::blocking::TrayMethods;

/// Shared state between the tray icon and the eframe application.
#[derive(Clone)]
pub struct TrayState {
    /// Set to true by the tray when the window should be shown.
    pub show_requested: Arc<AtomicBool>,
    /// Set to true by the tray when the application should quit.
    pub quit_requested: Arc<AtomicBool>,
    /// Current visibility state — the app sets this so the tray
    /// knows whether "activate" should show or hide.
    pub window_visible: Arc<AtomicBool>,
}

impl TrayState {
    pub fn new() -> Self {
        Self {
            show_requested: Arc::new(AtomicBool::new(false)),
            quit_requested: Arc::new(AtomicBool::new(false)),
            window_visible: Arc::new(AtomicBool::new(true)),
        }
    }
}

/// The ksni Tray implementation for ZestBay.
struct ZestBayTray {
    state: TrayState,
}

impl ksni::Tray for ZestBayTray {
    fn id(&self) -> String {
        "zestbay".into()
    }

    fn icon_name(&self) -> String {
        // Use a standard audio icon from the system icon theme.
        // KDE/freedesktop icon themes provide this.
        "audio-card".into()
    }

    fn title(&self) -> String {
        "ZestBay".into()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::ApplicationStatus
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        // Left-click: toggle window visibility
        let currently_visible = self.state.window_visible.load(Ordering::Acquire);
        log::info!("Tray: activate (left-click), currently_visible={currently_visible}");
        if currently_visible {
            self.state.window_visible.store(false, Ordering::Release);
            self.state.show_requested.store(false, Ordering::Release);
        } else {
            self.state.window_visible.store(true, Ordering::Release);
            self.state.show_requested.store(true, Ordering::Release);
        }
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::*;
        vec![
            StandardItem {
                label: "Show".into(),
                icon_name: "window-new".into(),
                activate: Box::new(|tray: &mut Self| {
                    log::info!("Tray: Show menu item clicked");
                    tray.state.window_visible.store(true, Ordering::Release);
                    tray.state.show_requested.store(true, Ordering::Release);
                }),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|tray: &mut Self| {
                    log::info!("Tray: Quit menu item clicked");
                    tray.state.quit_requested.store(true, Ordering::Release);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Spawn the system tray icon on a background thread.
///
/// Returns the shared `TrayState` for communicating with the tray.
/// The tray runs until the application exits.
pub fn spawn_tray() -> TrayState {
    let state = TrayState::new();
    let tray_state = state.clone();

    std::thread::Builder::new()
        .name("zestbay-tray".into())
        .spawn(move || {
            let tray = ZestBayTray { state: tray_state };
            match tray.spawn() {
                Ok(_handle) => {
                    // The handle keeps the tray alive. Park this thread
                    // indefinitely — the tray's DBus service runs in the
                    // background via ksni's internal async runtime.
                    loop {
                        std::thread::park();
                    }
                }
                Err(e) => {
                    log::warn!("Failed to create system tray icon: {}", e);
                    log::warn!("The application will still run but won't have a tray icon.");
                }
            }
        })
        .expect("Failed to spawn tray thread");

    state
}
