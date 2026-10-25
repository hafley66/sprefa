//! `Sqlite` over a file in a temp dir: write, reopen, replay; the `<rel>_text` views; meta checks;
//! read-only queries; one statement per table and sign.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use v9::_0_id::Dict;
use v9::_1_effects::open_db;
use v9::_2_ir::Col::{Int, Text};
use v9::_2_ir::*;
use v9::_3_plan::{check, Plan, Row};
use v9::_5_ends::{Change, Delta, Diag, Diff, Store};
use v9::_7_store::_1_sqlite::Sqlite;

const RECIPE: &str = "engine=dd,store=sqlite,wire=http";

fn program() -> Program {
    Program {
        rels: vec![
            rel("edge", [("from", Text), ("to", Text)]),
            rel("weight", [("node", Text), ("n", Int)]),
            derived("reach", [("from", Text), ("to", Text)]),
            derived("lit", [] as [(&str, Col); 0]),
            request("svc", "ask", [("path", Text)]),
            answer("svc", "told", [("path", Text), ("size", Int)]),
        ],
        rules: vec![
            rule(
                atom("reach", [v("a"), v("c")]),
                [pos("edge", [v("a"), v("c")])],
            ),
            rule(atom("lit", []), [pos("edge", [text("a"), v("b")])]),
            rule(atom("ask", [v("a")]), [pos("edge", [v("a"), v("b")])]),
        ],
    }
}

fn plan(program: Program, dict: &mut Dict) -> Plan {
    check(program, dict).expect("program checks")
}

/// A fresh dir per test under the system temp dir.
fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("v9-4-store-{}", std::process::id()))
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir.join("store.db")
}

fn open(path: &Path, plan: &Plan, recipe: &str) -> Result<Sqlite, Diag> {
    Sqlite::open(plan, recipe, open_db(path).expect("open db"))
}

enum V<'a> {
    T(&'a str),
    I(i64),
}
use V::{I, T};

fn change(plan: &Plan, dict: &mut Dict, rel: &str, values: &[V], diff: Diff) -> Change {
    let row: Row = values
        .iter()
        .map(|value| match value {
            T(text) => dict.intern(text.as_bytes()),
            I(int) => *int as u64,
        })
        .collect();
    Change {
        rel: plan.rels.iter().position(|r| r.name == rel).expect("rel") as u32,
        row,
        diff,
    }
}

fn delta(event: u64, mut changes: Vec<Change>) -> Delta {
    changes.sort();
    Delta { event, changes }
}

/// `rel(col, ...) diff`, Text through the dict, Int as i64.
fn decode(plan: &Plan, dict: &Dict, changes: &[Change]) -> Vec<String> {
    changes
        .iter()
        .map(|change| {
            let rel = &plan.rels[change.rel as usize];
            let values: Vec<String> = rel
                .cols
                .iter()
                .zip(&change.row)
                .map(|(col, value)| match col {
                    Text => String::from_utf8_lossy(&dict.bytes[value]).into_owned(),
                    Int => (*value as i64).to_string(),
                })
                .collect();
            format!("{}({}) {:+}", rel.name, values.join(", "), change.diff)
        })
        .collect()
}

/// Event 1: every rel kind, Int edges at i64::MIN and i64::MAX, a nullary row.
fn first(plan: &Plan, dict: &mut Dict) -> Delta {
    delta(
        1,
        vec![
            change(plan, dict, "edge", &[T("a"), T("b")], 1),
            change(plan, dict, "edge", &[T("b"), T("c")], 1),
            change(plan, dict, "weight", &[T("a"), I(-5)], 1),
            change(plan, dict, "weight", &[T("b"), I(i64::MIN)], 1),
            change(plan, dict, "weight", &[T("c"), I(i64::MAX)], 1),
            change(plan, dict, "reach", &[T("a"), T("b")], 1),
            change(plan, dict, "reach", &[T("b"), T("c")], 1),
            change(plan, dict, "lit", &[], 1),
            change(plan, dict, "ask", &[T("a")], 1),
            change(plan, dict, "ask", &[T("b")], 1),
            change(plan, dict, "told", &[T("a"), I(42)], 1),
        ],
    )
}

fn base_and_answer(plan: &Plan, delta: &Delta) -> Vec<Change> {
    delta
        .changes
        .iter()
        .filter(|change| {
            matches!(
                plan.rels[change.rel as usize].kind,
                RelKind::Base | RelKind::Answer { .. }
            )
        })
        .cloned()
        .collect()
}

