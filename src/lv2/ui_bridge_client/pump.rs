use super::*;

fn pump_port_events(map: &mut HashMap<u64, BridgeInstance>, tx: &SyncSender<Vec<u8>>) -> bool {
    for (&instance_id, inst) in map.iter_mut() {
        let slots = inst
            .port_updates
            .control_inputs
            .iter()
            .chain(inst.port_updates.control_outputs.iter());
        for (slot, prev) in slots.zip(&mut inst.prev) {
            let value = slot.value.load();
            if value.to_bits() == prev.to_bits() {
                continue;
            }
            let message = HostMessage::PortEvent {
                instance_id,
                port_index: slot.port_index,
                value,
            };
            let Some(line) = encode_line(&message) else {
                continue;
            };
            match tx.try_send(line) {
                Ok(()) => *prev = value,
                Err(TrySendError::Full(_)) => return true,
                Err(TrySendError::Disconnected(_)) => return false,
            }
        }
    }
    true
}

pub(super) fn port_event_pump(shared: &BridgeShared, stop: &AtomicBool, tx: &SyncSender<Vec<u8>>) {
    while !stop.load(Ordering::Acquire) && shared.alive.load(Ordering::Acquire) {
        let connected = match shared.instances.lock() {
            Ok(mut map) => pump_port_events(&mut map, tx),
            Err(_) => false,
        };
        if !connected {
            shared.alive.store(false, Ordering::Release);
            return;
        }
        std::thread::sleep(PUMP_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::types::{AtomicF32, PortSlot, PortUpdates};

    fn updates() -> SharedPortUpdates {
        Arc::new(PortUpdates {
            control_inputs: vec![PortSlot {
                port_index: 3,
                value: AtomicF32::new(0.5),
            }],
            control_outputs: vec![PortSlot {
                port_index: 7,
                value: AtomicF32::new(0.0),
            }],
            atom_outputs: Vec::new(),
            atom_inputs: Vec::new(),
        })
    }

    #[test]
    fn pump_sends_only_changes() {
        let (tx, rx) = std::sync::mpsc::sync_channel(2);
        let pu = updates();
        let mut map = HashMap::new();
        map.insert(
            42,
            BridgeInstance {
                port_updates: pu.clone(),
                prev: vec![0.5, 0.0],
            },
        );
        assert!(pump_port_events(&mut map, &tx));
        assert!(rx.try_recv().is_err());

        pu.control_outputs[0].value.store(0.25);
        assert!(pump_port_events(&mut map, &tx));
        let event: HostMessage = serde_json::from_slice(&rx.try_recv().unwrap()).unwrap();
        match &event {
            HostMessage::PortEvent {
                instance_id,
                port_index,
                value,
            } => {
                assert_eq!(*instance_id, 42);
                assert_eq!(*port_index, 7);
                assert_eq!(*value, 0.25);
            }
            _ => panic!("expected PortEvent"),
        }
        assert!(pump_port_events(&mut map, &tx));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn saturated_transport_retries_latest_value_without_blocking() {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        tx.send(Vec::new()).unwrap();
        let pu = updates();
        let mut map = HashMap::from([(
            42,
            BridgeInstance {
                port_updates: pu.clone(),
                prev: vec![0.5, 0.0],
            },
        )]);
        pu.control_outputs[0].value.store(0.25);
        assert!(pump_port_events(&mut map, &tx));
        assert_eq!(map[&42].prev[1], 0.0);
        pu.control_outputs[0].value.store(0.75);
        rx.try_recv().unwrap();
        assert!(pump_port_events(&mut map, &tx));
        let event: HostMessage = serde_json::from_slice(&rx.try_recv().unwrap()).unwrap();
        assert!(matches!(event, HostMessage::PortEvent { value: 0.75, .. }));
        assert_eq!(map[&42].prev[1], 0.75);
        drop(rx);
        pu.control_outputs[0].value.store(1.0);
        assert!(!pump_port_events(&mut map, &tx));
        assert_eq!(map[&42].prev[1], 0.75);
    }

    #[test]
    fn saturated_transport_closes_the_bridge_instead_of_losing_close() {
        let child = Command::new("sleep").arg("30").spawn().unwrap();
        let (tx, _rx) = std::sync::mpsc::sync_channel(1);
        tx.send(Vec::new()).unwrap();
        let shared = Arc::new(BridgeShared {
            instances: Mutex::new(HashMap::new()),
            alive: AtomicBool::new(true),
        });
        let mut client = UiBridgeClient {
            child,
            stdin_tx: Some(tx),
            shared,
            pump_stop: Arc::new(AtomicBool::new(false)),
            pump_thread: None,
            shut_down: false,
        };
        assert!(!client.send(&HostMessage::Close { instance_id: 42 }));
        assert!(!client.is_alive());
        client.shutdown();
        assert!(client.child.try_wait().unwrap().is_some());
    }

    #[test]
    fn encoded_line_is_single_terminated_line() {
        let line = encode_line(&HostMessage::Close { instance_id: 1 }).unwrap();
        assert_eq!(line.last(), Some(&b'\n'));
        assert_eq!(line.iter().filter(|&&b| b == b'\n').count(), 1);
    }
}
