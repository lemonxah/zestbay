// SPDX-License-Identifier: MIT
//
// Y-coordinate assignment using the Brandes-Köpf algorithm.
// Runs in 4 directions (left→right / right→left alignment, each compacted
// downward and upward) and combines them for a balanced layout.
//
// Reference: Brandes & Köpf, 2001 (doi:10.1007/3-540-45848-4_3), with the
// class-shift computation done as a separate longest-path pass over the
// class separation constraints (cf. Brandes, Walter & Zink, 2020 erratum).
//
// Expects every edge to span exactly one column (dummy nodes inserted).

use std::collections::HashSet;

use super::graph::{LayoutDirection, LayoutGraph};

/// Assign y-coordinates to all nodes using Brandes-Köpf.
pub fn assign_y_coords(graph: &mut LayoutGraph) {
    let n = graph.nodes.len();
    if n == 0 {
        return;
    }

    let marked = mark_type1_conflicts(graph);

    let single = |graph: &LayoutGraph, right_to_left: bool, up: bool| {
        compute_single_direction(graph, &marked, right_to_left, up)
    };

    let ys: Vec<f64> = match graph.config.direction {
        LayoutDirection::Balanced => {
            let mut layouts: Vec<Vec<f64>> = [(false, false), (false, true), (true, false), (true, true)]
                .iter()
                .map(|&(rl, up)| single(graph, rl, up))
                .collect();

            // Align all 4 layouts to the same origin before averaging.
            balance_layouts(&mut layouts);

            // Average of the two median candidates.  Every candidate layout is
            // overlap-free and separation is preserved by order statistics, so
            // the combined layout is overlap-free too.
            (0..n)
                .map(|i| {
                    let mut values: Vec<f64> = layouts.iter().map(|l| l[i]).collect();
                    values.sort_by(|a, b| a.total_cmp(b));
                    (values[1] + values[2]) / 2.0
                })
                .collect()
        }
        LayoutDirection::RightDown => single(graph, false, false),
        LayoutDirection::RightUp => single(graph, false, true),
        LayoutDirection::LeftDown => single(graph, true, false),
        LayoutDirection::LeftUp => single(graph, true, true),
    };

    for (node, y) in graph.nodes.iter_mut().zip(ys) {
        node.y = y;
    }
}

/// Mark type-1 conflicts: non-inner edges that cross an inner segment
/// (dummy→dummy edge).  Inner segments get alignment priority so long edges
/// stay straight.  Returns the set of marked (from_node, to_node) pairs.
///
/// Linear per layer pair: `l` continues from where the previous segment
/// stopped instead of rescanning from 0.
fn mark_type1_conflicts(graph: &LayoutGraph) -> HashSet<(usize, usize)> {
    let mut marked = HashSet::new();

    let inner_pred = |v: usize| -> Option<usize> {
        if !graph.nodes[v].is_dummy {
            return None;
        }
        graph.in_edges[v]
            .iter()
            .map(|&ei| graph.edges[ei].from_node)
            .find(|&u| graph.nodes[u].is_dummy)
    };

    for i in 0..graph.columns.len().saturating_sub(1) {
        let upper = &graph.columns[i];
        let lower = &graph.columns[i + 1];
        if upper.is_empty() || lower.is_empty() {
            continue;
        }

        let mut k0 = 0usize;
        let mut l = 0usize;
        for l1 in 0..lower.len() {
            let inner = inner_pred(lower[l1]);
            if l1 != lower.len() - 1 && inner.is_none() {
                continue;
            }
            let k1 = inner
                .map(|u| graph.nodes[u].order)
                .unwrap_or(upper.len() - 1);

            while l <= l1 {
                let v = lower[l];
                for &ei in &graph.in_edges[v] {
                    let u = graph.edges[ei].from_node;
                    let k = graph.nodes[u].order;
                    if k < k0 || k > k1 {
                        marked.insert((u, v));
                    }
                }
                l += 1;
            }
            k0 = k1;
        }
    }

    marked
}

