// SPDX-License-Identifier: MIT
//
// Sugiyama-style layered graph layout algorithm.
//
// This module implements the classic Sugiyama framework for hierarchical
// graph drawing, adapted from the node-arrange Blender addon:
// https://github.com/Leonardo-Pike-Excell/node-arrange
//
// The pipeline consists of 5 phases:
//   1. Ranking: Assign each node to a column/layer (cycle breaking, longest path, tightening)
//   2. Dummy nodes: Insert virtual nodes for edges spanning multiple layers
//   3. Ordering: Minimize edge crossings via barycenter heuristic
//   4. Y-coordinates: Brandes-Köpf vertical positioning (4-direction balanced)
//   5. X-coordinates: Adaptive column spacing based on edge density
//
// The algorithm preserves the center of mass of the original graph.

pub mod graph;
mod ordering;
mod ranking;
mod x_coords;
mod y_coords;

use graph::{LayoutConfig, LayoutGraph, NodeId};
use std::collections::HashMap;

pub type LayoutResult = HashMap<NodeId, (f64, f64)>;

/// `pinned`: nodes with fixed positions — participate in ranking/ordering but keep their positions.
pub fn sugiyama_layout(
    mut nodes: Vec<(NodeId, String, &str, f64, f64)>,
    mut ports: Vec<(u32, u32, usize, bool)>,
    mut links: Vec<(u32, u32, u32, u32, u32)>,
    config: LayoutConfig,
    pinned: &HashMap<NodeId, (f64, f64)>,
) -> LayoutResult {
    if nodes.is_empty() {
        return HashMap::new();
    }

    // Callers often build these from HashMap iteration; sort so the result
    // only depends on the graph, not on input order.
    nodes.sort_by_key(|n| n.0);
    nodes.dedup_by_key(|n| n.0);
    ports.sort_by_key(|p| (p.0, p.1));
    links.sort_by_key(|l| (l.0, l.1, l.2, l.3, l.4));

    let margin_y = config.margin_y;
    let mut graph = LayoutGraph::new(nodes, ports, links, config);

    log::debug!(
        "Layout: {} nodes ({} pinned), {} edges",
        graph.nodes.len(),
        pinned.len(),
        graph.edges.len()
    );

    ranking::compute_ranks(&mut graph);
    ranking::insert_dummy_nodes(&mut graph);
    graph.build_columns();

    log::debug!(
        "Layout: {} columns, {} total nodes (with dummies)",
        graph.columns.len(),
        graph.nodes.len()
    );

    ordering::minimize_crossings(&mut graph);
    y_coords::assign_y_coords(&mut graph);
    x_coords::assign_x_coords(&mut graph);

    let result = if pinned.is_empty() {
        graph
            .nodes
            .iter()
            .filter(|n| !n.is_dummy)
            .map(|n| (n.id, (n.x, n.y)))
            .collect()
    } else {
        apply_pins(&graph, pinned, margin_y)
    };

    log::debug!("Layout: complete, {} positions", result.len());
    result
}

