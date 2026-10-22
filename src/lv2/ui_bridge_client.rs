//! Host-side client for the UI bridge process.
//!
//! Spawns `zestbay-ui-bridge` as a child process and communicates
//! via stdin/stdout newline-delimited JSON messages.
//!
//! Besides forwarding open/close requests, the client runs a small pump
//! thread that mirrors control-port changes (plugin outputs and host-side
//! parameter changes) to the bridge UIs, like the in-process GTK timer does.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine;

use crate::lv2::urid::UridMapper;
use crate::pipewire::{PluginEvent, PwCommand, PwEvent};
use crate::plugin::types::SharedPortUpdates;
use crate::ui_bridge::protocol::{BridgeMessage, HostMessage};

const PUMP_INTERVAL: Duration = Duration::from_millis(33);
const QUIT_TIMEOUT: Duration = Duration::from_secs(2);

/// A UI hosted by the bridge.
struct BridgeInstance {
    port_updates: SharedPortUpdates,
    /// Last value sent per slot (control inputs, then control outputs).
    prev: Vec<f32>,
}

struct BridgeShared {
    instances: Mutex<HashMap<u64, BridgeInstance>>,
    /// Cleared when the bridge's stdout closes (child exited or crashed).
    alive: AtomicBool,
}

fn encode_line(msg: &HostMessage) -> Option<Vec<u8>> {
    let mut line = serde_json::to_vec(msg).ok()?;
    line.push(b'\n');
    Some(line)
}

pub struct UiBridgeClient {
    child: Child,
    /// Complete lines (JSON + '\n') for the stdin writer thread.
    stdin_tx: Option<Sender<Vec<u8>>>,
    shared: Arc<BridgeShared>,
    pump_stop: Arc<AtomicBool>,
    pump_thread: Option<std::thread::JoinHandle<()>>,
    shut_down: bool,
}

