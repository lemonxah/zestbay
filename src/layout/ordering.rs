// SPDX-License-Identifier: MIT
//
// Crossing minimization phase: reorder nodes within columns to minimize
// edge crossings using the barycenter heuristic with iterative sweeps.
//
// Adapted from the node-arrange Blender addon, which implements:
// - Jünger & Mutzel 2003, Forster 2004, Chimani et al. 2010

use super::graph::LayoutGraph;

/// Minimize edge crossings by reordering nodes within each column.
/// Uses the barycenter heuristic with alternating forward/backward sweeps.
pub fn minimize_crossings(graph: &mut LayoutGraph) {
    let iterations = graph.config.iterations;
    let num_cols = graph.columns.len();
    if num_cols < 2 {
        return;
    }

    let mut best_crossings = count_all_crossings(graph);
    let mut best_columns = graph.columns.clone();

    for _iter in 0..iterations {
        let mut improved_this_round = false;

        // Forward sweep: fix column i, reorder column i+1
        for col_idx in 0..(num_cols - 1) {
            let old_cross = count_crossings_between(graph, col_idx, col_idx + 1);
            reorder_by_barycenter(graph, col_idx, col_idx + 1, true);
            let new_cross = count_crossings_between(graph, col_idx, col_idx + 1);
            if new_cross > old_cross {
                // Revert
                graph.columns[col_idx + 1] = best_columns[col_idx + 1].clone();
                update_orders(graph, col_idx + 1);
            }
        }

        // Backward sweep: fix column i+1, reorder column i
        for col_idx in (1..num_cols).rev() {
            let old_cross = count_crossings_between(graph, col_idx - 1, col_idx);
            reorder_by_barycenter(graph, col_idx, col_idx - 1, false);
            let new_cross = count_crossings_between(graph, col_idx - 1, col_idx);
            if new_cross > old_cross {
                // Revert
                graph.columns[col_idx - 1] = best_columns[col_idx - 1].clone();
                update_orders(graph, col_idx - 1);
            }
        }

        let total_cross = count_all_crossings(graph);
        if total_cross < best_crossings {
            best_crossings = total_cross;
            best_columns = graph.columns.clone();
            improved_this_round = true;
        }

        if best_crossings == 0 || !improved_this_round {
            break;
        }
    }

    // Apply best result
    graph.columns = best_columns;
    for col_idx in 0..graph.columns.len() {
        update_orders(graph, col_idx);
    }
}

/// Reorder the free column based on barycenters computed from the fixed column.
fn reorder_by_barycenter(
    graph: &mut LayoutGraph,
    fixed_col_idx: usize,
    free_col_idx: usize,
    forward: bool,
) {
    let free_col = graph.columns[free_col_idx].clone();
    if free_col.is_empty() {
        return;
    }

    // Compute barycenter for each node in the free column
    let mut barycenters: Vec<(usize, f64)> = Vec::new();

    for &node_idx in &free_col {
        let neighbors: Vec<usize> = if forward {
            // Free column is to the right; its predecessors are in the fixed column
            graph.predecessors(node_idx)
        } else {
            // Free column is to the left; its successors are in the fixed column
            graph.successors(node_idx)
        };

        // Filter to neighbors actually in the fixed column
        let fixed_set: Vec<usize> = graph.columns[fixed_col_idx].clone();
        let positions: Vec<f64> = neighbors
            .iter()
            .filter(|n| fixed_set.contains(n))
            .map(|&n| graph.nodes[n].order as f64)
            .collect();

        let barycenter = if positions.is_empty() {
            // No connections to fixed column: keep current position
            graph.nodes[node_idx].order as f64
        } else {
            positions.iter().sum::<f64>() / positions.len() as f64
        };

        barycenters.push((node_idx, barycenter));
    }

    // Sort by barycenter (stable sort preserves order of equal barycenters)
    barycenters.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    // Update column
    let new_col: Vec<usize> = barycenters.iter().map(|(idx, _)| *idx).collect();
    graph.columns[free_col_idx] = new_col;
    update_orders(graph, free_col_idx);
}

/// Update the `order` field on all nodes in a column.
fn update_orders(graph: &mut LayoutGraph, col_idx: usize) {
    for (order, &node_idx) in graph.columns[col_idx].iter().enumerate() {
        graph.nodes[node_idx].order = order;
    }
}

/// Count total crossings across all adjacent column pairs.
fn count_all_crossings(graph: &LayoutGraph) -> usize {
    let mut total = 0;
    for col_idx in 0..(graph.columns.len().saturating_sub(1)) {
        total += count_crossings_between(graph, col_idx, col_idx + 1);
    }
    total
}

/// Count edge crossings between two adjacent columns using the accumulator tree method.
/// O(k log k) where k is the number of edges between the columns.
fn count_crossings_between(graph: &LayoutGraph, left_col: usize, right_col: usize) -> usize {
    // Collect all edges between the two columns as (left_order, right_order) pairs.
    let left_nodes: &[usize] = graph.column(left_col);
    let right_nodes: &[usize] = graph.column(right_col);
    if left_nodes.is_empty() || right_nodes.is_empty() {
        return 0;
    }

    // Build a set of right-column node indices for fast lookup
    let right_set: std::collections::HashSet<usize> = right_nodes.iter().copied().collect();

    let mut edge_pairs: Vec<(usize, usize)> = Vec::new();
    for &left_node in left_nodes {
        for &ei in &graph.out_edges[left_node] {
            let right_node = graph.edges[ei].to_node;
            if right_set.contains(&right_node) {
                edge_pairs.push((graph.nodes[left_node].order, graph.nodes[right_node].order));
            }
        }
    }

    if edge_pairs.is_empty() {
        return 0;
    }

    // Sort by left position, then by right position
    edge_pairs.sort();

    // Count inversions in the right positions using an accumulator tree (BIT/Fenwick tree)
    let max_right = right_nodes.len();
    count_inversions(&edge_pairs, max_right)
}

/// Count inversions using a Fenwick tree (accumulator tree).
/// This counts the number of edge crossings.
fn count_inversions(edges: &[(usize, usize)], max_pos: usize) -> usize {
    // edges is sorted by left position.
    // We need to count, for each edge, how many previously inserted edges
    // have a larger right position (= crossing).

    let tree_size = max_pos + 1;
    let mut tree = vec![0usize; tree_size + 1];
    let mut crossings = 0;

    for &(_left, right) in edges {
        // Count elements already inserted with position > right
        crossings += query(&tree, tree_size) - query(&tree, right + 1);
        // Insert this position
        update(&mut tree, right + 1, 1, tree_size);
    }

    crossings
}

/// Fenwick tree: prefix sum query [1..pos]
fn query(tree: &[usize], mut pos: usize) -> usize {
    let mut sum = 0;
    while pos > 0 {
        sum += tree[pos];
        pos -= pos & pos.wrapping_neg();
    }
    sum
}

/// Fenwick tree: update position
fn update(tree: &mut [usize], mut pos: usize, val: usize, max_pos: usize) {
    while pos <= max_pos {
        tree[pos] += val;
        pos += pos & pos.wrapping_neg();
    }
}
