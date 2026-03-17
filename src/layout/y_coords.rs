// SPDX-License-Identifier: MIT
//
// Y-coordinate assignment using the Brandes-Köpf algorithm.
// Runs in 4 directions (right-down, right-up, left-down, left-up)
// and combines them for a balanced layout.
//
// Reference: Brandes & Köpf, 2001 (doi:10.1007/3-540-45848-4_3)

use super::graph::{LayoutDirection, LayoutGraph};

/// Assign y-coordinates to all nodes using Brandes-Köpf.
pub fn assign_y_coords(graph: &mut LayoutGraph) {
    let n = graph.nodes.len();
    if n == 0 {
        return;
    }

    match graph.config.direction {
        LayoutDirection::Balanced => {
            // Run all 4 directions and take the balanced average
            let mut layouts: Vec<Vec<f64>> = Vec::new();

            for &(reverse_cols, up) in &[
                (false, false), // right-down
                (false, true),  // right-up
                (true, false),  // left-down
                (true, true),   // left-up
            ] {
                let layout = compute_single_direction(graph, reverse_cols, up);
                layouts.push(layout);
            }

            // Balance: align all 4 layouts to the same bounding box
            balance_layouts(&mut layouts, graph);

            for i in 0..n {
                let mut values: Vec<f64> = layouts.iter().map(|l| l[i]).collect();
                values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                graph.nodes[i].y = (values[1] + values[2]) / 2.0;
            }
        }
        LayoutDirection::RightDown => {
            let layout = compute_single_direction(graph, false, false);
            for (i, &y) in layout.iter().enumerate() {
                graph.nodes[i].y = y;
            }
        }
        LayoutDirection::RightUp => {
            let layout = compute_single_direction(graph, false, true);
            for (i, &y) in layout.iter().enumerate() {
                graph.nodes[i].y = y;
            }
        }
        LayoutDirection::LeftDown => {
            let layout = compute_single_direction(graph, true, false);
            for (i, &y) in layout.iter().enumerate() {
                graph.nodes[i].y = y;
            }
        }
        LayoutDirection::LeftUp => {
            let layout = compute_single_direction(graph, true, true);
            for (i, &y) in layout.iter().enumerate() {
                graph.nodes[i].y = y;
            }
        }
    }
}

/// Compute y-coordinates for a single direction.
/// Returns a vec of y values indexed by node index.
fn compute_single_direction(graph: &mut LayoutGraph, reverse_columns: bool, up: bool) -> Vec<f64> {
    let margin_y = graph.config.margin_y;

    for node in &mut graph.nodes {
        node.bk_reset();
    }

    // For "up" direction: reverse the order within each column so the compaction
    // places nodes in the opposite vertical direction.
    if up {
        for col in &mut graph.columns {
            col.reverse();
        }
        for col_idx in 0..graph.columns.len() {
            for (order, &node_idx) in graph.columns[col_idx].iter().enumerate() {
                graph.nodes[node_idx].order = order;
            }
        }
    }

    let col_indices: Vec<usize> = if reverse_columns {
        (0..graph.columns.len()).rev().collect()
    } else {
        (0..graph.columns.len()).collect()
    };

    let marked_edges = mark_conflicts(graph, &col_indices, up);
    horizontal_alignment(graph, &col_indices, &marked_edges, up);
    vertical_compaction(graph, &col_indices, margin_y, up);

    let dir_sign = if up { -1.0 } else { 1.0 };
    let layout: Vec<f64> = graph.nodes.iter().map(|n| n.y * dir_sign).collect();

    if up {
        for col in &mut graph.columns {
            col.reverse();
        }
        for col_idx in 0..graph.columns.len() {
            for (order, &node_idx) in graph.columns[col_idx].iter().enumerate() {
                graph.nodes[node_idx].order = order;
            }
        }
    }

    for node in &mut graph.nodes {
        node.bk_reset();
    }

    layout
}

