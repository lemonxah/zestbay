#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(bool, patchbay_enabled)]
        #[qproperty(i32, active_plugin_count)]
        #[qproperty(i32, node_count)]
        #[qproperty(i32, link_count)]
        #[qproperty(QString, cpu_usage)]
        type AppController = super::AppControllerRust;

        #[qinvokable]
        fn init(self: Pin<&mut Self>);

        #[qinvokable]
        fn poll_events(self: Pin<&mut Self>);

        #[qinvokable]
        fn request_quit(self: Pin<&mut Self>);

        #[qinvokable]
        fn get_nodes_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn get_links_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn get_ports_json(self: Pin<&mut Self>, node_id: u32) -> QString;

        #[qinvokable]
        fn connect_ports(self: Pin<&mut Self>, output_port_id: u32, input_port_id: u32);

        #[qinvokable]
        fn disconnect_link(self: Pin<&mut Self>, link_id: u32);

        #[qinvokable]
        fn insert_node_on_link(self: Pin<&mut Self>, link_id: u32, node_id: u32);

        #[qinvokable]
        fn get_layout_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn save_layout(self: Pin<&mut Self>, json: QString);

        #[qinvokable]
        fn get_hidden_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn save_hidden(self: Pin<&mut Self>, json: QString);

        #[qinvokable]
        fn get_available_plugins_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn add_plugin(self: Pin<&mut Self>, uri: QString) -> QString;

        #[qinvokable]
        fn remove_plugin(self: Pin<&mut Self>, node_id: u32);

        #[qinvokable]
        fn open_plugin_ui(self: Pin<&mut Self>, node_id: u32);

        #[qinvokable]
        fn rename_plugin(self: Pin<&mut Self>, node_id: u32, new_name: QString);

        #[qinvokable]
        fn get_plugin_params_json(self: Pin<&mut Self>, node_id: u32) -> QString;

        #[qinvokable]
        fn set_plugin_parameter(self: Pin<&mut Self>, node_id: u32, port_index: u32, value: f32);

        #[qinvokable]
        fn set_plugin_bypass(self: Pin<&mut Self>, node_id: u32, bypassed: bool);

        #[qinvokable]
        fn get_active_plugins_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn remove_plugin_by_stable_id(self: Pin<&mut Self>, stable_id: QString);

        #[qinvokable]
        fn reset_plugin_params_by_stable_id(self: Pin<&mut Self>, stable_id: QString);

        #[qinvokable]
        fn set_plugin_param_by_stable_id(
            self: Pin<&mut Self>,
            stable_id: QString,
            port_index: u32,
            value: f32,
        );

        #[qinvokable]
        fn get_rules_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn toggle_rule(self: Pin<&mut Self>, rule_id: QString);

        #[qinvokable]
        fn remove_rule(self: Pin<&mut Self>, rule_id: QString);

        #[qinvokable]
        fn apply_rules(self: Pin<&mut Self>);

        #[qinvokable]
        fn snapshot_rules(self: Pin<&mut Self>);

        #[qinvokable]
        fn toggle_patchbay(self: Pin<&mut Self>, enabled: bool);

        #[qinvokable]
        fn get_node_names_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn add_rule(
            self: Pin<&mut Self>,
            source_pattern: QString,
            source_type: QString,
            target_pattern: QString,
            target_type: QString,
        );

        #[qinvokable]
        fn get_window_geometry_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn save_window_geometry(self: Pin<&mut Self>, json: QString);

        #[qinvokable]
        fn get_viewport_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn save_viewport(self: Pin<&mut Self>, json: QString);

        #[qinvokable]
        fn get_preferences_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn set_preference(self: Pin<&mut Self>, key: QString, value: QString);

        #[qinvokable]
        fn reset_preferences(self: Pin<&mut Self>);

        #[qinvokable]
        fn get_poll_interval_ms(self: Pin<&mut Self>) -> i32;

        #[qinvokable]
        fn set_window_visible(self: Pin<&mut Self>, visible: bool);

        #[qinvokable]
        fn get_cpu_history(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn get_plugin_cpu_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn get_default_node(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn set_default_node(self: Pin<&mut Self>, layout_key: QString);
    }

    unsafe extern "RustQt" {
        #[qsignal]
        fn graph_changed(self: Pin<&mut AppController>);

        #[qsignal]
        fn error_occurred(self: Pin<&mut AppController>, message: QString);

        #[qsignal]
        fn show_window_requested(self: Pin<&mut AppController>);

        #[qsignal]
        fn hide_window_requested(self: Pin<&mut AppController>);
    }
}

use core::pin::Pin;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use std::path::PathBuf;

use crate::plugin::PluginManager;
use crate::patchbay::{PatchbayManager, rules};
use crate::pipewire::{GraphState, PluginEvent, Node, NodeType, Port, PortDirection, PwCommand, PwEvent};
use crate::tray::TrayState;

/// Tracks the mapping between virtual sub-node IDs (used in the UI for split
/// bridge nodes) and the real PipeWire node ID + port group.
#[derive(Debug, Default)]
struct BridgeSplitState {
    /// virtual_node_id -> (real_node_id, port_group)
    virtual_to_real: HashMap<u32, (u32, String)>,
    /// (real_node_id, port_group) -> virtual_node_id
    real_to_virtual: HashMap<(u32, String), u32>,
    /// port_id -> virtual_node_id (for link rewriting)
    port_to_virtual: HashMap<u32, u32>,
    /// Next virtual node ID to allocate (starts high to avoid collisions with PipeWire IDs)
    next_virtual_id: u32,
}

impl BridgeSplitState {
    /// Virtual IDs start at 1,000,000 — well above any real PipeWire object ID
    /// (which are typically < 1000) but safely within QML's signed 32-bit int
    /// range (max 2,147,483,647).
    const VIRTUAL_ID_BASE: u32 = 1_000_000;

    fn new() -> Self {
        Self {
            next_virtual_id: Self::VIRTUAL_ID_BASE,
            ..Default::default()
        }
    }

    fn clear(&mut self) {
        self.virtual_to_real.clear();
        self.real_to_virtual.clear();
        self.port_to_virtual.clear();
        self.next_virtual_id = Self::VIRTUAL_ID_BASE;
    }

    fn get_or_create_virtual_id(&mut self, real_node_id: u32, group: &str) -> u32 {
        let key = (real_node_id, group.to_string());
        if let Some(&vid) = self.real_to_virtual.get(&key) {
            return vid;
        }
        let vid = self.next_virtual_id;
        self.next_virtual_id += 1;
        self.virtual_to_real
            .insert(vid, (real_node_id, group.to_string()));
        self.real_to_virtual.insert(key, vid);
        vid
    }

    fn register_port(&mut self, port_id: u32, virtual_node_id: u32) {
        self.port_to_virtual.insert(port_id, virtual_node_id);
    }

    fn resolve_virtual_node(&self, virtual_id: u32) -> Option<&(u32, String)> {
        self.virtual_to_real.get(&virtual_id)
    }

    fn resolve_port_virtual_node(&self, port_id: u32) -> Option<u32> {
        self.port_to_virtual.get(&port_id).copied()
    }

    fn is_virtual_id(&self, id: u32) -> bool {
        id >= Self::VIRTUAL_ID_BASE
    }
}

pub struct AppControllerRust {
    patchbay_enabled: bool,
    active_plugin_count: i32,
    node_count: i32,
    link_count: i32,
    cpu_usage: QString,

    graph: Option<Arc<GraphState>>,
    event_rx: Option<Receiver<PwEvent>>,
    cmd_tx: Option<Sender<PwCommand>>,
    patchbay: Option<PatchbayManager>,
    plugin_manager: Option<PluginManager>,
    last_change_counter: u64,

    next_instance_id: u64,

    cached_nodes: Vec<Node>,

    last_change_time: Option<std::time::Instant>,
    rules_apply_pending: bool,
    rules_loaded: bool,

    params_dirty: bool,
    params_dirty_since: Option<std::time::Instant>,

    pending_restore_count: usize,
    pending_links: Vec<SavedPluginLink>,

    links_dirty: bool,
    links_dirty_since: Option<std::time::Instant>,

    prefs: Preferences,

    tray_state: Option<TrayState>,

    prev_cpu_ticks: u64,
    prev_cpu_time: Option<Instant>,
    cpu_avg: f64,
    cpu_history: Vec<f64>,

    bridge_split: BridgeSplitState,
}

impl Default for AppControllerRust {
    fn default() -> Self {
        Self {
            patchbay_enabled: true,
            active_plugin_count: 0,
            node_count: 0,
            link_count: 0,
            graph: None,
            event_rx: None,
            cmd_tx: None,
            patchbay: None,
            plugin_manager: None,
            last_change_counter: 0,
            next_instance_id: 1,
            cached_nodes: Vec::new(),
            last_change_time: None,
            rules_apply_pending: false,
            rules_loaded: false,
            params_dirty: false,
            params_dirty_since: None,
            pending_restore_count: 0,
            pending_links: Vec::new(),
            links_dirty: false,
            links_dirty_since: None,
            prefs: load_preferences(),
            tray_state: None,
            cpu_usage: QString::from("0.0%"),
            prev_cpu_ticks: 0,
            prev_cpu_time: None,
            cpu_avg: 0.0,
            cpu_history: vec![0.0; 120],
            bridge_split: BridgeSplitState::new(),
        }
    }
}

