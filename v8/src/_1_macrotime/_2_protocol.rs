//! Port of `v7/src/1_libtime/0a_syntax_macro_program.pl`: resolve the
//! `syntax_*` relation identities the macro program declares, pick the rule
//! cone the expander needs, turn graph rows into seeds and read the claims and
//! ordered expansion edges back out of the closure.

use super::_0_rows::{
    args_of, atom_of_text, is_atom_named, macrotime_diagnostic, sort_terms, text_of_atom, Graph,
};
use crate::_6_eval::{Arg, Polarity, Program, Row, Rule, TermId, Universe};
use std::collections::{HashMap, HashSet};

/// The sliced `checked_datalog(root_graph(_, Edges), datalog_program(...))`
/// term, split at the JSON boundary.
pub struct MacroProgram {
    pub edges: Vec<TermId>,
    pub relations: Vec<TermId>,
    pub program: Program,
}

pub struct Protocol {
    pub frontier: TermId,
    pub form: TermId,
    pub atom: TermId,
    pub literal: TermId,
    pub variable: TermId,
    pub source: TermId,
    pub claim: TermId,
    pub node: TermId,
    pub colon: TermId,
    pub item: TermId,
    pub expansion: TermId,
    pub none: TermId,
}

impl Protocol {
    pub fn inputs(&self) -> [TermId; 8] {
        [
            self.frontier,
            self.form,
            self.atom,
            self.literal,
            self.variable,
            self.source,
            self.node,
            self.colon,
        ]
    }

    pub fn roots(&self) -> [TermId; 6] {
        [
            self.form,
            self.atom,
            self.literal,
            self.variable,
            self.source,
            self.claim,
        ]
    }
}

fn wrap_ref(u: &mut Universe, inner: TermId) -> TermId {
    u.compound("ref", vec![inner])
}

fn tagged(u: &mut Universe, tag: &str, inner: TermId) -> TermId {
    u.compound(tag, vec![inner])
}

/// `resolve_protocol_relation/6`: the one relation of this arity a protocol
/// edge of this name points at.
fn resolve(
    u: &mut Universe,
    mp: &MacroProgram,
    name: &str,
    arity: i64,
) -> (Option<TermId>, Vec<TermId>) {
    let declared: HashSet<(TermId, i64)> = mp
        .relations
        .iter()
        .filter_map(|r| {
            let args = args_of(u, *r, "relation", 3)?;
            let inner = u.unary(args[0], "ref")?;
            Some((inner, u.as_int(args[1])?))
        })
        .collect();
    let mut candidates: Vec<TermId> = mp
        .edges
        .iter()
        .filter_map(|e| {
            let args = args_of(u, *e, ":", 4)?;
            if !is_atom_named(u, args[1], name) {
                return None;
            }
            let inner = u.unary(args[2], "ref")?;
            if declared.contains(&(inner, arity)) {
                Some(inner)
            } else {
                None
            }
        })
        .collect();
    sort_terms(u, &mut candidates);
    match candidates.len() {
        1 => (Some(candidates[0]), Vec::new()),
        0 => {
            let name = u.atom(name);
            let arity = u.int(arity);
            let payload = u.compound("missing_protocol_relation", vec![name, arity]);
            let none = u.atom("none");
            (None, vec![macrotime_diagnostic(u, none, payload)])
        }
        _ => {
            let name = u.atom(name);
            let arity_term = u.int(arity);
            let list = u.list(&candidates);
            let payload = u.compound("ambiguous_protocol_relation", vec![name, arity_term, list]);
            let none = u.atom("none");
            (None, vec![macrotime_diagnostic(u, none, payload)])
        }
    }
}

pub fn macro_protocol(u: &mut Universe, mp: &MacroProgram) -> (Option<Protocol>, Vec<TermId>) {
    let wanted: [(&str, i64); 7] = [
        ("syntax_frontier", 2),
        ("syntax_form", 1),
        ("syntax_atom", 2),
        ("syntax_literal", 2),
        ("syntax_variable", 3),
        ("syntax_source", 8),
        ("syntax_claim", 2),
    ];
    let mut found = Vec::new();
    let mut diagnostics = Vec::new();
    for (name, arity) in wanted {
        let (relation, mut d) = resolve(u, mp, name, arity);
        found.push(relation);
        diagnostics.append(&mut d);
    }
    if !diagnostics.is_empty() {
        return (None, diagnostics);
    }
    let rel = |u: &mut Universe, inner: TermId| wrap_ref(u, inner);
    let frontier = rel(u, found[0].unwrap());
    let form = rel(u, found[1].unwrap());
    let atom = rel(u, found[2].unwrap());
    let literal = rel(u, found[3].unwrap());
    let variable = rel(u, found[4].unwrap());
    let source = rel(u, found[5].unwrap());
    let claim = rel(u, found[6].unwrap());
    let node_name = u.atom("node");
    let node_kernel = tagged(u, "kernel", node_name);
    let node = wrap_ref(u, node_kernel);
    let colon_name = u.atom(":");
    let colon_kernel = tagged(u, "kernel", colon_name);
    let colon = wrap_ref(u, colon_kernel);
    (
        Some(Protocol {
            frontier,
            form,
            atom,
            literal,
            variable,
            source,
            claim,
            node,
            colon,
            item: u.atom("item"),
            expansion: u.atom("expansion"),
            none: u.atom("none"),
        }),
        Vec::new(),
    )
}

