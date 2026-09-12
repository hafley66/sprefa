//! `evaluate_checked/4`. Port of `2_compiler.pl:718-767` and `:1505-1591`.
//! v7's `:- dynamic` round caches are not ported; fork F1 in the PLAN.

use super::finish::finish_evaluation;
use super::rounds::{rounds, Round};
use crate::_3_check::api::prolog_sort;
use crate::_3_check::{Checked, Stop};
use crate::_6_eval::term::{TermId, Universe};

/// `compiled_unit(TypeGraphFacts, RuntimeProgram, CompilerFacts)` at `:910`.
pub struct Compiled {
    pub type_graph_facts: Vec<TermId>,
    pub runtime: Checked,
    pub compiler_facts: Vec<TermId>,
}

/// `generated_program(Relations, Rules, Depends, Strata)` at `:1459`.
#[derive(Default, Clone)]
pub struct Generated {
    pub relations: Vec<TermId>,
    pub rules: Vec<TermId>,
    pub depends: Vec<TermId>,
    pub strata: Vec<TermId>,
}

/// `final_checked_program/5` at `:929`, `deferred_checked_program/5` at `:949`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Refreeze {
    Final,
    Deferred,
}

/// Comptime's one call back into lowering; `Replay` and the driver lane's real
/// lowering are the two impls.
pub trait Sources {
    fn refreeze(
        &mut self,
        u: &mut Universe,
        mode: Refreeze,
        compiler_facts: &[TermId],
        generated_relations: &[TermId],
    ) -> Result<(Option<Checked>, Vec<TermId>), Stop>;
}

/// `:718`. `None` is v7's `Compiled = []`.
pub fn evaluate_checked(
    u: &mut Universe,
    checked: &Checked,
    sources: &mut dyn Sources,
    fx: &mut dyn FnMut(Round),
) -> Result<(Option<Compiled>, Vec<TermId>), Stop> {
    let graph_seeds = graph_seeds(u, &checked.nodes, &checked.edges);
    let mut base = graph_seeds;
    base.extend_from_slice(&checked.seeds);
    let base_seeds = prolog_sort(u, base);

    let mut initial = colon_rows(u, &base_seeds);
    initial.extend(source_application_edges(u, &checked.rules));
    let initial_edges = prolog_sort(u, initial);
    let initial_requests = intern_rows(u, &base_seeds);

    let outcome = rounds(
        u,
        &checked.rules,
        &checked.relations,
        &base_seeds,
        initial_edges,
        initial_requests,
        Vec::new(),
        Vec::new(),
        1,
        fx,
    )?;
    finish_evaluation(u, outcome, sources, fx)
}

/// `:745`. Heads of shape `:(ref(Owner), const(Name), var(derived_bind(_)), const(Index))`.
pub fn derived_bind_slots(u: &mut Universe, rules: &[TermId]) -> Vec<TermId> {
    let mut out = Vec::new();
    for rule in rules {
        let Some(("rule", args)) = u.functor(*rule) else {
            continue;
        };
        let head = args[0];
        let Some(parts) = colon_call_parts(u, head) else {
            continue;
        };
        let [owner, name, value, index] = parts;
        if u.unary(owner, "ref").is_none() {
            continue;
        }
        let (Some(name), Some(index)) = (u.unary(name, "const"), u.unary(index, "const")) else {
            continue;
        };
        let Some(bind) = u.unary(value, "var") else {
            continue;
        };
        if u.unary(bind, "derived_bind").is_none() {
            continue;
        }
        let owner = u.unary(owner, "ref").unwrap();
        out.push(u.compound("derived_bind_slot", vec![owner, name, index]));
    }
    prolog_sort(u, out)
}

/// The four arguments of `call(ref(kernel(':')), [_, _, _, _])`.
pub fn colon_call_parts(u: &Universe, term: TermId) -> Option<[TermId; 4]> {
    let ("call", args) = u.functor(term)? else {
        return None;
    };
    if args.len() != 2 {
        return None;
    }
    let (rel, arguments) = (args[0], args[1]);
    if !is_kernel_ref(u, rel, ":") {
        return None;
    }
    let items = u.as_list(arguments)?;
    match items.len() {
        4 => Some([items[0], items[1], items[2], items[3]]),
        _ => None,
    }
}

/// `ref(kernel(Name))`.
pub fn is_kernel_ref(u: &Universe, term: TermId, name: &str) -> bool {
    let Some(inner) = u.unary(term, "ref") else {
        return false;
    };
    u.unary(inner, "kernel")
        .and_then(|n| u.functor_or_atom(n))
        .is_some_and(|(n, args)| args.is_empty() && n == name)
}

