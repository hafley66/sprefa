//! The `reach` program on Core. Each case: a first run commits `root` with the stored edges as
//! ryi's answer; a reopen replays them, ryi answers the run's edges, and every reach change must
//! move the kept set to the oracle's evaluation of the run. Then `v9 reach` itself over a ryi
//! fixture: cold, rerun, an edit, rerun.

mod common {
    pub mod oracle;
}

use common::oracle::Oracle;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use v9::_0_id::{id_of, Dict};
use v9::_1_effects::{v9_ryi, CallEdge};
use v9::_3_plan::Row;
use v9::_4_recipe::Recipe;
use v9::_5_ends::{Bounds, Cadence, Cell, Change, IService};
use v9::_8_runtime::_0_programs::reach;
use v9::_8_runtime::_2_ryi::Ryi;
use v9::_8_runtime::_3_core::{build, Core};

type Edge = (u64, u64);

const CASES: &[(&str, &[Edge], &[Edge])] = &[
    ("cold chain", &[], &[(1, 2), (2, 3), (3, 4)]),
    ("cycle", &[], &[(1, 2), (2, 1), (2, 3)]),
    (
        "edge removed mid chain",
        &[(1, 2), (2, 3), (3, 4)],
        &[(1, 2), (3, 4)],
    ),
    (
        "edge added joins chains",
        &[(1, 2), (3, 4)],
        &[(1, 2), (2, 3), (3, 4)],
    ),
    ("cycle broken", &[(1, 2), (2, 1)], &[(1, 2)]),
    ("no change", &[(1, 2), (2, 3)], &[(1, 2), (2, 3)]),
];

const BOUNDS: Bounds = Bounds { max_rounds: 64 };

const ROOT: &str = "src";

/// `Ryi`'s answer diffing with the edges put in `pending` by the test; `call` runs nothing.
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
}

/// A fresh dir per test under the system temp dir.
fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("v9-0-reach-{}", std::process::id()))
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn ints(core: &Core, sql: &str) -> BTreeSet<Row> {
    core.query(sql)
        .expect("query")
        .rows
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|cell| match cell {
                    Cell::Int(value) => value as u64,
                    other => panic!("{sql}: {other:?}"),
                })
                .collect()
        })
        .collect()
}

/// Opens Core on `db` with `answered` seeded from the stored `ryi_edge` rows and `edges` ready as
/// ryi's next answer for ROOT, then commits ROOT and refreshes ryi. Returns the core and the reach rows before the
/// commit.
fn run(db: &Path, edges: &[Edge]) -> (Core, BTreeSet<Row>, Vec<(Row, isize)>) {
    let ryi = Rc::new(RefCell::new(Ryi::new(String::new())));
    let services: Vec<Box<dyn IService>> = vec![Box::new(Inject(Rc::clone(&ryi)))];
    let mut core = build(Recipe::default(), reach(), db, services, BOUNDS).expect("build");
    let before = ints(&core, "SELECT caller, callee FROM reach");
    let mut answered: HashMap<u64, BTreeSet<Row>> = HashMap::new();
    for row in ints(&core, "SELECT root, caller, callee FROM ryi_edge") {
        answered.entry(row[0]).or_default().insert(row);
    }
    let found = edges
        .iter()
        .map(|(from, to)| CallEdge {
            caller: format!("n{from}"),
            callee: format!("n{to}"),
        })
        .collect();
    {
        let mut ryi = ryi.borrow_mut();
        ryi.answered = answered;
        ryi.pending.push((id_of(ROOT.as_bytes()), found));
    }
    let (_, mut changed) = core
        .commit(vec![("root".into(), vec![Cell::Text(ROOT.into())], 1)])
        .expect("commit");
    // A rerun's commit of the stored ROOT changes nothing; the refresh polls the edges.
    if let Some((_, refreshed)) = core.refresh("ryi").expect("refresh") {
        changed.extend(refreshed);
    }
    let reach_changes = changed
        .into_iter()
        .filter(|(rel, _, _)| rel == "reach")
        .map(|(_, cells, diff)| {
            let row = cells
                .iter()
                .map(|cell| match cell {
                    Cell::Text(text) => id_of(text.as_bytes()),
                    other => panic!("{other:?}"),
                })
                .collect();
            (row, diff)
        })
        .collect();
    (core, before, reach_changes)
}