/// Per-direction working state.  Columns are stored in "local" vertical order
/// (reversed for upward compaction) so the core algorithm is always top-down.
struct Bk<'a> {
    graph: &'a LayoutGraph,
    cols: Vec<Vec<usize>>,
    col_of: Vec<usize>,
    pos: Vec<usize>,
    up: bool,
    margin_y: f64,
    root: Vec<usize>,
    align: Vec<usize>,
    sink: Vec<usize>,
    y: Vec<Option<f64>>,
}

impl<'a> Bk<'a> {
    fn new(graph: &'a LayoutGraph, up: bool) -> Self {
        let n = graph.nodes.len();
        let mut cols = graph.columns.clone();
        if up {
            for col in &mut cols {
                col.reverse();
            }
        }
        let mut col_of = vec![0; n];
        let mut pos = vec![0; n];
        for (c, col) in cols.iter().enumerate() {
            for (p, &v) in col.iter().enumerate() {
                col_of[v] = c;
                pos[v] = p;
            }
        }
        Self {
            graph,
            cols,
            col_of,
            pos,
            up,
            margin_y: graph.config.margin_y,
            root: (0..n).collect(),
            align: (0..n).collect(),
            sink: (0..n).collect(),
            y: vec![None; n],
        }
    }

    /// Minimum distance between the tops of `a` and `b`, where `a` directly
    /// precedes `b` in local order.  Physically the upper node's height counts.
    fn delta(&self, a: usize, b: usize) -> f64 {
        let upper = if self.up { b } else { a };
        self.graph.nodes[upper].height + self.margin_y
    }

    /// Neighbours of `v` in the previously swept column, sorted by local position.
    fn sweep_neighbors(&self, v: usize, right_to_left: bool, prev_col: usize) -> Vec<usize> {
        let g = self.graph;
        let mut nbrs: Vec<usize> = if right_to_left {
            g.out_edges[v].iter().map(|&ei| g.edges[ei].to_node).collect()
        } else {
            g.in_edges[v].iter().map(|&ei| g.edges[ei].from_node).collect()
        };
        nbrs.retain(|&u| self.col_of[u] == prev_col);
        nbrs.sort_by_key(|&u| self.pos[u]);
        nbrs.dedup();
        nbrs
    }

    fn horizontal_alignment(&mut self, marked: &HashSet<(usize, usize)>, right_to_left: bool) {
        let num_cols = self.cols.len();
        let sweep: Vec<usize> = if right_to_left {
            (0..num_cols).rev().collect()
        } else {
            (0..num_cols).collect()
        };

        for w in sweep.windows(2) {
            let (prev_col, col) = (w[0], w[1]);
            let mut r: isize = -1;
            for idx in 0..self.cols[col].len() {
                let v = self.cols[col][idx];
                let nbrs = self.sweep_neighbors(v, right_to_left, prev_col);
                if nbrs.is_empty() {
                    continue;
                }
                let d = nbrs.len();
                let medians = if d % 2 == 1 {
                    vec![(d - 1) / 2]
                } else {
                    vec![d / 2 - 1, d / 2]
                };
                for m in medians {
                    if self.align[v] != v {
                        break;
                    }
                    let u = nbrs[m];
                    let is_marked = if right_to_left {
                        marked.contains(&(v, u))
                    } else {
                        marked.contains(&(u, v))
                    };
                    if !is_marked && r < self.pos[u] as isize {
                        self.align[u] = v;
                        self.root[v] = self.root[u];
                        self.align[v] = self.root[v];
                        r = self.pos[u] as isize;
                    }
                }
            }
        }
    }

