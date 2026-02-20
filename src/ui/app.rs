//! Main application UI
//!
//! This is the top-level UI component that manages the application state
//! and renders the interface.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

use std::time::{Duration, Instant};

use eframe::egui;

use super::graph::GraphView;
use crate::lv2::{
    Lv2Manager, Lv2PluginInfo, PluginInstanceId, SavedParameter, SavedPluginInstance,
    SavedPluginLink, SavedSession,
};
use crate::patchbay::PatchbayManager;
use crate::pipewire::{
    GraphState, Link, Lv2Event, Node, NodeType, ObjectId, Port, PortDirection, PwCommand, PwEvent,
};
use crate::tray::TrayState;

/// Settle time for auto-routing (ms)
/// We wait for the graph to stop changing for this long before applying rules.
const ROUTING_SETTLE_MS: u64 = 500;

/// Main application state
pub struct ZestBayApp {
    /// Shared graph state
    graph: Arc<GraphState>,
    /// PipeWire event receiver
    event_rx: Receiver<PwEvent>,
    /// PipeWire command sender
    cmd_tx: Sender<PwCommand>,
    /// Patchbay manager
    patchbay: PatchbayManager,
    /// LV2 plugin manager
    lv2_manager: Lv2Manager,
    /// Graph visualization
    graph_view: GraphView,
    /// Cached nodes for rendering
    nodes: Vec<Node>,
    /// Cached ports grouped by node
    ports_by_node: HashMap<ObjectId, Vec<Port>>,
    /// Cached links for rendering
    links: Vec<Link>,
    /// Last change counter (for polling updates)
    last_change_counter: u64,
    /// Last time the graph changed (for settle timer)
    last_change_time: Instant,
    /// Whether we have a pending rule application
    rules_apply_pending: bool,
    /// Show settings panel
    show_settings: bool,
    /// Show plugin browser
    show_plugin_browser: bool,
    /// Plugin browser search filter
    plugin_search: String,
    /// Plugin browser category filter
    plugin_category_filter: Option<String>,
    /// Show plugin parameters window (instance_id -> visible)
    show_plugin_params: HashMap<PluginInstanceId, bool>,
    /// Rename dialog state: (instance_id, pw_node_id, current text)
    rename_dialog: Option<(PluginInstanceId, ObjectId, String)>,
    /// Error message to display
    error_message: Option<String>,
    /// Next instance ID counter (for AddPlugin commands)
    next_instance_id: u64,
    /// Whether we need to save the session state
    session_dirty: bool,
    /// Whether we have restored the session on startup
    session_restored: bool,
    /// Pending session restore (plugins to add once PW is ready)
    pending_session: Option<SavedSession>,
    /// Number of plugins we're still waiting to come online from session restore
    pending_restore_count: usize,
    /// Set of plugin instance IDs that currently have their native UI window open
    open_plugin_uis: std::collections::HashSet<PluginInstanceId>,
    /// System tray state for show/hide/quit communication
    tray_state: TrayState,
}