/// Mark conflicting edges: edges that cross "inner segments" (dummy-to-dummy edges).
/// Returns a set of (from_node, to_node) pairs that are marked as conflicts.
fn mark_conflicts(
    graph: &LayoutGraph,
    col_indices: &[usize],
    _up: bool,
) -> std::collections::HashSet<(usize, usize)> {
    let mut marked = std::collections::HashSet::new();

    // For each pair of adjacent columns, mark edges that cross inner segments
    for window in col_indices.windows(2) {
        let prev_col_idx = window[0];
        let curr_col_idx = window[1];

        let curr_col = &graph.columns[curr_col_idx];
        let prev_col = &graph.columns[prev_col_idx];

        if curr_col.is_empty() || prev_col.is_empty() {
            continue;
        }

        let mut k_0: usize = 0;

        for (l_1, &u) in curr_col.iter().enumerate() {
            // Check if u is connected to a dummy node in prev_col (inner segment)
            let is_inner = graph.nodes[u].is_dummy
                && graph
                    .predecessors(u)
                    .iter()
                    .any(|&p| graph.nodes[p].is_dummy);

            let k_1 = if is_inner {
                let pred = graph.predecessors(u);
                pred.iter()
                    .filter_map(|&p| {
                        if prev_col.contains(&p) {
                            Some(graph.nodes[p].order)
                        } else {
                            None
                        }
                    })
                    .next()
                    .unwrap_or(prev_col.len() - 1)
            } else if l_1 == curr_col.len() - 1 {
                prev_col.len() - 1
            } else {
                continue;
            };

            // Mark crossing edges
            for l in 0..=l_1.min(curr_col.len() - 1) {
                let v = curr_col[l];
                if graph.nodes[v].is_dummy
                    && graph
                        .predecessors(v)
                        .iter()
                        .any(|&p| graph.nodes[p].is_dummy)
                {
                    continue;
                }

                for &pred in &graph.predecessors(v) {
                    if prev_col.iter().any(|&p| p == pred) {
                        let k = graph.nodes[pred].order;
                        if k < k_0 || k > k_1 {
                            marked.insert((pred, v));
                        }
                    }
                }
            }

            k_0 = k_1;
        }
    }

    marked
}

/// Horizontal alignment: form blocks of nodes that will be vertically aligned.
/// Each block is a chain connected via `root` and `aligned` pointers.
fn horizontal_alignment(
    graph: &mut LayoutGraph,
    col_indices: &[usize],
    marked_edges: &std::collections::HashSet<(usize, usize)>,
    _up: bool,
) {
    for &col_idx in col_indices.iter().skip(1) {
        let col = graph.columns[col_idx].clone();
        let mut prev_aligned_pos: i32 = -1;

        for &v in &col {
            // Get predecessors in the previous column, sorted by their order
            let prev_col_idx = if col_idx > 0 { col_idx - 1 } else { continue };
            let prev_col = &graph.columns[prev_col_idx];

            let mut preds_in_prev: Vec<usize> = graph
                .predecessors(v)
                .into_iter()
                .filter(|p| prev_col.contains(p))
                .collect();
            preds_in_prev.sort_by_key(|&p| graph.nodes[p].order);

            if preds_in_prev.is_empty() {
                continue;
            }

            // Find the median predecessor(s)
            let m = (preds_in_prev.len() as f64 - 1.0) / 2.0;
            let lo = m.floor() as usize;
            let hi = m.ceil() as usize;

            for &u in &preds_in_prev[lo..=hi] {
                // Check constraints
                if graph.nodes[v].aligned != v {
                    continue;
                }
                if marked_edges.contains(&(u, v)) {
                    continue;
                }
                let u_pos = graph.nodes[u].order as i32;
                if prev_aligned_pos >= u_pos {
                    continue;
                }

                // Align v with u
                graph.nodes[u].aligned = v;
                graph.nodes[v].root = graph.nodes[u].root;
                graph.nodes[v].aligned = graph.nodes[v].root;
                prev_aligned_pos = u_pos;
            }
        }
    }
}

