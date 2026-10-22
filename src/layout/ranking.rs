// SPDX-License-Identifier: MIT
//
// Ranking phase: assign each node to a column/layer.
//
//   1. Break cycles (reverse DFS back edges; self-loops are dropped earlier).
//   2. Longest-path ranking from sources.
//   3. Tightening: nodes with more outgoing than incoming edges are pulled
//      right toward their successors (shortens total edge length); source
//      devices/streams stay in the leftmost column.
//   4. Sink placement: sink-only nodes go to the rightmost column of their
//      component; isolated sinks go to the global last column.
//   5. Balancing of pass-through nodes into less populated columns.

use super::graph::{LayoutGraph, LayoutNodeKind};

/// Assign ranks to all nodes.  Afterwards every edge goes from a lower to a
/// strictly higher rank and the minimum rank is 0.
pub fn compute_ranks(graph: &mut LayoutGraph) {
    let n = graph.nodes.len();
    if n == 0 {
        return;
    }

    let reversed = graph.break_cycles();
    if reversed > 0 {
        log::debug!("Layout: reversed {} feedback edge(s) to break cycles", reversed);
    }

    let order = initial_ranking(graph);
    tighten_ranks(graph, &order);
    normalize_components(graph);
    place_sinks(graph);
    balance(graph);

    debug_assert!(graph
        .edges
        .iter()
        .all(|e| graph.nodes[e.to_node].rank > graph.nodes[e.from_node].rank));
}

/// Longest-path ranking: rank(v) = length of the longest path from a source.
/// Returns the nodes in topological order.
fn initial_ranking(graph: &mut LayoutGraph) -> Vec<usize> {
    let generations = graph.topological_generations();
    for (gen_idx, generation) in generations.iter().enumerate() {
        for &node_idx in generation {
            graph.nodes[node_idx].rank = gen_idx as i32;
        }
    }
    generations.into_iter().flatten().collect()
}

fn is_source_kind(kind: LayoutNodeKind) -> bool {
    matches!(kind, LayoutNodeKind::Source | LayoutNodeKind::StreamOutput)
}

fn is_sink_kind(kind: LayoutNodeKind) -> bool {
    matches!(kind, LayoutNodeKind::Sink | LayoutNodeKind::StreamInput)
}

/// Pull nodes with out-degree > in-degree as far right as their successors
/// allow.  Moving such a node right by one shortens more edges than it
/// lengthens, so this strictly reduces total edge length.  Processed in
/// reverse topological order so successors are final first.
fn tighten_ranks(graph: &mut LayoutGraph, topo_order: &[usize]) {
    for &v in topo_order.iter().rev() {
        let in_deg = graph.in_edges[v].len();
        let out_deg = graph.out_edges[v].len();
        if out_deg <= in_deg {
            continue;
        }
        if in_deg == 0 && is_source_kind(graph.nodes[v].kind) {
            continue;
        }
        let max_rank = graph.out_edges[v]
            .iter()
            .map(|&ei| graph.nodes[graph.edges[ei].to_node].rank - 1)
            .min()
            .unwrap_or(graph.nodes[v].rank);
        if max_rank > graph.nodes[v].rank {
            graph.nodes[v].rank = max_rank;
        }
    }
}

/// Shift each connected component so its minimum rank is 0.
fn normalize_components(graph: &mut LayoutGraph) {
    for component in graph.connected_components() {
        let min_rank = component.iter().map(|&i| graph.nodes[i].rank).min().unwrap_or(0);
        if min_rank != 0 {
            for &i in &component {
                graph.nodes[i].rank -= min_rank;
            }
        }
    }
}

