// SPDX-License-Identifier: MIT
//
// Graph data structures for the Sugiyama layered graph layout algorithm.
// Adapted from the node-arrange Blender addon.

use std::collections::{HashMap, HashSet};

/// Unique identifier for layout nodes (maps to PipeWire ObjectId or virtual IDs).
pub type NodeId = u32;
/// Unique identifier for ports.
pub type PortId = u32;
/// Unique identifier for edges/links.
pub type EdgeId = u32;

/// A node in the layout graph.
#[derive(Debug, Clone)]
pub struct LayoutNode {
    /// Original PipeWire node ID (or virtual bridge sub-node ID).
    pub id: NodeId,
    /// Display name.
    pub name: String,
    /// Node kind — used for determining default flow direction.
    pub kind: LayoutNodeKind,
    /// Width of the node in canvas pixels.
    pub width: f64,
    /// Height of the node in canvas pixels.
    pub height: f64,

    // --- Layout state (set by the algorithm) ---
    /// Column/layer rank (x-axis index). Set by ranking phase.
    pub rank: i32,
    /// Position within the column (ordering). Set by ordering phase.
    pub order: usize,
    /// Final x coordinate (top-left corner). Set by x_coords phase.
    pub x: f64,
    /// Final y coordinate (top-left corner). Set by y_coords phase.
    pub y: f64,

    /// Whether this is a dummy node inserted for long edges.
    pub is_dummy: bool,
    /// Index of this node in the graph's node vector.
    pub idx: usize,
}

/// The kind/type of a layout node, used for determining default column assignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayoutNodeKind {
    Source,
    Sink,
    StreamOutput,
    StreamInput,
    Duplex,
    Plugin,
    Dummy,
    Unknown,
}

/// A port (socket) on a layout node.
#[derive(Debug, Clone)]
pub struct LayoutPort {
    /// Original PipeWire port ID.
    pub id: PortId,
    /// Node this port belongs to (index into graph's node vec).
    pub node_idx: usize,
    /// Index of this port among its direction group on the node (0-based).
    pub port_index: usize,
    /// Whether this is an output (true) or input (false) port.
    pub is_output: bool,
}

impl LayoutPort {
    /// Get the x position of this port given the node's position and width.
    pub fn x(&self, node: &LayoutNode) -> f64 {
        if self.is_output {
            node.x + node.width
        } else {
            node.x
        }
    }

    /// Get the y position of this port given the node's y position and layout constants.
    pub fn y(
        &self,
        node: &LayoutNode,
        header_height: f64,
        padding: f64,
        port_height: f64,
        port_spacing: f64,
    ) -> f64 {
        let base_y = node.y + header_height + padding;
        base_y + self.port_index as f64 * (port_height + port_spacing) + port_height / 2.0
    }
}

/// A directed edge in the layout graph.
#[derive(Debug, Clone)]
pub struct LayoutEdge {
    /// Original PipeWire link ID (0 for dummy edges).
    pub id: EdgeId,
    /// Source node index.
    pub from_node: usize,
    /// Target node index.
    pub to_node: usize,
    /// Output port info (if available).
    pub from_port: Option<PortId>,
    /// Input port info (if available).
    pub to_port: Option<PortId>,
}

/// Configuration for the layout algorithm.
#[derive(Debug, Clone)]
pub struct LayoutConfig {
    /// Horizontal margin between columns.
    pub margin_x: f64,
    /// Vertical margin between nodes in a column.
    pub margin_y: f64,
    /// Number of crossing-minimization iterations (higher = fewer crossings, slower).
    pub iterations: usize,
    /// Layout direction for Brandes-Köpf. "balanced" averages 4 directions.
    pub direction: LayoutDirection,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            margin_x: 50.0,
            margin_y: 20.0,
            iterations: 25,
            direction: LayoutDirection::Balanced,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutDirection {
    RightDown,
    RightUp,
    LeftDown,
    LeftUp,
    Balanced,
}

/// The main graph structure for the layout algorithm.
#[derive(Debug)]
pub struct LayoutGraph {
    /// All nodes (real + dummy). Index = node's `idx` field.
    pub nodes: Vec<LayoutNode>,
    /// All edges (real + dummy).
    pub edges: Vec<LayoutEdge>,
    /// Columns: columns[rank] = list of node indices in that column, in order.
    pub columns: Vec<Vec<usize>>,
    /// Map from original PipeWire node ID to node index.
    pub id_to_idx: HashMap<NodeId, usize>,
    /// Adjacency list: out_edges[node_idx] = list of edge indices.
    pub out_edges: Vec<Vec<usize>>,
    /// Reverse adjacency: in_edges[node_idx] = list of edge indices.
    pub in_edges: Vec<Vec<usize>>,
    /// Port lookup: port_id -> (node_idx, port_index, is_output).
    pub port_map: HashMap<PortId, (usize, usize, bool)>,
    /// Configuration.
    pub config: LayoutConfig,
}

impl LayoutNode {
    pub fn new_real(
        id: NodeId,
        name: String,
        kind: LayoutNodeKind,
        width: f64,
        height: f64,
        idx: usize,
    ) -> Self {
        Self {
            id,
            name,
            kind,
            width,
            height,
            rank: 0,
            order: 0,
            x: 0.0,
            y: 0.0,
            is_dummy: false,
            idx,
        }
    }

