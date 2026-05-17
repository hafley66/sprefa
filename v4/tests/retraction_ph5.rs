//! Phase 5 binding tests: integer support `mult` + DRed
//! `cascade_retract`, counted teardown.
//!
//! The plan's Phase-5 invariant: a row is present in its sink table
//! IFF `sum(SUPPORT.mult) > 0` for that `row_id`. `SupportLedger::add`
//! registers one derivation path (one distinct deriving cursor =
//! one triple = one unit of mult); `cascade_retract` is the DRed
//! delete half — decrement, delete + descend only when `sum(mult)`
//! reaches 0, keep + stop-descending when another path still derives
//! the row.
//!
//! Tests 1 & 2 drive the ledger/cascade directly (the plan's Layer-1
//! `SupportLedger` surface). Test 3 adapts the Ph4 generic non-rule
//! pipe so the single `Retract` from `reconcile` goes through the
//! counted teardown.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use effect_runtime::v2::{
    expand, Component, ExpandOpts, FactStore, MemQueue, Next, Node, PipeInstance, QueueBackend,
    QueueRow, RenderCtx,
};
use tempfile::tempdir;
use v4::fact::SqliteFactStore;
use v4::memo_seam_impl::V4MemoSeam;
use v4::mounted_query::{cascade_retract, SupportLedger};
use v4::runtime_graph::RuntimeGraph;
use v4::source_clock::{SourceClock, SourceId};
use v4::store::SprfStore;
use v4::Cursor;

const SINK: &str = "ph5_sink";
const SINK_VAL: &str = "val";

fn open_store(path: &std::path::Path) -> Arc<dyn FactStore<Cursor>> {
    Arc::new(SqliteFactStore::<Cursor>::open_file(path).unwrap())
}

/// Insert one sink row carrying `val`; return its store row id (the
/// id `SupportLedger`/`cascade_retract` key the sink `(table,row_id)`
/// on).
fn put_sink(store: &dyn FactStore<Cursor>, val: &str) -> String {
    store.declare(SINK, &[SINK_VAL]);
    let mut row = Cursor::default();
    row.set(SINK_VAL, val);
    let row = Arc::new(row);
    let row_id = store.row_id_for(SINK, row.as_ref());
    store.insert_batch(SINK, vec![row]);
    row_id
}

/// `rows_of`/`read_where` project only declared user columns (never
/// `_id`), so presence is checked by re-deriving each row's store id
/// from its declared columns and matching it.
fn sink_present(store: &dyn FactStore<Cursor>, row_id: &str) -> bool {
    store
        .rows_of(SINK)
        .iter()
        .any(|r| store.row_id_for(SINK, r.as_ref()) == row_id)
}

#[test]
fn row_with_two_supports_survives_losing_one() {
    // A sink row derived/supported by two DISTINCT (owner,in_key)
    // sources — two distinct deriving cursor ids => two SUPPORT
    // triples. Retract one → sum(mult) still > 0 → row stays.
    // Retract the second → mult hits 0 → row deleted.
    let tmp = tempdir().unwrap();
    let store = open_store(&tmp.path().join("rt.db"));
    let row_id = put_sink(store.as_ref(), "shared");

    let led = SupportLedger::new(store.as_ref());
    led.add("ownerA", SINK, &row_id);
    led.add("ownerB", SINK, &row_id);
    assert_eq!(
        led.sink_mult(SINK, &row_id),
        2,
        "two distinct paths => sum(mult) == 2"
    );
    assert!(sink_present(store.as_ref(), &row_id));

    // Retract path A. sum(mult) drops to 1 (> 0) → row survives, NOT
    // deleted.
    cascade_retract(store.as_ref(), &["ownerA".to_string()]);
    assert_eq!(
        led.sink_mult(SINK, &row_id),
        1,
        "losing one of two paths: sum(mult) == 1"
    );
    assert!(
        sink_present(store.as_ref(), &row_id),
        "row survives losing one of two supports"
    );

    // Retract path B. sum(mult) hits 0 → row deleted.
    cascade_retract(store.as_ref(), &["ownerB".to_string()]);
    assert_eq!(
        led.sink_mult(SINK, &row_id),
        0,
        "last path gone: sum(mult) == 0"
    );
    assert!(
        !sink_present(store.as_ref(), &row_id),
        "row deleted when its last support is retracted"
    );
}

