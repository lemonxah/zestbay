mod pump;
mod reader;

use pump::port_event_pump;
use reader::bridge_reader;

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine;

use crate::lv2::urid::UridMapper;
use crate::pipewire::{PluginEvent, PwCommand, PwEvent};
use crate::plugin::types::SharedPortUpdates;
use crate::ui_bridge::protocol::{BridgeMessage, HostMessage};

const QUEUE_CAPACITY: usize = 256;

const PUMP_INTERVAL: Duration = Duration::from_millis(33);
const QUIT_TIMEOUT: Duration = Duration::from_secs(2);

struct BridgeInstance {
    port_updates: SharedPortUpdates,
    prev: Vec<f32>,
}

struct BridgeShared {
    instances: Mutex<HashMap<u64, BridgeInstance>>,
    alive: AtomicBool,
}

fn encode_line(msg: &HostMessage) -> Option<Vec<u8>> {
    let mut line = serde_json::to_vec(msg).ok()?;
    line.push(b'\n');
    Some(line)
}

pub struct UiBridgeClient {
    child: Child,
    stdin_tx: Option<SyncSender<Vec<u8>>>,
    shared: Arc<BridgeShared>,
    pump_stop: Arc<AtomicBool>,
    pump_thread: Option<std::thread::JoinHandle<()>>,
    shut_down: bool,
}

impl UiBridgeClient {
    pub fn spawn(event_tx: Sender<PwEvent>, cmd_tx: Sender<PwCommand>) -> Result<Self, String> {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()));
        let candidates = [
            exe_dir.as_ref().map(|d| d.join("zestbay-ui-bridge")),
            exe_dir
                .as_ref()
                .map(|d| d.join("../lib/zestbay/zestbay-ui-bridge")),
            Some(std::path::PathBuf::from(
                "/usr/lib/zestbay/zestbay-ui-bridge",
            )),
            Some(std::path::PathBuf::from(
                "/usr/local/lib/zestbay/zestbay-ui-bridge",
            )),
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
            .stderr(Stdio::inherit())
            .env("__GLX_VENDOR_LIBRARY_NAME", "mesa")
            .spawn()
            .map_err(|e| format!("Failed to spawn UI bridge: {}", e))?;

        let stdout = child.stdout.take().unwrap();
        let stdin = child.stdin.take().unwrap();

        let shared = Arc::new(BridgeShared {
            instances: Mutex::new(HashMap::new()),
            alive: AtomicBool::new(true),
        });

        let (stdin_tx, stdin_rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(QUEUE_CAPACITY);
        std::thread::Builder::new()
            .name("ui-bridge-stdin".into())
            .spawn(move || {
                let mut stdin = stdin;
                for line in stdin_rx {
                    if stdin.write_all(&line).is_err() || stdin.flush().is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| format!("Failed to spawn stdin thread: {}", e))?;

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
                    let _ =
                        event_tx.send(PwEvent::Plugin(PluginEvent::PluginUiClosed { instance_id }));
                }
                log::info!("UI bridge stdout reader exited");
            })
            .map_err(|e| format!("Failed to spawn stdout thread: {}", e))?;

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

    fn send(&mut self, msg: &HostMessage) -> bool {
        match (&self.stdin_tx, encode_line(msg)) {
            (Some(tx), Some(line)) => match tx.try_send(line) {
                Ok(()) => true,
                Err(error) => {
                    log::warn!("UI bridge transport unavailable: {error}");
                    self.shared.alive.store(false, Ordering::Release);
                    let _ = self.child.kill();
                    false
                }
            },
            _ => false,
        }
    }

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
            log::info!("UI bridge: UI for instance {} is already open", instance_id);
            return true;
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
            lv2_handle: 0,
            sample_rate,
        };

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
