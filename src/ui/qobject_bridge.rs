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
        fn get_pinned_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn save_pinned(self: Pin<&mut Self>, json: QString);

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

        #[qinvokable]
        fn get_app_version(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn get_qt_version(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn backup_rules(self: Pin<&mut Self>, name: QString) -> QString;

        #[qinvokable]
        fn list_rule_backups_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn restore_rule_backup(self: Pin<&mut Self>, filename: QString);

        #[qinvokable]
        fn delete_rule_backup(self: Pin<&mut Self>, filename: QString);

        #[qinvokable]
        fn auto_layout(
            self: Pin<&mut Self>,
            node_sizes_json: QString,
            pinned_positions_json: QString,
        ) -> QString;

        #[qinvokable]
        fn start_midi_learn(
            self: Pin<&mut Self>,
            instance_id: u64,
            port_index: u32,
            label: QString,
            mode: QString,
        );

        #[qinvokable]
        fn cancel_midi_learn(self: Pin<&mut Self>);

        #[qinvokable]
        fn remove_midi_mapping_for_param(
            self: Pin<&mut Self>,
            instance_id: u64,
            port_index: u32,
        );

        #[qinvokable]
        fn get_midi_mappings_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn get_midi_mapping_for_param_json(
            self: Pin<&mut Self>,
            instance_id: u64,
            port_index: u32,
        ) -> QString;

        #[qinvokable]
        fn restore_known_good(self: Pin<&mut Self>) -> bool;

        #[qinvokable]
        fn reassign_midi_mapping(self: Pin<&mut Self>) -> bool;

        #[qinvokable]
        fn dismiss_midi_conflict(self: Pin<&mut Self>);
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

        #[qsignal]
        fn midi_mapping_added(self: Pin<&mut AppController>, mapping_json: QString);

        #[qsignal]
        fn midi_mapping_removed(self: Pin<&mut AppController>, instance_id: u64, port_index: u32);

        #[qsignal]
        fn midi_learn_started(self: Pin<&mut AppController>, instance_id: u64, port_index: u32);

        #[qsignal]
        fn midi_learn_cancelled(self: Pin<&mut AppController>);

        #[qsignal]
        fn midi_mapping_conflict(self: Pin<&mut AppController>, source_json: QString, existing_label: QString);

        #[qsignal]
        fn crash_recovery_available(self: Pin<&mut AppController>, crashed_uris: QString);

        #[qsignal]
        fn plugin_parameter_changed(
            self: Pin<&mut AppController>,
            instance_id: u64,
            port_index: u32,
            value: f32,
        );
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

    /// Rebuild only the port mappings. Node id mappings survive for the whole
    /// session: a device must keep its virtual id across refreshes, otherwise a
    /// reconnecting device inherits another node's id — and with it, that
    /// node's on-screen position — instead of its own saved one.
    fn clear_ports(&mut self) {
        self.port_to_virtual.clear();
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

    /// Instance ids of the plugins restored from plugins.json at startup that
    /// have not reported back yet (added or failed).
    restore_instance_ids: std::collections::HashSet<u64>,
    restore_started_at: Option<std::time::Instant>,
    restore_batch_size: usize,
    /// Saved links not (yet) present in the graph. Kept across persists so a
    /// device or plugin that is absent right now doesn't lose its links.
    pending_links: Vec<PendingLink>,
    /// Persistable links seen in the graph at the last persist.
    last_graph_links: Vec<SavedPluginLink>,
    /// Display names of plugins removed/renamed this session; links to them
    /// are forgotten rather than remembered as pending.
    forgotten_link_nodes: std::collections::HashSet<String>,
    links_retry_due: bool,

    links_dirty: bool,
    links_dirty_since: Option<std::time::Instant>,

    /// The mapping a MIDI-learn just tried to add (kept for conflict resolution).
    last_learned_mapping: Option<crate::midi::MidiCcMapping>,
    pending_conflict_mapping: Option<crate::midi::MidiCcMapping>,

    state_request_sent_at: Option<Instant>,
    last_state_autosave: Instant,
    /// plugins.json could not be parsed at startup — never snapshot it as known-good.
    plugins_load_failed: bool,

    /// PipeWire connection dropped; the graph is being torn down/rebuilt.
    pw_reconnecting: bool,
    /// Links are not persisted until this time (graph still settling after a
    /// reconnect or an in-place plugin reload).
    links_suppressed_until: Option<Instant>,
    /// MIDI mappings whose device is unplugged: kept on disk and re-added
    /// when the device comes back.
    dormant_midi_mappings: Vec<crate::midi::MidiCcMapping>,

    prefs: Preferences,

    tray_state: Option<TrayState>,

    prev_cpu_ticks: u64,
    prev_cpu_time: Option<Instant>,
    cpu_avg: f64,
    cpu_history: Vec<f64>,

    bridge_split: BridgeSplitState,

    midi_mappings: Vec<crate::midi::MidiCcMapping>,
    midi_learn_target: Option<(u64, usize, String, crate::midi::MappingMode)>,
    plugins_frozen: bool,
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
            restore_instance_ids: std::collections::HashSet::new(),
            restore_started_at: None,
            restore_batch_size: 0,
            pending_links: Vec::new(),
            last_graph_links: Vec::new(),
            forgotten_link_nodes: std::collections::HashSet::new(),
            links_retry_due: false,
            links_dirty: false,
            links_dirty_since: None,
            last_learned_mapping: None,
            pending_conflict_mapping: None,
            state_request_sent_at: None,
            last_state_autosave: Instant::now(),
            plugins_load_failed: false,
            pw_reconnecting: false,
            links_suppressed_until: None,
            dormant_midi_mappings: Vec::new(),
            prefs: Preferences::default(),
            tray_state: None,
            cpu_usage: QString::from("0.0%"),
            prev_cpu_ticks: 0,
            prev_cpu_time: None,
            cpu_avg: 0.0,
            cpu_history: vec![0.0; 120],
            bridge_split: BridgeSplitState::new(),
            midi_mappings: Vec::new(),
            midi_learn_target: None,
            plugins_frozen: false,
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
                "Loaded {} saved plugin/MIDI links for restoration",
                saved_links.len()
            );
            self.as_mut().rust_mut().pending_links =
                saved_links.into_iter().map(PendingLink::new).collect();
        }

        let saved = match load_saved_plugins() {
            Ok(saved) => saved,
            Err(()) => {
                self.as_mut().rust_mut().plugins_load_failed = true;
                Vec::new()
            }
        };
        let safe_mode = crate::SAFE_MODE.load(std::sync::atomic::Ordering::SeqCst);

        let crashed_uris_str = if !safe_mode && has_crash_marker() {
            let crashed_uris = read_crash_marker();
            log::warn!(
                "Crash marker detected! Previous plugin restore crashed. \
                 Auto-entering safe mode. Crashed URIs: {:?}",
                crashed_uris
            );
            remove_crash_marker();
            Some(crashed_uris.join(", "))
        } else {
            None
        };
        let auto_safe = crashed_uris_str.is_some();

        let skip_restore = safe_mode || auto_safe;

        // stable_id -> instance id of every plugin restored below, plus the
        // restore order (legacy MIDI mappings stored the old per-session ids,
        // which were assigned 1..N in this order).
        let mut stable_to_instance: HashMap<String, u64> = HashMap::new();
        let mut restore_order: Vec<u64> = Vec::new();

        if skip_restore {
            log::warn!(
                "Safe mode active: skipping restoration of {} saved plugins. \
                 Plugins remain in plugins.json but will not be loaded. \
                 Restart without --safe-mode to try again.",
                saved.len()
            );
            self.as_mut().rust_mut().plugins_frozen = true;
            crate::PLUGINS_FROZEN.store(true, std::sync::atomic::Ordering::SeqCst);
        } else if !saved.is_empty() {
            log::info!("Restoring {} saved plugins", saved.len());
            let restore_uris: Vec<String> = saved.iter().map(|sp| sp.uri.clone()).collect();
            write_crash_marker(&restore_uris);
            self.as_mut().rust_mut().restore_started_at = Some(Instant::now());
            self.as_mut().rust_mut().restore_batch_size = saved.len();
            for sp in saved {
                let instance_id = self.rust().next_instance_id;
                self.as_mut().rust_mut().next_instance_id += 1;
                self.as_mut().rust_mut().restore_instance_ids.insert(instance_id);
                restore_order.push(instance_id);

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
                                    is_toggle: port.is_toggle,
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
                                is_toggle: false,
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
                stable_to_instance.insert(sid.clone(), instance_id);

                let plugin_format = match sp.format.as_str() {
                    "CLAP" => crate::plugin::PluginFormat::Clap,
                    "VST3" => crate::plugin::PluginFormat::Vst3,
                    _ => crate::plugin::PluginFormat::Lv2,
                };

                let plugin_state = sp.decode_plugin_state();

                if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
                    let info = crate::lv2::Lv2InstanceInfo {
                        id: instance_id,
                        stable_id: sid.clone(),
                        plugin_uri: sp.uri.clone(),
                        format: plugin_format,
                        display_name: sp.display_name.clone(),
                        pw_node_id: None,
                        parameters: restored_params,
                        active: true,
                        bypassed: sp.bypassed,
                        lv2_state: sp.lv2_state.clone(),
                        plugin_state: plugin_state.clone(),
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
                        lv2_state: sp.lv2_state,
                        plugin_state,
                        stable_id: sid,
                    });
                }
            }
        }

        // MIDI mappings target plugins, so they are only restored when the
        // plugins are; in safe mode midi_mappings.json is left untouched.
        if !skip_restore {
            let saved_midi = load_midi_mappings();
            if !saved_midi.is_empty() {
                let resolved =
                    resolve_saved_midi_mappings(saved_midi, &stable_to_instance, &restore_order);
                log::info!("Restoring {} saved MIDI mappings", resolved.len());
                if let Some(ref tx) = self.rust().cmd_tx {
                    for mapping in &resolved {
                        let _ = tx.send(PwCommand::AddMidiMapping(mapping.clone()));
                    }
                }
                self.as_mut().rust_mut().midi_mappings = resolved;
            }
        }
        let tray_state = crate::tray::spawn_tray();
        if self.rust().prefs.start_minimized {
            tray_state
                .window_visible
                .store(false, std::sync::atomic::Ordering::Release);
        }
        self.as_mut().rust_mut().tray_state = Some(tray_state);

        if let Some(ref uris) = crashed_uris_str {
            if has_known_good_plugins() {
                self.as_mut().crash_recovery_available(QString::from(uris.as_str()));
            } else {
                let msg = format!(
                    "Crash detected during plugin restore. No known-good snapshot available.\n\
                     Crashed plugins: {}\n\
                     Restart without --safe-mode to try again, or manually edit plugins.json.",
                    uris
                );
                self.as_mut().error_occurred(QString::from(msg.as_str()));
            }
        }

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
        let mut errors: Vec<String> = Vec::new();
        let mut plugin_events: Vec<PluginEvent> = Vec::new();

        let has_events = self.rust().event_rx.is_some();
        if has_events {
            let rx = self.as_mut().rust_mut().event_rx.take();
            if let Some(rx) = rx {
                while let Ok(event) = rx.try_recv() {
                    match event {
                        PwEvent::NodeChanged(_) => {
                            changed = true;
                            if self.rust().pw_reconnecting {
                                // Nodes are coming back; give the graph a few
                                // seconds to fill in before links are saved.
                                self.as_mut().rust_mut().pw_reconnecting = false;
                                self.as_mut().suppress_link_persist(LINK_SETTLE_AFTER_RECONNECT);
                            }
                        }
                        PwEvent::NodeRemoved(_)
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
                            if msg.contains("reconnecting to PipeWire") {
                                self.as_mut().rust_mut().pw_reconnecting = true;
                            }
                            if !errors.contains(&msg) {
                                errors.push(msg);
                            }
                        }
                        PwEvent::Plugin(plugin_event) => {
                            plugin_events.push(plugin_event);
                        }
                    }
                }
                self.as_mut().rust_mut().event_rx = Some(rx);
            }
        }

        // Latest value per (instance, port); emitted once per poll.
        let mut param_updates: Vec<(u64, u32, f32)> = Vec::new();

        for event in plugin_events {
            match event {
                PluginEvent::PluginAdded {
                    instance_id,
                    pw_node_id,
                    display_name,
                } => {
                    changed = true;
                    log::info!(
                        "LV2 plugin added: instance={} pw_node={} name={}",
                        instance_id,
                        pw_node_id,
                        display_name
                    );
                    // A PluginAdded for an instance that already had a node is
                    // a re-add (PipeWire reconnect or in-place reload at a new
                    // sample rate), not a new plugin.
                    let re_added = self
                        .rust()
                        .plugin_manager
                        .as_ref()
                        .and_then(|mgr| mgr.get_instance(instance_id))
                        .is_some_and(|info| info.pw_node_id.is_some());
                    if re_added {
                        log::info!("Plugin instance {} re-added (reconnect/reload)", instance_id);
                        self.as_mut().suppress_link_persist(LINK_SETTLE_AFTER_RECONNECT);
                    }
                    if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
                        mgr.set_instance_pw_node_id(instance_id, pw_node_id);
                        if let Some(info) = mgr.get_instance_mut(instance_id) {
                            info.active = true;
                        }
                    }
                    if pw_node_id != 0
                        && pw_node_id != u32::MAX
                        && let Some(ref graph) = self.rust().graph
                    {
                        graph.set_node_type(pw_node_id, NodeType::Plugin);
                    }
                    self.as_mut()
                        .rust_mut()
                        .forgotten_link_nodes
                        .remove(&display_name);

                    // A reload/reconnect restores params and bypass on the
                    // PipeWire side (on top of the plugin's restored state);
                    // only a first add gets the saved values from here.
                    if !re_added
                        && let Some(ref mgr) = self.rust().plugin_manager
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

                    self.as_mut().finish_restore_of(instance_id);
                }
                PluginEvent::PluginRemoved { instance_id } => {
                    changed = true;
                    log::info!("LV2 plugin removed: instance={}", instance_id);
                    let name = self
                        .rust()
                        .plugin_manager
                        .as_ref()
                        .and_then(|mgr| mgr.get_instance(instance_id))
                        .map(|info| info.display_name.clone());
                    if let Some(name) = name {
                        self.as_mut().forget_plugin_links(&name);
                    }
                    if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
                        mgr.remove_instance(instance_id);
                    }
                    self.as_mut().drop_plugin_midi_mappings(instance_id);
                    persist_active_plugins(self.rust().plugin_manager.as_ref());
                    self.as_mut().mark_links_dirty();
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
                    let port = port_index as u32;
                    if let Some(entry) = param_updates
                        .iter_mut()
                        .find(|(i, p, _)| *i == instance_id && *p == port)
                    {
                        entry.2 = value;
                    } else {
                        param_updates.push((instance_id, port, value));
                    }
                }
                PluginEvent::PluginSkipped { instance_id } => {
                    // Not running this session, but its entry (state, params,
                    // links, MIDI mappings) stays in the config.
                    changed = true;
                    log::warn!("Plugin instance {} skipped for this session", instance_id);
                    if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager
                        && let Some(info) = mgr.get_instance_mut(instance_id)
                    {
                        info.active = false;
                        info.pw_node_id = None;
                    }
                    self.as_mut().finish_restore_of(instance_id);
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
                            changed = true;
                            if let Some(ref name) = plugin_name {
                                self.as_mut().forget_plugin_links(name);
                            }
                            if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
                                mgr.remove_instance(id);
                            }
                            self.as_mut().drop_plugin_midi_mappings(id);
                            persist_active_plugins(self.rust().plugin_manager.as_ref());
                            self.as_mut().finish_restore_of(id);
                        }

                        errors.push(match (plugin_name, fatal) {
                            (Some(name), true) => {
                                format!("Plugin \"{}\" failed to load: {}", name, message)
                            }
                            (Some(name), false) => format!("Plugin \"{}\": {}", name, message),
                            (None, true) => format!("Plugin failed to load: {}", message),
                            (None, false) => message,
                        });
                    } else {
                        errors.push(message);
                    }
                }
                PluginEvent::MidiLearnStarted { instance_id, port_index } => {
                    log::info!("MIDI learn started: instance={} port={}", instance_id, port_index);
                    self.as_mut().midi_learn_started(instance_id, port_index as u32);
                }
                PluginEvent::MidiLearnCancelled => {
                    log::info!("MIDI learn cancelled");
                    self.as_mut().midi_learn_cancelled();
                }
                PluginEvent::MidiMappingAdded(ref mapping) => {
                    log::info!("MIDI mapping added: {:?} -> {:?}", mapping.source, mapping.target);
                    // One mapping per source and per target: a re-learn replaces.
                    let replaced: Vec<crate::midi::MidiCcTarget> = self
                        .rust()
                        .midi_mappings
                        .iter()
                        .filter(|m| m.source == mapping.source && m.target != mapping.target)
                        .map(|m| m.target.clone())
                        .collect();
                    self.as_mut()
                        .rust_mut()
                        .midi_mappings
                        .retain(|m| m.target != mapping.target && m.source != mapping.source);
                    self.as_mut().rust_mut().midi_mappings.push(mapping.clone());
                    if self
                        .rust()
                        .last_learned_mapping
                        .as_ref()
                        .is_some_and(|m| m.source == mapping.source)
                    {
                        self.as_mut().rust_mut().last_learned_mapping = None;
                    }
                    self.as_mut().persist_midi();
                    for t in replaced {
                        self.as_mut()
                            .midi_mapping_removed(t.instance_id, t.port_index as u32);
                    }
                    let json = serde_json::to_string(mapping).unwrap_or_else(|_| "{}".to_string());
                    self.as_mut().midi_mapping_added(QString::from(&json));
                }
                PluginEvent::MidiMappingRemoved(ref source) => {
                    log::info!("MIDI mapping removed: {:?}", source);
                    let removed: Vec<(u64, u32)> = self
                        .rust()
                        .midi_mappings
                        .iter()
                        .filter(|m| m.source == *source)
                        .map(|m| (m.target.instance_id, m.target.port_index as u32))
                        .collect();
                    // Removed because its device was unplugged: keep it (on
                    // disk too) and re-add it when the device returns.
                    let device_gone = self.rust().graph.as_ref().is_some_and(|g| {
                        !node_name_present(g, &g.get_all_nodes(), &source.device_name)
                    });
                    if device_gone {
                        let gone: Vec<crate::midi::MidiCcMapping> = self
                            .rust()
                            .midi_mappings
                            .iter()
                            .filter(|m| m.source == *source)
                            .cloned()
                            .collect();
                        let mut rust = self.as_mut().rust_mut();
                        rust.dormant_midi_mappings.retain(|m| m.source != *source);
                        rust.dormant_midi_mappings.extend(gone);
                    }
                    self.as_mut().rust_mut().midi_mappings.retain(|m| m.source != *source);
                    self.as_mut().persist_midi();
                    for (iid, pidx) in removed {
                        self.as_mut().midi_mapping_removed(iid, pidx);
                    }
                }
                PluginEvent::MidiMappingConflict { ref source, ref existing_label } => {
                    log::warn!(
                        "MIDI mapping conflict: {:?} already assigned to '{}'",
                        source,
                        existing_label
                    );
                    let learned = self.as_mut().rust_mut().last_learned_mapping.take();
                    self.as_mut().rust_mut().pending_conflict_mapping =
                        learned.filter(|m| m.source == *source);
                    let source_json = serde_json::to_string(source).unwrap_or_else(|_| "{}".to_string());
                    self.as_mut().midi_mapping_conflict(
                        QString::from(&source_json),
                        QString::from(existing_label.as_str()),
                    );
                }
                PluginEvent::PluginStatesDone => {
                    self.as_mut().rust_mut().state_request_sent_at = None;
                    persist_active_plugins(self.rust().plugin_manager.as_ref());
                }
                PluginEvent::PluginStateSaved {
                    instance_id,
                    lv2_state,
                    blob,
                } => {
                    self.as_mut().apply_plugin_state(instance_id, lv2_state, blob);
                }
                PluginEvent::MidiCcReceived { ref device_name, channel, cc, message_type } => {
                    if let Some((instance_id, port_index, label, mode)) =
                        self.as_mut().rust_mut().midi_learn_target.take()
                    {
                        let mapping = crate::midi::MidiCcMapping {
                            source: crate::midi::MidiCcSource {
                                device_name: device_name.clone(),
                                channel: Some(channel),
                                cc,
                                message_type,
                            },
                            target: crate::midi::MidiCcTarget {
                                instance_id,
                                port_index,
                            },
                            mode,
                            label,
                        };
                        self.as_mut().rust_mut().last_learned_mapping = Some(mapping.clone());
                        if let Some(ref tx) = self.rust().cmd_tx {
                            let _ = tx.send(PwCommand::CancelMidiLearn);
                            let _ = tx.send(PwCommand::AddMidiMapping(mapping));
                        }
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
            self.as_mut().rust_mut().links_retry_due = true;
            self.as_mut().revive_dormant_midi_mappings();
        }

        // Pending (remembered) links are merged into every persist, so link
        // changes can be persisted as soon as the plugin restore is done.
        if link_changed && self.rust().restore_instance_ids.is_empty() {
            self.as_mut().mark_links_dirty();
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

        let should_retry_links = {
            let settled = self
                .rust()
                .last_change_time
                .map(|t| t.elapsed() >= Duration::from_millis(rule_settle_ms))
                .unwrap_or(false);
            let expired = self
                .rust()
                .pending_links
                .iter()
                .any(|p| p.connect_sent_at.is_some_and(|t| t.elapsed() >= LINK_CONNECT_GRACE));
            self.rust().restore_instance_ids.is_empty()
                && !self.rust().pending_links.is_empty()
                && ((self.rust().links_retry_due && settled) || expired)
        };
        if should_retry_links {
            self.as_mut().rust_mut().links_retry_due = false;
            self.as_mut().retry_pending_links();
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
        if should_persist_links && !self.links_persist_suppressed() {
            self.as_mut().rust_mut().links_dirty = false;
            self.as_mut().rust_mut().links_dirty_since = None;
            self.as_mut().persist_links_now();
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

            if let Ok(mut req) = tray.open_plugin_ui.lock() {
                if let Some(node_id) = req.take() {
                    log::info!("Tray: open plugin UI requested for node_id={}", node_id);
                    self.as_mut().open_plugin_ui(node_id);
                }
            }
        }

        errors.extend(take_config_errors());
        if !errors.is_empty() {
            let qmsg = QString::from(&errors.join("\n\n"));
            self.as_mut().error_occurred(qmsg);
        }

        // The restore completes when every plugin of the batch reported back
        // (PluginAdded, PluginSkipped or a fatal PluginError); this is a safety
        // cap, scaled with the batch since probes and adds run in sequence.
        if !self.rust().restore_instance_ids.is_empty() {
            if let Some(started) = self.rust().restore_started_at {
                let cap = restore_safety_cap(self.rust().restore_batch_size);
                if started.elapsed() > cap {
                    let remaining = self.rust().restore_instance_ids.len();
                    log::error!(
                        "Plugin restore timed out after {}s with {} plugin(s) still pending. \
                         Clearing restore state.",
                        cap.as_secs(),
                        remaining,
                    );
                    self.as_mut().rust_mut().restore_instance_ids.clear();
                    self.as_mut().rust_mut().restore_started_at = None;
                    self.as_mut().rust_mut().links_retry_due = true;
                    remove_crash_marker();
                    persist_active_plugins(self.rust().plugin_manager.as_ref());
                }
            }
        }

        self.as_mut().maybe_autosave_plugin_states();

        if changed {
            self.as_mut().refresh_cache();
            self.as_mut().sync_tray_plugins();
            self.as_mut().graph_changed();
        }

        for (instance_id, port_index, value) in param_updates {
            self.as_mut()
                .plugin_parameter_changed(instance_id, port_index, value);
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

    /// A plugin from the startup restore batch reported back (added or failed).
    fn finish_restore_of(mut self: Pin<&mut Self>, instance_id: u64) {
        if !self.as_mut().rust_mut().restore_instance_ids.remove(&instance_id) {
            return;
        }
        if self.rust().restore_instance_ids.is_empty() {
            log::info!("All plugins restored — will attempt link restoration");
            self.as_mut().rust_mut().restore_started_at = None;
            self.as_mut().rust_mut().links_retry_due = true;
            remove_crash_marker();
        }
    }

    fn mark_links_dirty(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().links_dirty = true;
        if self.rust().links_dirty_since.is_none() {
            self.as_mut().rust_mut().links_dirty_since = Some(Instant::now());
        }
    }

    /// Links to a plugin that was removed or renamed must not be remembered.
    fn forget_plugin_links(mut self: Pin<&mut Self>, name: &str) {
        let involves = |l: &SavedPluginLink| l.output_node_name == name || l.input_node_name == name;
        {
            let mut rust = self.as_mut().rust_mut();
            rust.forgotten_link_nodes.insert(name.to_string());
            rust.pending_links.retain(|p| !involves(&p.link));
            rust.last_graph_links.retain(|l| !involves(l));
        }
        self.as_mut().mark_links_dirty();
    }

    fn persist_links_now(mut self: Pin<&mut Self>) {
        let Some(graph) = self.rust().graph.clone() else {
            return;
        };
        let current = build_persistable_links(&graph);
        let all_nodes = graph.get_all_nodes();
        let present = |name: &str| node_name_present(&graph, &all_nodes, name);
        let forgotten = self.rust().forgotten_link_nodes.clone();
        let last = std::mem::take(&mut self.as_mut().rust_mut().last_graph_links);
        let mut pending = std::mem::take(&mut self.as_mut().rust_mut().pending_links);
        let merged = merge_links_for_persist(&current, &last, &mut pending, &forgotten, present);
        self.as_mut().rust_mut().pending_links = pending;
        self.as_mut().rust_mut().last_graph_links = current;
        write_saved_links(&merged);
    }

    /// Try to (re)connect remembered links whose endpoints are now present.
    fn retry_pending_links(mut self: Pin<&mut Self>) {
        let Some(graph) = self.rust().graph.clone() else {
            return;
        };
        let current = build_persistable_links(&graph);
        let all_nodes = graph.get_all_nodes();
        let mut pending = std::mem::take(&mut self.as_mut().rust_mut().pending_links);
        let before = pending.len();
        let mut connects: Vec<(u32, u32)> = Vec::new();
        pending.retain_mut(|p| {
            if current.contains(&p.link) {
                return false;
            }
            if let Some(sent) = p.connect_sent_at {
                if sent.elapsed() < LINK_CONNECT_GRACE {
                    return true;
                }
                if node_name_present(&graph, &all_nodes, &p.link.output_node_name)
                    && node_name_present(&graph, &all_nodes, &p.link.input_node_name)
                {
                    log::warn!(
                        "Saved link {}:{} -> {}:{} did not come up; forgetting it",
                        p.link.output_node_name,
                        p.link.output_port_name,
                        p.link.input_node_name,
                        p.link.input_port_name
                    );
                    return false;
                }
                p.connect_sent_at = None;
            }
            if let (Some(out_id), Some(in_id)) = find_saved_link_ports(&graph, &all_nodes, &p.link) {
                log::info!(
                    "Restoring link: {}:{} -> {}:{}",
                    p.link.output_node_name,
                    p.link.output_port_name,
                    p.link.input_node_name,
                    p.link.input_port_name
                );
                connects.push((out_id, in_id));
                p.connect_sent_at = Some(Instant::now());
            }
            true
        });
        let dropped = before != pending.len();
        self.as_mut().rust_mut().pending_links = pending;
        if let Some(ref tx) = self.rust().cmd_tx {
            for (output_port_id, input_port_id) in connects {
                let _ = tx.send(PwCommand::Connect {
                    output_port_id,
                    input_port_id,
                });
            }
        }
        if dropped {
            self.as_mut().mark_links_dirty();
        }
    }

    fn persist_midi(self: Pin<&mut Self>) {
        let mut all = self.rust().midi_mappings.clone();
        for m in &self.rust().dormant_midi_mappings {
            if !all.iter().any(|a| a.source == m.source || a.target == m.target) {
                all.push(m.clone());
            }
        }
        persist_midi_mappings(&all, self.rust().plugin_manager.as_ref());
    }

    /// Re-add mappings whose MIDI device has reappeared.
    fn revive_dormant_midi_mappings(mut self: Pin<&mut Self>) {
        if self.rust().dormant_midi_mappings.is_empty() {
            return;
        }
        let Some(graph) = self.rust().graph.clone() else {
            return;
        };
        let all_nodes = graph.get_all_nodes();
        let (back, still): (Vec<_>, Vec<_>) = std::mem::take(
            &mut self.as_mut().rust_mut().dormant_midi_mappings,
        )
        .into_iter()
        .partition(|m| node_name_present(&graph, &all_nodes, &m.source.device_name));
        self.as_mut().rust_mut().dormant_midi_mappings = still;
        if back.is_empty() {
            return;
        }
        log::info!("MIDI device back: re-adding {} mapping(s)", back.len());
        if let Some(ref tx) = self.rust().cmd_tx {
            for m in back {
                let _ = tx.send(PwCommand::AddMidiMapping(m));
            }
        }
    }

    fn suppress_link_persist(mut self: Pin<&mut Self>, for_how_long: Duration) {
        let until = Instant::now() + for_how_long;
        let current = self.rust().links_suppressed_until;
        if current.is_none_or(|t| t < until) {
            self.as_mut().rust_mut().links_suppressed_until = Some(until);
        }
    }

    fn links_persist_suppressed(&self) -> bool {
        self.rust().pw_reconnecting
            || self
                .rust()
                .links_suppressed_until
                .is_some_and(|t| Instant::now() < t)
    }

    /// Remove every MIDI mapping that targets `instance_id` (plugin removed).
    fn drop_plugin_midi_mappings(mut self: Pin<&mut Self>, instance_id: u64) {
        if let Some(ref tx) = self.rust().cmd_tx {
            let _ = tx.send(PwCommand::RemoveMidiMappingsForPlugin { instance_id });
        }
        let had_dormant = self
            .rust()
            .dormant_midi_mappings
            .iter()
            .any(|m| m.target.instance_id == instance_id);
        self.as_mut()
            .rust_mut()
            .dormant_midi_mappings
            .retain(|m| m.target.instance_id != instance_id);
        let removed: Vec<u32> = self
            .rust()
            .midi_mappings
            .iter()
            .filter(|m| m.target.instance_id == instance_id)
            .map(|m| m.target.port_index as u32)
            .collect();
        if removed.is_empty() {
            if had_dormant {
                self.as_mut().persist_midi();
            }
            return;
        }
        self.as_mut()
            .rust_mut()
            .midi_mappings
            .retain(|m| m.target.instance_id != instance_id);
        self.as_mut().persist_midi();
        for port in removed {
            self.as_mut().midi_mapping_removed(instance_id, port);
        }
    }

    fn apply_plugin_state(
        mut self: Pin<&mut Self>,
        instance_id: u64,
        lv2_state: Vec<crate::lv2::state::StateEntry>,
        blob: Option<Vec<u8>>,
    ) {
        if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager
            && let Some(info) = mgr.get_instance_mut(instance_id)
        {
            log::debug!(
                "Plugin state received for instance {}: {} LV2 entries, blob {} bytes",
                instance_id,
                lv2_state.len(),
                blob.as_ref().map(|b| b.len()).unwrap_or(0)
            );
            match info.format {
                crate::plugin::PluginFormat::Lv2 => info.lv2_state = lv2_state,
                _ => {
                    // Keep the previous blob if the plugin couldn't produce one.
                    if blob.is_some() {
                        info.plugin_state = blob;
                    }
                }
            }
        }
    }

    fn plugins_persistable(&self) -> bool {
        !crate::PLUGINS_FROZEN.load(std::sync::atomic::Ordering::SeqCst)
            && self
                .rust()
                .plugin_manager
                .as_ref()
                .is_some_and(|mgr| !mgr.active_instances().is_empty())
    }

    /// Periodically snapshot plugin state so a crash doesn't lose it.
    fn maybe_autosave_plugin_states(mut self: Pin<&mut Self>) {
        const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(60);
        const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
        if let Some(sent) = self.rust().state_request_sent_at {
            if sent.elapsed() < REQUEST_TIMEOUT {
                return;
            }
            log::warn!("Plugin state request timed out; will retry");
            self.as_mut().rust_mut().state_request_sent_at = None;
        }
        if self.rust().last_state_autosave.elapsed() < AUTOSAVE_INTERVAL
            || !self.rust().restore_instance_ids.is_empty()
            || !self.plugins_persistable()
        {
            return;
        }
        self.as_mut().rust_mut().last_state_autosave = Instant::now();
        if let Some(ref tx) = self.rust().cmd_tx
            && tx.send(PwCommand::RequestPluginStates).is_ok()
        {
            self.as_mut().rust_mut().state_request_sent_at = Some(Instant::now());
        }
    }

    /// Ask the PipeWire thread for every plugin's state and wait (bounded) for
    /// the replies. Other events received meanwhile are dropped — only used on quit.
    fn collect_plugin_states_blocking(mut self: Pin<&mut Self>, timeout: Duration) {
        let Some(rx) = self.as_mut().rust_mut().event_rx.take() else {
            return;
        };
        let sent = self
            .rust()
            .cmd_tx
            .as_ref()
            .is_some_and(|tx| tx.send(PwCommand::RequestPluginStates).is_ok());
        if sent {
            // An autosave request may still be in flight; its Done comes first.
            let mut dones_needed = if self.rust().state_request_sent_at.is_some() { 2 } else { 1 };
            let deadline = Instant::now() + timeout;
            while dones_needed > 0 {
                let now = Instant::now();
                if now >= deadline {
                    log::warn!("Timed out waiting for plugin states on quit");
                    break;
                }
                match rx.recv_timeout(deadline - now) {
                    Ok(PwEvent::Plugin(PluginEvent::PluginStateSaved {
                        instance_id,
                        lv2_state,
                        blob,
                    })) => {
                        self.as_mut().apply_plugin_state(instance_id, lv2_state, blob);
                    }
                    Ok(PwEvent::Plugin(PluginEvent::PluginStatesDone)) => dones_needed -= 1,
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            self.as_mut().rust_mut().state_request_sent_at = None;
        }
        self.as_mut().rust_mut().event_rx = Some(rx);
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

            // Rebuild port mappings each refresh; virtual node ids stay stable
            self.as_mut().rust_mut().bridge_split.clear_ports();

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
                    let display_name =
                        port_ui_name(p, self.rust().bridge_split.is_virtual_id(node_id));
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

    pub fn request_quit(mut self: Pin<&mut Self>) {
        log::info!("Quit requested");
        let frozen = crate::PLUGINS_FROZEN.load(std::sync::atomic::Ordering::SeqCst);
        if self.plugins_persistable() {
            self.as_mut().collect_plugin_states_blocking(Duration::from_secs(3));
        }
        let restore_complete = self.rust().restore_instance_ids.is_empty();
        remove_crash_marker();
        // While PipeWire is reconnecting the graph is empty/partial; links.json
        // already holds the last good state.
        if !self.links_persist_suppressed() {
            self.as_mut().persist_links_now();
        }
        persist_active_plugins(self.rust().plugin_manager.as_ref());
        // Only a session that really loaded plugins.json (no safe mode, no
        // parse failure, restore finished) may become the known-good snapshot.
        if !frozen && !self.rust().plugins_load_failed && restore_complete {
            save_known_good_plugins();
        }
        crate::lv2::ui::shutdown_gtk_thread();
        std::process::exit(0);
    }

    pub fn restore_known_good(self: Pin<&mut Self>) -> bool {
        // plugins.json is replaced on disk and loaded on the next normal start.
        // Plugins stay frozen for the rest of this session so nothing written
        // here (an empty plugin list, the known-good copy) can clobber it.
        if restore_known_good_plugins() {
            log::info!("Known-good plugins restored. Restart to load them.");
            true
        } else {
            false
        }
    }

    pub fn reassign_midi_mapping(mut self: Pin<&mut Self>) -> bool {
        let Some(mapping) = self.as_mut().rust_mut().pending_conflict_mapping.take() else {
            log::warn!("reassign_midi_mapping: no pending conflict");
            return false;
        };
        log::info!(
            "Reassigning MIDI {:?} to instance {} port {}",
            mapping.source,
            mapping.target.instance_id,
            mapping.target.port_index
        );
        if let Some(ref tx) = self.rust().cmd_tx {
            let _ = tx.send(PwCommand::RemoveMidiMapping(mapping.source.clone()));
            let _ = tx.send(PwCommand::AddMidiMapping(mapping));
            true
        } else {
            false
        }
    }

    pub fn dismiss_midi_conflict(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().pending_conflict_mapping = None;
    }

    pub fn get_layout_json(self: Pin<&mut Self>) -> QString {
        QString::from(&read_config_json_text("layout.json", "{}"))
    }

    pub fn save_layout(self: Pin<&mut Self>, json: QString) {
        write_config_json_text("layout.json", &json.to_string());
    }

    /// `node_sizes_json`: nodeId → [width, height] for every node shown in the
    /// UI (hidden nodes are omitted and therefore not laid out).
    /// `pinned_positions_json`: nodeId → [x, y]. Returns nodeId → [x, y].
    /// Pinned nodes keep their positions; free nodes are laid out.
    pub fn auto_layout(mut self: Pin<&mut Self>, node_sizes_json: QString, pinned_positions_json: QString) -> QString {
        use crate::layout;

        let sizes_str: String = node_sizes_json.to_string();
        let node_sizes: HashMap<String, Vec<f64>> =
            serde_json::from_str(&sizes_str).unwrap_or_default();
        let size_of = |id: u32| -> Option<(f64, f64)> {
            node_sizes
                .get(&id.to_string())
                .filter(|s| s.len() >= 2 && s[0].is_finite() && s[1].is_finite())
                .map(|s| (s[0], s[1]))
        };

        let pinned_str: String = pinned_positions_json.to_string();
        let pinned_raw: HashMap<String, Vec<f64>> =
            serde_json::from_str(&pinned_str).unwrap_or_default();

        let graph = match self.rust().graph.clone() {
            Some(g) => g,
            None => return QString::from("{}"),
        };

        // Sort everything by id: HashMap iteration order would otherwise make
        // the layout differ from run to run.
        let mut all_nodes = graph.get_all_nodes();
        all_nodes.sort_by_key(|n| n.id);
        let mut all_links = graph.get_all_links();
        all_links.sort_by_key(|l| l.id);

        let mut layout_nodes: Vec<(u32, String, &str, f64, f64)> = Vec::new();
        let mut layout_ports: Vec<(u32, u32, usize, bool)> = Vec::new();
        let mut layout_links: Vec<(u32, u32, u32, u32, u32)> = Vec::new();
        let mut included: std::collections::HashSet<u32> = std::collections::HashSet::new();

        // Phase 1: resolve bridge virtual IDs (needs mutable self for bridge_split)
        let mut bridge_vids: Vec<(u32, u32, String, String)> = Vec::new();
        for n in all_nodes.iter().filter(|n| n.ready && n.is_bridge) {
            let groups = graph.get_bridge_port_groups(n.id);
            for (group, device_name) in &groups {
                let vid = self.as_mut().rust_mut().bridge_split
                    .get_or_create_virtual_id(n.id, group);
                bridge_vids.push((n.id, vid, group.clone(), device_name.clone()));
            }
        }

        // Phase 2: build layout data
        for n in all_nodes.iter().filter(|n| n.ready) {
            let has_groups = n.is_bridge && bridge_vids.iter().any(|(rid, ..)| *rid == n.id);
            if has_groups {
                for (real_id, vid, group, device_name) in &bridge_vids {
                    if *real_id != n.id {
                        continue;
                    }
                    let Some((w, h)) = size_of(*vid) else { continue };

                    let mut group_ports = graph.get_ports_for_bridge_group(n.id, group);
                    let has_inputs = group_ports.iter().any(|p| p.direction == PortDirection::Input);
                    let has_outputs = group_ports.iter().any(|p| p.direction == PortDirection::Output);
                    let type_str = if has_inputs && has_outputs { "Duplex" }
                        else if has_outputs { "Source" }
                        else if has_inputs { "Sink" }
                        else { "Duplex" };

                    layout_nodes.push((*vid, device_name.clone(), type_str, w, h));
                    included.insert(*vid);
                    sort_ports_for_layout(&mut group_ports, true);
                    add_ports_to_layout(&group_ports, *vid, &mut layout_ports);
                }
            } else {
                let Some((w, h)) = size_of(n.id) else { continue };
                let type_str = node_type_str(n);
                layout_nodes.push((n.id, n.display_name().to_string(), type_str, w, h));
                included.insert(n.id);

                let mut ports = graph.get_ports_for_node(n.id);
                sort_ports_for_layout(&mut ports, false);
                add_ports_to_layout(&ports, n.id, &mut layout_ports);
            }
        }

        for l in &all_links {
            let out_node = self.rust().bridge_split
                .resolve_port_virtual_node(l.output_port_id)
                .unwrap_or(l.output_node_id);
            let in_node = self.rust().bridge_split
                .resolve_port_virtual_node(l.input_port_id)
                .unwrap_or(l.input_node_id);

            if included.contains(&out_node) && included.contains(&in_node) {
                layout_links.push((l.id, out_node, l.output_port_id, in_node, l.input_port_id));
            }
        }

        let config = layout::graph::LayoutConfig::default();
        let mut pinned_by_id: HashMap<u32, (f64, f64)> = HashMap::new();
        for (key, pos) in &pinned_raw {
            if pos.len() >= 2
                && let Ok(id) = key.parse::<u32>()
                && included.contains(&id)
            {
                pinned_by_id.insert(id, (pos[0], pos[1]));
            }
        }

        for &(id, ref name, type_str, w, h) in &layout_nodes {
            let pin_tag = if pinned_by_id.contains_key(&id) { " [PINNED]" } else { "" };
            log::debug!("  node {}: {}({}) {}x{}{}", id, name, type_str, w as i32, h as i32, pin_tag);
        }
        log::info!("auto_layout: {} nodes ({} pinned), {} links",
            layout_nodes.len(), pinned_by_id.len(), layout_links.len());

        let positions = layout::sugiyama_layout(layout_nodes, layout_ports, layout_links, config, &pinned_by_id);

        let mut result: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        for (node_id, (x, y)) in &positions {
            if included.contains(node_id) && x.is_finite() && y.is_finite() {
                result.insert(node_id.to_string(), serde_json::json!([x, y]));
            }
        }

        let json = serde_json::to_string(&serde_json::Value::Object(result)).unwrap_or_else(|_| "{}".to_string());
        log::info!("auto_layout: computed {} positions", positions.len());
        QString::from(&json)
    }

    pub fn get_hidden_json(self: Pin<&mut Self>) -> QString {
        QString::from(&read_config_json_text("hidden.json", "[]"))
    }

    pub fn save_hidden(self: Pin<&mut Self>, json: QString) {
        write_config_json_text("hidden.json", &json.to_string());
    }

    pub fn get_pinned_json(self: Pin<&mut Self>) -> QString {
        QString::from(&read_config_json_text("pinned.json", "[]"))
    }

    pub fn save_pinned(self: Pin<&mut Self>, json: QString) {
        write_config_json_text("pinned.json", &json.to_string());
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
                            is_toggle: port.is_toggle,
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

        let stable_id = uuid::Uuid::new_v4().to_string();
        if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
            let info = crate::lv2::Lv2InstanceInfo {
                id: instance_id,
                stable_id: stable_id.clone(),
                plugin_uri: uri_str.clone(),
                format: plugin_format,
                display_name: display_name.clone(),
                pw_node_id: None,
                parameters: initial_params,
                active: true,
                bypassed: false,
                lv2_state: Vec::new(),
                plugin_state: None,
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
                lv2_state: Vec::new(),
                plugin_state: None,
                stable_id,
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
            let old_name = self
                .rust()
                .plugin_manager
                .as_ref()
                .and_then(|mgr| mgr.get_instance(instance_id))
                .map(|info| info.display_name.clone());
            if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager
                && let Some(info) = mgr.get_instance_mut(instance_id)
            {
                info.display_name = name_str.clone();
            }
            if let Some(ref graph) = self.rust().graph {
                graph.set_node_description(node_id, &name_str);
            }
            // Links are saved by display name: drop the old-name entries, the
            // current links are re-saved under the new name.
            if let Some(old_name) = old_name
                && old_name != name_str
            {
                self.as_mut().forget_plugin_links(&old_name);
            }
            self.as_mut().rust_mut().forgotten_link_nodes.remove(&name_str);
            self.as_mut().mark_links_dirty();
            persist_active_plugins(self.rust().plugin_manager.as_ref());
            self.as_mut().graph_changed();
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
                        "isToggle": p.is_toggle,
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
            // Let the graph view pick up the new bypass state.
            self.as_mut().graph_changed();
        }
    }

    pub fn start_midi_learn(
        mut self: Pin<&mut Self>,
        instance_id: u64,
        port_index: u32,
        label: QString,
        mode: QString,
    ) {
        let mode_str = mode.to_string();
        let mapping_mode = match mode_str.as_str() {
            "toggle" => crate::midi::MappingMode::Toggle,
            "momentary" => crate::midi::MappingMode::Momentary,
            _ => crate::midi::MappingMode::Continuous,
        };
        let label_str = label.to_string();
        self.as_mut().rust_mut().midi_learn_target = Some((
            instance_id,
            port_index as usize,
            label_str.clone(),
            mapping_mode,
        ));
        if let Some(ref tx) = self.rust().cmd_tx {
            let _ = tx.send(PwCommand::StartMidiLearn {
                instance_id,
                port_index: port_index as usize,
                label: label_str,
                mode: mapping_mode,
            });
        }
    }

    pub fn cancel_midi_learn(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().midi_learn_target = None;
        if let Some(ref tx) = self.rust().cmd_tx {
            let _ = tx.send(PwCommand::CancelMidiLearn);
        }
    }

    pub fn remove_midi_mapping_for_param(
        mut self: Pin<&mut Self>,
        instance_id: u64,
        port_index: u32,
    ) {
        let target = crate::midi::MidiCcTarget {
            instance_id,
            port_index: port_index as usize,
        };
        if self.rust().dormant_midi_mappings.iter().any(|m| m.target == target) {
            self.as_mut()
                .rust_mut()
                .dormant_midi_mappings
                .retain(|m| m.target != target);
            self.as_mut().persist_midi();
        }
        let source = self.rust().midi_mappings
            .iter()
            .find(|m| m.target == target)
            .map(|m| m.source.clone());
        if let Some(source) = source {
            if let Some(ref tx) = self.rust().cmd_tx {
                let _ = tx.send(PwCommand::RemoveMidiMapping(source));
            }
        }
    }

    pub fn get_midi_mappings_json(self: Pin<&mut Self>) -> QString {
        let json = serde_json::to_string(&self.rust().midi_mappings).unwrap_or_else(|_| "[]".to_string());
        QString::from(&json)
    }

    pub fn get_midi_mapping_for_param_json(
        self: Pin<&mut Self>,
        instance_id: u64,
        port_index: u32,
    ) -> QString {
        let target = crate::midi::MidiCcTarget {
            instance_id,
            port_index: port_index as usize,
        };
        let mapping = self.rust().midi_mappings
            .iter()
            .find(|m| m.target == target);
        match mapping {
            Some(m) => {
                let json = serde_json::to_string(m).unwrap_or_else(|_| "{}".to_string());
                QString::from(&json)
            }
            None => QString::from(""),
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
                                "isToggle": p.is_toggle,
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
            let name = self
                .rust()
                .plugin_manager
                .as_ref()
                .and_then(|mgr| mgr.get_instance(instance_id))
                .map(|info| info.display_name.clone());
            if let Some(name) = name {
                self.as_mut().forget_plugin_links(&name);
            }
            if let Some(ref mut mgr) = self.as_mut().rust_mut().plugin_manager {
                mgr.remove_instance(instance_id);
            }
            self.as_mut().drop_plugin_midi_mappings(instance_id);
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
        QString::from(&read_config_json_text("window.json", "{}"))
    }

    pub fn save_window_geometry(self: Pin<&mut Self>, json: QString) {
        write_config_json_text("window.json", &json.to_string());
    }

    pub fn get_viewport_json(self: Pin<&mut Self>) -> QString {
        QString::from(&read_config_json_text("viewport.json", "{}"))
    }

    pub fn save_viewport(self: Pin<&mut Self>, json: QString) {
        write_config_json_text("viewport.json", &json.to_string());
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
                    "workerPercent": (snap.worker_percent * 100.0).round() / 100.0,
                    "workerAvgUs": snap.worker_avg_ns / 1000,
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
        if key.is_empty() {
            let _ = std::fs::remove_file(&path);
            log::info!("Cleared default node");
        } else if write_atomic(&path, key.as_bytes()).is_ok() {
            log::info!("Set default node: {}", key);
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

    pub fn get_app_version(self: Pin<&mut Self>) -> QString {
        QString::from(env!("CARGO_PKG_VERSION"))
    }

    pub fn get_qt_version(self: Pin<&mut Self>) -> QString {
        QString::from(env!("QT_VERSION"))
    }

    pub fn backup_rules(self: Pin<&mut Self>, name: QString) -> QString {
        let name_str: String = name.to_string();
        let backup_dir = config_path("rule_backups");
        if let Err(e) = std::fs::create_dir_all(&backup_dir) {
            log::error!("Failed to create backup dir: {}", e);
            return QString::from("");
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // Format as YYYYMMDD_HHMMSS using simple arithmetic (UTC)
        let secs = now as i64;
        let days = secs / 86400;
        let time_of_day = (secs % 86400) as u32;
        let hours = time_of_day / 3600;
        let minutes = (time_of_day % 3600) / 60;
        let seconds = time_of_day % 60;
        // Compute date from days since epoch (1970-01-01)
        let (year, month, day) = days_to_ymd(days);
        let timestamp = format!("{:04}{:02}{:02}_{:02}{:02}{:02}", year, month, day, hours, minutes, seconds);
        let safe_name = if name_str.trim().is_empty() {
            timestamp.clone()
        } else {
            let sanitized: String = name_str
                .trim()
                .chars()
                .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' })
                .collect();
            format!("{}_{}", timestamp, sanitized)
        };
        let filename = format!("{}.json", safe_name);
        let dest = backup_dir.join(&filename);

        let src = config_path("rules.json");
        match std::fs::read_to_string(&src) {
            Ok(content) => {
                if write_atomic(&dest, content.as_bytes()).is_err() {
                    return QString::from("");
                }
                log::info!("Rules backed up to {:?}", dest);
                QString::from(&filename)
            }
            Err(e) => {
                log::error!("Failed to read rules for backup: {}", e);
                QString::from("")
            }
        }
    }

    pub fn list_rule_backups_json(self: Pin<&mut Self>) -> QString {
        let backup_dir = config_path("rule_backups");
        let mut backups: Vec<serde_json::Value> = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&backup_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    let filename = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string();

                    // Parse rule count from the file
                    let rule_count = std::fs::read_to_string(&path)
                        .ok()
                        .and_then(|s| serde_json::from_str::<Vec<serde_json::Value>>(&s).ok())
                        .map(|v| v.len())
                        .unwrap_or(0);

                    // Format: YYYYMMDD_HHMMSS[_OptionalName].json
                    let stem = filename.trim_end_matches(".json");
                    let (parsed_date, display_name) = parse_backup_stem(stem);

                    let date_str = if let Some(d) = parsed_date {
                        d
                    } else {
                        // Fallback to file modification time
                        entry
                            .metadata()
                            .ok()
                            .and_then(|m| m.modified().ok())
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|dur| {
                                let secs = dur.as_secs() as i64;
                                let tod = (secs % 86400) as u32;
                                let (y, mo, d) = days_to_ymd(secs / 86400);
                                format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                                    y, mo, d, tod / 3600, (tod % 3600) / 60, tod % 60)
                            })
                            .unwrap_or_default()
                    };

                    backups.push(serde_json::json!({
                        "filename": filename,
                        "name": display_name,
                        "date": date_str,
                        "ruleCount": rule_count,
                    }));
                }
            }
        }

        // Sort newest first
        backups.sort_by(|a, b| {
            let fa = a["filename"].as_str().unwrap_or("");
            let fb = b["filename"].as_str().unwrap_or("");
            fb.cmp(fa)
        });

        let json = serde_json::to_string(&backups).unwrap_or_else(|_| "[]".to_string());
        QString::from(&json)
    }

    pub fn restore_rule_backup(mut self: Pin<&mut Self>, filename: QString) {
        let filename_str: String = filename.to_string();
        if !is_plain_file_name(&filename_str) {
            log::error!("restore_rule_backup: invalid backup name {:?}", filename_str);
            return;
        }
        let backup_path = config_path("rule_backups").join(&filename_str);

        match std::fs::read_to_string(&backup_path) {
            Ok(content) => {
                // Validate it parses as rules
                match serde_json::from_str::<Vec<crate::patchbay::rules::AutoConnectRule>>(&content) {
                    Ok(rules) => {
                        // Write to rules.json
                        let rules_path = config_path("rules.json");
                        if write_atomic(&rules_path, content.as_bytes()).is_err() {
                            return;
                        }
                        // Load into patchbay manager
                        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
                            patchbay.set_rules(rules.clone());
                        }
                        log::info!("Restored {} rules from backup {:?}", rules.len(), filename_str);
                    }
                    Err(e) => {
                        log::error!("Backup file {:?} contains invalid rules: {}", filename_str, e);
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to read backup {:?}: {}", backup_path, e);
            }
        }
    }

    pub fn delete_rule_backup(self: Pin<&mut Self>, filename: QString) {
        let filename_str: String = filename.to_string();
        if !is_plain_file_name(&filename_str) {
            log::error!("delete_rule_backup: invalid backup name {:?}", filename_str);
            return;
        }
        let backup_path = config_path("rule_backups").join(&filename_str);
        if let Err(e) = std::fs::remove_file(&backup_path) {
            log::error!("Failed to delete backup {:?}: {}", backup_path, e);
        } else {
            log::info!("Deleted rule backup: {}", filename_str);
        }
    }

    pub fn set_window_visible(self: Pin<&mut Self>, visible: bool) {
        if let Some(ref tray) = self.rust().tray_state {
            use std::sync::atomic::Ordering;
            tray.window_visible.store(visible, Ordering::Release);
            log::info!("Window visible state updated to {}", visible);
        }
    }

    fn sync_tray_plugins(self: Pin<&mut Self>) {
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
                    .find_plugin(&info.plugin_uri)
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

        if let Ok(mut plugins) = tray.plugins.lock() {
            *plugins = entries;
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

/// Convert days since Unix epoch to (year, month, day).
fn days_to_ymd(days: i64) -> (i64, u32, u32) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // year of era [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn config_path(filename: &str) -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("zestbay")
        .join(filename)
}

// ---------------------------------------------------------------------------
// Config file robustness: atomic writes, corrupt-file backups, error surfacing
// ---------------------------------------------------------------------------

/// Errors from loading/saving config files, surfaced to the user by poll_events.
static CONFIG_ERRORS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Config files that failed to parse this session. An empty list/object is
/// never written over them (the user's data would otherwise silently vanish);
/// real content may be written once a backup exists.
static LOAD_FAILED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Config files that failed to parse AND could not be backed up: never written.
static SAVE_BLOCKED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

fn push_config_error(msg: String) {
    log::error!("{}", msg);
    if let Ok(mut errs) = CONFIG_ERRORS.lock() {
        errs.push(msg);
    }
}

fn take_config_errors() -> Vec<String> {
    CONFIG_ERRORS
        .lock()
        .map(|mut errs| std::mem::take(&mut *errs))
        .unwrap_or_default()
}

fn list_contains(list: &std::sync::Mutex<Vec<String>>, name: &str) -> bool {
    list.lock().map(|l| l.iter().any(|n| n == name)).unwrap_or(false)
}

fn list_set(list: &std::sync::Mutex<Vec<String>>, name: &str, present: bool) {
    if let Ok(mut l) = list.lock() {
        l.retain(|n| n != name);
        if present {
            l.push(name.to_string());
        }
    }
}

/// UTC timestamp `YYYYMMDD_HHMMSS`.
fn timestamp_string() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let tod = (secs % 86400) as u32;
    let (y, mo, d) = days_to_ymd(secs / 86400);
    format!(
        "{:04}{:02}{:02}_{:02}{:02}{:02}",
        y,
        mo,
        d,
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

/// Write `data` to `path` atomically: temp file in the same directory, fsync,
/// rename over the target, fsync the directory.
fn write_atomic(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let result = (|| {
        let dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."));
        std::fs::create_dir_all(dir)?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "config".to_string());
        let tmp = dir.join(format!(".{}.tmp-{}", name, std::process::id()));
        {
            let mut f = std::fs::File::create(&tmp)?;
            if let Err(e) = f.write_all(data).and_then(|_| f.sync_all()) {
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
        }
        if let Err(e) = std::fs::rename(&tmp, path) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
        if let Ok(d) = std::fs::File::open(dir) {
            let _ = d.sync_all();
        }
        Ok(())
    })();
    if let Err(ref e) = result {
        log::error!("Failed to write {:?}: {}", path, e);
    }
    result
}

/// Write a config file (by name). `is_empty` marks content that holds no user
/// data — it is not written over a file that failed to load this session.
fn write_config(filename: &str, data: &[u8], is_empty: bool) -> bool {
    if list_contains(&SAVE_BLOCKED, filename) {
        log::warn!("Not saving {}: it failed to load and could not be backed up", filename);
        return false;
    }
    if list_contains(&LOAD_FAILED, filename) {
        if is_empty {
            log::warn!("Not overwriting {} (failed to load) with empty content", filename);
            return false;
        }
        list_set(&LOAD_FAILED, filename, false);
    }
    write_atomic(&config_path(filename), data).is_ok()
}

/// Copy a config file that failed to parse to `<name>.corrupt-<timestamp>`.
fn backup_corrupt_file(filename: &str, err: &dyn std::fmt::Display) {
    let path = config_path(filename);
    let backup = config_path(&format!("{}.corrupt-{}", filename, timestamp_string()));
    match std::fs::copy(&path, &backup) {
        Ok(_) => {
            list_set(&LOAD_FAILED, filename, true);
            push_config_error(format!(
                "{} could not be read ({}). A copy was saved as {}.",
                filename,
                err,
                backup.display()
            ));
        }
        Err(e) => {
            list_set(&SAVE_BLOCKED, filename, true);
            push_config_error(format!(
                "{} could not be read ({}) and could not be backed up ({}). \
                 It will not be overwritten this session.",
                filename, err, e
            ));
        }
    }
}

/// Load a JSON config file. `Ok(None)`: the file doesn't exist. `Err(())`: it
/// exists but couldn't be read/parsed — it has been backed up and the error
/// queued for the user.
fn load_config<T: serde::de::DeserializeOwned>(filename: &str) -> Result<Option<T>, ()> {
    let path = config_path(filename);
    let text = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            list_set(&SAVE_BLOCKED, filename, true);
            push_config_error(format!("{} could not be read: {}", filename, e));
            return Err(());
        }
    };
    match serde_json::from_str::<T>(&text) {
        Ok(v) => Ok(Some(v)),
        Err(e) => {
            backup_corrupt_file(filename, &e);
            Err(())
        }
    }
}

/// Raw JSON text of a config file owned by QML (layout, hidden, ...), or
/// `default` when missing/invalid (invalid files are backed up first).
fn read_config_json_text(filename: &str, default: &str) -> String {
    match load_config::<serde_json::Value>(filename) {
        Ok(Some(v)) => serde_json::to_string(&v).unwrap_or_else(|_| default.to_string()),
        _ => default.to_string(),
    }
}

fn write_config_json_text(filename: &str, json: &str) {
    let parsed = match serde_json::from_str::<serde_json::Value>(json) {
        Ok(v) => v,
        Err(e) => {
            log::error!("Refusing to save invalid JSON to {}: {}", filename, e);
            return;
        }
    };
    let is_empty = match &parsed {
        serde_json::Value::Array(a) => a.is_empty(),
        serde_json::Value::Object(o) => o.is_empty(),
        _ => false,
    };
    write_config(filename, json.as_bytes(), is_empty);
}

fn crash_marker_path() -> PathBuf {
    config_path(".zestbay-restoring")
}

fn write_crash_marker(plugin_uris: &[String]) {
    let content = plugin_uris.join("\n");
    let _ = write_atomic(&crash_marker_path(), content.as_bytes());
}

fn remove_crash_marker() {
    let path = crash_marker_path();
    let _ = std::fs::remove_file(&path);
}

fn has_crash_marker() -> bool {
    crash_marker_path().exists()
}

fn read_crash_marker() -> Vec<String> {
    let path = crash_marker_path();
    match std::fs::read_to_string(&path) {
        Ok(s) => s.lines().filter(|l| !l.is_empty()).map(String::from).collect(),
        Err(_) => Vec::new(),
    }
}

fn known_good_plugins_path() -> PathBuf {
    config_path("plugins.known_good.json")
}

/// Copy `src` over `dst` atomically, but only if `src` holds a valid plugin list.
fn copy_plugins_file(src: &std::path::Path, dst: &std::path::Path) -> bool {
    let content = match std::fs::read(src) {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to read {:?}: {}", src, e);
            return false;
        }
    };
    if let Err(e) = serde_json::from_slice::<Vec<SavedPlugin>>(&content) {
        log::error!("{:?} is not a valid plugin list ({}); not copying", src, e);
        return false;
    }
    write_atomic(dst, &content).is_ok()
}

fn save_known_good_plugins() {
    let src = config_path("plugins.json");
    let dst = known_good_plugins_path();
    if src.exists() && copy_plugins_file(&src, &dst) {
        log::info!("Saved known-good plugins snapshot to {:?}", dst);
    }
}

fn has_known_good_plugins() -> bool {
    known_good_plugins_path().exists()
}

fn restore_known_good_plugins() -> bool {
    let src = known_good_plugins_path();
    let dst = config_path("plugins.json");
    if !src.exists() {
        log::warn!("No known-good plugins snapshot to restore");
        return false;
    }
    if copy_plugins_file(&src, &dst) {
        log::info!("Restored plugins.json from known-good snapshot");
        list_set(&LOAD_FAILED, "plugins.json", false);
        true
    } else {
        log::error!("Failed to restore known-good plugins");
        false
    }
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    lv2_state: Vec<crate::lv2::state::StateEntry>,
    /// CLAP/VST3 state blob, base64.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    plugin_state: Option<String>,
}

impl SavedPlugin {
    fn decode_plugin_state(&self) -> Option<Vec<u8>> {
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
struct SavedPluginParam {
    port_index: usize,
    symbol: String,
    value: f32,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
struct SavedPluginLink {
    output_node_name: String,
    output_port_name: String,
    input_node_name: String,
    input_port_name: String,
}

/// A saved link that isn't in the graph right now (endpoint absent, or a
/// connect was just requested).
#[derive(Clone, Debug)]
struct PendingLink {
    link: SavedPluginLink,
    connect_sent_at: Option<Instant>,
}

impl PendingLink {
    fn new(link: SavedPluginLink) -> Self {
        Self {
            link,
            connect_sent_at: None,
        }
    }
}

/// How long a requested connect may take to show up before we give up on it.
const LINK_CONNECT_GRACE: Duration = Duration::from_secs(5);

/// After a PipeWire reconnect or in-place plugin reload, links aren't saved
/// until the graph has had this long to come back.
const LINK_SETTLE_AFTER_RECONNECT: Duration = Duration::from_secs(5);

fn default_lv2_format_str() -> String {
    "LV2".to_string()
}

fn load_saved_plugins() -> Result<Vec<SavedPlugin>, ()> {
    load_config::<Vec<SavedPlugin>>("plugins.json").map(Option::unwrap_or_default)
}

/// Upper bound on how long the startup restore may take before it is
/// considered finished anyway: 30 s plus 15 s per plugin in the batch.
fn restore_safety_cap(batch_size: usize) -> Duration {
    Duration::from_secs(30 + 15 * batch_size as u64)
}

fn persist_active_plugins(plugin_manager: Option<&PluginManager>) {
    if crate::PLUGINS_FROZEN.load(std::sync::atomic::Ordering::SeqCst) {
        log::info!("persist_active_plugins: skipped (plugins frozen in safe mode)");
        return;
    }
    use base64::Engine;
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
                    lv2_state: info.lv2_state.clone(),
                    plugin_state: info
                        .plugin_state
                        .as_ref()
                        .map(|blob| base64::engine::general_purpose::STANDARD.encode(blob)),
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    plugins.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    let json = serde_json::to_string_pretty(&plugins).unwrap_or_default();
    if write_config("plugins.json", json.as_bytes(), plugins.is_empty()) {
        log::debug!("persist_active_plugins: {} plugins written", plugins.len());
    }
}

fn load_saved_links() -> Vec<SavedPluginLink> {
    load_config::<Vec<SavedPluginLink>>("links.json")
        .ok()
        .flatten()
        .unwrap_or_default()
}

fn write_saved_links(links: &[SavedPluginLink]) {
    let json = serde_json::to_string_pretty(links).unwrap_or_default();
    if write_config("links.json", json.as_bytes(), links.is_empty()) {
        log::debug!("persist links: {} links written", links.len());
    }
}

/// On-disk form of a MIDI mapping: the per-session instance id is useless
/// after a restart, so the plugin's stable id is stored alongside it.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct SavedMidiMapping {
    #[serde(flatten)]
    mapping: crate::midi::MidiCcMapping,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    stable_id: String,
}

fn load_midi_mappings() -> Vec<SavedMidiMapping> {
    load_config::<Vec<SavedMidiMapping>>("midi_mappings.json")
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// Map saved MIDI mappings onto this session's instance ids. Mappings whose
/// plugin no longer exists are dropped. Legacy entries (no stable id) carry
/// the previous session's instance id, which was assigned 1..N in plugins.json
/// order — `restore_order[k-1]` is this session's id for old id k.
fn resolve_saved_midi_mappings(
    saved: Vec<SavedMidiMapping>,
    stable_to_instance: &HashMap<String, u64>,
    restore_order: &[u64],
) -> Vec<crate::midi::MidiCcMapping> {
    let mut out: Vec<crate::midi::MidiCcMapping> = Vec::new();
    for s in saved {
        let mut mapping = s.mapping;
        let instance_id = if !s.stable_id.is_empty() {
            match stable_to_instance.get(&s.stable_id) {
                Some(&id) => id,
                None => {
                    log::warn!(
                        "Dropping MIDI mapping '{}': plugin {} no longer exists",
                        mapping.label,
                        s.stable_id
                    );
                    continue;
                }
            }
        } else {
            let old = mapping.target.instance_id;
            match old
                .checked_sub(1)
                .and_then(|i| restore_order.get(i as usize))
            {
                Some(&id) => {
                    log::warn!(
                        "Legacy MIDI mapping '{}': mapping old instance {} to {} by restore order",
                        mapping.label,
                        old,
                        id
                    );
                    id
                }
                None => {
                    log::warn!(
                        "Dropping legacy MIDI mapping '{}': unknown old instance {}",
                        mapping.label,
                        old
                    );
                    continue;
                }
            }
        };
        mapping.target.instance_id = instance_id;
        // One mapping per source and per target; later entries win.
        out.retain(|m| m.source != mapping.source && m.target != mapping.target);
        out.push(mapping);
    }
    out
}

fn persist_midi_mappings(
    mappings: &[crate::midi::MidiCcMapping],
    plugin_manager: Option<&PluginManager>,
) {
    if crate::PLUGINS_FROZEN.load(std::sync::atomic::Ordering::SeqCst) {
        log::info!("persist_midi_mappings: skipped (plugins frozen in safe mode)");
        return;
    }
    let saved: Vec<SavedMidiMapping> = mappings
        .iter()
        .filter_map(|m| {
            let stable_id = plugin_manager
                .and_then(|mgr| mgr.get_instance(m.target.instance_id))
                .map(|info| info.stable_id.clone())
                .filter(|sid| !sid.is_empty());
            match stable_id {
                Some(stable_id) => Some(SavedMidiMapping {
                    mapping: m.clone(),
                    stable_id,
                }),
                None => {
                    log::warn!(
                        "Not saving MIDI mapping '{}': plugin instance {} unknown",
                        m.label,
                        m.target.instance_id
                    );
                    None
                }
            }
        })
        .collect();
    let json = serde_json::to_string_pretty(&saved).unwrap_or_default();
    if write_config("midi_mappings.json", json.as_bytes(), saved.is_empty()) {
        log::debug!("persist_midi_mappings: {} mappings written", saved.len());
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
            let saved = SavedPluginLink {
                output_node_name: out_name,
                output_port_name: out_port.name.clone(),
                input_node_name: in_name,
                input_port_name: in_port.name.clone(),
            };
            if !saved_links.contains(&saved) {
                saved_links.push(saved);
            }
        }
    }

    saved_links.sort_by(|a, b| {
        (&a.output_node_name, &a.output_port_name, &a.input_node_name, &a.input_port_name).cmp(&(
            &b.output_node_name,
            &b.output_port_name,
            &b.input_node_name,
            &b.input_port_name,
        ))
    });
    saved_links
}

/// Is a node (or bridge sub-device) with this saved name in the graph?
fn node_name_present(graph: &GraphState, all_nodes: &[Node], name: &str) -> bool {
    all_nodes.iter().any(|n| {
        n.display_name() == name
            || (n.is_bridge
                && graph
                    .get_bridge_port_groups(n.id)
                    .values()
                    .any(|device| device == name))
    })
}

fn find_saved_link_port(
    graph: &GraphState,
    all_nodes: &[Node],
    node_name: &str,
    port_name: &str,
    direction: PortDirection,
) -> Option<u32> {
    // First try matching by node display name
    for n in all_nodes.iter().filter(|n| n.display_name() == node_name) {
        let ports = graph.get_ports_for_node(n.id);
        if let Some(p) = ports
            .iter()
            .find(|p| p.name == port_name && p.direction == direction)
        {
            return Some(p.id);
        }
    }
    // Then bridge sub-nodes by device name (from port.alias)
    for n in all_nodes.iter().filter(|n| n.is_bridge) {
        for (group, device_name) in &graph.get_bridge_port_groups(n.id) {
            if device_name != node_name {
                continue;
            }
            let ports = graph.get_ports_for_bridge_group(n.id, group);
            if let Some(p) = ports
                .iter()
                .find(|p| p.name == port_name && p.direction == direction)
            {
                return Some(p.id);
            }
        }
    }
    None
}

fn find_saved_link_ports(
    graph: &GraphState,
    all_nodes: &[Node],
    link: &SavedPluginLink,
) -> (Option<u32>, Option<u32>) {
    (
        find_saved_link_port(
            graph,
            all_nodes,
            &link.output_node_name,
            &link.output_port_name,
            PortDirection::Output,
        ),
        find_saved_link_port(
            graph,
            all_nodes,
            &link.input_node_name,
            &link.input_port_name,
            PortDirection::Input,
        ),
    )
}

/// Compute the link list to persist: the links in the graph now, plus every
/// remembered link that can't be in the graph because an endpoint is absent.
/// Links that were in the graph last time and vanished because an endpoint
/// went away are remembered (moved to `pending`); links that vanished while
/// both endpoints are still present were removed on purpose and are dropped.
fn merge_links_for_persist(
    current: &[SavedPluginLink],
    last_current: &[SavedPluginLink],
    pending: &mut Vec<PendingLink>,
    forgotten: &std::collections::HashSet<String>,
    node_present: impl Fn(&str) -> bool,
) -> Vec<SavedPluginLink> {
    let is_forgotten = |l: &SavedPluginLink| {
        forgotten.contains(&l.output_node_name) || forgotten.contains(&l.input_node_name)
    };
    pending.retain(|p| !current.contains(&p.link) && !is_forgotten(&p.link));
    for l in last_current {
        if current.contains(l) || is_forgotten(l) || pending.iter().any(|p| p.link == *l) {
            continue;
        }
        if !node_present(&l.output_node_name) || !node_present(&l.input_node_name) {
            pending.push(PendingLink::new(l.clone()));
        }
    }
    let mut merged: Vec<SavedPluginLink> = current.to_vec();
    for p in pending.iter() {
        if !merged.contains(&p.link) {
            merged.push(p.link.clone());
        }
    }
    merged
}

fn load_rules() -> Vec<crate::patchbay::rules::AutoConnectRule> {
    load_config::<Vec<crate::patchbay::rules::AutoConnectRule>>("rules.json")
        .ok()
        .flatten()
        .unwrap_or_default()
}

fn save_rules(patchbay: Option<&PatchbayManager>) {
    let rules: Vec<crate::patchbay::rules::AutoConnectRule> = if let Some(mgr) = patchbay {
        mgr.rules().to_vec()
    } else {
        Vec::new()
    };
    let json = serde_json::to_string_pretty(&rules).unwrap_or_default();
    if write_config("rules.json", json.as_bytes(), rules.is_empty()) {
        log::debug!("save_rules: {} rules written", rules.len());
    }
}

/// A backup name from QML must be a bare file name inside rule_backups.
fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains('\0')
}

/// Parse a rule backup stem `YYYYMMDD_HHMMSS[_Name]` into a display date and
/// name. Never panics on arbitrary (non-ASCII) file names.
fn parse_backup_stem(stem: &str) -> (Option<String>, String) {
    let b = stem.as_bytes();
    let digits = |r: std::ops::Range<usize>| b.get(r).is_some_and(|s| s.iter().all(u8::is_ascii_digit));
    if b.len() >= 15 && digits(0..8) && b[8] == b'_' && digits(9..15) {
        // Bytes 0..15 are ASCII, so these are char boundaries.
        let date = format!(
            "{}-{}-{} {}:{}:{}",
            &stem[0..4],
            &stem[4..6],
            &stem[6..8],
            &stem[9..11],
            &stem[11..13],
            &stem[13..15]
        );
        let name = if b.get(15) == Some(&b'_') {
            stem.get(16..).unwrap_or("").to_string()
        } else {
            String::new()
        };
        (Some(date), name)
    } else {
        (None, String::new())
    }
}

/// Order ports like the graph view draws them: MIDI first, then natural name.
fn sort_ports_for_layout(ports: &mut [Port], bridge_sub_node: bool) {
    ports.sort_by(|a, b| {
        let am = a.media_type != Some(crate::pipewire::MediaType::Midi);
        let bm = b.media_type != Some(crate::pipewire::MediaType::Midi);
        am.cmp(&bm)
            .then_with(|| {
                crate::pipewire::state::natural_cmp(
                    &port_ui_name(a, bridge_sub_node),
                    &port_ui_name(b, bridge_sub_node),
                )
            })
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// Port name as shown in the UI (bridge sub-node ports use their alias).
fn port_ui_name(p: &Port, bridge_sub_node: bool) -> String {
    if bridge_sub_node
        && let Some(ref alias) = p.port_alias
        && let Some(colon_pos) = alias.find(':')
    {
        return alias[colon_pos + 1..].trim().to_string();
    }
    p.display_name().to_string()
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
                val["pluginBypassed"] = serde_json::json!(instance.bypassed);
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

fn node_type_str(n: &Node) -> &'static str {
    match n.node_type {
        Some(NodeType::Sink) => "Sink",
        Some(NodeType::Source) => "Source",
        Some(NodeType::StreamOutput) => "StreamOutput",
        Some(NodeType::StreamInput) => "StreamInput",
        Some(NodeType::Duplex) => "Duplex",
        Some(NodeType::Plugin) => "Plugin",
        None => "Unknown",
    }
}

fn add_ports_to_layout(
    ports: &[crate::pipewire::Port],
    node_id: u32,
    layout_ports: &mut Vec<(u32, u32, usize, bool)>,
) {
    let mut input_idx = 0usize;
    let mut output_idx = 0usize;
    for p in ports {
        let is_output = p.direction == crate::pipewire::PortDirection::Output;
        let port_index = if is_output {
            let idx = output_idx;
            output_idx += 1;
            idx
        } else {
            let idx = input_idx;
            input_idx += 1;
            idx
        };
        layout_ports.push((p.id, node_id, port_index, is_output));
    }
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
    load_config::<Preferences>("preferences.json")
        .ok()
        .flatten()
        .unwrap_or_default()
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
    let json = serde_json::to_string_pretty(prefs).unwrap_or_default();
    if write_config("preferences.json", json.as_bytes(), false) {
        log::debug!("save_preferences: written");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapping(instance_id: u64, port_index: usize, cc: u8) -> crate::midi::MidiCcMapping {
        crate::midi::MidiCcMapping {
            source: crate::midi::MidiCcSource {
                device_name: "dev".into(),
                channel: Some(0),
                cc,
                message_type: crate::midi::MidiMessageType::Cc,
            },
            target: crate::midi::MidiCcTarget {
                instance_id,
                port_index,
            },
            mode: crate::midi::MappingMode::Continuous,
            label: format!("p{}", port_index),
        }
    }

    fn link(a: &str, b: &str) -> SavedPluginLink {
        SavedPluginLink {
            output_node_name: a.into(),
            output_port_name: "out".into(),
            input_node_name: b.into(),
            input_port_name: "in".into(),
        }
    }

    #[test]
    fn midi_mappings_resolve_by_stable_id() {
        let saved = vec![
            SavedMidiMapping { mapping: mapping(7, 1, 10), stable_id: "a".into() },
            SavedMidiMapping { mapping: mapping(3, 2, 11), stable_id: "gone".into() },
        ];
        let map: HashMap<String, u64> = [("a".to_string(), 42u64)].into_iter().collect();
        let out = resolve_saved_midi_mappings(saved, &map, &[42]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].target.instance_id, 42);
        assert_eq!(out[0].target.port_index, 1);
    }

    #[test]
    fn restore_safety_cap_scales_with_batch() {
        assert_eq!(restore_safety_cap(0), Duration::from_secs(30));
        assert_eq!(restore_safety_cap(1), Duration::from_secs(45));
        assert_eq!(restore_safety_cap(20), Duration::from_secs(330));
    }

    #[test]
    fn legacy_midi_mappings_use_restore_order() {
        let json = r#"[
            {"source":{"device_name":"dev","channel":0,"cc":1},"target":{"instance_id":2,"port_index":5},"mode":"Continuous","label":"x"},
            {"source":{"device_name":"dev","channel":0,"cc":2},"target":{"instance_id":9,"port_index":5},"mode":"Continuous","label":"y"}
        ]"#;
        let saved: Vec<SavedMidiMapping> = serde_json::from_str(json).unwrap();
        assert!(saved.iter().all(|s| s.stable_id.is_empty()));
        let out = resolve_saved_midi_mappings(saved, &HashMap::new(), &[100, 101]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].target.instance_id, 101);
    }

    #[test]
    fn saved_midi_mapping_roundtrip_keeps_stable_id() {
        let s = SavedMidiMapping { mapping: mapping(1, 2, 3), stable_id: "sid".into() };
        let json = serde_json::to_string(&s).unwrap();
        let back: SavedMidiMapping = serde_json::from_str(&json).unwrap();
        assert_eq!(back.stable_id, "sid");
        assert_eq!(back.mapping.target.port_index, 2);
    }

    #[test]
    fn plugin_state_base64_roundtrip() {
        use base64::Engine;
        let blob = vec![0u8, 1, 2, 255];
        let sp = SavedPlugin {
            stable_id: "s".into(),
            uri: "u".into(),
            display_name: "d".into(),
            bypassed: false,
            parameters: Vec::new(),
            format: "CLAP".into(),
            lv2_state: Vec::new(),
            plugin_state: Some(base64::engine::general_purpose::STANDARD.encode(&blob)),
        };
        let json = serde_json::to_string(&sp).unwrap();
        let back: SavedPlugin = serde_json::from_str(&json).unwrap();
        assert_eq!(back.decode_plugin_state(), Some(blob));
        let legacy: SavedPlugin =
            serde_json::from_str(r#"{"uri":"u","display_name":"d"}"#).unwrap();
        assert_eq!(legacy.decode_plugin_state(), None);
    }

    #[test]
    fn merge_links_remembers_links_of_absent_nodes() {
        let forgotten = std::collections::HashSet::new();
        // Device "B" vanished: its link is remembered; A->C was deleted on purpose.
        let mut pending = Vec::new();
        let last = vec![link("A", "B"), link("A", "C")];
        let current = vec![];
        let present = |n: &str| n != "B";
        let merged = merge_links_for_persist(&current, &last, &mut pending, &forgotten, present);
        assert_eq!(merged, vec![link("A", "B")]);
        assert_eq!(pending.len(), 1);

        // Back in the graph: no longer pending, persisted once.
        let current = vec![link("A", "B")];
        let merged = merge_links_for_persist(&current, &[], &mut pending, &forgotten, |_| true);
        assert_eq!(merged, vec![link("A", "B")]);
        assert!(pending.is_empty());
    }

    #[test]
    fn merge_links_drops_forgotten_plugins() {
        let forgotten: std::collections::HashSet<String> = ["P".to_string()].into_iter().collect();
        let mut pending = vec![PendingLink::new(link("P", "X")), PendingLink::new(link("Y", "X"))];
        let merged =
            merge_links_for_persist(&[], &[link("P", "Z")], &mut pending, &forgotten, |_| false);
        assert_eq!(merged, vec![link("Y", "X")]);
    }

    #[test]
    fn backup_stem_parsing_is_panic_free() {
        assert_eq!(
            parse_backup_stem("20240102_030405_My rules"),
            (Some("2024-01-02 03:04:05".to_string()), "My rules".to_string())
        );
        assert_eq!(
            parse_backup_stem("20240102_030405"),
            (Some("2024-01-02 03:04:05".to_string()), String::new())
        );
        // Multi-byte chars around the indices that used to be sliced.
        assert_eq!(parse_backup_stem("ééééééééééééééééé"), (None, String::new()));
        assert_eq!(parse_backup_stem("20240102_03040é_x"), (None, String::new()));
        assert_eq!(
            parse_backup_stem("20240102_030405_é"),
            (Some("2024-01-02 03:04:05".to_string()), "é".to_string())
        );
        assert_eq!(parse_backup_stem("short"), (None, String::new()));
    }

    #[test]
    fn plain_file_names_only() {
        assert!(is_plain_file_name("20240101_000000.json"));
        assert!(!is_plain_file_name("../rules.json"));
        assert!(!is_plain_file_name("a/b.json"));
        assert!(!is_plain_file_name(".."));
        assert!(!is_plain_file_name(""));
    }

    #[test]
    fn atomic_write_replaces_file() {
        let dir = std::env::temp_dir().join(format!("zestbay-test-{}", std::process::id()));
        let path = dir.join("x.json");
        write_atomic(&path, b"[1]").unwrap();
        write_atomic(&path, b"[2]").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[2]");
        let entries = std::fs::read_dir(&dir).unwrap().count();
        assert_eq!(entries, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
