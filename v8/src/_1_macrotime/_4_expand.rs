//! Port of `v7/src/1_libtime/1_syntax_expander.pl` `expand_syntax/5`. Each
//! wave evaluates the macro program over the current rows through the one
//! evaluator in `_6_eval`, then rewrites every claimed node. The loop ends
//! when a wave claims nothing, repeats a row set, or reaches the wave limit.

use super::_0_rows::{macrotime_diagnostic, rows_of_terms, sort_terms, terms_of_rows, Graph};
use super::_2_protocol::{
    macro_dispatch, macro_protocol, macro_results, macro_rules, syntax_seeds, Dispatch,
    MacroProgram,
};
use super::_3_rewrite::rewrite_active_graph;
use crate::_6_eval::{evaluate, Program, Row, TermId, Trace, Universe};

pub const WAVE_LIMIT: i64 = 64;

/// What one expansion wave did. The sink is the seam a later yieldable
/// macrotime would suspend on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Wave {
    Evaluated {
        wave: i64,
        seeds: usize,
        closure: usize,
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

pub fn expand_terms(
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
    let rules: Vec<_> = selected
        .iter()
        .map(|i| macro_program.program.rules[*i].clone())
        .collect();
    let none = u.atom("none");
    let mut seen: Vec<Vec<TermId>> = vec![graph.rows.clone()];
    let mut origin: Vec<TermId> = Vec::new();

    for wave in 0..WAVE_LIMIT {
        let mut seeds = macro_program.program.seeds.clone();
        seeds.extend(syntax_seeds(u, &protocol, &graph.rows));
        let program = Program {
            rules: rules.clone(),
            seeds,
        };
        let closure = evaluate(u, &program, &mut |_: Trace| {});
        if !closure.diagnostics.is_empty() {
            let terms = closure
                .diagnostics
                .iter()
                .map(|d| {
                    let phase = u.atom(d.phase);
                    u.compound("diagnostic", vec![phase, none, d.payload])
                })
                .collect();
            return (Vec::new(), Vec::new(), terms);
        }
        fx(Wave::Evaluated {
            wave,
            seeds: program.seeds.len(),
            closure: closure.rows.len(),
        });
        let active = graph.active_nodes();
        let (available, claims, outputs) = macro_results(u, &protocol, &closure.rows, &active);
        if claims.is_empty() {
            sort_terms(u, &mut origin);
            fx(Wave::Settled {
                wave,
                rows: graph.rows.len(),
            });
            return (graph.rows, origin, Vec::new());
        }
        let available = Graph::build(u, available);
        let rewrite = rewrite_active_graph(u, &graph, &available, &claims, &outputs, wave);
        if !rewrite.diagnostics.is_empty() {
            return (Vec::new(), Vec::new(), rewrite.diagnostics);
        }
        if seen.contains(&rewrite.rows) {
            let wave_term = u.int(wave);
            let payload = u.compound("expansion_cycle", vec![wave_term]);
            let d = macrotime_diagnostic(u, none, payload);
            return (Vec::new(), Vec::new(), vec![d]);
        }
        fx(Wave::Rewritten {
            wave,
            claimed: claims.len(),
            edges: outputs.len(),
            rows: rewrite.rows.len(),
        });
        origin.extend(rewrite.origin);
        seen.push(rewrite.rows.clone());
        graph = Graph::build(u, rewrite.rows);
    }
    let limit = u.int(WAVE_LIMIT);
    let payload = u.compound("expansion_round_limit", vec![limit]);
    let d = macrotime_diagnostic(u, none, payload);
    (Vec::new(), Vec::new(), vec![d])
}