impl ZestBayApp {
    /// Create a new application
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        graph: Arc<GraphState>,
        event_rx: Receiver<PwEvent>,
        cmd_tx: Sender<PwCommand>,
        lv2_manager: Lv2Manager,
        tray_state: TrayState,
    ) -> Self {
        // Try to hide from taskbar on X11/XWayland
        Self::set_skip_taskbar(cc);

        Self {
            patchbay: PatchbayManager::new(graph.clone()),
            graph,
            event_rx,
            cmd_tx,
            lv2_manager,
            graph_view: GraphView::new(),
            nodes: Vec::new(),
            ports_by_node: HashMap::new(),
            links: Vec::new(),
            last_change_counter: 0,
            last_change_time: Instant::now(),
            rules_apply_pending: false,
            show_settings: false,
            show_plugin_browser: false,
            plugin_search: String::new(),
            plugin_category_filter: None,
            show_plugin_params: HashMap::new(),
            rename_dialog: None,
            error_message: None,
            next_instance_id: 1,
            session_dirty: false,
            session_restored: false,
            pending_session: None,
            pending_restore_count: 0,
            open_plugin_uis: std::collections::HashSet::new(),
            tray_state,
        }
    }

    // ─── Skip taskbar on X11/XWayland ──────────────────────────────────────

    /// Set `_NET_WM_STATE_SKIP_TASKBAR` on the X11 window.
    ///
    /// `with_taskbar(false)` in egui is Windows-only; winit doesn't expose
    /// skip-taskbar on Linux.  We use raw Xlib FFI via the window handle
    /// that eframe provides.  The main window is forced to X11/XWayland
    /// in main() so this should always succeed.
    fn set_skip_taskbar(cc: &eframe::CreationContext<'_>) {
        use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

        let window_handle = match cc.window_handle() {
            Ok(h) => h.as_raw(),
            Err(_) => return,
        };
        let display_handle = match cc.display_handle() {
            Ok(h) => h.as_raw(),
            Err(_) => return,
        };

        match (window_handle, display_handle) {
            (
                raw_window_handle::RawWindowHandle::Xlib(win),
                raw_window_handle::RawDisplayHandle::Xlib(disp),
            ) => {
                if let Some(display) = disp.display {
                    unsafe {
                        Self::send_skip_taskbar_xlib(display.as_ptr(), win.window as u64);
                    }
                }
            }
            (
                raw_window_handle::RawWindowHandle::Xcb(win),
                raw_window_handle::RawDisplayHandle::Xlib(disp),
            ) => {
                // winit may use xcb windows with xlib display
                if let Some(display) = disp.display {
                    unsafe {
                        Self::send_skip_taskbar_xlib(display.as_ptr(), win.window.get() as u64);
                    }
                }
            }
            _ => {
                log::info!(
                    "Not on X11/XWayland — skip-taskbar requires a KWin window rule \
                     (match app_id \"zestbay\", force Skip Taskbar = Yes)"
                );
            }
        }
    }

    /// Send the `_NET_WM_STATE` client message to add SKIP_TASKBAR + SKIP_PAGER.
    ///
    /// # Safety
    /// `display` must be a valid Xlib `Display*` and `window` a valid X11 window.
    unsafe fn send_skip_taskbar_xlib(display: *mut std::ffi::c_void, window: u64) {
        type XDisplay = *mut std::ffi::c_void;
        type XWindow = u64;
        type Atom = u64;

        // Minimal Xlib FFI — link against libX11 for the few functions we need.
        #[link(name = "X11")]
        unsafe extern "C" {
            fn XInternAtom(display: XDisplay, name: *const i8, only_if_exists: i32) -> Atom;
            fn XDefaultRootWindow(display: XDisplay) -> XWindow;
            fn XSendEvent(
                display: XDisplay,
                window: XWindow,
                propagate: i32,
                event_mask: i64,
                event: *mut XClientMessageEvent,
            ) -> i32;
            fn XFlush(display: XDisplay) -> i32;
        }

        #[repr(C)]
        struct XClientMessageEvent {
            type_: i32,          // ClientMessage = 33
            serial: u64,
            send_event: i32,
            display: XDisplay,
            window: XWindow,
            message_type: Atom,
            format: i32,
            data: [i64; 5],
        }

        const CLIENT_MESSAGE: i32 = 33;
        const NET_WM_STATE_ADD: i64 = 1;
        const SUBSTRUCTURE_REDIRECT_MASK: i64 = 1 << 20;
        const SUBSTRUCTURE_NOTIFY_MASK: i64 = 1 << 19;

        unsafe {
            let state_atom = XInternAtom(display, c"_NET_WM_STATE".as_ptr(), 0);
            let skip_taskbar =
                XInternAtom(display, c"_NET_WM_STATE_SKIP_TASKBAR".as_ptr(), 0);
            let skip_pager = XInternAtom(display, c"_NET_WM_STATE_SKIP_PAGER".as_ptr(), 0);
            let root = XDefaultRootWindow(display);

            let mut event = std::mem::zeroed::<XClientMessageEvent>();
            event.type_ = CLIENT_MESSAGE;
            event.window = window;
            event.message_type = state_atom;
            event.format = 32;
            event.data[0] = NET_WM_STATE_ADD;
            event.data[1] = skip_taskbar as i64;
            event.data[2] = skip_pager as i64;
            event.data[3] = 1; // source indication: normal application

            XSendEvent(
                display,
                root,
                0,
                SUBSTRUCTURE_REDIRECT_MASK | SUBSTRUCTURE_NOTIFY_MASK,
                &mut event,
            );
            XFlush(display);
        }

        log::info!("Set _NET_WM_STATE_SKIP_TASKBAR on X11 window");
    }

    /// Process pending PipeWire events
    fn process_events(&mut self) {
        let mut changed = false;

        // Process all pending events
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                PwEvent::NodeChanged(_)
                | PwEvent::NodeRemoved(_)
                | PwEvent::PortChanged(_)
                | PwEvent::PortRemoved { .. }
                | PwEvent::LinkChanged(_)
                | PwEvent::LinkRemoved(_) => {
                    // State is updated by the pipewire thread
                    changed = true;
                }
                PwEvent::Error(msg) => {
                    self.error_message = Some(msg);
                }
                PwEvent::BatchComplete => {
                    changed = true;
                }
                PwEvent::Lv2(lv2_event) => {
                    self.handle_lv2_event(lv2_event);
                    changed = true;
                }
            }
        }

        // Check if state has changed
        let current_counter = self.graph.change_counter();
        if current_counter != self.last_change_counter || changed {
            self.last_change_counter = current_counter;
            self.last_change_time = Instant::now();
            self.rules_apply_pending = true;
            self.refresh_cache();
        }
    }

    /// Handle LV2-specific events from the PipeWire thread
    fn handle_lv2_event(&mut self, event: Lv2Event) {
        match event {
            Lv2Event::PluginAdded {
                instance_id,
                pw_node_id,
                display_name,
            } => {
                log::info!(
                    "LV2 plugin added: {} (instance {}, pw_node {})",
                    display_name,
                    instance_id,
                    pw_node_id
                );
                // SPA_ID_INVALID = 0xffffffff, also treat 0 as invalid
                if pw_node_id != 0 && pw_node_id != u32::MAX {
                    self.lv2_manager
                        .set_instance_pw_node_id(instance_id, pw_node_id);
                    // Ensure the node in GraphState is marked as Lv2Plugin.
                    // The registry listener may have classified it as Duplex
                    // if the custom property wasn't visible in the registry.
                    self.graph.set_node_type(pw_node_id, NodeType::Lv2Plugin);
                }

                // Restore parameters for this instance (session restore)
                self.restore_instance_params(instance_id);

                // Track pending restore count for link restoration
                if self.pending_restore_count > 0 {
                    self.pending_restore_count -= 1;
                    if self.pending_restore_count == 0 {
                        // All plugins from session are online, restore links
                        // (defer by one frame to let ports register)
                    }
                }

                self.session_dirty = true;
            }
            Lv2Event::PluginRemoved { instance_id } => {
                log::info!("LV2 plugin removed: instance {}", instance_id);
                self.lv2_manager.remove_instance(instance_id);
                self.show_plugin_params.remove(&instance_id);
                self.open_plugin_uis.remove(&instance_id);
                self.session_dirty = true;
            }
            Lv2Event::ParameterChanged {
                instance_id,
                port_index,
                value,
            } => {
                self.lv2_manager
                    .update_parameter(instance_id, port_index, value);
                self.session_dirty = true;
            }
            Lv2Event::PluginUiOpened { instance_id } => {
                log::info!("Plugin UI opened for instance {}", instance_id);
                self.open_plugin_uis.insert(instance_id);
            }
            Lv2Event::PluginUiClosed { instance_id } => {
                log::info!("Plugin UI closed for instance {}", instance_id);
                self.open_plugin_uis.remove(&instance_id);
            }
            Lv2Event::PluginError {
                instance_id,
                message,
            } => {
                log::error!("LV2 plugin error (instance {:?}): {}", instance_id, message);
                self.error_message = Some(format!("Plugin error: {}", message));
            }
        }
    }

    /// Refresh cached data from graph state
    fn refresh_cache(&mut self) {
        self.nodes = self.graph.get_all_nodes();
        self.links = self.graph.get_all_links();

        // Group ports by node, sorted: inputs first (alphabetical), then outputs (alphabetical)
        self.ports_by_node.clear();
        for node in &self.nodes {
            let mut ports = self.graph.get_ports_for_node(node.id);
            ports.sort_by(|a, b| {
                a.direction.cmp(&b.direction).then_with(|| {
                    a.display_name()
                        .to_lowercase()
                        .cmp(&b.display_name().to_lowercase())
                })
            });
            self.ports_by_node.insert(node.id, ports);
        }

        // Auto-layout new nodes
        self.graph_view
            .auto_layout(&self.nodes, &self.ports_by_node);
    }

    /// Render the top menu bar
    fn render_menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Save Session").clicked() {
                        self.save_session();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("View", |ui| {
                    if ui.button("Auto Layout").clicked() {
                        self.graph_view = GraphView::new();
                        self.graph_view
                            .auto_layout(&self.nodes, &self.ports_by_node);
                        ui.close_menu();
                    }
                });

                ui.menu_button("Plugins", |ui| {
                    if ui.button("Plugin Browser...").clicked() {
                        self.show_plugin_browser = true;
                        ui.close_menu();
                    }
                    if ui.button("Rescan Plugins").clicked() {
                        self.lv2_manager.rescan();
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.label(format!(
                        "{} plugins available",
                        self.lv2_manager.available_plugins().len()
                    ));

                    let active_count = self.lv2_manager.active_instances().len();
                    if active_count > 0 {
                        ui.separator();
                        ui.label(format!("{} active instances", active_count));

                        // List active instances with controls
                        let instance_ids: Vec<_> = self
                            .lv2_manager
                            .active_instances()
                            .keys()
                            .copied()
                            .collect();
                        for id in instance_ids {
                            if let Some(info) = self.lv2_manager.get_instance(id) {
                                let name = info.display_name.clone();
                                ui.horizontal(|ui| {
                                    if ui.small_button("P").clicked() {
                                        self.show_plugin_params.insert(id, true);
                                    }
                                    if ui.small_button("X").clicked() {
                                        let _ = self
                                            .cmd_tx
                                            .send(PwCommand::RemovePlugin { instance_id: id });
                                    }
                                    ui.label(&name);
                                });
                            }
                        }
                    }
                });

                ui.menu_button("Patchbay", |ui| {
                    let _enabled = self.patchbay.enabled;
                    if ui
                        .checkbox(&mut self.patchbay.enabled, "Enable Rules")
                        .changed()
                    {
                        // Rules toggled
                    }
                    ui.separator();
                    if ui.button("Edit Rules...").clicked() {
                        self.show_settings = true;
                        ui.close_menu();
                    }
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let active = self.lv2_manager.active_instances().len();
                    if active > 0 {
                        ui.label(format!("LV2: {} active", active));
                        ui.separator();
                    }

                    let status = if self.patchbay.enabled {
                        "● Rules Active"
                    } else {
                        "○ Rules Disabled"
                    };
                    ui.label(status);
                });
            });
        });
    }

    /// Render the plugin browser panel
    fn render_plugin_browser(&mut self, ctx: &egui::Context) {
        // add_request is declared outside the .show() closure so we can
        // process it after the window is drawn (avoids &mut self borrow
        // conflict between .open() and methods called inside the closure).
        let mut add_request: Option<String> = None;

        egui::Window::new("Plugin Browser")
            .open(&mut self.show_plugin_browser)
            .resizable(true)
            .default_width(500.0)
            .default_height(600.0)
            .show(ctx, |ui| {
                // Search bar
                ui.horizontal(|ui| {
                    ui.label("Search:");
                    ui.text_edit_singleline(&mut self.plugin_search);
                    if ui.button("Clear").clicked() {
                        self.plugin_search.clear();
                    }
                });

                // Category filter dropdown
                ui.horizontal(|ui| {
                    ui.label("Category:");

                    // Collect unique categories
                    let mut categories: Vec<String> = self
                        .lv2_manager
                        .available_plugins()
                        .iter()
                        .map(|p| p.category.display_name().to_string())
                        .collect();
                    categories.sort();
                    categories.dedup();

                    let selected_text = self.plugin_category_filter.as_deref().unwrap_or("All");

                    egui::ComboBox::from_id_salt("plugin_category")
                        .selected_text(selected_text)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_value(&mut self.plugin_category_filter, None, "All")
                                .clicked()
                            {
                                ui.close_menu();
                            }
                            for cat in &categories {
                                let value = Some(cat.clone());
                                if ui
                                    .selectable_value(&mut self.plugin_category_filter, value, cat)
                                    .clicked()
                                {
                                    ui.close_menu();
                                }
                            }
                        });
                });

                ui.separator();

                // Plugin count - clone filtered results to avoid borrow issues
                let search_lower = self.plugin_search.to_lowercase();
                let cat_filter = self.plugin_category_filter.clone();
                let filtered: Vec<Lv2PluginInfo> = self
                    .lv2_manager
                    .available_plugins()
                    .iter()
                    .filter(|p| {
                        if !search_lower.is_empty() {
                            let matches_name = p.name.to_lowercase().contains(&search_lower);
                            let matches_uri = p.uri.to_lowercase().contains(&search_lower);
                            let matches_author = p
                                .author
                                .as_ref()
                                .is_some_and(|a| a.to_lowercase().contains(&search_lower));
                            if !(matches_name || matches_uri || matches_author) {
                                return false;
                            }
                        }
                        if let Some(ref cat_filter) = cat_filter {
                            if p.category.display_name() != cat_filter.as_str() {
                                return false;
                            }
                        }
                        true
                    })
                    .cloned()
                    .collect();

                let total = self.lv2_manager.available_plugins().len();
                ui.label(format!("Showing {} of {} plugins", filtered.len(), total));
                ui.separator();

                // Plugin list
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for plugin in &filtered {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.strong(&plugin.name);
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.button("Add").clicked() {
                                            add_request = Some(plugin.uri.clone());
                                        }
                                    },
                                );
                            });

                            ui.horizontal(|ui| {
                                ui.label(format!("[{}]", plugin.category.display_name()));
                                if let Some(ref author) = plugin.author {
                                    ui.label(format!("by {}", author));
                                }
                            });

                            ui.horizontal(|ui| {
                                ui.label(format!(
                                    "Audio: {}in/{}out",
                                    plugin.audio_inputs, plugin.audio_outputs
                                ));
                                ui.label(format!(
                                    "Control: {}in/{}out",
                                    plugin.control_inputs, plugin.control_outputs
                                ));
                            });

                            ui.label(
                                egui::RichText::new(&plugin.uri)
                                    .small()
                                    .color(egui::Color32::GRAY),
                            );
                        });
                    }
                });
            });

        // Process add request outside the window closure
        if let Some(uri) = add_request {
            self.add_plugin_from_browser(uri);
        }
    }

    /// Render plugin parameter windows
    fn render_plugin_params(&mut self, ctx: &egui::Context) {
        let instance_ids: Vec<PluginInstanceId> = self.show_plugin_params.keys().copied().collect();

        for instance_id in instance_ids {
            let mut open = self
                .show_plugin_params
                .get(&instance_id)
                .copied()
                .unwrap_or(false);

            if !open {
                continue;
            }

            let instance_info = match self.lv2_manager.get_instance(instance_id) {
                Some(info) => info.clone(),
                None => {
                    self.show_plugin_params.remove(&instance_id);
                    continue;
                }
            };

            let window_title = format!("{} [{}]", instance_info.display_name, instance_id);

            egui::Window::new(&window_title)
                .id(egui::Id::new(("plugin_params", instance_id)))
                .open(&mut open)
                .resizable(true)
                .default_width(350.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Plugin:");
                        ui.strong(&instance_info.display_name);
                    });

                    ui.horizontal(|ui| {
                        ui.label("URI:");
                        ui.label(
                            egui::RichText::new(&instance_info.plugin_uri)
                                .small()
                                .color(egui::Color32::GRAY),
                        );
                    });

                    // Bypass toggle
                    let mut bypassed = instance_info.bypassed;
                    if ui.checkbox(&mut bypassed, "Bypass").changed() {
                        let _ = self.cmd_tx.send(PwCommand::SetPluginBypass {
                            instance_id,
                            bypassed,
                        });
                        self.session_dirty = true;
                    }

                    // Remove button
                    if ui
                        .button(egui::RichText::new("Remove Plugin").color(egui::Color32::RED))
                        .clicked()
                    {
                        let _ = self.cmd_tx.send(PwCommand::RemovePlugin { instance_id });
                        self.session_dirty = true;
                    }

                    ui.separator();
                    ui.heading("Parameters");

                    if instance_info.parameters.is_empty() {
                        ui.label("No control parameters");
                    } else {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for param in &instance_info.parameters {
                                ui.horizontal(|ui| {
                                    ui.label(&param.name);
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.label(format!("{:.3}", param.value));
                                        },
                                    );
                                });

                                let mut value = param.value;
                                let range = param.min..=param.max;
                                let slider = egui::Slider::new(&mut value, range)
                                    .show_value(false)
                                    .clamping(egui::SliderClamping::Always);

                                if ui.add(slider).changed() {
                                    let _ = self.cmd_tx.send(PwCommand::SetPluginParameter {
                                        instance_id,
                                        port_index: param.port_index,
                                        value,
                                    });
                                    self.lv2_manager.update_parameter(
                                        instance_id,
                                        param.port_index,
                                        value,
                                    );
                                    self.session_dirty = true;
                                }

                                // Reset to default on double-click
                                if ui.small_button("Reset").clicked() {
                                    let _ = self.cmd_tx.send(PwCommand::SetPluginParameter {
                                        instance_id,
                                        port_index: param.port_index,
                                        value: param.default,
                                    });
                                    self.lv2_manager.update_parameter(
                                        instance_id,
                                        param.port_index,
                                        param.default,
                                    );
                                    self.session_dirty = true;
                                }

                                ui.separator();
                            }
                        });
                    }
                });

            self.show_plugin_params.insert(instance_id, open);
        }

        // Clean up closed windows
        self.show_plugin_params.retain(|_, open| *open);
    }

    /// Render the settings panel
    fn render_settings(&mut self, ctx: &egui::Context) {
        // Collect actions from the UI closure, apply them after.
        // This avoids the borrow conflict between .open(&mut self.show_settings)
        // and the closure needing &mut self.
        let mut rule_to_delete: Option<String> = None;
        let mut rule_to_toggle: Option<String> = None;
        let mut apply_rules = false;
        let mut snapshot_rules = false;

        let rules: Vec<_> = self.patchbay.rules().to_vec();

        egui::Window::new("Patchbay Settings")
            .open(&mut self.show_settings)
            .resizable(true)
            .default_width(500.0)
            .show(ctx, |ui| {
                ui.heading("Auto-Connect Rules");
                ui.label("Rules are auto-learned when you connect ports manually.");

                ui.separator();

                // ── Rule list ──────────────────────────────────────────────
                if rules.is_empty() {
                    ui.label("No rules yet. Connect some ports to create rules automatically.");
                } else {
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            egui::Grid::new("rules_grid")
                                .num_columns(4)
                                .spacing([8.0, 4.0])
                                .striped(true)
                                .show(ui, |ui| {
                                    // Header
                                    ui.strong("Source");
                                    ui.strong("Target");
                                    ui.strong("On");
                                    ui.strong("");
                                    ui.end_row();

                                    for rule in &rules {
                                        ui.label(rule.source_label());
                                        ui.label(format!("-> {}", rule.target_label()));

                                        let mut enabled = rule.enabled;
                                        if ui.checkbox(&mut enabled, "").changed() {
                                            rule_to_toggle = Some(rule.id.clone());
                                        }

                                        if ui.small_button("X").clicked() {
                                            rule_to_delete = Some(rule.id.clone());
                                        }
                                        ui.end_row();
                                    }
                                });
                        });
                }

                ui.separator();

                // ── Action buttons ─────────────────────────────────────────
                ui.horizontal(|ui| {
                    if ui.button("Apply Rules Now").clicked() {
                        apply_rules = true;
                    }

                    if ui
                        .button("Replace rules with current connections")
                        .clicked()
                    {
                        snapshot_rules = true;
                    }
                });
            });

        // ── Apply deferred actions ─────────────────────────────────────────
        if let Some(id) = rule_to_toggle {
            self.patchbay.toggle_rule(&id);
            self.save_rules();
        }
        if let Some(id) = rule_to_delete {
            self.patchbay.remove_rule(&id);
            self.save_rules();
        }
        if apply_rules {
            let commands = self.patchbay.scan();
            for cmd in commands {
                let _ = self.cmd_tx.send(cmd);
            }
        }
        if snapshot_rules {
            self.patchbay.snapshot_current_connections();
            self.save_rules();
            log::info!(
                "Replaced rules with {} current connections",
                self.patchbay.rules().len()
            );
        }
    }

    // ─── Rules persistence ─────────────────────────────────────────────────

    fn rules_path() -> Option<std::path::PathBuf> {
        dirs::config_dir().map(|d| d.join("zestbay").join("rules.json"))
    }

    fn save_rules(&mut self) {
        self.patchbay.rules_dirty = false;
        let path = match Self::rules_path() {
            Some(p) => p,
            None => return,
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let rules = self.patchbay.rules().to_vec();
        if let Ok(json) = serde_json::to_string_pretty(&rules) {
            let _ = std::fs::write(&path, json);
        }
    }

    fn load_rules(&mut self) {
        let path = match Self::rules_path() {
            Some(p) => p,
            None => return,
        };
        if let Ok(text) = std::fs::read_to_string(&path)
            && let Ok(rules) =
                serde_json::from_str::<Vec<crate::patchbay::rules::AutoConnectRule>>(&text)
        {
            log::info!("Loaded {} patchbay rules", rules.len());
            self.patchbay.set_rules(rules);
            self.patchbay.rules_dirty = false;
        }
    }

    /// Add a plugin from the browser (extracted to avoid borrow conflicts
    /// with the plugin browser closure).
    fn add_plugin_from_browser(&mut self, uri: String) {
        let instance_id = self.next_instance_id;
        self.next_instance_id += 1;

        log::info!(
            "Requesting to add plugin: {} (instance {})",
            uri,
            instance_id
        );

        // Register in our manager first (with no PW node ID yet)
        if let Some(plugin_info) = self.lv2_manager.find_plugin(&uri).cloned() {
            let params = plugin_info
                .ports
                .iter()
                .filter(|p| p.port_type == crate::lv2::Lv2PortType::ControlInput)
                .map(|p| crate::lv2::Lv2ParameterValue {
                    port_index: p.index,
                    symbol: p.symbol.clone(),
                    name: p.name.clone(),
                    value: p.default_value,
                    min: p.min_value,
                    max: p.max_value,
                    default: p.default_value,
                })
                .collect();

            let display_name = self.unique_display_name(&plugin_info.name);

            let instance_info = crate::lv2::Lv2InstanceInfo {
                id: instance_id,
                plugin_uri: uri.clone(),
                display_name: display_name.clone(),
                pw_node_id: None,
                parameters: params,
                active: false,
                bypassed: false,
            };

            self.lv2_manager.register_instance(instance_info);

            let _ = self.cmd_tx.send(PwCommand::AddPlugin {
                plugin_uri: uri,
                instance_id,
                display_name,
            });
        }

        self.session_dirty = true;
    }

    /// Generate a unique display name for a new plugin instance.
    ///
    /// If no other active instance uses `base_name`, returns it as-is.
    /// Otherwise appends ` #2`, ` #3`, etc. until unique.
    fn unique_display_name(&self, base_name: &str) -> String {
        let existing: Vec<&str> = self
            .lv2_manager
            .active_instances()
            .values()
            .map(|info| info.display_name.as_str())
            .collect();

        if !existing.contains(&base_name) {
            return base_name.to_string();
        }

        // Find the next available number
        for n in 2.. {
            let candidate = format!("{} #{}", base_name, n);
            if !existing.contains(&candidate.as_str()) {
                return candidate;
            }
        }
        unreachable!()
    }

    /// Find LV2 instance ID by PipeWire node ID
    fn find_instance_by_pw_node(&self, pw_node_id: ObjectId) -> Option<PluginInstanceId> {
        self.lv2_manager
            .active_instances()
            .iter()
            .find(|(_, info)| info.pw_node_id == Some(pw_node_id))
            .map(|(&id, _)| id)
    }

    /// Open the native LV2 plugin UI for an instance
    fn open_plugin_ui(&mut self, instance_id: PluginInstanceId) {
        if let Some(info) = self.lv2_manager.get_instance(instance_id) {
            log::info!(
                "Opening native UI for plugin: {} (instance {})",
                info.display_name,
                instance_id
            );
            let _ = self.cmd_tx.send(PwCommand::OpenPluginUI { instance_id });
        }
    }

    // ─── Session persistence ───────────────────────────────────────────────

    fn session_path() -> Option<std::path::PathBuf> {
        dirs::config_dir().map(|d| d.join("zestbay").join("session.json"))
    }

    fn load_session() -> Option<SavedSession> {
        let path = Self::session_path()?;
        let text = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn save_session(&self) {
        let path = match Self::session_path() {
            Some(p) => p,
            None => return,
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let session = self.build_session_state();
        if let Ok(json) = serde_json::to_string_pretty(&session) {
            let _ = std::fs::write(&path, json);
        }
    }

    fn build_session_state(&self) -> SavedSession {
        let mut plugins = Vec::new();

        for (_id, info) in self.lv2_manager.active_instances() {
            let params: Vec<SavedParameter> = info
                .parameters
                .iter()
                .map(|p| SavedParameter {
                    port_index: p.port_index,
                    symbol: p.symbol.clone(),
                    value: p.value,
                })
                .collect();

            plugins.push(SavedPluginInstance {
                plugin_uri: info.plugin_uri.clone(),
                display_name: info.display_name.clone(),
                bypassed: info.bypassed,
                parameters: params,
            });
        }

        // Save links that involve LV2 plugin nodes
        let mut links = Vec::new();
        for link in &self.links {
            let out_node = self.nodes.iter().find(|n| n.id == link.output_node_id);
            let in_node = self.nodes.iter().find(|n| n.id == link.input_node_id);

            let involves_lv2 = out_node
                .map(|n| n.node_type == Some(NodeType::Lv2Plugin))
                .unwrap_or(false)
                || in_node
                    .map(|n| n.node_type == Some(NodeType::Lv2Plugin))
                    .unwrap_or(false);

            if involves_lv2 {
                let out_port = self
                    .ports_by_node
                    .get(&link.output_node_id)
                    .and_then(|ports| ports.iter().find(|p| p.id == link.output_port_id));
                let in_port = self
                    .ports_by_node
                    .get(&link.input_node_id)
                    .and_then(|ports| ports.iter().find(|p| p.id == link.input_port_id));

                if let (Some(out_node), Some(in_node), Some(out_port), Some(in_port)) =
                    (out_node, in_node, out_port, in_port)
                {
                    links.push(SavedPluginLink {
                        output_node_name: out_node.display_name().to_string(),
                        output_port_name: out_port.name.clone(),
                        input_node_name: in_node.display_name().to_string(),
                        input_port_name: in_port.name.clone(),
                    });
                }
            }
        }

        SavedSession { plugins, links }
    }

    fn restore_session(&mut self) {
        let session = match Self::load_session() {
            Some(s) => s,
            None => return,
        };

        if session.plugins.is_empty() {
            return;
        }

        log::info!(
            "Restoring session: {} plugins, {} links",
            session.plugins.len(),
            session.links.len()
        );

        self.pending_restore_count = session.plugins.len();

        for saved in &session.plugins {
            let instance_id = self.next_instance_id;
            self.next_instance_id += 1;

            // Register in the LV2 manager
            if let Some(plugin_info) = self.lv2_manager.find_plugin(&saved.plugin_uri).cloned() {
                let params = plugin_info
                    .ports
                    .iter()
                    .filter(|p| p.port_type == crate::lv2::Lv2PortType::ControlInput)
                    .map(|p| {
                        // Restore saved value if available, otherwise use default
                        let value = saved
                            .parameters
                            .iter()
                            .find(|sp| sp.symbol == p.symbol)
                            .map(|sp| sp.value)
                            .unwrap_or(p.default_value);

                        crate::lv2::Lv2ParameterValue {
                            port_index: p.index,
                            symbol: p.symbol.clone(),
                            name: p.name.clone(),
                            value,
                            min: p.min_value,
                            max: p.max_value,
                            default: p.default_value,
                        }
                    })
                    .collect();

                // Use the saved display_name (which includes the auto-
                // numbered suffix like " #2") so that link restoration
                // can match nodes by their unique display names.
                let display_name = if saved.display_name.is_empty() {
                    self.unique_display_name(&plugin_info.name)
                } else {
                    saved.display_name.clone()
                };

                let instance_info = crate::lv2::Lv2InstanceInfo {
                    id: instance_id,
                    plugin_uri: saved.plugin_uri.clone(),
                    display_name: display_name.clone(),
                    pw_node_id: None,
                    parameters: params,
                    active: false,
                    bypassed: saved.bypassed,
                };

                self.lv2_manager.register_instance(instance_info);

                let _ = self.cmd_tx.send(PwCommand::AddPlugin {
                    plugin_uri: saved.plugin_uri.clone(),
                    instance_id,
                    display_name,
                });
            }
        }

        // Store the session for link restoration after plugins are online
        self.pending_session = Some(session);
        self.session_restored = true;
    }

    fn try_restore_links(&mut self) {
        let session = match self.pending_session.take() {
            Some(s) => s,
            None => return,
        };

        for saved_link in &session.links {
            // Find nodes by display name
            let out_node = self
                .nodes
                .iter()
                .find(|n| n.display_name() == saved_link.output_node_name);
            let in_node = self
                .nodes
                .iter()
                .find(|n| n.display_name() == saved_link.input_node_name);

            if let (Some(out_node), Some(in_node)) = (out_node, in_node) {
                let out_port = self.ports_by_node.get(&out_node.id).and_then(|ports| {
                    ports.iter().find(|p| {
                        p.name == saved_link.output_port_name
                            && p.direction == PortDirection::Output
                    })
                });
                let in_port = self.ports_by_node.get(&in_node.id).and_then(|ports| {
                    ports.iter().find(|p| {
                        p.name == saved_link.input_port_name && p.direction == PortDirection::Input
                    })
                });

                if let (Some(out_port), Some(in_port)) = (out_port, in_port) {
                    log::info!(
                        "Restoring link: {}:{} -> {}:{}",
                        saved_link.output_node_name,
                        saved_link.output_port_name,
                        saved_link.input_node_name,
                        saved_link.input_port_name
                    );
                    let _ = self.cmd_tx.send(PwCommand::Connect {
                        output_port_id: out_port.id,
                        input_port_id: in_port.id,
                    });
                }
            }
        }
    }

    /// Restore parameter values for a plugin instance that just came online
    fn restore_instance_params(&mut self, instance_id: PluginInstanceId) {
        if let Some(info) = self.lv2_manager.get_instance(instance_id) {
            // Send saved parameter values to the PW thread
            for param in &info.parameters {
                let _ = self.cmd_tx.send(PwCommand::SetPluginParameter {
                    instance_id,
                    port_index: param.port_index,
                    value: param.value,
                });
            }

            // Send bypass state
            if info.bypassed {
                let _ = self.cmd_tx.send(PwCommand::SetPluginBypass {
                    instance_id,
                    bypassed: true,
                });
            }
        }
    }

    /// Apply a rename to a plugin instance, updating both the Lv2InstanceInfo
    /// and the GraphState node description.
    fn apply_rename(
        &mut self,
        instance_id: PluginInstanceId,
        pw_node_id: ObjectId,
        new_name: String,
    ) {
        if let Some(info) = self.lv2_manager.get_instance_mut(instance_id) {
            info.display_name = new_name.clone();
        }
        self.graph.set_node_description(pw_node_id, &new_name);
        self.session_dirty = true;
    }

    /// Render the rename dialog window
    fn render_rename_dialog(&mut self, ctx: &egui::Context) {
        if self.rename_dialog.is_none() {
            return;
        }

        let mut apply = false;
        let mut cancel = false;

        let (instance_id, pw_node_id, _) = self.rename_dialog.as_ref().unwrap().clone();

        egui::Window::new("Rename Plugin")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    let (_, _, text) = self.rename_dialog.as_mut().unwrap();
                    let te = ui.text_edit_singleline(text);
                    // Auto-focus the text field on first frame
                    te.request_focus();
                    // Enter key applies
                    if te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        apply = true;
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        apply = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });

        if apply {
            let new_name = self.rename_dialog.as_ref().unwrap().2.clone();
            if !new_name.is_empty() {
                self.apply_rename(instance_id, pw_node_id, new_name);
            }
            self.rename_dialog = None;
        } else if cancel {
            self.rename_dialog = None;
        }
    }

    /// Render error dialog
    fn render_error(&mut self, ctx: &egui::Context) {
        if let Some(ref msg) = self.error_message.clone() {
            egui::Window::new("Error")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(msg);
                    if ui.button("OK").clicked() {
                        self.error_message = None;
                    }
                });
        }
    }
}

