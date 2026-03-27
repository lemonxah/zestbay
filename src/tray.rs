use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use ksni::blocking::TrayMethods;

#[derive(Clone)]
pub struct PluginEntry {
    pub name: String,
    pub node_id: u32,
    pub has_ui: bool,
}

#[derive(Clone)]
pub struct TrayState {
    pub show_requested: Arc<AtomicBool>,
    pub hide_requested: Arc<AtomicBool>,
    pub quit_requested: Arc<AtomicBool>,
    pub window_visible: Arc<AtomicBool>,
    pub plugins: Arc<Mutex<Vec<PluginEntry>>>,
    pub open_plugin_ui: Arc<Mutex<Option<u32>>>,
}

impl TrayState {
    pub fn new() -> Self {
        Self {
            show_requested: Arc::new(AtomicBool::new(false)),
            hide_requested: Arc::new(AtomicBool::new(false)),
            quit_requested: Arc::new(AtomicBool::new(false)),
            window_visible: Arc::new(AtomicBool::new(true)),
            plugins: Arc::new(Mutex::new(Vec::new())),
            open_plugin_ui: Arc::new(Mutex::new(None)),
        }
    }
}

struct ZestBayTray {
    state: TrayState,
    icon_theme_path: String,
}

impl ksni::Tray for ZestBayTray {
    fn id(&self) -> String {
        "zestbay".into()
    }

    fn icon_theme_path(&self) -> String {
        self.icon_theme_path.clone()
    }

    fn icon_name(&self) -> String {
        "zestbay-tray".into()
    }

    fn title(&self) -> String {
        "ZestBay".into()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::ApplicationStatus
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let currently_visible = self.state.window_visible.load(Ordering::Acquire);
        log::info!("Tray: activate (left-click), currently_visible={currently_visible}");
        if currently_visible {
            self.state.window_visible.store(false, Ordering::Release);
            self.state.hide_requested.store(true, Ordering::Release);
        } else {
            self.state.window_visible.store(true, Ordering::Release);
            self.state.show_requested.store(true, Ordering::Release);
        }
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::*;

        let mut items: Vec<ksni::MenuItem<Self>> = vec![StandardItem {
            label: "Show".into(),
            icon_name: "window-new".into(),
            activate: Box::new(|tray: &mut Self| {
                log::info!("Tray: Show menu item clicked");
                tray.state.window_visible.store(true, Ordering::Release);
                tray.state.show_requested.store(true, Ordering::Release);
            }),
            ..Default::default()
        }
        .into()];

        let plugins = self
            .state
            .plugins
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if !plugins.is_empty() {
            let mut plugin_items: Vec<ksni::MenuItem<Self>> = Vec::new();
            for plugin in &plugins {
                let node_id = plugin.node_id;
                let has_ui = plugin.has_ui;
                plugin_items.push(
                    StandardItem {
                        label: plugin.name.clone(),
                        enabled: has_ui,
                        activate: Box::new(move |tray: &mut ZestBayTray| {
                            log::info!("Tray: open plugin UI for node_id={}", node_id);
                            if let Ok(mut req) = tray.state.open_plugin_ui.lock() {
                                *req = Some(node_id);
                            }
                        }),
                        ..Default::default()
                    }
                    .into(),
                );
            }
            items.push(ksni::MenuItem::Separator);
            items.push(
                SubMenu {
                    label: "Plugin UIs".into(),
                    icon_name: "preferences-system".into(),
                    submenu: plugin_items,
                    ..Default::default()
                }
                .into(),
            );
        }

        items.push(ksni::MenuItem::Separator);
        items.push(
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
        );

        items
    }
}

pub fn spawn_tray() -> TrayState {
    let state = TrayState::new();
    let tray_state = state.clone();

    let icon_theme_path = {
        let mut path = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("icons")))
            .unwrap_or_default();
        if !path.exists() {
            path = std::path::PathBuf::from("icons");
        }
        if path.exists() {
            path.to_string_lossy().into_owned()
        } else {
            String::new()
        }
    };

    std::thread::Builder::new()
        .name("zestbay-tray".into())
        .spawn(move || {
            let tray = ZestBayTray {
                state: tray_state,
                icon_theme_path,
            };
            match tray.spawn() {
                Ok(_handle) => loop {
                    std::thread::park();
                },
                Err(e) => {
                    log::warn!("Failed to create system tray icon: {}", e);
                    log::warn!("The application will still run but won't have a tray icon.");
                }
            }
        })
        .expect("Failed to spawn tray thread");

    state
}