/// Pinned nodes keep their positions; every free node is moved by the delta
/// (pinned − computed) of its nearest pinned node in the graph (or the average
/// delta if it is not connected to any pin), then pushed clear of anything it
/// would overlap.
fn apply_pins(
    graph: &LayoutGraph,
    pinned: &HashMap<NodeId, (f64, f64)>,
    margin_y: f64,
) -> LayoutResult {
    let n = graph.nodes.len();
    let is_real = |i: usize| !graph.nodes[i].is_dummy;

    // Multi-source BFS from all pinned nodes (in index order, so ties resolve
    // deterministically): each node inherits the delta of the pin that
    // reaches it first.
    let mut delta: Vec<Option<(f64, f64)>> = vec![None; n];
    let mut queue = std::collections::VecDeque::new();
    let (mut sum_dx, mut sum_dy, mut pin_count) = (0.0, 0.0, 0usize);
    for node in &graph.nodes {
        if !is_real(node.idx) {
            continue;
        }
        if let Some(&(px, py)) = pinned.get(&node.id) {
            let d = (px - node.x, py - node.y);
            delta[node.idx] = Some(d);
            sum_dx += d.0;
            sum_dy += d.1;
            pin_count += 1;
            queue.push_back(node.idx);
        }
    }
    while let Some(v) = queue.pop_front() {
        let d = delta[v];
        let nbrs = graph.out_edges[v]
            .iter()
            .map(|&ei| graph.edges[ei].to_node)
            .chain(graph.in_edges[v].iter().map(|&ei| graph.edges[ei].from_node));
        for w in nbrs {
            if delta[w].is_none() {
                delta[w] = d;
                queue.push_back(w);
            }
        }
    }
    let fallback = if pin_count > 0 {
        (sum_dx / pin_count as f64, sum_dy / pin_count as f64)
    } else {
        (0.0, 0.0)
    };

    let mut result: LayoutResult = HashMap::new();
    // Occupied rectangles (x, y, w, h): pins first, then free nodes as placed.
    let mut placed: Vec<(f64, f64, f64, f64)> = Vec::new();
    let mut free: Vec<(usize, f64, f64)> = Vec::new();
    for node in graph.nodes.iter().filter(|n| is_real(n.idx)) {
        if let Some(&pos) = pinned.get(&node.id) {
            result.insert(node.id, pos);
            placed.push((pos.0, pos.1, node.width, node.height));
        } else {
            let (dx, dy) = delta[node.idx].unwrap_or(fallback);
            free.push((node.idx, node.x + dx, node.y + dy));
        }
    }

    // Place free nodes column by column, top to bottom, so their relative
    // order is kept when they have to make room.
    free.sort_by(|a, b| {
        graph.nodes[a.0]
            .rank
            .cmp(&graph.nodes[b.0].rank)
            .then(a.2.total_cmp(&b.2))
            .then(a.0.cmp(&b.0))
    });
    for (idx, x, y) in free {
        let node = &graph.nodes[idx];
        let y = nearest_free_y(&placed, x, y, node.width, node.height, margin_y);
        placed.push((x, y, node.width, node.height));
        result.insert(node.id, (x, y));
    }
    result
}

/// True if rectangle (x, y, w, h) overlaps `r` horizontally and comes closer
/// than `gap` vertically.
fn collides(r: &(f64, f64, f64, f64), x: f64, y: f64, w: f64, h: f64, gap: f64) -> bool {
    const EPS: f64 = 1e-6;
    x < r.0 + r.2 - EPS
        && r.0 < x + w - EPS
        && y < r.1 + r.3 + gap - EPS
        && r.1 < y + h + gap - EPS
}

/// The y closest to `y` at which a w×h rectangle at `x` keeps `gap` from
/// every rectangle in `placed` (searching both downward and upward).
fn nearest_free_y(placed: &[(f64, f64, f64, f64)], x: f64, y: f64, w: f64, h: f64, gap: f64) -> f64 {
    let search = |down: bool| -> f64 {
        let mut cy = y;
        // Each step moves strictly past at least one rectangle, so this ends
        // after at most placed.len() steps.
        for _ in 0..=placed.len() {
            let hits = placed.iter().filter(|r| collides(r, x, cy, w, h, gap));
            let next = if down {
                hits.map(|r| r.1 + r.3 + gap).fold(f64::NEG_INFINITY, f64::max)
            } else {
                hits.map(|r| r.1 - h - gap).fold(f64::INFINITY, f64::min)
            };
            if !next.is_finite() {
                return cy;
            }
            cy = next;
        }
        cy
    };
    let down = search(true);
    let up = search(false);
    if (up - y).abs() < (down - y).abs() {
        up
    } else {
        down
    }
}

