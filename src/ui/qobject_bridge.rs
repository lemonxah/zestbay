#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// Main application controller exposed to QML.
        #[qobject]
        #[qml_element]
        #[qproperty(bool, patchbay_enabled)]
        #[qproperty(i32, active_plugin_count)]
        #[qproperty(i32, node_count)]
        #[qproperty(i32, link_count)]
        type AppController = super::AppControllerRust;

        /// Called by QML once on startup to kick off PipeWire.
        #[qinvokable]
        fn init(self: Pin<&mut Self>);

        /// Poll for PipeWire events. Called by a QML Timer at ~10Hz.
        #[qinvokable]
        fn poll_events(self: Pin<&mut Self>);

        /// Request the application to quit.
        #[qinvokable]
        fn request_quit(self: Pin<&mut Self>);

        /// Get a JSON string of all nodes for the graph view.
        #[qinvokable]
        fn get_nodes_json(self: Pin<&mut Self>) -> QString;

        /// Get a JSON string of all links for the graph view.
        #[qinvokable]
        fn get_links_json(self: Pin<&mut Self>) -> QString;

        /// Get a JSON string of ports for a given node.
        #[qinvokable]
        fn get_ports_json(self: Pin<&mut Self>, node_id: u32) -> QString;

        /// Request to connect two ports.
        #[qinvokable]
        fn connect_ports(self: Pin<&mut Self>, output_port_id: u32, input_port_id: u32);

        /// Request to disconnect a link.
        #[qinvokable]
        fn disconnect_link(self: Pin<&mut Self>, link_id: u32);

        /// Get saved layout positions as JSON: {"Type:Name": [x, y], ...}
        #[qinvokable]
        fn get_layout_json(self: Pin<&mut Self>) -> QString;

        /// Save layout positions from JSON string.
        #[qinvokable]
        fn save_layout(self: Pin<&mut Self>, json: QString);

        /// Get hidden node keys as JSON array: ["Type:Name", ...]
        #[qinvokable]
        fn get_hidden_json(self: Pin<&mut Self>) -> QString;

        /// Save hidden node keys from JSON string.
        #[qinvokable]
        fn save_hidden(self: Pin<&mut Self>, json: QString);

        // ── Plugin browser ─────────────────────────────────────────

        /// Get all available LV2 plugins as JSON array.
        #[qinvokable]
        fn get_available_plugins_json(self: Pin<&mut Self>) -> QString;

        /// Add an LV2 plugin instance by URI. Returns the display name.
        #[qinvokable]
        fn add_plugin(self: Pin<&mut Self>, uri: QString) -> QString;

        // ── Plugin context-menu actions ────────────────────────────

        /// Remove an LV2 plugin by its PipeWire node ID.
        #[qinvokable]
        fn remove_plugin(self: Pin<&mut Self>, node_id: u32);

        /// Open the native LV2 plugin UI for the given PipeWire node ID.
        #[qinvokable]
        fn open_plugin_ui(self: Pin<&mut Self>, node_id: u32);

        /// Rename an LV2 plugin instance (by PipeWire node ID).
        #[qinvokable]
        fn rename_plugin(self: Pin<&mut Self>, node_id: u32, new_name: QString);

        /// Get plugin parameters as JSON for the given PipeWire node ID.
        #[qinvokable]
        fn get_plugin_params_json(self: Pin<&mut Self>, node_id: u32) -> QString;

        /// Set a plugin parameter value.
        #[qinvokable]
        fn set_plugin_parameter(self: Pin<&mut Self>, node_id: u32, port_index: u32, value: f32);

        /// Toggle bypass on a plugin.
        #[qinvokable]
        fn set_plugin_bypass(self: Pin<&mut Self>, node_id: u32, bypassed: bool);

        // ── Plugin manager (persisted plugins) ─────────────────────

        /// Get all active/persisted plugin instances as JSON array.
        #[qinvokable]
        fn get_active_plugins_json(self: Pin<&mut Self>) -> QString;

        /// Remove a persisted plugin by its stable_id (does NOT remove the
        /// live PipeWire node — use remove_plugin for that).
        #[qinvokable]
        fn remove_plugin_by_stable_id(self: Pin<&mut Self>, stable_id: QString);

        /// Reset all parameters of a plugin instance to defaults (by stable_id).
        #[qinvokable]
        fn reset_plugin_params_by_stable_id(self: Pin<&mut Self>, stable_id: QString);

        /// Set a plugin parameter by stable_id + port_index.
        #[qinvokable]
        fn set_plugin_param_by_stable_id(
            self: Pin<&mut Self>,
            stable_id: QString,
            port_index: u32,
            value: f32,
        );

        // ── Patchbay rules ─────────────────────────────────────────

        /// Get all patchbay rules as a JSON array.
        #[qinvokable]
        fn get_rules_json(self: Pin<&mut Self>) -> QString;

        /// Toggle a rule's enabled state by ID. Returns the new state as JSON.
        #[qinvokable]
        fn toggle_rule(self: Pin<&mut Self>, rule_id: QString);

        /// Remove a rule by ID.
        #[qinvokable]
        fn remove_rule(self: Pin<&mut Self>, rule_id: QString);

        /// Apply all enabled rules now (scan and execute).
        #[qinvokable]
        fn apply_rules(self: Pin<&mut Self>);

        /// Snapshot current connections as rules (replaces all rules).
        #[qinvokable]
        fn snapshot_rules(self: Pin<&mut Self>);

        /// Toggle patchbay rules on/off.
        #[qinvokable]
        fn toggle_patchbay(self: Pin<&mut Self>, enabled: bool);

        /// Get list of available node names for rule creation as JSON array.
        #[qinvokable]
        fn get_node_names_json(self: Pin<&mut Self>) -> QString;

        /// Add a new rule manually. source_pattern and target_pattern are glob patterns.
        #[qinvokable]
        fn add_rule(
            self: Pin<&mut Self>,
            source_pattern: QString,
            source_type: QString,
            target_pattern: QString,
            target_type: QString,
        );

        // ── Window geometry ────────────────────────────────────────

        /// Get saved window geometry as JSON: { x, y, width, height }
        #[qinvokable]
        fn get_window_geometry_json(self: Pin<&mut Self>) -> QString;

        /// Save window geometry.
        #[qinvokable]
        fn save_window_geometry(self: Pin<&mut Self>, json: QString);

        /// Get saved viewport (pan/zoom) as JSON: { panX, panY, zoom }
        #[qinvokable]
        fn get_viewport_json(self: Pin<&mut Self>) -> QString;

        /// Save viewport (pan/zoom).
        #[qinvokable]
        fn save_viewport(self: Pin<&mut Self>, json: QString);

        // ── Preferences ───────────────────────────────────────────

        /// Get all preferences as a JSON object.
        #[qinvokable]
        fn get_preferences_json(self: Pin<&mut Self>) -> QString;

        /// Update a single preference by key. value is a JSON-encoded value.
        #[qinvokable]
        fn set_preference(self: Pin<&mut Self>, key: QString, value: QString);

        /// Reset all preferences to defaults.
        #[qinvokable]
        fn reset_preferences(self: Pin<&mut Self>);

        /// Get the poll interval in ms (QML reads this to set the Timer interval).
        #[qinvokable]
        fn get_poll_interval_ms(self: Pin<&mut Self>) -> i32;

        // ── Tray / window visibility ──────────────────────────────

        /// Notify the tray of the current window visibility state.
        #[qinvokable]
        fn set_window_visible(self: Pin<&mut Self>, visible: bool);
    }

    // Signals emitted from Rust to QML
    unsafe extern "RustQt" {
        /// Emitted when the graph data changes and QML should refresh.
        #[qsignal]
        fn graph_changed(self: Pin<&mut AppController>);

        /// Emitted when an error occurs.
        #[qsignal]
        fn error_occurred(self: Pin<&mut AppController>, message: QString);

        /// Emitted when the tray icon requests the window to be shown.
        #[qsignal]
        fn show_window_requested(self: Pin<&mut AppController>);

        /// Emitted when the tray icon requests the window to be hidden.
        #[qsignal]
        fn hide_window_requested(self: Pin<&mut AppController>);
    }
}

