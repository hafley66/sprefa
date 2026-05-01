//! Synthetic scope for card A's smoke.
//!
//! Mirrors ~/projects/ext/sprefa-dd-poc/src/bin/larger.rs. Three gens:
//!   gen 0: seed three file_edit rows (two violate the long-line rule)
//!   gen 1: shorten one line (retract long, insert short)
//!   gen 2: introduce a new violation
//!
//! Outputs an EffectDesc per rule diff (PUBLISH on +1, CLEAR on -1) into
//! a crossbeam channel and a SmokeReport tallying gen-by-gen state.
//!
//! IR-driven scope construction is card E. The shape here is hand-wired.

use std::sync::{Arc, Mutex};

use crossbeam_channel::{unbounded, Receiver, Sender};
use differential_dataflow::input::InputSession;
use differential_dataflow::operators::Consolidate;
use timely::dataflow::operators::probe::Handle as ProbeHandle;

use super::_0_row::{Diff, Time};

/// Effect description emitted by a sink. Card A keeps this local; card G
/// will fold it into the pipeline-wide effects.rs surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EffectDesc {
    PublishDiag {
        rule: &'static str,
        path: String,
        line: u64,
        gen: Time,
    },
    ClearDiag {
        rule: &'static str,
        path: String,
        line: u64,
        gen: Time,
    },
}

/// Per-gen tally captured for the smoke test.
#[derive(Default, Debug, Eq, PartialEq)]
pub struct SmokeReport {
    /// At each gen seal: snapshot of (path, line, content) currently
    /// present (count > 0) in the file_edit fact log.
    pub fact_states: Vec<Vec<(String, u64, String)>>,
    /// At each gen seal: snapshot of (path, line) violations.
    pub rule_states: Vec<Vec<(String, u64)>>,
    /// Effect descriptions emitted, in order.
    pub effects: Vec<EffectDesc>,
}

