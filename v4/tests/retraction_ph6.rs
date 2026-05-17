//! Phase 6 binding tests.
//!
//! Test 1 (`fs_read_re_file_edit_one_retract_one_assert`) is the core
//! value of Phase 6: it drives the LITERAL content-threaded chain
//! `fs > read > re` (real `FsComponent`/`ReadComponent`/`ReComponent`,
//! no rule/fact wrapper, no `MatchOp` shim) over a tmpdir, edits ONE
//! fn name in ONE file, runs the dirty-source loop, and asserts
//! exactly 1 Retract + 1 Assert with every other match untouched —
//! for BOTH a length-neutral and a length-changing rename. This is
//! what closes the Phase-4 deviation #3 (`fs>read>re` retraction gap):
//! before Phase 6 the `re` op's memo `in_key` was the hash of the file
//! BYTES carried in its in-cursor, so an edit looked brand-new (memo
//! MISS) and `reconcile` never fired. Phase 6 keys the `re` owner's
//! memo on `(op_instance ⊕ recorded SourceId set)` (source-keyed
//! in_key), so an edit is a STALE probe and reconcile produces the
//! minimal delta.

use std::sync::Arc;

use effect_runtime::v2::{
    Component, FactStore, PipeInstance, QueueBackend, QueueRow, RenderCtx,
};
use tempfile::tempdir;
use v4::dirty_source::DirtySourceDriver;
use v4::fact::SqliteFactStore;
use v4::memo_seam_impl::V4MemoSeam;
use v4::mounted_query::{record_fact_supports, SupportLedger};
use v4::runtime_graph::RuntimeGraph;
use v4::store::SprfStore;
use v4::v2_ops::{FsComponent, ReComponent, ReadComponent};
use v4::Cursor;

const SINK: &str = "ph6_sink";

fn sqlite_graph(path: &std::path::Path) -> (Arc<dyn FactStore<Cursor>>, Arc<RuntimeGraph>) {
    let facts: Arc<dyn FactStore<Cursor>> =
        Arc::new(SqliteFactStore::<Cursor>::open_file(path).unwrap());
    let store = SprfStore::new(facts.clone());
    let graph = Arc::new(RuntimeGraph::new(store, facts.clone()));
    (facts, graph)
}

/// Terminal of the literal chain. Writes each `re` match into the
/// SINK table keyed by its STABLE identity (FS + NAME — never the byte
/// offsets), and registers one SUPPORT triple per match under the
/// seam's stable support cursor id. A span-shifted-but-same-NAME
/// match re-renders to the SAME sink row id and SAME support id, so
/// the re-add is idempotent (`mult` stays 1) and only the renamed
/// match's row is torn down.
struct SinkWrite {
    store: Arc<dyn FactStore<Cursor>>,
    seam: Arc<V4MemoSeam>,
}

impl Component for SinkWrite {
    type Next = Cursor;
    fn kind(&self) -> &'static str {
        "test.ph6.sinkwrite"
    }
    fn dispatch(
        &self,
        _ctx: &RenderCtx,
        rows: &[QueueRow<Cursor>],
        _queue: &dyn QueueBackend<Cursor>,
    ) {
        self.store.declare(SINK, &["FS", "NAME"]);
        for r in rows {
            let c = r.value.as_ref();
            let (Some(fs), Some(name)) = (c.get("FS"), c.get("NAME")) else {
                continue;
            };
            // Sink row keyed by STABLE identity only (FS + NAME). A
            // span shift of the same fn maps to the SAME sink row id.
            let mut srow = Cursor::default();
            srow.set("FS", fs);
            srow.set("NAME", name);
            let srow = Arc::new(srow);
            let sink_row_id = self.store.row_id_for(SINK, srow.as_ref());
            self.store.insert_batch(SINK, vec![srow]);
            // Support keyed by the seam's stable support cursor id so
            // reconcile's Retract teardown computes the identical id.
            let support_cursor_id = self.seam.support_cursor_id(c);
            record_fact_supports(
                self.store.as_ref(),
                &support_cursor_id,
                SINK,
                &sink_row_id,
            );
        }
    }
}