impl qobject::AppController {
    pub fn init(mut self: Pin<&mut Self>) {
        log::info!("AppController::init — starting PipeWire");

        let prefs = load_preferences();
        log::info!(
            "Preferences: rule_settle={}ms, params_persist={}ms, links_persist={}ms, poll={}ms, auto_learn={}, pw_tick={}ms, pw_cooldown={}ms",
            prefs.rule_settle_ms,
            prefs.params_persist_ms,
            prefs.links_persist_ms,
            prefs.poll_interval_ms,
            prefs.auto_learn_rules,
            prefs.pw_tick_interval_ms,
            prefs.pw_operation_cooldown_ms,
        );
        self.as_mut().rust_mut().prefs = prefs;

        let graph = GraphState::new();

        // Scan all plugin formats and populate the unified plugin manager
        let lv2_scanner = crate::lv2::Lv2Manager::new();
        let mut plugin_manager = PluginManager::new();
        plugin_manager.extend_available_plugins(lv2_scanner.available_plugins().to_vec());

        let clap_plugins = crate::clap::scanner::scan_plugins();
        plugin_manager.extend_available_plugins(clap_plugins);

        let vst3_plugins = crate::vst3::scanner::scan_plugins();
        plugin_manager.extend_available_plugins(vst3_plugins);

        plugin_manager.sort_catalog();

        let (event_rx, cmd_tx) = crate::pipewire::start(
            graph.clone(),
            self.rust().prefs.pw_tick_interval_ms,
            self.rust().prefs.pw_operation_cooldown_ms,
        );

        let patchbay = PatchbayManager::new(graph.clone());

        self.as_mut().rust_mut().graph = Some(graph);
        self.as_mut().rust_mut().event_rx = Some(event_rx);
        self.as_mut().rust_mut().cmd_tx = Some(cmd_tx);
        self.as_mut().rust_mut().patchbay = Some(patchbay);
        self.as_mut().rust_mut().plugin_manager = Some(plugin_manager);

        let saved_links = load_saved_links();
        if !saved_links.is_empty() {
            log::info!(
                "Loaded {} saved LV2 links for restoration",
                saved_links.len()
            );
            self.as_mut().rust_mut().pending_links = saved_links;
        }

        let saved = load_saved_plugins();
        if !saved.is_empty() {
            log::info!("Restoring {} saved plugins", saved.len());
            self.as_mut().rust_mut().pending_restore_count = saved.len();
            for sp in saved {
                let instance_id = self.rust().next_instance_id;
                self.as_mut().rust_mut().next_instance_id += 1;

                let restored_params: Vec<crate::lv2::Lv2ParameterValue> = if let Some(ref mgr) =
                    self.rust().plugin_manager
                {
                    if let Some(plugin_info) = mgr.find_plugin(&sp.uri) {
                        plugin_info
                            .ports
                            .iter()
                            .filter(|port| port.port_type == crate::lv2::Lv2PortType::ControlInput)
                            .map(|port| {
                                let saved_value = sp.parameters.iter().find(|s| {
                                    s.port_index == port.index
                                        || (!s.symbol.is_empty() && s.symbol == port.symbol)
                                });
                                crate::lv2::Lv2ParameterValue {
                                    port_index: port.index,
                                    symbol: port.symbol.clone(),
                                    name: port.name.clone(),
                                    value: saved_value
                                        .map(|s| s.value)
                                        .unwrap_or(port.default_value),
                                    min: port.min_value,
                                    max: port.max_value,
                                    default: port.default_value,
                                }
                            })
                            .collect()
                    } else {
                        sp.parameters
                            .iter()
                            .map(|p| crate::lv2::Lv2ParameterValue {
                                port_index: p.port_index,
                                symbol: p.symbol.clone(),
                                name: String::new(),
                                value: p.value,
                                min: 0.0,
                                max: 1.0,
                                default: 0.0,
                            })
                            .collect()
                    }
                } else {
                    Vec::new()
                };

                let sid = if sp.stable_id.is_empty() {
                    uuid::Uuid::new_v4().to_string()
                } else {
                    sp.stable_id.clone()
                };

                let plugin_format = match sp.format.as_str() {
                    "CLAP" => crate::plugin::PluginFormat::Clap,
                    "VST3" => crate::plugin::PluginFormat::Vst3,
                    _ => crate::plugin::PluginFormat::Lv2,
                };

                if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
                    let info = crate::lv2::Lv2InstanceInfo {
                        id: instance_id,
                        stable_id: sid,
                        plugin_uri: sp.uri.clone(),
                        format: plugin_format,
                        display_name: sp.display_name.clone(),
                        pw_node_id: None,
                        parameters: restored_params,
                        active: true,
                        bypassed: sp.bypassed,
                    };
                    mgr.register_instance(info);
                }

                let format_str = sp.format.clone();
                if let Some(ref tx) = self.rust().cmd_tx {
                    log::info!("Restoring plugin: {} ({}) [{}]", sp.display_name, sp.uri, format_str);
                    let _ = tx.send(PwCommand::AddPlugin {
                        plugin_uri: sp.uri,
                        instance_id,
                        display_name: sp.display_name,
                        format: format_str,
                    });
                }
            }
        }

        let tray_state = crate::tray::spawn_tray();
        if self.rust().prefs.start_minimized {
            tray_state
                .window_visible
                .store(false, std::sync::atomic::Ordering::Release);
        }
        self.as_mut().rust_mut().tray_state = Some(tray_state);

