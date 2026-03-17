// SPDX-License-Identifier: MIT
//
// Sugiyama-style layered graph layout algorithm.
//
// This module implements the classic Sugiyama framework for hierarchical
// graph drawing, adapted from the node-arrange Blender addon:
// https://github.com/Leonardo-Pike-Excell/node-arrange
//
// The pipeline consists of 5 phases:
//   1. Ranking: Assign each node to a column/layer (network simplex)
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
    nodes: Vec<(NodeId, String, &str, f64, f64)>,
    ports: Vec<(u32, u32, usize, bool)>,
    links: Vec<(u32, u32, u32, u32, u32)>,
    config: LayoutConfig,
    pinned: &HashMap<NodeId, (f64, f64)>,
) -> LayoutResult {
    if nodes.is_empty() {
        return HashMap::new();
    }

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

    // For each pinned node, store the delta between actual and computed position.
    let mut pin_deltas: HashMap<usize, (f64, f64)> = HashMap::new();
    let (mut global_dx, mut global_dy, mut pin_count) = (0.0, 0.0, 0u32);
    for node in &graph.nodes {
        if node.is_dummy || node.id == 0 {
            continue;
        }
        if let Some(&(px, py)) = pinned.get(&node.id) {
            pin_deltas.insert(node.idx, (px - node.x, py - node.y));
            global_dx += px - node.x;
            global_dy += py - node.y;
            pin_count += 1;
        }
    }
    if pin_count > 0 {
        global_dx /= pin_count as f64;
        global_dy /= pin_count as f64;
    }

    // BFS from each free node to find the nearest pinned node's delta.
    let nearest_pin_offset = |start_idx: usize| -> (f64, f64) {
        if pin_deltas.is_empty() {
            return (0.0, 0.0);
        }
        let mut visited = vec![false; graph.nodes.len()];
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(start_idx);
        visited[start_idx] = true;
        while let Some(v) = queue.pop_front() {
            if let Some(&delta) = pin_deltas.get(&v) {
                return delta;
            }
            for &ei in &graph.out_edges[v] {
                let w = graph.edges[ei].to_node;
                if !visited[w] {
                    visited[w] = true;
                    queue.push_back(w);
                }
            }
            for &ei in &graph.in_edges[v] {
                let w = graph.edges[ei].from_node;
                if !visited[w] {
                    visited[w] = true;
                    queue.push_back(w);
                }
            }
        }
        (global_dx, global_dy)
    };

    let mut result = HashMap::new();
    for node in &graph.nodes {
        if node.is_dummy || node.id == 0 {
            continue;
        }
        if let Some(&pos) = pinned.get(&node.id) {
            result.insert(node.id, pos);
        } else {
            let (dx, dy) = nearest_pin_offset(node.idx);
            result.insert(node.id, (node.x + dx, node.y + dy));
        }
    }

    log::debug!("Layout: complete, {} positions", result.len());
    result
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
        let mut ys: Vec<f64> = result.values().map(|p| p.1).collect();
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(
            ys[1] - ys[0] >= 80.0,
            "Nodes too close: y[0]={}, y[1]={}",
            ys[0],
            ys[1]
        );
        assert!(
            ys[2] - ys[1] >= 80.0,
            "Nodes too close: y[1]={}, y[2]={}",
            ys[1],
            ys[2]
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
}