fn build_driver(
    root: &std::path::Path,
    graph: Arc<RuntimeGraph>,
    store: Arc<dyn FactStore<Cursor>>,
) -> (DirtySourceDriver, Arc<V4MemoSeam>) {
    // Source-keyed seam: identity = capture NAME, payload = none
    // (a span shift with a stable NAME is a NOOP), in_source = FS
    // (memo in_key is the path, stable under file-byte edits).
    let seam = V4MemoSeam::with_capture_keying(
        graph.clone(),
        vec!["NAME".into()],
        vec![],
        vec!["FS".into()],
    );
    let fs: Arc<dyn Component<Next = Cursor>> = Arc::new(
        FsComponent::new(root.to_path_buf(), 64).with_include_exts(vec!["rs".into()]),
    );
    let read: Arc<dyn Component<Next = Cursor>> =
        Arc::new(ReadComponent::new().with_root(root.to_path_buf()));
    // `re\`fn (?P<NAME>[a-z_]+)\`` — matches over cursor.value (the
    // file bytes `read` produced); NAME is the capture.
    let re: Arc<dyn Component<Next = Cursor>> =
        Arc::new(ReComponent::new(r"fn (?P<NAME>[a-z_]+)", &["NAME"]));
    let sink: Arc<dyn Component<Next = Cursor>> = Arc::new(SinkWrite {
        store,
        seam: seam.clone(),
    });
    let pipe = Arc::new(PipeInstance::new(vec![fs, read, re, sink]));
    let driver = DirtySourceDriver::new(
        pipe,
        graph,
        seam.clone(),
        vec![Arc::new(Cursor::default())],
    );
    (driver, seam)
}

fn sink_names(store: &dyn FactStore<Cursor>) -> Vec<String> {
    let mut v: Vec<String> = store
        .rows_of(SINK)
        .iter()
        .filter_map(|r| r.get("NAME").map(|s| s.to_string()))
        .collect();
    v.sort();
    v
}

fn sink_row_id_for_name(store: &dyn FactStore<Cursor>, name: &str) -> Option<String> {
    store
        .rows_of(SINK)
        .iter()
        .find(|r| r.get("NAME") == Some(name))
        .map(|r| store.row_id_for(SINK, r.as_ref()))
}

