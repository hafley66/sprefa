//! Rule and goal origin rows. Port of `0_lowerer.pl:536-665`.

use crate::_6_eval::term::{TermId, Universe};

pub fn rule_origin(u: &mut Universe, rule_index: i64, node: TermId) -> TermId {
    let index = u.int(rule_index);
    let rule = u.compound("rule", vec![index]);
    u.compound("origin", vec![rule, node])
}

/// `:652`.
pub fn indexed_goal_origins(u: &mut Universe, nodes: &[TermId], rule_index: i64) -> Vec<TermId> {
    nodes
        .iter()
        .enumerate()
        .map(|(goal_index, node)| {
            let rule = u.int(rule_index);
            let goal = u.int(goal_index as i64);
            let key = u.compound("goal", vec![rule, goal]);
            u.compound("origin", vec![key, *node])
        })
        .collect()
}

/// `:536`. One origin per rule in a block that shares a source node.
pub fn indexed_empty_rule_origins(
    u: &mut Universe,
    count: usize,
    rule_index: i64,
    node: TermId,
) -> Vec<TermId> {
    (0..count)
        .map(|offset| rule_origin(u, rule_index + offset as i64, node))
        .collect()
}
