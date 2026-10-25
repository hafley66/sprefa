//! Each program under `examples/` on Core in-process through `_5_run::run`, its `<rel>_text` views
//! snapshotted after each commit; `Core::refresh` over a reopened store; a failing service.

#[allow(dead_code)]
#[path = "../examples/0_reach.rs"]
mod reach_example;

#[allow(dead_code)]
#[path = "../examples/1_negation_add.rs"]
mod negation_example;

#[allow(dead_code)]
#[path = "../examples/2_service.rs"]
mod service_example;

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::rc::Rc;
use v9::_0_id::{id_of, Dict};
use v9::_1_effects::CallEdge;
use v9::_3_plan::Row;
use v9::_5_ends::{Cadence, Cell, Change, Diag, Diff, IService};
use v9::_8_runtime::_0_programs::reach;
use v9::_8_runtime::_2_ryi::Ryi;
use v9::_8_runtime::_3_core::{Core, Named};
use v9::_8_runtime::_5_run::{run, views};

/// A fresh dir per test under the system temp dir.
fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("v9-8-examples-{}", std::process::id()))
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn texts(rel: &str, rows: &[&[&str]], diff: Diff) -> Vec<Named> {
    rows.iter()
        .map(|row| {
            let cells = row
                .iter()
                .map(|text| Cell::Text(text.to_string()))
                .collect();
            (rel.to_string(), cells, diff)
        })
        .collect()
}

/// Commits each batch in order; the views after each.
fn steps(core: &mut Core, batches: Vec<Vec<Named>>) -> Vec<Vec<String>> {
    batches
        .into_iter()
        .map(|batch| {
            core.commit(batch).expect("commit");
            views(core).expect("views")
        })
        .collect()
}

#[test]
fn reach_example_derives_and_retracts() {
    let mut core = run(
        reach_example::program(),
        reach_example::services(),
        &dir("reach").join("core.db"),
    )
    .expect("run");
    let seen = steps(
        &mut core,
        vec![
            texts(
                "edge",
                &[&["a", "b"], &["b", "c"], &["c", "a"], &["c", "d"]],
                1,
            ),
            texts("edge", &[&["c", "a"]], -1),
        ],
    );
    insta::assert_debug_snapshot!(seen);
}

#[test]
fn negation_add_example_strata_and_builtins() {
    let mut core = run(
        negation_example::program(),
        negation_example::services(),
        &dir("negation").join("core.db"),
    )
    .expect("run");
    let chain: &[&[&str]] = &[
        &["s", "a"],
        &["a", "b"],
        &["b", "c"],
        &["c", "d"],
        &["d", "e"],
        &["x", "y"],
    ];
    let seen = steps(
        &mut core,
        vec![
            [
                texts("step", chain, 1),
                texts("start", &[&["s"], &["x"]], 1),
            ]
            .concat(),
            texts("blocked", &[&["b"]], 1),
            texts("start", &[&["x"]], -1),
        ],
    );
    insta::assert_debug_snapshot!(seen);
}

#[test]
fn service_example_answers_and_refresh_after_reopen_changes_nothing() {
    let db = dir("service").join("core.db");
    let mut core = run(service_example::program(), service_example::services(), &db).expect("run");
    let seen = steps(
        &mut core,
        vec![
            texts("word", &[&["hi"], &["kernel"], &["datalog"]], 1),
            texts("word", &[&["kernel"]], -1),
        ],
    );
    drop(core);
    let mut core =
        run(service_example::program(), service_example::services(), &db).expect("reopen");
    let refreshed = core.refresh("len").expect("refresh");
    insta::assert_debug_snapshot!((seen, refreshed, views(&core).expect("views")));
}

/// `Ryi`'s answer diffing over edges the test puts in `pending`; `call` runs nothing.
struct Inject(Rc<RefCell<Ryi>>);

impl IService for Inject {
    fn service(&self) -> &str {
        "ryi"
    }
    fn cadence(&self) -> Cadence {
        Cadence::Once
    }
    fn call(&mut self, _requests: &[Change], _dict: &Dict) {}
    fn poll(&mut self, dict: &mut Dict, out: &mut Vec<Change>) {
        self.0.borrow_mut().poll(dict, out);
    }
    fn seed(&mut self, answers: &[Change]) {
        self.0.borrow_mut().seed(answers);
    }
}

fn found(edges: &[(&str, &str)]) -> Vec<CallEdge> {
    edges
        .iter()
        .map(|(caller, callee)| CallEdge {
            caller: caller.to_string(),
            callee: callee.to_string(),
        })
        .collect()
}

#[test]
fn refresh_after_reopen_moves_stored_answers_to_the_new_ones() {
    let db = dir("refresh").join("core.db");
    let root = id_of(b"src");
    let open = || {
        let ryi = Rc::new(RefCell::new(Ryi::new(String::new())));
        let services: Vec<Box<dyn IService>> = vec![Box::new(Inject(Rc::clone(&ryi)))];
        (run(reach(), services, &db).expect("run"), ryi)
    };
    let (mut core, ryi) = open();
    ryi.borrow_mut()
        .pending
        .push((root, found(&[("a", "b"), ("b", "c")])));
    core.commit(vec![("root".into(), vec![Cell::Text("src".into())], 1)])
        .expect("commit root");
    let first = views(&core).expect("views");
    drop(core);

    let (mut core, ryi) = open();
    let seeded: BTreeSet<Row> = ryi.borrow().answered[&root].clone();
    let stored: BTreeSet<Row> = core
        .query("SELECT root, caller, callee FROM ryi_edge")
        .expect("query")
        .rows
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|cell| match cell {
                    Cell::Int(value) => value as u64,
                    other => panic!("{other:?}"),
                })
                .collect()
        })
        .collect();
    assert_eq!(seeded, stored);
    ryi.borrow_mut()
        .pending
        .push((root, found(&[("a", "b"), ("c", "d")])));
    let (_, changed) = core.refresh("ryi").expect("refresh").expect("changes");
    let unknown = core.refresh("nope");
    insta::assert_debug_snapshot!((first, changed, views(&core).expect("views"), unknown));
}

#[test]
fn a_service_error_stops_the_commit_with_a_diag() {
    let services: Vec<Box<dyn IService>> =
        vec![Box::new(Ryi::new("/nonexistent/v9-no-ryi".into()))];
    let dir = dir("error");
    let mut core = run(reach(), services, &dir.join("core.db")).expect("run");
    let root = dir.to_string_lossy().into_owned();
    let failed = core.commit(vec![("root".into(), vec![Cell::Text(root.clone())], 1)]);
    assert!(
        matches!(&failed, Err(Diag::Service { service, message })
            if service == "ryi"
                && message.starts_with(&format!("{root}: spawn /nonexistent/v9-no-ryi"))),
        "{failed:?}"
    );
}