#[test]
fn write_then_replay_round_trips_and_retracts() {
    let path = dir("round_trip");
    let mut dict = Dict::default();
    let plan = plan(program(), &mut dict);
    let event1 = first(&plan, &mut dict);
    let written1 = open(&path, &plan, RECIPE)
        .expect("first open")
        .write(&event1, &dict.take_new())
        .expect("write 1");

    let mut replayed_dict = Dict::default();
    let replayed1 = open(&path, &plan, RECIPE)
        .expect("reopen")
        .replay(&mut replayed_dict)
        .expect("replay 1");
    assert_eq!(replayed1, base_and_answer(&plan, &event1));
    assert_eq!(replayed_dict.bytes, dict.bytes);
    assert_eq!(replayed_dict.fresh, Vec::<u64>::new());

    let event2 = delta(
        2,
        vec![
            change(&plan, &mut dict, "edge", &[T("b"), T("c")], -1),
            change(&plan, &mut dict, "edge", &[T("c"), T("d")], 1),
            change(&plan, &mut dict, "weight", &[T("b"), I(i64::MIN)], -1),
            change(&plan, &mut dict, "reach", &[T("b"), T("c")], -1),
            change(&plan, &mut dict, "reach", &[T("c"), T("d")], 1),
            change(&plan, &mut dict, "lit", &[], -1),
            change(&plan, &mut dict, "told", &[T("a"), I(42)], -1),
            change(&plan, &mut dict, "told", &[T("a"), I(7)], 1),
        ],
    );
    let mut store = open(&path, &plan, RECIPE).expect("reopen 2");
    let written2 = store.write(&event2, &dict.take_new()).expect("write 2");
    drop(store);

    let mut replayed_dict2 = Dict::default();
    let replayed2 = open(&path, &plan, RECIPE)
        .expect("reopen 3")
        .replay(&mut replayed_dict2)
        .expect("replay 2");
    insta::assert_debug_snapshot!((
        written1,
        decode(&plan, &replayed_dict, &replayed1),
        written2,
        decode(&plan, &replayed_dict2, &replayed2),
    ));
}

#[test]
fn text_views_decode_text_and_keep_ints() {
    let path = dir("text_view");
    let mut dict = Dict::default();
    let plan = plan(program(), &mut dict);
    let mut store = open(&path, &plan, RECIPE).expect("open");
    store
        .write(&first(&plan, &mut dict), &dict.take_new())
        .expect("write");
    let tables: Vec<_> = [
        "SELECT * FROM edge_text ORDER BY 1, 2",
        "SELECT * FROM weight_text ORDER BY 1",
        "SELECT * FROM reach_text ORDER BY 1, 2",
        "SELECT * FROM lit_text",
        "SELECT * FROM ask_text ORDER BY 1",
        "SELECT * FROM told_text",
        "SELECT n FROM weight ORDER BY n",
    ]
    .iter()
    .map(|sql| (*sql, store.query(sql).expect("query")))
    .collect();
    insta::assert_debug_snapshot!(tables);
}

#[test]
fn reopen_under_another_recipe_or_plan_is_a_mismatch() {
    let path = dir("meta");
    let mut dict = Dict::default();
    let plan_a = plan(program(), &mut dict);
    let mut other = program();
    other.rels.push(rel("extra", [("x", Int)]));
    let plan_b = plan(other, &mut dict);
    drop(open(&path, &plan_a, RECIPE).expect("first open"));

    let recipe_b = "engine=dd,store=sqlite,wire=grpc";
    assert_eq!(
        open(&path, &plan_a, recipe_b).err(),
        Some(Diag::Mismatch {
            stored_recipe: RECIPE.to_string(),
            stored_hash: plan_a.hash,
            recipe: recipe_b.to_string(),
            hash: plan_a.hash,
        })
    );
    assert_eq!(
        open(&path, &plan_b, RECIPE).err(),
        Some(Diag::Mismatch {
            stored_recipe: RECIPE.to_string(),
            stored_hash: plan_a.hash,
            recipe: RECIPE.to_string(),
            hash: plan_b.hash,
        })
    );
    let store = open(&path, &plan_a, RECIPE).expect("same recipe and plan reopen");
    insta::assert_debug_snapshot!(store
        .query("SELECT name FROM sqlite_schema ORDER BY name")
        .expect("schema"));
}

#[test]
fn query_rejects_everything_but_one_read_only_select() {
    let path = dir("query");
    let mut dict = Dict::default();
    let plan = plan(program(), &mut dict);
    let store = open(&path, &plan, RECIPE).expect("open");
    let out: Vec<_> = [
        "SELECT count(*) AS edges FROM edge",
        "INSERT INTO edge (\"from\", \"to\") VALUES (1, 2)",
        "DELETE FROM dict",
        "UPDATE meta SET plan_hash = 0",
        "CREATE TABLE extra (x INTEGER)",
        "DROP VIEW edge_text",
        "BEGIN",
        "ATTACH ':memory:' AS other",
        "SELECT 1; DELETE FROM edge",
    ]
    .iter()
    .map(|sql| (*sql, store.query(sql)))
    .collect();
    insta::assert_debug_snapshot!(out);
}

