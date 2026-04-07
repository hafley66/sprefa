/// Dependency graph for cross-rule references.
///
/// Cross-rule refs (`deploy_config(repo: $REPO, pin: $PIN)` inside another
/// rule's body) create dependency edges. These edges form a DAG that controls
/// discovery ordering: level 0 rules have no deps, level 1 rules depend on
/// level 0, etc. Cycles are a hard error.
use anyhow::{bail, Result};
use std::collections::{HashMap, HashSet, VecDeque};

/// One dependency edge between two rules.
#[derive(Debug, Clone)]
pub struct DepEdge {
    /// Rule whose table is read.
    pub producer: String,
    /// Rule that reads from the producer.
    pub consumer: String,
    /// (producer_column, consumer_variable) bindings.
    pub bindings: Vec<(String, String)>,
}

/// Rules grouped into dependency levels for ordered discovery.
#[derive(Debug, Clone)]
pub struct RuleGraph {
    /// Rules grouped by dependency level. Level 0 = no cross-ref deps.
    /// Each inner vec contains rule names at that level.
    pub levels: Vec<Vec<String>>,
    /// All dependency edges.
    pub edges: Vec<DepEdge>,
}

/// Build a dependency graph from rule names and cross-ref edges.
///
/// Uses Kahn's algorithm for topological sort into levels.
/// Returns error if a cycle is detected (with the cycle path in the message).
pub fn build_rule_graph(rule_names: &[String], edges: Vec<DepEdge>) -> Result<RuleGraph> {
    if edges.is_empty() {
        return Ok(RuleGraph {
            levels: vec![rule_names.to_vec()],
            edges,
        });
    }

    // Build adjacency list and in-degree map.
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();

    for name in rule_names {
        in_degree.entry(name.as_str()).or_insert(0);
    }

    for edge in &edges {
        // Validate that producer exists in rule_names.
        if !rule_names.iter().any(|n| n == &edge.producer) {
            bail!(
                "cross-ref in rule '{}' references unknown rule '{}'",
                edge.consumer,
                edge.producer,
            );
        }
        *in_degree.entry(edge.consumer.as_str()).or_insert(0) += 1;
        dependents
            .entry(edge.producer.as_str())
            .or_default()
            .push(edge.consumer.as_str());
    }

    // Kahn's algorithm: peel off nodes with in_degree 0 in waves (= levels).
    let mut levels: Vec<Vec<String>> = vec![];
    let mut queue: VecDeque<&str> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&name, _)| name)
        .collect();

    let mut visited = 0usize;

    while !queue.is_empty() {
        let level: Vec<&str> = queue.drain(..).collect();
        visited += level.len();

        for &node in &level {
            if let Some(deps) = dependents.get(node) {
                for &dep in deps {
                    let deg = in_degree.get_mut(dep).unwrap();
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(dep);
                    }
                }
            }
        }

        levels.push(level.into_iter().map(String::from).collect());
    }

    if visited < rule_names.len() {
        // Cycle detected. Find the cycle path for diagnostics.
        let in_cycle: Vec<&str> = in_degree
            .iter()
            .filter(|(_, &deg)| deg > 0)
            .map(|(&name, _)| name)
            .collect();
        let path = find_cycle_path(&in_cycle, &edges);
        bail!("dependency cycle detected: {}", path);
    }

    Ok(RuleGraph { levels, edges })
}

