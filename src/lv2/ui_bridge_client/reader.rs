use super::*;

pub(super) fn bridge_reader(
    reader: impl BufRead,
    shared: &BridgeShared,
    event_tx: &Sender<PwEvent>,
    cmd_tx: &Sender<PwCommand>,
) {
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.is_empty() {
            continue;
        }

        let msg: BridgeMessage = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(e) => {
                log::warn!("UI bridge: invalid message: {} — {:?}", line, e);
                continue;
            }
        };

        match msg {
            BridgeMessage::Ready => {
                log::info!("UI bridge process ready");
            }
            BridgeMessage::Opened { instance_id } => {
                log::info!("UI bridge: UI opened for instance {}", instance_id);
                let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginUiOpened { instance_id }));
            }
            BridgeMessage::OpenFailed { instance_id, error } => {
                log::error!(
                    "UI bridge: UI open failed for instance {}: {}",
                    instance_id,
                    error
                );
                if let Ok(mut map) = shared.instances.lock() {
                    map.remove(&instance_id);
                }
                let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginError {
                    instance_id: Some(instance_id),
                    message: format!("Plugin UI failed: {}", error),
                    fatal: false,
                }));
            }
            BridgeMessage::Closed { instance_id } => {
                log::info!("UI bridge: UI closed for instance {}", instance_id);
                if let Ok(mut map) = shared.instances.lock() {
                    map.remove(&instance_id);
                }
                let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginUiClosed { instance_id }));
            }
            BridgeMessage::PortWrite {
                instance_id,
                port_index,
                value,
            } => {
                let _ = cmd_tx.send(PwCommand::SetPluginParameter {
                    instance_id,
                    port_index,
                    value,
                });
            }
            BridgeMessage::AtomWrite {
                instance_id,
                port_index,
                data_b64,
            } => {
                let data = match base64::engine::general_purpose::STANDARD.decode(&data_b64) {
                    Ok(d) => d,
                    Err(e) => {
                        log::warn!("UI bridge: bad atom payload: {}", e);
                        continue;
                    }
                };
                let map = match shared.instances.lock() {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                if let Some(inst) = map.get(&instance_id)
                    && let Some(atom_in) = inst
                        .port_updates
                        .atom_inputs
                        .iter()
                        .find(|a| a.port_index == port_index)
                    && !atom_in.write(&data)
                {
                    log::warn!(
                        "UI atom queue full for instance {} port {}; message dropped",
                        instance_id,
                        port_index
                    );
                }
            }
        }
    }
}
