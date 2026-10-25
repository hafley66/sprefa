//! `Core` over a temp db: commits, reopen, service rounds through fake services, every commit a
//! client may not make, the HTTP routes through `Handle`, `v9 serve` under `V9_SYSTEM`, and the
//! ryi service when ryi resolves.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use v9::_0_id::Dict;
use v9::_1_effects::{ryi_call_edges, v9_ryi};
use v9::_2_ir::Builtin::Lt;
use v9::_2_ir::Col::{Int, Text};
use v9::_2_ir::*;
use v9::_4_recipe::Recipe;
use v9::_5_ends::{Bounds, Cadence, Cell, Change, Diag, IService};
use v9::_8_runtime::_0_programs::reach;
use v9::_8_runtime::_2_ryi::Ryi;
use v9::_8_runtime::_3_core::{build, Core, Named, MAX_SERVICE_ROUNDS};
use v9::_8_runtime::_4_handle::start;
use v9::_9_wire::http_auto::router;

/// `want` asks `size` for a path's size; `start` asks `succ` for n + 1, whose answer asks again.
fn program() -> Program {
    Program {
        rels: vec![
            rel("edge", [("from", Text), ("to", Text)]),
            rel("want", [("path", Text)]),
            rel("start", [("n", Int)]),
            derived("reach", [("from", Text), ("to", Text)]),
            request("size", "ask", [("path", Text)]),
            answer("size", "told", [("path", Text), ("size", Int)]),
            derived("big", [("path", Text)]),
            request("succ", "next_ask", [("n", Int)]),
            answer("succ", "next_told", [("n", Int), ("m", Int)]),
        ],
        rules: vec![
            rule(
                atom("reach", [v("a"), v("b")]),
                [pos("edge", [v("a"), v("b")])],
            ),
            rule(
                atom("reach", [v("a"), v("c")]),
                [
                    pos("reach", [v("a"), v("b")]),
                    pos("edge", [v("b"), v("c")]),
                ],
            ),
            rule(atom("ask", [v("p")]), [pos("want", [v("p")])]),
            rule(
                atom("big", [v("p")]),
                [pos("told", [v("p"), v("s")]), call(Lt, [int(100), v("s")])],
            ),
            rule(atom("next_ask", [v("n")]), [pos("start", [v("n")])]),
            rule(
                atom("next_ask", [v("m")]),
                [pos("next_told", [v("n"), v("m")])],
            ),
        ],
    }
}

/// Every `call` it gets, as `name(row) diff`.
type Log = Rc<RefCell<Vec<String>>>;

/// Answers each asserted request row with `answer(row)` appended; Once.
struct Fake {
    name: &'static str,
    answer: fn(&Dict, &[u64]) -> u64,
    ready: Vec<Change>,
    log: Log,
}

impl IService for Fake {
    fn service(&self) -> &str {
        self.name
    }
    fn cadence(&self) -> Cadence {
        Cadence::Once
    }
    fn call(&mut self, requests: &[Change], dict: &Dict) {
        for request in requests {
            self.log
                .borrow_mut()
                .push(format!("{}{:?} {:+}", self.name, request.row, request.diff));
            if request.diff > 0 {
                let mut row = request.row.clone();
                row.push((self.answer)(dict, &request.row));
                self.ready.push(Change {
                    rel: 0,
                    row,
                    diff: 1,
                });
            }
        }
    }
    fn poll(&mut self, _dict: &mut Dict, out: &mut Vec<Change>) {
        out.append(&mut self.ready);
    }
}

/// `size`: ten per byte of the path. `succ`: n + 1.
fn fakes(log: &Log) -> Vec<Box<dyn IService>> {
    vec![
        Box::new(Fake {
            name: "size",
            answer: |dict, row| dict.bytes[&row[0]].len() as u64 * 10,
            ready: Vec::new(),
            log: Rc::clone(log),
        }),
        Box::new(Fake {
            name: "succ",
            answer: |_, row| row[0] + 1,
            ready: Vec::new(),
            log: Rc::clone(log),
        }),
    ]
}

const BOUNDS: Bounds = Bounds { max_rounds: 8 };

