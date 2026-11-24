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
        #[qproperty(bool, patchbay_exclusive)]
        #[qproperty(bool, patchbay_hide_unnamed)]
        #[qproperty(i32, active_plugin_count)]
        #[qproperty(i32, node_count)]
        #[qproperty(i32, link_count)]
        #[qproperty(QString, cpu_usage)]
        #[qproperty(i32, desktop_color_scheme)]
        #[qproperty(i32, custom_themes_rev)]
        #[qproperty(bool, plugins_scanning)]
        #[qproperty(f64, plugin_scan_progress)]
        #[qproperty(bool, plugins_rescanning)]
        #[qproperty(f64, plugin_rescan_progress)]
        type AppController = super::AppControllerRust;

        #[qinvokable]
        fn init(self: Pin<&mut Self>);

        #[qinvokable]
        fn poll_events(self: Pin<&mut Self>);

        #[qinvokable]
        fn request_quit(self: Pin<&mut Self>);

        #[qinvokable]
        fn get_sessions_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn create_session(self: Pin<&mut Self>, name: QString) -> QString;

        #[qinvokable]
        fn delete_session(self: Pin<&mut Self>, name: QString) -> QString;

        #[qinvokable]
        fn switch_session(self: Pin<&mut Self>, name: QString) -> QString;

        #[qinvokable]
        fn restart_pipewire(self: Pin<&mut Self>);

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
        fn block_link(self: Pin<&mut Self>, link_id: u32);

        #[qinvokable]
        fn block_ports(self: Pin<&mut Self>, output_port_id: u32, input_port_id: u32);

        #[qinvokable]
        fn blocked_reason(self: Pin<&mut Self>, output_port_id: u32, input_port_id: u32)
        -> QString;

        #[qinvokable]
        fn get_blocked_pairs_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn insert_node_on_link(self: Pin<&mut Self>, link_id: u32, node_id: u32);

        #[qinvokable]
        fn insert_on_link_preview_json(self: Pin<&mut Self>, link_id: u32, node_id: u32)
        -> QString;

        #[qinvokable]
        fn get_layout_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn save_layout(self: Pin<&mut Self>, json: QString);

        #[qinvokable]
        fn get_hidden_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn save_hidden(self: Pin<&mut Self>, json: QString);

        #[qinvokable]
        fn get_docked_meters_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn save_docked_meters(self: Pin<&mut Self>, json: QString);

        #[qinvokable]
        fn get_pinned_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn save_pinned(self: Pin<&mut Self>, json: QString);

        #[qinvokable]
        fn get_node_styles_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn get_custom_themes_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn themes_folder(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn save_node_styles(self: Pin<&mut Self>, json: QString);

        #[qinvokable]
        fn get_available_plugins_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn rescan_plugins(self: Pin<&mut Self>);

        #[qinvokable]
        fn rescan_plugins_if_stale(self: Pin<&mut Self>);

        #[qinvokable]
        fn add_plugin(self: Pin<&mut Self>, uri: QString) -> QString;

        #[qinvokable]
        fn add_meter(self: Pin<&mut Self>, channels: i32) -> QString;

        #[qinvokable]
        fn rename_meter(self: Pin<&mut Self>, node_id: u32, base_name: QString) -> QString;

        #[qinvokable]
        fn meter_levels_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn remove_plugin(self: Pin<&mut Self>, node_id: u32);

        #[qinvokable]
        fn open_plugin_ui(self: Pin<&mut Self>, node_id: u32);

        #[qinvokable]
        fn rename_plugin(self: Pin<&mut Self>, node_id: u32, new_name: QString);

        #[qinvokable]
        fn rename_port(self: Pin<&mut Self>, port_id: u32, new_name: QString);

        #[qinvokable]
        fn get_plugin_params_json(self: Pin<&mut Self>, node_id: u32) -> QString;

        #[qinvokable]
        fn plugin_node_for_instance(self: Pin<&mut Self>, instance_id: u64) -> i32;

        #[qinvokable]
        fn set_plugin_parameter(self: Pin<&mut Self>, node_id: u32, port_index: u32, value: f32);

        #[qinvokable]
        fn set_plugin_bypass(self: Pin<&mut Self>, node_id: u32, bypassed: bool);

        #[qinvokable]
        fn set_plugin_enabled(self: Pin<&mut Self>, node_id: u32, enabled: bool);

        #[qinvokable]
        fn get_plugin_presets_all_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn get_plugin_presets_json(self: Pin<&mut Self>, node_id: u32) -> QString;

        #[qinvokable]
        fn apply_plugin_preset(self: Pin<&mut Self>, node_id: u32, name: QString) -> bool;

        #[qinvokable]
        fn save_plugin_preset(self: Pin<&mut Self>, node_id: u32, name: QString) -> bool;

        #[qinvokable]
        fn save_modified_presets(self: Pin<&mut Self>) -> i32;

        #[qinvokable]
        fn delete_plugin_preset(self: Pin<&mut Self>, node_id: u32, name: QString);

        #[qinvokable]
        fn start_new_plugin_preset(self: Pin<&mut Self>, node_id: u32);

        #[qinvokable]
        fn set_default_plugin_preset(self: Pin<&mut Self>, node_id: u32, name: QString);

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
        fn toggle_patchbay_exclusive(self: Pin<&mut Self>, exclusive: bool);

        #[qinvokable]
        fn toggle_patchbay_hide_unnamed(self: Pin<&mut Self>, enabled: bool);

        #[qinvokable]
        fn get_node_names_json(self: Pin<&mut Self>) -> QString;

        #[qinvokable]
        fn add_rule(
            self: Pin<&mut Self>,
            source_pattern: QString,
            source_type: QString,
            target_pattern: QString,
            target_type: QString,
            action: QString,
        );

        #[qinvokable]
        fn get_subgraphs_json(self: Pin<&mut Self>) -> QString;
        #[qinvokable]
        fn get_subgraph_view(self: Pin<&mut Self>) -> QString;
        #[qinvokable]
        fn set_subgraph_view(self: Pin<&mut Self>, subgraph_id: QString);
        #[qinvokable]
        fn infer_subgraph_io(self: Pin<&mut Self>, node_ids_json: QString) -> QString;
        #[qinvokable]
        fn create_subgraph(
            self: Pin<&mut Self>,
            node_ids_json: QString,
            config_json: QString,
        ) -> QString;
        #[qinvokable]
        fn update_subgraph(self: Pin<&mut Self>, subgraph_id: QString, config_json: QString);
        #[qinvokable]
        fn ungroup_subgraph(self: Pin<&mut Self>, subgraph_id: QString);
        #[qinvokable]
        fn delete_subgraph(self: Pin<&mut Self>, subgraph_id: QString);
        #[qinvokable]
        fn save_subgraph_view_state(self: Pin<&mut Self>, subgraph_id: QString, json: QString);
        #[qinvokable]
        fn duplicate_subgraph(
            self: Pin<&mut Self>,
            subgraph_id: QString,
            x: f64,
            y: f64,
        ) -> QString;
        #[qinvokable]
        fn save_subgraph_template(
            self: Pin<&mut Self>,
            subgraph_id: QString,
            name: QString,
        ) -> QString;
        #[qinvokable]
        fn list_subgraph_templates_json(self: Pin<&mut Self>) -> QString;
        #[qinvokable]
        fn insert_subgraph_template(self: Pin<&mut Self>, file: QString, x: f64, y: f64)
        -> QString;
        #[qinvokable]
        fn delete_subgraph_template(self: Pin<&mut Self>, file: QString);
        #[qinvokable]
        fn get_virtual_devices_json(self: Pin<&mut Self>) -> QString;
        #[qinvokable]
        fn add_virtual_device(
            self: Pin<&mut Self>,
            name: QString,
            kind: QString,
            channels: u32,
        ) -> QString;
        #[qinvokable]
        fn update_virtual_device(self: Pin<&mut Self>, id: QString, name: QString, channels: u32);
        #[qinvokable]
        fn remove_virtual_device(self: Pin<&mut Self>, id: QString);

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
            grid_size: f64,
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
        fn remove_midi_mapping_for_param(self: Pin<&mut Self>, instance_id: u64, port_index: u32);

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
        fn rules_changed(self: Pin<&mut AppController>);

        #[qsignal]
        fn show_window_requested(self: Pin<&mut AppController>);

        #[qsignal]
        fn tray_action_requested(self: Pin<&mut AppController>, action: QString);

        #[qsignal]
        fn hide_window_requested(self: Pin<&mut AppController>);

        #[qsignal]
        fn subgraphs_changed(self: Pin<&mut AppController>);

        #[qsignal]
        fn session_changing(self: Pin<&mut AppController>);

        #[qsignal]
        fn session_changed(self: Pin<&mut AppController>);

        #[qsignal]
        fn midi_mapping_added(self: Pin<&mut AppController>, mapping_json: QString);

        #[qsignal]
        fn midi_mapping_removed(self: Pin<&mut AppController>, instance_id: u64, port_index: u32);

        #[qsignal]
        fn midi_learn_started(self: Pin<&mut AppController>, instance_id: u64, port_index: u32);

        #[qsignal]
        fn midi_learn_cancelled(self: Pin<&mut AppController>);

        #[qsignal]
        fn midi_mapping_conflict(
            self: Pin<&mut AppController>,
            source_json: QString,
            existing_label: QString,
        );

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

use crate::patchbay::{PatchbayManager, rules};
use crate::pipewire::{
    GraphState, Node, NodeType, PluginEvent, Port, PortDirection, PwCommand, PwEvent,
};
use crate::plugin::PluginManager;
use crate::subgraph::{
    self as sg, NamedLink, PlanEnd, SubgraphDef, VirtualDevice, VirtualDeviceKind,
};
use crate::tray::TrayState;
use crate::ui::subgraphs::{self as sgui, Subgraphs, ViewNodeKind};

#[derive(Debug, Default)]
struct BridgeSplitState {
    virtual_to_real: HashMap<u32, (u32, String)>,
    real_to_virtual: HashMap<(u32, String), u32>,
    port_to_virtual: HashMap<u32, u32>,
    next_virtual_id: u32,
}

impl BridgeSplitState {
    const VIRTUAL_ID_BASE: u32 = 1_000_000;

    fn new() -> Self {
        Self {
            next_virtual_id: Self::VIRTUAL_ID_BASE,
            ..Default::default()
        }
    }

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
    patchbay_exclusive: bool,
    patchbay_hide_unnamed: bool,
    active_plugin_count: i32,
    node_count: i32,
    link_count: i32,
    cpu_usage: QString,
    desktop_color_scheme: i32,
    custom_themes_rev: i32,
    themes_stamp: Vec<(String, Option<std::time::SystemTime>, u64)>,
    themes_checked_at: Option<Instant>,
    plugins_scanning: bool,
    plugin_scan_progress: f64,
    plugins_rescanning: bool,
    plugin_rescan_progress: f64,
    last_plugin_scan: Option<Instant>,
    plugin_scan_rx: Option<Receiver<crate::plugin::scan::ScanEvent>>,
    pending_plugin_adds: Vec<PwCommand>,
    stashed_events: std::collections::VecDeque<PwEvent>,

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

    presets: crate::presets::PresetStore,
    preset_state_refresh: std::collections::HashMap<u64, (String, String)>,

    restore_instance_ids: std::collections::HashSet<u64>,
    restore_started_at: Option<std::time::Instant>,
    restore_batch_size: usize,
    pending_links: Vec<PendingLink>,
    last_graph_links: Vec<SavedPluginLink>,
    forgotten_link_nodes: std::collections::HashSet<String>,
    docked_meters: std::collections::HashMap<String, String>,
    reconnect_instances: std::collections::HashSet<u64>,
    links_retry_due: bool,

    links_dirty: bool,
    links_dirty_since: Option<std::time::Instant>,

    last_learned_mapping: Option<crate::midi::MidiCcMapping>,
    pending_conflict_mapping: Option<crate::midi::MidiCcMapping>,

    state_request_sent_at: Option<Instant>,
    last_state_autosave: Instant,
    plugins_load_failed: bool,

    pw_reconnecting: bool,
    links_suppressed_until: Option<Instant>,
    dormant_midi_mappings: Vec<crate::midi::MidiCcMapping>,

    prefs: Preferences,

    tray_state: Option<TrayState>,

    prev_cpu_ticks: u64,
    prev_cpu_time: Option<Instant>,
    cpu_avg: f64,
    cpu_history: Vec<f64>,

    bridge_split: BridgeSplitState,
    subgraphs: Subgraphs,

    midi_mappings: Vec<crate::midi::MidiCcMapping>,
    midi_learn_target: Option<(u64, usize, String, crate::midi::MappingMode)>,
    plugins_frozen: bool,

    osc: Option<crate::osc::OscServer>,

    session_swap: Option<SessionSwap>,
}

struct SessionSwap {
    target: String,
    started: Instant,
}

impl Default for AppControllerRust {
    fn default() -> Self {
        Self {
            patchbay_enabled: true,
            patchbay_exclusive: true,
            patchbay_hide_unnamed: true,
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
            presets: load_presets(),
            preset_state_refresh: std::collections::HashMap::new(),
            restore_instance_ids: std::collections::HashSet::new(),
            restore_started_at: None,
            restore_batch_size: 0,
            pending_links: Vec::new(),
            last_graph_links: Vec::new(),
            forgotten_link_nodes: std::collections::HashSet::new(),
            docked_meters: std::collections::HashMap::new(),
            reconnect_instances: std::collections::HashSet::new(),
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
            prefs: load_preferences(),
            tray_state: None,
            cpu_usage: QString::from("0.0%"),
            desktop_color_scheme: 0,
            custom_themes_rev: 0,
            themes_stamp: Vec::new(),
            themes_checked_at: None,
            plugins_scanning: false,
            plugin_scan_progress: -1.0,
            plugins_rescanning: false,
            plugin_rescan_progress: 0.0,
            last_plugin_scan: None,
            plugin_scan_rx: None,
            pending_plugin_adds: Vec::new(),
            stashed_events: std::collections::VecDeque::new(),
            prev_cpu_ticks: 0,
            prev_cpu_time: None,
            cpu_avg: 0.0,
            cpu_history: vec![0.0; 120],
            bridge_split: BridgeSplitState::new(),
            subgraphs: Subgraphs::default(),
            midi_mappings: Vec::new(),
            midi_learn_target: None,
            plugins_frozen: false,
            osc: None,
            session_swap: None,
        }
    }
}

mod config;
mod events;
mod graph_model;
mod layout;
mod links;
mod meters;
mod midi;
mod osc;
mod parameters;
mod patchbay_rules;
mod plugin_state;
mod plugins;
mod preferences;
mod presets;
mod routing;
mod rule_backups;
mod saved_state;
mod sessions;
mod startup;
mod stats;
mod tray;

use config::*;
use graph_model::*;
use links::*;
use meters::*;
use midi::*;
use patchbay_rules::*;
use plugin_state::*;
use preferences::*;
use presets::*;
use routing::*;
use saved_state::*;
use stats::*;

#[cfg(test)]
mod tests;

#[path = "subgraphs_bridge.rs"]
mod subgraphs_bridge;