impl UiBridgeClient {
    pub fn spawn(
        event_tx: Sender<PwEvent>,
        cmd_tx: Sender<PwCommand>,
    ) -> Result<Self, String> {
        // Find the bridge binary: check next to our executable first,
        // then the system install location.
        let candidates = [
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("zestbay-ui-bridge"))),
            Some(std::path::PathBuf::from("/usr/lib/zestbay/zestbay-ui-bridge")),
            Some(std::path::PathBuf::from("/usr/local/lib/zestbay/zestbay-ui-bridge")),
        ];

        let bridge_path = candidates
            .into_iter()
            .flatten()
            .find(|p| p.exists())
            .ok_or_else(|| {
                "UI bridge binary not found (searched next to exe and /usr/lib/zestbay/)"
                    .to_string()
            })?;

        log::info!("UI bridge binary: {:?}", bridge_path);

        let mut child = Command::new(&bridge_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()) // bridge errors go to our stderr
            // Force Mesa GLX client library — avoids NVIDIA/Mesa vendor
            // mismatch on Wayland/XWayland where the server is Mesa but
            // the host process loaded NVIDIA's EGL.
            .env("__GLX_VENDOR_LIBRARY_NAME", "mesa")
            .spawn()
            .map_err(|e| format!("Failed to spawn UI bridge: {}", e))?;

        let stdout = child.stdout.take().unwrap();
        let stdin = child.stdin.take().unwrap();

        let shared = Arc::new(BridgeShared {
            instances: Mutex::new(HashMap::new()),
            alive: AtomicBool::new(true),
        });

        // Stdin writer thread: one write_all per complete line.
        let (stdin_tx, stdin_rx) = std::sync::mpsc::channel::<Vec<u8>>();
        std::thread::Builder::new()
            .name("ui-bridge-stdin".into())
            .spawn(move || {
                let mut stdin = stdin;
                for line in stdin_rx {
                    if stdin.write_all(&line).is_err() || stdin.flush().is_err() {
                        break;
                    }
                }
                // Dropping stdin closes the pipe: the bridge sees EOF and exits.
            })
            .map_err(|e| format!("Failed to spawn stdin thread: {}", e))?;

        // Stdout reader thread
        let reader_shared = shared.clone();
        std::thread::Builder::new()
            .name("ui-bridge-stdout".into())
            .spawn(move || {
                bridge_reader(BufReader::new(stdout), &reader_shared, &event_tx, &cmd_tx);
                reader_shared.alive.store(false, Ordering::Release);
                let orphaned: Vec<u64> = match reader_shared.instances.lock() {
                    Ok(mut map) => map.drain().map(|(id, _)| id).collect(),
                    Err(_) => Vec::new(),
                };
                for instance_id in orphaned {
                    let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginUiClosed {
                        instance_id,
                    }));
                }
                log::info!("UI bridge stdout reader exited");
            })
            .map_err(|e| format!("Failed to spawn stdout thread: {}", e))?;

        // Port event pump
        let pump_stop = Arc::new(AtomicBool::new(false));
        let pump_thread = {
            let shared = shared.clone();
            let stop = pump_stop.clone();
            let tx = stdin_tx.clone();
            std::thread::Builder::new()
                .name("ui-bridge-pump".into())
                .spawn(move || port_event_pump(&shared, &stop, &tx))
                .map_err(|e| format!("Failed to spawn pump thread: {}", e))?
        };

        Ok(Self {
            child,
            stdin_tx: Some(stdin_tx),
            shared,
            pump_stop,
            pump_thread: Some(pump_thread),
            shut_down: false,
        })
    }

    fn send(&self, msg: &HostMessage) -> bool {
        match (&self.stdin_tx, encode_line(msg)) {
            (Some(tx), Some(line)) => tx.send(line).is_ok(),
            _ => false,
        }
    }

    /// Whether the child process is still running and talking to us.
    pub fn is_alive(&mut self) -> bool {
        if self.shut_down || !self.shared.alive.load(Ordering::Acquire) {
            return false;
        }
        match self.child.try_wait() {
            Ok(None) => true,
            Ok(Some(status)) => {
                log::warn!("UI bridge process exited: {}", status);
                false
            }
            Err(e) => {
                log::warn!("UI bridge process state unknown: {}", e);
                false
            }
        }
    }

    /// Ask the bridge to open a UI.  Re-opening an instance that is already
    /// open closes the old window first.  Returns false if the request could
    /// not be sent.
    #[allow(clippy::too_many_arguments)]
    pub fn open_ui(
        &mut self,
        instance_id: u64,
        plugin_uri: &str,
        ui_uri: &str,
        ui_type_uri: &str,
        bundle_path: &str,
        binary_path: &str,
        display_name: &str,
        control_values: Vec<(usize, f32)>,
        urid_mapper: &Arc<UridMapper>,
        sample_rate: f32,
        port_updates: SharedPortUpdates,
    ) -> bool {
        if !self.is_alive() {
            return false;
        }
        if self.is_open(instance_id) {
            log::info!("UI bridge: re-opening UI for instance {}", instance_id);
            self.close_ui(instance_id);
        }

        let urid_map: Vec<(String, u32)> = urid_mapper.snapshot();

        let msg = HostMessage::Open {
            instance_id,
            plugin_uri: plugin_uri.to_string(),
            ui_uri: ui_uri.to_string(),
            ui_type_uri: ui_type_uri.to_string(),
            bundle_path: bundle_path.to_string(),
            binary_path: binary_path.to_string(),
            title: format!("ZestBay — {}", display_name),
            control_values,
            urid_map,
            lv2_handle: 0, // Not usable across processes
            sample_rate,
        };

        // Register before sending so the pump starts mirroring right away;
        // initial values were sent with Open.
        let prev: Vec<f32> = port_updates
            .control_inputs
            .iter()
            .chain(port_updates.control_outputs.iter())
            .map(|s| s.value.load())
            .collect();
        if let Ok(mut map) = self.shared.instances.lock() {
            map.insert(instance_id, BridgeInstance { port_updates, prev });
        }

        log::info!(
            "UI bridge: sending Open for instance {} (plugin={})",
            instance_id,
            plugin_uri
        );
        if self.send(&msg) {
            true
        } else {
            if let Ok(mut map) = self.shared.instances.lock() {
                map.remove(&instance_id);
            }
            false
        }
    }

    /// Close the bridge UI for `instance_id` if it is open.  Returns whether
    /// it was open.
    pub fn close_ui(&mut self, instance_id: u64) -> bool {
        let was_open = match self.shared.instances.lock() {
            Ok(mut map) => map.remove(&instance_id).is_some(),
            Err(_) => false,
        };
        if was_open {
            self.send(&HostMessage::Close { instance_id });
        }
        was_open
    }

    pub fn is_open(&self, instance_id: u64) -> bool {
        self.shared
            .instances
            .lock()
            .map(|m| m.contains_key(&instance_id))
            .unwrap_or(false)
    }

    /// Ask the bridge to quit and reap the child (killing it if it does not
    /// exit within a couple of seconds).  Idempotent.
    pub fn shutdown(&mut self) {
        if self.shut_down {
            return;
        }
        self.shut_down = true;

        self.send(&HostMessage::Quit);
        self.pump_stop.store(true, Ordering::Release);
        if let Some(pump) = self.pump_thread.take() {
            let _ = pump.join();
        }
        // Last sender gone → writer thread exits → bridge stdin hits EOF.
        self.stdin_tx = None;

        let deadline = Instant::now() + QUIT_TIMEOUT;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                _ => {
                    log::warn!("UI bridge did not exit in time; killing it");
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    break;
                }
            }
        }
        if let Ok(mut map) = self.shared.instances.lock() {
            map.clear();
        }
    }
}