/// The naive oracle's `reach` for ROOT committed and ryi answering `edges`.
fn oracle(core: &Core, edges: &[Edge]) -> BTreeSet<Row> {
    let root = id_of(ROOT.as_bytes());
    let mut input = vec![Change {
        rel: core.ids["root"],
        row: vec![root],
        diff: 1,
    }];
    input.extend(edges.iter().map(|(from, to)| Change {
        rel: core.ids["ryi_edge"],
        row: vec![
            root,
            id_of(format!("n{from}").as_bytes()),
            id_of(format!("n{to}").as_bytes()),
        ],
        diff: 1,
    }));
    let mut oracle = Oracle::default();
    oracle.apply(&input);
    oracle.eval(&core.plan)[core.ids["reach"] as usize].clone()
}

#[test]
fn reach_after_each_run_equals_the_oracle() {
    for (index, (name, stored, edges)) in CASES.iter().enumerate() {
        let db = dir(&format!("case{index}")).join("core.db");
        let (core, _, _) = run(&db, stored);
        assert_eq!(
            ints(&core, "SELECT caller, callee FROM reach"),
            oracle(&core, stored),
            "{name}: stored"
        );
        drop(core);

        let (core, mut kept, changes) = run(&db, edges);
        for (row, diff) in changes {
            match diff {
                1 => assert!(
                    kept.insert(row.clone()),
                    "{name}: +1 on a present row {row:?}"
                ),
                -1 => assert!(kept.remove(&row), "{name}: -1 on an absent row {row:?}"),
                other => panic!("{name}: diff {other} on {row:?}"),
            }
        }
        let want = oracle(&core, edges);
        assert_eq!(kept, want, "{name}: changes");
        assert_eq!(
            ints(&core, "SELECT caller, callee FROM reach"),
            want,
            "{name}: stored"
        );
    }
}

/// Writes the ryi fixture: `c.ts#top -> a.ts#mid -> b.ts#leaf`; `a.ts` without the call when
/// `cut`. False when ryi does not run.
#[allow(clippy::disallowed_methods)]
fn ryi_fixture(root: &Path, cut: bool) -> bool {
    let probe = std::process::Command::new(v9_ryi()).arg("--help").output();
    if !probe.is_ok_and(|output| output.status.success()) {
        return false;
    }
    let mid = if cut {
        "import { leaf } from \"./b\";\nexport function mid() { return 2; }\n"
    } else {
        "import { leaf } from \"./b\";\nexport function mid() { return leaf(); }\n"
    };
    for (file, text) in [
        ("b.ts", "export function leaf() { return 1; }\n"),
        ("a.ts", mid),
        (
            "c.ts",
            "import { mid } from \"./a\";\nexport function top() { return mid(); }\n",
        ),
    ] {
        std::fs::write(root.join(file), text).expect("fixture");
    }
    true
}

/// `v9 reach ROOT DB --check`: stdout without the `ms` line, and the exit code.
#[allow(clippy::disallowed_methods)]
fn v9_reach(root: &Path, db: &Path) -> String {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_v9"))
        .arg("reach")
        .arg(root)
        .arg(db)
        .arg("--check")
        .output()
        .expect("spawn v9");
    let lines: Vec<&str> = std::str::from_utf8(&output.stdout)
        .expect("utf8")
        .lines()
        .filter(|line| !line.starts_with("ms "))
        .collect();
    format!("{} exit {:?}", lines.join(" | "), output.status.code())
}

#[test]
fn v9_reach_cold_rerun_edit_rerun() {
    let dir = dir("bin");
    let root = dir.join("src");
    std::fs::create_dir_all(&root).expect("root");
    if !ryi_fixture(&root, false) {
        println!("skip: {} does not run", v9_ryi());
        return;
    }
    let db = dir.join("reach.db");
    let mut seen = vec![v9_reach(&root, &db), v9_reach(&root, &db)];
    ryi_fixture(&root, true);
    seen.push(v9_reach(&root, &db));
    seen.push(v9_reach(&root, &db));
    insta::assert_debug_snapshot!(seen);
}
