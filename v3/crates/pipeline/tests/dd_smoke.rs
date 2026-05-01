//! Card sprefa-07b smoke. Off-by-default; run with:
//!
//!   cargo test --release --features pipeline/dd --test dd_smoke -p pipeline
//!
//! Release is required because DD 0.12 ships abomonation, and nightly's
//! debug-mode UB checker (-Zub-checks) fires on
//! ptr::swap_nonoverlapping inside abomonation's serialization. DD 0.13
//! moved off abomonation but its transitive `columnar` dep does not
//! compile against the timely_communication 0.18 surface yet (E0576 on
//! `Container::reborrow_ref`). Card E will revisit the version pin once
//! the upstream story stabilizes.
//!
//! Mirrors ~/projects/ext/sprefa-dd-poc/src/bin/larger.rs as a 3-gen
//! retraction round-trip through the DD bridge module.

#![cfg(feature = "dd")]

use pipeline::dd::{DdRunner, EffectDesc};

#[test]
fn three_gen_retraction_smoke() {
    let runner = DdRunner::new();
    let report = runner.run_smoke(20);

    // Three gen snapshots captured.
    assert_eq!(report.fact_states.len(), 3);
    assert_eq!(report.rule_states.len(), 3);

    // Gen 0: three facts in, two violate the >=20 char rule.
    assert_eq!(report.fact_states[0].len(), 3);
    let g0_rules: Vec<_> = report.rule_states[0]
        .iter()
        .map(|(p, l)| (p.as_str(), *l))
        .collect();
    assert_eq!(g0_rules, vec![("src/a.rs", 11), ("src/b.rs", 42)]);

    // Gen 1: a.rs:11 shortened. Still 3 facts (different content), but
    // only 1 violation (b.rs:42) remains.
    assert_eq!(report.fact_states[1].len(), 3);
    let g1_rules: Vec<_> = report.rule_states[1]
        .iter()
        .map(|(p, l)| (p.as_str(), *l))
        .collect();
    assert_eq!(g1_rules, vec![("src/b.rs", 42)]);

    // Gen 2: c.rs:5 added (long). 4 facts, 2 violations.
    assert_eq!(report.fact_states[2].len(), 4);
    let g2_rules: Vec<_> = report.rule_states[2]
        .iter()
        .map(|(p, l)| (p.as_str(), *l))
        .collect();
    assert_eq!(g2_rules, vec![("src/b.rs", 42), ("src/c.rs", 5)]);

    // Effect ordering. PUBLISH for the two initial violations, CLEAR for
    // a.rs:11 when shortened, PUBLISH for c.rs:5 when added.
    let publish_count = report
        .effects
        .iter()
        .filter(|e| matches!(e, EffectDesc::PublishDiag { .. }))
        .count();
    let clear_count = report
        .effects
        .iter()
        .filter(|e| matches!(e, EffectDesc::ClearDiag { .. }))
        .count();
    assert_eq!(publish_count, 3, "two seed violations + one new violation");
    assert_eq!(clear_count, 1, "a.rs:11 cleared on shorten");
}

#[test]
fn runner_gen_counter_monotonic() {
    let runner = DdRunner::new();
    assert_eq!(runner.current_gen(), 0);
    let g1 = runner.bump_gen();
    let g2 = runner.bump_gen();
    let g3 = runner.bump_gen();
    assert_eq!(g1, 1);
    assert_eq!(g2, 2);
    assert_eq!(g3, 3);
}
