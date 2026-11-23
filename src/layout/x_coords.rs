use super::graph::LayoutGraph;

const MAX_EXTRA_MARGINS: f64 = 2.0;
const HALF_WIDENING_DETOUR: f64 = 12.0;

pub fn assign_x_coords(graph: &mut LayoutGraph) {
    let margin = graph.config.margin_x.max(0.0);
    let columns = std::mem::take(&mut graph.columns);
    let mut left = 0.0;

    for column in columns.iter().filter(|column| !column.is_empty()) {
        let width = column
            .iter()
            .map(|&v| graph.nodes[v].width)
            .fold(0.0_f64, f64::max);
        for &v in column {
            let node = &mut graph.nodes[v];
            node.x = left + (width - node.width) / 2.0;
        }
        left += width + gap_after(graph, column, margin);
    }

    graph.columns = columns;
}

fn gap_after(graph: &LayoutGraph, column: &[usize], margin: f64) -> f64 {
    if margin <= 0.0 {
        return 0.0;
    }
    let total_detour: f64 = column
        .iter()
        .flat_map(|&v| &graph.out_edges[v])
        .map(|&ei| {
            let edge = &graph.edges[ei];
            let rise = middle_y(graph, edge.to_node) - middle_y(graph, edge.from_node);
            detour_in_margins(rise, margin)
        })
        .sum();
    let crowding = 1.0 - HALF_WIDENING_DETOUR / (total_detour + HALF_WIDENING_DETOUR);
    margin * (1.0 + MAX_EXTRA_MARGINS * crowding)
}

fn middle_y(graph: &LayoutGraph, v: usize) -> f64 {
    let node = &graph.nodes[v];
    node.y + node.height / 2.0
}