/// A fresh dir per test under the system temp dir.
fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("v9-5-runtime-{}", std::process::id()))
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn open(db: &Path, log: &Log) -> Core {
    build(Recipe::default(), program(), db, fakes(log), BOUNDS).expect("build")
}

fn t(text: &str) -> Cell {
    Cell::Text(text.into())
}

fn named(rel: &str, cells: Vec<Cell>, diff: isize) -> Named {
    (rel.to_string(), cells, diff)
}

fn show(changes: &[Named]) -> Vec<String> {
    changes
        .iter()
        .map(|(rel, cells, diff)| {
            let cells: Vec<String> = cells
                .iter()
                .map(|cell| match cell {
                    Cell::Text(text) => text.clone(),
                    Cell::Int(int) => int.to_string(),
                    other => format!("{other:?}"),
                })
                .collect();
            format!("{rel}({}) {diff:+}", cells.join(", "))
        })
        .collect()
}

/// Every `<rel>_text` view of the program, one line per row, rel order then row order.
fn rows(core: &Core) -> Vec<String> {
    let mut lines = Vec::new();
    for declared in &program().rels {
        let order: Vec<String> = (1..=declared.cols.len()).map(|n| n.to_string()).collect();
        let table = core
            .query(&format!(
                "SELECT * FROM {}_text ORDER BY {}",
                declared.name,
                order.join(", ")
            ))
            .expect("query");
        lines.extend(
            show(
                &table
                    .rows
                    .into_iter()
                    .map(|row| named(&declared.name, row, 1))
                    .collect::<Vec<_>>(),
            )
            .into_iter()
            .map(|line| line.trim_end_matches(" +1").to_string()),
        );
    }
    lines
}

fn first_commit() -> Vec<Named> {
    vec![
        named("edge", vec![t("a"), t("b")], 1),
        named("edge", vec![t("b"), t("c")], 1),
        named("want", vec![t("x.rs")], 1),
        named("want", vec![t("src/a/long/path.rs")], 1),
    ]
}

#[test]
fn commit_stores_and_reopen_replays() {
    let dir = dir("commit");
    let db = dir.join("core.db");
    let log = Log::default();
    let mut core = open(&db, &log);
    let (event, delta) = core.commit(first_commit()).expect("commit");
    let after_commit = rows(&core);
    drop(core);

    let mut core = open(&db, &log);
    let reopened = rows(&core);
    let (event2, delta2) = core
        .commit(vec![named("edge", vec![t("b"), t("c")], -1)])
        .expect("commit after reopen");
    insta::assert_debug_snapshot!((
        event,
        show(&delta),
        &after_commit,
        reopened == after_commit,
        event2,
        show(&delta2),
        rows(&core),
        log.borrow().clone(),
    ));
}

#[test]
fn reopen_under_another_plan_is_a_mismatch() {
    let dir = dir("mismatch");
    let db = dir.join("core.db");
    let log = Log::default();
    drop(open(&db, &log));
    let mut other = program();
    other.rels.push(rel("extra", [("x", Int)]));
    let found = build(Recipe::default(), other, &db, fakes(&log), BOUNDS).err();
    assert!(
        matches!(found.as_deref(), Some([Diag::Mismatch { recipe, .. }]) if recipe == "engine=dd,store=sqlite,wire=http"),
        "{found:?}"
    );
}

#[test]
fn a_service_round_answers_and_a_retracted_request_hides_its_answers() {
    let dir = dir("service");
    let log = Log::default();
    let mut core = open(&dir.join("core.db"), &log);
    let path = || vec![t("src/a/long/path.rs")];
    let (_, asked) = core.commit(vec![named("want", path(), 1)]).expect("ask");
    let asked_rows = rows(&core);
    let (_, retracted) = core
        .commit(vec![named("want", path(), -1)])
        .expect("retract");
    let retracted_rows = rows(&core);
    let (_, again) = core
        .commit(vec![named("want", path(), 1)])
        .expect("ask again");
    insta::assert_debug_snapshot!((
        show(&asked),
        asked_rows,
        show(&retracted),
        retracted_rows,
        show(&again),
        rows(&core),
        log.borrow().clone(),
    ));
}