    pub fn new_dummy(idx: usize, rank: i32) -> Self {
        Self {
            id: 0,
            name: String::new(),
            kind: LayoutNodeKind::Dummy,
            width: 0.0,
            height: 0.0,
            rank,
            order: 0,
            x: 0.0,
            y: 0.0,
            is_dummy: true,
            idx,
        }
    }
}

impl LayoutGraph {
    /// Create a new graph from PipeWire data.
    ///
    /// - `nodes`: (id, name, type_str, width, height)
    /// - `ports`: (port_id, node_id, port_index, is_output)
    /// - `links`: (link_id, output_node_id, output_port_id, input_node_id, input_port_id)
    pub fn new(
        nodes: Vec<(NodeId, String, &str, f64, f64)>,
        ports: Vec<(PortId, NodeId, usize, bool)>,
        links: Vec<(EdgeId, NodeId, PortId, NodeId, PortId)>,
        config: LayoutConfig,
    ) -> Self {
        let mut graph = Self {
            nodes: Vec::with_capacity(nodes.len()),
            edges: Vec::new(),
            columns: Vec::new(),
            id_to_idx: HashMap::new(),
            out_edges: Vec::new(),
            in_edges: Vec::new(),
            port_map: HashMap::new(),
            config,
        };

        // Add real nodes
        for (id, name, type_str, width, height) in nodes {
            let kind = match type_str {
                "Source" => LayoutNodeKind::Source,
                "Sink" => LayoutNodeKind::Sink,
                "StreamOutput" => LayoutNodeKind::StreamOutput,
                "StreamInput" => LayoutNodeKind::StreamInput,
                "Duplex" => LayoutNodeKind::Duplex,
                "Plugin" => LayoutNodeKind::Plugin,
                _ => LayoutNodeKind::Unknown,
            };
            let idx = graph.nodes.len();
            graph.id_to_idx.insert(id, idx);
            graph
                .nodes
                .push(LayoutNode::new_real(id, name, kind, width, height, idx));
            graph.out_edges.push(Vec::new());
            graph.in_edges.push(Vec::new());
        }

        // Register ports
        for (port_id, node_id, port_index, is_output) in ports {
            if let Some(&node_idx) = graph.id_to_idx.get(&node_id) {
                graph
                    .port_map
                    .insert(port_id, (node_idx, port_index, is_output));
            }
        }

        // Add edges (self-loops carry no layering information and are dropped)
        for (link_id, out_node_id, out_port_id, in_node_id, in_port_id) in links {
            if let (Some(&from_idx), Some(&to_idx)) = (
                graph.id_to_idx.get(&out_node_id),
                graph.id_to_idx.get(&in_node_id),
            ) {
                if from_idx == to_idx {
                    continue;
                }
                let edge_idx = graph.edges.len();
                graph.edges.push(LayoutEdge {
                    id: link_id,
                    from_node: from_idx,
                    to_node: to_idx,
                    from_port: Some(out_port_id),
                    to_port: Some(in_port_id),
                });
                graph.out_edges[from_idx].push(edge_idx);
                graph.in_edges[to_idx].push(edge_idx);
            }
        }

        graph
    }

    /// Add a dummy node and return its index.
    pub fn add_dummy_node(&mut self, rank: i32) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(LayoutNode::new_dummy(idx, rank));
        self.out_edges.push(Vec::new());
        self.in_edges.push(Vec::new());
        idx
    }

    /// Add a dummy edge between two nodes (used for long-span edge splitting).
    pub fn add_dummy_edge(&mut self, from_idx: usize, to_idx: usize) -> usize {
        let edge_idx = self.edges.len();
        self.edges.push(LayoutEdge {
            id: 0,
            from_node: from_idx,
            to_node: to_idx,
            from_port: None,
            to_port: None,
        });
        self.out_edges[from_idx].push(edge_idx);
        self.in_edges[to_idx].push(edge_idx);
        edge_idx
    }

    /// Get all successor node indices of a given node.
    pub fn successors(&self, node_idx: usize) -> Vec<usize> {
        self.out_edges[node_idx]
            .iter()
            .map(|&ei| self.edges[ei].to_node)
            .collect()
    }

    /// Get all predecessor node indices of a given node.
    pub fn predecessors(&self, node_idx: usize) -> Vec<usize> {
        self.in_edges[node_idx]
            .iter()
            .map(|&ei| self.edges[ei].from_node)
            .collect()
    }

