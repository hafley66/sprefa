//! `DL8_MACROTIME=engine`: one unit's wave loop is one `next` LetRec (`macrotime/3_waves.dl7`),
//! entered once and read once. `wave_limit` from the settle is `expansion_round_limit`.

use super::_0_rows::{atom_of_text, is_atom_named, macrotime_diagnostic, sort_terms, Graph};
use super::_2_protocol::{
    macro_dispatch, macro_protocol, resolve, seed_cell, Dispatch, MacroProgram, Protocol,
};
use super::_4_expand::{diagnostic_terms, MacroSession, Wave, WAVE_LIMIT};
use crate::_6_eval::ir_eval::{shape_of, Applied, ApplyError, SignedRow};
use crate::_6_eval::program::next_inner;
use crate::_6_eval::{Program, Row, Rule, Term, TermId, Universe};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum MacrotimeMode {
    Rust,
    Engine,
}

static MACROTIME_MODE: OnceLock<Result<MacrotimeMode, String>> = OnceLock::new();

fn read_macrotime_mode() -> Result<MacrotimeMode, String> {
    match std::env::var("DL8_MACROTIME").as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("rust") => Ok(MacrotimeMode::Rust),
        Ok("engine") => Ok(MacrotimeMode::Engine),
        Ok(other) => Err(format!(
            "unknown DL8_MACROTIME {other:?}; expected rust or engine"
        )),
        Err(error) => Err(format!("DL8_MACROTIME {error}; expected rust or engine")),
    }
}

/// The binary reads `DL8_MACROTIME` here, once, at start; a library caller that never did
/// reads it on first use.
pub fn init_macrotime_mode() -> Result<MacrotimeMode, String> {
    MACROTIME_MODE.get_or_init(read_macrotime_mode).clone()
}

pub fn macrotime_mode() -> MacrotimeMode {
    init_macrotime_mode().unwrap_or(MacrotimeMode::Rust)
}

/// Graph row name and arity, the suffix of its `graph_*` relation, and the seed tag of each
/// `graph_*` column. An item edge drops its `item` label.
pub const GRAPH_KINDS: [(&str, usize, &str, &[&str]); 8] = [
    ("node", 1, "node", &["ref"]),
    (":", 4, "item", &["ref", "raw", "const"]),
    ("syntax_frontier", 2, "frontier", &["const", "ref"]),
    ("syntax_form", 1, "form", &["ref"]),
    ("syntax_atom", 2, "atom", &["ref", "text"]),
    ("syntax_literal", 2, "literal", &["ref", "const"]),
    ("syntax_variable", 3, "variable", &["ref", "ref", "text"]),
    (
        "source",
        8,
        "source",
        &[
            "ref", "const", "const", "const", "const", "const", "const", "const",
        ],
    ),
];

/// Relations the expander reads back, with their arity.
const READS: [(&str, usize); 12] = [
    ("cur_wave", 1),
    ("replaced", 3),
    ("expansion_claim", 3),
    ("expansion_output", 5),
    ("rewrite_reported", 2),
    ("rewrite_conflict", 2),
    ("rewrite_non_dense", 1),
    ("rewrite_unknown", 2),
    ("rewrite_non_dense_children", 1),
    ("rewrite_missing_node", 1),
    ("rewrite_non_dense_frontier", 1),
    ("rewrite_cycle", 1),
];

/// The relations of `macrotime/3_waves.dl7` the expander seeds and reads.
pub struct WaveProtocol {
    /// The `graph_*` relation of each `GRAPH_KINDS` entry, in order.
    pub graph: Vec<TermId>,
    /// Each `READS` name to its relation.
    pub reads: HashMap<&'static str, TermId>,
}

impl WaveProtocol {
    pub fn read(&self, name: &str) -> TermId {
        self.reads[name]
    }

    pub fn reads(&self, rel: TermId) -> bool {
        self.graph.contains(&rel) || self.reads.values().any(|r| *r == rel)
    }

    pub fn relations(&self) -> Vec<TermId> {
        self.graph
            .iter()
            .chain(self.reads.values())
            .copied()
            .collect()
    }

    /// The `graph_*` relations with their arities: the sources the macro IR declares.
    pub fn sources(&self) -> Vec<(TermId, usize)> {
        self.graph
            .iter()
            .zip(GRAPH_KINDS.iter())
            .map(|(rel, kind)| (*rel, kind.3.len()))
            .collect()
    }
}

