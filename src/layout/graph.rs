use std::collections::{HashMap, HashSet};

pub type NodeId = u32;
pub type PortId = u32;
pub type EdgeId = u32;

#[derive(Debug, Clone)]
pub struct LayoutNode {
    pub id: NodeId,
    #[allow(dead_code)]
    pub name: String,
    pub kind: LayoutNodeKind,
    pub width: f64,
    pub height: f64,

    pub rank: i32,
    pub order: usize,
    pub x: f64,
    pub y: f64,

    pub is_dummy: bool,
    pub idx: usize,
}

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

#[derive(Debug, Clone)]
pub struct LayoutEdge {
    #[allow(dead_code)]
    pub id: EdgeId,
    pub from_node: usize,
    pub to_node: usize,
    pub from_port: Option<PortId>,
    pub to_port: Option<PortId>,
}

#[derive(Debug, Clone)]
pub struct LayoutConfig {
    pub margin_x: f64,
    pub margin_y: f64,
    pub iterations: usize,
    pub direction: LayoutDirection,
    pub max_column_nodes: usize,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            margin_x: 50.0,
            margin_y: 20.0,
            iterations: 25,
            direction: LayoutDirection::Balanced,
            max_column_nodes: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum LayoutDirection {
    RightDown,
    RightUp,
    LeftDown,
    LeftUp,
    Balanced,
}

#[derive(Debug)]
pub struct LayoutGraph {
    pub nodes: Vec<LayoutNode>,
    pub edges: Vec<LayoutEdge>,
    pub columns: Vec<Vec<usize>>,
    pub id_to_idx: HashMap<NodeId, usize>,
    pub out_edges: Vec<Vec<usize>>,
    pub in_edges: Vec<Vec<usize>>,
    pub port_map: HashMap<PortId, (usize, usize, bool)>,
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

        for (port_id, node_id, port_index, is_output) in ports {
            if let Some(&node_idx) = graph.id_to_idx.get(&node_id) {
                graph
                    .port_map
                    .insert(port_id, (node_idx, port_index, is_output));
            }
        }

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

    pub fn add_dummy_node(&mut self, rank: i32) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(LayoutNode::new_dummy(idx, rank));
        self.out_edges.push(Vec::new());
        self.in_edges.push(Vec::new());
        idx
    }

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

    pub fn successors(&self, node_idx: usize) -> Vec<usize> {
        self.out_edges[node_idx]
            .iter()
            .map(|&ei| self.edges[ei].to_node)
            .collect()
    }

    pub fn predecessors(&self, node_idx: usize) -> Vec<usize> {
        self.in_edges[node_idx]
            .iter()
            .map(|&ei| self.edges[ei].from_node)
            .collect()
    }

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

        for col in &columns {
            for (order, &node_idx) in col.iter().enumerate() {
                self.nodes[node_idx].order = order;
            }
        }

        self.columns = columns;
    }

    pub fn column(&self, col_idx: usize) -> &[usize] {
        if col_idx < self.columns.len() {
            &self.columns[col_idx]
        } else {
            &[]
        }
    }

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

    pub fn break_cycles(&mut self) -> usize {
        let n = self.nodes.len();
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

        if seen < n {
            let placed: HashSet<usize> = generations.iter().flatten().copied().collect();
            for i in (0..n).filter(|i| !placed.contains(i)) {
                generations.push(vec![i]);
            }
        }

        generations
    }
}
