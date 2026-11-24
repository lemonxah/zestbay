use super::*;

impl qobject::AppController {
    pub fn set_window_visible(self: Pin<&mut Self>, visible: bool) {
        if let Some(ref tray) = self.rust().tray_state {
            use std::sync::atomic::Ordering;
            tray.window_visible.store(visible, Ordering::Release);
            log::info!("Window visible state updated to {}", visible);
            tray.refresh();
        }
    }

    pub(super) fn sync_tray_flags(self: Pin<&mut Self>) {
        if let Some(ref tray) = self.rust().tray_state {
            use std::sync::atomic::Ordering;
            tray.rules_enabled
                .store(self.rust().patchbay_enabled, Ordering::Release);
            tray.exclusive
                .store(self.rust().patchbay_exclusive, Ordering::Release);
            tray.refresh();
        }
    }

    pub(super) fn sync_tray_plugins(self: Pin<&mut Self>) {
        let tray = match self.rust().tray_state.as_ref() {
            Some(t) => t,
            None => return,
        };
        let mgr = match self.rust().plugin_manager.as_ref() {
            Some(m) => m,
            None => return,
        };

        let mut entries: Vec<crate::tray::PluginEntry> = mgr
            .active_instances()
            .values()
            .filter(|info| info.pw_node_id.is_some())
            .map(|info| {
                let has_ui = mgr
                    .find_plugin_with_format(&info.plugin_uri, info.format)
                    .or(info.scan_info.as_ref())
                    .map(|p| p.has_ui)
                    .unwrap_or(false);
                crate::tray::PluginEntry {
                    name: info.display_name.clone(),
                    node_id: info.pw_node_id.unwrap_or(0),
                    has_ui,
                }
            })
            .collect();
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        let changed = match tray.plugins.lock() {
            Ok(mut plugins) if *plugins != entries => {
                *plugins = entries;
                true
            }
            _ => false,
        };
        if changed {
            tray.refresh();
        }
    }
}
