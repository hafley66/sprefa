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