/// Resolve every relation of `WaveProtocol` by its declaration, as `macro_protocol` resolves
/// the `syntax_*` relations.
pub fn wave_protocol(u: &mut Universe, mp: &MacroProgram) -> (Option<WaveProtocol>, Vec<TermId>) {
    let mut diagnostics = Vec::new();
    let mut graph = Vec::new();
    for (_, _, suffix, tags) in GRAPH_KINDS {
        let (relation, mut d) = resolve(u, mp, &format!("graph_{suffix}"), tags.len() as i64);
        graph.push(relation);
        diagnostics.append(&mut d);
    }
    let mut reads = Vec::new();
    for (name, arity) in READS {
        let (relation, mut d) = resolve(u, mp, name, arity as i64);
        reads.push((name, relation));
        diagnostics.append(&mut d);
    }
    if !diagnostics.is_empty() {
        return (None, diagnostics);
    }
    let graph = graph
        .into_iter()
        .map(|inner| u.compound("ref", vec![inner.unwrap()]))
        .collect();
    let reads = reads
        .into_iter()
        .map(|(name, inner)| (name, u.compound("ref", vec![inner.unwrap()])))
        .collect();
    (Some(WaveProtocol { graph, reads }), Vec::new())
}

/// `macro_rules` plus every `next` rule and every writer of a read relation, closed over
/// their non-input dependencies.
pub fn wave_rules(u: &Universe, p: &Protocol, wp: &WaveProtocol, rules: &[Rule]) -> Vec<usize> {
    let roots = p.roots();
    let read: HashSet<TermId> = wp.relations().into_iter().collect();
    let mut selected: HashSet<usize> = HashSet::new();
    let ground_label = |arg: &crate::_6_eval::Arg, name: &str| match arg {
        crate::_6_eval::Arg::Ground(t) => u
            .unary(*t, "const")
            .is_some_and(|inner| is_atom_named(u, inner, name)),
        _ => false,
    };
    for (i, rule) in rules.iter().enumerate() {
        let is_root = roots.contains(&rule.rel)
            || rule.rel == p.node
            || (rule.rel == p.colon
                && rule.head.len() == 4
                && (ground_label(&rule.head[1], "item")
                    || ground_label(&rule.head[1], "expansion")))
            || next_inner(u, rule.rel).is_some()
            || read.contains(&rule.rel);
        if is_root {
            selected.insert(i);
        }
    }
    let inputs = p.inputs();
    // Each pass adds at least one rule or stops.
    for _ in 0..=rules.len() {
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

/// One `graph_*` seed row per graph row, tagged for the evaluator.
pub fn wave_seeds(u: &mut Universe, wp: &WaveProtocol, rows: &[TermId]) -> Vec<Row> {
    let mut out = Vec::with_capacity(rows.len());
    for &row in rows {
        let Some((name, args)) = u.functor(row).map(|(n, a)| (n.to_string(), a.to_vec())) else {
            continue;
        };
        let Some(kind) = GRAPH_KINDS
            .iter()
            .position(|(n, arity, _, _)| *n == name && *arity == args.len())
        else {
            continue;
        };
        let args = if name == ":" {
            if !is_atom_named(u, args[1], "item") {
                continue;
            }
            vec![args[0], args[2], args[3]]
        } else {
            args
        };
        let cells = args
            .iter()
            .zip(GRAPH_KINDS[kind].3)
            .map(|(arg, tag)| seed_cell(u, tag, *arg))
            .collect();
        out.push(Row {
            rel: wp.graph[kind],
            args: cells,
        });
    }
    out
}

/// A `ref` or `const` cell untagged.
fn untag(u: &Universe, cell: TermId) -> TermId {
    u.unary(cell, "ref")
        .or_else(|| u.unary(cell, "const"))
        .unwrap_or(cell)
}

/// A claim's macro identity: a constant untagged, a reference kept tagged.
fn identity(u: &Universe, cell: TermId) -> TermId {
    u.unary(cell, "const").unwrap_or(cell)
}

/// The graph row one `graph_*` row names.
fn graph_row(u: &mut Universe, kind: usize, row: &Row) -> TermId {
    let (name, _, _, tags) = GRAPH_KINDS[kind];
    let mut args: Vec<TermId> = Vec::with_capacity(tags.len());
    for (cell, tag) in row.args.iter().zip(tags) {
        let value = untag(u, *cell);
        args.push(match *tag {
            "text" => atom_of_text(u, value),
            _ => value,
        });
    }
    if name == ":" {
        let item = u.atom("item");
        let target = u.compound("ref", vec![args[1]]);
        return u.compound(":", vec![args[0], item, target, args[2]]);
    }
    u.compound(name, args)
}

/// The rows the engine holds for the read relations: `base` plus every delta since.
fn present(session: &MacroSession, wp: &WaveProtocol) -> HashMap<TermId, Vec<Row>> {
    let mut weights: HashMap<&Row, i64> = HashMap::new();
    for change in session.base.iter().filter(|c| wp.reads(c.row.rel)) {
        *weights.entry(&change.row).or_default() += change.weight;
    }
    for (row, weight) in &session.net {
        *weights.entry(row).or_default() += weight;
    }
    let mut by_rel: HashMap<TermId, Vec<Row>> = HashMap::new();
    for (row, weight) in weights {
        if weight > 0 {
            by_rel.entry(row.rel).or_default().push(row.clone());
        }
    }
    by_rel
}

/// What the settled waves say: the macros' own diagnostics, then the rewrite's, then a cycle;
/// otherwise the graph and its origin rows.
fn read_result(
    u: &mut Universe,
    wp: &WaveProtocol,
    rows: &HashMap<TermId, Vec<Row>>,
) -> (Vec<TermId>, Vec<TermId>, Vec<TermId>, i64) {
    let of = |name: &str| -> Vec<Row> { rows.get(&wp.read(name)).cloned().unwrap_or_default() };
    let wave = of("cur_wave")
        .first()
        .and_then(|row| u.as_int(untag(u, row.args[0])))
        .unwrap_or(0);

    let mut reported: Vec<TermId> = Vec::new();
    for row in of("rewrite_reported") {
        let node = untag(u, row.args[0]);
        let text = untag(u, row.args[1]);
        let payload = atom_of_text(u, text);
        reported.push(u.compound("reported", vec![node, payload]));
    }
    if !reported.is_empty() {
        sort_terms(u, &mut reported);
        let diagnostics = reported
            .iter()
            .map(|t| {
                let args = u.functor(*t).unwrap().1.to_vec();
                macrotime_diagnostic(u, args[0], args[1])
            })
            .collect();
        return (Vec::new(), Vec::new(), diagnostics, wave);
    }

    let mut diagnostics: Vec<TermId> = Vec::new();
    let mut conflicts: HashMap<TermId, Vec<TermId>> = HashMap::new();
    for row in of("rewrite_conflict") {
        let node = untag(u, row.args[0]);
        let macro_identity = identity(u, row.args[1]);
        conflicts.entry(node).or_default().push(macro_identity);
    }
    for (node, mut macros) in conflicts {
        sort_terms(u, &mut macros);
        let list = u.list(&macros);
        let payload = u.compound("conflicting_macro_claims", vec![list]);
        diagnostics.push(macrotime_diagnostic(u, node, payload));
    }
    for row in of("rewrite_non_dense") {
        let node = untag(u, row.args[0]);
        let mut ordinals = non_dense_ordinals(u, rows, wp, node);
        ordinals.sort_by(|a, b| u.cmp(*a, *b));
        let list = u.list(&ordinals);
        let payload = u.compound("non_dense_expansion_ordinals", vec![list]);
        diagnostics.push(macrotime_diagnostic(u, node, payload));
    }
    for row in of("rewrite_unknown") {
        let node = untag(u, row.args[0]);
        let output = untag(u, row.args[1]);
        let payload = u.compound("unknown_expansion_output", vec![output]);
        diagnostics.push(macrotime_diagnostic(u, node, payload));
    }
    for row in of("rewrite_non_dense_children") {
        let owner = untag(u, row.args[0]);
        let payload = u.atom("non_dense_child_ordinals");
        diagnostics.push(macrotime_diagnostic(u, owner, payload));
    }
    for row in of("rewrite_missing_node") {
        let node = untag(u, row.args[0]);
        let payload = u.atom("missing_node_row");
        diagnostics.push(macrotime_diagnostic(u, node, payload));
    }
    if !of("rewrite_non_dense_frontier").is_empty() {
        let none = u.atom("none");
        let payload = u.atom("non_dense_frontier_ordinals");
        diagnostics.push(macrotime_diagnostic(u, none, payload));
    }
    if !diagnostics.is_empty() {
        sort_terms(u, &mut diagnostics);
        return (Vec::new(), Vec::new(), diagnostics, wave);
    }
    if let Some(row) = of("rewrite_cycle").first() {
        let none = u.atom("none");
        let cycle_wave = untag(u, row.args[0]);
        let payload = u.compound("expansion_cycle", vec![cycle_wave]);
        return (
            Vec::new(),
            Vec::new(),
            vec![macrotime_diagnostic(u, none, payload)],
            wave,
        );
    }

    let mut graph = Vec::new();
    for (kind, rel) in wp.graph.iter().enumerate() {
        for row in rows.get(rel).into_iter().flatten() {
            graph.push(graph_row(u, kind, row));
        }
    }
    sort_terms(u, &mut graph);
    let mut origin = Vec::new();
    for row in of("expansion_claim") {
        let node = untag(u, row.args[0]);
        let macro_identity = identity(u, row.args[1]);
        let at = untag(u, row.args[2]);
        origin.push(u.compound("expansion_claim", vec![node, macro_identity, at]));
    }
    for row in &of("expansion_output") {
        let node = untag(u, row.args[0]);
        let macro_identity = identity(u, row.args[1]);
        let cells: Vec<TermId> = row.args[2..].iter().map(|c| untag(u, *c)).collect();
        origin.push(u.compound(
            "expansion_output",
            vec![node, macro_identity, cells[0], cells[1], cells[2]],
        ));
    }
    sort_terms(u, &mut origin);
    (graph, origin, Vec::new(), wave)
}

/// The output ordinals of `node` in the stop wave, duplicates kept.
fn non_dense_ordinals(
    u: &mut Universe,
    rows: &HashMap<TermId, Vec<Row>>,
    wp: &WaveProtocol,
    node: TermId,
) -> Vec<TermId> {
    rows.get(&wp.read("replaced"))
        .into_iter()
        .flatten()
        .filter(|row| untag(u, row.args[0]) == node)
        .map(|row| untag(u, row.args[2]))
        .collect()
}

impl MacroSession {
    /// One settle; the read relations' rows fold into `net`.
    fn settle_waves(
        &mut self,
        u: &mut Universe,
        wp: &WaveProtocol,
        changes: &[SignedRow],
    ) -> Result<Applied, ApplyError> {
        let applied = self.ir.apply(u, changes)?;
        for change in &applied.rows {
            if !wp.reads(change.row.rel) {
                continue;
            }
            let weight = self.net.entry(change.row.clone()).or_default();
            *weight += change.weight;
            if *weight == 0 {
                self.net.remove(&change.row);
            }
        }
        Ok(applied)
    }

    /// The prepared IR lacks a source or constructor for a row of `seeds`: declare every
    /// compound shape they hold, prepare again and enter them.
    fn reprepare_waves(
        &mut self,
        u: &mut Universe,
        wp: &WaveProtocol,
        program: &Program,
        seeds: &[Row],
    ) -> Result<Applied, Vec<TermId>> {
        self.reprepares += 1;
        let mut todo: Vec<TermId> = seeds
            .iter()
            .flat_map(|row| row.args.iter().copied())
            .collect();
        let mut walked = HashSet::new();
        let mut added = Vec::new();
        while let Some(term) = todo.pop() {
            if !walked.insert(term) {
                continue;
            }
            if let Term::Compound(_, args) = u.get(term) {
                todo.extend(args.iter().copied());
                let shape = shape_of(u, term);
                if !self.ir.declares(&shape) && self.declared.shapes.insert(shape.clone()) {
                    added.push(shape.0);
                }
            }
        }
        tracing::info!(target: "dl8::macrotime", added = ?added, "macrotime declared shapes");
        self.prepare(u, program)?;
        self.dirty = true;
        let changes: Vec<SignedRow> = seeds
            .iter()
            .map(|row| SignedRow {
                row: row.clone(),
                weight: 1,
            })
            .collect();
        match self.settle_waves(u, wp, &changes) {
            Ok(applied) => Ok(applied),
            Err(ApplyError::Unprepared(reason)) => {
                self.program = None;
                let reason = u.string(reason);
                let payload = u.compound("unprepared_after_reprepare", vec![reason]);
                let none = u.atom("none");
                Err(vec![macrotime_diagnostic(u, none, payload)])
            }
            Err(ApplyError::Failed(diagnostic)) => {
                self.program = None;
                Err(failed_terms(u, diagnostic))
            }
        }
    }

    /// Wave 0 enters at +1 after the last unit leaves (rewind, or -1 in the same settle).
    fn enter_waves(
        &mut self,
        u: &mut Universe,
        wp: &WaveProtocol,
        program: &Program,
        seeds: &[Row],
    ) -> Result<Applied, Vec<TermId>> {
        let sources = wp.sources();
        let prepared = self
            .program
            .as_ref()
            .is_some_and(|p| p.rules == program.rules && p.seeds == program.seeds)
            && self.declared.sources == sources;
        if !prepared {
            self.declared.sources = sources;
            self.prepare(u, program)?;
        }
        let rewound = self.rewinds && self.dirty;
        if rewound {
            if let Err(diagnostic) = self.ir.rewind(u) {
                self.program = None;
                return Err(diagnostic_terms(u, &[diagnostic]));
            }
            self.net.clear();
            self.dirty = false;
        }
        self.dirty = true;
        let mut changes: Vec<SignedRow> = self
            .resident
            .iter()
            .map(|row| SignedRow {
                row: row.clone(),
                weight: -1,
            })
            .collect();
        let retracted = changes.len();
        changes.extend(seeds.iter().map(|row| SignedRow {
            row: row.clone(),
            weight: 1,
        }));
        match self.settle_waves(u, wp, &changes) {
            Ok(applied) => {
                self.resident.clear();
                tracing::info!(target: "dl8::macrotime", retracted, rewound, "macrotime enter waves");
                Ok(applied)
            }
            Err(ApplyError::Unprepared(reason)) => {
                tracing::info!(target: "dl8::macrotime", reason, "macrotime reprepare");
                self.reprepare_waves(u, wp, program, seeds)
            }
            Err(ApplyError::Failed(diagnostic)) => {
                self.program = None;
                Err(failed_terms(u, diagnostic))
            }
        }
    }
}

/// A failed settle as diagnostics; the wave LetRec's limit is the expander's round limit.
fn failed_terms(u: &mut Universe, diagnostic: crate::_6_eval::Diagnostic) -> Vec<TermId> {
    if u.functor(diagnostic.payload)
        .is_some_and(|(name, _)| name == "wave_limit")
    {
        let none = u.atom("none");
        let limit = u.int(WAVE_LIMIT);
        let payload = u.compound("expansion_round_limit", vec![limit]);
        return vec![macrotime_diagnostic(u, none, payload)];
    }
    diagnostic_terms(u, &[diagnostic])
}

/// `expand_rows` for `DL8_MACROTIME=engine`.
pub fn expand_rows_engine(
    u: &mut Universe,
    rows: Vec<TermId>,
    macro_program: &MacroProgram,
    session: &mut MacroSession,
    fx: &mut dyn FnMut(Wave),
) -> (Vec<TermId>, Vec<TermId>, Vec<TermId>) {
    let (protocol, diagnostics) = macro_protocol(u, macro_program);
    let Some(protocol) = protocol else {
        return (Vec::new(), Vec::new(), diagnostics);
    };
    let (wp, diagnostics) = wave_protocol(u, macro_program);
    let Some(wp) = wp else {
        return (Vec::new(), Vec::new(), diagnostics);
    };
    let mut canonical = rows;
    sort_terms(u, &mut canonical);
    let graph = Graph::build(u, canonical);
    if let Dispatch::Absent = macro_dispatch(u, &protocol, &macro_program.program.rules, &graph) {
        fx(Wave::Settled {
            wave: 0,
            rows: graph.rows.len(),
        });
        return (graph.rows, Vec::new(), Vec::new());
    }
    let selected = wave_rules(u, &protocol, &wp, &macro_program.program.rules);
    let program = Program {
        rules: selected
            .iter()
            .map(|i| macro_program.program.rules[*i].clone())
            .collect(),
        seeds: macro_program.program.seeds.clone(),
        ..Program::default()
    };
    let seeds = wave_seeds(u, &wp, &graph.rows);
    let settled = session.enter_waves(u, &wp, &program, &seeds);
    if session.program.is_some() && !session.rewinds {
        session.resident = seeds;
    }
    let applied = match settled {
        Ok(applied) if applied.diagnostics.is_empty() => applied,
        Ok(applied) => {
            return (
                Vec::new(),
                Vec::new(),
                diagnostic_terms(u, &applied.diagnostics),
            )
        }
        Err(diagnostics) => return (Vec::new(), Vec::new(), diagnostics),
    };
    let present = present(session, &wp);
    let read: usize = present.values().map(Vec::len).sum();
    fx(Wave::Evaluated {
        wave: 0,
        sent: applied.sent,
        changed: applied.rows.len(),
        read,
    });
    let (graph_rows, origin, diagnostics, wave) = read_result(u, &wp, &present);
    tracing::info!(
        target: "dl8::macrotime",
        waves = wave + 1,
        sent = applied.sent,
        changed = applied.rows.len(),
        read,
        intern_read = applied.intern_read,
        rows = graph_rows.len(),
        fingerprint = session.ir.fingerprint(),
        prepares = session.prepares,
        reprepares = session.reprepares,
        "macrotime unit waves"
    );
    if !diagnostics.is_empty() {
        return (Vec::new(), Vec::new(), diagnostics);
    }
    fx(Wave::Settled {
        wave,
        rows: graph_rows.len(),
    });
    (graph_rows, origin, Vec::new())
}
