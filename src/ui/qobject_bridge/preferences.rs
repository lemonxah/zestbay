use super::*;

impl qobject::AppController {
    pub(super) fn poll_custom_themes(mut self: Pin<&mut Self>) {
        if self
            .rust()
            .themes_checked_at
            .is_some_and(|t| t.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        self.as_mut().rust_mut().themes_checked_at = Some(Instant::now());
        let stamp: Vec<(String, Option<std::time::SystemTime>, u64)> =
            crate::theme_css::list_files()
                .into_iter()
                .map(|(name, _, modified, len)| (name, modified, len))
                .collect();
        if stamp != self.rust().themes_stamp {
            self.as_mut().rust_mut().themes_stamp = stamp;
            let rev = self.rust().custom_themes_rev.wrapping_add(1);
            self.as_mut().set_custom_themes_rev(rev);
        }
    }

    pub fn get_custom_themes_json(self: Pin<&mut Self>) -> QString {
        let themes: Vec<serde_json::Value> = crate::theme_css::load_all()
            .into_iter()
            .map(|(name, theme)| {
                serde_json::json!({
                    "id": format!("custom:{name}"),
                    "name": name,
                    "dark": theme.dark,
                    "light": theme.light,
                })
            })
            .collect();
        QString::from(&serde_json::Value::Array(themes).to_string())
    }

    pub fn themes_folder(self: Pin<&mut Self>) -> QString {
        let dir = crate::theme_css::themes_dir();
        if let Err(e) = std::fs::create_dir_all(&dir) {
            log::warn!("Could not create {}: {}", dir.display(), e);
        }
        QString::from(&dir.to_string_lossy().to_string())
    }
}

impl qobject::AppController {
    pub fn get_preferences_json(self: Pin<&mut Self>) -> QString {
        let json = serde_json::to_string(&self.rust().prefs).unwrap_or_default();
        QString::from(&json)
    }

    pub fn set_preference(mut self: Pin<&mut Self>, key: QString, value: QString) {
        let key_str: String = key.to_string();
        let val_str: String = value.to_string();

        match key_str.as_str() {
            "rule_settle_ms" => {
                if let Ok(v) = val_str.parse::<u64>() {
                    self.as_mut().rust_mut().prefs.rule_settle_ms = v.clamp(0, 10000);
                }
            }
            "params_persist_ms" => {
                if let Ok(v) = val_str.parse::<u64>() {
                    self.as_mut().rust_mut().prefs.params_persist_ms = v.clamp(100, 30000);
                }
            }
            "links_persist_ms" => {
                if let Ok(v) = val_str.parse::<u64>() {
                    self.as_mut().rust_mut().prefs.links_persist_ms = v.clamp(100, 30000);
                }
            }
            "poll_interval_ms" => {
                if let Ok(v) = val_str.parse::<u64>() {
                    self.as_mut().rust_mut().prefs.poll_interval_ms = v.clamp(16, 1000);
                }
            }
            "auto_learn_rules" => {
                if let Ok(v) = val_str.parse::<bool>() {
                    self.as_mut().rust_mut().prefs.auto_learn_rules = v;
                }
            }
            "start_minimized" => {
                if let Ok(v) = val_str.parse::<bool>() {
                    self.as_mut().rust_mut().prefs.start_minimized = v;
                }
            }
            "close_to_tray" => {
                if let Ok(v) = val_str.parse::<bool>() {
                    self.as_mut().rust_mut().prefs.close_to_tray = v;
                }
            }
            "pw_tick_interval_ms" => {
                if let Ok(v) = val_str.parse::<u64>() {
                    self.as_mut().rust_mut().prefs.pw_tick_interval_ms = v.clamp(1, 200);
                }
            }
            "passive_plugins" => {
                if let Ok(v) = val_str.parse::<bool>() {
                    self.as_mut().rust_mut().prefs.passive_plugins = v;
                    crate::plugin::PASSIVE_EFFECTS.store(v, std::sync::atomic::Ordering::Relaxed);
                }
            }
            "hide_unnamed_nodes" => {
                if let Ok(v) = val_str.parse::<bool>() {
                    self.as_mut().toggle_patchbay_hide_unnamed(v);
                }
            }
            "osc_enabled" => {
                if let Ok(v) = val_str.parse::<bool>() {
                    self.as_mut().rust_mut().prefs.osc_enabled = v;
                    self.as_mut().restart_osc();
                }
            }
            "osc_port" => {
                if let Ok(v) = val_str.parse::<u16>() {
                    self.as_mut().rust_mut().prefs.osc_port = v.max(1024);
                    self.as_mut().restart_osc();
                }
            }
            "osc_listen_all" => {
                if let Ok(v) = val_str.parse::<bool>() {
                    self.as_mut().rust_mut().prefs.osc_listen_all = v;
                    self.as_mut().restart_osc();
                }
            }
            "snap_to_grid" => {
                if let Ok(v) = val_str.parse::<bool>() {
                    self.as_mut().rust_mut().prefs.snap_to_grid = v;
                }
            }
            "grid_size_px" => {
                if let Ok(v) = val_str.parse::<u32>() {
                    self.as_mut().rust_mut().prefs.grid_size_px = v.clamp(8, 96);
                }
            }
            "snap_gap_px" => {
                if let Ok(v) = val_str.parse::<u32>() {
                    self.as_mut().rust_mut().prefs.snap_gap_px = v.clamp(0, 200);
                }
            }
            "node_spacing_px" => {
                if let Ok(v) = val_str.parse::<u32>() {
                    self.as_mut().rust_mut().prefs.node_spacing_px = v.clamp(4, 96);
                }
            }
            "card_row_px" => {
                if let Ok(v) = val_str.parse::<u32>() {
                    self.as_mut().rust_mut().prefs.card_row_px = v.clamp(16, 32);
                }
            }
            "wire_style" => {
                if matches!(
                    val_str.as_str(),
                    "bezier" | "straight" | "stepped" | "circuit"
                ) {
                    self.as_mut().rust_mut().prefs.wire_style = val_str.clone();
                } else {
                    log::warn!("Unknown wire style '{}'", val_str);
                    return;
                }
            }
            "gradient_wires" => {
                if let Ok(v) = val_str.parse::<bool>() {
                    self.as_mut().rust_mut().prefs.gradient_wires = v;
                }
            }
            "column_wrap" => {
                if let Ok(v) = val_str.parse::<u32>() {
                    self.as_mut().rust_mut().prefs.column_wrap = v.min(40);
                }
            }
            "palette" => {
                if PALETTES.contains(&val_str.as_str())
                    || val_str
                        .strip_prefix("custom:")
                        .is_some_and(crate::theme_css::valid_theme_name)
                {
                    self.as_mut().rust_mut().prefs.palette = val_str.clone();
                } else {
                    log::warn!("Unknown palette '{}'", val_str);
                    return;
                }
            }
            "drag_mode" => {
                if matches!(val_str.as_str(), "grab" | "select") {
                    self.as_mut().rust_mut().prefs.drag_mode = val_str.clone();
                } else {
                    log::warn!("Unknown drag mode '{}'", val_str);
                    return;
                }
            }
            "theme" => {
                if matches!(val_str.as_str(), "system" | "dark" | "light") {
                    self.as_mut().rust_mut().prefs.theme = val_str.clone();
                } else {
                    log::warn!("Unknown theme '{}'", val_str);
                    return;
                }
            }
            "pw_operation_cooldown_ms" => {
                if let Ok(v) = val_str.parse::<u64>() {
                    self.as_mut().rust_mut().prefs.pw_operation_cooldown_ms = v.clamp(10, 1000);
                }
            }
            _ => {
                log::warn!("Unknown preference key: {}", key_str);
                return;
            }
        }

        log::info!("Preference updated: {} = {}", key_str, val_str);
        save_preferences(&self.rust().prefs);
    }