#[test]
fn fs_read_re_file_edit_one_retract_one_assert() {
    // ── Length-NEUTRAL rename ────────────────────────────────────────
    {
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("src");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("a.rs"), "fn alpha() {}\nfn bravo() {}\nfn gamma() {}\n")
            .unwrap();
        // Second, UNEDITED file in the corpus — must do 0 dispatch on
        // the dirty sweep (replay).
        std::fs::write(root.join("b.rs"), "fn keep() {}\nfn other() {}\n").unwrap();
        let db = tmp.path().join("rt.db");
        let (facts, graph) = sqlite_graph(&db);
        let (driver, seam) = build_driver(&root, graph.clone(), facts.clone());

        // Cold prime: 3 + 2 matches → 5 sink rows, 5 Assert.
        driver.prime();
        assert_eq!(
            sink_names(facts.as_ref()),
            vec!["alpha", "bravo", "gamma", "keep", "other"],
            "cold: all five fn names sink"
        );
        assert_eq!(seam.assert_count(), 5, "cold: 5 Assert");
        assert_eq!(seam.retract_count(), 0, "cold: 0 Retract");

        let led = SupportLedger::new(facts.as_ref());
        let bravo_old = sink_row_id_for_name(facts.as_ref(), "bravo").unwrap();
        let gamma_id = sink_row_id_for_name(facts.as_ref(), "gamma").unwrap();
        assert_eq!(led.sink_mult(SINK, &bravo_old), 1, "bravo cold mult == 1");

        // Edit ONE file, ONE fn name, same byte length.
        std::fs::write(
            root.join("a.rs"),
            "fn alpha() {}\nfn bobby() {}\nfn gamma() {}\n",
        )
        .unwrap();
        let a_path = root.join("a.rs");
        let a_path_s = a_path.to_str().unwrap();

        // Diagram-A loop: bump clock → scoped MEMO_DEPS reverse-lookup
        // → mark dirty → capped sweep → reconcile + DRed teardown.
        let marked = driver.on_file_changed(a_path_s);
        assert!(marked >= 1, "the `re` owner for a.rs is marked dirty");
        let handled = driver
            .sweep_to_quiescence(v4::dirty_source::DEFAULT_ROUND_CAP)
            .expect("sweep reaches quiescence within the cap");
        assert!(handled >= 1, "at least one dirty row retired");

        assert_eq!(
            seam.retract_count(),
            1,
            "length-neutral rename: exactly 1 Retract (old NAME=bravo)"
        );
        assert_eq!(
            seam.assert_count(),
            6,
            "5 cold + 1 re-assert (new NAME=bobby); unedited file replayed"
        );
        assert_eq!(
            sink_names(facts.as_ref()),
            vec!["alpha", "bobby", "gamma", "keep", "other"],
            "only bravo→bobby changed; all others untouched"
        );
        // Retracted row's support mult reached 0 and it left the sink.
        assert_eq!(
            led.sink_mult(SINK, &bravo_old),
            0,
            "retracted bravo row mult driven to 0 by cascade_retract"
        );
        assert!(
            !facts
                .rows_of(SINK)
                .iter()
                .any(|r| facts.row_id_for(SINK, r.as_ref()) == bravo_old),
            "retracted bravo row left the sink table"
        );
        // Co-derived gamma untouched (same file, span unchanged here).
        assert_eq!(
            led.sink_mult(SINK, &gamma_id),
            1,
            "co-derived gamma row untouched"
        );
    }

    // ── Length-CHANGING rename ───────────────────────────────────────
    {
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("src");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("a.rs"), "fn alpha() {}\nfn bravo() {}\nfn gamma() {}\n")
            .unwrap();
        std::fs::write(root.join("b.rs"), "fn keep() {}\n").unwrap();
        let db = tmp.path().join("rt.db");
        let (facts, graph) = sqlite_graph(&db);
        let (driver, seam) = build_driver(&root, graph.clone(), facts.clone());

        driver.prime();
        assert_eq!(seam.assert_count(), 4, "cold: 4 Assert (3 + 1)");
        let led = SupportLedger::new(facts.as_ref());
        let bravo_old = sink_row_id_for_name(facts.as_ref(), "bravo").unwrap();
        let gamma_id = sink_row_id_for_name(facts.as_ref(), "gamma").unwrap();

        // Rename `bravo` → `verylongname` (LONGER → `gamma`'s byte
        // offsets shift, but its captured NAME is unchanged).
        std::fs::write(
            root.join("a.rs"),
            "fn alpha() {}\nfn verylongname() {}\nfn gamma() {}\n",
        )
        .unwrap();
        let a_path = root.join("a.rs");

        driver
            .react_to_file_change(a_path.to_str().unwrap())
            .expect("sweep reaches quiescence");

        assert_eq!(
            seam.retract_count(),
            1,
            "length-changing rename: still exactly 1 Retract \
             (gamma's span shifted but its NAME is stable → NOOP)"
        );
        assert_eq!(
            seam.assert_count(),
            5,
            "4 cold + 1 re-assert (verylongname); gamma NOT re-asserted"
        );
        assert_eq!(
            sink_names(facts.as_ref()),
            vec!["alpha", "gamma", "keep", "verylongname"],
            "span shift of gamma did not churn it; only bravo replaced"
        );
        assert_eq!(
            led.sink_mult(SINK, &bravo_old),
            0,
            "renamed bravo's support mult driven to 0"
        );
        assert!(
            !facts
                .rows_of(SINK)
                .iter()
                .any(|r| facts.row_id_for(SINK, r.as_ref()) == bravo_old),
            "renamed bravo's sink row removed"
        );
        assert_eq!(
            led.sink_mult(SINK, &gamma_id),
            1,
            "span-shifted gamma row survived with mult 1 (Ph5 path)"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────
// Pieces 3+4: stratify + semi-naive fixpoint.
// ─────────────────────────────────────────────────────────────────────

use v4::fixpoint::{
    eval_all, eval_stratum, support_cursor_ids_for_sink, EvalRule,
};
use v4::mounted_query::cascade_retract;
use v4::stratify::{stratify, RuleDep};

const EDGE: &str = "edge";
const REACH: &str = "reach";
const DEP: &str = "dep";
const STALE: &str = "stale";

fn open_store(path: &std::path::Path) -> Arc<dyn FactStore<Cursor>> {
    Arc::new(SqliteFactStore::<Cursor>::open_file(path).unwrap())
}

/// Parse `import <Y> from <X>` lines from a fixture into `edge(X, Y)`
/// rows (X imports Y ⇒ edge X→Y). The plan's "edge parsed from a small
/// fixture (e.g. import lines)".
fn load_edges_from_fixture(store: &dyn FactStore<Cursor>, text: &str) {
    store.declare(EDGE, &["X", "Y"]);
    // Clear then reload (a fixture edit re-derives the base relation).
    for r in store.rows_of(EDGE) {
        let x = r.get("X").unwrap_or("").to_string();
        let y = r.get("Y").unwrap_or("").to_string();
        store.delete_matching(EDGE, &[("X", x.as_str()), ("Y", y.as_str())]);
    }
    let re = regex::Regex::new(r"import (\w+) from (\w+)").unwrap();
    for caps in re.captures_iter(text) {
        let y = caps.get(1).unwrap().as_str();
        let x = caps.get(2).unwrap().as_str();
        let mut row = Cursor::default();
        row.set("X", x);
        row.set("Y", y);
        store.insert_batch(EDGE, vec![Arc::new(row)]);
    }
}

/// The recursive `reach` rule, as ONE semi-naive step (not a SQL
/// WITH RECURSIVE — the fixpoint loop in `eval_stratum` IS the
/// recursion). Round 1 (reach empty) yields the base edges; each later
/// round joins the growing `reach` against `edge` for the next hop.
/// `__witness` = the immediate edge that produced the row, so a base
/// change retracts exactly the derivations through it.
fn reach_rule() -> EvalRule {
    EvalRule {
        name: REACH.into(),
        sink: REACH.into(),
        sink_cols: vec!["X".into(), "Y".into()],
        deps: vec![EDGE.into(), REACH.into()],
        sql: "
            SELECT X AS X, Y AS Y, ('e:'||X||'>'||Y) AS __witness FROM edge
            UNION
            SELECT r.X AS X, e.Y AS Y, ('e:'||e.X||'>'||e.Y) AS __witness
              FROM reach r JOIN edge e ON r.Y = e.X
        "
        .into(),
    }
}

fn reach_set(store: &dyn FactStore<Cursor>) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = store
        .rows_of(REACH)
        .iter()
        .filter_map(|r| Some((r.get("X")?.to_string(), r.get("Y")?.to_string())))
        .collect();
    v.sort();
    v.dedup();
    v
}