#[test]
fn service_rounds_stop_at_the_bound_and_core_still_serves() {
    let dir = dir("rounds");
    let log = Log::default();
    // A derivation's round count is its depth across events, so the chain needs more than BOUNDS.
    let bounds = Bounds { max_rounds: 1024 };
    let mut core = build(
        Recipe::default(),
        program(),
        &dir.join("core.db"),
        fakes(&log),
        bounds,
    )
    .expect("build");
    let endless = core.commit(vec![named("start", vec![Cell::Int(0)], 1)]);
    assert_eq!(
        endless,
        Err(Diag::ServiceRounds {
            max_rounds: MAX_SERVICE_ROUNDS
        })
    );
    let count = |core: &Core, rel: &str| {
        core.query(&format!("SELECT count(*), max(n) FROM {rel}"))
            .expect("count")
            .rows
    };
    let stored = (count(&core, "next_ask"), count(&core, "next_told"));
    let (_, next) = core
        .commit(vec![named("edge", vec![t("p"), t("q")], 1)])
        .expect("next commit");
    insta::assert_debug_snapshot!((stored, show(&next), log.borrow().len()));
}

#[test]
fn a_fixpoint_limit_leaves_the_store_and_the_next_commit_works() {
    let dir = dir("fixpoint");
    let log = Log::default();
    let mut core = open(&dir.join("core.db"), &log);
    let chain: Vec<Named> = (0..20)
        .map(|index| {
            named(
                "edge",
                vec![t(&format!("n{index}")), t(&format!("n{}", index + 1))],
                1,
            )
        })
        .collect();
    let limited = core.commit(chain);
    assert!(
        matches!(limited, Err(Diag::FixpointLimit { max_rounds: 8, .. })),
        "{limited:?}"
    );
    assert_eq!(rows(&core), Vec::<String>::new());
    let (_, delta) = core
        .commit(vec![
            named("edge", vec![t("a"), t("b")], 1),
            named("edge", vec![t("b"), t("c")], 1),
        ])
        .expect("normal commit");
    insta::assert_debug_snapshot!((show(&delta), rows(&core)));
}

#[test]
fn commits_a_client_may_not_make_are_diags_and_change_nothing() {
    let dir = dir("refused");
    let log = Log::default();
    let mut core = open(&dir.join("core.db"), &log);
    let refused: Vec<_> = [
        named("reach", vec![t("a"), t("b")], 1),
        named("ask", vec![t("a")], 1),
        named("told", vec![t("a"), Cell::Int(1)], 1),
        named("nope", vec![], 1),
        named("edge", vec![t("a")], 1),
        named("edge", vec![t("a"), Cell::Int(3)], 1),
        named("start", vec![t("3")], 1),
        named("start", vec![Cell::Null], 1),
    ]
    .into_iter()
    .map(|change| {
        core.commit(vec![named("edge", vec![t("ok"), t("ok")], 1), change])
            .expect_err("refused")
    })
    .collect();
    assert_eq!(rows(&core), Vec::<String>::new());
    insta::assert_debug_snapshot!(refused);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_routes_reach_core_through_the_handle() {
    let dir = dir("http");
    let db = dir.join("core.db");
    let handle = start(move || {
        let log = Log::default();
        build(Recipe::default(), program(), &db, fakes(&log), BOUNDS)
    })
    .expect("start");
    v9::ops::install(Arc::new(handle)).expect("install");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, router()).await });
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let mut seen = Vec::new();
    for (method, path, body) in [
        (
            "POST",
            "/commit",
            Some(json!([
                { "rel": "edge", "cells": [{ "text": "a" }, { "text": "b" }], "diff": 1 },
                { "rel": "want", "cells": [{ "text": "src/a/long/path.rs" }], "diff": 1 },
            ])),
        ),
        (
            "POST",
            "/commit",
            Some(
                json!([{ "rel": "reach", "cells": [{ "text": "a" }, { "text": "b" }], "diff": 1 }]),
            ),
        ),
        (
            "POST",
            "/query",
            Some(json!({ "sql": "SELECT path, size FROM told_text" })),
        ),
        ("GET", "/plan", None),
        ("GET", "/recipe", None),
    ] {
        let request = match method {
            "POST" => client.post(format!("{base}{path}")),
            _ => client.get(format!("{base}{path}")),
        };
        let request = match body {
            Some(body) => request.json(&body),
            None => request,
        };
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let value: Value = response.json().await.unwrap();
        seen.push(json!({ "route": format!("{method} {path}"), "status": status, "body": value }));
    }
    server.abort();
    insta::assert_snapshot!(serde_json::to_string_pretty(&seen).unwrap());
}

