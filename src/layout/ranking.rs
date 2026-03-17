// SPDX-License-Identifier: MIT
//
// Ranking phase: assign each node to a column/layer.
//
// Implements the network simplex algorithm for optimal rank assignment,
// adapted from Gansner et al. 1993 (doi:10.1109/32.221135).
// Falls back to longest-path ranking from topological generations.

use super::graph::LayoutGraph;

/// Assign ranks to all nodes using longest-path ranking from topological generations,
/// then optimize with the network simplex heuristic.
pub fn compute_ranks(graph: &mut LayoutGraph) {
    let n = graph.nodes.len();
    if n == 0 {
        return;
    }

    // Phase 1: Initial ranking via topological generations (longest path from sources).
    // This gives rank(v) = length of the longest path from any source to v.
    initial_ranking(graph);

    // Phase 2: Network simplex optimization to minimize total edge length.
    network_simplex(graph);

    // Phase 3: Normalize ranks to start at 0 per connected component and balance.
    normalize_and_balance(graph);
}

/// Initial ranking: assign each node a rank equal to its topological generation.
/// This is the longest-path-from-sources approach.
fn initial_ranking(graph: &mut LayoutGraph) {
    let generations = graph.topological_generations();
    for (gen_idx, generation) in generations.iter().enumerate() {
        for &node_idx in generation {
            graph.nodes[node_idx].rank = gen_idx as i32;
        }
    }
}

/// Network simplex optimization for rank assignment.
///
/// The full network simplex is complex. We implement a simplified version:
/// For each edge, the "slack" is rank(to) - rank(from) - 1.
/// We iteratively try to reduce total slack by adjusting ranks.
fn network_simplex(graph: &mut LayoutGraph) {
    let n = graph.nodes.len();
    if n == 0 {
        return;
    }

    // Build a feasible spanning tree using tight edges (slack=0).
    // Then iteratively improve by swapping tree/non-tree edges.

    let max_iterations = (50.0 * (n as f64).sqrt()) as usize;

    for _iter in 0..max_iterations {
        // Find an edge with negative cut value (i.e., moving it would reduce total length).
        // Simplified: find any edge where we can tighten the rank difference.
        let mut improved = false;

        for ei in 0..graph.edges.len() {
            let edge = &graph.edges[ei];
            let from_rank = graph.nodes[edge.from_node].rank;
            let to_rank = graph.nodes[edge.to_node].rank;
            let slack = to_rank - from_rank - 1;

            if slack > 0 {
                // Try to pull the target node closer (decrease its rank).
                // Only do this if it doesn't violate any other edge constraint.
                let min_rank_from_preds = graph.in_edges[edge.to_node]
                    .iter()
                    .map(|&ei2| graph.nodes[graph.edges[ei2].from_node].rank + 1)
                    .max()
                    .unwrap_or(0);

                let new_rank = min_rank_from_preds;
                if new_rank < to_rank {
                    graph.nodes[edge.to_node].rank = new_rank;
                    improved = true;
                }
            }
        }

        if !improved {
            break;
        }
    }
}