#[test]
fn recursive_transitive_closure_terminates_and_retracts() {
    let tmp = tempdir().unwrap();
    let db = tmp.path().join("rt.db");
    let store = open_store(&db);

    // Diamond + tail: a→b→d, a→c→d, d→e. reach(a,d) is derivable TWO
    // ways (through b and through c) — the Phase-5 mult case.
    let fixture_v1 = "\
        import b from a\n\
        import d from b\n\
        import c from a\n\
        import d from c\n\
        import e from d\n";
    load_edges_from_fixture(store.as_ref(), fixture_v1);

    let reach = reach_rule();
    // TERMINATES: the cap is generous; a correct semi-naive loop
    // settles in a few rounds. `eval_stratum` returns Ok (NOT
    // RoundCap) — that is the termination assertion.
    let derived = eval_stratum(store.as_ref(), std::slice::from_ref(&reach), 10_000)
        .expect("recursive closure terminates within the cap");
    assert!(derived > 0, "closure derived some rows");

    let closure_v1 = reach_set(store.as_ref());
    // Full transitive closure of the diamond+tail.
    let expected_v1 = vec![
        ("a", "b"),
        ("a", "c"),
        ("a", "d"),
        ("a", "e"),
        ("b", "d"),
        ("b", "e"),
        ("c", "d"),
        ("c", "e"),
        ("d", "e"),
    ];
    let got: Vec<(&str, &str)> = closure_v1
        .iter()
        .map(|(x, y)| (x.as_str(), y.as_str()))
        .collect();
    assert_eq!(got, expected_v1, "transitive closure is correct");

    let led = SupportLedger::new(store.as_ref());
    let reach_id = |x: &str, y: &str| -> String {
        let mut c = Cursor::default();
        c.set("X", x);
        c.set("Y", y);
        store.row_id_for(REACH, &c)
    };
    // reach(a,d) is double-derived (witness e:b>d AND e:c>d) ⇒ mult 2.
    assert_eq!(
        led.sink_mult(REACH, &reach_id("a", "d")),
        2,
        "reach(a,d) derivable two ways → Phase-5 mult == 2"
    );
    let ad_id = reach_id("a", "d");
    let bd_id = reach_id("b", "d");
    let be_id = reach_id("b", "e");

    // Edit the fixture: REMOVE the `b→d` edge mid-graph.
    let fixture_v2 = "\
        import b from a\n\
        import c from a\n\
        import d from c\n\
        import e from d\n";
    load_edges_from_fixture(store.as_ref(), fixture_v2);

    // DRed: over-delete the possibly-affected closure (cascade_retract
    // every reach support — Phase-5 delete half, mult-aware), then
    // re-derive (the semi-naive re-derive half). A row still derivable
    // comes back (its surviving witness re-asserts mult > 0); a row
    // with no path stays gone.
    let doomed = support_cursor_ids_for_sink(store.as_ref(), REACH);
    cascade_retract(store.as_ref(), &doomed);
    assert!(
        reach_set(store.as_ref()).is_empty(),
        "DRed over-delete cleared the reach closure"
    );
    let rederived = eval_stratum(store.as_ref(), std::slice::from_ref(&reach), 10_000)
        .expect("re-derive terminates within the cap");
    assert!(rederived > 0, "re-derive produced rows");

    let closure_v2 = reach_set(store.as_ref());
    // b→d gone: b reaches nothing; a still reaches d/e VIA c (alternate
    // path). reach(a,d) and reach(a,e) SURVIVE; reach(b,d)/reach(b,e)
    // are retracted (lost ALL derivation paths).
    let expected_v2 = vec![
        ("a", "b"),
        ("a", "c"),
        ("a", "d"),
        ("a", "e"),
        ("c", "d"),
        ("c", "e"),
        ("d", "e"),
    ];
    let got2: Vec<(&str, &str)> = closure_v2
        .iter()
        .map(|(x, y)| (x.as_str(), y.as_str()))
        .collect();
    assert_eq!(
        got2, expected_v2,
        "DRed over the closure: dead branch retracted, alternate path survives"
    );
    assert!(
        led.sink_mult(REACH, &ad_id) >= 1,
        "reach(a,d) survived via the c-path (Phase-5 alternate derivation)"
    );
    assert_eq!(
        led.sink_mult(REACH, &bd_id),
        0,
        "reach(b,d) lost all paths → retracted"
    );
    assert_eq!(
        led.sink_mult(REACH, &be_id),
        0,
        "reach(b,e) lost all paths → retracted"
    );
}