impl Drop for UiBridgeClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn bridge_reader(
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
                let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginUiOpened {
                    instance_id,
                }));
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
                let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginUiClosed {
                    instance_id,
                }));
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

/// Collect port events for values that changed since the last call.
fn collect_port_events(map: &mut HashMap<u64, BridgeInstance>) -> Vec<HostMessage> {
    let mut out = Vec::new();
    for (&instance_id, inst) in map.iter_mut() {
        let slots = inst
            .port_updates
            .control_inputs
            .iter()
            .chain(inst.port_updates.control_outputs.iter());
        for (i, slot) in slots.enumerate() {
            let value = slot.value.load();
            let Some(prev) = inst.prev.get_mut(i) else {
                break;
            };
            if value.to_bits() != prev.to_bits() {
                *prev = value;
                out.push(HostMessage::PortEvent {
                    instance_id,
                    port_index: slot.port_index,
                    value,
                });
            }
        }
    }
    out
}

fn port_event_pump(shared: &BridgeShared, stop: &AtomicBool, tx: &Sender<Vec<u8>>) {
    while !stop.load(Ordering::Acquire) && shared.alive.load(Ordering::Acquire) {
        let events = match shared.instances.lock() {
            Ok(mut map) => collect_port_events(&mut map),
            Err(_) => break,
        };
        for msg in &events {
            if let Some(line) = encode_line(msg)
                && tx.send(line).is_err()
            {
                return;
            }
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
        let pu = updates();
        let mut map = HashMap::new();
        map.insert(
            42,
            BridgeInstance {
                port_updates: pu.clone(),
                prev: vec![0.5, 0.0],
            },
        );
        assert!(collect_port_events(&mut map).is_empty());

        pu.control_outputs[0].value.store(0.25);
        let events = collect_port_events(&mut map);
        assert_eq!(events.len(), 1);
        match &events[0] {
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
        assert!(collect_port_events(&mut map).is_empty());
    }

    #[test]
    fn encoded_line_is_single_terminated_line() {
        let line = encode_line(&HostMessage::Close { instance_id: 1 }).unwrap();
        assert_eq!(line.last(), Some(&b'\n'));
        assert_eq!(line.iter().filter(|&&b| b == b'\n').count(), 1);
    }
}
