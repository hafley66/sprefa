//! `examples/goto-flows.dl` (F3 of the flow-marks-goto-recorder plan): named,
//! union/anti-unified dataflows reverse-derived from recorded editor goto
//! jumps. In-process, mirroring `tests/it/extract_cache.rs`: builds a real
//! `Engine`, feeds `hook_event` rows via `insert_hook_event` (the same method
//! F2's `dl/hookEvent` LSP request and the daemon `hook_event` RPC call), and
//! ticks. No CLI subprocess, no LSP session: the goto-flows rules are pure
//! joins over `hook_event` + `call_def`, so an in-process tick is the direct
//! test of the .dl logic.

use sprefa_v5::{db, engine::Engine};
use std::fs;
use std::path::PathBuf;

const PROG: &str = include_str!("../../examples/goto-flows.dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("goto_flows_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

/// Four functions at known 1-based decl lines, plus a trailing comment (line
/// 17) that sits inside no `call_def` span, the deliberate miss target for
/// test 4.
///
///   1  fn checkout_start() {
///   5  fn validate_cart() {
///   9  fn apply_discount() {
///  13  fn charge_card() {
///  17  // trailing comment, outside every fn span
const FIXTURE_SRC: &str = "fn checkout_start() {\n    helper();\n}\n\n\
    fn validate_cart() {\n    ok();\n}\n\n\
    fn apply_discount() {\n    reduce();\n}\n\n\
    fn charge_card() {\n    pay();\n}\n\n\
    // trailing comment, outside every fn span\n";

const CHECKOUT_START_LINE: i64 = 1;
const VALIDATE_CART_LINE: i64 = 5;
const APPLY_DISCOUNT_LINE: i64 = 9;
const MISS_LINE: i64 = 17; // the trailing comment

/// One `goto` event's json body: `{"from": {...}|null, "to": {...}, "word": ...}`.
fn goto_json(from: Option<(&str, i64)>, to: (&str, i64), word: &str) -> String {
    let from_val = match from {
        Some((file, line)) => serde_json::json!({"file": file, "line": line}),
        None => serde_json::Value::Null,
    };
    serde_json::json!({
        "from": from_val,
        "to": {"file": to.0, "line": to.1},
        "word": word,
    })
    .to_string()
}

fn feed(eng: &mut Engine, session: &str, seq: i64, json: &str) {
    eng.insert_hook_event("goto", session, seq, json).unwrap();
}

/// Build an engine over `program_text` written to a real `.dl` file and run
/// it through `prepare_paths` (NOT raw `parse::parse`): a rule head using
/// named args (`hover_note`'s named-arg form) stores its columns in
/// `Atom.named` until `typecheck::check_and_normalize`'s `resolve_named_args`
/// pass folds them into positional `terms`, the pass `prepare_paths` runs and
/// bare `parse::parse` skips. Every production entry point (CLI, `--hook`,
/// LSP) goes through `prepare_paths`; this mirrors it instead of hitting a
/// harness-only "empty head terms" bug.
fn engine_over(dir: &PathBuf, program_text: &str) -> (Engine, sprefa_v5::ast::Program) {
    fs::write(dir.join("src/flow.rs"), FIXTURE_SRC).unwrap();
    let prog_path = dir.join("p.dl");
    fs::write(&prog_path, program_text).unwrap();
    let (prog, diags, _display) = sprefa_v5::prepare_paths(&[prog_path]).unwrap();
    let errs: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == sprefa_v5::ast::Severity::Error)
        .collect();
    assert!(errs.is_empty(), "program must typecheck cleanly: {errs:?}");
    let conn = db::open(Some(dir.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, dir.clone());
    // Prime: declares hook_event (and every other built-in the program
    // references) before the first insert.
    eng.tick(&prog, true).unwrap();
    (eng, prog)
}

fn s(v: &serde_json::Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}
fn i(v: &serde_json::Value) -> i64 {
    v.as_i64().unwrap_or_default()
}

/// Test (1): two takes of the same named flow, one with an extra detour edge.
/// `flow_union_edge` carries BOTH edges; `flow_common_edge` carries ONLY the
/// edge present in every take (the detour is in union, not common).
#[test]
fn union_carries_every_edge_common_carries_only_the_shared_spine() {
    let d = sandbox("union_common");
    let prog_text = format!(
        "{PROG}\nflow_take(\"checkout\", \"take-1\").\nflow_take(\"checkout\", \"take-2\").\n"
    );
    let (mut eng, prog) = engine_over(&d, &prog_text);

    // take-1: checkout_start -> validate_cart -> apply_discount (the detour).
    feed(
        &mut eng,
        "take-1",
        1000,
        &goto_json(None, ("src/flow.rs", CHECKOUT_START_LINE), "checkout_start"),
    );
    feed(
        &mut eng,
        "take-1",
        1001,
        &goto_json(
            Some(("src/flow.rs", CHECKOUT_START_LINE)),
            ("src/flow.rs", VALIDATE_CART_LINE),
            "validate_cart",
        ),
    );
    feed(
        &mut eng,
        "take-1",
        1002,
        &goto_json(
            Some(("src/flow.rs", VALIDATE_CART_LINE)),
            ("src/flow.rs", APPLY_DISCOUNT_LINE),
            "apply_discount",
        ),
    );

    // take-2: checkout_start -> validate_cart only.
    feed(
        &mut eng,
        "take-2",
        2000,
        &goto_json(None, ("src/flow.rs", CHECKOUT_START_LINE), "checkout_start"),
    );
    feed(
        &mut eng,
        "take-2",
        2001,
        &goto_json(
            Some(("src/flow.rs", CHECKOUT_START_LINE)),
            ("src/flow.rs", VALIDATE_CART_LINE),
            "validate_cart",
        ),
    );

    eng.tick(&prog, true).unwrap();

    let union_rows = eng
        .query_sql(
            "SELECT src_sym, dst_sym FROM rel_flow_union_edge_txt WHERE name = 'checkout'",
            &[],
        )
        .unwrap();
    let union_pairs: Vec<(String, String)> =
        union_rows.iter().map(|r| (s(&r[0]), s(&r[1]))).collect();
    assert_eq!(
        union_pairs.len(),
        2,
        "union carries both edges: {union_pairs:?}"
    );
    assert!(
        union_pairs
            .iter()
            .any(|(a, b)| a.contains("checkout_start") && b.contains("validate_cart")),
        "union has checkout_start->validate_cart: {union_pairs:?}"
    );
    assert!(
        union_pairs
            .iter()
            .any(|(a, b)| a.contains("validate_cart") && b.contains("apply_discount")),
        "union has the take-1-only detour validate_cart->apply_discount: {union_pairs:?}"
    );

    let common_rows = eng
        .query_sql(
            "SELECT src_sym, dst_sym FROM rel_flow_common_edge_txt WHERE name = 'checkout'",
            &[],
        )
        .unwrap();
    let common_pairs: Vec<(String, String)> =
        common_rows.iter().map(|r| (s(&r[0]), s(&r[1]))).collect();
    assert_eq!(
        common_pairs.len(),
        1,
        "common carries only the shared edge: {common_pairs:?}"
    );
    assert!(
        common_pairs
            .iter()
            .any(|(a, b)| a.contains("checkout_start") && b.contains("validate_cart")),
        "common has checkout_start->validate_cart: {common_pairs:?}"
    );
    assert!(
        !common_pairs
            .iter()
            .any(|(a, b)| a.contains("validate_cart") && b.contains("apply_discount")),
        "the take-1-only detour must NOT be in common: {common_pairs:?}"
    );
}

/// Test (2): a session with no `flow_take` fact still becomes its own
/// one-take flow (the `take_of(session, session) <- ..., !session_named(...)`
/// identity rule), and shows up in `flow_stat`.
#[test]
fn unnamed_session_defaults_to_its_own_flow() {
    let d = sandbox("unnamed");
    let (mut eng, prog) = engine_over(&d, PROG);

    feed(
        &mut eng,
        "solo",
        1000,
        &goto_json(None, ("src/flow.rs", CHECKOUT_START_LINE), "checkout_start"),
    );
    feed(
        &mut eng,
        "solo",
        1001,
        &goto_json(
            Some(("src/flow.rs", CHECKOUT_START_LINE)),
            ("src/flow.rs", VALIDATE_CART_LINE),
            "validate_cart",
        ),
    );

    eng.tick(&prog, true).unwrap();

    let take_of_rows = eng
        .query_sql(
            "SELECT name, session FROM rel_take_of_txt WHERE session = 'solo'",
            &[],
        )
        .unwrap();
    assert_eq!(
        take_of_rows.len(),
        1,
        "solo gets exactly one take_of row: {take_of_rows:?}"
    );
    assert_eq!(
        s(&take_of_rows[0][0]),
        "solo",
        "the identity name is the session itself"
    );

    let stat_rows = eng
        .query_sql(
            "SELECT takes, edges FROM rel_flow_stat_txt WHERE name = 'solo'",
            &[],
        )
        .unwrap();
    assert_eq!(
        stat_rows.len(),
        1,
        "solo has one flow_stat row: {stat_rows:?}"
    );
    assert_eq!(i(&stat_rows[0][0]), 1, "one take");
    assert_eq!(i(&stat_rows[0][1]), 1, "one edge");
}

/// Test (3): `hover_note` rows derive at the right 0-based lines (`call_def`
/// is 1-based; `checkout_start` at decl line 1 -> hover_note line 0,
/// `validate_cart` at decl line 5 -> hover_note line 4), with the flow name in
/// the markdown.
#[test]
fn hover_note_lands_at_zero_based_member_lines() {
    let d = sandbox("hover");
    let prog_text = format!("{PROG}\nflow_take(\"onboarding\", \"hovertake\").\n");
    let (mut eng, prog) = engine_over(&d, &prog_text);

    feed(
        &mut eng,
        "hovertake",
        1000,
        &goto_json(None, ("src/flow.rs", CHECKOUT_START_LINE), "checkout_start"),
    );
    feed(
        &mut eng,
        "hovertake",
        1001,
        &goto_json(
            Some(("src/flow.rs", CHECKOUT_START_LINE)),
            ("src/flow.rs", VALIDATE_CART_LINE),
            "validate_cart",
        ),
    );

    eng.tick(&prog, true).unwrap();

    let rows = eng
        .query_sql(
            "SELECT path, line, col, end_line, end_col, md FROM rel_hover_note_txt ORDER BY line",
            &[],
        )
        .unwrap();
    assert_eq!(rows.len(), 2, "one hover_note per flow member: {rows:?}");

    // checkout_start: decl line 1 (1-based) -> hover_note line 0.
    assert_eq!(
        i(&rows[0][1]),
        0,
        "checkout_start's note lands at 0-based line 0: {rows:?}"
    );
    assert_eq!(i(&rows[0][2]), 0, "col starts at 0");
    assert_eq!(i(&rows[0][3]), 0, "end_line matches the single-line span");
    assert_eq!(i(&rows[0][4]), 200, "end_col is the wide whole-line span");
    assert!(
        s(&rows[0][5]).contains("in flow:"),
        "the note text: {:?}",
        s(&rows[0][5])
    );
    assert!(
        s(&rows[0][5]).contains("onboarding"),
        "names the flow: {:?}",
        s(&rows[0][5])
    );

    // validate_cart: decl line 5 (1-based) -> hover_note line 4.
    assert_eq!(
        i(&rows[1][1]),
        4,
        "validate_cart's note lands at 0-based line 4: {rows:?}"
    );
    assert!(
        s(&rows[1][5]).contains("onboarding"),
        "names the flow: {:?}",
        s(&rows[1][5])
    );
}

/// Test (4): a jump whose `to` lands outside every `call_def` span (the
/// trailing comment, line 17) produces `goto_jump` (extraction succeeded) but
/// NO `jump_dst_sym`/`take_edge` row: the sym-lift miss is silent, not an
/// error and not a fallback file-level edge.
#[test]
fn jump_outside_every_span_is_a_silent_lift_miss() {
    let d = sandbox("miss");
    let (mut eng, prog) = engine_over(&d, PROG);

    feed(
        &mut eng,
        "misstake",
        1000,
        &goto_json(None, ("src/flow.rs", CHECKOUT_START_LINE), "checkout_start"),
    );
    feed(
        &mut eng,
        "misstake",
        1001,
        &goto_json(
            Some(("src/flow.rs", CHECKOUT_START_LINE)),
            ("src/flow.rs", MISS_LINE),
            "nothing_here",
        ),
    );

    eng.tick(&prog, true).unwrap();

    // The jump itself extracted fine (both from and to were present).
    let jump_rows = eng
        .query_sql(
            "SELECT seq FROM rel_goto_jump_txt WHERE session = 'misstake'",
            &[],
        )
        .unwrap();
    assert_eq!(
        jump_rows.len(),
        1,
        "the from/to jump extracted despite the miss: {jump_rows:?}"
    );

    // But the destination never lifted to a sym, so no edge.
    let dst_rows = eng
        .query_sql(
            "SELECT sym FROM rel_jump_dst_sym_txt WHERE session = 'misstake'",
            &[],
        )
        .unwrap();
    assert!(
        dst_rows.is_empty(),
        "the comment line lifts to no sym: {dst_rows:?}"
    );

    let edge_rows = eng
        .query_sql(
            "SELECT src_sym, dst_sym FROM rel_take_edge_txt WHERE session = 'misstake'",
            &[],
        )
        .unwrap();
    assert!(
        edge_rows.is_empty(),
        "no take_edge from a lift miss: {edge_rows:?}"
    );
}