/// `v9 serve` with `V9_SYSTEM` set; the first stdout line, then the child is killed.
#[allow(clippy::disallowed_methods)]
fn serve_line(system: &str, db: &Path) -> (String, Option<i32>) {
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_v9"))
        .args(["serve", "reach"])
        .arg(db)
        .args(["--port", "0"])
        .env("V9_SYSTEM", system)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn v9");
    let mut line = String::new();
    std::io::BufReader::new(child.stdout.take().expect("stdout"))
        .read_line(&mut line)
        .expect("read line");
    let _ = child.kill();
    let status = child.wait().expect("wait");
    let line = match line.split_once("http://") {
        Some((head, tail)) => format!("{head}ADDR{}", &tail[tail.find(' ').unwrap_or(0)..]),
        None => line,
    };
    (line.trim_end().to_string(), status.code())
}

#[test]
fn serve_takes_its_recipe_from_v9_system() {
    let dir = dir("bin");
    let (good, _) = serve_line(" Engine=DD ; store=sqlite ", &dir.join("good.db"));
    let (bad, code) = serve_line("engine=duckdb", &dir.join("bad.db"));
    insta::assert_debug_snapshot!((good, bad, code));
}

/// Two files of calls; ryi resolves `c.ts#top -> a.ts#mid -> b.ts#leaf`.
#[allow(clippy::disallowed_methods)]
fn ryi_fixture(root: &Path) -> bool {
    let probe = std::process::Command::new(v9_ryi()).arg("--help").output();
    if !probe.is_ok_and(|output| output.status.success()) {
        return false;
    }
    for (file, text) in [
        ("b.ts", "export function leaf() { return 1; }\n"),
        (
            "a.ts",
            "import { leaf } from \"./b\";\nexport function mid() { return leaf(); }\n",
        ),
        (
            "c.ts",
            "import { mid } from \"./a\";\nexport function top() { return mid(); }\n",
        ),
    ] {
        std::fs::write(root.join(file), text).expect("fixture");
    }
    true
}

#[test]
fn ryi_service_answers_the_reach_program() {
    let dir = dir("ryi");
    let root = dir.join("src");
    std::fs::create_dir_all(&root).expect("root");
    if !ryi_fixture(&root) {
        println!("skip: {} does not run", v9_ryi());
        return;
    }
    let services: Vec<Box<dyn IService>> = vec![Box::new(Ryi::new(v9_ryi()))];
    let mut core = build(
        Recipe::default(),
        reach(),
        &dir.join("core.db"),
        services,
        BOUNDS,
    )
    .expect("build");
    let root_text = root.to_string_lossy().into_owned();
    core.commit(vec![named("root", vec![t(&root_text)], 1)])
        .expect("commit root");
    let decoded = |sql: &str| -> BTreeSet<(String, String)> {
        core.query(sql)
            .expect("query")
            .rows
            .into_iter()
            .map(|row| match &row[..] {
                [Cell::Text(caller), Cell::Text(callee)] => (caller.clone(), callee.clone()),
                other => panic!("{other:?}"),
            })
            .collect()
    };
    let edges: BTreeSet<(String, String)> = ryi_call_edges(&v9_ryi(), &root)
        .expect("ryi")
        .into_iter()
        .map(|edge| (edge.caller, edge.callee))
        .collect();
    assert_eq!(decoded("SELECT caller, callee FROM edge_text"), edges);
    let reach = decoded("SELECT caller, callee FROM reach_text");
    insta::assert_debug_snapshot!(reach);
}