/// Run the synthetic 3-gen scenario. Single-thread, single-worker.
/// Returns the report; the worker exits when the closure returns.
pub fn run_synthetic_scope(long_line_threshold: usize) -> SmokeReport {
    // Sink-shared state. Mutex<...> mirrors larger.rs FactLog/LongLineBag.
    let report: Arc<Mutex<SmokeReport>> = Arc::new(Mutex::new(SmokeReport::default()));
    let (tx_eff, rx_eff): (Sender<EffectDesc>, Receiver<EffectDesc>) = unbounded();

    // Per-gen tallies built up by sinks.
    type EditDiff = (String, u64, String, Time, Diff);
    type RuleDiff = (String, u64, Time, Diff);
    let edits_log: Arc<Mutex<Vec<EditDiff>>> = Arc::new(Mutex::new(Vec::new()));
    let rule_log: Arc<Mutex<Vec<RuleDiff>>> = Arc::new(Mutex::new(Vec::new()));

    let edits_log_run = edits_log.clone();
    let rule_log_run = rule_log.clone();

    timely::execute_directly(move |worker| {
        let mut file_edit: InputSession<Time, (String, u64, String), Diff> =
            InputSession::new();
        let mut probe = ProbeHandle::new();

        worker.dataflow::<Time, _, _>(|scope| {
            let edits = file_edit.to_collection(scope);

            // fact-log sink (the "fact bag" persistence)
            let edits_w = edits_log_run.clone();
            edits
                .inspect_batch(move |t, batch| {
                    let mut log = edits_w.lock().unwrap();
                    for ((p, l, c), _, d) in batch {
                        log.push((p.clone(), *l, c.clone(), *t, *d));
                    }
                })
                .probe_with(&mut probe);

            // rule body: long_line filter
            let violations = edits
                .filter(move |(_p, _l, c)| c.len() >= long_line_threshold)
                .map(|(p, l, _c)| (p, l))
                .consolidate();

            // rule-log + effect sink
            let rule_w = rule_log_run.clone();
            let tx = tx_eff.clone();
            violations
                .inspect_batch(move |t, batch| {
                    let mut log = rule_w.lock().unwrap();
                    for ((p, l), _, d) in batch {
                        log.push((p.clone(), *l, *t, *d));
                        let eff = if *d > 0 {
                            EffectDesc::PublishDiag {
                                rule: "long_line",
                                path: p.clone(),
                                line: *l,
                                gen: *t,
                            }
                        } else {
                            EffectDesc::ClearDiag {
                                rule: "long_line",
                                path: p.clone(),
                                line: *l,
                                gen: *t,
                            }
                        };
                        let _ = tx.send(eff);
                    }
                })
                .probe_with(&mut probe);
        });

        let mut tick = |fe: &mut InputSession<Time, _, Diff>, to: Time| {
            fe.advance_to(to);
            fe.flush();
            worker.step_while(|| probe.less_than(fe.time()));
        };

        // gen 0: seed three rows
        file_edit.insert(("src/a.rs".into(), 10, "let x = 1;".into()));
        file_edit.insert((
            "src/a.rs".into(),
            11,
            "let y = vec![1,2,3,4,5,6,7];".into(),
        ));
        file_edit.insert((
            "src/b.rs".into(),
            42,
            "fn run() { do_a_lot_of_work(); }".into(),
        ));
        tick(&mut file_edit, 1);

        // gen 1: shorten a.rs:11
        file_edit.remove((
            "src/a.rs".into(),
            11,
            "let y = vec![1,2,3,4,5,6,7];".into(),
        ));
        file_edit.insert(("src/a.rs".into(), 11, "let y = 1;".into()));
        tick(&mut file_edit, 2);

        // gen 2: new violation in c.rs
        file_edit.insert((
            "src/c.rs".into(),
            5,
            "very_long_identifier_that_exceeds_twenty_chars()".into(),
        ));
        tick(&mut file_edit, 3);
    });

    // Build snapshots after the worker has exited. fact_states[i] is the
    // multiset projection of all diffs with t <= i.
    let edits_diffs = edits_log.lock().unwrap().clone();
    let rule_diffs = rule_log.lock().unwrap().clone();

    let mut report = report.lock().unwrap();
    for gen in 0..3u64 {
        report.fact_states.push(project_facts(&edits_diffs, gen));
        report.rule_states.push(project_rules(&rule_diffs, gen));
    }
    while let Ok(e) = rx_eff.try_recv() {
        report.effects.push(e);
    }
    SmokeReport {
        fact_states: std::mem::take(&mut report.fact_states),
        rule_states: std::mem::take(&mut report.rule_states),
        effects: std::mem::take(&mut report.effects),
    }
}

fn project_facts(diffs: &[(String, u64, String, Time, Diff)], gen: Time) -> Vec<(String, u64, String)> {
    use std::collections::HashMap;
    let mut counts: HashMap<(String, u64, String), Diff> = HashMap::new();
    for (p, l, c, t, d) in diffs {
        if *t <= gen {
            *counts.entry((p.clone(), *l, c.clone())).or_insert(0) += *d;
        }
    }
    let mut out: Vec<_> = counts
        .into_iter()
        .filter(|(_, c)| *c > 0)
        .map(|(k, _)| k)
        .collect();
    out.sort();
    out
}

fn project_rules(diffs: &[(String, u64, Time, Diff)], gen: Time) -> Vec<(String, u64)> {
    use std::collections::HashMap;
    let mut counts: HashMap<(String, u64), Diff> = HashMap::new();
    for (p, l, t, d) in diffs {
        if *t <= gen {
            *counts.entry((p.clone(), *l)).or_insert(0) += *d;
        }
    }
    let mut out: Vec<_> = counts
        .into_iter()
        .filter(|(_, c)| *c > 0)
        .map(|(k, _)| k)
        .collect();
    out.sort();
    out
}