#[test]
fn cascade_retract_deletes_transitive_children_only_when_unsupported() {
    // Support chain r0 → r1 → r2 where r1 also has a SECOND
    // independent supporter. cascade_retract(r0):
    //   - r0's triple decremented to 0, r0 deleted, descend.
    //   - r1 decremented but r1 still has its second supporter
    //     (sum(mult) > 0) → r1 SURVIVES, stop descending → r2
    //     SURVIVES (DRed over-delete bounded by stop-at-supported).
    // Remove r1's other support → r1 collapses, descend → r2
    // collapses.
    let tmp = tempdir().unwrap();
    let store = open_store(&tmp.path().join("rt.db"));

    let r0 = put_sink(store.as_ref(), "r0");
    let r1 = put_sink(store.as_ref(), "r1");
    let r2 = put_sink(store.as_ref(), "r2");

    let led = SupportLedger::new(store.as_ref());
    // root cursor "src0" supports r0; r0 supports r1; r1 supports r2.
    // The chain link: a deleted sink row's `row_id` is the support
    // cursor id of the next level (cascade chases SUPPORT_CURSOR_ID
    // == the deleted row id).
    led.add("src0", SINK, &r0);
    led.add(&r0, SINK, &r1);
    led.add(&r1, SINK, &r2);
    // r1's SECOND independent supporter (a different deriving cursor).
    led.add("alt", SINK, &r1);

    assert_eq!(led.sink_mult(SINK, &r1), 2, "r1 has two supports");
    for (n, id) in [("r0", &r0), ("r1", &r1), ("r2", &r2)] {
        assert!(sink_present(store.as_ref(), id), "{n} present pre-retract");
    }

    // Retract the root support of r0.
    cascade_retract(store.as_ref(), &["src0".to_string()]);
    assert!(
        !sink_present(store.as_ref(), &r0),
        "r0 deleted: its only support gone"
    );
    assert_eq!(
        led.sink_mult(SINK, &r1),
        1,
        "r1 decremented (r0 gone) but its second supporter remains"
    );
    assert!(
        sink_present(store.as_ref(), &r1),
        "r1 SURVIVES: second support still derives it"
    );
    assert!(
        sink_present(store.as_ref(), &r2),
        "r2 SURVIVES: cascade stopped at the still-supported r1"
    );

    // Remove r1's other support → r1 collapses, descent reaches r2.
    cascade_retract(store.as_ref(), &["alt".to_string()]);
    assert_eq!(led.sink_mult(SINK, &r1), 0, "r1 last support gone");
    assert!(
        !sink_present(store.as_ref(), &r1),
        "r1 deleted once unsupported"
    );
    assert_eq!(led.sink_mult(SINK, &r2), 0, "r2 collapses transitively");
    assert!(
        !sink_present(store.as_ref(), &r2),
        "r2 deleted: its support (r1) is gone"
    );
}

// ── Test 3: reconcile's Retract path goes through cascade_retract ──
//
// Ph4-style generic non-rule pipe: `MatchOp` reads a file, emits one
// child per `fn NAME` match. A `FactWrite`-style terminal records
// each child into a sink table AND a SUPPORT triple (the real entry
// point, `mounted_query::record_fact_support_batch`). A value edit
// makes `reconcile` emit exactly one `Retract`; that single Retract
// runs `cascade_retract`, driving the changed row's mult to 0 so it
// leaves the sink table, while a co-derived unrelated row is
// untouched.

struct MatchOp {
    dispatches: Arc<AtomicUsize>,
}