        log::info!("AppController initialized successfully");
    }

    pub fn poll_events(mut self: Pin<&mut Self>) {
        if !self.rust().rules_loaded {
            self.as_mut().rust_mut().rules_loaded = true;
            let rules = load_rules();
            if !rules.is_empty() {
                log::info!("Loaded {} patchbay rules", rules.len());
                if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
                    patchbay.set_rules(rules);
                    patchbay.rules_dirty = false;
                }
            }

            // Load default node setting
            let default_node_path = config_path("default_node.txt");
            if let Ok(key) = std::fs::read_to_string(&default_node_path) {
                let key = key.trim().to_string();
                if !key.is_empty() {
                    let display_name = if let Some(pos) = key.find(':') {
                        key[pos + 1..].to_string()
                    } else {
                        key.clone()
                    };
                    log::info!("Loaded default node: {}", display_name);
                    if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
                        patchbay.set_default_target(Some(display_name));
                    }
                }
            }
        }

        let mut changed = false;
        let mut link_changed = false;
        let mut error_msg: Option<String> = None;
        let mut plugin_events: Vec<PluginEvent> = Vec::new();

        let has_events = self.rust().event_rx.is_some();
        if has_events {
            let rx = self.as_mut().rust_mut().event_rx.take();
            if let Some(rx) = rx {
                while let Ok(event) = rx.try_recv() {
                    match event {
                        PwEvent::NodeChanged(_)
                        | PwEvent::NodeRemoved(_)
                        | PwEvent::PortChanged(_)
                        | PwEvent::PortRemoved { .. }
                        | PwEvent::BatchComplete => {
                            changed = true;
                        }
                        PwEvent::LinkChanged(_) | PwEvent::LinkRemoved(_) => {
                            changed = true;
                            link_changed = true;
                        }
                        PwEvent::Error(msg) => {
                            log::error!("PipeWire error: {}", msg);
                            error_msg = Some(msg);
                        }
                        PwEvent::Plugin(plugin_event) => {
                            changed = true;
                            plugin_events.push(plugin_event);
                        }
                    }
                }
                self.as_mut().rust_mut().event_rx = Some(rx);
            }
        }

        for event in plugin_events {
            match event {
                PluginEvent::PluginAdded {
                    instance_id,
                    pw_node_id,
                    display_name,
                } => {
                    log::info!(
                        "LV2 plugin added: instance={} pw_node={} name={}",
                        instance_id,
                        pw_node_id,
                        display_name
                    );
                    if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
                        mgr.set_instance_pw_node_id(instance_id, pw_node_id);
                    }
                    if pw_node_id != 0
                        && pw_node_id != u32::MAX
                        && let Some(ref graph) = self.rust().graph
                    {
                        graph.set_node_type(pw_node_id, NodeType::Plugin);
                    }

                    if let Some(ref mgr) = self.rust().plugin_manager
                        && let Some(info) = mgr.get_instance(instance_id)
                        && (!info.parameters.is_empty() || info.bypassed)
                        && let Some(ref tx) = self.rust().cmd_tx
                    {
                        for param in &info.parameters {
                            let _ = tx.send(PwCommand::SetPluginParameter {
                                instance_id,
                                port_index: param.port_index,
                                value: param.value,
                            });
                        }
                        if info.bypassed {
                            let _ = tx.send(PwCommand::SetPluginBypass {
                                instance_id,
                                bypassed: true,
                            });
                        }
                        log::info!(
                            "Restored {} params + bypass={} for instance {}",
                            info.parameters.len(),
                            info.bypassed,
                            instance_id
                        );
                    }

                    if self.rust().pending_restore_count > 0 {
                        let count = self.rust().pending_restore_count - 1;
                        self.as_mut().rust_mut().pending_restore_count = count;
                        if count == 0 {
                            log::info!("All plugins restored — will attempt link restoration");
                        }
                    }
                }
                PluginEvent::PluginRemoved { instance_id } => {
                    log::info!("LV2 plugin removed: instance={}", instance_id);
                    if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
                        mgr.remove_instance(instance_id);
                    }
                    persist_active_plugins(self.rust().plugin_manager.as_ref());
                    self.as_mut().rust_mut().links_dirty = true;
                    if self.rust().links_dirty_since.is_none() {
                        self.as_mut().rust_mut().links_dirty_since = Some(Instant::now());
                    }
                }
                PluginEvent::ParameterChanged {
                    instance_id,
                    port_index,
                    value,
                } => {
                    if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
                        mgr.update_parameter(instance_id, port_index, value);
                    }
                    self.as_mut().rust_mut().params_dirty = true;
                    if self.rust().params_dirty_since.is_none() {
                        self.as_mut().rust_mut().params_dirty_since = Some(Instant::now());
                    }
                }
                PluginEvent::PluginUiOpened { instance_id } => {
                    log::info!("LV2 plugin UI opened: instance={}", instance_id);
                }
                PluginEvent::PluginUiClosed { instance_id } => {
                    log::info!("LV2 plugin UI closed: instance={}", instance_id);
                }
                PluginEvent::PluginError {
                    instance_id,
                    message,
                    fatal,
                } => {
                    log::error!(
                        "LV2 plugin error: instance={:?} fatal={} msg={}",
                        instance_id,
                        fatal,
                        message
                    );

                    if let Some(id) = instance_id {
                        let plugin_name = self
                            .rust()
                            .plugin_manager
                            .as_ref()
                            .and_then(|mgr| mgr.get_instance(id))
                            .map(|info| info.display_name.clone());

                        if fatal {
                            if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
                                mgr.remove_instance(id);
                            }
                            persist_active_plugins(self.rust().plugin_manager.as_ref());

                            if self.rust().pending_restore_count > 0 {
                                let count = self.rust().pending_restore_count - 1;
                                self.as_mut().rust_mut().pending_restore_count = count;
                            }
                        }

                        if let Some(name) = plugin_name {
                            error_msg =
                                Some(format!("Plugin \"{}\" failed to load: {}", name, message));
                        } else {
                            error_msg = Some(format!("Plugin failed to load: {}", message));
                        }
                    } else {
                        error_msg = Some(message);
                    }
                }
            }
        }

        if let Some(ref graph) = self.rust().graph {
            let current = graph.change_counter();
            let last = self.rust().last_change_counter;
            if current != last {
                changed = true;
                self.as_mut().rust_mut().last_change_counter = current;
            }
        }

        if changed {
            self.as_mut().rust_mut().last_change_time = Some(Instant::now());
            self.as_mut().rust_mut().rules_apply_pending = true;
        }

        if link_changed
            && self.rust().pending_restore_count == 0
            && self.rust().pending_links.is_empty()
        {
            self.as_mut().rust_mut().links_dirty = true;
            if self.rust().links_dirty_since.is_none() {
                self.as_mut().rust_mut().links_dirty_since = Some(Instant::now());
            }
        }

        let rule_settle_ms = self.rust().prefs.rule_settle_ms;
        let should_apply = {
            let pending = self.rust().rules_apply_pending;
            let patchbay_enabled = self
                .rust()
                .patchbay
                .as_ref()
                .map(|p| p.enabled)
                .unwrap_or(false);
            let settled = self
                .rust()
                .last_change_time
                .map(|t| t.elapsed() >= Duration::from_millis(rule_settle_ms))
                .unwrap_or(false);
            pending && patchbay_enabled && settled
        };

        if should_apply {
            self.as_mut().rust_mut().rules_apply_pending = false;
            let commands = if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
                patchbay.scan()
            } else {
                Vec::new()
            };
            if !commands.is_empty() {
                log::info!("Auto-applying {} patchbay rule commands", commands.len());
                if let Some(ref tx) = self.rust().cmd_tx {
                    for cmd in commands {
                        let _ = tx.send(cmd);
                    }
                }
            }
            if self
                .rust()
                .patchbay
                .as_ref()
                .map(|p| p.rules_dirty)
                .unwrap_or(false)
            {
                if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
                    patchbay.rules_dirty = false;
                }
                save_rules(self.rust().patchbay.as_ref());
            }
        }

        let params_persist_ms = self.rust().prefs.params_persist_ms;
        let should_persist_params = {
            self.rust().params_dirty
                && self
                    .rust()
                    .params_dirty_since
                    .map(|t| t.elapsed() >= Duration::from_millis(params_persist_ms))
                    .unwrap_or(false)
        };
        if should_persist_params {
            self.as_mut().rust_mut().params_dirty = false;
            self.as_mut().rust_mut().params_dirty_since = None;
            persist_active_plugins(self.rust().plugin_manager.as_ref());
        }

        let should_restore_links = {
            self.rust().pending_restore_count == 0
                && !self.rust().pending_links.is_empty()
                && self
                    .rust()
                    .last_change_time
                    .map(|t| t.elapsed() >= Duration::from_millis(rule_settle_ms))
                    .unwrap_or(false)
        };
        if should_restore_links {
            let links = std::mem::take(&mut self.as_mut().rust_mut().pending_links);
            log::info!("Attempting to restore {} saved LV2 links", links.len());
            if let Some(ref graph) = self.rust().graph {
                for saved_link in &links {
                    let all_nodes = graph.get_all_nodes();

                    let mut out_port_id = None;
                    // First try matching by node display name
                    for n in all_nodes
                        .iter()
                        .filter(|n| n.display_name() == saved_link.output_node_name)
                    {
                        let ports = graph.get_ports_for_node(n.id);
                        if let Some(p) = ports.iter().find(|p| {
                            p.name == saved_link.output_port_name
                                && p.direction == PortDirection::Output
                        }) {
                            out_port_id = Some(p.id);
                            break;
                        }
                    }
                    // If not found, check bridge sub-nodes by device name (from port.alias)
                    if out_port_id.is_none() {
                        for n in all_nodes.iter().filter(|n| n.is_bridge) {
                            let groups = graph.get_bridge_port_groups(n.id);
                            for (group, device_name) in &groups {
                                if *device_name == saved_link.output_node_name {
                                    let ports = graph.get_ports_for_bridge_group(n.id, group);
                                    if let Some(p) = ports.iter().find(|p| {
                                        p.name == saved_link.output_port_name
                                            && p.direction == PortDirection::Output
                                    }) {
                                        out_port_id = Some(p.id);
                                        break;
                                    }
                                }
                            }
                            if out_port_id.is_some() { break; }
                        }
                    }

                    let mut in_port_id = None;
                    for n in all_nodes
                        .iter()
                        .filter(|n| n.display_name() == saved_link.input_node_name)
                    {
                        let ports = graph.get_ports_for_node(n.id);
                        if let Some(p) = ports.iter().find(|p| {
                            p.name == saved_link.input_port_name
                                && p.direction == PortDirection::Input
                        }) {
                            in_port_id = Some(p.id);
                            break;
                        }
                    }
                    // If not found, check bridge sub-nodes by device name
                    if in_port_id.is_none() {
                        for n in all_nodes.iter().filter(|n| n.is_bridge) {
                            let groups = graph.get_bridge_port_groups(n.id);
                            for (group, device_name) in &groups {
                                if *device_name == saved_link.input_node_name {
                                    let ports = graph.get_ports_for_bridge_group(n.id, group);
                                    if let Some(p) = ports.iter().find(|p| {
                                        p.name == saved_link.input_port_name
                                            && p.direction == PortDirection::Input
                                    }) {
                                        in_port_id = Some(p.id);
                                        break;
                                    }
                                }
                            }
                            if in_port_id.is_some() { break; }
                        }
                    }

                    if let (Some(out_id), Some(in_id)) = (out_port_id, in_port_id) {
                        log::info!(
                            "Restoring link: {}:{} -> {}:{}",
                            saved_link.output_node_name,
                            saved_link.output_port_name,
                            saved_link.input_node_name,
                            saved_link.input_port_name
                        );
                        if let Some(ref tx) = self.rust().cmd_tx {
                            let _ = tx.send(PwCommand::Connect {
                                output_port_id: out_id,
                                input_port_id: in_id,
                            });
                        }
                    } else {
                        log::warn!(
                            "Could not find ports for saved link: {}:{} -> {}:{}",
                            saved_link.output_node_name,
                            saved_link.output_port_name,
                            saved_link.input_node_name,
                            saved_link.input_port_name
                        );
                    }
                }
            }
        }

        let links_persist_ms = self.rust().prefs.links_persist_ms;
        let should_persist_links = {
            self.rust().links_dirty
                && self
                    .rust()
                    .links_dirty_since
                    .map(|t| t.elapsed() >= Duration::from_millis(links_persist_ms))
                    .unwrap_or(false)
        };
        if should_persist_links {
            self.as_mut().rust_mut().links_dirty = false;
            self.as_mut().rust_mut().links_dirty_since = None;
            persist_lv2_links(self.rust().graph.as_ref());
        }

        let tray_state = self.rust().tray_state.clone();
        if let Some(ref tray) = tray_state {
            use std::sync::atomic::Ordering;
            if tray.quit_requested.load(Ordering::Acquire) {
                self.as_mut().request_quit();
                return;
            }
            if tray.show_requested.swap(false, Ordering::AcqRel) {
                log::info!("Tray: show window requested — emitting signal to QML");
                tray.window_visible.store(true, Ordering::Release);
                self.as_mut().show_window_requested();
            }
            if tray.hide_requested.swap(false, Ordering::AcqRel) {
                log::info!("Tray: hide window requested — emitting signal to QML");
                tray.window_visible.store(false, Ordering::Release);
                self.as_mut().hide_window_requested();
            }
        }

        if let Some(msg) = error_msg {
            let qmsg = QString::from(&msg);
            self.as_mut().error_occurred(qmsg);
        }

        if changed {
            self.as_mut().refresh_cache();
            self.as_mut().graph_changed();
        }

        let mut prev_ticks = self.rust().prev_cpu_ticks;
        let mut prev_time = self.rust().prev_cpu_time;
        let mut avg = self.rust().cpu_avg;
        let cpu_str = measure_cpu_usage(&mut prev_ticks, &mut prev_time, &mut avg);
        self.as_mut().rust_mut().prev_cpu_ticks = prev_ticks;
        self.as_mut().rust_mut().prev_cpu_time = prev_time;
        self.as_mut().rust_mut().cpu_avg = avg;
        {
            let h = &mut self.as_mut().rust_mut().cpu_history;
            if h.len() >= 120 {
                h.remove(0);
            }
            h.push(avg);
        }
        self.as_mut().set_cpu_usage(QString::from(&cpu_str));
    }

    fn refresh_cache(mut self: Pin<&mut Self>) {
        let (node_count, link_count, nodes) = {
            if let Some(ref graph) = self.rust().graph {
                let nodes = graph.get_all_nodes();
                let links = graph.get_all_links();
                (nodes.len() as i32, links.len() as i32, nodes)
            } else {
                (0, 0, Vec::new())
            }
        };

        self.as_mut().set_node_count(node_count);
        self.as_mut().set_link_count(link_count);
        self.as_mut().rust_mut().cached_nodes = nodes;
    }

    pub fn get_nodes_json(mut self: Pin<&mut Self>) -> QString {
        if let Some(graph) = self.rust().graph.clone() {
            let nodes = graph.get_all_nodes();
            log::debug!(
                "get_nodes_json: {} nodes ({} ready)",
                nodes.len(),
                nodes.iter().filter(|n| n.ready).count()
            );

            // Rebuild bridge split state each refresh
            self.as_mut().rust_mut().bridge_split.clear();

            let mut json_nodes: Vec<serde_json::Value> = Vec::new();

            for n in nodes.iter().filter(|n| n.ready) {
                let media_str = match n.media_type {
                    Some(crate::pipewire::MediaType::Audio) => "Audio",
                    Some(crate::pipewire::MediaType::Video) => "Video",
                    Some(crate::pipewire::MediaType::Midi) => "Midi",
                    None => "Unknown",
                };

                // Split bridge nodes into per-device sub-nodes
                if n.is_bridge {
                    let groups = graph.get_bridge_port_groups(n.id);
                    if groups.is_empty() {
                        // No ports with groups yet — show the bridge as-is
                        let mgr = self.rust().plugin_manager.as_ref();
                        json_nodes.push(node_to_json(n, mgr));
                    } else {
                        for (group, device_name) in &groups {
                            let vid = self.as_mut().rust_mut().bridge_split
                                .get_or_create_virtual_id(n.id, group);

                            // Register all ports in this group for link rewriting
                            let group_ports = graph.get_ports_for_bridge_group(n.id, group);
                            for port in &group_ports {
                                self.as_mut().rust_mut().bridge_split
                                    .register_port(port.id, vid);
                            }

                            // Determine sub-node type based on port directions
                            let has_inputs = group_ports.iter().any(|p| p.direction == PortDirection::Input);
                            let has_outputs = group_ports.iter().any(|p| p.direction == PortDirection::Output);
                            let type_str = if has_inputs && has_outputs {
                                "Duplex"
                            } else if has_outputs {
                                "Source"
                            } else if has_inputs {
                                "Sink"
                            } else {
                                "Duplex"
                            };

                            json_nodes.push(serde_json::json!({
                                "id": vid,
                                "name": device_name,
                                "type": type_str,
                                "mediaType": media_str,
                                "isVirtual": n.is_virtual,
                                "isJack": n.is_jack,
                                "layoutKey": format!("MidiBridge:{}", device_name),
                                "ready": true,
                            }));
                        }
                    }
                } else {
                    let mgr = self.rust().plugin_manager.as_ref();
                    json_nodes.push(node_to_json(n, mgr));
                }
            }

            let json = serde_json::to_string(&json_nodes).unwrap_or_default();
            QString::from(&json)
        } else {
            QString::from("[]")
        }
    }

    pub fn get_links_json(self: Pin<&mut Self>) -> QString {
        if let Some(ref graph) = self.rust().graph {
            let links = graph.get_all_links();
            let json_links: Vec<serde_json::Value> = links
                .iter()
                .map(|l| {
                    // Rewrite node IDs for ports belonging to bridge sub-nodes
                    let out_node = self.rust().bridge_split
                        .resolve_port_virtual_node(l.output_port_id)
                        .unwrap_or(l.output_node_id);
                    let in_node = self.rust().bridge_split
                        .resolve_port_virtual_node(l.input_port_id)
                        .unwrap_or(l.input_node_id);
                    serde_json::json!({
                        "id": l.id,
                        "outputNodeId": out_node,
                        "outputPortId": l.output_port_id,
                        "inputNodeId": in_node,
                        "inputPortId": l.input_port_id,
                        "active": l.active,
                    })
                })
                .collect();
            let json = serde_json::to_string(&json_links).unwrap_or_default();
            QString::from(&json)
        } else {
            QString::from("[]")
        }
    }

    pub fn get_ports_json(self: Pin<&mut Self>, node_id: u32) -> QString {
        log::debug!("get_ports_json: node_id={}", node_id);
        if let Some(ref graph) = self.rust().graph {
            // Check if this is a virtual bridge sub-node ID
            let ports = if let Some((real_node_id, group)) =
                self.rust().bridge_split.resolve_virtual_node(node_id).cloned()
            {
                graph.get_ports_for_bridge_group(real_node_id, &group)
            } else {
                graph.get_ports_for_node(node_id)
            };

            let json_ports: Vec<serde_json::Value> = ports
                .iter()
                .map(|p| {
                    let media_str = match p.media_type {
                        Some(crate::pipewire::MediaType::Audio) => "Audio",
                        Some(crate::pipewire::MediaType::Video) => "Video",
                        Some(crate::pipewire::MediaType::Midi) => "Midi",
                        None => "Unknown",
                    };
                    // For bridge sub-node ports, use a cleaner display name
                    // from port.alias (the part after the colon) or fall back to default
                    let display_name = if self.rust().bridge_split.is_virtual_id(node_id) {
                        if let Some(ref alias) = p.port_alias {
                            if let Some(colon_pos) = alias.find(':') {
                                alias[colon_pos + 1..].trim().to_string()
                            } else {
                                p.display_name().to_string()
                            }
                        } else {
                            p.display_name().to_string()
                        }
                    } else {
                        p.display_name().to_string()
                    };
                    serde_json::json!({
                        "id": p.id,
                        "name": display_name,
                        "direction": format!("{:?}", p.direction),
                        "nodeId": node_id,
                        "mediaType": media_str,
                    })
                })
                .collect();
            let json = serde_json::to_string(&json_ports).unwrap_or_default();
            QString::from(&json)
        } else {
            QString::from("[]")
        }
    }

    pub fn connect_ports(mut self: Pin<&mut Self>, output_port_id: u32, input_port_id: u32) {
        // Reject self-loops: don't connect a node's output to its own input
        // For bridge nodes, allow cross-device connections (different port groups)
        if let Some(ref graph) = self.rust().graph {
            let out_port = graph.get_port(output_port_id);
            let in_port = graph.get_port(input_port_id);
            if let (Some(op), Some(ip)) = (&out_port, &in_port) {
                if op.node_id == ip.node_id {
                    // Same PipeWire node — only reject if same port group (or no groups)
                    let same_group = match (&op.port_group, &ip.port_group) {
                        (Some(og), Some(ig)) => og == ig,
                        _ => true, // If either has no group, treat as same device
                    };
                    if same_group {
                        log::warn!(
                            "Rejected self-loop connect: ports {} and {} belong to the same node/device",
                            output_port_id, input_port_id
                        );
                        return;
                    }
                }
            }
        }

        if let Some(ref tx) = self.rust().cmd_tx {
            log::info!("Connect request: {} -> {}", output_port_id, input_port_id);
            let _ = tx.send(PwCommand::Connect {
                output_port_id,
                input_port_id,
            });
        }

        let learned = if !self.rust().prefs.auto_learn_rules {
            false
        } else {
            let graph = self.rust().graph.clone();
            if let Some(ref graph) = graph {
                if let (Some(out_port), Some(in_port)) = (
                    graph.get_port(output_port_id),
                    graph.get_port(input_port_id),
                ) {
                    if let (Some(source_node), Some(target_node)) = (
                        graph.get_node(out_port.node_id),
                        graph.get_node(in_port.node_id),
                    ) {
                        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
                            let changed = patchbay.learn_from_link(
                                &source_node,
                                &target_node,
                                &out_port,
                                &in_port,
                            );
                            if changed {
                                log::info!(
                                    "Auto-learned rule: {}:{} -> {}:{}",
                                    source_node.display_name(),
                                    out_port.name,
                                    target_node.display_name(),
                                    in_port.name,
                                );
                            }
                            changed
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        };

        if learned {
            save_rules(self.rust().patchbay.as_ref());
        }

        self.as_mut().rust_mut().links_dirty = true;
        if self.rust().links_dirty_since.is_none() {
            self.as_mut().rust_mut().links_dirty_since = Some(Instant::now());
        }
    }

    pub fn disconnect_link(mut self: Pin<&mut Self>, link_id: u32) {
        let link_info = self.rust().graph.as_ref().and_then(|g| g.get_link(link_id));

        if let Some(ref tx) = self.rust().cmd_tx {
            log::info!("Disconnect request: {}", link_id);
            let _ = tx.send(PwCommand::Disconnect { link_id });
        }

        if let Some(link) = link_info {
            let unlearned = {
                let graph = self.rust().graph.clone();
                if let Some(ref graph) = graph {
                    if let (Some(out_port), Some(in_port)) = (
                        graph.get_port(link.output_port_id),
                        graph.get_port(link.input_port_id),
                    ) {
                        if let (Some(source_node), Some(target_node)) = (
                            graph.get_node(link.output_node_id),
                            graph.get_node(link.input_node_id),
                        ) {
                            if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
                                let changed = patchbay.unlearn_from_link(
                                    &source_node,
                                    &target_node,
                                    &out_port,
                                    &in_port,
                                );
                                if changed {
                                    log::info!(
                                        "Unlearned rule: {}:{} -> {}:{}",
                                        source_node.display_name(),
                                        out_port.name,
                                        target_node.display_name(),
                                        in_port.name,
                                    );
                                }
                                changed
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            };

            if unlearned {
                save_rules(self.rust().patchbay.as_ref());
            }
        }

        self.as_mut().rust_mut().links_dirty = true;
        if self.rust().links_dirty_since.is_none() {
            self.as_mut().rust_mut().links_dirty_since = Some(Instant::now());
        }
    }

    pub fn insert_node_on_link(mut self: Pin<&mut Self>, link_id: u32, node_id: u32) {
        let graph = self.rust().graph.clone();
        let Some(ref graph) = graph else { return };

        let Some(link) = graph.get_link(link_id) else {
            log::warn!("insert_node_on_link: link {} not found", link_id);
            return;
        };

        let Some(node) = graph.get_node(node_id) else {
            log::warn!("insert_node_on_link: node {} not found", node_id);
            return;
        };

        if link.output_node_id == node_id || link.input_node_id == node_id {
            log::warn!("insert_node_on_link: node {} is already part of link {}, ignoring", node_id, link_id);
            return;
        }

        if node.node_type != Some(NodeType::Plugin) {
            log::warn!("insert_node_on_link: node {} is not an LV2 plugin, ignoring", node_id);
            return;
        }

        let node_ports = graph.get_ports_for_node(node_id);
        let mut node_inputs: Vec<_> = node_ports
            .iter()
            .filter(|p| p.direction == PortDirection::Input && p.media_type == Some(crate::pipewire::MediaType::Audio))
            .collect();
        let mut node_outputs: Vec<_> = node_ports
            .iter()
            .filter(|p| p.direction == PortDirection::Output && p.media_type == Some(crate::pipewire::MediaType::Audio))
            .collect();

        if node_inputs.is_empty() || node_outputs.is_empty() {
            log::warn!("insert_node_on_link: node {} has no audio input/output ports", node_id);
            return;
        }

        node_inputs.sort_by(|a, b| crate::pipewire::state::natural_cmp(&a.name, &b.name));
        node_outputs.sort_by(|a, b| crate::pipewire::state::natural_cmp(&a.name, &b.name));

        let upstream_out = link.output_port_id;
        let downstream_in = link.input_port_id;

        let upstream_node_id = link.output_node_id;
        let downstream_node_id = link.input_node_id;

        let upstream_ports: Vec<_> = graph
            .get_ports_for_node(upstream_node_id)
            .into_iter()
            .filter(|p| p.direction == PortDirection::Output && p.media_type == Some(crate::pipewire::MediaType::Audio))
            .collect();
        let downstream_ports: Vec<_> = graph
            .get_ports_for_node(downstream_node_id)
            .into_iter()
            .filter(|p| p.direction == PortDirection::Input && p.media_type == Some(crate::pipewire::MediaType::Audio))
            .collect();

        let upstream_idx = upstream_ports.iter().position(|p| p.id == upstream_out).unwrap_or(0);
        let downstream_idx = downstream_ports.iter().position(|p| p.id == downstream_in).unwrap_or(0);

        let all_links = graph.get_all_links();
        let mut links_to_remove = Vec::new();
        let mut rewire_pairs: Vec<(u32, usize, u32, usize)> = Vec::new();

        for existing in &all_links {
            if existing.output_node_id == upstream_node_id && existing.input_node_id == downstream_node_id {
                let u_idx = upstream_ports.iter().position(|p| p.id == existing.output_port_id);
                let d_idx = downstream_ports.iter().position(|p| p.id == existing.input_port_id);
                if let (Some(ui), Some(di)) = (u_idx, d_idx) {
                    links_to_remove.push(existing.id);
                    rewire_pairs.push((existing.output_port_id, ui, existing.input_port_id, di));
                }
            }
        }

        if links_to_remove.is_empty() {
            links_to_remove.push(link_id);
            rewire_pairs.push((upstream_out, upstream_idx, downstream_in, downstream_idx));
        }

        if let Some(ref tx) = self.rust().cmd_tx {
            for lid in &links_to_remove {
                let _ = tx.send(PwCommand::Disconnect { link_id: *lid });
            }

            let max_in = node_inputs.len() - 1;
            let max_out = node_outputs.len() - 1;
            for (up_port, up_idx, down_port, down_idx) in &rewire_pairs {
                let in_idx = *up_idx.min(&max_in);
                let out_idx = *down_idx.min(&max_out);

                let _ = tx.send(PwCommand::Connect {
                    output_port_id: *up_port,
                    input_port_id: node_inputs[in_idx].id,
                });
                let _ = tx.send(PwCommand::Connect {
                    output_port_id: node_outputs[out_idx].id,
                    input_port_id: *down_port,
                });
            }
        }

        log::info!(
            "insert_node_on_link: inserted node {} on {} links between nodes {} and {}",
            node_id,
            links_to_remove.len(),
            upstream_node_id,
            downstream_node_id
        );

        let mut rule_data: Vec<(Node, Node, Port, Port)> = Vec::new();
        let mut new_link_data: Vec<(Node, Node, Port, Port, Node, Node, Port, Port)> = Vec::new();

        {
            let max_in = node_inputs.len() - 1;
            let max_out = node_outputs.len() - 1;

            for (up_port_id, up_idx, down_port_id, down_idx) in &rewire_pairs {
                if let (Some(source_node), Some(target_node), Some(out_port), Some(in_port)) = (
                    graph.get_node(upstream_node_id),
                    graph.get_node(downstream_node_id),
                    graph.get_port(*up_port_id),
                    graph.get_port(*down_port_id),
                ) {
                    rule_data.push((source_node, target_node, out_port, in_port));
                }

                let in_idx = *up_idx.min(&max_in);
                let out_idx = *down_idx.min(&max_out);

                if let (Some(up_node), Some(ins_node), Some(up_port), Some(ins_in_port)) = (
                    graph.get_node(upstream_node_id),
                    graph.get_node(node_id),
                    graph.get_port(*up_port_id),
                    graph.get_port(node_inputs[in_idx].id),
                ) {
                    if let (Some(ins_node2), Some(dn_node), Some(ins_out_port), Some(dn_port)) = (
                        graph.get_node(node_id),
                        graph.get_node(downstream_node_id),
                        graph.get_port(node_outputs[out_idx].id),
                        graph.get_port(*down_port_id),
                    ) {
                        new_link_data.push((
                            up_node, ins_node, up_port, ins_in_port,
                            ins_node2, dn_node, ins_out_port, dn_port,
                        ));
                    }
                }
            }
        }

        let mut rules_changed = false;
        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            for (source_node, target_node, out_port, in_port) in &rule_data {
                if patchbay.unlearn_from_link(source_node, target_node, out_port, in_port) {
                    log::info!(
                        "insert_node_on_link: unlearned rule {}:{} -> {}:{}",
                        source_node.display_name(),
                        out_port.name,
                        target_node.display_name(),
                        in_port.name,
                    );
                    rules_changed = true;
                }
            }

            for (up_node, ins_node, up_port, ins_in_port, ins_node2, dn_node, ins_out_port, dn_port) in &new_link_data {
                if patchbay.learn_from_link(up_node, ins_node, up_port, ins_in_port) {
                    log::info!(
                        "insert_node_on_link: learned rule {}:{} -> {}:{}",
                        up_node.display_name(),
                        up_port.name,
                        ins_node.display_name(),
                        ins_in_port.name,
                    );
                    rules_changed = true;
                }
                if patchbay.learn_from_link(ins_node2, dn_node, ins_out_port, dn_port) {
                    log::info!(
                        "insert_node_on_link: learned rule {}:{} -> {}:{}",
                        ins_node2.display_name(),
                        ins_out_port.name,
                        dn_node.display_name(),
                        dn_port.name,
                    );
                    rules_changed = true;
                }
            }
        }

        if rules_changed {
            save_rules(self.rust().patchbay.as_ref());
        }

        self.as_mut().rust_mut().links_dirty = true;
        if self.rust().links_dirty_since.is_none() {
            self.as_mut().rust_mut().links_dirty_since = Some(Instant::now());
        }
    }

    pub fn request_quit(self: Pin<&mut Self>) {
        log::info!("Quit requested");
        persist_lv2_links(self.rust().graph.as_ref());
        persist_active_plugins(self.rust().plugin_manager.as_ref());
        crate::lv2::ui::shutdown_gtk_thread();
        std::process::exit(0);
    }

    pub fn get_layout_json(self: Pin<&mut Self>) -> QString {
        let path = config_path("layout.json");
        let json = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => "{}".to_string(),
        };
        log::debug!("get_layout_json: loaded from {:?}", path);
        QString::from(&json)
    }

    pub fn save_layout(self: Pin<&mut Self>, json: QString) {
        let path = config_path("layout.json");
        let s: String = json.to_string();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&path, &s) {
            log::error!("Failed to save layout to {:?}: {}", path, e);
        } else {
            log::debug!("save_layout: written to {:?}", path);
        }
    }

    pub fn get_hidden_json(self: Pin<&mut Self>) -> QString {
        let path = config_path("hidden.json");
        let json = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => "[]".to_string(),
        };
        log::debug!("get_hidden_json: loaded from {:?}", path);
        QString::from(&json)
    }

    pub fn save_hidden(self: Pin<&mut Self>, json: QString) {
        let path = config_path("hidden.json");
        let s: String = json.to_string();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&path, &s) {
            log::error!("Failed to save hidden to {:?}: {}", path, e);
        } else {
            log::debug!("save_hidden: written to {:?}", path);
        }
    }

    pub fn get_available_plugins_json(self: Pin<&mut Self>) -> QString {
        if let Some(ref mgr) = self.rust().plugin_manager {
            let json_plugins: Vec<serde_json::Value> = mgr
                .available_plugins()
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "uri": p.uri,
                        "name": p.name,
                        "category": p.category.display_name(),
                        "author": p.author.as_deref().unwrap_or(""),
                        "audioIn": p.audio_inputs,
                        "audioOut": p.audio_outputs,
                        "controlIn": p.control_inputs,
                        "controlOut": p.control_outputs,
                        "compatible": p.compatible,
                        "requiredFeatures": p.required_features,
                        "hasUi": p.has_ui,
                        "format": p.format.as_str(),
                    })
                })
                .collect();
            let json = serde_json::to_string(&json_plugins).unwrap_or_default();
            QString::from(&json)
        } else {
            QString::from("[]")
        }
    }

    pub fn add_plugin(mut self: Pin<&mut Self>, uri: QString) -> QString {
        let uri_str: String = uri.to_string();

        let (display_name, initial_params, plugin_format) = if let Some(ref mgr) =
            self.rust().plugin_manager
        {
            let plugin = mgr.find_plugin(&uri_str);
            let base_name = plugin
                .map(|p| p.name.clone())
                .unwrap_or_else(|| uri_str.clone());
            let name = self.unique_display_name(&base_name);
            let format = plugin
                .map(|p| p.format)
                .unwrap_or(crate::plugin::PluginFormat::Lv2);
            let params: Vec<crate::lv2::Lv2ParameterValue> = plugin
                .map(|p| {
                    p.ports
                        .iter()
                        .filter(|port| port.port_type == crate::lv2::Lv2PortType::ControlInput)
                        .map(|port| crate::lv2::Lv2ParameterValue {
                            port_index: port.index,
                            symbol: port.symbol.clone(),
                            name: port.name.clone(),
                            value: port.default_value,
                            min: port.min_value,
                            max: port.max_value,
                            default: port.default_value,
                        })
                        .collect()
                })
                .unwrap_or_default();
            (name, params, format)
        } else {
            return QString::from("");
        };

        let instance_id = self.rust().next_instance_id;
        self.as_mut().rust_mut().next_instance_id += 1;

        let format_str = plugin_format.as_str().to_string();

        if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
            let info = crate::lv2::Lv2InstanceInfo {
                id: instance_id,
                stable_id: uuid::Uuid::new_v4().to_string(),
                plugin_uri: uri_str.clone(),
                format: plugin_format,
                display_name: display_name.clone(),
                pw_node_id: None,
                parameters: initial_params,
                active: true,
                bypassed: false,
            };
            mgr.register_instance(info);
        }

        if let Some(ref tx) = self.rust().cmd_tx {
            log::info!(
                "Adding plugin: uri={} instance_id={} name={} format={}",
                uri_str,
                instance_id,
                display_name,
                format_str
            );
            let _ = tx.send(PwCommand::AddPlugin {
                plugin_uri: uri_str.clone(),
                instance_id,
                display_name: display_name.clone(),
                format: format_str,
            });
        }

        persist_active_plugins(self.rust().plugin_manager.as_ref());

        QString::from(&display_name)
    }

    pub fn remove_plugin(self: Pin<&mut Self>, node_id: u32) {
        let instance_id = self.find_instance_id_for_node(node_id);
        if let Some(instance_id) = instance_id {
            if let Some(ref tx) = self.rust().cmd_tx {
                log::info!(
                    "Remove plugin: node_id={} instance_id={}",
                    node_id,
                    instance_id
                );
                let _ = tx.send(PwCommand::RemovePlugin { instance_id });
            }
        } else {
            log::warn!(
                "remove_plugin: no LV2 instance found for node_id={}",
                node_id
            );
        }
    }

    pub fn open_plugin_ui(self: Pin<&mut Self>, node_id: u32) {
        let instance_id = self.find_instance_id_for_node(node_id);
        if let Some(instance_id) = instance_id {
            if let Some(ref tx) = self.rust().cmd_tx {
                log::info!(
                    "Open plugin UI: node_id={} instance_id={}",
                    node_id,
                    instance_id
                );
                let _ = tx.send(PwCommand::OpenPluginUI { instance_id });
            }
        } else {
            log::warn!(
                "open_plugin_ui: no LV2 instance found for node_id={}",
                node_id
            );
        }
    }

    pub fn rename_plugin(mut self: Pin<&mut Self>, node_id: u32, new_name: QString) {
        let name_str: String = new_name.to_string();
        let instance_id = self.find_instance_id_for_node(node_id);
        if let Some(instance_id) = instance_id {
            log::info!(
                "Rename plugin: node_id={} instance_id={} new_name={}",
                node_id,
                instance_id,
                name_str
            );
            if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager
                && let Some(info) = mgr.get_instance_mut(instance_id)
            {
                info.display_name = name_str.clone();
            }
            if let Some(ref graph) = self.rust().graph {
                graph.set_node_description(node_id, &name_str);
            }
            persist_active_plugins(self.rust().plugin_manager.as_ref());
        } else {
            log::warn!(
                "rename_plugin: no LV2 instance found for node_id={}",
                node_id
            );
        }
    }

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
                    })
                })
                .collect();
            let result = serde_json::json!({
                "instanceId": instance_id,
                "pluginUri": info.plugin_uri,
                "displayName": info.display_name,
                "bypassed": info.bypassed,
                "parameters": params,
            });
            let json = serde_json::to_string(&result).unwrap_or_default();
            return QString::from(&json);
        }
        QString::from("{}")
    }

    pub fn set_plugin_parameter(
        mut self: Pin<&mut Self>,
        node_id: u32,
        port_index: u32,
        value: f32,
    ) {
        let instance_id = self.find_instance_id_for_node(node_id);
        if let Some(instance_id) = instance_id {
            if let Some(ref tx) = self.rust().cmd_tx {
                let _ = tx.send(PwCommand::SetPluginParameter {
                    instance_id,
                    port_index: port_index as usize,
                    value,
                });
            }
            if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
                mgr.update_parameter(instance_id, port_index as usize, value);
            }
            self.as_mut().rust_mut().params_dirty = true;
            if self.rust().params_dirty_since.is_none() {
                self.as_mut().rust_mut().params_dirty_since = Some(Instant::now());
            }
        }
    }

    pub fn set_plugin_bypass(mut self: Pin<&mut Self>, node_id: u32, bypassed: bool) {
        let instance_id = self.find_instance_id_for_node(node_id);
        if let Some(instance_id) = instance_id {
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
    }

    pub fn get_active_plugins_json(self: Pin<&mut Self>) -> QString {
        if let Some(ref mgr) = self.rust().plugin_manager {
            let mut entries: Vec<serde_json::Value> = mgr
                .active_instances()
                .values()
                .map(|info| {
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
                            })
                        })
                        .collect();
                    serde_json::json!({
                        "instanceId": info.id,
                        "stableId": info.stable_id,
                        "pluginUri": info.plugin_uri,
                        "displayName": info.display_name,
                        "bypassed": info.bypassed,
                        "active": info.pw_node_id.is_some(),
                        "parameters": params,
                    })
                })
                .collect();
            entries.sort_by(|a, b| {
                let a_name = a["displayName"].as_str().unwrap_or("");
                let b_name = b["displayName"].as_str().unwrap_or("");
                a_name.cmp(b_name)
            });
            let json = serde_json::to_string(&entries).unwrap_or_default();
            QString::from(&json)
        } else {
            QString::from("[]")
        }
    }

    pub fn remove_plugin_by_stable_id(mut self: Pin<&mut Self>, stable_id: QString) {
        let sid: String = stable_id.to_string();

        let instance_id = self
            .rust()
            .plugin_manager
            .as_ref()
            .and_then(|mgr| mgr.instance_id_for_stable_id(&sid));

        if let Some(instance_id) = instance_id {
            if let Some(ref tx) = self.rust().cmd_tx {
                let _ = tx.send(PwCommand::RemovePlugin { instance_id });
            }
            if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
                mgr.remove_instance(instance_id);
            }
            persist_active_plugins(self.rust().plugin_manager.as_ref());
            log::info!("Removed plugin instance (stable_id={})", sid);
        } else {
            log::warn!(
                "remove_plugin_by_stable_id: no instance found for stable_id={}",
                sid
            );
        }
    }

    pub fn reset_plugin_params_by_stable_id(mut self: Pin<&mut Self>, stable_id: QString) {
        let sid: String = stable_id.to_string();

        let resets: Vec<(u64, usize, f32)> = if let Some(ref mgr) = self.rust().plugin_manager {
            if let Some(info) = mgr.find_by_stable_id(&sid) {
                info.parameters
                    .iter()
                    .map(|p| (info.id, p.port_index, p.default))
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        if resets.is_empty() {
            return;
        }

        let instance_id = resets[0].0;
        for (_, port_index, default) in &resets {
            if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
                mgr.update_parameter(instance_id, *port_index, *default);
            }
            if let Some(ref tx) = self.rust().cmd_tx {
                let _ = tx.send(PwCommand::SetPluginParameter {
                    instance_id,
                    port_index: *port_index,
                    value: *default,
                });
            }
        }

        self.as_mut().rust_mut().params_dirty = true;
        if self.rust().params_dirty_since.is_none() {
            self.as_mut().rust_mut().params_dirty_since = Some(Instant::now());
        }
        log::info!(
            "Reset {} params to defaults for stable_id={}",
            resets.len(),
            sid
        );
    }

    pub fn set_plugin_param_by_stable_id(
        mut self: Pin<&mut Self>,
        stable_id: QString,
        port_index: u32,
        value: f32,
    ) {
        let sid: String = stable_id.to_string();

        let instance_id = self
            .rust()
            .plugin_manager
            .as_ref()
            .and_then(|mgr| mgr.instance_id_for_stable_id(&sid));

        if let Some(instance_id) = instance_id {
            if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
                mgr.update_parameter(instance_id, port_index as usize, value);
            }
            if let Some(ref tx) = self.rust().cmd_tx {
                let _ = tx.send(PwCommand::SetPluginParameter {
                    instance_id,
                    port_index: port_index as usize,
                    value,
                });
            }
            self.as_mut().rust_mut().params_dirty = true;
            if self.rust().params_dirty_since.is_none() {
                self.as_mut().rust_mut().params_dirty_since = Some(Instant::now());
            }
        }
    }

    pub fn get_window_geometry_json(self: Pin<&mut Self>) -> QString {
        let path = config_path("window.json");
        let json = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => "{}".to_string(),
        };
        QString::from(&json)
    }

    pub fn save_window_geometry(self: Pin<&mut Self>, json: QString) {
        let path = config_path("window.json");
        let s: String = json.to_string();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&path, &s) {
            log::error!("Failed to save window geometry to {:?}: {}", path, e);
        }
    }

    pub fn get_viewport_json(self: Pin<&mut Self>) -> QString {
        let path = config_path("viewport.json");
        let json = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => "{}".to_string(),
        };
        QString::from(&json)
    }

    pub fn save_viewport(self: Pin<&mut Self>, json: QString) {
        let path = config_path("viewport.json");
        let s: String = json.to_string();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&path, &s) {
            log::error!("Failed to save viewport to {:?}: {}", path, e);
        }
    }

    pub fn get_rules_json(self: Pin<&mut Self>) -> QString {
        if let Some(ref patchbay) = self.rust().patchbay {
            let json_rules: Vec<serde_json::Value> = patchbay
                .rules()
                .iter()
                .map(|r| {
                    let mappings: Vec<serde_json::Value> = r
                        .port_mappings
                        .iter()
                        .map(|m| {
                            serde_json::json!({
                                "outputPort": m.output_port_name,
                                "inputPort": m.input_port_name,
                            })
                        })
                        .collect();
                    serde_json::json!({
                        "id": r.id,
                        "sourcePattern": r.source_pattern,
                        "sourceType": r.source_node_type.map(rules::node_type_label).unwrap_or("Any"),
                        "targetPattern": r.target_pattern,
                        "targetType": r.target_node_type.map(rules::node_type_label).unwrap_or("Any"),
                        "sourceLabel": r.source_label(),
                        "targetLabel": r.target_label(),
                        "enabled": r.enabled,
                        "portMappings": mappings,
                    })
                })
                .collect();
            let json = serde_json::to_string(&json_rules).unwrap_or_default();
            QString::from(&json)
        } else {
            QString::from("[]")
        }
    }

    pub fn toggle_rule(mut self: Pin<&mut Self>, rule_id: QString) {
        let id: String = rule_id.to_string();
        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            patchbay.toggle_rule(&id);
        }
        save_rules(self.rust().patchbay.as_ref());
    }

    pub fn remove_rule(mut self: Pin<&mut Self>, rule_id: QString) {
        let id: String = rule_id.to_string();
        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            patchbay.remove_rule(&id);
        }
        save_rules(self.rust().patchbay.as_ref());
    }

    pub fn apply_rules(mut self: Pin<&mut Self>) {
        let commands = if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            patchbay.scan()
        } else {
            Vec::new()
        };
        if let Some(ref tx) = self.rust().cmd_tx {
            for cmd in commands {
                let _ = tx.send(cmd);
            }
        }
    }

    pub fn snapshot_rules(mut self: Pin<&mut Self>) {
        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            patchbay.snapshot_current_connections();
        }
        save_rules(self.rust().patchbay.as_ref());
        log::info!("Snapshot: replaced rules with current connections");
    }

    pub fn toggle_patchbay(mut self: Pin<&mut Self>, enabled: bool) {
        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            patchbay.enabled = enabled;
        }
        self.as_mut().set_patchbay_enabled(enabled);
    }

    pub fn get_node_names_json(self: Pin<&mut Self>) -> QString {
        if let Some(ref graph) = self.rust().graph {
            let nodes = graph.get_all_nodes();
            let mut entries: Vec<serde_json::Value> = Vec::new();

            for n in nodes.iter().filter(|n| n.ready) {
                let media_str = match n.media_type {
                    Some(crate::pipewire::MediaType::Audio) => "Audio",
                    Some(crate::pipewire::MediaType::Video) => "Video",
                    Some(crate::pipewire::MediaType::Midi) => "Midi",
                    None => "Unknown",
                };

                if n.is_bridge {
                    // For bridge nodes, list each device sub-node separately
                    let groups = graph.get_bridge_port_groups(n.id);
                    for (_group, device_name) in &groups {
                        entries.push(serde_json::json!({
                            "name": device_name,
                            "type": "Duplex",
                            "mediaType": media_str,
                        }));
                    }
                } else {
                    let type_str = match n.node_type {
                        Some(NodeType::Sink) => "Sink",
                        Some(NodeType::Source) => "Source",
                        Some(NodeType::StreamOutput) => "App Out",
                        Some(NodeType::StreamInput) => "App In",
                        Some(NodeType::Duplex) => "Duplex",
                        Some(NodeType::Plugin) => "Plugin",
                        None => "Unknown",
                    };
                    entries.push(serde_json::json!({
                        "name": n.display_name(),
                        "type": type_str,
                        "mediaType": media_str,
                    }));
                }
            }

            entries.sort_by(|a, b| {
                let a_name = a["name"].as_str().unwrap_or("");
                let b_name = b["name"].as_str().unwrap_or("");
                a_name.cmp(b_name)
            });
            entries.dedup_by(|a, b| {
                a["name"].as_str() == b["name"].as_str() && a["type"].as_str() == b["type"].as_str()
            });
            let json = serde_json::to_string(&entries).unwrap_or_default();
            QString::from(&json)
        } else {
            QString::from("[]")
        }
    }

    pub fn add_rule(
        mut self: Pin<&mut Self>,
        source_pattern: QString,
        source_type: QString,
        target_pattern: QString,
        target_type: QString,
    ) {
        let src_pat: String = source_pattern.to_string();
        let src_type: String = source_type.to_string();
        let tgt_pat: String = target_pattern.to_string();
        let tgt_type: String = target_type.to_string();

        let src_node_type = parse_node_type(&src_type);
        let tgt_node_type = parse_node_type(&tgt_type);

        let rule = crate::patchbay::rules::AutoConnectRule::new(
            src_pat,
            src_node_type,
            tgt_pat,
            tgt_node_type,
            None,
        );

        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            patchbay.add_rule(rule);
        }
        save_rules(self.rust().patchbay.as_ref());
    }

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
        log::info!("Preferences reset to defaults");
    }

    pub fn get_poll_interval_ms(self: Pin<&mut Self>) -> i32 {
        self.rust().prefs.poll_interval_ms as i32
    }

    pub fn get_cpu_history(self: Pin<&mut Self>) -> QString {
        let json = serde_json::to_string(&self.rust().cpu_history).unwrap_or_default();
        QString::from(&json)
    }

    pub fn get_plugin_cpu_json(self: Pin<&mut Self>) -> QString {
        use crate::plugin::cpu_stats::global_cpu_tracker;

        let snapshots = global_cpu_tracker().take_all_snapshots();
        let items: Vec<serde_json::Value> = snapshots
            .into_iter()
            .map(|(id, name, snap)| {
                serde_json::json!({
                    "id": id,
                    "name": name,
                    "dspPercent": (snap.dsp_percent * 100.0).round() / 100.0,
                    "avgUs": snap.avg_ns / 1000,
                    "lastUs": snap.last_ns / 1000,
                    "calls": snap.calls,
                })
            })
            .collect();
        let json = serde_json::to_string(&items).unwrap_or_default();
        QString::from(&json)
    }

    pub fn get_default_node(self: Pin<&mut Self>) -> QString {
        let path = config_path("default_node.txt");
        match std::fs::read_to_string(&path) {
            Ok(s) => QString::from(&s.trim().to_string()),
            Err(_) => QString::from(""),
        }
    }

    pub fn set_default_node(mut self: Pin<&mut Self>, layout_key: QString) {
        let key: String = layout_key.to_string();
        let path = config_path("default_node.txt");
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if key.is_empty() {
            let _ = std::fs::remove_file(&path);
            log::info!("Cleared default node");
        } else {
            if let Err(e) = std::fs::write(&path, &key) {
                log::error!("Failed to save default node to {:?}: {}", path, e);
            } else {
                log::info!("Set default node: {}", key);
            }
        }

        // Update patchbay manager with the new default
        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            if key.is_empty() {
                patchbay.set_default_target(None);
            } else {
                // Extract the display name from the layout key (format is "Type:DisplayName")
                let display_name = if let Some(pos) = key.find(':') {
                    key[pos + 1..].to_string()
                } else {
                    key.clone()
                };
                patchbay.set_default_target(Some(display_name));
            }
        }
    }

    pub fn set_window_visible(self: Pin<&mut Self>, visible: bool) {
        if let Some(ref tray) = self.rust().tray_state {
            use std::sync::atomic::Ordering;
            tray.window_visible.store(visible, Ordering::Release);
            log::info!("Window visible state updated to {}", visible);
        }
    }

    fn find_instance_id_for_node(&self, node_id: u32) -> Option<u64> {
        if let Some(ref mgr) = self.rust().plugin_manager {
            for (id, info) in mgr.active_instances() {
                if info.pw_node_id == Some(node_id) {
                    return Some(*id);
                }
            }
        }
        None
    }

    fn unique_display_name(&self, base_name: &str) -> String {
        let existing: Vec<String> = if let Some(ref mgr) = self.rust().plugin_manager {
            mgr.active_instances()
                .values()
                .map(|info| info.display_name.clone())
                .collect()
        } else {
            Vec::new()
        };

        if !existing.iter().any(|n| n == base_name) {
            return base_name.to_string();
        }

        for n in 2.. {
            let candidate = format!("{} #{}", base_name, n);
            if !existing.iter().any(|n| n == &candidate) {
                return candidate;
            }
        }
        unreachable!()
    }
}