#[test]
fn stratified_negation_evaluated_after_base() {
    let tmp = tempdir().unwrap();
    let db = tmp.path().join("rt.db");
    let store = open_store(&db);

    // `stale(P) :- dep(P), not reach(P, _)` — a package P with a
    // declared dep that is NOT reachable is stale. `reach` is the
    // recursive relation from test 2; `stale` antijoins it, so
    // stratify MUST place `stale` strictly above `reach`.
    let strata_plan = stratify(&[
        RuleDep::new(REACH).reads(EDGE).reads(REACH),
        RuleDep::new(STALE).reads(DEP).negates(REACH),
    ])
    .expect("stratifiable: one negative edge, no cycle");
    assert_eq!(strata_plan.len(), 2, "two strata");
    assert_eq!(strata_plan[0].rules, vec![REACH], "reach is the base stratum");
    assert_eq!(
        strata_plan[1].rules,
        vec![STALE],
        "stale antijoins reach → strictly higher stratum"
    );

    // Base: a→b, b→c. dep(P) for a, b, x (x has no edges at all).
    load_edges_from_fixture(store.as_ref(), "import b from a\nimport c from b\n");
    store.declare(DEP, &["P"]);
    for p in ["a", "b", "x"] {
        let mut c = Cursor::default();
        c.set("P", p);
        store.insert_batch(DEP, vec![Arc::new(c)]);
    }

    let reach = reach_rule();
    // `stale` reads the COMPLETE reach relation (antijoin). Because
    // `eval_all` runs stratum 0 (reach) to its fixed point BEFORE
    // stratum 1 (stale), the NOT EXISTS sees a finished `reach`.
    let stale = EvalRule {
        name: STALE.into(),
        sink: STALE.into(),
        sink_cols: vec!["P".into()],
        deps: vec![DEP.into(), REACH.into()],
        sql: "
            SELECT d.P AS P, ('p:'||d.P) AS __witness
              FROM dep d
             WHERE NOT EXISTS (SELECT 1 FROM reach r WHERE r.X = d.P)
        "
        .into(),
    };

    let strata: Vec<Vec<EvalRule>> = vec![vec![reach.clone()], vec![stale.clone()]];
    eval_all(store.as_ref(), &strata, 10_000).expect("stratified eval terminates");

    let stale_set = |store: &dyn FactStore<Cursor>| -> Vec<String> {
        let mut v: Vec<String> = store
            .rows_of(STALE)
            .iter()
            .filter_map(|r| r.get("P").map(|s| s.to_string()))
            .collect();
        v.sort();
        v.dedup();
        v
    };
    // a and b have outgoing reach; x does not → only x is stale.
    assert_eq!(
        stale_set(store.as_ref()),
        vec!["x".to_string()],
        "stale computed against the COMPLETE reach (stratum order)"
    );

    // Source edit that FLIPS a reach fact: add an edge out of x
    // (`x→a`) so x is now reachable-from ⇒ x's `stale` row clears
    // (antijoin auto-clear). Re-stratified-eval after a DRed
    // over-delete of both derived relations.
    load_edges_from_fixture(
        store.as_ref(),
        "import b from a\nimport c from b\nimport a from x\n",
    );
    for sink in [REACH, STALE] {
        let doomed = support_cursor_ids_for_sink(store.as_ref(), sink);
        cascade_retract(store.as_ref(), &doomed);
    }
    eval_all(store.as_ref(), &strata, 10_000).expect("re-eval terminates");
    assert!(
        stale_set(store.as_ref()).is_empty(),
        "x became reachable → its stale row cleared (antijoin auto-clear)"
    );

    // And the converse: remove every edge → nobody is reachable ⇒
    // every dep is stale (antijoin auto-raise).
    load_edges_from_fixture(store.as_ref(), "");
    for sink in [REACH, STALE] {
        let doomed = support_cursor_ids_for_sink(store.as_ref(), sink);
        cascade_retract(store.as_ref(), &doomed);
    }
    eval_all(store.as_ref(), &strata, 10_000).expect("re-eval terminates");
    assert_eq!(
        stale_set(store.as_ref()),
        vec!["a".to_string(), "b".to_string(), "x".to_string()],
        "no reach → every dep stale (antijoin auto-raise)"
    );
}

#[test]
fn unstratifiable_negative_cycle_is_diagnostic_not_hang() {
    // p :- not q ;  q :- not p — a negative cycle. `stratify` MUST
    // return a lower-time diagnostic (NOT panic, NOT hang). The whole
    // body is trivially bounded (stratify is a pure graph algorithm
    // with no loop that can spin), so no timeout harness is needed —
    // but we still assert it returns promptly with the diagnostic.
    let res = stratify(&[
        RuleDep::new("p").negates("q"),
        RuleDep::new("q").negates("p"),
    ]);
    let err = res.expect_err("negative cycle must NOT stratify");
    assert!(
        err.cycle.contains(&"p".to_string()) && err.cycle.contains(&"q".to_string()),
        "diagnostic names the cycle members: {:?}",
        err.cycle
    );
    let diag = err.to_diag();
    assert_eq!(
        &*diag.code, "lower/unstratifiable-negation",
        "same lower-time diag vocabulary as other lower diagnostics"
    );
    assert_eq!(diag.severity, effect_runtime::v2::Severity::Error);
    assert!(
        diag.message.contains("negative cycle"),
        "message explains the user error: {}",
        diag.message
    );
}