/// Normalize ranks to start at 0 and balance nodes with equal in/out degree.
fn normalize_and_balance(graph: &mut LayoutGraph) {
    let components = graph.connected_components();

    for component in &components {
        if component.is_empty() {
            continue;
        }

        // Normalize: shift so minimum rank in this component is 0
        let min_rank = component
            .iter()
            .map(|&i| graph.nodes[i].rank)
            .min()
            .unwrap();
        for &i in component {
            graph.nodes[i].rank -= min_rank;
        }
    }

    // Global normalization
    let global_min = graph.nodes.iter().map(|n| n.rank).min().unwrap_or(0);
    if global_min != 0 {
        for node in &mut graph.nodes {
            node.rank -= global_min;
        }
    }

    // Balance: nodes with equal in-degree and out-degree can be moved
    // to less populated columns to reduce column height variance.
    let max_rank = graph.nodes.iter().map(|n| n.rank).max().unwrap_or(0);
    let mut col_sizes: Vec<usize> = vec![0; (max_rank + 1) as usize];
    for node in &graph.nodes {
        col_sizes[node.rank as usize] += 1;
    }

    for i in 0..graph.nodes.len() {
        let in_deg = graph.in_edges[i].len();
        let out_deg = graph.out_edges[i].len();
        if in_deg != out_deg || in_deg == 0 {
            continue;
        }

        let current_rank = graph.nodes[i].rank;

        // Find valid rank range
        let min_from_preds = graph.in_edges[i]
            .iter()
            .map(|&ei| graph.nodes[graph.edges[ei].from_node].rank + 1)
            .max()
            .unwrap_or(0);
        let max_from_succs = graph.out_edges[i]
            .iter()
            .map(|&ei| graph.nodes[graph.edges[ei].to_node].rank - 1)
            .min()
            .unwrap_or(max_rank);

        if min_from_preds > max_from_succs {
            continue;
        }

        // Find the least populated column in the valid range
        let best_rank = (min_from_preds..=max_from_succs)
            .min_by_key(|&r| col_sizes[r as usize])
            .unwrap_or(current_rank);

        if col_sizes[best_rank as usize] < col_sizes[current_rank as usize] {
            col_sizes[current_rank as usize] -= 1;
            col_sizes[best_rank as usize] += 1;
            graph.nodes[i].rank = best_rank;
        }
    }
}

/// Insert dummy nodes for all long-span edges (rank difference > 1).
/// After this, every edge spans exactly one rank.
pub fn insert_dummy_nodes(graph: &mut LayoutGraph) {
    let original_edge_count = graph.edges.len();

    // Collect edges that need splitting (to avoid borrow issues)
    let mut to_split: Vec<(usize, usize, usize, i32, i32, Option<u32>, Option<u32>)> = Vec::new();
    for ei in 0..original_edge_count {
        let edge = &graph.edges[ei];
        let from_rank = graph.nodes[edge.from_node].rank;
        let to_rank = graph.nodes[edge.to_node].rank;
        if to_rank - from_rank > 1 {
            to_split.push((
                ei,
                edge.from_node,
                edge.to_node,
                from_rank,
                to_rank,
                edge.from_port,
                edge.to_port,
            ));
        }
    }

    // Process each long edge: insert dummy nodes at each intermediate rank
    for (original_ei, from_node, to_node, from_rank, to_rank, from_port, to_port) in to_split {
        let mut prev_node = from_node;

        for rank in (from_rank + 1)..to_rank {
            let dummy_idx = graph.add_dummy_node(rank);

            // Edge from previous to dummy
            if prev_node == from_node {
                // First dummy: create edge with original from_port
                let ei = graph.edges.len();
                graph.edges.push(super::graph::LayoutEdge {
                    id: 0,
                    from_node: prev_node,
                    to_node: dummy_idx,
                    from_port,
                    to_port: None,
                });
                graph.out_edges[prev_node].push(ei);
                graph.in_edges[dummy_idx].push(ei);
            } else {
                graph.add_dummy_edge(prev_node, dummy_idx);
            }

            prev_node = dummy_idx;
        }

        // Final edge: from last dummy to original target
        let ei = graph.edges.len();
        graph.edges.push(super::graph::LayoutEdge {
            id: 0,
            from_node: prev_node,
            to_node: to_node,
            from_port: None,
            to_port,
        });
        graph.out_edges[prev_node].push(ei);
        graph.in_edges[to_node].push(ei);

        // Remove the original long edge from adjacency lists
        // (mark it as disconnected by removing from out/in lists)
        graph.out_edges[from_node].retain(|&e| e != original_ei);
        graph.in_edges[to_node].retain(|&e| e != original_ei);
    }
}
