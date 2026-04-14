//! Rule DAG — cross-ref dependency graph over registered rules.
//!
//! Input: `pctx.rules` after `lower_rules`. Each `RuleHandle` carries
//! `depends_on: Vec<Arc<str>>` (rule → target rule names). This module
//! runs Kahn's algorithm to produce either dependency-ordered levels
//! or a cycle path for diagnostic reporting.
//!
//! Level 0 = rules with no cross-ref deps; level N = rules that depend
//! only on levels <N. Independent rules share a level and can run in
//! parallel. Kahn's in-degree waves give us that grouping for free.
//!
//! Unknown-target edges (e.g. `${nope.$X}` where `nope` is not a rule)
//! are skipped here — `xref/unknown-rule` already flags them upstream
//! in `analysis.rs`. The DAG treats them as no-op so lower still gets
//! a sensible topo order for the rules that do exist.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use crate::_5_op::RuleHandle;

/// Dependency-ordered result of a topological sort.
#[derive(Debug, Clone)]
pub enum RuleDag {
    /// No cycles. `levels[0]` = rules with in-degree 0; level N+1 = rules
    /// unblocked once level N finishes. Intra-level order is unspecified.
    Ordered { levels: Vec<Vec<Arc<str>>> },
    /// Cycle among these rule names. `path` reconstructs a loop (A→B→C→A).
    Cycle   { path: Vec<Arc<str>> },
}

/// Build a rule DAG from `pctx.rules` handles.
pub fn build(rules: &HashMap<Arc<str>, RuleHandle>) -> RuleDag {
    let names: Vec<Arc<str>> = rules.keys().cloned().collect();

    // Adjacency: producer → consumers. In-degree: consumer → count.
    let mut dependents: HashMap<Arc<str>, Vec<Arc<str>>> = HashMap::new();
    let mut in_degree:  HashMap<Arc<str>, usize>         = HashMap::new();
    for n in &names { in_degree.insert(n.clone(), 0); }
    for (consumer, h) in rules {
        for producer in &h.depends_on {
            // Skip unknown producers; analysis.rs already diags them.
            if !rules.contains_key(producer) { continue; }
            // Guard against duplicate edges inflating in-degree.
            if dependents.get(producer).is_some_and(|v| v.iter().any(|c| c == consumer)) {
                continue;
            }
            dependents.entry(producer.clone()).or_default().push(consumer.clone());
            *in_degree.entry(consumer.clone()).or_insert(0) += 1;
        }
    }

    // Kahn's: peel in-degree-0 waves into levels.
    let mut levels: Vec<Vec<Arc<str>>> = Vec::new();
    let mut queue:  VecDeque<Arc<str>> = in_degree
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(n, _)| n.clone())
        .collect();

    let mut visited = 0usize;
    while !queue.is_empty() {
        let level: Vec<Arc<str>> = queue.drain(..).collect();
        visited += level.len();
        for node in &level {
            if let Some(deps) = dependents.get(node) {
                for dep in deps {
                    let d = in_degree.get_mut(dep).unwrap();
                    *d -= 1;
                    if *d == 0 { queue.push_back(dep.clone()); }
                }
            }
        }
        levels.push(level);
    }

    if visited == names.len() {
        return RuleDag::Ordered { levels };
    }

    // Cycle: walk remaining in-degree>0 nodes and recover a loop via DFS.
    let remaining: Vec<Arc<str>> = in_degree
        .iter()
        .filter(|(_, &d)| d > 0)
        .map(|(n, _)| n.clone())
        .collect();
    let path = find_cycle(&remaining, &dependents).unwrap_or(remaining);
    RuleDag::Cycle { path }
}

