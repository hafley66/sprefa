//! The demand gate. `positive_solutions` proved every bound goal on a
//! rule-heading relation top down, with the proof memo rebuilt per firing;
//! one firing of the macrotime caret rule `ambiguous_return` over the
//! prelude's 16151 syntax rows cost 48353 demand calls. Relations whose
//! rules never need bound head arguments no longer demand, and the memo
//! lives once per round. On `origin/main` the prelude does not flow through
//! macrotime, so `tests/fixtures/demand_count/` reproduces the bleed at
//! fixture scale and this test drives `evaluate` directly: 100 source rows,
//! a derived relation whose rule needs nothing bound (the bleed), and one
//! whose kernel goal reads its head argument (demand stays).

use std::path::Path;

use dl8::_6_eval::evaluate::{evaluate, Trace};
use dl8::_6_eval::json::program_from_json;
use dl8::_6_eval::{Program, TermId, Universe};

fn fixture(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/demand_count")
        .join(name)
        .display()
        .to_string()
}

fn relation(u: &mut Universe, name: &str) -> TermId {
    let inner = u.atom(name);
    u.compound("ref", vec![inner])
}

#[test]
fn demand_calls_stay_under_the_round_bar() {
    let text = std::fs::read_to_string(fixture("ambiguous_return_bleed.json")).unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    let mut u = Universe::new();
    let program = program_from_json(&mut u, &value).unwrap();
    let caret = relation(&mut u, "caret_item");
    let flagged = relation(&mut u, "flagged");
    let mut demand_calls = 0usize;
    let closure = evaluate(&mut u, &program, &mut |t: Trace| {
        if let Trace::Demand { calls } = t {
            demand_calls += calls;
        }
    });
    assert!(closure.diagnostics.is_empty(), "unexpected diagnostics");
    let caret_rows = closure.rows.iter().filter(|r| r.rel == caret).count();
    let flagged_rows = closure.rows.iter().filter(|r| r.rel == flagged).count();
    assert_eq!(caret_rows, 100, "caret_item rows");
    assert_eq!(flagged_rows, 50, "flagged rows");
    println!("demand calls: {demand_calls}");
    assert!(
        demand_calls < 500,
        "demand calls {demand_calls} over the round bar"
    );
}