    fn place_block(&mut self, v: usize) {
        if self.y[v].is_some() {
            return;
        }
        self.y[v] = Some(0.0);
        let mut w = v;
        loop {
            let p = self.pos[w];
            if p > 0 {
                let above = self.cols[self.col_of[w]][p - 1];
                let u = self.root[above];
                self.place_block(u);
                if self.sink[v] == v {
                    self.sink[v] = self.sink[u];
                }
                if self.sink[v] == self.sink[u] {
                    let cand = self.y[u].unwrap_or(0.0) + self.delta(above, w);
                    let cur = self.y[v].unwrap_or(0.0);
                    self.y[v] = Some(cur.max(cand));
                }
            }
            w = self.align[w];
            if w == v {
                break;
            }
        }
    }

    /// Place every block relative to its class, then separate the classes.
    /// Returns top-down local y per node (all finite).
    fn vertical_compaction(&mut self) -> Vec<f64> {
        let n = self.graph.nodes.len();
        for v in 0..n {
            if self.root[v] == v {
                self.place_block(v);
            }
        }

        let rel = |s: &Self, v: usize| s.y[s.root[v]].unwrap_or(0.0);
        let class = |s: &Self, v: usize| s.sink[s.root[v]];

        // Class separation constraints: shift[cb] >= shift[ca] + weight.
        let mut out: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
        let mut indeg = vec![0usize; n];
        for col in &self.cols {
            for win in col.windows(2) {
                let (a, b) = (win[0], win[1]);
                let (ca, cb) = (class(self, a), class(self, b));
                if ca != cb {
                    let weight = rel(self, a) + self.delta(a, b) - rel(self, b);
                    out[ca].push((cb, weight));
                    indeg[cb] += 1;
                }
            }
        }

        // Longest path over the class DAG (Kahn, deterministic FIFO order).
        let mut shift = vec![0.0f64; n];
        let mut queue: std::collections::VecDeque<usize> =
            (0..n).filter(|&c| self.sink[c] == c && indeg[c] == 0).collect();
        let mut done = vec![false; n];
        loop {
            while let Some(c) = queue.pop_front() {
                done[c] = true;
                for &(t, wgt) in &out[c] {
                    shift[t] = shift[t].max(shift[c] + wgt);
                    indeg[t] -= 1;
                    if indeg[t] == 0 {
                        queue.push_back(t);
                    }
                }
            }
            // Defensive: a cyclic class graph should not occur; if it does,
            // force the lowest pending class through (the sweep below keeps
            // the result overlap-free).
            match (0..n).find(|&c| self.sink[c] == c && !done[c]) {
                Some(c) => {
                    indeg[c] = 0;
                    queue.push_back(c);
                }
                None => break,
            }
        }

        let mut ys: Vec<f64> = (0..n).map(|v| rel(self, v) + shift[class(self, v)]).collect();

        // Safety net: guarantee separation within every column.
        for col in &self.cols {
            for win in col.windows(2) {
                let (a, b) = (win[0], win[1]);
                let min_b = ys[a] + self.delta(a, b);
                if ys[b] < min_b {
                    ys[b] = min_b;
                }
            }
        }
        ys
    }
}

/// Compute y-coordinates (top-left) for a single direction.
/// Returns a vec of y values indexed by node index.
fn compute_single_direction(
    graph: &LayoutGraph,
    marked: &HashSet<(usize, usize)>,
    right_to_left: bool,
    up: bool,
) -> Vec<f64> {
    let mut bk = Bk::new(graph, up);
    bk.horizontal_alignment(marked, right_to_left);
    let ys = bk.vertical_compaction();
    if up {
        ys.into_iter().map(|y| -y).collect()
    } else {
        ys
    }
}

/// Align all layouts so their minimum y is 0 before median averaging.
fn balance_layouts(layouts: &mut [Vec<f64>]) {
    for layout in layouts.iter_mut() {
        let min_val = layout.iter().copied().fold(f64::INFINITY, f64::min);
        if min_val.is_finite() {
            for y in layout.iter_mut() {
                *y -= min_val;
            }
        }
    }
}
