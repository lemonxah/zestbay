// SPDX-License-Identifier: MIT

use super::graph::LayoutGraph;

pub fn wrap_columns(graph: &mut LayoutGraph) {
    let max = graph.config.max_column_nodes;
    if max == 0 {
        return;
    }
    let gap_x = graph.config.margin_y.max(graph.config.margin_x * 0.4);
    let gap_y = graph.config.margin_y;
    let mut shift = 0.0;

    for col_idx in 0..graph.columns.len() {
        let col = graph.columns[col_idx].clone();
        for &ni in &col {
            graph.nodes[ni].x += shift;
        }
        let mut real: Vec<usize> = col
            .iter()
            .copied()
            .filter(|&ni| !graph.nodes[ni].is_dummy)
            .collect();
        if real.len() <= max {
            continue;
        }
        real.sort_by(|&a, &b| {
            graph.nodes[a]
                .y
                .total_cmp(&graph.nodes[b].y)
                .then(a.cmp(&b))
        });

        let n = real.len();
        let per_row = n.div_ceil(max);
        let rows = n.div_ceil(per_row);
        let width = real
            .iter()
            .map(|&ni| graph.nodes[ni].width)
            .fold(0.0_f64, f64::max);
        let left = real
            .iter()
            .map(|&ni| graph.nodes[ni].x - (width - graph.nodes[ni].width) / 2.0)
            .fold(f64::INFINITY, f64::min);
        let top = real
            .iter()
            .map(|&ni| graph.nodes[ni].y)
            .fold(f64::INFINITY, f64::min);
        let bottom = real
            .iter()
            .map(|&ni| graph.nodes[ni].y + graph.nodes[ni].height)
            .fold(f64::NEG_INFINITY, f64::max);

        let row_heights: Vec<f64> = (0..rows)
            .map(|r| {
                real[r * per_row..((r + 1) * per_row).min(n)]
                    .iter()
                    .map(|&ni| graph.nodes[ni].height)
                    .fold(0.0_f64, f64::max)
            })
            .collect();
        let total: f64 = row_heights.iter().sum::<f64>() + gap_y * (rows as f64 - 1.0);
        let mut row_top = (top + bottom) / 2.0 - total / 2.0;

        for (r, &row_h) in row_heights.iter().enumerate() {
            for (c, &ni) in real[r * per_row..((r + 1) * per_row).min(n)].iter().enumerate() {
                let node = &mut graph.nodes[ni];
                node.x = left + c as f64 * (width + gap_x) + (width - node.width) / 2.0;
                node.y = row_top;
            }
            row_top += row_h + gap_y;
        }

        shift += (per_row as f64 - 1.0) * (width + gap_x);
    }
}

#[cfg(test)]
mod tests {
    use super::super::graph::LayoutConfig;
    use super::super::sugiyama_layout;
    use std::collections::{HashMap, HashSet};

    fn fan_in(sources: u32) -> (
        Vec<(u32, String, &'static str, f64, f64)>,
        Vec<(u32, u32, usize, bool)>,
        Vec<(u32, u32, u32, u32, u32)>,
    ) {
        let mut nodes = vec![(1000, "Speakers".to_string(), "Sink", 180.0, 80.0)];
        let mut ports = Vec::new();
        let mut links = Vec::new();
        for i in 0..sources {
            let id = i + 1;
            nodes.push((id, format!("App {id}"), "StreamOutput", 180.0, 60.0 + (i % 3) as f64 * 20.0));
            ports.push((id * 10, id, 0, true));
            ports.push((5000 + id, 1000, i as usize, false));
            links.push((9000 + id, id, id * 10, 1000, 5000 + id));
        }
        (nodes, ports, links)
    }

    fn overlaps(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
        a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
    }

    #[test]
    fn off_leaves_one_tall_column() {
        let (nodes, ports, links) = fan_in(10);
        let result = sugiyama_layout(nodes, ports, links, LayoutConfig::default(), &HashMap::new());
        let xs: HashSet<i64> = (1..=10).map(|id| result[&id].0.round() as i64).collect();
        assert_eq!(xs.len(), 1);
    }

    #[test]
    fn tall_column_wraps_into_a_grid_without_overlap() {
        let (nodes, ports, links) = fan_in(10);
        let sizes: HashMap<u32, (f64, f64)> = nodes.iter().map(|n| (n.0, (n.3, n.4))).collect();
        let config = LayoutConfig {
            max_column_nodes: 4,
            ..Default::default()
        };
        let result = sugiyama_layout(nodes, ports, links, config, &HashMap::new());

        let mut per_x: HashMap<i64, usize> = HashMap::new();
        for id in 1..=10 {
            *per_x.entry(result[&id].0.round() as i64).or_default() += 1;
        }
        assert!(per_x.len() >= 3);
        assert!(per_x.values().all(|&count| count <= 4));

        let speakers = result[&1000];
        for id in 1..=10 {
            assert!(result[&id].0 + 180.0 < speakers.0);
        }

        let rects: Vec<(f64, f64, f64, f64)> = result
            .iter()
            .map(|(id, &(x, y))| (x, y, sizes[id].0, sizes[id].1))
            .collect();
        for i in 0..rects.len() {
            for j in i + 1..rects.len() {
                assert!(!overlaps(rects[i], rects[j]), "{:?} overlaps {:?}", rects[i], rects[j]);
            }
        }
    }

    #[test]
    fn short_columns_are_untouched() {
        let (nodes, ports, links) = fan_in(3);
        let plain = sugiyama_layout(
            nodes.clone(),
            ports.clone(),
            links.clone(),
            LayoutConfig::default(),
            &HashMap::new(),
        );
        let config = LayoutConfig {
            max_column_nodes: 4,
            ..Default::default()
        };
        let wrapped = sugiyama_layout(nodes, ports, links, config, &HashMap::new());
        assert_eq!(plain, wrapped);
    }
}
