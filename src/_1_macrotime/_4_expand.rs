//! Port of `v7/src/1_libtime/1_syntax_expander.pl` `expand_syntax/5`. The
//! macro program is installed once per expansion; each wave settles the rows
//! the previous rewrite removed and added, then rewrites every claimed node.
//! The loop ends when a wave claims nothing, repeats a row set (by hash), or
//! reaches the wave limit.

use super::_0_rows::{
    macrotime_diagnostic, row_hash, rows_of_terms, sort_terms, terms_of_rows, Graph,
};
use super::_2_protocol::{
    macro_dispatch, macro_protocol, macro_results, macro_rules, syntax_seeds, Available, Dispatch,
    MacroProgram, MacroResults, Protocol,
};
use super::_3_rewrite::{reported_diagnostics, rewrite_active_graph};
use crate::_6_eval::ir_eval::{Applied, ApplyError, IrSession, SignedRow};
use crate::_6_eval::{Diagnostic, Program, Row, TermId, Universe};
use crate::_7_effect::{reduce_then_apply, Slice};
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
    fx: &mut dyn FnMut(Wave),
) -> (Vec<Row>, Vec<Row>, Vec<TermId>) {
    let terms = terms_of_rows(u, rows);
    let (expanded, origin, diagnostics) = expand_terms(u, terms, macro_program, fx);
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
    type Event = (&'a mut Universe, &'a MacroProgram);
    type Output = (Vec<TermId>, Vec<TermId>, Vec<TermId>);
    type Effect = Wave;

    fn reduce(
        st: &mut Vec<TermId>,
        (u, macro_program): Self::Event,
        fx: &mut dyn FnMut(Wave),
    ) -> Self::Output {
        expand_rows(u, std::mem::take(st), macro_program, fx)
    }
}

pub fn expand_terms(
    u: &mut Universe,
    rows: Vec<TermId>,
    macro_program: &MacroProgram,
    fx: &mut dyn FnMut(Wave),
) -> (Vec<TermId>, Vec<TermId>, Vec<TermId>) {
    let mut state = rows;
    let mut waves = Vec::new();
    reduce_then_apply::<Expand>(&mut state, (u, macro_program), &mut waves, |_, wave| {
        fx(wave)
    })
}

fn expand_rows(
    u: &mut Universe,
    rows: Vec<TermId>,
    macro_program: &MacroProgram,
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
    let mut program = Program {
        rules: selected
            .iter()
            .map(|i| macro_program.program.rules[*i].clone())
            .collect(),
        seeds: Vec::new(),
        ..Program::default()
    };
    let none = u.atom("none");
    let mut seen: Vec<u64> = vec![graph.hash];
    let mut origin: Vec<TermId> = Vec::new();
    let mut ir_session = IrSession::configured();
    let mut available = Available::default();
    let mut active = graph.active_nodes();

    let unit_start = std::time::Instant::now();
    let mut evaluate_start = std::time::Instant::now();
    let mut settled = prepare(
        u,
        &mut ir_session,
        &mut program,
        macro_program,
        &protocol,
        &graph,
    );
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
        let read = available.apply(u, &protocol, &applied.rows, &graph);
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
        } = macro_results(u, &protocol, &available, &active);
        if !reported.is_empty() {
            return (Vec::new(), Vec::new(), reported_diagnostics(u, &reported));
        }
        if claims.is_empty() {
            tracing::info!(
                target: "dl8::macrotime",
                waves = wave + 1,
                rows = graph.rows.len(),
                unit_us = unit_start.elapsed().as_micros() as u64,
                "macrotime unit"
            );
            sort_terms(u, &mut origin);
            fx(Wave::Settled {
                wave,
                rows: graph.rows.len(),
            });
            let mut rows = graph.rows;
            sort_terms(u, &mut rows);
            return (rows, origin, Vec::new());
        }
        let rewrite = rewrite_active_graph(u, &graph, &available, &claims, &outputs, wave);
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
        let mut changes = signed_seeds(u, &protocol, &rewrite.removed, -1);
        changes.extend(signed_seeds(u, &protocol, &rewrite.added, 1));
        evaluate_start = std::time::Instant::now();
        settled = match ir_session.apply(u, &changes) {
            Ok(applied) => Ok(applied),
            Err(ApplyError::Unprepared(reason)) => {
                tracing::info!(target: "dl8::macrotime", wave, reason, "macrotime reprepare");
                available = Available::default();
                prepare(
                    u,
                    &mut ir_session,
                    &mut program,
                    macro_program,
                    &protocol,
                    &graph,
                )
            }
            Err(ApplyError::Failed(diagnostic)) => Err(diagnostic_terms(u, &[diagnostic])),
        };
    }
    let limit = u.int(WAVE_LIMIT);
    let payload = u.compound("expansion_round_limit", vec![limit]);
    let d = macrotime_diagnostic(u, none, payload);
    (Vec::new(), Vec::new(), vec![d])
}

/// Install the rule cone with the macro seeds and every seed row of `graph`.
fn prepare(
    u: &mut Universe,
    ir_session: &mut IrSession,
    program: &mut Program,
    macro_program: &MacroProgram,
    protocol: &Protocol,
    graph: &Graph,
) -> Result<Applied, Vec<TermId>> {
    program.seeds = macro_program.program.seeds.clone();
    program.seeds.extend(syntax_seeds(u, protocol, &graph.rows));
    ir_session
        .prepare(u, program)
        .map_err(|diagnostic| diagnostic_terms(u, &[diagnostic]))
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
