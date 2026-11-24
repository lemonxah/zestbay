use super::*;

#[derive(Default)]
struct PortEventPump {
    next_port: (u64, usize),
}

impl PortEventPump {
    fn send_pending(
        &mut self,
        map: &mut HashMap<u64, BridgeInstance>,
        tx: &SyncSender<Vec<u8>>,
    ) -> bool {
        let mut instances: Vec<_> = map
            .iter()
            .map(|(&id, inst)| {
                let count = inst.port_updates.control_inputs.len()
                    + inst.port_updates.control_outputs.len();
                (id, count.min(inst.prev.len()))
            })
            .collect();
        instances.sort_unstable();
        let ports = instances
            .iter()
            .flat_map(|&(id, count)| (0..count).map(move |index| (id, index)));
        let next_port = self.next_port;
        let ordered = ports
            .clone()
            .filter(|port| *port >= next_port)
            .chain(ports.filter(|port| *port < next_port));
        for (instance_id, index) in ordered {
            let inst = map.get_mut(&instance_id).unwrap();
            let slot = inst
                .port_updates
                .control_inputs
                .iter()
                .chain(inst.port_updates.control_outputs.iter())
                .nth(index)
                .unwrap();
            let prev = &mut inst.prev[index];
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
                Err(TrySendError::Full(_)) => {
                    self.next_port = (instance_id, index);
                    return true;
                }
                Err(TrySendError::Disconnected(_)) => return false,
            }
        }
        self.next_port = (0, 0);
        true
    }
}

pub(super) fn port_event_pump(shared: &BridgeShared, stop: &AtomicBool, tx: &SyncSender<Vec<u8>>) {
    let mut pump = PortEventPump::default();
    while !stop.load(Ordering::Acquire) && shared.alive.load(Ordering::Acquire) {
        let connected = match shared.instances.lock() {
            Ok(mut map) => pump.send_pending(&mut map, tx),
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
        let mut pump = PortEventPump::default();
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
        assert!(pump.send_pending(&mut map, &tx));
        assert!(rx.try_recv().is_err());

        pu.control_outputs[0].value.store(0.25);
        assert!(pump.send_pending(&mut map, &tx));
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
        assert!(pump.send_pending(&mut map, &tx));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn saturated_transport_retries_latest_value_without_blocking() {
        let mut pump = PortEventPump::default();
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
        assert!(pump.send_pending(&mut map, &tx));
        assert_eq!(map[&42].prev[1], 0.0);
        pu.control_outputs[0].value.store(0.75);
        rx.try_recv().unwrap();
        assert!(pump.send_pending(&mut map, &tx));
        let event: HostMessage = serde_json::from_slice(&rx.try_recv().unwrap()).unwrap();
        assert!(matches!(event, HostMessage::PortEvent { value: 0.75, .. }));
        assert_eq!(map[&42].prev[1], 0.75);
        drop(rx);
        pu.control_outputs[0].value.store(1.0);
        assert!(!pump.send_pending(&mut map, &tx));
        assert_eq!(map[&42].prev[1], 0.75);
    }

    #[test]
    fn busy_controls_do_not_starve_other_ports_or_instances() {
        let mut pump = PortEventPump::default();
        let (tx, rx) = std::sync::mpsc::sync_channel(QUEUE_CAPACITY);
        let mut map: HashMap<_, _> = [17, 41, 99]
            .into_iter()
            .map(|instance_id| {
                let port_updates = Arc::new(PortUpdates {
                    control_inputs: (0..QUEUE_CAPACITY)
                        .map(|i| PortSlot {
                            port_index: 3 + i * 2,
                            value: AtomicF32::new(0.0),
                        })
                        .collect(),
                    control_outputs: vec![PortSlot {
                        port_index: 10_000,
                        value: AtomicF32::new(0.75),
                    }],
                    atom_outputs: Vec::new(),
                    atom_inputs: Vec::new(),
                });
                (
                    instance_id,
                    BridgeInstance {
                        port_updates,
                        prev: vec![0.0; QUEUE_CAPACITY + 1],
                    },
                )
            })
            .collect();
        let mut received = std::collections::HashSet::new();
        for cycle in 1..=7 {
            for inst in map.values() {
                for slot in &inst.port_updates.control_inputs {
                    slot.value.store(cycle as f32);
                }
            }
            assert!(pump.send_pending(&mut map, &tx));
            for line in rx.try_iter() {
                let message: HostMessage = serde_json::from_slice(&line).unwrap();
                let HostMessage::PortEvent {
                    instance_id,
                    port_index,
                    value,
                } = message
                else {
                    panic!("expected PortEvent");
                };
                received.insert((instance_id, port_index));
                if port_index == 10_000 {
                    assert_eq!(value, 0.75);
                }
            }
        }
        assert_eq!(received.len(), 3 * (QUEUE_CAPACITY + 1));
        for inst in map.values() {
            assert_eq!(inst.prev[QUEUE_CAPACITY], 0.75);
        }
    }

    #[test]
    fn closing_the_pending_instance_does_not_skip_remaining_controls() {
        let mut pump = PortEventPump::default();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let mut map: HashMap<_, _> = [10, 20]
            .into_iter()
            .map(|id| {
                (
                    id,
                    BridgeInstance {
                        port_updates: updates(),
                        prev: vec![0.0, -1.0],
                    },
                )
            })
            .collect();
        assert!(pump.send_pending(&mut map, &tx));
        let first: HostMessage = serde_json::from_slice(&rx.try_recv().unwrap()).unwrap();
        assert!(matches!(
            first,
            HostMessage::PortEvent {
                instance_id: 10,
                port_index: 3,
                ..
            }
        ));
        map.remove(&10);
        map.insert(
            5,
            BridgeInstance {
                port_updates: updates(),
                prev: vec![0.0, -1.0],
            },
        );
        let mut received = std::collections::HashSet::new();
        for _ in 0..4 {
            assert!(pump.send_pending(&mut map, &tx));
            let message: HostMessage = serde_json::from_slice(&rx.try_recv().unwrap()).unwrap();
            let HostMessage::PortEvent {
                instance_id,
                port_index,
                ..
            } = message
            else {
                panic!("expected PortEvent");
            };
            received.insert((instance_id, port_index));
        }
        assert_eq!(
            received,
            std::collections::HashSet::from([(5, 3), (5, 7), (20, 3), (20, 7)])
        );
        assert!(pump.send_pending(&mut map, &tx));
        assert!(rx.try_recv().is_err());
        map.clear();
        assert!(pump.send_pending(&mut map, &tx));
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
