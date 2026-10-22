//! The plain Rust reference: transitive closure by search from each caller. The dataflow's stored
//! `reach` must equal it.

use crate::_2_store::Edge;
use std::collections::{BTreeSet, HashMap};

pub fn closure(edges: &BTreeSet<Edge>) -> BTreeSet<Edge> {
    let mut out: HashMap<u64, Vec<u64>> = HashMap::new();
    for (from, to) in edges {
        out.entry(*from).or_default().push(*to);
    }
    let mut reach = BTreeSet::new();
    for &start in out.keys() {
        let mut seen = BTreeSet::new();
        let mut stack = out[&start].clone();
        while let Some(node) = stack.pop() {
            if seen.insert(node) {
                stack.extend(out.get(&node).into_iter().flatten().copied());
            }
        }
        reach.extend(seen.into_iter().map(|node| (start, node)));
    }
    reach
}