    pub fn reset_preferences(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().prefs = Preferences::default();
        save_preferences(&self.rust().prefs);
        crate::plugin::PASSIVE_EFFECTS.store(false, std::sync::atomic::Ordering::Relaxed);
        let exclusive = self.rust().prefs.patchbay_exclusive;
        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            patchbay.exclusive = exclusive;
        }
        self.as_mut().set_patchbay_exclusive(exclusive);
        let hide_unnamed = self.rust().prefs.hide_unnamed_nodes;
        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            patchbay.hide_unnamed = hide_unnamed;
        }
        crate::pipewire::HIDE_UNNAMED_NODES
            .store(hide_unnamed, std::sync::atomic::Ordering::Relaxed);
        self.as_mut().set_patchbay_hide_unnamed(hide_unnamed);
        log::info!("Preferences reset to defaults");
    }

    pub fn get_poll_interval_ms(self: Pin<&mut Self>) -> i32 {
        self.rust().prefs.poll_interval_ms as i32
    }
}

pub(super) const PALETTES: &[&str] = &["zest", "catppuccin", "gruvbox", "nord", "solarized"];

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct Preferences {
    #[serde(default = "Preferences::default_rule_settle_ms")]
    pub rule_settle_ms: u64,

    #[serde(default = "Preferences::default_params_persist_ms")]
    pub params_persist_ms: u64,

    #[serde(default = "Preferences::default_links_persist_ms")]
    pub links_persist_ms: u64,

    #[serde(default = "Preferences::default_poll_interval_ms")]
    pub poll_interval_ms: u64,

    #[serde(default = "Preferences::default_auto_learn_rules")]
    pub auto_learn_rules: bool,

    #[serde(default = "Preferences::default_start_minimized")]
    pub start_minimized: bool,

    #[serde(default = "Preferences::default_close_to_tray")]
    pub close_to_tray: bool,

    #[serde(default = "Preferences::default_pw_tick_interval_ms")]
    pub pw_tick_interval_ms: u64,

    #[serde(default = "Preferences::default_pw_operation_cooldown_ms")]
    pub pw_operation_cooldown_ms: u64,

    #[serde(default = "Preferences::default_patchbay_exclusive")]
    pub patchbay_exclusive: bool,

    #[serde(default = "Preferences::default_theme")]
    pub theme: String,

    #[serde(default = "Preferences::default_snap_gap_px")]
    pub snap_gap_px: u32,

    #[serde(default = "Preferences::default_snap_to_grid")]
    pub snap_to_grid: bool,

    #[serde(default = "Preferences::default_grid_size_px")]
    pub grid_size_px: u32,

    #[serde(default = "Preferences::default_node_spacing_px")]
    pub node_spacing_px: u32,

    #[serde(default = "Preferences::default_card_row_px")]
    pub card_row_px: u32,

    #[serde(default = "Preferences::default_drag_mode")]
    pub drag_mode: String,

    #[serde(default = "Preferences::default_wire_style")]
    pub wire_style: String,

    #[serde(default)]
    pub gradient_wires: bool,

    #[serde(default)]
    pub column_wrap: u32,

    #[serde(default = "Preferences::default_palette")]
    pub palette: String,

    #[serde(default)]
    pub passive_plugins: bool,

    #[serde(default = "Preferences::default_hide_unnamed_nodes")]
    pub hide_unnamed_nodes: bool,

    #[serde(default)]
    pub osc_enabled: bool,

    #[serde(default = "Preferences::default_osc_port")]
    pub osc_port: u16,

    #[serde(default)]
    pub osc_listen_all: bool,
}