fn ground_const(u: &Universe, arg: &Arg, name: &str) -> bool {
    match arg {
        Arg::Ground(t) => match u.unary(*t, "const") {
            Some(inner) => is_atom_named(u, inner, name),
            None => false,
        },
        _ => false,
    }
}

fn ground_const_int(u: &Universe, arg: &Arg, value: i64) -> bool {
    match arg {
        Arg::Ground(t) => match u.unary(*t, "const") {
            Some(inner) => u.as_int(inner) == Some(value),
            None => false,
        },
        _ => false,
    }
}

fn const_text(u: &Universe, arg: &Arg) -> Option<TermId> {
    match arg {
        Arg::Ground(t) => u.unary(*t, "const"),
        _ => None,
    }
}

/// `macro_rules/3`: the protocol writers, the kernel node and item/expansion
/// edge writers, and everything they read that is not a protocol input.
pub fn macro_rules(u: &Universe, p: &Protocol, rules: &[Rule]) -> Vec<usize> {
    let roots = p.roots();
    let mut selected: HashSet<usize> = HashSet::new();
    for (i, rule) in rules.iter().enumerate() {
        let is_root = roots.contains(&rule.rel)
            || rule.rel == p.node
            || (rule.rel == p.colon
                && rule.head.len() == 4
                && (ground_const(u, &rule.head[1], "item")
                    || ground_const(u, &rule.head[1], "expansion")));
        if is_root {
            selected.insert(i);
        }
    }
    let inputs = p.inputs();
    loop {
        let mut dependencies: HashSet<TermId> = HashSet::new();
        for &i in &selected {
            for goal in &rules[i].body {
                if !inputs.contains(&goal.rel) {
                    dependencies.insert(goal.rel);
                }
            }
        }
        let before = selected.len();
        for (j, rule) in rules.iter().enumerate() {
            if dependencies.contains(&rule.rel) {
                selected.insert(j);
            }
        }
        if selected.len() == before {
            break;
        }
    }
    let mut out: Vec<usize> = selected.into_iter().collect();
    out.sort_unstable();
    out
}

pub enum Dispatch {
    Absent,
    Present,
    Unknown,
}

/// `claim_rule_head_name/4`: `memberchk/2` commits to the first matching goal,
/// so only the first item-zero edge goal of this rule can name the macro.
fn claim_rule_name(u: &mut Universe, p: &Protocol, rule: &Rule) -> Option<TermId> {
    if rule.head.len() != 2 {
        return None;
    }
    let invocation = &rule.head[0];
    let head_arg = rule.body.iter().find_map(|goal| {
        if goal.polarity != Polarity::Positive
            || goal.rel != p.colon
            || goal.args.len() != 4
            || &goal.args[0] != invocation
            || !ground_const(u, &goal.args[1], "item")
            || !ground_const_int(u, &goal.args[3], 0)
        {
            return None;
        }
        Some(goal.args[2].clone())
    })?;
    let text = rule.body.iter().find_map(|goal| {
        if goal.polarity != Polarity::Positive
            || goal.rel != p.atom
            || goal.args.len() != 2
            || goal.args[0] != head_arg
        {
            return None;
        }
        const_text(u, &goal.args[1])
    })?;
    Some(atom_of_text(u, text))
}

