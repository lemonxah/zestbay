use super::graph::{LayoutGraph, LayoutNodeKind};

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

const MAX_BALANCE_PASSES: usize = 8;
const MIN_RELATIVE_GAIN: f64 = 1e-9;

#[derive(Debug, Clone, Copy, Default)]
struct ColumnLoad {
    count: usize,
    content: f64,
}

impl ColumnLoad {
    fn with(self, height: f64) -> Self {
        Self {
            count: self.count + 1,
            content: self.content + height,
        }
    }

    fn without(self, height: f64) -> Self {
        Self {
            count: self.count - 1,
            content: self.content - height,
        }
    }

    fn unevenness(self, margin_y: f64) -> f64 {
        let stacked = self.content + self.count.saturating_sub(1) as f64 * margin_y;
        stacked * stacked
    }
}

fn balance(graph: &mut LayoutGraph) {
    let Some(first_rank) = graph.nodes.iter().map(|n| n.rank).min() else {
        return;
    };
    let last_rank = graph.nodes.iter().map(|n| n.rank).max().unwrap_or(first_rank);
    let mut loads = vec![ColumnLoad::default(); (last_rank - first_rank + 1) as usize];
    for node in &graph.nodes {
        let load = &mut loads[(node.rank - first_rank) as usize];
        *load = load.with(node.height);
    }

    let mut movable: Vec<usize> = (0..graph.nodes.len())
        .filter(|&v| !graph.in_edges[v].is_empty() && !graph.out_edges[v].is_empty())
        .collect();
    movable.sort_by(|&a, &b| {
        graph.nodes[b]
            .height
            .total_cmp(&graph.nodes[a].height)
            .then(a.cmp(&b))
    });

    for _ in 0..MAX_BALANCE_PASSES {
        let mut moved = false;
        for &v in &movable {
            let Some(target) = less_crowded_rank(graph, &loads, first_rank, v) else {
                continue;
            };
            let height = graph.nodes[v].height;
            let from = (graph.nodes[v].rank - first_rank) as usize;
            let to = (target - first_rank) as usize;
            loads[from] = loads[from].without(height);
            loads[to] = loads[to].with(height);
            graph.nodes[v].rank = target;
            moved = true;
        }
        if !moved {
            break;
        }
    }
}

fn less_crowded_rank(
    graph: &LayoutGraph,
    loads: &[ColumnLoad],
    first_rank: i32,
    v: usize,
) -> Option<i32> {
    let RankWindow { lowest, highest } = non_lengthening_window(graph, v)?;
    let current = graph.nodes[v].rank;
    let height = graph.nodes[v].height;
    let margin_y = graph.config.margin_y;
    let from = loads[(current - first_rank) as usize];
    if from.count < 2 {
        return None;
    }
    let from_before = from.unevenness(margin_y);
    let from_after = from.without(height).unevenness(margin_y);

    (lowest..=highest)
        .filter(|&rank| rank != current)
        .filter_map(|rank| {
            let to = loads[(rank - first_rank) as usize];
            let before = from_before + to.unevenness(margin_y);
            let after = from_after + to.with(height).unevenness(margin_y);
            let gain = before - after;
            (gain > before * MIN_RELATIVE_GAIN).then_some((rank, gain))
        })
        .min_by(|a, b| {
            b.1.total_cmp(&a.1)
                .then((a.0 - current).abs().cmp(&(b.0 - current).abs()))
                .then(a.0.cmp(&b.0))
        })
        .map(|(rank, _)| rank)
}

struct RankWindow {
    lowest: i32,
    highest: i32,
}

fn non_lengthening_window(graph: &LayoutGraph, v: usize) -> Option<RankWindow> {
    let current = graph.nodes[v].rank;
    let after_inputs = graph.in_edges[v]
        .iter()
        .map(|&ei| graph.nodes[graph.edges[ei].from_node].rank + 1)
        .max()?;
    let before_outputs = graph.out_edges[v]
        .iter()
        .map(|&ei| graph.nodes[graph.edges[ei].to_node].rank - 1)
        .min()?;
    let inputs = graph.in_edges[v].len();
    let outputs = graph.out_edges[v].len();
    Some(RankWindow {
        lowest: if outputs > inputs { current } else { after_inputs },
        highest: if inputs > outputs { current } else { before_outputs },
    })
}