impl Preferences {
    pub(super) fn default_rule_settle_ms() -> u64 {
        500
    }
    pub(super) fn default_params_persist_ms() -> u64 {
        1000
    }
    pub(super) fn default_links_persist_ms() -> u64 {
        2000
    }
    pub(super) fn default_poll_interval_ms() -> u64 {
        100
    }
    pub(super) fn default_auto_learn_rules() -> bool {
        true
    }
    pub(super) fn default_start_minimized() -> bool {
        false
    }
    pub(super) fn default_close_to_tray() -> bool {
        false
    }
    pub(super) fn default_pw_tick_interval_ms() -> u64 {
        10
    }
    pub(super) fn default_pw_operation_cooldown_ms() -> u64 {
        50
    }
    pub(super) fn default_patchbay_exclusive() -> bool {
        true
    }
    pub(super) fn default_theme() -> String {
        "system".to_string()
    }
    pub(super) fn default_snap_gap_px() -> u32 {
        10
    }
    pub(super) fn default_grid_size_px() -> u32 {
        24
    }
    pub(super) fn default_snap_to_grid() -> bool {
        true
    }
    pub(super) fn default_node_spacing_px() -> u32 {
        20
    }
    pub(super) fn default_card_row_px() -> u32 {
        22
    }
    pub(super) fn default_drag_mode() -> String {
        "grab".to_string()
    }
    pub(super) fn default_wire_style() -> String {
        "bezier".to_string()
    }
    pub(super) fn default_palette() -> String {
        "zest".to_string()
    }
    pub(super) fn default_hide_unnamed_nodes() -> bool {
        true
    }
    pub(super) fn default_osc_port() -> u16 {
        crate::osc::DEFAULT_PORT
    }
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            rule_settle_ms: Self::default_rule_settle_ms(),
            params_persist_ms: Self::default_params_persist_ms(),
            links_persist_ms: Self::default_links_persist_ms(),
            poll_interval_ms: Self::default_poll_interval_ms(),
            auto_learn_rules: Self::default_auto_learn_rules(),
            start_minimized: Self::default_start_minimized(),
            close_to_tray: Self::default_close_to_tray(),
            pw_tick_interval_ms: Self::default_pw_tick_interval_ms(),
            pw_operation_cooldown_ms: Self::default_pw_operation_cooldown_ms(),
            patchbay_exclusive: Self::default_patchbay_exclusive(),
            theme: Self::default_theme(),
            snap_gap_px: Self::default_snap_gap_px(),
            snap_to_grid: Self::default_snap_to_grid(),
            grid_size_px: Self::default_grid_size_px(),
            node_spacing_px: Self::default_node_spacing_px(),
            card_row_px: Self::default_card_row_px(),
            drag_mode: Self::default_drag_mode(),
            wire_style: Self::default_wire_style(),
            gradient_wires: false,
            column_wrap: 0,
            palette: Self::default_palette(),
            passive_plugins: false,
            hide_unnamed_nodes: Self::default_hide_unnamed_nodes(),
            osc_enabled: false,
            osc_port: Self::default_osc_port(),
            osc_listen_all: false,
        }
    }
}

pub(super) fn load_preferences() -> Preferences {
    load_config::<Preferences>("preferences.json")
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub(super) fn save_preferences(prefs: &Preferences) {
    let json = serde_json::to_string_pretty(prefs).unwrap_or_default();
    if write_config("preferences.json", json.as_bytes(), false) {
        log::debug!("save_preferences: written");
    }
}