fn detour_in_margins(rise: f64, margin: f64) -> f64 {
    (rise / margin).hypot(1.0) - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::graph::LayoutConfig;

    const EPS: f64 = 1e-6;

    struct Spec {
        column: usize,
        width: f64,
        height: f64,
        middle: f64,
    }

    fn spec(column: usize, width: f64, height: f64, middle: f64) -> Spec {
        Spec {
            column,
            width,
            height,
            middle,
        }
    }

    fn placed(specs: &[Spec], wires: &[(usize, usize)], margin_x: f64) -> LayoutGraph {
        let nodes = specs
            .iter()
            .enumerate()
            .map(|(i, s)| (i as u32, String::new(), "Plugin", s.width, s.height))
            .collect();
        let links = wires
            .iter()
            .enumerate()
            .map(|(i, &(a, b))| (i as u32, a as u32, 0, b as u32, 0))
            .collect();
        let config = LayoutConfig {
            margin_x,
            ..Default::default()
        };
        let mut g = LayoutGraph::new(nodes, vec![], links, config);
        let column_count = specs.iter().map(|s| s.column + 1).max().unwrap_or(0);
        g.columns = vec![Vec::new(); column_count];
        for (i, s) in specs.iter().enumerate() {
            g.nodes[i].rank = s.column as i32;
            g.nodes[i].order = g.columns[s.column].len();
            g.nodes[i].y = s.middle - s.height / 2.0;
            g.columns[s.column].push(i);
        }
        assign_x_coords(&mut g);
        g
    }

    fn gap_for_rises(rises: &[f64], margin_x: f64) -> f64 {
        let mut specs = Vec::new();
        let mut wires = Vec::new();
        for &rise in rises {
            specs.push(spec(0, 100.0, 40.0, 0.0));
            specs.push(spec(1, 100.0, 40.0, rise));
            wires.push((specs.len() - 2, specs.len() - 1));
        }
        let g = placed(&specs, &wires, margin_x);
        g.nodes[1].x - 100.0
    }

    #[test]
    fn empty_graph_and_empty_columns_do_not_panic() {
        let g = placed(&[], &[], 50.0);
        assert!(g.columns.is_empty());

        let mut g = LayoutGraph::new(vec![], vec![], vec![], LayoutConfig::default());
        g.columns = vec![Vec::new(), Vec::new()];
        assign_x_coords(&mut g);
        assert_eq!(g.columns.len(), 2);
    }

    #[test]
    fn empty_columns_take_no_space() {
        let g = placed(
            &[spec(1, 100.0, 40.0, 0.0), spec(4, 80.0, 40.0, 0.0)],
            &[],
            50.0,
        );
        assert_eq!(g.nodes[0].x, 0.0);
        assert_eq!(g.nodes[1].x, 150.0);
        assert_eq!(g.columns, vec![vec![], vec![0], vec![], vec![], vec![1]]);
    }

    #[test]
    fn nodes_are_centred_in_their_column() {
        let mut g = placed(
            &[
                spec(0, 200.0, 40.0, 0.0),
                spec(0, 120.0, 40.0, 100.0),
                spec(1, 60.0, 40.0, 0.0),
                spec(1, 180.0, 40.0, 100.0),
            ],
            &[],
            50.0,
        );
        assert_eq!(g.nodes[0].x, 0.0);
        assert_eq!(g.nodes[1].x, 40.0);
        assert_eq!(g.nodes[2].x, 310.0);
        assert_eq!(g.nodes[3].x, 250.0);

        let dummy = g.add_dummy_node(1);
        g.columns[1].push(dummy);
        assign_x_coords(&mut g);
        assert_eq!(g.nodes[dummy].x, 340.0);
    }

    #[test]
    fn level_wires_leave_the_margin_alone() {
        assert_eq!(gap_for_rises(&[0.0; 30], 50.0), 50.0);
        let nearly_level = gap_for_rises(&[2.0, -2.0, 1.0], 50.0);
        assert!((50.0..50.5).contains(&nearly_level));
    }

    #[test]
    fn wires_are_measured_between_node_middles() {
        let g = placed(
            &[spec(0, 100.0, 400.0, 200.0), spec(1, 100.0, 40.0, 200.0)],
            &[(0, 1)],
            50.0,
        );
        assert_eq!(g.nodes[1].x, 150.0);
    }

    #[test]
    fn gap_grows_with_distance_and_with_wire_count() {
        let mut previous = 50.0;
        for rise in [50.0, 100.0, 200.0, 400.0, 800.0] {
            let gap = gap_for_rises(&[rise], 50.0);
            assert!(gap > previous);
            assert!((gap - gap_for_rises(&[-rise], 50.0)).abs() < EPS);
            previous = gap;
        }

        let mut previous = 50.0;
        for count in [1, 2, 4, 8, 16] {
            let gap = gap_for_rises(&vec![200.0; count], 50.0);
            assert!(gap > previous);
            previous = gap;
        }
    }

    #[test]
    fn gap_changes_smoothly() {
        for step in 0..2000 {
            let rise = step as f64;
            let change = gap_for_rises(&[rise + 1.0], 50.0) - gap_for_rises(&[rise], 50.0);
            assert!((0.0..0.25).contains(&change));
        }
    }

    #[test]
    fn widening_is_bounded_and_scales_with_the_margin() {
        for margin in [0.5, 10.0, 50.0, 200.0] {
            let gap = gap_for_rises(&vec![1e7; 500], margin);
            assert!(gap.is_finite());
            assert!(gap > 2.5 * margin);
            assert!(gap <= (1.0 + MAX_EXTRA_MARGINS) * margin + EPS);
        }
        let small = gap_for_rises(&[40.0, 80.0], 10.0);
        let large = gap_for_rises(&[200.0, 400.0], 50.0);
        assert!((small * 5.0 - large).abs() < EPS);
    }

    #[test]
    fn extreme_distances_stay_finite() {
        let gap = gap_for_rises(&[1e308, 1e308, -1e308], 0.1);
        assert!((gap - 0.1 * (1.0 + MAX_EXTRA_MARGINS)).abs() < EPS);
    }

    #[test]
    fn zero_margin_packs_columns_tightly() {
        assert_eq!(gap_for_rises(&[0.0, 500.0, -900.0], 0.0), 0.0);
    }

    #[test]
    fn only_the_busy_gap_is_widened() {
        let g = placed(
            &[
                spec(0, 100.0, 40.0, 0.0),
                spec(1, 100.0, 40.0, 0.0),
                spec(2, 100.0, 40.0, 600.0),
                spec(3, 100.0, 40.0, 600.0),
            ],
            &[(0, 1), (1, 2), (2, 3)],
            50.0,
        );
        let gap = |v: usize| g.nodes[v].x - g.nodes[v - 1].x - 100.0;
        assert_eq!(gap(1), 50.0);
        assert!(gap(2) > 55.0);
        assert!((gap(3) - 50.0).abs() < EPS);
    }

    #[test]
    fn columns_keep_their_distance_and_results_repeat() {
        for margin in [0.0, 10.0, 50.0] {
            let build = || {
                let mut specs = Vec::new();
                let mut wires = Vec::new();
                for i in 0..120usize {
                    let column = i / 12;
                    let width = 60.0 + ((i * 37) % 11) as f64 * 14.3;
                    let height = 30.0 + ((i * 53) % 7) as f64 * 21.7;
                    specs.push(spec(column, width, height, (i % 12) as f64 * 190.0));
                    if column > 0 {
                        wires.push(((column - 1) * 12 + (i * 5) % 12, i));
                        wires.push(((column - 1) * 12 + (i * 7 + 3) % 12, i));
                    }
                }
                placed(&specs, &wires, margin)
            };
            let g = build();
            for pair in g.columns.windows(2) {
                let right = pair[0]
                    .iter()
                    .map(|&v| g.nodes[v].x + g.nodes[v].width)
                    .fold(f64::NEG_INFINITY, f64::max);
                for &v in &pair[1] {
                    assert!(g.nodes[v].x.is_finite());
                    assert!(g.nodes[v].x >= right + margin - EPS);
                }
            }
            let xs = |g: &LayoutGraph| g.nodes.iter().map(|n| n.x).collect::<Vec<f64>>();
            assert_eq!(xs(&g), xs(&build()));
        }
    }
}