impl MatchOp {
    fn new() -> (Arc<Self>, Arc<AtomicUsize>) {
        let d = Arc::new(AtomicUsize::new(0));
        (Arc::new(Self { dispatches: d.clone() }), d)
    }
    fn owner_hex(&self, row: &QueueRow<Cursor>) -> String {
        let mut h = blake3::Hasher::new();
        h.update(&row.pipe_hash.to_le_bytes());
        h.update(&row.instance_id.to_le_bytes());
        h.update(&row.depth.to_le_bytes());
        h.update(self.kind().as_bytes());
        hex32(h.finalize().as_bytes())
    }
}

fn hex32(b: &[u8]) -> String {
    let mut s = String::with_capacity(64);
    for x in b {
        s.push_str(&format!("{x:02x}"));
    }
    s
}

impl Component for MatchOp {
    type Next = Cursor;
    fn kind(&self) -> &'static str {
        "test.matchop"
    }
    fn dispatch(
        &self,
        ctx: &RenderCtx,
        rows: &[QueueRow<Cursor>],
        queue: &dyn QueueBackend<Cursor>,
    ) {
        self.dispatches.fetch_add(1, Ordering::SeqCst);
        let graph = ctx.runtime::<RuntimeGraph>();
        let re = regex::Regex::new(r"fn (?P<NAME>\w+)").unwrap();
        for row in rows {
            let c = row.value.as_ref();
            let path = match c.get("FS") {
                Some(p) if !p.is_empty() => p.to_string(),
                _ => continue,
            };
            if let Some(g) = &graph {
                let owner_hex = self.owner_hex(row);
                let in_hex = hex32(&c.content_hash());
                let sid = SourceId::for_file(&path);
                let gen = g.clock().current_gen(sid);
                g.record_memo_dep(&owner_hex, &in_hex, sid, gen);
            }
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let mut idx = 0u32;
            for caps in re.captures_iter(&text) {
                let m0 = caps.get(0).unwrap();
                let name = caps.name("NAME").unwrap();
                let mut child = Cursor::default();
                child.set("FS", path.as_str());
                child.set("NAME", name.as_str());
                child.set("LO", (m0.start() as u64).to_string());
                child.set("HI", (m0.end() as u64).to_string());
                child.set("MATCH", m0.as_str());
                effect_runtime::v2::splice_into_at(
                    row,
                    Node::Emit(Arc::new(child)),
                    ctx.depth + 1,
                    ctx.expand_tick,
                    queue,
                    idx,
                );
                idx += 1;
            }
        }
    }
}

/// Terminal: write each child into the sink table and register one
/// SUPPORT triple per child via the real ledger entry point. The
/// support cursor id is the prior render's stamped cursor id — here,
/// the child's content storage id (stable per logical match content).
struct SinkWrite {
    store: Arc<dyn FactStore<Cursor>>,
}

impl Component for SinkWrite {
    type Next = Cursor;
    fn kind(&self) -> &'static str {
        "test.sinkwrite"
    }
    fn dispatch(
        &self,
        _ctx: &RenderCtx,
        rows: &[QueueRow<Cursor>],
        _queue: &dyn QueueBackend<Cursor>,
    ) {
        self.store.declare(SINK, &["NAME", "LO", "HI", "MATCH"]);
        for r in rows {
            let c = r.value.as_ref();
            let mut srow = Cursor::default();
            for col in ["NAME", "LO", "HI", "MATCH"] {
                if let Some(v) = c.get(col) {
                    srow.set(col, v);
                }
            }
            let srow = Arc::new(srow);
            let sink_row_id = self.store.row_id_for(SINK, srow.as_ref());
            self.store.insert_batch(SINK, vec![srow]);
            // The deriving cursor id: content id of the match cursor.
            // A value edit changes this id for the changed match only
            // (NAME stable, span/MATCH differ), so reconcile's single
            // Retract carries the OLD deriving cursor → cascade drives
            // exactly the old sink row's mult to 0.
            let support_cursor_id = hex32(c.content_hash().as_slice());
            v4::mounted_query::record_fact_supports(
                self.store.as_ref(),
                &support_cursor_id,
                SINK,
                &sink_row_id,
            );
        }
    }
}

fn path_cursor(path: &str) -> Cursor {
    let mut c = Cursor::default();
    c.set("FS", path);
    c
}