/// Put a finished layout on a `grid`: round every free node's position to
/// the nearest grid point, then push any node that now collides with one
/// already placed (keeping `gap` between them) down by whole grid steps.
/// Pinned nodes are obstacles and keep their positions. Nodes are placed
/// left to right, top to bottom, so columns stay stacked in order.
pub fn snap_to_grid(
    positions: &mut LayoutResult,
    sizes: &HashMap<NodeId, (f64, f64)>,
    pinned: &HashMap<NodeId, (f64, f64)>,
    grid: f64,
    gap: f64,
) {
    if !(grid > 0.0) {
        return;
    }
    let size = |id: &NodeId| sizes.get(id).copied().unwrap_or((0.0, 0.0));
    let mut placed: Vec<(f64, f64, f64, f64)> = positions
        .iter()
        .filter(|(id, _)| pinned.contains_key(id))
        .map(|(id, &(x, y))| {
            let (w, h) = size(id);
            (x, y, w, h)
        })
        .collect();

    let mut free: Vec<(NodeId, f64, f64)> = positions
        .iter()
        .filter(|(id, _)| !pinned.contains_key(id))
        .map(|(&id, &(x, y))| (id, x, y))
        .collect();
    free.sort_by(|a, b| {
        a.1.total_cmp(&b.1)
            .then(a.2.total_cmp(&b.2))
            .then(a.0.cmp(&b.0))
    });

    for (id, x, y) in free {
        let (w, h) = size(&id);
        let gx = (x / grid).round() * grid;
        let mut gy = (y / grid).round() * grid;
        // Each step clears at least one grid row; bounded by the stack size.
        let limit = placed.iter().map(|r| r.3 + gap).sum::<f64>() / grid + placed.len() as f64 + 1.0;
        let mut steps = 0.0;
        while steps < limit && placed.iter().any(|r| collides(r, gx, gy, w, h, gap)) {
            gy += grid;
            steps += 1.0;
        }
        placed.push((gx, gy, w, h));
        positions.insert(id, (gx, gy));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_chain() {
        // Source → Plugin → Sink
        let nodes = vec![
            (1, "Mic".into(), "Source", 180.0, 80.0),
            (2, "EQ".into(), "Plugin", 180.0, 100.0),
            (3, "Speakers".into(), "Sink", 180.0, 80.0),
        ];
        let ports = vec![
            (10, 1, 0, true),  // Mic output
            (20, 2, 0, false), // EQ input
            (21, 2, 0, true),  // EQ output
            (30, 3, 0, false), // Speakers input
        ];
        let links = vec![
            (100, 1, 10, 2, 20), // Mic → EQ
            (101, 2, 21, 3, 30), // EQ → Speakers
        ];
        let no_pins = HashMap::new();
        let result = sugiyama_layout(nodes, ports, links, LayoutConfig::default(), &no_pins);

        assert_eq!(result.len(), 3);
        let mic = result[&1];
        let eq = result[&2];
        let spk = result[&3];

        assert!(mic.0 < eq.0, "Mic x={} should be < EQ x={}", mic.0, eq.0);
        assert!(
            eq.0 < spk.0,
            "EQ x={} should be < Speakers x={}",
            eq.0,
            spk.0
        );
    }

    #[test]
    fn test_parallel_streams() {
        let nodes = vec![
            (1, "Firefox".into(), "StreamOutput", 180.0, 80.0),
            (2, "Spotify".into(), "StreamOutput", 180.0, 80.0),
            (3, "Speakers".into(), "Sink", 180.0, 80.0),
        ];
        let ports = vec![
            (10, 1, 0, true),
            (20, 2, 0, true),
            (30, 3, 0, false),
            (31, 3, 1, false),
        ];
        let links = vec![(100, 1, 10, 3, 30), (101, 2, 20, 3, 31)];
        let no_pins = HashMap::new();
        let result = sugiyama_layout(nodes, ports, links, LayoutConfig::default(), &no_pins);

        assert_eq!(result.len(), 3);
        let ff = result[&1];
        let sp = result[&2];
        let spk = result[&3];

        assert!(
            ff.0 < spk.0,
            "Firefox x={} should be < Speakers x={}",
            ff.0,
            spk.0
        );
        assert!(
            sp.0 < spk.0,
            "Spotify x={} should be < Speakers x={}",
            sp.0,
            spk.0
        );
        let ff_bottom = ff.1 + 80.0;
        assert!(
            ff_bottom <= sp.1 || sp.1 + 80.0 <= ff.1,
            "Firefox [{}, {}] and Spotify [{}, {}] overlap vertically",
            ff.1,
            ff_bottom,
            sp.1,
            sp.1 + 80.0
        );
    }

    #[test]
    fn test_isolated_nodes() {
        let nodes = vec![
            (1, "A".into(), "Source", 180.0, 80.0),
            (2, "B".into(), "Sink", 180.0, 80.0),
            (3, "C".into(), "StreamOutput", 180.0, 80.0),
        ];
        let no_pins = HashMap::new();
        let result = sugiyama_layout(nodes, vec![], vec![], LayoutConfig::default(), &no_pins);

        assert_eq!(result.len(), 3);
        // Isolated sources stay in the first column, isolated sinks go last.
        assert_eq!(result[&1].0, result[&3].0);
        assert!(result[&2].0 > result[&1].0);
        assert!(
            (result[&1].1 - result[&3].1).abs() >= 80.0,
            "Nodes too close: {:?} {:?}",
            result[&1],
            result[&3]
        );
    }

    #[test]
    fn test_pinned_nodes() {
        // Pin the EQ at a specific position — it should stay there
        let nodes = vec![
            (1, "Mic".into(), "Source", 180.0, 80.0),
            (2, "EQ".into(), "Plugin", 180.0, 100.0),
            (3, "Speakers".into(), "Sink", 180.0, 80.0),
        ];
        let ports = vec![
            (10, 1, 0, true),
            (20, 2, 0, false),
            (21, 2, 0, true),
            (30, 3, 0, false),
        ];
        let links = vec![(100, 1, 10, 2, 20), (101, 2, 21, 3, 30)];
        let mut pinned = HashMap::new();
        pinned.insert(2u32, (500.0, 300.0));
        let result = sugiyama_layout(nodes, ports, links, LayoutConfig::default(), &pinned);

        assert_eq!(result.len(), 3);
        let eq = result[&2];
        assert_eq!(eq.0, 500.0, "Pinned EQ x should be exactly 500");
        assert_eq!(eq.1, 300.0, "Pinned EQ y should be exactly 300");
    }

    #[test]
    fn test_mixed_heights_no_overlap() {
        // Tall node followed by short node — must not overlap
        let nodes = vec![
            (1, "Tall".into(), "Source", 180.0, 200.0),
            (2, "Short".into(), "Source", 180.0, 60.0),
            (3, "Medium".into(), "Source", 180.0, 120.0),
        ];
        let no_pins = HashMap::new();
        let result = sugiyama_layout(nodes, vec![], vec![], LayoutConfig::default(), &no_pins);

        let mut entries: Vec<(u32, f64, f64)> = result
            .iter()
            .map(|(&id, &(_, y))| {
                let h = match id {
                    1 => 200.0,
                    2 => 60.0,
                    3 => 120.0,
                    _ => 80.0,
                };
                (id, y, h)
            })
            .collect();
        entries.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        for w in entries.windows(2) {
            let bottom_prev = w[0].1 + w[0].2;
            let top_next = w[1].1;
            assert!(
                top_next >= bottom_prev,
                "Node {} (y={}, h={}, bottom={}) overlaps with node {} (y={})",
                w[0].0,
                w[0].1,
                w[0].2,
                bottom_prev,
                w[1].0,
                w[1].1
            );
        }
    }

    #[test]
    fn test_real_scenario_pinned_plugins() {
        // Simulate: 3 pinned plugins in a chain, 1 free source, 1 free sink
        let nodes = vec![
            (1, "Mic".into(), "Source", 180.0, 80.0),
            (2, "EQ".into(), "Plugin", 180.0, 200.0),
            (3, "Comp".into(), "Plugin", 180.0, 180.0),
            (4, "Reverb".into(), "Plugin", 180.0, 160.0),
            (5, "Speakers".into(), "Sink", 180.0, 80.0),
        ];
        let ports = vec![
            (10, 1, 0, true),
            (20, 2, 0, false),
            (21, 2, 0, true),
            (30, 3, 0, false),
            (31, 3, 0, true),
            (40, 4, 0, false),
            (41, 4, 0, true),
            (50, 5, 0, false),
        ];
        let links = vec![
            (100, 1, 10, 2, 20),
            (101, 2, 21, 3, 30),
            (102, 3, 31, 4, 40),
            (103, 4, 41, 5, 50),
        ];
        let mut pinned = HashMap::new();
        pinned.insert(2, (300.0, 200.0));
        pinned.insert(3, (550.0, 200.0));
        pinned.insert(4, (800.0, 200.0));

        let result = sugiyama_layout(nodes, ports, links, LayoutConfig::default(), &pinned);
        let mic = result[&1];
        let eq = result[&2];
        let comp = result[&3];
        let rev = result[&4];
        let spk = result[&5];

        eprintln!("Mic(free):     ({:.0}, {:.0})", mic.0, mic.1);
        eprintln!("EQ(pinned):    ({:.0}, {:.0})", eq.0, eq.1);
        eprintln!("Comp(pinned):  ({:.0}, {:.0})", comp.0, comp.1);
        eprintln!("Reverb(pinned):({:.0}, {:.0})", rev.0, rev.1);
        eprintln!("Speakers(free):({:.0}, {:.0})", spk.0, spk.1);

        // Free nodes should be within ~200px y of pinned nodes at y=200
        assert!(
            (mic.1 - 200.0).abs() < 300.0,
            "Mic y={:.0} too far from pinned y=200",
            mic.1
        );
        assert!(
            (spk.1 - 200.0).abs() < 300.0,
            "Speakers y={:.0} too far from pinned y=200",
            spk.1
        );
    }

    #[test]
    fn test_pinned_plugins_scattered_y() {
        // Plugins manually placed at scattered y positions, not a neat row
        let nodes = vec![
            (1, "Mic".into(), "Source", 180.0, 80.0),
            (2, "EQ".into(), "Plugin", 180.0, 200.0),
            (3, "Comp".into(), "Plugin", 180.0, 180.0),
            (4, "Speakers".into(), "Sink", 180.0, 80.0),
        ];
        let ports = vec![
            (10, 1, 0, true),
            (20, 2, 0, false),
            (21, 2, 0, true),
            (30, 3, 0, false),
            (31, 3, 0, true),
            (40, 4, 0, false),
        ];
        let links = vec![
            (100, 1, 10, 2, 20),
            (101, 2, 21, 3, 30),
            (102, 3, 31, 4, 40),
        ];
        let mut pinned = HashMap::new();
        pinned.insert(2, (350.0, 100.0));
        pinned.insert(3, (600.0, 400.0));

        let result = sugiyama_layout(nodes, ports, links, LayoutConfig::default(), &pinned);
        let mic = result[&1];
        let eq = result[&2];
        let comp = result[&3];
        let spk = result[&4];

        eprintln!("Scattered pins:");
        eprintln!("  Mic(free):    ({:.0}, {:.0})", mic.0, mic.1);
        eprintln!("  EQ(pin@100):  ({:.0}, {:.0})", eq.0, eq.1);
        eprintln!("  Comp(pin@400):({:.0}, {:.0})", comp.0, comp.1);
        eprintln!("  Spk(free):    ({:.0}, {:.0})", spk.0, spk.1);

        assert!((eq.0, eq.1) == (350.0, 100.0));
        assert!((comp.0, comp.1) == (600.0, 400.0));
        // Mic should be near EQ's y (same rank neighborhood)
        assert!(
            (mic.1 - 100.0).abs() < 250.0,
            "Mic y={:.0} too far from EQ pinned at y=100",
            mic.1
        );
        // Speakers should be near Comp's y (same rank neighborhood)
        assert!(
            (spk.1 - 400.0).abs() < 250.0,
            "Speakers y={:.0} too far from Comp pinned at y=400",
            spk.1
        );
    }

    #[test]
    fn test_pinned_offset_keeps_free_nodes_nearby() {
        // Pin Speakers at (600, 200). Free nodes should end up near that y, not at y≈0.
        let nodes = vec![
            (1, "Firefox".into(), "StreamOutput", 180.0, 80.0),
            (2, "Spotify".into(), "StreamOutput", 180.0, 80.0),
            (3, "Speakers".into(), "Sink", 180.0, 80.0),
        ];
        let ports = vec![
            (10, 1, 0, true),
            (20, 2, 0, true),
            (30, 3, 0, false),
            (31, 3, 1, false),
        ];
        let links = vec![(100, 1, 10, 3, 30), (101, 2, 20, 3, 31)];
        let mut pinned = HashMap::new();
        pinned.insert(3u32, (600.0, 200.0));
        let result = sugiyama_layout(nodes, ports, links, LayoutConfig::default(), &pinned);

        let ff = result[&1];
        let sp = result[&2];
        let spk = result[&3];
        assert_eq!(spk, (600.0, 200.0));
        let max_dist = (ff.1 - spk.1).abs().max((sp.1 - spk.1).abs());
        assert!(
            max_dist < 300.0,
            "Free nodes too far from pinned: ff.y={}, sp.y={}, pinned.y=200, max_dist={}",
            ff.1,
            sp.1,
            max_dist
        );
    }

    type Node = (NodeId, String, &'static str, f64, f64);
    type Link = (u32, u32, u32, u32, u32);

    fn node(id: NodeId, kind: &'static str, h: f64) -> Node {
        (id, format!("n{}", id), kind, 180.0, h)
    }

    /// Links between nodes; port ids are derived from the link id.
    fn links(pairs: &[(u32, u32)]) -> Vec<Link> {
        pairs
            .iter()
            .enumerate()
            .map(|(i, &(a, b))| (1000 + i as u32, a, 10_000 + i as u32, b, 20_000 + i as u32))
            .collect()
    }

    fn ports_for(links: &[Link]) -> Vec<(u32, u32, usize, bool)> {
        links
            .iter()
            .flat_map(|l| [(l.2, l.1, 0, true), (l.4, l.3, 0, false)])
            .collect()
    }

    fn assert_no_overlap(nodes: &[Node], result: &LayoutResult) {
        for (i, a) in nodes.iter().enumerate() {
            for b in &nodes[i + 1..] {
                let (pa, pb) = (result[&a.0], result[&b.0]);
                assert!(pa.0.is_finite() && pa.1.is_finite(), "non-finite {:?}", pa);
                let x_overlap = pa.0 < pb.0 + b.3 && pb.0 < pa.0 + a.3;
                let y_overlap = pa.1 < pb.1 + b.4 && pb.1 < pa.1 + a.4;
                assert!(
                    !(x_overlap && y_overlap),
                    "node {} at {:?} overlaps node {} at {:?}",
                    a.0,
                    pa,
                    b.0,
                    pb
                );
            }
        }
    }

    fn run(nodes: &[Node], links: &[Link], pinned: &HashMap<NodeId, (f64, f64)>) -> LayoutResult {
        sugiyama_layout(
            nodes.to_vec(),
            ports_for(links),
            links.to_vec(),
            LayoutConfig::default(),
            pinned,
        )
    }

    #[test]
    fn test_cycle_and_self_loop() {
        // S → A → B → C → A (cycle), B → B (self-loop), C → T
        let nodes = vec![
            node(1, "Source", 80.0),
            node(2, "Plugin", 100.0),
            node(3, "Plugin", 120.0),
            node(4, "Plugin", 90.0),
            node(5, "Sink", 80.0),
        ];
        let l = links(&[(1, 2), (2, 3), (3, 4), (4, 2), (3, 3), (4, 5)]);
        let result = run(&nodes, &l, &HashMap::new());
        assert_eq!(result.len(), 5);
        let xs: Vec<f64> = (1..=5).map(|id| result[&id].0).collect();
        for w in xs.windows(2) {
            assert!(w[0] < w[1], "cycle nodes should spread across columns: {:?}", xs);
        }
        assert_no_overlap(&nodes, &result);
    }

    #[test]
    fn test_pure_cycle_no_panic() {
        let nodes = vec![node(1, "Plugin", 80.0), node(2, "Plugin", 80.0), node(3, "Plugin", 80.0)];
        let l = links(&[(1, 2), (2, 3), (3, 1), (1, 1)]);
        let result = run(&nodes, &l, &HashMap::new());
        let mut xs: Vec<f64> = result.values().map(|p| p.0).collect();
        xs.sort_by(f64::total_cmp);
        xs.dedup();
        assert_eq!(xs.len(), 3, "each cycle node gets its own column");
    }

    fn sample_graph() -> (Vec<Node>, Vec<Link>) {
        let nodes = vec![
            node(1, "Source", 80.0),
            node(2, "StreamOutput", 60.0),
            node(3, "Plugin", 140.0),
            node(4, "Plugin", 100.0),
            node(5, "Plugin", 120.0),
            node(6, "Sink", 80.0),
            node(7, "StreamInput", 80.0),
            node(8, "Duplex", 200.0),
            node(9, "Sink", 80.0),
        ];
        let l = links(&[
            (1, 3),
            (3, 4),
            (4, 5),
            (5, 6),
            (2, 6),
            (2, 4),
            (1, 7),
            (8, 5),
            (3, 6),
            (1, 6),
            (8, 9),
            (5, 3),
        ]);
        (nodes, l)
    }

    #[test]
    fn test_deterministic_under_shuffled_input() {
        let (nodes, l) = sample_graph();
        let reference = run(&nodes, &l, &HashMap::new());
        for shift in 1..nodes.len() {
            let mut n2 = nodes.clone();
            n2.rotate_left(shift);
            let mut l2 = l.clone();
            l2.rotate_left(shift % l.len());
            l2.reverse();
            let mut p2 = ports_for(&l2);
            p2.reverse();
            let result = sugiyama_layout(n2, p2, l2, LayoutConfig::default(), &HashMap::new());
            assert_eq!(result, reference, "layout changed with input order (shift {})", shift);
        }
    }

    #[test]
    fn test_mixed_heights_connected_no_overlap() {
        let (nodes, l) = sample_graph();
        let result = run(&nodes, &l, &HashMap::new());
        assert_eq!(result.len(), nodes.len());
        assert_no_overlap(&nodes, &result);
    }

    #[test]
    fn test_long_edges_crossing_column() {
        // Two long edges (1→5, 2→5) and (1→4) cross the columns of the chain
        // 1 → 3 → 6 → 4 → 5, plus a short side branch.
        let nodes = vec![
            node(1, "Source", 80.0),
            node(2, "Source", 120.0),
            node(3, "Plugin", 100.0),
            node(6, "Plugin", 160.0),
            node(4, "Plugin", 90.0),
            node(5, "Sink", 80.0),
            node(7, "Plugin", 70.0),
        ];
        let l = links(&[(1, 3), (3, 6), (6, 4), (4, 5), (1, 5), (2, 5), (1, 4), (2, 7), (7, 6)]);
        let result = run(&nodes, &l, &HashMap::new());
        assert_eq!(result.len(), nodes.len());
        assert!(result[&1].0 < result[&3].0);
        assert!(result[&3].0 < result[&6].0);
        assert!(result[&6].0 < result[&4].0);
        assert!(result[&4].0 < result[&5].0);
        assert_no_overlap(&nodes, &result);
    }

    #[test]
    fn test_random_graphs_no_overlap() {
        let kinds = ["Source", "Sink", "StreamOutput", "StreamInput", "Duplex", "Plugin"];
        let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
        let mut rnd = move |m: u64| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) % m
        };
        for _ in 0..30 {
            let count = 2 + rnd(40) as u32;
            let nodes: Vec<Node> = (1..=count)
                .map(|id| node(id, kinds[rnd(kinds.len() as u64) as usize], 40.0 + rnd(200) as f64))
                .collect();
            let pairs: Vec<(u32, u32)> = (0..rnd(count as u64 * 2))
                .map(|_| (1 + rnd(count as u64) as u32, 1 + rnd(count as u64) as u32))
                .collect();
            let l = links(&pairs);
            let result = run(&nodes, &l, &HashMap::new());
            assert_eq!(result.len(), nodes.len());
            assert_no_overlap(&nodes, &result);
        }
    }

    #[test]
    fn test_pinned_and_free_never_overlap() {
        let (mut nodes, l) = sample_graph();
        nodes.push(node(50, "Plugin", 150.0)); // isolated obstacle, pinned below
        let base: HashMap<NodeId, (f64, f64)> =
            [(3u32, (400.0, 100.0)), (6u32, (900.0, 300.0))].into_iter().collect();
        let mut x = -200.0;
        while x <= 1400.0 {
            let mut y = -300.0;
            while y <= 700.0 {
                let mut pinned = base.clone();
                pinned.insert(50, (x, y));
                let result = run(&nodes, &l, &pinned);
                assert_eq!(result[&50], (x, y));
                assert_eq!(result[&3], (400.0, 100.0));
                // Pinned nodes may overlap each other (user's choice), but free
                // nodes must not overlap anything.
                let free: Vec<&Node> = nodes.iter().filter(|n| !pinned.contains_key(&n.0)).collect();
                for a in &free {
                    for b in &nodes {
                        if a.0 == b.0 {
                            continue;
                        }
                        let (pa, pb) = (result[&a.0], result[&b.0]);
                        let x_overlap = pa.0 < pb.0 + b.3 && pb.0 < pa.0 + a.3;
                        let y_overlap = pa.1 < pb.1 + b.4 && pb.1 < pa.1 + a.4;
                        assert!(
                            !(x_overlap && y_overlap),
                            "obstacle at ({}, {}): free node {} {:?} overlaps node {} {:?}",
                            x,
                            y,
                            a.0,
                            pa,
                            b.0,
                            pb
                        );
                    }
                }
                y += 50.0;
            }
            x += 100.0;
        }
    }

    #[test]
    fn snap_to_grid_rounds_and_keeps_gaps() {
        let mut pos: LayoutResult = HashMap::new();
        pos.insert(1, (13.0, 11.0));
        pos.insert(2, (13.0, 70.0)); // would overlap node 1 after rounding
        pos.insert(3, (500.0, 500.0));
        let sizes: HashMap<NodeId, (f64, f64)> =
            [(1, (100.0, 50.0)), (2, (100.0, 50.0)), (3, (100.0, 50.0))].into_iter().collect();
        let pinned: HashMap<NodeId, (f64, f64)> = [(3, (500.0, 500.0))].into_iter().collect();
        snap_to_grid(&mut pos, &sizes, &pinned, 24.0, 10.0);
        let (x1, y1) = pos[&1];
        let (x2, y2) = pos[&2];
        for v in [x1, y1, x2, y2] {
            assert_eq!(v % 24.0, 0.0, "{v} is not on the grid");
        }
        assert!(y2 >= y1 + 50.0 + 10.0, "node 2 must keep the gap below node 1");
        assert_eq!(pos[&3], (500.0, 500.0), "pinned nodes don't move");
    }

    #[test]
    fn snap_to_grid_off_is_a_no_op() {
        let mut pos: LayoutResult = [(1, (13.0, 11.0))].into_iter().collect();
        snap_to_grid(&mut pos, &HashMap::new(), &HashMap::new(), 0.0, 10.0);
        assert_eq!(pos[&1], (13.0, 11.0));
    }
}