fn config_path(filename: &str) -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("zestbay")
        .join(filename)
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct SavedPlugin {
    #[serde(default)]
    stable_id: String,
    uri: String,
    display_name: String,
    #[serde(default)]
    bypassed: bool,
    #[serde(default)]
    parameters: Vec<SavedPluginParam>,
    /// "LV2", "CLAP", or "VST3".  Defaults to "LV2" for backwards compat.
    #[serde(default = "default_lv2_format_str")]
    format: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct SavedPluginParam {
    port_index: usize,
    symbol: String,
    value: f32,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct SavedPluginLink {
    output_node_name: String,
    output_port_name: String,
    input_node_name: String,
    input_port_name: String,
}

fn default_lv2_format_str() -> String {
    "LV2".to_string()
}

fn load_saved_plugins() -> Vec<SavedPlugin> {
    let path = config_path("plugins.json");
    match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn persist_active_plugins(plugin_manager: Option<&PluginManager>) {
    let mut plugins: Vec<SavedPlugin> = if let Some(mgr) = plugin_manager {
        mgr.active_instances()
            .values()
            .map(|info| {
                let params: Vec<SavedPluginParam> = info
                    .parameters
                    .iter()
                    .map(|p| SavedPluginParam {
                        port_index: p.port_index,
                        symbol: p.symbol.clone(),
                        value: p.value,
                    })
                    .collect();
                SavedPlugin {
                    stable_id: info.stable_id.clone(),
                    uri: info.plugin_uri.clone(),
                    display_name: info.display_name.clone(),
                    bypassed: info.bypassed,
                    parameters: params,
                    format: info.format.as_str().to_string(),
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    plugins.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    let path = config_path("plugins.json");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(&plugins).unwrap_or_default();
    if let Err(e) = std::fs::write(&path, &json) {
        log::error!("Failed to save plugins to {:?}: {}", path, e);
    } else {
        log::debug!("persist_active_plugins: {} plugins written", plugins.len());
    }
}

fn load_saved_links() -> Vec<SavedPluginLink> {
    let path = config_path("links.json");
    match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn bridge_device_name_for_port(port: &Port) -> Option<String> {
    port.port_alias.as_ref().and_then(|alias| {
        alias.find(':').map(|pos| alias[..pos].to_string())
    })
}

fn build_persistable_links(graph: &GraphState) -> Vec<SavedPluginLink> {
    let links = graph.get_all_links();
    let mut saved_links = Vec::new();

    for link in &links {
        let out_node = graph.get_node(link.output_node_id);
        let in_node = graph.get_node(link.input_node_id);
        let out_port = graph.get_port(link.output_port_id);
        let in_port = graph.get_port(link.input_port_id);

        let involves_lv2 = out_node
            .as_ref()
            .map(|n| n.node_type == Some(NodeType::Plugin))
            .unwrap_or(false)
            || in_node
                .as_ref()
                .map(|n| n.node_type == Some(NodeType::Plugin))
                .unwrap_or(false);

        let involves_midi = out_port
            .as_ref()
            .map(|p| p.media_type == Some(crate::pipewire::MediaType::Midi))
            .unwrap_or(false)
            || in_port
                .as_ref()
                .map(|p| p.media_type == Some(crate::pipewire::MediaType::Midi))
                .unwrap_or(false);

        if !involves_lv2 && !involves_midi {
            continue;
        }

        if let (Some(out_node), Some(in_node), Some(out_port), Some(in_port)) =
            (out_node, in_node, out_port, in_port)
        {
            // For bridge nodes, use the device name from port.alias instead
            // of the bridge node name for more stable link restoration
            let out_name = if out_node.is_bridge {
                bridge_device_name_for_port(&out_port)
                    .unwrap_or_else(|| out_node.display_name().to_string())
            } else {
                out_node.display_name().to_string()
            };
            let in_name = if in_node.is_bridge {
                bridge_device_name_for_port(&in_port)
                    .unwrap_or_else(|| in_node.display_name().to_string())
            } else {
                in_node.display_name().to_string()
            };
            saved_links.push(SavedPluginLink {
                output_node_name: out_name,
                output_port_name: out_port.name.clone(),
                input_node_name: in_name,
                input_port_name: in_port.name.clone(),
            });
        }
    }

    saved_links
}

fn persist_lv2_links(graph: Option<&Arc<GraphState>>) {
    let links = if let Some(graph) = graph {
        build_persistable_links(graph)
    } else {
        Vec::new()
    };
    let path = config_path("links.json");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(&links).unwrap_or_default();
    if let Err(e) = std::fs::write(&path, &json) {
        log::error!("Failed to save links to {:?}: {}", path, e);
    } else {
        log::debug!("persist_lv2_links: {} links written", links.len());
    }
}

fn load_rules() -> Vec<crate::patchbay::rules::AutoConnectRule> {
    let path = config_path("rules.json");
    match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_rules(patchbay: Option<&PatchbayManager>) {
    let rules: Vec<crate::patchbay::rules::AutoConnectRule> = if let Some(mgr) = patchbay {
        mgr.rules().to_vec()
    } else {
        Vec::new()
    };
    let path = config_path("rules.json");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(&rules).unwrap_or_default();
    if let Err(e) = std::fs::write(&path, &json) {
        log::error!("Failed to save rules to {:?}: {}", path, e);
    } else {
        log::debug!("save_rules: {} rules written", rules.len());
    }
}

fn parse_node_type(s: &str) -> Option<NodeType> {
    match s {
        "Sink" => Some(NodeType::Sink),
        "Source" => Some(NodeType::Source),
        "App Out" | "StreamOutput" => Some(NodeType::StreamOutput),
        "App In" | "StreamInput" => Some(NodeType::StreamInput),
        "Duplex" => Some(NodeType::Duplex),
        "Plugin" | "Lv2Plugin" => Some(NodeType::Plugin),
        _ => None,
    }
}

fn node_to_json(
    n: &Node,
    plugin_manager: Option<&crate::plugin::manager::PluginManager>,
) -> serde_json::Value {
    let type_str = match n.node_type {
        Some(NodeType::Sink) => "Sink",
        Some(NodeType::Source) => "Source",
        Some(NodeType::StreamOutput) => "StreamOutput",
        Some(NodeType::StreamInput) => "StreamInput",
        Some(NodeType::Duplex) => "Duplex",
        Some(NodeType::Plugin) => "Plugin",
        None => "Unknown",
    };
    let media_str = match n.media_type {
        Some(crate::pipewire::MediaType::Audio) => "Audio",
        Some(crate::pipewire::MediaType::Video) => "Video",
        Some(crate::pipewire::MediaType::Midi) => "Midi",
        None => "Unknown",
    };

    let mut val = serde_json::json!({
        "id": n.id,
        "name": n.display_name(),
        "type": type_str,
        "mediaType": media_str,
        "isVirtual": n.is_virtual,
        "isJack": n.is_jack,
        "layoutKey": layout_key(n, plugin_manager),
        "ready": n.ready,
    });

    // Enrich plugin nodes with format and hasUi info
    if n.node_type == Some(NodeType::Plugin) {
        if let Some(mgr) = plugin_manager {
            // Find the active instance whose pw_node_id matches this node
            if let Some(instance) = mgr
                .active_instances()
                .values()
                .find(|inst| inst.pw_node_id == Some(n.id))
            {
                let format_str = instance.format.as_str();
                let has_ui = mgr
                    .find_plugin(&instance.plugin_uri)
                    .map(|p| p.has_ui)
                    .unwrap_or(false);

                val["pluginFormat"] = serde_json::json!(format_str);
                val["pluginHasUi"] = serde_json::json!(has_ui);
            }
        }
    }

    val
}

fn layout_key(
    node: &Node,
    plugin_manager: Option<&crate::plugin::manager::PluginManager>,
) -> String {
    let prefix = match node.node_type {
        Some(NodeType::Sink) => "Sink".to_string(),
        Some(NodeType::Source) => "Source".to_string(),
        Some(NodeType::StreamOutput) => "StreamOut".to_string(),
        Some(NodeType::StreamInput) => "StreamIn".to_string(),
        Some(NodeType::Duplex) => "Duplex".to_string(),
        Some(NodeType::Plugin) => {
            // Use the actual format if available
            if let Some(mgr) = plugin_manager {
                mgr.active_instances()
                    .values()
                    .find(|inst| inst.pw_node_id == Some(node.id))
                    .map(|inst| inst.format.as_str().to_string())
                    .unwrap_or_else(|| "Plugin".to_string())
            } else {
                "Plugin".to_string()
            }
        }
        None => "Unknown".to_string(),
    };
    format!("{}:{}", prefix, node.display_name())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Preferences {
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
}

impl Preferences {
    fn default_rule_settle_ms() -> u64 {
        500
    }
    fn default_params_persist_ms() -> u64 {
        1000
    }
    fn default_links_persist_ms() -> u64 {
        2000
    }
    fn default_poll_interval_ms() -> u64 {
        100
    }
    fn default_auto_learn_rules() -> bool {
        true
    }
    fn default_start_minimized() -> bool {
        false
    }
    fn default_close_to_tray() -> bool {
        false
    }
    fn default_pw_tick_interval_ms() -> u64 {
        10
    }
    fn default_pw_operation_cooldown_ms() -> u64 {
        50
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
        }
    }
}

fn load_preferences() -> Preferences {
    let path = config_path("preferences.json");
    match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Preferences::default(),
    }
}

fn read_process_cpu_ticks() -> u64 {
    let Ok(stat) = std::fs::read_to_string("/proc/self/stat") else {
        return 0;
    };
    let fields: Vec<&str> = stat.split_whitespace().collect();
    if fields.len() < 15 {
        return 0;
    }
    let utime: u64 = fields[13].parse().unwrap_or(0);
    let stime: u64 = fields[14].parse().unwrap_or(0);
    utime + stime
}

fn clock_ticks_per_sec() -> f64 {
    static TICKS: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *TICKS.get_or_init(|| {
        unsafe extern "C" {
            fn sysconf(name: i32) -> i64;
        }
        const _SC_CLK_TCK: i32 = 2;
        let val = unsafe { sysconf(_SC_CLK_TCK) };
        if val > 0 { val as f64 } else { 100.0 }
    })
}

fn num_cpus() -> f64 {
    static CPUS: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *CPUS.get_or_init(|| {
        unsafe extern "C" {
            fn sysconf(name: i32) -> i64;
        }
        const _SC_NPROCESSORS_ONLN: i32 = 84;
        let val = unsafe { sysconf(_SC_NPROCESSORS_ONLN) };
        if val > 0 { val as f64 } else { 1.0 }
    })
}

fn measure_cpu_usage(
    prev_ticks: &mut u64,
    prev_time: &mut Option<Instant>,
    avg: &mut f64,
) -> String {
    const ALPHA: f64 = 0.15;

    let current_ticks = read_process_cpu_ticks();
    let now = Instant::now();

    if let Some(prev) = *prev_time {
        let elapsed = now.duration_since(prev).as_secs_f64();
        if elapsed > 0.0 {
            let delta_ticks = current_ticks.saturating_sub(*prev_ticks) as f64;
            let cpu_secs = delta_ticks / clock_ticks_per_sec();
            let sample = (cpu_secs / elapsed / num_cpus()) * 100.0;
            *avg = *avg * (1.0 - ALPHA) + sample * ALPHA;
        }
    }

    *prev_ticks = current_ticks;
    *prev_time = Some(now);
    format!("{:.1}%", *avg)
}

fn save_preferences(prefs: &Preferences) {
    let path = config_path("preferences.json");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(prefs).unwrap_or_default();
    if let Err(e) = std::fs::write(&path, &json) {
        log::error!("Failed to save preferences to {:?}: {}", path, e);
    } else {
        log::debug!("save_preferences: written to {:?}", path);
    }
}
