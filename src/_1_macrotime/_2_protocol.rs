//! Port of `v7/src/1_libtime/0a_syntax_macro_program.pl`: resolve the
//! `syntax_*` relation identities the macro program declares, pick the rule
//! cone the expander needs, turn graph rows into seeds and read the claims and
//! ordered expansion edges back out of the closure.

use super::_0_rows::{
    args_of, atom_of_text, is_atom_named, macrotime_diagnostic, sort_terms, text_of_atom, Graph,
};
use crate::_6_eval::ir_eval::SignedRow;
use crate::_6_eval::{Arg, Polarity, Program, Row, Rule, TermId, Universe};
use std::collections::{HashMap, HashSet};

/// The sliced `checked_datalog(root_graph(_, Edges), datalog_program(...))`
/// term, split at the JSON boundary.
#[derive(Clone)]
pub struct MacroProgram {
    pub edges: Vec<TermId>,
    pub relations: Vec<TermId>,
    pub program: Program,
    /// The pool the library's comptime session was left in (`Pool::library`): a macrotime
    /// session on it adopts that install for the rule cone. `None`: the cone installs alone.
    pub pool: Option<crate::_6_eval::ir_eval::SharedPool>,
}

pub struct Protocol {
    pub frontier: TermId,
    pub form: TermId,
    pub atom: TermId,
    pub literal: TermId,
    pub variable: TermId,
    pub source: TermId,
    pub claim: TermId,
    /// `syntax_diagnostic(Node, Payload)`; `None` when the macro library
    /// declares no such relation.
    pub diagnostic: Option<TermId>,
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

    pub fn roots(&self) -> Vec<TermId> {
        let mut roots = vec![
            self.form,
            self.atom,
            self.literal,
            self.variable,
            self.source,
            self.claim,
        ];
        roots.extend(self.diagnostic);
        roots
    }

    /// The relations `Available` keeps.
    pub fn reads(&self, rel: TermId) -> bool {
        self.inputs().contains(&rel) || rel == self.claim || Some(rel) == self.diagnostic
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
pub fn resolve(
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
    // The one optional protocol relation: a library that never reports keeps
    // resolving.
    let (diagnostic, mut d) = resolve(u, mp, "syntax_diagnostic", 2);
    if diagnostic.is_some() || !is_missing(u, &d) {
        diagnostics.append(&mut d);
    }
    if !diagnostics.is_empty() {
        return (None, diagnostics);
    }
    let rel = |u: &mut Universe, inner: TermId| wrap_ref(u, inner);
    let diagnostic = diagnostic.map(|inner| rel(u, inner));
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
            diagnostic,
            node,
            colon,
            item: u.atom("item"),
            expansion: u.atom("expansion"),
            none: u.atom("none"),
        }),
        Vec::new(),
    )
}

fn is_missing(u: &Universe, diagnostics: &[TermId]) -> bool {
    diagnostics.iter().all(|d| {
        args_of(u, *d, "diagnostic", 3)
            .and_then(|args| u.functor(args[2]))
            .is_some_and(|(name, _)| name == "missing_protocol_relation")
    })
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

/// A writer no item-zero atom names can still name an atom it needs anywhere
/// in the graph: any positive `syntax_atom` goal with a literal name.
fn atom_anywhere_name(u: &mut Universe, p: &Protocol, rule: &Rule) -> Option<TermId> {
    let text = rule.body.iter().find_map(|goal| {
        if goal.polarity != Polarity::Positive || goal.rel != p.atom || goal.args.len() != 2 {
            return None;
        }
        const_text(u, &goal.args[1])
    })?;
    Some(atom_of_text(u, text))
}

/// `macro_dispatch/4`: when every claim and diagnostic writer names an atom it
/// needs, the absence of those names in the graph proves an empty claim and
/// diagnostic set without running the evaluator. A name taken from an
/// item-zero edge must sit at item zero; any other name anywhere.
pub fn macro_dispatch(u: &mut Universe, p: &Protocol, rules: &[Rule], g: &Graph) -> Dispatch {
    let writers: Vec<&Rule> = rules
        .iter()
        .filter(|r| r.rel == p.claim || Some(r.rel) == p.diagnostic)
        .collect();
    let mut heads: HashSet<TermId> = HashSet::new();
    let mut anywhere: HashSet<TermId> = HashSet::new();
    for rule in &writers {
        if let Some(name) = claim_rule_name(u, p, rule) {
            heads.insert(name);
        } else if let Some(name) = atom_anywhere_name(u, p, rule) {
            anywhere.insert(name);
        } else {
            return Dispatch::Unknown;
        }
    }
    if g.names.values().any(|name| anywhere.contains(name)) {
        return Dispatch::Present;
    }
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
            if heads.contains(name) {
                return Dispatch::Present;
            }
        }
    }
    Dispatch::Absent
}