/// Find a cycle path for error reporting. Returns "A -> B -> C -> A".
fn find_cycle_path(in_cycle: &[&str], edges: &[DepEdge]) -> String {
    if in_cycle.is_empty() {
        return String::new();
    }

    // Build adjacency restricted to cycle members.
    let cycle_set: HashSet<&str> = in_cycle.iter().copied().collect();
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in edges {
        if cycle_set.contains(edge.producer.as_str())
            && cycle_set.contains(edge.consumer.as_str())
        {
            adj.entry(edge.producer.as_str())
                .or_default()
                .push(edge.consumer.as_str());
        }
    }

    // DFS from first node to find a cycle.
    let start = in_cycle[0];
    let mut stack = vec![start];
    let mut visited: HashSet<&str> = HashSet::new();
    let mut path: Vec<&str> = vec![];

    fn dfs<'a>(
        node: &'a str,
        adj: &HashMap<&'a str, Vec<&'a str>>,
        visited: &mut HashSet<&'a str>,
        path: &mut Vec<&'a str>,
    ) -> Option<Vec<String>> {
        if let Some(pos) = path.iter().position(|&n| n == node) {
            let mut cycle: Vec<String> = path[pos..].iter().map(|s| s.to_string()).collect();
            cycle.push(node.to_string());
            return Some(cycle);
        }
        if visited.contains(node) {
            return None;
        }
        visited.insert(node);
        path.push(node);
        if let Some(neighbors) = adj.get(node) {
            for &next in neighbors {
                if let Some(cycle) = dfs(next, adj, visited, path) {
                    return Some(cycle);
                }
            }
        }
        path.pop();
        None
    }

    if let Some(cycle) = dfs(start, &adj, &mut visited, &mut path) {
        cycle.join(" -> ")
    } else {
        in_cycle.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(producer: &str, consumer: &str) -> DepEdge {
        DepEdge {
            producer: producer.to_string(),
            consumer: consumer.to_string(),
            bindings: vec![],
        }
    }

    fn names(ns: &[&str]) -> Vec<String> {
        ns.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_edges_single_level() {
        let graph = build_rule_graph(&names(&["a", "b", "c"]), vec![]).unwrap();
        assert_eq!(graph.levels.len(), 1);
        assert_eq!(graph.levels[0].len(), 3);
    }

    #[test]
    fn linear_chain() {
        // A -> B -> C
        let graph = build_rule_graph(
            &names(&["a", "b", "c"]),
            vec![edge("a", "b"), edge("b", "c")],
        )
        .unwrap();
        assert_eq!(graph.levels.len(), 3);
        assert_eq!(graph.levels[0], vec!["a"]);
        assert_eq!(graph.levels[1], vec!["b"]);
        assert_eq!(graph.levels[2], vec!["c"]);
    }

    #[test]
    fn diamond() {
        // A -> B, A -> C, B -> D, C -> D
        let graph = build_rule_graph(
            &names(&["a", "b", "c", "d"]),
            vec![
                edge("a", "b"),
                edge("a", "c"),
                edge("b", "d"),
                edge("c", "d"),
            ],
        )
        .unwrap();
        assert_eq!(graph.levels.len(), 3);
        assert_eq!(graph.levels[0], vec!["a"]);
        // b and c at same level (order within level is unspecified)
        let mut level1 = graph.levels[1].clone();
        level1.sort();
        assert_eq!(level1, vec!["b", "c"]);
        assert_eq!(graph.levels[2], vec!["d"]);
    }

    #[test]
    fn cycle_detected() {
        let result = build_rule_graph(
            &names(&["a", "b"]),
            vec![edge("a", "b"), edge("b", "a")],
        );
        let err = result.unwrap_err().to_string();
        assert!(err.contains("dependency cycle detected"), "got: {err}");
        assert!(err.contains("a") && err.contains("b"), "got: {err}");
    }

    #[test]
    fn three_node_cycle() {
        let result = build_rule_graph(
            &names(&["a", "b", "c"]),
            vec![edge("a", "b"), edge("b", "c"), edge("c", "a")],
        );
        let err = result.unwrap_err().to_string();
        assert!(err.contains("dependency cycle detected"), "got: {err}");
    }

    #[test]
    fn unknown_producer_error() {
        let result = build_rule_graph(
            &names(&["a"]),
            vec![edge("nonexistent", "a")],
        );
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unknown rule 'nonexistent'"), "got: {err}");
    }

    #[test]
    fn independent_plus_chain() {
        // x is independent, a -> b
        let graph = build_rule_graph(
            &names(&["x", "a", "b"]),
            vec![edge("a", "b")],
        )
        .unwrap();
        assert_eq!(graph.levels.len(), 2);
        // Level 0: x and a (both have in_degree 0)
        let mut level0 = graph.levels[0].clone();
        level0.sort();
        assert_eq!(level0, vec!["a", "x"]);
        assert_eq!(graph.levels[1], vec!["b"]);
    }
}