thread_local! {
    static TRACED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn traced(event: rusqlite::trace::TraceEvent<'_>) {
    if let rusqlite::trace::TraceEvent::Profile(statement, _) = event {
        let sql = statement.sql().into_owned();
        TRACED.with(|traced| traced.borrow_mut().push(sql));
    }
}

#[test]
fn a_multi_row_delta_is_one_statement_per_table_and_sign() {
    let path = dir("statements");
    let mut dict = Dict::default();
    let plan = plan(program(), &mut dict);
    let mut store = open(&path, &plan, RECIPE).expect("open");
    store
        .write(&first(&plan, &mut dict), &dict.take_new())
        .expect("seed");

    let mut changes = Vec::new();
    for node in ["d", "e", "f", "g"] {
        changes.push(change(&plan, &mut dict, "edge", &[T("a"), T(node)], 1));
        changes.push(change(&plan, &mut dict, "reach", &[T("a"), T(node)], 1));
        changes.push(change(&plan, &mut dict, "weight", &[T(node), I(1)], 1));
    }
    changes.push(change(&plan, &mut dict, "edge", &[T("a"), T("b")], -1));
    changes.push(change(&plan, &mut dict, "edge", &[T("b"), T("c")], -1));
    changes.push(change(&plan, &mut dict, "reach", &[T("a"), T("b")], -1));
    changes.push(change(&plan, &mut dict, "reach", &[T("b"), T("c")], -1));
    let event2 = delta(2, changes);

    store.db.trace_v2(
        rusqlite::trace::TraceEventCodes::SQLITE_TRACE_PROFILE,
        Some(traced),
    );
    let written = store.write(&event2, &dict.take_new()).expect("write");
    store
        .db
        .trace_v2(rusqlite::trace::TraceEventCodes::empty(), None);
    let statements = TRACED.with(|traced| traced.take());
    assert_eq!(event2.changes.len(), 16);
    assert_eq!(written, 4 + 16);
    insta::assert_debug_snapshot!(statements);
}

#[test]
#[allow(clippy::disallowed_methods)]
fn query_stops_at_the_row_cap_and_the_time_budget() {
    let path = dir("query_bounds");
    let mut dict = Dict::default();
    let plan = plan(program(), &mut dict);
    let mut store = open(&path, &plan, RECIPE).expect("open");
    store.max_rows = 3;
    store.max_time = std::time::Duration::from_millis(100);
    let counted = "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c LIMIT 30000000) \
                   SELECT count(*) FROM c";
    let started = std::time::Instant::now();
    let out: Vec<_> = [
        "SELECT value FROM json_each('[1, 2, 3]')",
        "SELECT value FROM json_each('[1, 2, 3, 4]')",
        counted,
        "SELECT value FROM json_each('[5]')",
    ]
    .iter()
    .map(|sql| (*sql, store.query(sql)))
    .collect();
    assert!(
        started.elapsed() < std::time::Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
    insta::assert_debug_snapshot!(out);
}

#[test]
fn presence_is_one_statement_per_call() {
    let path = dir("presence");
    let mut dict = Dict::default();
    let plan = plan(program(), &mut dict);
    let mut store = open(&path, &plan, RECIPE).expect("open");
    store
        .write(&first(&plan, &mut dict), &dict.take_new())
        .expect("seed");
    let asked: Vec<Change> = [["a", "b"], ["a", "z"], ["b", "c"], ["z", "z"]]
        .iter()
        .map(|[from, to]| change(&plan, &mut dict, "edge", &[T(from), T(to)], 1))
        .collect();
    let rows: Vec<&Row> = asked.iter().map(|change| &change.row).collect();
    let lit = plan.rels.iter().position(|r| r.name == "lit").expect("lit") as u32;
    store.db.trace_v2(
        rusqlite::trace::TraceEventCodes::SQLITE_TRACE_PROFILE,
        Some(traced),
    );
    let present = store.present(asked[0].rel, &rows).expect("present");
    let unit = store.present(lit, &[&Vec::new()]).expect("present unit");
    store
        .db
        .trace_v2(rusqlite::trace::TraceEventCodes::empty(), None);
    let statements = TRACED.with(|traced| traced.take());
    let shown: Vec<Change> = present
        .into_iter()
        .map(|row| Change {
            rel: asked[0].rel,
            row,
            diff: 1,
        })
        .collect();
    insta::assert_debug_snapshot!((
        decode(&plan, &dict, &shown),
        unit,
        statements,
        store.event,
    ));
}
