use super::*;

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub(super) struct SavedPlugin {
    #[serde(default)]
    pub(super) stable_id: String,
    pub(super) uri: String,
    pub(super) display_name: String,
    #[serde(default)]
    pub(super) bypassed: bool,
    #[serde(default = "default_true")]
    pub(super) enabled: bool,
    #[serde(default)]
    pub(super) parameters: Vec<SavedPluginParam>,
    #[serde(default = "default_lv2_format_str")]
    pub(super) format: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) lv2_state: Vec<crate::lv2::state::StateEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) plugin_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) scan_info: Option<crate::plugin::PluginInfo>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub(super) port_names: std::collections::BTreeMap<String, String>,
}

impl From<&crate::plugin::PluginInstanceInfo> for SavedPlugin {
    fn from(info: &crate::plugin::PluginInstanceInfo) -> Self {
        use base64::Engine;
        Self {
            stable_id: info.stable_id.clone(),
            uri: info.plugin_uri.clone(),
            display_name: info.display_name.clone(),
            bypassed: info.bypassed,
            enabled: info.enabled,
            parameters: info
                .parameters
                .iter()
                .map(|p| SavedPluginParam {
                    port_index: p.port_index,
                    symbol: p.symbol.clone(),
                    value: p.value,
                })
                .collect(),
            format: info.format.as_str().to_string(),
            scan_info: info.scan_info.clone(),
            port_names: info.port_names.clone(),
            lv2_state: info.lv2_state.clone(),
            plugin_state: info
                .plugin_state
                .as_ref()
                .map(|blob| base64::engine::general_purpose::STANDARD.encode(blob)),
        }
    }
}

impl SavedPlugin {
    pub(super) fn usable_scan_info(&self) -> Option<crate::plugin::PluginInfo> {
        let info = self.scan_info.as_ref()?;
        if info.uri != self.uri || info.format.as_str() != self.format {
            return None;
        }
        let path = std::path::Path::new(&info.library_path);
        let exists = match info.format {
            crate::plugin::PluginFormat::Clap
            | crate::plugin::PluginFormat::Vst2
            | crate::plugin::PluginFormat::Ladspa => path.is_file(),
            crate::plugin::PluginFormat::Vst3 => crate::vst3::scanner::find_bundle_binary(path)
                .is_some_and(|binary| binary.is_file()),
            crate::plugin::PluginFormat::Lv2 => false,
        };
        exists.then(|| info.clone())
    }

    pub(super) fn decode_plugin_state(&self) -> Option<Vec<u8>> {
        use base64::Engine;
        let encoded = self.plugin_state.as_ref()?;
        match base64::engine::general_purpose::STANDARD.decode(encoded) {
            Ok(blob) => Some(blob),
            Err(e) => {
                log::error!(
                    "Saved state of plugin {} ({}) is not valid base64: {}",
                    self.display_name,
                    self.uri,
                    e
                );
                None
            }
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub(super) struct SavedPluginParam {
    pub(super) port_index: usize,
    pub(super) symbol: String,
    pub(super) value: f32,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct SavedPluginLink {
    pub(super) output_node_name: String,
    pub(super) output_port_name: String,
    pub(super) input_node_name: String,
    pub(super) input_port_name: String,
}

#[derive(Clone, Debug)]
pub(super) struct PendingLink {
    pub(super) link: SavedPluginLink,
    pub(super) connect_sent_at: Option<Instant>,
}

impl PendingLink {
    pub(super) fn new(link: SavedPluginLink) -> Self {
        Self {
            link,
            connect_sent_at: None,
        }
    }
}

pub(super) const LINK_CONNECT_GRACE: Duration = Duration::from_secs(5);

pub(super) const PLUGIN_RESCAN_AFTER: Duration = Duration::from_secs(10 * 60);

pub(super) const LINK_SETTLE_AFTER_RECONNECT: Duration = Duration::from_secs(5);

pub(super) fn default_true() -> bool {
    true
}

pub(super) fn default_lv2_format_str() -> String {
    "LV2".to_string()
}