use core::pin::Pin;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use std::path::PathBuf;

use crate::lv2::Lv2Manager;
use crate::patchbay::{PatchbayManager, rules};
use crate::pipewire::{GraphState, Lv2Event, Node, NodeType, PortDirection, PwCommand, PwEvent};
use crate::tray::TrayState;

/// Rust backing struct for AppController.
///
/// Fields that are `#[qproperty]` must be CXX-compatible types.
/// Internal Rust-only state uses `Option` since we need `Default`.
pub struct AppControllerRust {
    // QML-visible properties
    patchbay_enabled: bool,
    active_plugin_count: i32,
    node_count: i32,
    link_count: i32,

    // Internal state (not exposed to QML as properties)
    graph: Option<Arc<GraphState>>,
    event_rx: Option<Receiver<PwEvent>>,
    cmd_tx: Option<Sender<PwCommand>>,
    patchbay: Option<PatchbayManager>,
    lv2_manager: Option<Lv2Manager>,
    last_change_counter: u64,

    // Plugin instance ID counter
    next_instance_id: u64,

    // Cached data
    cached_nodes: Vec<Node>,

    // Rules auto-apply state
    last_change_time: Option<std::time::Instant>,
    rules_apply_pending: bool,
    rules_loaded: bool,

    // Plugin params persistence
    params_dirty: bool,
    params_dirty_since: Option<std::time::Instant>,

    // Link restoration state — links involving LV2 plugins are saved to disk
    // and restored after all plugins come online on startup.
    pending_restore_count: usize,
    pending_links: Vec<SavedPluginLink>,

    // Debounced link persistence
    links_dirty: bool,
    links_dirty_since: Option<std::time::Instant>,

    // User preferences (loaded from preferences.json)
    prefs: Preferences,

    // System tray state (ksni D-Bus StatusNotifier)
    tray_state: Option<TrayState>,
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
            lv2_manager: None,
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
        }
    }
}

