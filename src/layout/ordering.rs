use super::graph::LayoutGraph;

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

        for col_idx in 0..(num_cols - 1) {
            let old_cross = count_crossings_between(graph, col_idx, col_idx + 1);
            reorder_by_barycenter(graph, col_idx, col_idx + 1, true);
            let new_cross = count_crossings_between(graph, col_idx, col_idx + 1);
            if new_cross > old_cross {
                graph.columns[col_idx + 1] = best_columns[col_idx + 1].clone();
                update_orders(graph, col_idx + 1);
            }
        }

        for col_idx in (1..num_cols).rev() {
            let old_cross = count_crossings_between(graph, col_idx - 1, col_idx);
            reorder_by_barycenter(graph, col_idx, col_idx - 1, false);
            let new_cross = count_crossings_between(graph, col_idx - 1, col_idx);
            if new_cross > old_cross {
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

    graph.columns = best_columns;
    for col_idx in 0..graph.columns.len() {
        update_orders(graph, col_idx);
    }
}

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

    let mut barycenters: Vec<(usize, f64)> = Vec::new();

    for &node_idx in &free_col {
        let neighbors: Vec<usize> = if forward {
            graph.predecessors(node_idx)
        } else {
            graph.successors(node_idx)
        };

        let fixed_set: Vec<usize> = graph.columns[fixed_col_idx].clone();
        let positions: Vec<f64> = neighbors
            .iter()
            .filter(|n| fixed_set.contains(n))
            .map(|&n| graph.nodes[n].order as f64)
            .collect();

        let barycenter = if positions.is_empty() {
            graph.nodes[node_idx].order as f64
        } else {
            positions.iter().sum::<f64>() / positions.len() as f64
        };

        barycenters.push((node_idx, barycenter));
    }

    barycenters.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let new_col: Vec<usize> = barycenters.iter().map(|(idx, _)| *idx).collect();
    graph.columns[free_col_idx] = new_col;
    update_orders(graph, free_col_idx);
}

fn update_orders(graph: &mut LayoutGraph, col_idx: usize) {
    for (order, &node_idx) in graph.columns[col_idx].iter().enumerate() {
        graph.nodes[node_idx].order = order;
    }
}

fn count_all_crossings(graph: &LayoutGraph) -> usize {
    let mut total = 0;
    for col_idx in 0..(graph.columns.len().saturating_sub(1)) {
        total += count_crossings_between(graph, col_idx, col_idx + 1);
    }
    total
}

fn count_crossings_between(graph: &LayoutGraph, left_col: usize, right_col: usize) -> usize {
    let left_nodes: &[usize] = graph.column(left_col);
    let right_nodes: &[usize] = graph.column(right_col);
    if left_nodes.is_empty() || right_nodes.is_empty() {
        return 0;
    }

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

    edge_pairs.sort();

    let max_right = right_nodes.len();
    count_inversions(&edge_pairs, max_right)
}

fn count_inversions(edges: &[(usize, usize)], max_pos: usize) -> usize {
    let tree_size = max_pos + 1;
    let mut tree = vec![0usize; tree_size + 1];
    let mut crossings = 0;

    for &(_left, right) in edges {
        crossings += query(&tree, tree_size) - query(&tree, right + 1);
        update(&mut tree, right + 1, 1, tree_size);
    }

    crossings
}

fn query(tree: &[usize], mut pos: usize) -> usize {
    let mut sum = 0;
    while pos > 0 {
        sum += tree[pos];
        pos -= pos & pos.wrapping_neg();
    }
    sum
}

fn update(tree: &mut [usize], mut pos: usize, val: usize, max_pos: usize) {
    while pos <= max_pos {
        tree[pos] += val;
        pos += pos & pos.wrapping_neg();
    }
}