/// `macro_dispatch/4`: when every claim writer names its macro with a literal
/// item-zero atom, the absence of those names in the graph proves an empty
/// claim set without running the evaluator.
pub fn macro_dispatch(u: &mut Universe, p: &Protocol, rules: &[Rule], g: &Graph) -> Dispatch {
    let claim_rules: Vec<&Rule> = rules.iter().filter(|r| r.rel == p.claim).collect();
    let mut names: Vec<TermId> = claim_rules
        .iter()
        .filter_map(|r| claim_rule_name(u, p, r))
        .collect();
    let claim_count = claim_rules.len();
    sort_terms(u, &mut names);
    if claim_count != names.len() {
        return Dispatch::Unknown;
    }
    let named: HashSet<TermId> = names.into_iter().collect();
    for row in &g.rows {
        let head = match args_of(u, *row, ":", 4) {
            Some(args) if is_atom_named(u, args[1], "item") && u.as_int(args[3]) == Some(0) => {
                match u.unary(args[2], "ref") {
                    Some(head) => head,
                    None => continue,
                }
            }
            _ => continue,
        };
        if let Some(name) = g.names.get(&head) {
            if named.contains(name) {
                return Dispatch::Present;
            }
        }
    }
    Dispatch::Absent
}

/// `syntax_seed_calls/3`: one seed row per graph row, tagged for the
/// evaluator.
pub fn syntax_seeds(u: &mut Universe, p: &Protocol, rows: &[TermId]) -> Vec<Row> {
    let mut out = Vec::with_capacity(rows.len());
    for &row in rows {
        let (name, args) = match u.functor(row) {
            Some((n, a)) => (n.to_string(), a.to_vec()),
            None => continue,
        };
        let seed = match (name.as_str(), args.len()) {
            ("node", 1) => {
                let node = wrap_ref(u, args[0]);
                Row {
                    rel: p.node,
                    args: vec![node],
                }
            }
            (":", 4) => {
                let owner = wrap_ref(u, args[0]);
                let label = tagged(u, "const", args[1]);
                let ordinal = tagged(u, "const", args[3]);
                Row {
                    rel: p.colon,
                    args: vec![owner, label, args[2], ordinal],
                }
            }
            ("syntax_frontier", 2) => {
                let ordinal = tagged(u, "const", args[0]);
                let node = wrap_ref(u, args[1]);
                Row {
                    rel: p.frontier,
                    args: vec![ordinal, node],
                }
            }
            ("syntax_form", 1) => {
                let node = wrap_ref(u, args[0]);
                Row {
                    rel: p.form,
                    args: vec![node],
                }
            }
            ("syntax_atom", 2) => {
                let node = wrap_ref(u, args[0]);
                let text = text_of_atom(u, args[1]);
                let text = tagged(u, "const", text);
                Row {
                    rel: p.atom,
                    args: vec![node, text],
                }
            }
            ("syntax_literal", 2) => {
                let node = wrap_ref(u, args[0]);
                let value = tagged(u, "const", args[1]);
                Row {
                    rel: p.literal,
                    args: vec![node, value],
                }
            }
            ("syntax_variable", 3) => {
                let node = wrap_ref(u, args[0]);
                let variable = wrap_ref(u, args[1]);
                let text = text_of_atom(u, args[2]);
                let text = tagged(u, "const", text);
                Row {
                    rel: p.variable,
                    args: vec![node, variable, text],
                }
            }
            ("source", 8) => {
                let mut cells = vec![wrap_ref(u, args[0])];
                for &cell in &args[1..] {
                    cells.push(tagged(u, "const", cell));
                }
                Row {
                    rel: p.source,
                    args: cells,
                }
            }
            _ => continue,
        };
        out.push(seed);
    }
    out
}

pub struct Claim {
    pub invocation: TermId,
    pub identity: TermId,
}

pub struct Output {
    pub invocation: TermId,
    pub output: TermId,
    pub ordinal: TermId,
}