pub fn insert_dummy_nodes(graph: &mut LayoutGraph) {
    let original_edge_count = graph.edges.len();

    type SplitEdge = (usize, usize, usize, i32, i32, Option<u32>, Option<u32>);
    let mut to_split: Vec<SplitEdge> = Vec::new();
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

    for (original_ei, from_node, to_node, from_rank, to_rank, from_port, to_port) in to_split {
        let mut prev_node = from_node;

        for rank in (from_rank + 1)..to_rank {
            let dummy_idx = graph.add_dummy_node(rank);

            if prev_node == from_node {
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

        let ei = graph.edges.len();
        graph.edges.push(super::graph::LayoutEdge {
            id: 0,
            from_node: prev_node,
            to_node,
            from_port: None,
            to_port,
        });
        graph.out_edges[prev_node].push(ei);
        graph.in_edges[to_node].push(ei);

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

    fn graph_with_ranks(
        specs: &[(&'static str, f64, i32)],
        wires: &[(usize, usize)],
    ) -> LayoutGraph {
        let nodes = specs
            .iter()
            .enumerate()
            .map(|(i, &(kind, height, _))| (i as u32, String::new(), kind, 100.0, height))
            .collect();
        let links = wires
            .iter()
            .enumerate()
            .map(|(i, &(a, b))| (i as u32, a as u32, 0, b as u32, 0))
            .collect();
        let mut g = LayoutGraph::new(nodes, vec![], links, LayoutConfig::default());
        for (node, spec) in g.nodes.iter_mut().zip(specs) {
            node.rank = spec.2;
        }
        g
    }

    fn total_length(graph: &LayoutGraph) -> i32 {
        graph
            .edges
            .iter()
            .map(|e| graph.nodes[e.to_node].rank - graph.nodes[e.from_node].rank)
            .sum()
    }

    fn used_ranks(graph: &LayoutGraph) -> std::collections::BTreeSet<i32> {
        graph.nodes.iter().map(|n| n.rank).collect()
    }

    fn total_unevenness(graph: &LayoutGraph) -> f64 {
        let mut loads: std::collections::BTreeMap<i32, ColumnLoad> = Default::default();
        for node in &graph.nodes {
            let load = loads.entry(node.rank).or_default();
            *load = load.with(node.height);
        }
        loads
            .values()
            .map(|load| load.unevenness(graph.config.margin_y))
            .sum()
    }

    const CHAIN_WITH_BYPASS: [(usize, usize); 6] = [(0, 1), (1, 2), (2, 3), (3, 4), (0, 5), (5, 4)];

    fn chain_with_bypass(chain_heights: [f64; 3], bypass_height: f64) -> LayoutGraph {
        let [a, b, c] = chain_heights;
        graph_with_ranks(
            &[
                ("Source", 50.0, 0),
                ("Plugin", a, 1),
                ("Plugin", b, 2),
                ("Plugin", c, 3),
                ("Sink", 50.0, 4),
                ("Plugin", bypass_height, 1),
            ],
            &CHAIN_WITH_BYPASS,
        )
    }

    #[test]
    fn balance_accepts_empty_and_unwired_graphs() {
        let mut empty = graph_with_ranks(&[], &[]);
        balance(&mut empty);
        assert!(empty.nodes.is_empty());

        let mut unwired = graph_with_ranks(
            &[("Plugin", 500.0, 0), ("Plugin", 500.0, 0), ("Sink", 10.0, 1)],
            &[],
        );
        balance(&mut unwired);
        assert_eq!(ranks(&unwired), vec![0, 0, 1]);
    }

    #[test]
    fn balance_moves_free_node_to_least_crowded_column() {
        let mut g = chain_with_bypass([100.0, 20.0, 60.0], 80.0);
        let length = total_length(&g);
        balance(&mut g);
        assert_eq!(ranks(&g), vec![0, 1, 2, 3, 4, 2]);
        assert_eq!(total_length(&g), length);

        let mut g = chain_with_bypass([100.0, 60.0, 20.0], 80.0);
        balance(&mut g);
        assert_eq!(ranks(&g)[5], 3);
    }

    #[test]
    fn balance_reaches_through_the_whole_ranking() {
        let mut g = chain_with_bypass([100.0, 20.0, 60.0], 80.0);
        for node in &mut g.nodes {
            node.rank = 0;
        }
        compute_ranks(&mut g);
        assert_eq!(ranks(&g), vec![0, 1, 2, 3, 4, 2]);
    }

    #[test]
    fn balance_prefers_the_nearest_of_equal_columns() {
        let mut g = chain_with_bypass([100.0, 20.0, 20.0], 80.0);
        balance(&mut g);
        assert_eq!(ranks(&g)[5], 2);
    }

    #[test]
    fn balance_keeps_node_in_place_on_a_tie() {
        let mut g = chain_with_bypass([60.0, 60.0, 60.0], 60.0);
        balance(&mut g);
        assert_eq!(ranks(&g)[5], 1);

        let mut g = chain_with_bypass([33.3, 33.3, 33.3], 77.7);
        balance(&mut g);
        assert_eq!(ranks(&g)[5], 1);
    }

    #[test]
    fn balance_weighs_heights_rather_than_node_counts() {
        let mut specs = vec![
            ("Source", 50.0, 0),
            ("Plugin", 400.0, 1),
            ("Plugin", 20.0, 2),
            ("Sink", 50.0, 3),
            ("Plugin", 60.0, 1),
            ("Plugin", 20.0, 2),
            ("Plugin", 20.0, 2),
        ];
        let wires = [
            (0, 1),
            (1, 2),
            (2, 3),
            (0, 4),
            (4, 3),
            (1, 5),
            (5, 3),
            (1, 6),
            (6, 3),
        ];
        let mut g = graph_with_ranks(&specs, &wires);
        balance(&mut g);
        assert_eq!(ranks(&g)[4], 2);

        specs[1].1 = 40.0;
        let mut g = graph_with_ranks(&specs, &wires);
        balance(&mut g);
        assert_eq!(ranks(&g)[4], 1);
    }

    #[test]
    fn balance_never_lengthens_wires() {
        let specs = [
            ("Source", 50.0, 0),
            ("Plugin", 300.0, 1),
            ("Plugin", 10.0, 2),
            ("Sink", 50.0, 3),
            ("Plugin", 80.0, 1),
        ];
        let chain = [(0, 1), (1, 2), (2, 3)];

        let more_inputs = [&chain[..], &[(0, 4), (0, 4), (4, 3)]].concat();
        let mut g = graph_with_ranks(&specs, &more_inputs);
        balance(&mut g);
        assert_eq!(ranks(&g)[4], 1);

        let more_outputs = [&chain[..], &[(0, 4), (4, 3), (4, 3)]].concat();
        let mut g = graph_with_ranks(&specs, &more_outputs);
        let length = total_length(&g);
        balance(&mut g);
        assert_eq!(ranks(&g)[4], 2);
        assert_eq!(total_length(&g), length - 1);
    }

    #[test]
    fn balance_leaves_sources_sinks_and_isolated_nodes() {
        let mut g = graph_with_ranks(
            &[
                ("Source", 50.0, 0),
                ("Plugin", 20.0, 1),
                ("Plugin", 20.0, 2),
                ("Sink", 50.0, 3),
                ("Source", 400.0, 0),
                ("Plugin", 400.0, 3),
                ("Plugin", 400.0, 0),
            ],
            &[(0, 1), (1, 2), (2, 3), (4, 3), (0, 5)],
        );
        let before = ranks(&g);
        balance(&mut g);
        assert_eq!(ranks(&g), before);
    }

    #[test]
    fn balance_never_empties_a_column() {
        let mut g = graph_with_ranks(
            &[("Source", 50.0, 0), ("Plugin", 80.0, 1), ("Sink", 50.0, 3)],
            &[(0, 1), (1, 2)],
        );
        balance(&mut g);
        assert_eq!(ranks(&g), vec![0, 1, 3]);
    }

    #[test]
    fn balance_invariants_hold_on_random_graphs() {
        let mut state = 0x5eed_1234_u64;
        let mut next = move |bound: u64| {
            state = state.wrapping_mul(0x5851_f42d_4c95_7f2d).wrapping_add(0x1405_7b7e_f767_814f);
            (state >> 33) % bound
        };
        let mut moved_nodes = 0;
        for _ in 0..200 {
            let count = 5 + next(60) as usize;
            let kinds = ["Plugin", "Source", "Sink", "Plugin", "Plugin"];
            let specs: Vec<(&'static str, f64, i32)> = (0..count)
                .map(|_| (kinds[next(5) as usize], 30.0 + next(400) as f64 * 0.7, 0))
                .collect();
            let wires: Vec<(usize, usize)> = (0..count + next(count as u64) as usize)
                .map(|_| {
                    let a = next(count as u64) as usize;
                    let b = next(count as u64) as usize;
                    (a.min(b), a.max(b))
                })
                .collect();
            let mut g = graph_with_ranks(&specs, &wires);
            let order = initial_ranking(&mut g);
            tighten_ranks(&mut g, &order);
            normalize_components(&mut g);
            place_sinks(&mut g);

            let before = ranks(&g);
            let length = total_length(&g);
            let used = used_ranks(&g);
            let unevenness = total_unevenness(&g);
            balance(&mut g);

            assert!(g
                .edges
                .iter()
                .all(|e| g.nodes[e.to_node].rank > g.nodes[e.from_node].rank));
            assert!(total_length(&g) <= length);
            assert!(used.is_subset(&used_ranks(&g)));
            assert_eq!(used.first(), used_ranks(&g).first());
            assert_eq!(used.last(), used_ranks(&g).last());
            for (v, &rank) in before.iter().enumerate() {
                if g.in_edges[v].is_empty() || g.out_edges[v].is_empty() {
                    assert_eq!(g.nodes[v].rank, rank);
                } else if g.nodes[v].rank != rank {
                    moved_nodes += 1;
                }
            }
            if ranks(&g) == before {
                assert_eq!(total_unevenness(&g), unevenness);
            } else {
                assert!(total_unevenness(&g) < unevenness);
            }

            let after = ranks(&g);
            let mut again = graph_with_ranks(&specs, &wires);
            for (node, &rank) in again.nodes.iter_mut().zip(&before) {
                node.rank = rank;
            }
            balance(&mut again);
            assert_eq!(ranks(&again), after);
        }
        assert!(moved_nodes > 50);
    }
}
