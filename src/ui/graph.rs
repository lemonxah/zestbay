//! Graph visualization widget
//!
//! Renders the PipeWire graph as a node-based visual editor.

use egui::{Color32, Id, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use std::collections::{HashMap, HashSet};

use crate::pipewire::{Link, Node, NodeType, ObjectId, Port, PortDirection};

// ─── Layout persistence ──────────────────────────────────────────────────────

fn layout_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("zestbay").join("layout.json"))
}

fn load_layout() -> HashMap<String, Pos2> {
    let path = match layout_path() {
        Some(p) => p,
        None => return HashMap::new(),
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return HashMap::new(),
    };
    let raw: HashMap<String, [f32; 2]> = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return HashMap::new(),
    };
    raw.into_iter()
        .map(|(k, [x, y])| (k, Pos2::new(x, y)))
        .collect()
}

fn hidden_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("zestbay").join("hidden.json"))
}

fn load_hidden() -> HashSet<String> {
    let path = match hidden_path() {
        Some(p) => p,
        None => return HashSet::new(),
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return HashSet::new(),
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn save_hidden(keys: &HashSet<String>) {
    let path = match hidden_path() {
        Some(p) => p,
        None => return,
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(keys) {
        let _ = std::fs::write(&path, json);
    }
}

fn save_layout(positions: &HashMap<String, Pos2>) {
    let path = match layout_path() {
        Some(p) => p,
        None => return,
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let raw: HashMap<&str, [f32; 2]> = positions
        .iter()
        .map(|(k, v)| (k.as_str(), [v.x, v.y]))
        .collect();
    if let Ok(json) = serde_json::to_string_pretty(&raw) {
        let _ = std::fs::write(&path, json);
    }
}

/// Build a layout key that distinguishes nodes by type + display name.
/// e.g. "Sink:Built-in Audio" vs "Source:Built-in Audio"
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

// ─── Colours ─────────────────────────────────────────────────────────────────

mod colors {
    use egui::Color32;

    pub const SINK: Color32 = Color32::from_rgb(70, 130, 180);
    pub const SOURCE: Color32 = Color32::from_rgb(60, 179, 113);
    pub const STREAM_OUTPUT: Color32 = Color32::from_rgb(255, 165, 0);
    pub const STREAM_INPUT: Color32 = Color32::from_rgb(186, 85, 211);
    pub const DUPLEX: Color32 = Color32::from_rgb(255, 215, 0);
    pub const LV2_PLUGIN: Color32 = Color32::from_rgb(0, 191, 255);
    pub const DEFAULT: Color32 = Color32::from_rgb(128, 128, 128);

    pub const PORT_INPUT: Color32 = Color32::from_rgb(100, 149, 237);
    pub const PORT_OUTPUT: Color32 = Color32::from_rgb(144, 238, 144);

    pub const LINK_ACTIVE: Color32 = Color32::from_rgb(50, 205, 50);
    pub const LINK_INACTIVE: Color32 = Color32::from_rgb(169, 169, 169);
    pub const LINK_CONNECTING: Color32 = Color32::from_rgb(255, 255, 0);

    pub const SELECTED_BORDER: Color32 = Color32::from_rgb(255, 255, 255);
    pub const NODE_BORDER: Color32 = Color32::from_rgb(60, 60, 60);
    pub const NODE_BG: Color32 = Color32::from_rgb(40, 40, 40);
}

// ─── Dimensions ──────────────────────────────────────────────────────────────

mod layout {
    pub const NODE_WIDTH: f32 = 260.0;
    pub const NODE_HEADER_HEIGHT: f32 = 24.0;
    pub const PORT_HEIGHT: f32 = 20.0;
    pub const PORT_RADIUS: f32 = 6.0;
    pub const PORT_SPACING: f32 = 4.0;
    pub const NODE_PADDING: f32 = 8.0;
    pub const BUTTON_ROW_HEIGHT: f32 = 20.0;
    /// Gap between stacked same-name nodes
    pub const STACK_GAP: f32 = 10.0;
}

// ─── GraphView ───────────────────────────────────────────────────────────────

pub struct GraphView {
    /// Node positions (id → canvas position)
    node_positions: HashMap<ObjectId, Pos2>,
    /// Saved layout positions keyed by node display_name
    layout_positions: HashMap<String, Pos2>,
    /// Set to true whenever layout_positions changes; triggers a file save
    layout_dirty: bool,
    /// Hidden nodes keyed by layout_key (type:name), persisted to disk
    hidden_keys: HashSet<String>,
    /// Set to true whenever hidden_keys changes; triggers a file save
    hidden_dirty: bool,
    /// Currently selected node
    selected_node: Option<ObjectId>,
    /// Node being dragged
    dragging_node: Option<ObjectId>,
    /// Offset from node origin to mouse grab point (in canvas coords)
    drag_offset: Vec2,
    /// Port drag-to-connect: source port ID and its direction.
    /// Managed via raw pointer state (not egui widget drag) so the cable
    /// follows the mouse even after leaving the originating port widget.
    connecting_from: Option<(ObjectId, PortDirection)>,
    /// Port screen positions from the previous frame (for hit-testing on press/release)
    prev_port_positions: HashMap<ObjectId, Pos2>,
    /// Port directions from the previous frame
    prev_port_directions: HashMap<ObjectId, PortDirection>,
    /// Current mouse position (screen space)
    mouse_pos: Pos2,
    /// Zoom level
    zoom: f32,
    /// Pan offset
    pan: Vec2,
}

impl Default for GraphView {
    fn default() -> Self {
        Self {
            node_positions: HashMap::new(),
            layout_positions: load_layout(),
            layout_dirty: false,
            hidden_keys: load_hidden(),
            hidden_dirty: false,
            selected_node: None,
            dragging_node: None,
            drag_offset: Vec2::ZERO,
            connecting_from: None,
            prev_port_positions: HashMap::new(),
            prev_port_directions: HashMap::new(),
            mouse_pos: Pos2::ZERO,
            zoom: 1.0,
            pan: Vec2::ZERO,
        }
    }
}

// ─── GraphResponse ────────────────────────────────────────────────────────────

pub struct GraphResponse {
    pub connect_request: Option<(ObjectId, ObjectId)>,
    pub disconnect_request: Option<ObjectId>,
    pub selected_node: Option<ObjectId>,
    pub plugin_params_clicked: Option<ObjectId>,
    pub plugin_ui_clicked: Option<ObjectId>,
    /// "Remove Plugin" selected from context menu (PW node id)
    pub plugin_remove_clicked: Option<ObjectId>,
    /// "Rename" selected from context menu (PW node id)
    pub plugin_rename_clicked: Option<ObjectId>,
}

// ─── impl GraphView ──────────────────────────────────────────────────────────

impl GraphView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_node_position(&mut self, node_id: ObjectId, pos: Pos2) {
        self.node_positions.insert(node_id, pos);
    }

    pub fn get_node_position(&self, node_id: ObjectId) -> Option<Pos2> {
        self.node_positions.get(&node_id).copied()
    }

    /// Auto-layout nodes that don't have positions yet.
    ///
    /// Nodes whose display_name matches a saved layout entry are placed at
    /// that saved position (stacked vertically for duplicates).
    /// Everything else is placed in three columns: Sources | Streams | Sinks.
    /// Within each column, nodes with the same name are stacked vertically.
    pub fn auto_layout(&mut self, nodes: &[Node], ports_by_node: &HashMap<ObjectId, Vec<Port>>) {
        // How many nodes of each layout key already have a position (used
        // to compute stacking offsets for nodes placed from saved layouts).
        let mut key_placed: HashMap<String, usize> = HashMap::new();
        for node in nodes {
            if self.node_positions.contains_key(&node.id) {
                *key_placed.entry(layout_key(node)).or_insert(0) += 1;
            }
        }

        // Nodes that need column-based placement
        let mut sinks: Vec<&Node> = Vec::new();
        let mut sources: Vec<&Node> = Vec::new();
        let mut streams: Vec<&Node> = Vec::new();

        for node in nodes {
            if self.node_positions.contains_key(&node.id) {
                continue;
            }

            let key = layout_key(node);
            let ports = ports_by_node
                .get(&node.id)
                .map(|p| p.as_slice())
                .unwrap_or(&[]);
            let is_lv2 = node.node_type == Some(NodeType::Lv2Plugin);
            let height = self.calculate_node_height(ports, is_lv2);

            // Use saved base position if available
            if let Some(&base) = self.layout_positions.get(&key) {
                let stack_n = key_placed.entry(key).or_insert(0);
                let offset_y = *stack_n as f32 * (height + layout::STACK_GAP);
                self.node_positions
                    .insert(node.id, Pos2::new(base.x, base.y + offset_y));
                *stack_n += 1;
                continue;
            }

            // No saved position – add to column list
            *key_placed.entry(key).or_insert(0) += 1;
            match node.node_type {
                Some(NodeType::Sink) => sinks.push(node),
                Some(NodeType::Source) => sources.push(node),
                Some(NodeType::StreamOutput | NodeType::StreamInput) => streams.push(node),
                Some(NodeType::Duplex) => sinks.push(node),
                Some(NodeType::Lv2Plugin) => streams.push(node),
                None => streams.push(node),
            }
        }

        // Sort each column by layout key so same-type+name nodes end up adjacent
        sources.sort_by(|a, b| layout_key(a).cmp(&layout_key(b)));
        streams.sort_by(|a, b| layout_key(a).cmp(&layout_key(b)));
        sinks.sort_by(|a, b| layout_key(a).cmp(&layout_key(b)));

        let col_spacing = 300.0_f32;
        let row_gap = 40.0_f32; // gap between *groups* of same-name nodes

        self.place_column(&sources, 50.0, row_gap, ports_by_node);
        self.place_column(&streams, 50.0 + col_spacing, row_gap, ports_by_node);
        self.place_column(&sinks, 50.0 + col_spacing * 2.0, row_gap, ports_by_node);
    }

    /// Place a list of nodes into a vertical column at `x`, grouping
    /// same-key nodes (type+name) as stacks separated by `row_gap`.
    fn place_column(
        &mut self,
        nodes: &[&Node],
        x: f32,
        row_gap: f32,
        ports_by_node: &HashMap<ObjectId, Vec<Port>>,
    ) {
        let mut y = 50.0_f32;
        let mut prev_key: Option<String> = None;
        let mut stack_bottom = 50.0_f32; // bottom of the current stack

        for node in nodes {
            let ports = ports_by_node
                .get(&node.id)
                .map(|p| p.as_slice())
                .unwrap_or(&[]);
            let is_lv2 = node.node_type == Some(NodeType::Lv2Plugin);
            let height = self.calculate_node_height(ports, is_lv2);
            let key = layout_key(node);

            if prev_key.as_deref() == Some(&key) {
                // Same type+name as previous → stack directly below
                self.node_positions
                    .insert(node.id, Pos2::new(x, stack_bottom));
                stack_bottom += height + layout::STACK_GAP;
            } else {
                // New group
                self.node_positions.insert(node.id, Pos2::new(x, y));
                stack_bottom = y + height + layout::STACK_GAP;
                y = stack_bottom + row_gap;
                prev_key = Some(key);
            }
        }
    }

    fn calculate_node_height(&self, ports: &[Port], is_lv2: bool) -> f32 {
        let inputs = ports
            .iter()
            .filter(|p| p.direction == PortDirection::Input)
            .count();
        let outputs = ports
            .iter()
            .filter(|p| p.direction == PortDirection::Output)
            .count();
        let rows = inputs.max(outputs).max(1);
        let base = layout::NODE_HEADER_HEIGHT
            + layout::NODE_PADDING * 2.0
            + rows as f32 * (layout::PORT_HEIGHT + layout::PORT_SPACING);
        if is_lv2 {
            base + layout::BUTTON_ROW_HEIGHT + layout::NODE_PADDING
        } else {
            base
        }
    }

    // ─── Main show() ─────────────────────────────────────────────────────────

    pub fn show(
        &mut self,
        ui: &mut Ui,
        nodes: &[Node],
        ports_by_node: &HashMap<ObjectId, Vec<Port>>,
        links: &[Link],
    ) -> GraphResponse {
        let mut response = GraphResponse {
            connect_request: None,
            disconnect_request: None,
            selected_node: self.selected_node,
            plugin_params_clicked: None,
            plugin_ui_clicked: None,
            plugin_remove_clicked: None,
            plugin_rename_clicked: None,
        };

        let available_size = ui.available_size();
        let (rect, canvas_response) =
            ui.allocate_exact_size(available_size, Sense::click_and_drag());

        // Use the raw pointer position from input state rather than the canvas
        // widget's hover_pos.  When the pointer is captured by a node widget
        // (because the click started inside the node rect), the canvas widget's
        // hover_pos returns None and the mouse position would freeze — causing
        // the drag-to-connect cable to not follow the cursor.
        self.mouse_pos = ui
            .input(|i| i.pointer.hover_pos())
            .unwrap_or(self.mouse_pos);

        // Zoom with scroll wheel, centered on the mouse position
        let scroll_delta = ui.input(|i| i.smooth_scroll_delta);
        if scroll_delta.y != 0.0 {
            let old_zoom = self.zoom;
            let new_zoom = (old_zoom + scroll_delta.y * 0.001).clamp(0.25, 2.0);

            // The canvas point under the mouse before zoom
            let mouse_canvas_x = (self.mouse_pos.x - rect.min.x - self.pan.x) / old_zoom;
            let mouse_canvas_y = (self.mouse_pos.y - rect.min.y - self.pan.y) / old_zoom;

            // Adjust pan so the same canvas point stays under the mouse
            self.pan.x += mouse_canvas_x * (old_zoom - new_zoom);
            self.pan.y += mouse_canvas_y * (old_zoom - new_zoom);
            self.zoom = new_zoom;
        }

        // Pan with middle-mouse drag — read pointer delta directly so it works
        // even when the pointer is over a node (which would otherwise consume
        // the drag event).
        let middle_down = ui.input(|i| i.pointer.button_down(egui::PointerButton::Middle));
        if middle_down {
            let delta = ui.input(|i| i.pointer.delta());
            if delta != Vec2::ZERO {
                self.pan += delta;
            }
        }

        let painter = ui.painter_at(rect);

        // Coordinate transforms
        let zoom = self.zoom;
        let pan = self.pan;
        let rect_min = rect.min;

        let transform = |pos: Pos2| -> Pos2 {
            Pos2::new(
                (pos.x * zoom) + pan.x + rect_min.x,
                (pos.y * zoom) + pan.y + rect_min.y,
            )
        };

        let inverse_transform = |pos: Pos2| -> Pos2 {
            Pos2::new(
                (pos.x - rect_min.x - pan.x) / zoom,
                (pos.y - rect_min.y - pan.y) / zoom,
            )
        };

        // Port screen-positions for link drawing and hit-testing (built this frame)
        let mut port_positions: HashMap<ObjectId, Pos2> = HashMap::new();
        // Port directions for connection compatibility checks (built this frame)
        let mut port_directions: HashMap<ObjectId, PortDirection> = HashMap::new();

        // ─── Drag-to-connect via raw pointer state ─────────────────────────
        //
        // Uses previous frame's port positions for hit-testing so we can
        // detect press/release BEFORE drawing nodes this frame. This ensures
        // connecting_from is set before node drag checks, preventing conflicts.
        //
        //   Primary press   → if over a port, start connecting
        //   Primary held    → cable drawn to mouse (handled in draw section)
        //   Primary release → if over a compatible port, connect; else cancel
        //   Escape / RMB    → cancel
        //
        let primary_pressed = ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary));
        let primary_released =
            ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary));

        // Cancel on Escape or right-click
        if self.connecting_from.is_some() {
            let cancel = ui.input(|i| {
                i.key_pressed(egui::Key::Escape)
                    || i.pointer.button_pressed(egui::PointerButton::Secondary)
            });
            if cancel {
                self.connecting_from = None;
            }
        }

        // Start: primary button just pressed → check if mouse is over a port
        if primary_pressed && self.connecting_from.is_none() {
            let hit_radius = layout::PORT_RADIUS * self.zoom * 3.0;
            let mut best: Option<(ObjectId, f32)> = None;
            for (&port_id, &port_pos) in &self.prev_port_positions {
                let dist = self.mouse_pos.distance(port_pos);
                if dist <= hit_radius {
                    if best.as_ref().is_none_or(|(_, d)| dist < *d) {
                        best = Some((port_id, dist));
                    }
                }
            }
            if let Some((port_id, _)) = best {
                if let Some(&dir) = self.prev_port_directions.get(&port_id) {
                    self.connecting_from = Some((port_id, dir));
                }
            }
        }

        // Finish: primary button just released while connecting
        if primary_released {
            if let Some((from_id, from_dir)) = self.connecting_from.take() {
                let hit_radius = layout::PORT_RADIUS * self.zoom * 3.0;
                let mut best: Option<(ObjectId, f32)> = None;
                for (&port_id, &port_pos) in &self.prev_port_positions {
                    if port_id == from_id {
                        continue;
                    }
                    // Only consider ports of the opposite direction
                    if let Some(&dir) = self.prev_port_directions.get(&port_id) {
                        if dir == from_dir {
                            continue;
                        }
                    }
                    let dist = self.mouse_pos.distance(port_pos);
                    if dist <= hit_radius {
                        if best.as_ref().is_none_or(|(_, d)| dist < *d) {
                            best = Some((port_id, dist));
                        }
                    }
                }

                if let Some((target_id, _)) = best {
                    match from_dir {
                        PortDirection::Output => {
                            response.connect_request = Some((from_id, target_id));
                        }
                        PortDirection::Input => {
                            response.connect_request = Some((target_id, from_id));
                        }
                    }
                }
                // If no compatible target found, cable is simply dropped (cancelled)
            }
        }

        // Whether the pointer is currently over any node (for canvas context menu)
        let mut pointer_over_node = false;

        // ─── Draw nodes ──────────────────────────────────────────────────────
        for node in nodes {
            if !node.ready {
                continue;
            }
            if self.hidden_keys.contains(&layout_key(node)) {
                continue;
            }

            let pos = self
                .node_positions
                .get(&node.id)
                .copied()
                .unwrap_or(Pos2::new(100.0, 100.0));

            let ports = ports_by_node
                .get(&node.id)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let is_lv2 = node.node_type == Some(NodeType::Lv2Plugin);

            // Split ports by direction for independent per-side indexing
            let input_ports: Vec<&Port> = ports
                .iter()
                .filter(|p| p.direction == PortDirection::Input)
                .collect();
            let output_ports: Vec<&Port> = ports
                .iter()
                .filter(|p| p.direction == PortDirection::Output)
                .collect();

            // Height is driven by the taller side
            let port_rows = input_ports.len().max(output_ports.len()).max(1);
            let node_height = layout::NODE_HEADER_HEIGHT
                + layout::NODE_PADDING * 2.0
                + port_rows as f32 * (layout::PORT_HEIGHT + layout::PORT_SPACING)
                + if is_lv2 {
                    layout::BUTTON_ROW_HEIGHT + layout::NODE_PADDING
                } else {
                    0.0
                };

            let node_rect = Rect::from_min_size(
                transform(pos),
                Vec2::new(layout::NODE_WIDTH * zoom, node_height * zoom),
            );

            // ── Node interaction ─────────────────────────────────────────────
            let node_id_hash = Id::new(("node", node.id));
            let node_response = ui.interact(node_rect, node_id_hash, Sense::click_and_drag());

            if node_response.hovered() || node_response.contains_pointer() {
                pointer_over_node = true;
            }

            if node_response.clicked() {
                self.selected_node = Some(node.id);
                response.selected_node = Some(node.id);
            }

            if node_response.drag_started_by(egui::PointerButton::Primary)
                && self.connecting_from.is_none()
            {
                self.dragging_node = Some(node.id);
                let pointer = node_response
                    .interact_pointer_pos()
                    .unwrap_or(self.mouse_pos);
                let mouse_canvas = inverse_transform(pointer);
                self.drag_offset = Vec2::new(mouse_canvas.x - pos.x, mouse_canvas.y - pos.y);
            }

            if node_response.dragged_by(egui::PointerButton::Primary)
                && self.dragging_node == Some(node.id)
                && self.connecting_from.is_none()
            {
                let pointer = node_response
                    .interact_pointer_pos()
                    .unwrap_or(self.mouse_pos);
                let mouse_canvas = inverse_transform(pointer);
                let new_pos = Pos2::new(
                    mouse_canvas.x - self.drag_offset.x,
                    mouse_canvas.y - self.drag_offset.y,
                );
                self.node_positions.insert(node.id, new_pos);
            }

            if node_response.drag_stopped() {
                self.dragging_node = None;
                // Persist the new position keyed by type:display_name
                if let Some(&new_pos) = self.node_positions.get(&node.id) {
                    self.layout_positions.insert(layout_key(node), new_pos);
                    self.layout_dirty = true;
                }
            }

            // ── Node context menu ────────────────────────────────────────────
            {
                let node_id = node.id;
                let is_lv2_node = is_lv2;
                let node_key = layout_key(node);
                node_response.context_menu(|ui| {
                    if ui.button("Hide").clicked() {
                        self.hidden_keys.insert(node_key.clone());
                        self.hidden_dirty = true;
                        ui.close_menu();
                    }
                    if is_lv2_node {
                        ui.separator();
                        if ui.button("Rename...").clicked() {
                            response.plugin_rename_clicked = Some(node_id);
                            ui.close_menu();
                        }
                        if ui.button("Parameters...").clicked() {
                            response.plugin_params_clicked = Some(node_id);
                            ui.close_menu();
                        }
                        if ui.button("Open UI...").clicked() {
                            response.plugin_ui_clicked = Some(node_id);
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui
                            .button(egui::RichText::new("Remove Plugin").color(Color32::RED))
                            .clicked()
                        {
                            response.plugin_remove_clicked = Some(node_id);
                            ui.close_menu();
                        }
                    }
                });
            }

            // ── Draw node visuals ─────────────────────────────────────────────
            let node_color = match node.node_type {
                Some(NodeType::Sink) => colors::SINK,
                Some(NodeType::Source) => colors::SOURCE,
                Some(NodeType::StreamOutput) => colors::STREAM_OUTPUT,
                Some(NodeType::StreamInput) => colors::STREAM_INPUT,
                Some(NodeType::Duplex) => colors::DUPLEX,
                Some(NodeType::Lv2Plugin) => colors::LV2_PLUGIN,
                None => colors::DEFAULT,
            };

            painter.rect_filled(node_rect, 4.0 * zoom, colors::NODE_BG);

            let header_rect = Rect::from_min_size(
                node_rect.min,
                Vec2::new(node_rect.width(), layout::NODE_HEADER_HEIGHT * zoom),
            );
            painter.rect_filled(header_rect, 4.0 * zoom, node_color);

            let border_color = if self.selected_node == Some(node.id) {
                colors::SELECTED_BORDER
            } else {
                colors::NODE_BORDER
            };
            painter.rect_stroke(
                node_rect,
                4.0 * zoom,
                Stroke::new(2.0, border_color),
                egui::StrokeKind::Outside,
            );

            painter.text(
                header_rect.center(),
                egui::Align2::CENTER_CENTER,
                node.display_name(),
                egui::FontId::proportional(12.0 * zoom),
                Color32::WHITE,
            );

            // ── Draw ports ────────────────────────────────────────────────────
            let port_base_y =
                node_rect.min.y + layout::NODE_HEADER_HEIGHT * zoom + layout::NODE_PADDING * zoom;

            // Inputs on the left
            for (idx, port) in input_ports.iter().enumerate() {
                let port_y = port_base_y
                    + idx as f32 * (layout::PORT_HEIGHT + layout::PORT_SPACING) * zoom
                    + layout::PORT_HEIGHT * zoom / 2.0;
                let port_center = Pos2::new(node_rect.min.x, port_y);
                port_positions.insert(port.id, port_center);
                port_directions.insert(port.id, PortDirection::Input);

                let port_radius = layout::PORT_RADIUS * zoom;
                painter.circle_filled(port_center, port_radius, colors::PORT_INPUT);

                // Port hover highlight
                let port_hit_rect =
                    Rect::from_center_size(port_center, Vec2::splat(port_radius * 3.0));
                if port_hit_rect.contains(self.mouse_pos) {
                    if let Some((_, PortDirection::Output)) = self.connecting_from {
                        // Yellow ring — compatible drop target during drag
                        painter.circle_stroke(
                            port_center,
                            port_radius + 2.0,
                            Stroke::new(2.0, colors::LINK_CONNECTING),
                        );
                    } else if self.connecting_from.is_none() {
                        // Subtle white ring — hover feedback when idle
                        painter.circle_stroke(
                            port_center,
                            port_radius + 1.5,
                            Stroke::new(1.5, Color32::from_white_alpha(120)),
                        );
                    }
                }

                painter.text(
                    Pos2::new(port_center.x + port_radius + 4.0, port_center.y),
                    egui::Align2::LEFT_CENTER,
                    port.display_name(),
                    egui::FontId::proportional(10.0 * zoom),
                    Color32::LIGHT_GRAY,
                );
            }

            // Outputs on the right
            for (idx, port) in output_ports.iter().enumerate() {
                let port_y = port_base_y
                    + idx as f32 * (layout::PORT_HEIGHT + layout::PORT_SPACING) * zoom
                    + layout::PORT_HEIGHT * zoom / 2.0;
                let port_center = Pos2::new(node_rect.max.x, port_y);
                port_positions.insert(port.id, port_center);
                port_directions.insert(port.id, PortDirection::Output);

                let port_radius = layout::PORT_RADIUS * zoom;
                painter.circle_filled(port_center, port_radius, colors::PORT_OUTPUT);

                // Port hover highlight
                let port_hit_rect =
                    Rect::from_center_size(port_center, Vec2::splat(port_radius * 3.0));
                if port_hit_rect.contains(self.mouse_pos) {
                    if let Some((_, PortDirection::Input)) = self.connecting_from {
                        // Yellow ring — compatible drop target during drag
                        painter.circle_stroke(
                            port_center,
                            port_radius + 2.0,
                            Stroke::new(2.0, colors::LINK_CONNECTING),
                        );
                    } else if self.connecting_from.is_none() {
                        // Subtle white ring — hover feedback when idle
                        painter.circle_stroke(
                            port_center,
                            port_radius + 1.5,
                            Stroke::new(1.5, Color32::from_white_alpha(120)),
                        );
                    }
                }

                painter.text(
                    Pos2::new(port_center.x - port_radius - 4.0, port_center.y),
                    egui::Align2::RIGHT_CENTER,
                    port.display_name(),
                    egui::FontId::proportional(10.0 * zoom),
                    Color32::LIGHT_GRAY,
                );
            }

            // ── LV2 buttons ────────────────────────────────────────────────────
            if is_lv2 {
                let button_y = node_rect.max.y
                    - (layout::BUTTON_ROW_HEIGHT + layout::NODE_PADDING) * zoom
                    + layout::BUTTON_ROW_HEIGHT * zoom / 2.0;
                let button_w = (layout::NODE_WIDTH / 2.0 - layout::NODE_PADDING * 1.5) * zoom;
                let button_h = layout::BUTTON_ROW_HEIGHT * zoom;

                // "UI" button (left half)
                let ui_btn_rect = Rect::from_min_size(
                    Pos2::new(
                        node_rect.min.x + layout::NODE_PADDING * zoom,
                        button_y - button_h / 2.0,
                    ),
                    Vec2::new(button_w, button_h),
                );
                let ui_btn_response = ui.interact(
                    ui_btn_rect,
                    Id::new(("lv2_ui_btn", node.id)),
                    Sense::click(),
                );
                let ui_btn_color = if ui_btn_response.hovered() {
                    Color32::from_rgb(80, 80, 80)
                } else {
                    Color32::from_rgb(55, 55, 55)
                };
                painter.rect_filled(ui_btn_rect, 3.0 * zoom, ui_btn_color);
                painter.rect_stroke(
                    ui_btn_rect,
                    3.0 * zoom,
                    Stroke::new(1.0, Color32::from_rgb(90, 90, 90)),
                    egui::StrokeKind::Outside,
                );
                painter.text(
                    ui_btn_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "UI",
                    egui::FontId::proportional(10.0 * zoom),
                    Color32::WHITE,
                );
                if ui_btn_response.clicked() {
                    response.plugin_ui_clicked = Some(node.id);
                }

                // "Params" button (right half)
                let params_btn_rect = Rect::from_min_size(
                    Pos2::new(
                        node_rect.min.x
                            + (layout::NODE_WIDTH / 2.0 + layout::NODE_PADDING * 0.5) * zoom,
                        button_y - button_h / 2.0,
                    ),
                    Vec2::new(button_w, button_h),
                );
                let params_btn_response = ui.interact(
                    params_btn_rect,
                    Id::new(("lv2_params_btn", node.id)),
                    Sense::click(),
                );
                let params_btn_color = if params_btn_response.hovered() {
                    Color32::from_rgb(80, 80, 80)
                } else {
                    Color32::from_rgb(55, 55, 55)
                };
                painter.rect_filled(params_btn_rect, 3.0 * zoom, params_btn_color);
                painter.rect_stroke(
                    params_btn_rect,
                    3.0 * zoom,
                    Stroke::new(1.0, Color32::from_rgb(90, 90, 90)),
                    egui::StrokeKind::Outside,
                );
                painter.text(
                    params_btn_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Params",
                    egui::FontId::proportional(10.0 * zoom),
                    Color32::WHITE,
                );
                if params_btn_response.clicked() {
                    response.plugin_params_clicked = Some(node.id);
                }
            }
        } // end node loop

        // ─── Canvas context menu (right-click on empty space) ─────────────────
        if !pointer_over_node {
            canvas_response.context_menu(|ui| {
                // Unhide section
                if !self.hidden_keys.is_empty() {
                    if ui.button("Unhide All").clicked() {
                        self.hidden_keys.clear();
                        self.hidden_dirty = true;
                        ui.close_menu();
                    }
                    ui.separator();
                }

                if ui.button("Reset Zoom").clicked() {
                    self.zoom = 1.0;
                    self.pan = Vec2::ZERO;
                    ui.close_menu();
                }

                if ui.button("Auto Layout").clicked() {
                    self.node_positions.clear();
                    ui.close_menu();
                }
            });
        }

        // ─── Draw links ───────────────────────────────────────────────────────
        for link in links {
            if let (Some(&from_pos), Some(&to_pos)) = (
                port_positions.get(&link.output_port_id),
                port_positions.get(&link.input_port_id),
            ) {
                let color = if link.active {
                    colors::LINK_ACTIVE
                } else {
                    colors::LINK_INACTIVE
                };
                draw_bezier_link(&painter, from_pos, to_pos, color, 2.0);

                let mid = Pos2::new((from_pos.x + to_pos.x) / 2.0, (from_pos.y + to_pos.y) / 2.0);
                let link_rect = Rect::from_center_size(mid, Vec2::new(20.0, 20.0));
                let link_response =
                    ui.interact(link_rect, Id::new(("link", link.id)), Sense::click());

                if link_response.clicked() {
                    response.disconnect_request = Some(link.id);
                }
                if link_response.hovered() {
                    painter.circle_filled(mid, 5.0, Color32::RED);
                }
            }
        }

        // ─── Draw in-progress connection line ────────────────────────────────
        if let Some((from_port_id, from_dir)) = self.connecting_from {
            // Try current frame positions first, fall back to previous frame
            let from_pos = port_positions
                .get(&from_port_id)
                .or_else(|| self.prev_port_positions.get(&from_port_id));
            if let Some(&from_pos) = from_pos {
                // Draw the cable from the source port to the mouse cursor.
                // If dragging from an input port, reverse the curve direction
                // so the bezier bows correctly (control points go left from input).
                let (start, end) = match from_dir {
                    PortDirection::Output => (from_pos, self.mouse_pos),
                    PortDirection::Input => (self.mouse_pos, from_pos),
                };
                draw_bezier_link(&painter, start, end, colors::LINK_CONNECTING, 2.0);
            }
        }

        // ─── Update previous-frame port data for next frame's hit-testing ────
        self.prev_port_positions = port_positions;
        self.prev_port_directions = port_directions;

        // ─── Save layout if dirty ────────────────────────────────────────────
        if self.layout_dirty {
            save_layout(&self.layout_positions);
            self.layout_dirty = false;
        }

        // ─── Save hidden nodes if dirty ───────────────────────────────────────
        if self.hidden_dirty {
            save_hidden(&self.hidden_keys);
            self.hidden_dirty = false;
        }

        response
    }
}

// ─── Bezier link helper ───────────────────────────────────────────────────────

fn draw_bezier_link(painter: &egui::Painter, from: Pos2, to: Pos2, color: Color32, width: f32) {
    let ctrl_dist = ((to.x - from.x).abs() / 2.0).max(50.0);
    let ctrl1 = Pos2::new(from.x + ctrl_dist, from.y);
    let ctrl2 = Pos2::new(to.x - ctrl_dist, to.y);

    let segments = 64;
    let points: Vec<Pos2> = (0..=segments)
        .map(|i| {
            let t = i as f32 / segments as f32;
            let t2 = t * t;
            let t3 = t2 * t;
            let mt = 1.0 - t;
            let mt2 = mt * mt;
            let mt3 = mt2 * mt;
            Pos2::new(
                mt3 * from.x + 3.0 * mt2 * t * ctrl1.x + 3.0 * mt * t2 * ctrl2.x + t3 * to.x,
                mt3 * from.y + 3.0 * mt2 * t * ctrl1.y + 3.0 * mt * t2 * ctrl2.y + t3 * to.y,
            )
        })
        .collect();

    painter.add(egui::Shape::line(points, Stroke::new(width, color)));
}