/// The relation and the per-argument tag list of one graph row shape.
pub fn seed_shape(
    p: &Protocol,
    name: &str,
    arity: usize,
) -> Option<(TermId, &'static [&'static str])> {
    const SOURCE: &[&str] = &[
        "ref", "const", "const", "const", "const", "const", "const", "const",
    ];
    Some(match (name, arity) {
        ("node", 1) => (p.node, &["ref"]),
        (":", 4) => (p.colon, &["ref", "const", "raw", "const"]),
        ("syntax_frontier", 2) => (p.frontier, &["const", "ref"]),
        ("syntax_form", 1) => (p.form, &["ref"]),
        ("syntax_atom", 2) => (p.atom, &["ref", "text"]),
        ("syntax_literal", 2) => (p.literal, &["ref", "const"]),
        ("syntax_variable", 3) => (p.variable, &["ref", "ref", "text"]),
        ("source", 8) => (p.source, SOURCE),
        _ => return None,
    })
}

/// The relation and arity of every graph row shape `seed_shape` maps: the
/// sources the macro IR declares, so a unit's rows reach it through `apply`.
pub fn seed_relations(p: &Protocol) -> Vec<(TermId, usize)> {
    [
        ("node", 1),
        (":", 4),
        ("syntax_frontier", 2),
        ("syntax_form", 1),
        ("syntax_atom", 2),
        ("syntax_literal", 2),
        ("syntax_variable", 3),
        ("source", 8),
    ]
    .iter()
    .filter_map(|&(name, arity)| seed_shape(p, name, arity).map(|(rel, _)| (rel, arity)))
    .collect()
}

/// Compound shapes the reader writes into graph rows: node ids (`reader_node`,
/// `src/_0_read/_2_reader.rs`; `expansion_node`, `src/_0_read/_4_expand.rs`), variable
/// identities, and the `tree_sitter_query` and `symbol` literal payloads. The macro IR declares
/// a constructor for each, so a unit's terms install without lowering again.
pub const READER_SHAPES: [(&str, usize); 5] = [
    ("reader_node", 2),
    ("expansion_node", 4),
    ("variable", 2),
    ("tree_sitter_query", 1),
    ("symbol", 1),
];

/// `raw` passes an already-tagged argument through; `text` interns the atom's
/// text before tagging it.
pub fn seed_cell(u: &mut Universe, tag: &str, arg: TermId) -> TermId {
    match tag {
        "ref" => wrap_ref(u, arg),
        "raw" => arg,
        "text" => {
            let text = text_of_atom(u, arg);
            tagged(u, "const", text)
        }
        _ => tagged(u, "const", arg),
    }
}