impl eframe::App for ZestBayApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ── System tray integration ────────────────────────────────────────
        let quitting = self
            .tray_state
            .quit_requested
            .load(std::sync::atomic::Ordering::Acquire);

        // Intercept window close: minimize to tray instead of quitting.
        // When the user actually chose "Quit" from the tray, let the close
        // proceed — otherwise CancelClose would fight with Close forever.
        if ctx.input(|i| i.viewport().close_requested()) && !quitting {
            log::info!("Close requested — cancelling and hiding to tray");
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            // Also minimize in case Visible(false) doesn't fully hide on Wayland
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            self.tray_state
                .window_visible
                .store(false, std::sync::atomic::Ordering::Relaxed);
        }

        // Quit requested from tray — send Close and stop processing this frame.
        if quitting {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        // Check if the tray requested we show the window.
        if self
            .tray_state
            .show_requested
            .swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            log::info!("Show requested from tray — making visible and focusing");
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            self.tray_state
                .window_visible
                .store(true, std::sync::atomic::Ordering::Relaxed);
            ctx.request_repaint();
        }

        // Restore session and rules on first frame (after PW connection is established)
        if !self.session_restored {
            self.restore_session();
            self.load_rules();
        }

        // Process PipeWire events
        self.process_events();

        // Handle auto-routing settle timer
        if self.rules_apply_pending && self.patchbay.enabled {
            if self.last_change_time.elapsed() >= Duration::from_millis(ROUTING_SETTLE_MS) {
                log::info!("Auto-applying patchbay rules");
                let commands = self.patchbay.scan();
                for cmd in commands {
                    let _ = self.cmd_tx.send(cmd);
                }
                self.rules_apply_pending = false;
            }
        }

        // Try restoring links once all plugins are online and we have port data
        if self.pending_restore_count == 0 && self.pending_session.is_some() {
            self.try_restore_links();
        }

        // Save session if dirty
        if self.session_dirty {
            self.save_session();
            self.session_dirty = false;
        }

        // Request repaint for continuous updates
        ctx.request_repaint_after(std::time::Duration::from_millis(100));

        // Render UI
        self.render_menu_bar(ctx);
        self.render_error(ctx);

        if self.show_settings {
            self.render_settings(ctx);
        }

        if self.show_plugin_browser {
            self.render_plugin_browser(ctx);
        }

        // Render plugin parameter windows
        self.render_plugin_params(ctx);

        // Main graph panel
        egui::CentralPanel::default().show(ctx, |ui| {
            let response = self
                .graph_view
                .show(ui, &self.nodes, &self.ports_by_node, &self.links);

            // Handle graph interactions
            if let Some((output_port, input_port)) = response.connect_request {
                log::info!("Connect request: {} -> {}", output_port, input_port);
                let _ = self.cmd_tx.send(PwCommand::Connect {
                    output_port_id: output_port,
                    input_port_id: input_port,
                });
                self.session_dirty = true;

                // Auto-learn: create/update a patchbay rule from this manual connection
                if let Some(out_port) = self.graph.get_port(output_port)
                    && let Some(in_port) = self.graph.get_port(input_port)
                    && let Some(source_node) = self.graph.get_node(out_port.node_id)
                    && let Some(target_node) = self.graph.get_node(in_port.node_id)
                    && self.patchbay.learn_from_link(
                        &source_node,
                        &target_node,
                        &out_port,
                        &in_port,
                    )
                {
                    log::info!(
                        "Auto-learned rule: {}:{} -> {}:{}",
                        source_node.display_name(),
                        out_port.name,
                        target_node.display_name(),
                        in_port.name,
                    );
                    self.save_rules();
                }
            }

            if let Some(link_id) = response.disconnect_request {
                log::info!("Disconnect request: {}", link_id);
                let _ = self.cmd_tx.send(PwCommand::Disconnect { link_id });
                self.session_dirty = true;
            }

            // Handle LV2 plugin button clicks from graph nodes
            if let Some(pw_node_id) = response.plugin_params_clicked {
                // Find the instance_id for this PW node
                if let Some(instance_id) = self.find_instance_by_pw_node(pw_node_id) {
                    self.show_plugin_params.insert(instance_id, true);
                }
            }

            if let Some(pw_node_id) = response.plugin_ui_clicked {
                if let Some(instance_id) = self.find_instance_by_pw_node(pw_node_id) {
                    self.open_plugin_ui(instance_id);
                }
            }

            if let Some(pw_node_id) = response.plugin_remove_clicked {
                if let Some(instance_id) = self.find_instance_by_pw_node(pw_node_id) {
                    let _ = self.cmd_tx.send(PwCommand::RemovePlugin { instance_id });
                }
            }

            if let Some(pw_node_id) = response.plugin_rename_clicked {
                if let Some(instance_id) = self.find_instance_by_pw_node(pw_node_id) {
                    if let Some(info) = self.lv2_manager.get_instance(instance_id) {
                        self.rename_dialog =
                            Some((instance_id, pw_node_id, info.display_name.clone()));
                    }
                }
            }
        });

        // Render rename dialog (outside the CentralPanel closure)
        self.render_rename_dialog(ctx);
    }
}
