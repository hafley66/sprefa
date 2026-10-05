//! Port of `v7/src/1_libtime/1_syntax_expander.pl` `expand_syntax/5`. The
//! macro program is installed once per `MacroSession`; a unit enters as its
//! graph rows, each wave settles the rows the previous rewrite removed and
//! added, then rewrites every claimed node, and the unit's rows leave when it
//! is done. The loop ends when a wave claims nothing, repeats a row set (by
//! hash), or reaches the wave limit.

use super::_0_rows::{
    macrotime_diagnostic, row_hash, rows_of_terms, sort_terms, terms_of_rows, Graph,
};
use super::_2_protocol::{
    macro_dispatch, macro_protocol, macro_results, macro_rules, seed_relations, syntax_seeds,
    Available, Dispatch, MacroProgram, MacroResults, Protocol, READER_SHAPES,
};
use super::_3_rewrite::{reported_diagnostics, rewrite_active_graph};
use crate::_6_eval::ir_eval::{shape_of, Applied, ApplyError, IrSession, SignedRow};
use crate::_6_eval::ir_lower::Declared;
use crate::_6_eval::{Diagnostic, Program, Row, Term, TermId, Universe};
use crate::_7_effect::{reduce_then_apply, Slice};
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;

pub const WAVE_LIMIT: i64 = 64;

/// What one expansion wave did. The sink is the seam a later yieldable
/// macrotime would suspend on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Wave {
    /// `sent` seed rows settled, `changed` rows came back, `read` of them
    /// were protocol rows.
    Evaluated {
        wave: i64,
        sent: usize,
        changed: usize,
        read: usize,
    },
    Rewritten {
        wave: i64,
        claimed: usize,
        edges: usize,
        rows: usize,
    },
    Settled {
        wave: i64,
        rows: usize,
    },
}

#[tracing::instrument(skip_all, fields(rows = rows.len()))]
pub fn expand(
    u: &mut Universe,
    rows: &[Row],
    macro_program: &MacroProgram,
    session: &mut MacroSession,
    fx: &mut dyn FnMut(Wave),
) -> (Vec<Row>, Vec<Row>, Vec<TermId>) {
    let terms = terms_of_rows(u, rows);
    let (expanded, origin, diagnostics) = expand_terms(u, terms, macro_program, session, fx);
    (
        rows_of_terms(u, &expanded),
        rows_of_terms(u, &origin),
        diagnostics,
    )
}

/// The wave fixpoint as a reducer; the state is the row set being rewritten.
pub struct Expand<'a>(PhantomData<&'a ()>);

impl<'a> Slice for Expand<'a> {
    type State = Vec<TermId>;
    type Event = (&'a mut Universe, &'a MacroProgram, &'a mut MacroSession);
    type Output = (Vec<TermId>, Vec<TermId>, Vec<TermId>);
    type Effect = Wave;

    fn reduce(
        st: &mut Vec<TermId>,
        (u, macro_program, session): Self::Event,
        fx: &mut dyn FnMut(Wave),
    ) -> Self::Output {
        expand_rows(u, std::mem::take(st), macro_program, session, fx)
    }
}

pub fn expand_terms(
    u: &mut Universe,
    rows: Vec<TermId>,
    macro_program: &MacroProgram,
    session: &mut MacroSession,
    fx: &mut dyn FnMut(Wave),
) -> (Vec<TermId>, Vec<TermId>, Vec<TermId>) {
    let mut state = rows;
    let mut waves = Vec::new();
    reduce_then_apply::<Expand>(
        &mut state,
        (u, macro_program, session),
        &mut waves,
        |_, wave| fx(wave),
    )
}

/// The macrotime engine of one build: the macro rule cone lowered and
/// installed once, every unit a frontier on it. Units must not see each
/// other's rows: on an engine that rewinds, the next unit first returns the
/// engine to its mark after `base`; on one that does not, a unit's seed rows
/// leave in the first settle of the next unit. Rows hold `Universe` term ids,
/// so one session serves one universe.
pub struct MacroSession {
    pub ir: IrSession,
    /// The rule cone and macro seeds `ir` was prepared with.
    pub program: Option<Program>,
    /// The seed relations and term shapes the prepared IR declares.
    pub declared: Declared,
    /// What the macro program settled to before any unit entered: every
    /// unit's `Available` starts from it.
    pub base: Vec<SignedRow>,
    /// Seed rows of the last unit, still in the engine; on an engine that
    /// does not rewind, the next unit's first settle retracts them.
    pub resident: Vec<Row>,
    /// The engine rewinds (`IrSession::rewinds`): `prepare` marks it after
    /// `base` and every unit after the first enters on a rewound engine.
    pub rewinds: bool,
    /// A unit settled since the mark.
    pub dirty: bool,
    /// Protocol rows the engine holds above `base`: the sum of every delta
    /// since `base`.
    pub net: HashMap<Row, i64>,
    /// Installs: the first prepare, a changed program, a reprepare.
    pub prepares: usize,
    /// Prepares forced by a row or term the prepared IR has no source or
    /// constructor for (`macrotime reprepare`).
    pub reprepares: usize,
}

impl MacroSession {
    /// The `DL8_ENGINE` backend, declaring a constructor per `READER_SHAPES`.
    pub fn configured() -> Self {
        Self {
            ir: IrSession::configured(),
            program: None,
            declared: Declared {
                sources: Vec::new(),
                shapes: READER_SHAPES
                    .iter()
                    .map(|&(name, arity)| (format!("9:{arity}:{name}"), arity, false))
                    .collect(),
            },
            base: Vec::new(),
            resident: Vec::new(),
            rewinds: false,
            dirty: false,
            net: HashMap::new(),
            prepares: 0,
            reprepares: 0,
        }
    }