/// DFS through `dependents` restricted to `in_cycle` members. Returns the
/// first loop encountered as `[A, B, C, A]`.
fn find_cycle(
    in_cycle:   &[Arc<str>],
    dependents: &HashMap<Arc<str>, Vec<Arc<str>>>,
) -> Option<Vec<Arc<str>>> {
    let set: HashSet<Arc<str>> = in_cycle.iter().cloned().collect();
    let mut visited: HashSet<Arc<str>> = HashSet::new();
    let mut stack:   Vec<Arc<str>>     = Vec::new();
    fn dfs(
        node: Arc<str>,
        set: &HashSet<Arc<str>>,
        dependents: &HashMap<Arc<str>, Vec<Arc<str>>>,
        visited: &mut HashSet<Arc<str>>,
        stack: &mut Vec<Arc<str>>,
    ) -> Option<Vec<Arc<str>>> {
        if let Some(pos) = stack.iter().position(|n| *n == node) {
            let mut cycle: Vec<Arc<str>> = stack[pos..].to_vec();
            cycle.push(node);
            return Some(cycle);
        }
        if visited.contains(&node) { return None; }
        visited.insert(node.clone());
        stack.push(node.clone());
        if let Some(nexts) = dependents.get(&node) {
            for next in nexts {
                if !set.contains(next) { continue; }
                if let Some(c) = dfs(next.clone(), set, dependents, visited, stack) {
                    return Some(c);
                }
            }
        }
        stack.pop();
        None
    }
    for n in in_cycle {
        if let Some(c) = dfs(n.clone(), &set, dependents, &mut visited, &mut stack) {
            return Some(c);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use crate::_0_types::ParseSite;

    fn handle(name: &str, deps: &[&str]) -> (Arc<str>, RuleHandle) {
        let n: Arc<str> = Arc::from(name);
        let ps = Arc::new(ParseSite {
            file:       Arc::from(PathBuf::from("t.sprf").as_path()),
            path:       Arc::from(Vec::new().into_boxed_slice()),
            byte_range: 0..0,
        });
        (n.clone(), RuleHandle {
            name:       n,
            parse_site: ps,
            captures:   vec![],
            depends_on: deps.iter().map(|s| Arc::<str>::from(*s)).collect(),
        })
    }

    fn rules(items: Vec<(Arc<str>, RuleHandle)>) -> HashMap<Arc<str>, RuleHandle> {
        items.into_iter().collect()
    }

    #[test]
    fn no_edges_single_level() {
        let dag = build(&rules(vec![handle("a", &[]), handle("b", &[]), handle("c", &[])]));
        match dag {
            RuleDag::Ordered { levels } => {
                assert_eq!(levels.len(), 1);
                assert_eq!(levels[0].len(), 3);
            }
            RuleDag::Cycle { .. } => panic!("expected Ordered"),
        }
    }

    #[test]
    fn linear_chain() {
        // a producer, b → a, c → b. Expected levels: [a], [b], [c].
        let dag = build(&rules(vec![
            handle("a", &[]),
            handle("b", &["a"]),
            handle("c", &["b"]),
        ]));
        match dag {
            RuleDag::Ordered { levels } => {
                assert_eq!(levels.len(), 3);
                assert_eq!(&*levels[0][0], "a");
                assert_eq!(&*levels[1][0], "b");
                assert_eq!(&*levels[2][0], "c");
            }
            _ => panic!("expected Ordered"),
        }
    }

    #[test]
    fn diamond() {
        // a; b→a; c→a; d→b,c. Level 1 = {b,c}, Level 2 = {d}.
        let dag = build(&rules(vec![
            handle("a", &[]),
            handle("b", &["a"]),
            handle("c", &["a"]),
            handle("d", &["b", "c"]),
        ]));
        match dag {
            RuleDag::Ordered { levels } => {
                assert_eq!(levels.len(), 3);
                assert_eq!(&*levels[0][0], "a");
                let mut l1: Vec<&str> = levels[1].iter().map(|s| &**s).collect();
                l1.sort();
                assert_eq!(l1, vec!["b", "c"]);
                assert_eq!(&*levels[2][0], "d");
            }
            _ => panic!("expected Ordered"),
        }
    }

    #[test]
    fn cycle_two_node() {
        let dag = build(&rules(vec![
            handle("a", &["b"]),
            handle("b", &["a"]),
        ]));
        match dag {
            RuleDag::Cycle { path } => {
                let names: Vec<&str> = path.iter().map(|s| &**s).collect();
                assert!(names.contains(&"a") && names.contains(&"b"), "got: {names:?}");
                assert_eq!(path.first(), path.last(), "cycle path should loop");
            }
            _ => panic!("expected Cycle"),
        }
    }

    #[test]
    fn cycle_three_node() {
        let dag = build(&rules(vec![
            handle("a", &["b"]),
            handle("b", &["c"]),
            handle("c", &["a"]),
        ]));
        assert!(matches!(dag, RuleDag::Cycle { .. }));
    }

    #[test]
    fn unknown_producer_skipped() {
        // b depends on "nope" which isn't a rule. DAG treats it as no edge.
        // analysis.rs handles xref/unknown-rule separately.
        let dag = build(&rules(vec![handle("b", &["nope"])]));
        match dag {
            RuleDag::Ordered { levels } => {
                assert_eq!(levels.len(), 1);
                assert_eq!(&*levels[0][0], "b");
            }
            _ => panic!("expected Ordered"),
        }
    }

    #[test]
    fn self_ref_ignored_by_caller() {
        // Rule.parse filters self-refs before populating depends_on.
        // DAG doesn't: if depends_on contained self, it would be a cycle.
        // This test documents the contract at the DAG layer.
        let dag = build(&rules(vec![handle("a", &["a"])]));
        assert!(matches!(dag, RuleDag::Cycle { .. }));
    }
}