/// `syntax_seed_calls/3`: one seed row per graph row, tagged for the
/// evaluator.
pub fn syntax_seeds(u: &mut Universe, p: &Protocol, rows: &[TermId]) -> Vec<Row> {
    let mut out = Vec::with_capacity(rows.len());
    for &row in rows {
        let Some((name, args)) = u.functor(row).map(|(n, a)| (n.to_string(), a.to_vec())) else {
            continue;
        };
        let Some((rel, tags)) = seed_shape(p, &name, args.len()) else {
            continue;
        };
        let mut cells = Vec::with_capacity(args.len());
        for (arg, tag) in args.iter().zip(tags) {
            cells.push(seed_cell(u, tag, *arg));
        }
        out.push(Row { rel, args: cells });
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

/// The closure grouped by relation; `Rows` is what every reader below reads.
type Rows<'a> = HashMap<TermId, Vec<&'a Row>>;

fn rows_of<'a, 'r>(by_rel: &'a Rows<'r>, rel: TermId) -> &'a [&'r Row] {
    by_rel.get(&rel).map_or(&[], |v| v.as_slice())
}

/// Each argument of an arity-checked row unwrapped from its tag; a row whose
/// argument carries a different tag is dropped.
pub fn untagged_rows(u: &Universe, rows: &[&Row], tags: &[&str]) -> Vec<Vec<TermId>> {
    rows.iter()
        .filter(|row| row.args.len() == tags.len())
        .filter_map(|row| {
            row.args
                .iter()
                .zip(tags)
                .map(|(a, tag)| u.unary(*a, tag))
                .collect()
        })
        .collect()
}

/// `macro_results/6`: the claims on active nodes and the ordered expansion
/// edges of those claims, read from `Available::results`.
pub fn macro_results(
    u: &mut Universe,
    p: &Protocol,
    available: &Available,
    active: &HashSet<TermId>,
) -> MacroResults {
    let mut by_rel: Rows = HashMap::new();
    for row in &available.results {
        by_rel.entry(row.rel).or_default().push(row);
    }
    let (claims, invocations) = active_claims(u, p, &by_rel, active);
    let outputs = expansion_outputs(u, p, &by_rel, &invocations);
    let reported = reported_rows(u, p, &by_rel, active);
    MacroResults {
        claims,
        outputs,
        reported,
    }
}

pub struct MacroResults {
    pub claims: Vec<Claim>,
    pub outputs: Vec<Output>,
    /// `syntax_diagnostic(Node, Payload)` on active nodes, sorted, payload
    /// text turned into an atom.
    pub reported: Vec<(TermId, TermId)>,
}

fn reported_rows(
    u: &mut Universe,
    p: &Protocol,
    by_rel: &Rows,
    active: &HashSet<TermId>,
) -> Vec<(TermId, TermId)> {
    let Some(rel) = p.diagnostic else {
        return Vec::new();
    };
    let mut terms: Vec<TermId> = Vec::new();
    for cells in untagged_rows(u, rows_of(by_rel, rel), &["ref", "const"]) {
        if active.contains(&cells[0]) {
            let payload = atom_of_text(u, cells[1]);
            terms.push(u.compound("reported", vec![cells[0], payload]));
        }
    }
    sort_terms(u, &mut terms);
    terms
        .iter()
        .map(|t| {
            let args = u.functor(*t).unwrap().1;
            (args[0], args[1])
        })
        .collect()
}

/// What one closure row of a protocol relation is to the expander.
enum Read {
    /// an untagged syntax graph row
    Syntax(TermId),
    /// a claim, expansion edge or diagnostic row, kept tagged
    Result,
    Ignored,
}

/// The closure as the expander reads it, kept in step with each delta.
/// `node` and `item` rows in `graph` are unfiltered; `identity` filters them.
#[derive(Default)]
pub struct Available {
    /// tagged protocol row to its summed engine weight
    weights: HashMap<Row, i64>,
    /// untagged row to the number of present tagged rows that give it
    counts: HashMap<TermId, i64>,
    /// node to the number of present form, atom, literal and variable rows
    /// whose first argument names it
    identities: HashMap<TermId, i64>,
    /// present claim, expansion edge and diagnostic rows
    pub results: HashSet<Row>,
    pub graph: Graph,
    /// present untagged rows the expanded graph does not hold
    pub extra: HashSet<TermId>,
}

impl Available {
    /// Fold one settle's signed rows in; `expanded` is the graph the seeds
    /// came from. Returns the rows read.
    pub fn apply(
        &mut self,
        u: &mut Universe,
        p: &Protocol,
        rows: &[SignedRow],
        expanded: &Graph,
    ) -> usize {
        let mut read = 0;
        let mut touched: HashMap<TermId, bool> = HashMap::new();
        for change in rows {
            let row = &change.row;
            if !p.reads(row.rel) {
                continue;
            }
            read += 1;
            let weight = self.weights.entry(row.clone()).or_default();
            let before = *weight > 0;
            *weight += change.weight;
            let after = *weight > 0;
            if *weight == 0 {
                self.weights.remove(row);
            }
            if before == after {
                continue;
            }
            let sign = if after { 1 } else { -1 };
            if let Some(node) = identity_of(u, p, row) {
                let count = self.identities.entry(node).or_default();
                *count += sign;
                if *count == 0 {
                    self.identities.remove(&node);
                }
            }
            match read_row(u, p, row) {
                Read::Syntax(term) => {
                    let count = self.counts.entry(term).or_default();
                    touched.entry(term).or_insert(*count > 0);
                    *count += sign;
                    if *count == 0 {
                        self.counts.remove(&term);
                    }
                }
                Read::Result => {
                    if after {
                        self.results.insert(row.clone());
                    } else {
                        self.results.remove(row);
                    }
                }
                Read::Ignored => {}
            }
        }
        let mut removed = Vec::new();
        let mut added = Vec::new();
        for (term, before) in touched {
            let after = self.counts.contains_key(&term);
            if before == after {
                continue;
            }
            if after {
                added.push(term);
                if !expanded.set.contains(&term) {
                    self.extra.insert(term);
                }
            } else {
                removed.push(term);
                self.extra.remove(&term);
            }
        }
        self.graph.apply(u, &removed, &added);
        read
    }

    /// The expanded graph moved; keep `extra` its complement in `graph`.
    pub fn expanded_moved(&mut self, removed: &[TermId], added: &[TermId]) {
        for row in added {
            self.extra.remove(row);
        }
        for row in removed {
            if self.graph.set.contains(row) {
                self.extra.insert(*row);
            }
        }
    }

    /// `syntax_identities`: the node has a form, atom, literal or variable row.
    pub fn identity(&self, node: TermId) -> bool {
        self.identities.contains_key(&node)
    }
}

fn identity_of(u: &Universe, p: &Protocol, row: &Row) -> Option<TermId> {
    [(p.form, 1), (p.atom, 2), (p.literal, 2), (p.variable, 3)]
        .iter()
        .any(|&(rel, arity)| row.rel == rel && row.args.len() == arity)
        .then(|| u.unary(row.args[0], "ref"))
        .flatten()
}

/// One row of `available_rows`: each argument unwrapped from its tag, a
/// trailing text turned into an atom.
fn read_row(u: &mut Universe, p: &Protocol, row: &Row) -> Read {
    if row.rel == p.claim || Some(row.rel) == p.diagnostic {
        return Read::Result;
    }
    if row.rel == p.colon {
        if row.args.len() != 4 {
            return Read::Ignored;
        }
        if is_const_atom(u, row.args[1], "expansion") {
            return Read::Result;
        }
        if !is_const_atom(u, row.args[1], "item") {
            return Read::Ignored;
        }
        let owner = u.unary(row.args[0], "ref");
        let target = u.unary(row.args[2], "ref");
        let ordinal = u.unary(row.args[3], "const");
        let (Some(owner), Some(target), Some(ordinal)) = (owner, target, ordinal) else {
            return Read::Ignored;
        };
        let item = p.item;
        let reference = wrap_ref(u, target);
        return Read::Syntax(u.compound(":", vec![owner, item, reference, ordinal]));
    }
    const SOURCE: &[&str] = &[
        "ref", "const", "const", "const", "const", "const", "const", "const",
    ];
    let (name, tags, text_tail): (&str, &[&str], bool) = if row.rel == p.form {
        ("syntax_form", &["ref"], false)
    } else if row.rel == p.atom {
        ("syntax_atom", &["ref", "const"], true)
    } else if row.rel == p.literal {
        ("syntax_literal", &["ref", "const"], false)
    } else if row.rel == p.variable {
        ("syntax_variable", &["ref", "ref", "const"], true)
    } else if row.rel == p.source {
        ("source", SOURCE, false)
    } else if row.rel == p.node {
        ("node", &["ref"], false)
    } else {
        return Read::Ignored;
    };
    let Some(mut cells) = untagged_rows(u, &[row], tags).pop() else {
        return Read::Ignored;
    };
    if text_tail {
        let text = cells.pop().unwrap();
        cells.push(atom_of_text(u, text));
    }
    Read::Syntax(u.compound(name, cells))
}

/// The claims landing on nodes the rewriter has not already consumed, with the
/// invocation set the expansion edges are filtered against.
pub fn active_claims(
    u: &mut Universe,
    p: &Protocol,
    by_rel: &Rows,
    active: &HashSet<TermId>,
) -> (Vec<Claim>, HashSet<TermId>) {
    let mut claim_terms: Vec<TermId> = Vec::new();
    for row in rows_of(by_rel, p.claim) {
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
    let claims = claim_terms
        .iter()
        .map(|c| {
            let args = u.functor(*c).unwrap().1;
            Claim {
                invocation: args[0],
                identity: args[1],
            }
        })
        .collect();
    (claims, invocations)
}

/// The ordered `expansion` edges of the claimed invocations.
pub fn expansion_outputs(
    u: &mut Universe,
    p: &Protocol,
    by_rel: &Rows,
    invocations: &HashSet<TermId>,
) -> Vec<Output> {
    let mut output_terms: Vec<TermId> = Vec::new();
    for row in rows_of(by_rel, p.colon) {
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
    output_terms
        .iter()
        .map(|o| {
            let args = u.functor(*o).unwrap().1;
            Output {
                invocation: args[0],
                output: args[1],
                ordinal: args[2],
            }
        })
        .collect()
}

fn is_const_atom(u: &Universe, tagged: TermId, name: &str) -> bool {
    match u.unary(tagged, "const") {
        Some(inner) => is_atom_named(u, inner, name),
        None => false,
    }
}