/// Vertical compaction: assign actual y positions to blocks.
fn vertical_compaction(graph: &mut LayoutGraph, _col_indices: &[usize], margin_y: f64, up: bool) {
    let n = graph.nodes.len();

    // Place blocks
    let mut placed = vec![false; n];
    for i in 0..n {
        if graph.nodes[i].root == i {
            place_block(graph, i, &mut placed, margin_y, up);
        }
    }

    // Compute class shifts
    for col in &graph.columns {
        for window in col.windows(2) {
            let v = window[0];
            let u = window[1];
            let v_sink = graph.nodes[v].sink;
            let u_sink = graph.nodes[u].sink;
            if v_sink != u_sink {
                let delta = if up {
                    graph.nodes[u].height + margin_y
                } else {
                    graph.nodes[v].height + margin_y
                };
                let s_c = graph.nodes[u].y + graph.nodes[u].inner_shift
                    - graph.nodes[v].y
                    - graph.nodes[v].inner_shift
                    - delta;
                let v_sink_shift = graph.nodes[v_sink].shift;
                let u_sink_shift = graph.nodes[u_sink].shift;
                graph.nodes[v_sink].shift = v_sink_shift.min(u_sink_shift + s_c);
            }
        }

        // Initialize shift for first sink in column
        if !col.is_empty() {
            let first_sink = graph.nodes[col[0]].sink;
            if graph.nodes[first_sink].shift == f64::INFINITY {
                graph.nodes[first_sink].shift = 0.0;
            }
        }
    }

    // Apply shifts
    for i in 0..n {
        let sink = graph.nodes[i].sink;
        let shift = graph.nodes[sink].shift;
        let inner = graph.nodes[i].inner_shift;
        if shift != f64::INFINITY {
            graph.nodes[i].y += shift + inner;
        } else {
            graph.nodes[i].y += inner;
        }
    }
}

/// Recursively place a block root, computing its y position.
fn place_block(graph: &mut LayoutGraph, root: usize, placed: &mut [bool], margin_y: f64, up: bool) {
    if placed[root] {
        return;
    }
    placed[root] = true;
    graph.nodes[root].y = 0.0;

    let mut first = true;

    // Iterate through the block
    let mut w = root;
    loop {
        // Find the column and position of w
        let col_idx = graph.nodes[w].rank as usize;
        if col_idx >= graph.columns.len() {
            break;
        }
        let col = &graph.columns[col_idx];
        let pos = graph.nodes[w].order;

        if pos > 0 {
            // Get the node above w in its column
            let neighbor = col[pos - 1];
            let neighbor_root = graph.nodes[neighbor].root;

            // Place the neighbor's block first
            place_block(graph, neighbor_root, placed, margin_y, up);

            // Set sink
            if graph.nodes[root].sink == root {
                graph.nodes[root].sink = graph.nodes[neighbor_root].sink;
            }

            // Compute position relative to neighbor
            if graph.nodes[root].sink == graph.nodes[neighbor_root].sink {
                let delta = if up {
                    graph.nodes[w].height + margin_y
                } else {
                    graph.nodes[neighbor].height + margin_y
                };
                let s_b = graph.nodes[neighbor_root].y + graph.nodes[neighbor].inner_shift
                    - graph.nodes[w].inner_shift
                    + delta;

                if first {
                    graph.nodes[root].y = s_b;
                    first = false;
                } else {
                    graph.nodes[root].y = graph.nodes[root].y.max(s_b);
                }
            }
        }

        // Propagate y to all nodes in this block
        graph.nodes[w].y = graph.nodes[root].y;
        graph.nodes[w].sink = graph.nodes[root].sink;

        // Move to next in alignment chain
        let next = graph.nodes[w].aligned;
        if next == root {
            break;
        }
        w = next;
    }
}

/// Align all 4 layouts so their bounding boxes match before trimmed-mean averaging.
/// Each layout is shifted so its minimum y is 0.
fn balance_layouts(layouts: &mut [Vec<f64>], graph: &LayoutGraph) {
    if layouts.is_empty() || graph.nodes.is_empty() {
        return;
    }

    for layout in layouts.iter_mut() {
        let min_val = layout.iter().copied().fold(f64::INFINITY, f64::min);
        for y in layout.iter_mut() {
            *y -= min_val;
        }
    }
}