/// `macro_results/6`: the syntax rows the closure derived, the claims on nodes
/// that are still active, and the ordered expansion edges of those claims.
pub fn macro_results(
    u: &mut Universe,
    p: &Protocol,
    closure: &[Row],
    active: &HashSet<TermId>,
) -> (Vec<TermId>, Vec<Claim>, Vec<Output>) {
    let mut by_rel: HashMap<TermId, Vec<&Row>> = HashMap::new();
    for row in closure {
        by_rel.entry(row.rel).or_default().push(row);
    }
    let empty: Vec<&Row> = Vec::new();
    let at = |rel: TermId| -> &[&Row] { by_rel.get(&rel).map_or(&empty, |v| v.as_slice()) };

    let mut identities: HashSet<TermId> = HashSet::new();
    for (rel, arity) in [(p.form, 1), (p.atom, 2), (p.literal, 2), (p.variable, 3)] {
        for row in at(rel) {
            if row.args.len() == arity {
                if let Some(node) = u.unary(row.args[0], "ref") {
                    identities.insert(node);
                }
            }
        }
    }

    let mut available: Vec<TermId> = Vec::new();
    for row in at(p.form) {
        if let (1, Some(node)) = (row.args.len(), u.unary(row.args[0], "ref")) {
            available.push(u.compound("syntax_form", vec![node]));
        }
    }
    for row in at(p.atom) {
        if row.args.len() != 2 {
            continue;
        }
        if let (Some(node), Some(text)) =
            (u.unary(row.args[0], "ref"), u.unary(row.args[1], "const"))
        {
            let name = atom_of_text(u, text);
            available.push(u.compound("syntax_atom", vec![node, name]));
        }
    }
    for row in at(p.literal) {
        if row.args.len() != 2 {
            continue;
        }
        if let (Some(node), Some(value)) =
            (u.unary(row.args[0], "ref"), u.unary(row.args[1], "const"))
        {
            available.push(u.compound("syntax_literal", vec![node, value]));
        }
    }
    for row in at(p.variable) {
        if row.args.len() != 3 {
            continue;
        }
        let node = u.unary(row.args[0], "ref");
        let variable = u.unary(row.args[1], "ref");
        let text = u.unary(row.args[2], "const");
        if let (Some(node), Some(variable), Some(text)) = (node, variable, text) {
            let name = atom_of_text(u, text);
            available.push(u.compound("syntax_variable", vec![node, variable, name]));
        }
    }
    for row in at(p.source) {
        if row.args.len() != 8 {
            continue;
        }
        let node = match u.unary(row.args[0], "ref") {
            Some(n) => n,
            None => continue,
        };
        let mut cells = vec![node];
        let mut ok = true;
        for &cell in &row.args[1..] {
            match u.unary(cell, "const") {
                Some(value) => cells.push(value),
                None => ok = false,
            }
        }
        if ok {
            available.push(u.compound("source", cells));
        }
    }
    for row in at(p.node) {
        if row.args.len() != 1 {
            continue;
        }
        if let Some(node) = u.unary(row.args[0], "ref") {
            if identities.contains(&node) {
                available.push(u.compound("node", vec![node]));
            }
        }
    }
    for row in at(p.colon) {
        if row.args.len() != 4 || !is_const_atom(u, row.args[1], "item") {
            continue;
        }
        let owner = u.unary(row.args[0], "ref");
        let target = u.unary(row.args[2], "ref");
        let ordinal = u.unary(row.args[3], "const");
        if let (Some(owner), Some(target), Some(ordinal)) = (owner, target, ordinal) {
            if identities.contains(&owner) && identities.contains(&target) {
                let item = p.item;
                let reference = wrap_ref(u, target);
                available.push(u.compound(":", vec![owner, item, reference, ordinal]));
            }
        }
    }
    sort_terms(u, &mut available);

    let mut claim_terms: Vec<TermId> = Vec::new();
    for row in at(p.claim) {
        if row.args.len() != 2 {
            continue;
        }
        let invocation = match u.unary(row.args[0], "ref") {
            Some(i) if active.contains(&i) => i,
            _ => continue,
        };
        let identity = match u.unary(row.args[1], "const") {
            Some(value) => value,
            None => match u.unary(row.args[1], "ref") {
                Some(_) => row.args[1],
                None => continue,
            },
        };
        claim_terms.push(u.compound("claim", vec![invocation, identity]));
    }
    let invocations: HashSet<TermId> = claim_terms
        .iter()
        .filter_map(|c| u.functor(*c).map(|(_, args)| args[0]))
        .collect();
    sort_terms(u, &mut claim_terms);
    let claims: Vec<Claim> = claim_terms
        .iter()
        .map(|c| {
            let args = u.functor(*c).unwrap().1;
            Claim {
                invocation: args[0],
                identity: args[1],
            }
        })
        .collect();

    let mut output_terms: Vec<TermId> = Vec::new();
    for row in at(p.colon) {
        if row.args.len() != 4 || !is_const_atom(u, row.args[1], "expansion") {
            continue;
        }
        let invocation = u.unary(row.args[0], "ref");
        let output = u.unary(row.args[2], "ref");
        let ordinal = u.unary(row.args[3], "const");
        if let (Some(invocation), Some(output), Some(ordinal)) = (invocation, output, ordinal) {
            if invocations.contains(&invocation) {
                output_terms.push(u.compound("output", vec![invocation, output, ordinal]));
            }
        }
    }
    sort_terms(u, &mut output_terms);
    let outputs: Vec<Output> = output_terms
        .iter()
        .map(|o| {
            let args = u.functor(*o).unwrap().1;
            Output {
                invocation: args[0],
                output: args[1],
                ordinal: args[2],
            }
        })
        .collect();
    (available, claims, outputs)
}

fn is_const_atom(u: &Universe, tagged: TermId, name: &str) -> bool {
    match u.unary(tagged, "const") {
        Some(inner) => is_atom_named(u, inner, name),
        None => false,
    }
}