    /// Lower and install `program` with the declared sources and shapes,
    /// settle its own seeds into `base`, and mark the engine there when it
    /// rewinds.
    fn prepare(&mut self, u: &mut Universe, program: &Program) -> Result<(), Vec<TermId>> {
        self.program = None;
        self.base.clear();
        self.resident.clear();
        self.net.clear();
        self.dirty = false;
        let applied = self
            .ir
            .prepare(u, program, &self.declared)
            .map_err(|diagnostic| diagnostic_terms(u, &[diagnostic]))?;
        if !applied.diagnostics.is_empty() {
            return Err(diagnostic_terms(u, &applied.diagnostics));
        }
        self.rewinds = self.ir.rewinds();
        if self.rewinds {
            self.ir
                .mark(u)
                .map_err(|diagnostic| diagnostic_terms(u, &[diagnostic]))?;
        }
        self.base = applied.rows;
        self.program = Some(program.clone());
        self.prepares += 1;
        tracing::info!(
            target: "dl8::macrotime",
            fingerprint = self.ir.fingerprint(),
            base = self.base.len(),
            shapes = self.declared.shapes.len(),
            retire = if self.rewinds { "rewind" } else { "retract" },
            "macrotime prepare"
        );
        Ok(())
    }

    /// One settle; its protocol rows fold into `net`.
    fn settle(
        &mut self,
        u: &mut Universe,
        protocol: &Protocol,
        changes: &[SignedRow],
    ) -> Result<Applied, ApplyError> {
        let applied = self.ir.apply(u, changes)?;
        for change in &applied.rows {
            if !protocol.reads(change.row.rel) {
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

    /// The unit's graph rows enter at weight +1. The last unit's rows leave
    /// first: a rewinding engine returns to its mark and `net` empties; any
    /// other engine retracts them at -1 in the same settle. The returned rows
    /// are `net`: with `base` folded into `available` first, the unit reads
    /// exactly its own closure. The session prepares first when it holds no
    /// program or a different rule cone.
    fn enter(
        &mut self,
        u: &mut Universe,
        protocol: &Protocol,
        program: &Program,
        graph: &Graph,
        available: &mut Available,
    ) -> Result<Applied, Vec<TermId>> {
        let sources = seed_relations(protocol);
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
        changes.extend(signed_seeds(u, protocol, &graph.rows, 1));
        match self.settle(u, protocol, &changes) {
            Ok(applied) => {
                self.resident.clear();
                tracing::info!(target: "dl8::macrotime", retracted, rewound, "macrotime enter");
                *available = self.available(u, protocol, graph);
                Ok(Applied {
                    rows: self
                        .net
                        .iter()
                        .map(|(row, weight)| SignedRow {
                            row: row.clone(),
                            weight: *weight,
                        })
                        .collect(),
                    ..applied
                })
            }
            Err(ApplyError::Unprepared(reason)) => {
                tracing::info!(target: "dl8::macrotime", wave = 0, reason, "macrotime reprepare");
                self.reprepare(u, protocol, program, graph, available)
            }
            Err(ApplyError::Failed(diagnostic)) => {
                self.program = None;
                Err(diagnostic_terms(u, &[diagnostic]))
            }
        }
    }

    /// `base` folded into a fresh `Available` for the graph `expanded`.
    fn available(&self, u: &mut Universe, protocol: &Protocol, expanded: &Graph) -> Available {
        let mut available = Available::default();
        available.apply(u, protocol, &self.base, expanded);
        available
    }

    /// The prepared IR lacks a source or constructor for a row of `graph`:
    /// declare every compound shape the graph's seeds hold, prepare again and
    /// enter the whole graph.
    fn reprepare(
        &mut self,
        u: &mut Universe,
        protocol: &Protocol,
        program: &Program,
        graph: &Graph,
        available: &mut Available,
    ) -> Result<Applied, Vec<TermId>> {
        self.reprepares += 1;
        let seeds = syntax_seeds(u, protocol, &graph.rows);
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
        *available = self.available(u, protocol, graph);
        let changes: Vec<SignedRow> = seeds
            .into_iter()
            .map(|row| SignedRow { row, weight: 1 })
            .collect();
        match self.settle(u, protocol, &changes) {
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
                Err(diagnostic_terms(u, &[diagnostic]))
            }
        }
    }

    /// The unit is done: the engine holds its graph's seed rows until the
    /// next unit rewinds the engine or its first settle retracts them.
    fn leave(&mut self, u: &mut Universe, protocol: &Protocol, graph: &Graph) {
        if self.program.is_some() && !self.rewinds {
            self.resident = syntax_seeds(u, protocol, &graph.rows);
        }
    }
}

fn expand_rows(
    u: &mut Universe,
    rows: Vec<TermId>,
    macro_program: &MacroProgram,
    session: &mut MacroSession,
    fx: &mut dyn FnMut(Wave),
) -> (Vec<TermId>, Vec<TermId>, Vec<TermId>) {
    let (protocol, diagnostics) = macro_protocol(u, macro_program);
    let protocol = match protocol {
        Some(p) => p,
        None => return (Vec::new(), Vec::new(), diagnostics),
    };
    let mut canonical = rows;
    sort_terms(u, &mut canonical);
    let mut graph = Graph::build(u, canonical);
    if let Dispatch::Absent = macro_dispatch(u, &protocol, &macro_program.program.rules, &graph) {
        fx(Wave::Settled {
            wave: 0,
            rows: graph.rows.len(),
        });
        return (graph.rows, Vec::new(), Vec::new());
    }

    let selected = macro_rules(u, &protocol, &macro_program.program.rules);
    // The rule cone is fixed for the whole expansion; only the seeds move.
    let program = Program {
        rules: selected
            .iter()
            .map(|i| macro_program.program.rules[*i].clone())
            .collect(),
        seeds: macro_program.program.seeds.clone(),
        ..Program::default()
    };
    let expanded = expand_graph(u, &mut graph, &protocol, &program, session, fx);
    session.leave(u, &protocol, &graph);
    expanded
}

/// The waves of one unit on `session`; `graph` ends as the rows the engine holds.
fn expand_graph(
    u: &mut Universe,
    graph: &mut Graph,
    protocol: &Protocol,
    program: &Program,
    session: &mut MacroSession,
    fx: &mut dyn FnMut(Wave),
) -> (Vec<TermId>, Vec<TermId>, Vec<TermId>) {
    let none = u.atom("none");
    let mut seen: Vec<u64> = vec![graph.hash];
    let mut origin: Vec<TermId> = Vec::new();
    let mut available = Available::default();
    let mut active = graph.active_nodes();

    let unit_start = std::time::Instant::now();
    let mut evaluate_start = std::time::Instant::now();
    let mut settled = session.enter(u, protocol, program, graph, &mut available);
    for wave in 0..WAVE_LIMIT {
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
        let read = available.apply(u, protocol, &applied.rows, graph);
        tracing::info!(
            target: "dl8::macrotime",
            wave,
            sent = applied.sent,
            changed = applied.rows.len(),
            read,
            intern_read = applied.intern_read,
            evaluate_us = evaluate_start.elapsed().as_micros() as u64,
            "macrotime wave"
        );
        fx(Wave::Evaluated {
            wave,
            sent: applied.sent,
            changed: applied.rows.len(),
            read,
        });
        let MacroResults {
            claims,
            outputs,
            reported,
        } = macro_results(u, protocol, &available, &active);
        if !reported.is_empty() {
            return (Vec::new(), Vec::new(), reported_diagnostics(u, &reported));
        }
        if claims.is_empty() {
            tracing::info!(
                target: "dl8::macrotime",
                waves = wave + 1,
                rows = graph.rows.len(),
                unit_us = unit_start.elapsed().as_micros() as u64,
                fingerprint = session.ir.fingerprint(),
                prepares = session.prepares,
                reprepares = session.reprepares,
                "macrotime unit"
            );
            sort_terms(u, &mut origin);
            fx(Wave::Settled {
                wave,
                rows: graph.rows.len(),
            });
            let mut rows = graph.rows.clone();
            sort_terms(u, &mut rows);
            return (rows, origin, Vec::new());
        }
        let rewrite = rewrite_active_graph(u, graph, &available, &claims, &outputs, wave);
        if !rewrite.diagnostics.is_empty() {
            return (Vec::new(), Vec::new(), rewrite.diagnostics);
        }
        let mut hash = graph.hash;
        for row in &rewrite.removed {
            hash = hash.wrapping_sub(row_hash(*row));
        }
        for row in &rewrite.added {
            hash = hash.wrapping_add(row_hash(*row));
        }
        if seen.contains(&hash) {
            let wave_term = u.int(wave);
            let payload = u.compound("expansion_cycle", vec![wave_term]);
            let d = macrotime_diagnostic(u, none, payload);
            return (Vec::new(), Vec::new(), vec![d]);
        }
        graph.apply(u, &rewrite.removed, &rewrite.added);
        available.expanded_moved(&rewrite.removed, &rewrite.added);
        fx(Wave::Rewritten {
            wave,
            claimed: claims.len(),
            edges: outputs.len(),
            rows: graph.rows.len(),
        });
        origin.extend(rewrite.origin);
        seen.push(hash);
        active = rewrite.active;
        let mut changes = signed_seeds(u, protocol, &rewrite.removed, -1);
        changes.extend(signed_seeds(u, protocol, &rewrite.added, 1));
        evaluate_start = std::time::Instant::now();
        settled = match session.settle(u, protocol, &changes) {
            Ok(applied) => Ok(applied),
            Err(ApplyError::Unprepared(reason)) => {
                tracing::info!(target: "dl8::macrotime", wave, reason, "macrotime reprepare");
                session.reprepare(u, protocol, program, graph, &mut available)
            }
            Err(ApplyError::Failed(diagnostic)) => {
                session.program = None;
                Err(diagnostic_terms(u, &[diagnostic]))
            }
        };
    }
    let limit = u.int(WAVE_LIMIT);
    let payload = u.compound("expansion_round_limit", vec![limit]);
    let d = macrotime_diagnostic(u, none, payload);
    (Vec::new(), Vec::new(), vec![d])
}

fn signed_seeds(u: &mut Universe, p: &Protocol, rows: &[TermId], weight: i64) -> Vec<SignedRow> {
    syntax_seeds(u, p, rows)
        .into_iter()
        .map(|row| SignedRow { row, weight })
        .collect()
}

fn diagnostic_terms(u: &mut Universe, diagnostics: &[Diagnostic]) -> Vec<TermId> {
    let none = u.atom("none");
    diagnostics
        .iter()
        .map(|d| {
            let phase = u.atom(d.phase);
            u.compound("diagnostic", vec![phase, none, d.payload])
        })
        .collect()
}
