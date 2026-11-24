use super::*;

impl qobject::AppController {
    pub fn get_plugin_params_json(self: Pin<&mut Self>, node_id: u32) -> QString {
        let instance_id = self.find_instance_id_for_node(node_id);
        if let Some(instance_id) = instance_id
            && let Some(ref mgr) = self.rust().plugin_manager
            && let Some(info) = mgr.get_instance(instance_id)
        {
            let params: Vec<serde_json::Value> = info
                .parameters
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "portIndex": p.port_index,
                        "symbol": p.symbol,
                        "name": p.name,
                        "value": p.value,
                        "min": p.min,
                        "max": p.max,
                        "default": p.default,
                        "isToggle": p.is_toggle,
                    })
                })
                .collect();
            let result = serde_json::json!({
                "instanceId": instance_id,
                "pluginUri": info.plugin_uri,
                "displayName": info.display_name,
                "bypassed": info.bypassed,
                "enabled": info.enabled,
                "parameters": params,
            });
            let json = serde_json::to_string(&result).unwrap_or_default();
            return QString::from(&json);
        }
        QString::from("{}")
    }

    pub fn plugin_node_for_instance(self: Pin<&mut Self>, instance_id: u64) -> i32 {
        self.rust()
            .plugin_manager
            .as_ref()
            .and_then(|mgr| mgr.get_instance(instance_id))
            .and_then(|info| info.pw_node_id)
            .and_then(|id| i32::try_from(id).ok())
            .unwrap_or(-1)
    }

    pub fn set_plugin_parameter(
        mut self: Pin<&mut Self>,
        node_id: u32,
        port_index: u32,
        value: f32,
    ) {
        if let Some(instance_id) = self.find_instance_id_for_node(node_id) {
            self.as_mut()
                .set_instance_parameter(instance_id, port_index as usize, value);
        }
    }

    pub fn set_plugin_bypass(mut self: Pin<&mut Self>, node_id: u32, bypassed: bool) {
        if let Some(instance_id) = self.find_instance_id_for_node(node_id) {
            self.as_mut().set_instance_bypass(instance_id, bypassed);
            self.as_mut().graph_changed();
        }
    }

    pub fn set_plugin_enabled(mut self: Pin<&mut Self>, node_id: u32, enabled: bool) {
        if let Some(instance_id) = self.find_instance_id_for_node(node_id) {
            self.as_mut().set_instance_enabled(instance_id, enabled);
            self.as_mut().graph_changed();
        }
    }

    pub(super) fn set_instance_parameter(
        mut self: Pin<&mut Self>,
        instance_id: u64,
        port_index: usize,
        value: f32,
    ) {
        if let Some(ref tx) = self.rust().cmd_tx {
            let _ = tx.send(PwCommand::SetPluginParameter {
                instance_id,
                port_index,
                value,
            });
        }
        if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
            mgr.update_parameter(instance_id, port_index, value);
        }
        self.as_mut().rust_mut().params_dirty = true;
        if self.rust().params_dirty_since.is_none() {
            self.as_mut().rust_mut().params_dirty_since = Some(Instant::now());
        }
    }

    pub(super) fn set_instance_bypass(mut self: Pin<&mut Self>, instance_id: u64, bypassed: bool) {
        if let Some(ref tx) = self.rust().cmd_tx {
            let _ = tx.send(PwCommand::SetPluginBypass {
                instance_id,
                bypassed,
            });
        }
        if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager
            && let Some(info) = mgr.get_instance_mut(instance_id)
        {
            info.bypassed = bypassed;
        }
        self.as_mut().rust_mut().params_dirty = true;
        if self.rust().params_dirty_since.is_none() {
            self.as_mut().rust_mut().params_dirty_since = Some(Instant::now());
        }
    }

    pub(super) fn set_instance_enabled(mut self: Pin<&mut Self>, instance_id: u64, enabled: bool) {
        if let Some(ref tx) = self.rust().cmd_tx {
            let _ = tx.send(PwCommand::SetPluginEnabled {
                instance_id,
                enabled,
            });
        }
        if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager
            && let Some(info) = mgr.get_instance_mut(instance_id)
        {
            info.enabled = enabled;
        }
        self.as_mut().rust_mut().params_dirty = true;
        if self.rust().params_dirty_since.is_none() {
            self.as_mut().rust_mut().params_dirty_since = Some(Instant::now());
        }
    }
}