/// `call(ref(kernel(Name)), Args)`; `Some(args)` when the relation matches.
pub fn kernel_call_args(u: &Universe, term: TermId, name: &str) -> Option<Vec<TermId>> {
    let ("call", args) = u.functor(term)? else {
        return None;
    };
    if args.len() != 2 || !is_kernel_ref(u, args[0], name) {
        return None;
    }
    u.as_list(args[1])
}

/// `:1535`. `include(colon_row, Rows, _), sort/2`.
pub fn colon_rows(u: &mut Universe, rows: &[TermId]) -> Vec<TermId> {
    let kept: Vec<TermId> = rows
        .iter()
        .copied()
        .filter(|r| kernel_call_args(u, *r, ":").is_some())
        .collect();
    prolog_sort(u, kept)
}

/// `:1526`.
pub fn intern_rows(u: &mut Universe, rows: &[TermId]) -> Vec<TermId> {
    let kept: Vec<TermId> = rows
        .iter()
        .copied()
        .filter(|r| kernel_call_args(u, *r, "intern").is_some())
        .collect();
    prolog_sort(u, kept)
}

/// `:1530`. `exclude/3` keeps list order.
pub fn strip_intern_rows(u: &Universe, rows: &[TermId]) -> Vec<TermId> {
    rows.iter()
        .copied()
        .filter(|r| kernel_call_args(u, *r, "intern").is_none())
        .collect()
}

/// `:1520`.
pub fn strip_snapshot_rows(u: &Universe, rows: &[TermId]) -> Vec<TermId> {
    rows.iter()
        .copied()
        .filter(|r| {
            kernel_call_args(u, *r, "edge_snapshot").is_none()
                && kernel_call_args(u, *r, "intern_snapshot").is_none()
        })
        .collect()
}

/// `:1544`. A ground fact rule whose head is a `:/4` edge owned by a call.
pub fn source_application_edges(u: &mut Universe, rules: &[TermId]) -> Vec<TermId> {
    let mut out = Vec::new();
    for rule in rules {
        let Some(("rule", args)) = u.functor(*rule) else {
            continue;
        };
        if !u.as_list(args[1]).is_some_and(|b| b.is_empty()) {
            continue;
        }
        let head = args[0];
        let Some([owner, _, _, _]) = colon_call_parts(u, head) else {
            continue;
        };
        if u.unary(owner, "ref")
            .and_then(|inner| u.functor(inner))
            .is_some_and(|(n, a)| n == "call" && a.len() == 2)
        {
            out.push(head);
        }
    }
    prolog_sort(u, out)
}

/// `:1554`. Nodes and edges of the checked graph as evaluator seeds.
pub fn graph_seeds(u: &mut Universe, nodes: &[TermId], edges: &[TermId]) -> Vec<TermId> {
    let mut out = Vec::with_capacity(nodes.len() + edges.len());
    for node in nodes {
        let Some((name, args)) = u.functor(*node).map(|(n, a)| (n.to_string(), a.to_vec())) else {
            continue;
        };
        if args.len() != 1 || !matches!(name.as_str(), "node" | "module" | "product" | "sum") {
            continue;
        }
        let identity = u.compound("ref", vec![args[0]]);
        out.push(kernel_call(u, &name, vec![identity]));
    }
    for edge in edges {
        let Some((":", args)) = u.functor(*edge) else {
            continue;
        };
        if args.len() != 4 {
            continue;
        }
        let (owner, name, target, index) = (args[0], args[1], args[2], args[3]);
        let owner = u.compound("ref", vec![owner]);
        let name = u.compound("const", vec![name]);
        let index = u.compound("const", vec![index]);
        out.push(kernel_call(u, ":", vec![owner, name, target, index]));
    }
    out
}

/// `call(ref(kernel(Name)), Args)`.
pub fn kernel_call(u: &mut Universe, name: &str, args: Vec<TermId>) -> TermId {
    let atom = u.atom(name);
    let kernel = u.compound("kernel", vec![atom]);
    let rel = u.compound("ref", vec![kernel]);
    let list = u.list(&args);
    u.compound("call", vec![rel, list])
}

/// `:1160`. A compiler row value read as an edge target.
pub fn compiler_value_target(u: &mut Universe, value: TermId) -> Option<TermId> {
    if let Some(identity) = u.unary(value, "ref") {
        return Some(u.compound("target", vec![identity]));
    }
    if u.unary(value, "const").is_some() {
        return Some(value);
    }
    None
}

/// `diagnostic(Phase, none, Reason)`.
pub fn diagnostic(u: &mut Universe, phase: &str, reason: TermId) -> TermId {
    let phase = u.atom(phase);
    let none = u.atom("none");
    u.compound("diagnostic", vec![phase, none, reason])
}