    /// Build column lists from node ranks. Must be called after ranking.
    pub fn build_columns(&mut self) {
        if self.nodes.is_empty() {
            self.columns = Vec::new();
            return;
        }

        let min_rank = self.nodes.iter().map(|n| n.rank).min().unwrap_or(0);
        let max_rank = self.nodes.iter().map(|n| n.rank).max().unwrap_or(0);

        let num_cols = (max_rank - min_rank + 1) as usize;
        let mut columns: Vec<Vec<usize>> = vec![Vec::new(); num_cols];

        for (i, node) in self.nodes.iter().enumerate() {
            let col_idx = (node.rank - min_rank) as usize;
            columns[col_idx].push(i);
        }

        // Store column assignment on each node (order field = position in column)
        for col in &columns {
            for (order, &node_idx) in col.iter().enumerate() {
                self.nodes[node_idx].order = order;
            }
        }

        self.columns = columns;
    }

    /// Get all nodes in a given column (by rank offset).
    pub fn column(&self, col_idx: usize) -> &[usize] {
        if col_idx < self.columns.len() {
            &self.columns[col_idx]
        } else {
            &[]
        }
    }

    /// Find weakly connected components of the graph.
    pub fn connected_components(&self) -> Vec<Vec<usize>> {
        let n = self.nodes.len();
        let mut visited = vec![false; n];
        let mut components = Vec::new();

        for start in 0..n {
            if visited[start] {
                continue;
            }
            let mut component = Vec::new();
            let mut stack = vec![start];
            while let Some(v) = stack.pop() {
                if visited[v] {
                    continue;
                }
                visited[v] = true;
                component.push(v);
                // Follow both directions (weakly connected)
                for &ei in &self.out_edges[v] {
                    let w = self.edges[ei].to_node;
                    if !visited[w] {
                        stack.push(w);
                    }
                }
                for &ei in &self.in_edges[v] {
                    let w = self.edges[ei].from_node;
                    if !visited[w] {
                        stack.push(w);
                    }
                }
            }
            components.push(component);
        }
        components
    }


    /// Reverse edge `ei` in place (used to break cycles).
    fn reverse_edge(&mut self, ei: usize) {
        let (from, to) = (self.edges[ei].from_node, self.edges[ei].to_node);
        self.out_edges[from].retain(|&e| e != ei);
        self.in_edges[to].retain(|&e| e != ei);
        let edge = &mut self.edges[ei];
        std::mem::swap(&mut edge.from_node, &mut edge.to_node);
        std::mem::swap(&mut edge.from_port, &mut edge.to_port);
        self.out_edges[to].push(ei);
        self.in_edges[from].push(ei);
    }

    /// Make the graph acyclic by reversing the back edges of a deterministic
    /// DFS (started from source nodes first, then in index order).
    /// Returns the number of reversed edges.
    pub fn break_cycles(&mut self) -> usize {
        let n = self.nodes.len();
        // 0 = unvisited, 1 = on the DFS stack, 2 = finished
        let mut state = vec![0u8; n];
        let mut back_edges = Vec::new();
        let starts: Vec<usize> = (0..n)
            .filter(|&i| self.in_edges[i].is_empty())
            .chain(0..n)
            .collect();

        for s in starts {
            if state[s] != 0 {
                continue;
            }
            state[s] = 1;
            let mut stack: Vec<(usize, usize)> = vec![(s, 0)];
            while let Some(top) = stack.last_mut() {
                let v = top.0;
                if top.1 < self.out_edges[v].len() {
                    let ei = self.out_edges[v][top.1];
                    top.1 += 1;
                    let w = self.edges[ei].to_node;
                    match state[w] {
                        0 => {
                            state[w] = 1;
                            stack.push((w, 0));
                        }
                        1 => back_edges.push(ei),
                        _ => {}
                    }
                } else {
                    state[v] = 2;
                    stack.pop();
                }
            }
        }

        for &ei in &back_edges {
            self.reverse_edge(ei);
        }
        back_edges.len()
    }

    /// Topological generations: group nodes into layers where each layer's
    /// nodes have all predecessors in earlier layers.
    /// Returns layers from source to sink.  Expects an acyclic graph
    /// (see [`break_cycles`](Self::break_cycles)).
    pub fn topological_generations(&self) -> Vec<Vec<usize>> {
        let n = self.nodes.len();
        let mut in_degree: Vec<usize> = (0..n).map(|i| self.in_edges[i].len()).collect();

        let mut current: Vec<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
        let mut generations = Vec::new();
        let mut seen = 0;

        while !current.is_empty() {
            seen += current.len();
            let mut next = Vec::new();
            for &v in &current {
                for &ei in &self.out_edges[v] {
                    let w = self.edges[ei].to_node;
                    in_degree[w] -= 1;
                    if in_degree[w] == 0 {
                        next.push(w);
                    }
                }
            }
            generations.push(std::mem::replace(&mut current, next));
        }

        // Defensive: should not happen on a DAG.  Give each leftover node its
        // own trailing generation rather than piling them into one column.
        if seen < n {
            let placed: HashSet<usize> = generations.iter().flatten().copied().collect();
            for i in (0..n).filter(|i| !placed.contains(i)) {
                generations.push(vec![i]);
            }
        }

        generations
    }
}
