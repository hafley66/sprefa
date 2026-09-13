//! `slice_macro_program/3`. Port of
//! `v7/src/1_libtime/0a_syntax_macro_program.pl:38-85`.

use super::_2_protocol::{macro_protocol, macro_rules, MacroProgram};
use crate::_3_check::api::prolog_sort;
use crate::_3_check::Checked;
use crate::_4_comptime::rounds::round_program;
use crate::_6_eval::{Program, TermId, Universe};
use std::collections::HashSet;

/// `:38`. `None` is v7's `MacroProgram = []`, which the caller turns into an
/// empty compile result.
pub fn slice_macro_program(
    u: &mut Universe,
    checked: &Checked,
) -> (Option<MacroProgram>, Vec<TermId>) {
    let whole = MacroProgram {
        edges: checked.edges.clone(),
        relations: checked.relations.clone(),
        program: Program::default(),
    };
    let (protocol, diagnostics) = macro_protocol(u, &whole);
    let Some(protocol) = protocol else {
        return (None, diagnostics);
    };
    let Ok(program) = round_program(u, &checked.rules, &checked.seeds) else {
        return (None, diagnostics);
    };
    let selected = macro_rules(u, &protocol, &program.rules);

    let ids: HashSet<TermId> = [
        protocol.frontier,
        protocol.form,
        protocol.atom,
        protocol.literal,
        protocol.variable,
        protocol.source,
        protocol.claim,
    ]
    .iter()
    .filter_map(|r| u.unary(*r, "ref"))
    .collect();

    // :54. An edge counts when its target is a protocol relation; a
    // declaration counts when it declares one.
    let edges = checked
        .edges
        .iter()
        .copied()
        .filter(|edge| match u.functor(*edge) {
            Some((":", args)) if args.len() == 4 => u
                .unary(args[2], "ref")
                .is_some_and(|target| ids.contains(&target)),
            _ => false,
        })
        .collect();
    let relations = checked
        .relations
        .iter()
        .copied()
        .filter(|row| match u.functor(*row) {
            Some(("relation", args)) if args.len() == 3 => u
                .unary(args[0], "ref")
                .is_some_and(|relation| ids.contains(&relation)),
            _ => false,
        })
        .collect();

    let mut reachable = Vec::new();
    for index in &selected {
        let rule = &program.rules[*index];
        if let Some(relation) = u.unary(rule.rel, "ref") {
            reachable.push(relation);
        }
        for goal in &rule.body {
            if let Some(relation) = u.unary(goal.rel, "ref") {
                reachable.push(relation);
            }
        }
    }
    let reachable: HashSet<TermId> = prolog_sort(u, reachable).into_iter().collect();

    let rules = selected.iter().map(|i| program.rules[*i].clone()).collect();
    let seeds = program
        .seeds
        .iter()
        .filter(|seed| {
            u.unary(seed.rel, "ref")
                .is_some_and(|relation| reachable.contains(&relation))
        })
        .cloned()
        .collect();
    (
        Some(MacroProgram {
            edges,
            relations,
            program: Program { rules, seeds },
        }),
        Vec::new(),
    )
}
