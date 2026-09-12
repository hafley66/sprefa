//! One unique name per owner and dense zero-based indices.
//! Port of `1_checker.pl:477-559`.
//!
//! v7 concatenates three separate scans at `:481`, so all duplicate-name rows
//! come before all duplicate-index rows, which come before all non-dense rows.
//! `dense_index_diagnostics_scanned/4` (`:532`) is the non-ground fallback and
//! is unreachable for a ground program, so only the indexed path is ported.

use super::graph::{CheckerGraph, OriginArena};
use crate::_6_eval::term::{TermId, Universe};
use std::collections::HashSet;

pub fn bind_diagnostics(
    u: &mut Universe,
    graph: &CheckerGraph,
    origins: &OriginArena,
) -> Vec<TermId> {
    let check = u.atom("check");
    let mut names: Vec<TermId> = Vec::new();
    let mut indices: Vec<TermId> = Vec::new();
    let mut dense: Vec<TermId> = Vec::new();
    let mut seen_name: HashSet<(TermId, TermId)> = HashSet::new();
    let mut seen_index: HashSet<(TermId, i64)> = HashSet::new();

    for edge in &graph.edges {
        let node = origins.edge_origin(edge.owner, edge.name, edge.index);
        if !seen_name.insert((edge.owner, edge.name)) {
            let reason = u.compound("duplicate_bind", vec![edge.owner, edge.name]);
            names.push(u.compound("diagnostic", vec![check, node, reason]));
        }
        if !seen_index.insert((edge.owner, edge.index)) {
            let index = u.int(edge.index);
            let reason = u.compound("duplicate_bind_index", vec![edge.owner, index]);
            indices.push(u.compound("diagnostic", vec![check, node, reason]));
        }
        let count = graph.edge_count(edge.owner);
        if edge.index < 0 || edge.index >= count {
            let index = u.int(edge.index);
            let reason = u.compound("non_dense_index", vec![edge.owner, index]);
            dense.push(u.compound("diagnostic", vec![check, node, reason]));
        }
    }
    names.extend(indices);
    names.extend(dense);
    names
}
