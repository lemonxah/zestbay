// SPDX-License-Identifier: MIT
//
// X-coordinate assignment: position columns horizontally with adaptive spacing.
//
// Columns are spaced based on the widest node in each column, plus a margin
// that adapts to edge density (more edges with large vertical spans = more space).
//
// Adapted from node-arrange (doi:10.7155/jgaa.00220, p.139)

use super::graph::LayoutGraph;

/// Assign x-coordinates to all nodes.
/// Must be called after ranking, ordering, and y-coordinate assignment.
pub fn assign_x_coords(graph: &mut LayoutGraph) {
    if graph.columns.is_empty() {
        return;
    }

    let margin_x = graph.config.margin_x;
    let num_cols = graph.columns.len();
    let mut x = 0.0;

    for col_idx in 0..num_cols {
        let col = &graph.columns[col_idx];
        if col.is_empty() {
            continue;
        }

        // Find the widest node in this column
        let max_width = col
            .iter()
            .map(|&ni| graph.nodes[ni].width)
            .fold(0.0_f64, f64::max);

        // Assign x to each node: center within the column's max width
        // Dummy nodes (reroutes) are left-aligned
        for &ni in col {
            if graph.nodes[ni].is_dummy {
                graph.nodes[ni].x = x;
            } else {
                // Center the node horizontally within the column
                graph.nodes[ni].x = x + (max_width - graph.nodes[ni].width) / 2.0;
            }
        }

        // Compute adaptive spacing for this column
        // Count edges leaving this column with large vertical span
        let delta_i = count_large_vertical_edges(graph, col_idx);
        let spacing = (1.0 + (delta_i as f64 / 4.0).min(2.0)) * margin_x;

        x += max_width + spacing;
    }
}

/// Count edges from this column to the next with a large vertical span
/// (|to_socket.y - from_socket.y| >= margin_x * 3).
fn count_large_vertical_edges(graph: &LayoutGraph, col_idx: usize) -> usize {
    let col = &graph.columns[col_idx];
    let margin_x = graph.config.margin_x;
    let threshold = margin_x * 3.0;

    let mut count = 0;
    for &ni in col {
        for &ei in &graph.out_edges[ni] {
            let edge = &graph.edges[ei];
            let from_y = graph.nodes[edge.from_node].y;
            let to_y = graph.nodes[edge.to_node].y;
            if (to_y - from_y).abs() >= threshold {
                count += 1;
            }
        }
    }
    count
}