impl qobject::AppController {
    /// Initialize the application — called once from QML.
    pub fn init(mut self: Pin<&mut Self>) {
        log::info!("AppController::init — starting PipeWire");

        // Load user preferences
        let prefs = load_preferences();
        log::info!(
            "Preferences: rule_settle={}ms, params_persist={}ms, links_persist={}ms, poll={}ms, auto_learn={}",
            prefs.rule_settle_ms,
            prefs.params_persist_ms,
            prefs.links_persist_ms,
            prefs.poll_interval_ms,
            prefs.auto_learn_rules
        );
        self.as_mut().rust_mut().prefs = prefs;

        // Create shared graph state
        let graph = GraphState::new();

        // Initialize LV2 plugin manager
        let lv2_manager = Lv2Manager::new();

        // Start PipeWire manager thread
        let (event_rx, cmd_tx) = crate::pipewire::start(graph.clone());

        // Create patchbay manager
        let patchbay = PatchbayManager::new(graph.clone());

        // Store in our backing struct
        self.as_mut().rust_mut().graph = Some(graph);
        self.as_mut().rust_mut().event_rx = Some(event_rx);
        self.as_mut().rust_mut().cmd_tx = Some(cmd_tx);
        self.as_mut().rust_mut().patchbay = Some(patchbay);
        self.as_mut().rust_mut().lv2_manager = Some(lv2_manager);

        // Load saved links for restoration after plugins come online
        let saved_links = load_saved_links();
        if !saved_links.is_empty() {
            log::info!(
                "Loaded {} saved LV2 links for restoration",
                saved_links.len()
            );
            self.as_mut().rust_mut().pending_links = saved_links;
        }

        // Restore previously saved plugins
        let saved = load_saved_plugins();
        if !saved.is_empty() {
            log::info!("Restoring {} saved plugins", saved.len());
            self.as_mut().rust_mut().pending_restore_count = saved.len();
            for sp in saved {
                let instance_id = self.rust().next_instance_id;
                self.as_mut().rust_mut().next_instance_id += 1;

                // Build the full parameter list from the plugin info, then
                // overlay saved values on top.  This gives us correct
                // name/min/max/default metadata AND the saved values.
                let restored_params: Vec<crate::lv2::Lv2ParameterValue> = if let Some(ref mgr) =
                    self.rust().lv2_manager
                {
                    if let Some(plugin_info) = mgr.find_plugin(&sp.uri) {
                        plugin_info
                            .ports
                            .iter()
                            .filter(|port| port.port_type == crate::lv2::Lv2PortType::ControlInput)
                            .map(|port| {
                                // Use saved value if we have one for this port
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
                        // Plugin not found — fall back to raw saved data
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

                // Use the persisted stable_id so we can match this instance
                // to the correct saved params across restarts.
                let sid = if sp.stable_id.is_empty() {
                    // Legacy save without stable_id — generate one now
                    uuid::Uuid::new_v4().to_string()
                } else {
                    sp.stable_id.clone()
                };

                if let Some(ref mut mgr) = self.as_mut().rust_mut().lv2_manager {
                    let info = crate::lv2::Lv2InstanceInfo {
                        id: instance_id,
                        stable_id: sid,
                        plugin_uri: sp.uri.clone(),
                        display_name: sp.display_name.clone(),
                        pw_node_id: None,
                        parameters: restored_params,
                        active: true,
                        bypassed: sp.bypassed,
                    };
                    mgr.register_instance(info);
                }

                if let Some(ref tx) = self.rust().cmd_tx {
                    log::info!("Restoring plugin: {} ({})", sp.display_name, sp.uri);
                    let _ = tx.send(PwCommand::AddPlugin {
                        plugin_uri: sp.uri,
                        instance_id,
                        display_name: sp.display_name,
                    });
                }
            }
        }

        // Spawn the system tray icon (D-Bus StatusNotifier via ksni)
        let tray_state = crate::tray::spawn_tray();
        // If start_minimized is on, the window won't be shown — tell the tray.
        if self.rust().prefs.start_minimized {
            tray_state
                .window_visible
                .store(false, std::sync::atomic::Ordering::Release);
        }
        self.as_mut().rust_mut().tray_state = Some(tray_state);

        log::info!("AppController initialized successfully");
    }

    /// Poll for PipeWire events — called periodically by QML Timer.
    pub fn poll_events(mut self: Pin<&mut Self>) {
        // Load rules on first poll (after init has run)
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
        }

        let mut changed = false;
        let mut link_changed = false;
        let mut error_msg: Option<String> = None;
        let mut lv2_events: Vec<Lv2Event> = Vec::new();

        // Drain all pending PW events
        // We need to access event_rx through a raw pointer to avoid borrow issues
        // with Pin<&mut Self> — the Receiver is behind Option in the Rust struct.
        let has_events = self.rust().event_rx.is_some();
        if has_events {
            // Temporarily take the receiver out
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
                        PwEvent::Lv2(lv2_event) => {
                            changed = true;
                            lv2_events.push(lv2_event);
                        }
                    }
                }
                // Put the receiver back
                self.as_mut().rust_mut().event_rx = Some(rx);
            }
        }

        // Process LV2 events (needs mutable access to lv2_manager)
        for event in lv2_events {
            match event {
                Lv2Event::PluginAdded {
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
                    if let Some(ref mut mgr) = self.as_mut().rust_mut().lv2_manager {
                        mgr.set_instance_pw_node_id(instance_id, pw_node_id);
                    }
                    // Ensure the node in GraphState is marked as Lv2Plugin.
                    // The registry listener may have classified it as Duplex
                    // if the custom property wasn't visible in the registry.
                    if pw_node_id != 0
                        && pw_node_id != u32::MAX
                        && let Some(ref graph) = self.rust().graph
                    {
                        graph.set_node_type(pw_node_id, NodeType::Lv2Plugin);
                    }

                    // Restore saved parameters and bypass state for this instance.
                    // The params were pre-loaded into Lv2InstanceInfo during init.
                    if let Some(ref mgr) = self.rust().lv2_manager
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

                    // Track pending restore count for link restoration
                    if self.rust().pending_restore_count > 0 {
                        let count = self.rust().pending_restore_count - 1;
                        self.as_mut().rust_mut().pending_restore_count = count;
                        if count == 0 {
                            log::info!("All plugins restored — will attempt link restoration");
                            // Links will be restored in the next poll cycle
                            // (deferred so ports have time to register)
                        }
                    }
                }
                Lv2Event::PluginRemoved { instance_id } => {
                    log::info!("LV2 plugin removed: instance={}", instance_id);
                    if let Some(ref mut mgr) = self.as_mut().rust_mut().lv2_manager {
                        mgr.remove_instance(instance_id);
                    }
                    // Re-persist after removal
                    persist_active_plugins(self.rust().lv2_manager.as_ref());
                    // Links involving this plugin will be removed by PipeWire;
                    // mark dirty so the link file gets updated after settle.
                    self.as_mut().rust_mut().links_dirty = true;
                    if self.rust().links_dirty_since.is_none() {
                        self.as_mut().rust_mut().links_dirty_since = Some(Instant::now());
                    }
                }
                Lv2Event::ParameterChanged {
                    instance_id,
                    port_index,
                    value,
                } => {
                    if let Some(ref mut mgr) = self.as_mut().rust_mut().lv2_manager {
                        mgr.update_parameter(instance_id, port_index, value);
                    }
                    self.as_mut().rust_mut().params_dirty = true;
                    if self.rust().params_dirty_since.is_none() {
                        self.as_mut().rust_mut().params_dirty_since = Some(Instant::now());
                    }
                }
                Lv2Event::PluginUiOpened { instance_id } => {
                    log::info!("LV2 plugin UI opened: instance={}", instance_id);
                }
                Lv2Event::PluginUiClosed { instance_id } => {
                    log::info!("LV2 plugin UI closed: instance={}", instance_id);
                }
                Lv2Event::PluginError {
                    instance_id,
                    message,
                } => {
                    log::error!(
                        "LV2 plugin error: instance={:?} msg={}",
                        instance_id,
                        message
                    );

                    // Remove the failed plugin from the manager so it won't
                    // be re-persisted (and thus won't fail again on next start).
                    if let Some(id) = instance_id {
                        let plugin_name = self
                            .rust()
                            .lv2_manager
                            .as_ref()
                            .and_then(|mgr| mgr.get_instance(id))
                            .map(|info| info.display_name.clone());

                        if let Some(ref mut mgr) = self.as_mut().rust_mut().lv2_manager {
                            mgr.remove_instance(id);
                        }
                        persist_active_plugins(self.rust().lv2_manager.as_ref());

                        // If we're restoring plugins at startup, decrement
                        // the pending count so link restoration isn't blocked.
                        if self.rust().pending_restore_count > 0 {
                            let count = self.rust().pending_restore_count - 1;
                            self.as_mut().rust_mut().pending_restore_count = count;
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

        // Check if graph state has changed via counter
        if let Some(ref graph) = self.rust().graph {
            let current = graph.change_counter();
            let last = self.rust().last_change_counter;
            if current != last {
                changed = true;
                self.as_mut().rust_mut().last_change_counter = current;
            }
        }

        // Track change time for auto-apply settle timer
        if changed {
            self.as_mut().rust_mut().last_change_time = Some(Instant::now());
            self.as_mut().rust_mut().rules_apply_pending = true;
        }

        // Mark links dirty for debounced persistence (only after restore is done)
        if link_changed
            && self.rust().pending_restore_count == 0
            && self.rust().pending_links.is_empty()
        {
            self.as_mut().rust_mut().links_dirty = true;
            if self.rust().links_dirty_since.is_none() {
                self.as_mut().rust_mut().links_dirty_since = Some(Instant::now());
            }
        }

        // Auto-apply rules after graph settles (configurable, default 500ms)
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
            // Persist rules if scan refreshed stale target node IDs
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

        // Debounced plugin params persistence (configurable, default 1s)
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
            persist_active_plugins(self.rust().lv2_manager.as_ref());
        }

        // Try restoring saved links once all plugins are online and graph has settled
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
                    // Find nodes by display name
                    let all_nodes = graph.get_all_nodes();
                    let out_node = all_nodes
                        .iter()
                        .find(|n| n.display_name() == saved_link.output_node_name);
                    let in_node = all_nodes
                        .iter()
                        .find(|n| n.display_name() == saved_link.input_node_name);

                    if let (Some(out_node), Some(in_node)) = (out_node, in_node) {
                        let out_ports = graph.get_ports_for_node(out_node.id);
                        let in_ports = graph.get_ports_for_node(in_node.id);

                        let out_port = out_ports.iter().find(|p| {
                            p.name == saved_link.output_port_name
                                && p.direction == PortDirection::Output
                        });
                        let in_port = in_ports.iter().find(|p| {
                            p.name == saved_link.input_port_name
                                && p.direction == PortDirection::Input
                        });

                        if let (Some(out_port), Some(in_port)) = (out_port, in_port) {
                            log::info!(
                                "Restoring link: {}:{} -> {}:{}",
                                saved_link.output_node_name,
                                saved_link.output_port_name,
                                saved_link.input_node_name,
                                saved_link.input_port_name
                            );
                            if let Some(ref tx) = self.rust().cmd_tx {
                                let _ = tx.send(PwCommand::Connect {
                                    output_port_id: out_port.id,
                                    input_port_id: in_port.id,
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
                    } else {
                        log::warn!(
                            "Could not find nodes for saved link: {} -> {}",
                            saved_link.output_node_name,
                            saved_link.input_node_name
                        );
                    }
                }
            }
        }

        // Debounced link persistence (configurable, default 2s)
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

        // Check tray state flags (ksni D-Bus tray running on background thread).
        // Clone the TrayState (cheap Arc clones) to avoid holding a borrow on self
        // while emitting signals which need &mut self.
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

        // Emit signals
        if let Some(msg) = error_msg {
            let qmsg = QString::from(&msg);
            self.as_mut().error_occurred(qmsg);
        }

        if changed {
            self.as_mut().refresh_cache();
            self.as_mut().graph_changed();
        }
    }

    /// Refresh cached data from graph state and update QML properties.
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

    /// Get all nodes as a JSON string for the graph view.
    pub fn get_nodes_json(self: Pin<&mut Self>) -> QString {
        if let Some(ref graph) = self.rust().graph {
            let nodes = graph.get_all_nodes();
            log::debug!(
                "get_nodes_json: {} nodes ({} ready)",
                nodes.len(),
                nodes.iter().filter(|n| n.ready).count()
            );
            let json_nodes: Vec<serde_json::Value> = nodes
                .iter()
                .filter(|n| n.ready)
                .map(|n| {
                    let type_str = match n.node_type {
                        Some(NodeType::Sink) => "Sink",
                        Some(NodeType::Source) => "Source",
                        Some(NodeType::StreamOutput) => "StreamOutput",
                        Some(NodeType::StreamInput) => "StreamInput",
                        Some(NodeType::Duplex) => "Duplex",
                        Some(NodeType::Lv2Plugin) => "Lv2Plugin",
                        None => "Unknown",
                    };
                    let media_str = match n.media_type {
                        Some(crate::pipewire::MediaType::Audio) => "Audio",
                        Some(crate::pipewire::MediaType::Video) => "Video",
                        Some(crate::pipewire::MediaType::Midi) => "Midi",
                        None => "Unknown",
                    };
                    serde_json::json!({
                        "id": n.id,
                        "name": n.display_name(),
                        "type": type_str,
                        "mediaType": media_str,
                        "isVirtual": n.is_virtual,
                        "isJack": n.is_jack,
                        "layoutKey": layout_key(n),
                        "ready": n.ready,
                    })
                })
                .collect();
            let json = serde_json::to_string(&json_nodes).unwrap_or_default();
            QString::from(&json)
        } else {
            QString::from("[]")
        }
    }

    /// Get all links as a JSON string.
    pub fn get_links_json(self: Pin<&mut Self>) -> QString {
        if let Some(ref graph) = self.rust().graph {
            let links = graph.get_all_links();
            let json_links: Vec<serde_json::Value> = links
                .iter()
                .map(|l| {
                    serde_json::json!({
                        "id": l.id,
                        "outputNodeId": l.output_node_id,
                        "outputPortId": l.output_port_id,
                        "inputNodeId": l.input_node_id,
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

    /// Get ports for a node as a JSON string.
    pub fn get_ports_json(self: Pin<&mut Self>, node_id: u32) -> QString {
        log::debug!("get_ports_json: node_id={}", node_id);
        if let Some(ref graph) = self.rust().graph {
            let ports = graph.get_ports_for_node(node_id);
            let json_ports: Vec<serde_json::Value> = ports
                .iter()
                .map(|p| {
                    let media_str = match p.media_type {
                        Some(crate::pipewire::MediaType::Audio) => "Audio",
                        Some(crate::pipewire::MediaType::Video) => "Video",
                        Some(crate::pipewire::MediaType::Midi) => "Midi",
                        None => "Unknown",
                    };
                    serde_json::json!({
                        "id": p.id,
                        "name": p.display_name(),
                        "direction": format!("{:?}", p.direction),
                        "nodeId": p.node_id,
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

    /// Request to connect two ports.
    pub fn connect_ports(mut self: Pin<&mut Self>, output_port_id: u32, input_port_id: u32) {
        if let Some(ref tx) = self.rust().cmd_tx {
            log::info!("Connect request: {} -> {}", output_port_id, input_port_id);
            let _ = tx.send(PwCommand::Connect {
                output_port_id,
                input_port_id,
            });
        }

        // Auto-learn: create/update a patchbay rule from this manual connection
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

        // Mark links dirty so they get persisted
        self.as_mut().rust_mut().links_dirty = true;
        if self.rust().links_dirty_since.is_none() {
            self.as_mut().rust_mut().links_dirty_since = Some(Instant::now());
        }
    }

    /// Request to disconnect a link.
    pub fn disconnect_link(mut self: Pin<&mut Self>, link_id: u32) {
        // Look up link details before disconnecting so we can unlearn the rule
        let link_info = self.rust().graph.as_ref().and_then(|g| g.get_link(link_id));

        if let Some(ref tx) = self.rust().cmd_tx {
            log::info!("Disconnect request: {}", link_id);
            let _ = tx.send(PwCommand::Disconnect { link_id });
        }

        // Remove the corresponding port mapping from patchbay rules
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

        // Mark links dirty so they get persisted
        self.as_mut().rust_mut().links_dirty = true;
        if self.rust().links_dirty_since.is_none() {
            self.as_mut().rust_mut().links_dirty_since = Some(Instant::now());
        }
    }

    /// Request application quit.
    pub fn request_quit(self: Pin<&mut Self>) {
        log::info!("Quit requested");
        // Persist links before shutting down
        persist_lv2_links(self.rust().graph.as_ref());
        persist_active_plugins(self.rust().lv2_manager.as_ref());
        crate::lv2::ui::shutdown_gtk_thread();
        std::process::exit(0);
    }

    // ── Layout persistence ─────────────────────────────────────────

    /// Get saved layout positions as JSON: {"Type:Name": [x, y], ...}
    pub fn get_layout_json(self: Pin<&mut Self>) -> QString {
        let path = config_path("layout.json");
        let json = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => "{}".to_string(),
        };
        log::debug!("get_layout_json: loaded from {:?}", path);
        QString::from(&json)
    }

    /// Save layout positions from JSON string.
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

    /// Get hidden node keys as JSON array: ["Type:Name", ...]
    pub fn get_hidden_json(self: Pin<&mut Self>) -> QString {
        let path = config_path("hidden.json");
        let json = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => "[]".to_string(),
        };
        log::debug!("get_hidden_json: loaded from {:?}", path);
        QString::from(&json)
    }

    /// Save hidden node keys from JSON string.
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

    // ── Plugin browser ─────────────────────────────────────────────

    /// Get all available LV2 plugins as a JSON array.
    /// Each entry: { uri, name, category, author, audioIn, audioOut, controlIn, controlOut }
    pub fn get_available_plugins_json(self: Pin<&mut Self>) -> QString {
        if let Some(ref mgr) = self.rust().lv2_manager {
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
                    })
                })
                .collect();
            let json = serde_json::to_string(&json_plugins).unwrap_or_default();
            QString::from(&json)
        } else {
            QString::from("[]")
        }
    }

    /// Add an LV2 plugin by URI. Returns the display name used, or empty on error.
    pub fn add_plugin(mut self: Pin<&mut Self>, uri: QString) -> QString {
        let uri_str: String = uri.to_string();

        // Look up plugin info to get the display name and build initial params
        let (display_name, initial_params) = if let Some(ref mgr) = self.rust().lv2_manager {
            let plugin = mgr.find_plugin(&uri_str);
            let base_name = plugin
                .map(|p| p.name.clone())
                .unwrap_or_else(|| uri_str.clone());
            let name = self.unique_display_name(&base_name);
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
            (name, params)
        } else {
            return QString::from("");
        };

        // Generate instance ID
        let instance_id = self.rust().next_instance_id;
        self.as_mut().rust_mut().next_instance_id += 1;

        // Register with LV2 manager
        if let Some(ref mut mgr) = self.as_mut().rust_mut().lv2_manager {
            let info = crate::lv2::Lv2InstanceInfo {
                id: instance_id,
                stable_id: uuid::Uuid::new_v4().to_string(),
                plugin_uri: uri_str.clone(),
                display_name: display_name.clone(),
                pw_node_id: None,
                parameters: initial_params,
                active: true,
                bypassed: false,
            };
            mgr.register_instance(info);
        }

        // Send command to PipeWire thread
        if let Some(ref tx) = self.rust().cmd_tx {
            log::info!(
                "Adding plugin: uri={} instance_id={} name={}",
                uri_str,
                instance_id,
                display_name
            );
            let _ = tx.send(PwCommand::AddPlugin {
                plugin_uri: uri_str.clone(),
                instance_id,
                display_name: display_name.clone(),
            });
        }

        // Persist to plugins.json
        persist_active_plugins(self.rust().lv2_manager.as_ref());

        QString::from(&display_name)
    }

    // ── Plugin context-menu actions ────────────────────────────────

    /// Remove an LV2 plugin by its PipeWire node ID.
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

    /// Open the native LV2 plugin UI for the given PipeWire node ID.
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

    /// Rename an LV2 plugin instance (by PipeWire node ID).
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
            if let Some(ref mut mgr) = self.as_mut().rust_mut().lv2_manager
                && let Some(info) = mgr.get_instance_mut(instance_id)
            {
                info.display_name = name_str.clone();
            }
            // Update the node description in GraphState so the graph view
            // shows the new name immediately (without restarting).
            if let Some(ref graph) = self.rust().graph {
                graph.set_node_description(node_id, &name_str);
            }
            // Persist immediately so the rename survives a crash
            persist_active_plugins(self.rust().lv2_manager.as_ref());
        } else {
            log::warn!(
                "rename_plugin: no LV2 instance found for node_id={}",
                node_id
            );
        }
    }

    // ── Plugin parameters ────────────────────────────────────────

    /// Get plugin parameters as JSON for the given PipeWire node ID.
    pub fn get_plugin_params_json(self: Pin<&mut Self>, node_id: u32) -> QString {
        let instance_id = self.find_instance_id_for_node(node_id);
        if let Some(instance_id) = instance_id
            && let Some(ref mgr) = self.rust().lv2_manager
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

    /// Set a plugin parameter value.
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
            // Also update the in-memory value so persist sees it immediately
            if let Some(ref mut mgr) = self.as_mut().rust_mut().lv2_manager {
                mgr.update_parameter(instance_id, port_index as usize, value);
            }
            self.as_mut().rust_mut().params_dirty = true;
            if self.rust().params_dirty_since.is_none() {
                self.as_mut().rust_mut().params_dirty_since = Some(Instant::now());
            }
        }
    }

    /// Toggle bypass on a plugin.
    pub fn set_plugin_bypass(mut self: Pin<&mut Self>, node_id: u32, bypassed: bool) {
        let instance_id = self.find_instance_id_for_node(node_id);
        if let Some(instance_id) = instance_id {
            if let Some(ref tx) = self.rust().cmd_tx {
                let _ = tx.send(PwCommand::SetPluginBypass {
                    instance_id,
                    bypassed,
                });
            }
            // Update in-memory bypass state
            if let Some(ref mut mgr) = self.as_mut().rust_mut().lv2_manager
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

    // ── Plugin manager (persisted plugins) ───────────────────────

    /// Get all active/persisted plugin instances as JSON array.
    pub fn get_active_plugins_json(self: Pin<&mut Self>) -> QString {
        if let Some(ref mgr) = self.rust().lv2_manager {
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

    /// Remove a persisted plugin by its stable_id.
    /// This removes it from the LV2 manager AND sends a RemovePlugin
    /// command if it has a live PipeWire node.
    pub fn remove_plugin_by_stable_id(mut self: Pin<&mut Self>, stable_id: QString) {
        let sid: String = stable_id.to_string();

        // Find the instance ID and check if it has a live node
        let instance_id = self
            .rust()
            .lv2_manager
            .as_ref()
            .and_then(|mgr| mgr.instance_id_for_stable_id(&sid));

        if let Some(instance_id) = instance_id {
            // Send RemovePlugin to tear down the live PipeWire filter node
            if let Some(ref tx) = self.rust().cmd_tx {
                let _ = tx.send(PwCommand::RemovePlugin { instance_id });
            }
            // Remove from manager
            if let Some(ref mut mgr) = self.as_mut().rust_mut().lv2_manager {
                mgr.remove_instance(instance_id);
            }
            persist_active_plugins(self.rust().lv2_manager.as_ref());
            log::info!("Removed plugin instance (stable_id={})", sid);
        } else {
            log::warn!(
                "remove_plugin_by_stable_id: no instance found for stable_id={}",
                sid
            );
        }
    }

    /// Reset all parameters of a plugin instance to their defaults.
    pub fn reset_plugin_params_by_stable_id(mut self: Pin<&mut Self>, stable_id: QString) {
        let sid: String = stable_id.to_string();

        // Collect the resets we need to apply
        let resets: Vec<(u64, usize, f32)> = if let Some(ref mgr) = self.rust().lv2_manager {
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

        // Apply the resets
        let instance_id = resets[0].0;
        for (_, port_index, default) in &resets {
            // Update in-memory
            if let Some(ref mut mgr) = self.as_mut().rust_mut().lv2_manager {
                mgr.update_parameter(instance_id, *port_index, *default);
            }
            // Send to PipeWire RT thread
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

    /// Set a plugin parameter by stable_id + port_index.
    pub fn set_plugin_param_by_stable_id(
        mut self: Pin<&mut Self>,
        stable_id: QString,
        port_index: u32,
        value: f32,
    ) {
        let sid: String = stable_id.to_string();

        let instance_id = self
            .rust()
            .lv2_manager
            .as_ref()
            .and_then(|mgr| mgr.instance_id_for_stable_id(&sid));

        if let Some(instance_id) = instance_id {
            // Update in-memory
            if let Some(ref mut mgr) = self.as_mut().rust_mut().lv2_manager {
                mgr.update_parameter(instance_id, port_index as usize, value);
            }
            // Send to PipeWire RT thread
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

    // ── Window geometry ──────────────────────────────────────────

    /// Get saved window geometry as JSON.
    pub fn get_window_geometry_json(self: Pin<&mut Self>) -> QString {
        let path = config_path("window.json");
        let json = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => "{}".to_string(),
        };
        QString::from(&json)
    }

    /// Save window geometry from JSON string.
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

    /// Get saved viewport (pan/zoom) as JSON.
    pub fn get_viewport_json(self: Pin<&mut Self>) -> QString {
        let path = config_path("viewport.json");
        let json = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => "{}".to_string(),
        };
        QString::from(&json)
    }

    /// Save viewport (pan/zoom) from JSON string.
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

    // ── Patchbay rules ──────────────────────────────────────────

    /// Get all patchbay rules as a JSON array.
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

    /// Toggle a rule's enabled state by ID.
    pub fn toggle_rule(mut self: Pin<&mut Self>, rule_id: QString) {
        let id: String = rule_id.to_string();
        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            patchbay.toggle_rule(&id);
        }
        save_rules(self.rust().patchbay.as_ref());
    }

    /// Remove a rule by ID.
    pub fn remove_rule(mut self: Pin<&mut Self>, rule_id: QString) {
        let id: String = rule_id.to_string();
        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            patchbay.remove_rule(&id);
        }
        save_rules(self.rust().patchbay.as_ref());
    }

    /// Apply all enabled rules now.
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

    /// Snapshot current connections as rules.
    pub fn snapshot_rules(mut self: Pin<&mut Self>) {
        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            patchbay.snapshot_current_connections();
        }
        save_rules(self.rust().patchbay.as_ref());
        log::info!("Snapshot: replaced rules with current connections");
    }

    /// Toggle patchbay rules on/off.
    pub fn toggle_patchbay(mut self: Pin<&mut Self>, enabled: bool) {
        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
            patchbay.enabled = enabled;
        }
        self.as_mut().set_patchbay_enabled(enabled);
    }

    /// Get list of node names grouped by type for rule creation.
    pub fn get_node_names_json(self: Pin<&mut Self>) -> QString {
        if let Some(ref graph) = self.rust().graph {
            let nodes = graph.get_all_nodes();
            let mut entries: Vec<serde_json::Value> = nodes
                .iter()
                .filter(|n| n.ready)
                .map(|n| {
                    let type_str = match n.node_type {
                        Some(NodeType::Sink) => "Sink",
                        Some(NodeType::Source) => "Source",
                        Some(NodeType::StreamOutput) => "App Out",
                        Some(NodeType::StreamInput) => "App In",
                        Some(NodeType::Duplex) => "Duplex",
                        Some(NodeType::Lv2Plugin) => "Plugin",
                        None => "Unknown",
                    };
                    let media_str = match n.media_type {
                        Some(crate::pipewire::MediaType::Audio) => "Audio",
                        Some(crate::pipewire::MediaType::Video) => "Video",
                        Some(crate::pipewire::MediaType::Midi) => "Midi",
                        None => "Unknown",
                    };
                    serde_json::json!({
                        "name": n.display_name(),
                        "type": type_str,
                        "mediaType": media_str,
                    })
                })
                .collect();
            // Deduplicate by name+type
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

    /// Add a new rule manually.
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

    // ── Preferences ────────────────────────────────────────────────

    /// Get all preferences as a JSON object.
    pub fn get_preferences_json(self: Pin<&mut Self>) -> QString {
        let json = serde_json::to_string(&self.rust().prefs).unwrap_or_default();
        QString::from(&json)
    }

    /// Update a single preference by key.
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
            _ => {
                log::warn!("Unknown preference key: {}", key_str);
                return;
            }
        }

        log::info!("Preference updated: {} = {}", key_str, val_str);
        save_preferences(&self.rust().prefs);
    }

    /// Reset all preferences to defaults.
    pub fn reset_preferences(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().prefs = Preferences::default();
        save_preferences(&self.rust().prefs);
        log::info!("Preferences reset to defaults");
    }

    /// Get the poll interval in ms.
    pub fn get_poll_interval_ms(self: Pin<&mut Self>) -> i32 {
        self.rust().prefs.poll_interval_ms as i32
    }

    // ── Tray / window visibility ──────────────────────────────────

    /// Tell the tray about the current window visibility so left-click
    /// toggle works correctly.
    pub fn set_window_visible(self: Pin<&mut Self>, visible: bool) {
        if let Some(ref tray) = self.rust().tray_state {
            use std::sync::atomic::Ordering;
            tray.window_visible.store(visible, Ordering::Release);
            log::info!("Window visible state updated to {}", visible);
        }
    }

    // ── Helpers ────────────────────────────────────────────────────

    /// Find the LV2 instance ID for a given PipeWire node ID.
    fn find_instance_id_for_node(&self, node_id: u32) -> Option<u64> {
        if let Some(ref mgr) = self.rust().lv2_manager {
            for (id, info) in mgr.active_instances() {
                if info.pw_node_id == Some(node_id) {
                    return Some(*id);
                }
            }
        }
        None
    }

    /// Generate a unique display name for a new plugin instance.
    ///
    /// If no other active instance uses `base_name`, returns it as-is.
    /// Otherwise appends ` #2`, ` #3`, etc. until unique.
    fn unique_display_name(&self, base_name: &str) -> String {
        let existing: Vec<String> = if let Some(ref mgr) = self.rust().lv2_manager {
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

// ── Free functions ──────────────────────────────────────────────────

/// Returns the path to a config file under ~/.config/zestbay/
fn config_path(filename: &str) -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("zestbay")
        .join(filename)
}

/// A persisted plugin entry for session restore.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct SavedPlugin {
    /// Stable UUID that persists across sessions
    #[serde(default)]
    stable_id: String,
    uri: String,
    display_name: String,
    #[serde(default)]
    bypassed: bool,
    #[serde(default)]
    parameters: Vec<SavedPluginParam>,
}

/// A persisted parameter value.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct SavedPluginParam {
    port_index: usize,
    symbol: String,
    value: f32,
}

/// A persisted link between two nodes (by display name + port name).
/// Used to restore links involving LV2 plugin nodes across restarts.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct SavedPluginLink {
    output_node_name: String,
    output_port_name: String,
    input_node_name: String,
    input_port_name: String,
}

/// Load saved plugins from ~/.config/zestbay/plugins.json
fn load_saved_plugins() -> Vec<SavedPlugin> {
    let path = config_path("plugins.json");
    match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Build the current active plugin list from the Lv2Manager and save to disk.
fn persist_active_plugins(lv2_manager: Option<&Lv2Manager>) {
    let mut plugins: Vec<SavedPlugin> = if let Some(mgr) = lv2_manager {
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
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    // Sort by stable_id for deterministic file output
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

/// Load saved links from ~/.config/zestbay/links.json
fn load_saved_links() -> Vec<SavedPluginLink> {
    let path = config_path("links.json");
    match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Build the list of links that should be persisted.
///
/// This includes links that involve at least one LV2 plugin node,
/// as well as MIDI links (which are not managed by session managers
/// like WirePlumber and would be lost on restart).
fn build_persistable_links(graph: &GraphState) -> Vec<SavedPluginLink> {
    let links = graph.get_all_links();
    let mut saved_links = Vec::new();

    for link in &links {
        let out_node = graph.get_node(link.output_node_id);
        let in_node = graph.get_node(link.input_node_id);
        let out_port = graph.get_port(link.output_port_id);
        let in_port = graph.get_port(link.input_port_id);

        // Check if at least one endpoint is an LV2 plugin
        let involves_lv2 = out_node
            .as_ref()
            .map(|n| n.node_type == Some(NodeType::Lv2Plugin))
            .unwrap_or(false)
            || in_node
                .as_ref()
                .map(|n| n.node_type == Some(NodeType::Lv2Plugin))
                .unwrap_or(false);

        // Check if either port is MIDI
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
            saved_links.push(SavedPluginLink {
                output_node_name: out_node.display_name().to_string(),
                output_port_name: out_port.name.clone(),
                input_node_name: in_node.display_name().to_string(),
                input_port_name: in_port.name.clone(),
            });
        }
    }

    saved_links
}

/// Save persistable links (LV2 + MIDI) to disk.
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

/// Load patchbay rules from disk.
fn load_rules() -> Vec<crate::patchbay::rules::AutoConnectRule> {
    let path = config_path("rules.json");
    match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Save patchbay rules to disk.
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

/// Parse a node type string from QML into a NodeType.
fn parse_node_type(s: &str) -> Option<NodeType> {
    match s {
        "Sink" => Some(NodeType::Sink),
        "Source" => Some(NodeType::Source),
        "App Out" | "StreamOutput" => Some(NodeType::StreamOutput),
        "App In" | "StreamInput" => Some(NodeType::StreamInput),
        "Duplex" => Some(NodeType::Duplex),
        "Plugin" | "Lv2Plugin" => Some(NodeType::Lv2Plugin),
        _ => None,
    }
}

/// Produces a layout key like "Sink:Built-in Audio" for position persistence.
fn layout_key(node: &Node) -> String {
    let prefix = match node.node_type {
        Some(NodeType::Sink) => "Sink",
        Some(NodeType::Source) => "Source",
        Some(NodeType::StreamOutput) => "StreamOut",
        Some(NodeType::StreamInput) => "StreamIn",
        Some(NodeType::Duplex) => "Duplex",
        Some(NodeType::Lv2Plugin) => "LV2",
        None => "Unknown",
    };
    format!("{}:{}", prefix, node.display_name())
}

// ── Preferences ─────────────────────────────────────────────────────

/// User-configurable preferences, persisted to ~/.config/zestbay/preferences.json
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Preferences {
    /// Milliseconds to wait after the last graph change before auto-applying
    /// patchbay rules.  Higher values are more reliable on slow hardware;
    /// lower values make connections appear faster.
    #[serde(default = "Preferences::default_rule_settle_ms")]
    pub rule_settle_ms: u64,

    /// Milliseconds of debounce before writing plugin parameters to disk.
    #[serde(default = "Preferences::default_params_persist_ms")]
    pub params_persist_ms: u64,

    /// Milliseconds of debounce before writing LV2 links to disk.
    #[serde(default = "Preferences::default_links_persist_ms")]
    pub links_persist_ms: u64,

    /// QML poll timer interval in milliseconds (how often poll_events is called).
    #[serde(default = "Preferences::default_poll_interval_ms")]
    pub poll_interval_ms: u64,

    /// Whether to auto-learn patchbay rules from manual connections.
    #[serde(default = "Preferences::default_auto_learn_rules")]
    pub auto_learn_rules: bool,

    /// Whether to start with the window hidden (minimized to tray).
    #[serde(default = "Preferences::default_start_minimized")]
    pub start_minimized: bool,

    /// Whether closing the window hides to tray instead of quitting.
    #[serde(default = "Preferences::default_close_to_tray")]
    pub close_to_tray: bool,
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
        }
    }
}

/// Load preferences from disk, falling back to defaults.
fn load_preferences() -> Preferences {
    let path = config_path("preferences.json");
    match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Preferences::default(),
    }
}

/// Save preferences to disk.
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