/// Sink-only nodes (incoming but no outgoing edges) go to the last column of
/// their component — except plugins with an unconnected output, which stay
/// next to their input.  Isolated sink devices/streams go to the global last
/// column, other isolated nodes to column 0.
fn place_sinks(graph: &mut LayoutGraph) {
    let mut isolated_sinks = Vec::new();
    for component in graph.connected_components() {
        if component.len() == 1 {
            let v = component[0];
            graph.nodes[v].rank = 0;
            if is_sink_kind(graph.nodes[v].kind) {
                isolated_sinks.push(v);
            }
            continue;
        }
        let comp_max = component.iter().map(|&i| graph.nodes[i].rank).max().unwrap_or(0);
        for &v in &component {
            if graph.out_edges[v].is_empty()
                && !graph.in_edges[v].is_empty()
                && graph.nodes[v].kind != LayoutNodeKind::Plugin
            {
                graph.nodes[v].rank = comp_max;
            }
        }
    }

    if !isolated_sinks.is_empty() {
        let max_rank = graph.nodes.iter().map(|n| n.rank).max().unwrap_or(0);
        let has_others = isolated_sinks.len() < graph.nodes.len();
        let last = if has_others { max_rank.max(1) } else { max_rank };
        for v in isolated_sinks {
            graph.nodes[v].rank = last;
        }
    }
}

/// Balance: nodes with equal in-degree and out-degree can be moved within
/// their feasible range to less populated columns to reduce column height
/// variance.
fn balance(graph: &mut LayoutGraph) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::graph::LayoutConfig;

    fn ranks(graph: &LayoutGraph) -> Vec<i32> {
        graph.nodes.iter().map(|n| n.rank).collect()
    }

    #[test]
    fn cycle_nodes_spread_across_columns() {
        // S → A → B → C → A, C → T
        let nodes = vec![
            (1, "S".into(), "Source", 100.0, 50.0),
            (2, "A".into(), "Plugin", 100.0, 50.0),
            (3, "B".into(), "Plugin", 100.0, 50.0),
            (4, "C".into(), "Plugin", 100.0, 50.0),
            (5, "T".into(), "Sink", 100.0, 50.0),
        ];
        let links = vec![
            (10, 1, 0, 2, 0),
            (11, 2, 0, 3, 0),
            (12, 3, 0, 4, 0),
            (13, 4, 0, 2, 0),
            (14, 4, 0, 5, 0),
        ];
        let mut g = LayoutGraph::new(nodes, vec![], links, LayoutConfig::default());
        compute_ranks(&mut g);
        assert_eq!(ranks(&g), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn isolated_sink_goes_last_isolated_source_first() {
        let nodes = vec![
            (1, "Mic".into(), "Source", 100.0, 50.0),
            (2, "EQ".into(), "Plugin", 100.0, 50.0),
            (3, "Spk".into(), "Sink", 100.0, 50.0),
            (4, "LoneSink".into(), "Sink", 100.0, 50.0),
            (5, "LoneSrc".into(), "Source", 100.0, 50.0),
        ];
        let links = vec![(10, 1, 0, 2, 0), (11, 2, 0, 3, 0)];
        let mut g = LayoutGraph::new(nodes, vec![], links, LayoutConfig::default());
        compute_ranks(&mut g);
        assert_eq!(ranks(&g), vec![0, 1, 2, 2, 0]);
    }

    #[test]
    fn sink_moves_to_component_last_column_and_generator_tightens() {
        // Mic → EQ → Comp → Spk, Mic → Rec (sink), Gen(plugin, no inputs) → Comp
        let nodes = vec![
            (1, "Mic".into(), "Source", 100.0, 50.0),
            (2, "EQ".into(), "Plugin", 100.0, 50.0),
            (3, "Comp".into(), "Plugin", 100.0, 50.0),
            (4, "Spk".into(), "Sink", 100.0, 50.0),
            (5, "Rec".into(), "StreamInput", 100.0, 50.0),
            (6, "Gen".into(), "Plugin", 100.0, 50.0),
        ];
        let links = vec![
            (10, 1, 0, 2, 0),
            (11, 2, 0, 3, 0),
            (12, 3, 0, 4, 0),
            (13, 1, 0, 5, 0),
            (14, 6, 0, 3, 0),
        ];
        let mut g = LayoutGraph::new(nodes, vec![], links, LayoutConfig::default());
        compute_ranks(&mut g);
        let r = ranks(&g);
        assert_eq!(r[0], 0, "source stays left");
        assert_eq!(r[3], 3);
        assert_eq!(r[4], 3, "sink-only stream goes to last column");
        assert_eq!(r[5], 1, "generator pulled next to its successor");
    }
}
