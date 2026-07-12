//! Cross-language flow page (bench/flow): the OpenAPI spec is the source of
//! truth, and one operationId resolves to its backend implementor (Rust) AND
//! its consumers (TS) across language boundaries by the codegen rhythm (the
//! operationId verbatim). Low-fidelity tier: pure string identity (operationId
//! == fn name == callee name), no SCIP resolution yet. The guard: a spec op
//! must reach at least one Rust impl AND one TS call site, so the cross-lang
//! join can't silently regress to single-language.

use sprefa_v5::{db, engine::Engine, lex, parse};
use std::path::PathBuf;

const FLOW: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/bench/flow");

fn run() -> Engine {
    let root = PathBuf::from(FLOW);
    let prog_src = std::fs::read_to_string(root.join("flow.dl")).unwrap();
    let prog = parse::parse(lex::lex(&prog_src).unwrap()).unwrap();
    let db = std::env::temp_dir().join(format!("flow_xlang_{}.db", std::process::id()));
    let _ = std::fs::remove_file(&db);
    let conn = db::open(Some(db.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, root);
    eng.tick(&prog, true).unwrap();
    eng
}

/// (op, role, file) rows of the `flow` relation.
fn flow(eng: &Engine) -> Vec<(String, String, String)> {
    eng.query_sql("SELECT \"op\",\"role\",\"file\" FROM rel_flow_txt", &[])
        .unwrap().into_iter()
        .map(|r| (s(&r[0]), s(&r[1]), s(&r[2])))
        .collect()
}
fn s(v: &serde_json::Value) -> String { v.as_str().unwrap_or_default().to_string() }

#[test]
fn spec_op_resolves_across_languages() {
    let eng = run();

    // the spec (source of truth) yields its operationIds.
    let ops: Vec<String> = eng.query_sql("SELECT \"op\" FROM rel_spec_op_txt", &[])
        .unwrap().into_iter().map(|r| s(&r[0])).collect();
    assert!(ops.contains(&"getPet".to_string()), "spec_op missing getPet: {ops:?}");

    let rows = flow(&eng);
    assert!(!rows.is_empty(), "flow produced no rows");

    // getPet must resolve to a Rust backend implementor...
    let rust_impl = rows.iter().any(|(op, role, f)|
        op == "getPet" && role == "impl" && f.ends_with("rust/src/handler.rs"));
    assert!(rust_impl, "getPet has no Rust impl in flow: {rows:?}");

    // ...AND a TS consumer call site. Cross-language: one spec op, two langs.
    let ts_client = rows.iter().any(|(op, role, f)|
        op == "getPet" && role == "client" && f.ends_with("ts/app/pages.ts"));
    assert!(ts_client, "getPet has no TS client in flow: {rows:?}");
}