fn run_pipe(
    op: Arc<MatchOp>,
    store: Arc<dyn FactStore<Cursor>>,
    graph: Arc<RuntimeGraph>,
    seam: Arc<V4MemoSeam>,
    path: &str,
) {
    let sink: Arc<dyn Component<Next = Cursor>> = Arc::new(SinkWrite { store });
    let op_dyn: Arc<dyn Component<Next = Cursor>> = op;
    let pipe = PipeInstance::new(vec![op_dyn, sink]);
    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    expand(
        &pipe,
        queue,
        vec![Arc::new(path_cursor(path))],
        ExpandOpts::default()
            .with_runtime(graph.clone())
            .with_memo_seam::<Cursor>(seam),
    );
}

#[test]
fn reconcile_retract_path_uses_counted_teardown() {
    let tmp = tempdir().unwrap();
    let src = tmp.path().join("a.rs");
    std::fs::write(&src, "fn alpha() {}\nfn bravo() {}\nfn gamma() {}\n").unwrap();
    let db = tmp.path().join("rt.db");
    let store = open_store(&db);
    let sprf = SprfStore::new(store.clone());
    let graph = Arc::new(RuntimeGraph::new(sprf, store.clone()));
    let seam = V4MemoSeam::new(graph.clone()); // whole-cursor identity
    let path = src.to_str().unwrap().to_string();

    // Run 1: cold. Three matches → three sink rows + three SUPPORT
    // triples, each mult == 1.
    let (op, _d) = MatchOp::new();
    run_pipe(op, store.clone(), graph.clone(), seam.clone(), &path);
    assert_eq!(seam.assert_count(), 3, "cold: 3 Assert");
    assert_eq!(seam.retract_count(), 0, "cold: 0 Retract");

    let led = SupportLedger::new(store.as_ref());
    // Capture sink row ids by NAME so the assertions are stable.
    let id_of = |name: &str| -> Option<String> {
        store
            .rows_of(SINK)
            .iter()
            .find(|r| r.get("NAME") == Some(name))
            .map(|r| store.row_id_for(SINK, r.as_ref()))
    };
    let bravo_old = id_of("bravo").expect("bravo sink row present after cold run");
    let gamma_id = id_of("gamma").expect("gamma sink row present after cold run");
    assert_eq!(led.sink_mult(SINK, &bravo_old), 1, "bravo cold mult == 1");
    assert_eq!(led.sink_mult(SINK, &gamma_id), 1, "gamma cold mult == 1");

    // Edit: rename `bravo` → `bobby` (same byte length → gamma's span
    // unchanged ⇒ gamma's match cursor identical ⇒ co-derived row
    // untouched). Poke the file's source generation.
    std::fs::write(&src, "fn alpha() {}\nfn bobby() {}\nfn gamma() {}\n").unwrap();
    graph.clock().bump(SourceId::for_file(&path));

    // Run 2: Stale → re-render → reconcile. alpha/gamma byte-identical
    // → noop. bravo→bobby changed → exactly 1 Retract + 1 Assert; the
    // single Retract goes through cascade_retract.
    let (op2, _d2) = MatchOp::new();
    run_pipe(op2, store.clone(), graph.clone(), seam.clone(), &path);
    assert_eq!(
        seam.retract_count(),
        1,
        "value edit: exactly 1 Retract (through cascade_retract)"
    );

    // The OLD bravo sink row's mult reached 0 and it left the sink
    // table (counted teardown, not presence rescan).
    assert_eq!(
        led.sink_mult(SINK, &bravo_old),
        0,
        "retracted row's support mult driven to 0 by cascade_retract"
    );
    assert!(
        !sink_present(store.as_ref(), &bravo_old),
        "retracted row left the sink table"
    );

    // The unrelated co-derived `gamma` row is untouched.
    assert_eq!(
        led.sink_mult(SINK, &gamma_id),
        1,
        "co-derived gamma row untouched by the cascade"
    );
    assert!(
        sink_present(store.as_ref(), &gamma_id),
        "co-derived gamma row still present"
    );
}
