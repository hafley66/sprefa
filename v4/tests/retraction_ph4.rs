//! Phase 4 binding tests: the generic MemoSeam + reconcile path,
//! exercised on a NON-rule pipe (no `rule`/`fact` wrapper).
//!
//! The pipe is `MatchOp` (a regex-over-file op) followed by a
//! collector. `MatchOp` is keyed by its input cursor identity (the
//! file path) and records the file as a `SourceClock` dependency, so
//! a poke to the file's generation makes the memo probe return
//! `Stale` (not `Miss`) — that is the plan's `(owner, source-file)`
//! keying. The v3 driver owns the probe/replay/reconcile; this test
//! only builds the pipe and pokes the clock.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use effect_runtime::v2::{
    expand, Component, ExpandOpts, FactStore, MemQueue, Next, Node, PipeInstance, QueueBackend,
    QueueRow, RenderCtx,
};
use tempfile::tempdir;
use v4::fact::SqliteFactStore;
use v4::memo_seam_impl::V4MemoSeam;
use v4::runtime_graph::RuntimeGraph;
use v4::source_clock::{SourceClock, SourceId};
use v4::store::SprfStore;
use v4::Cursor;

fn sqlite_graph(path: &std::path::Path) -> (Arc<dyn FactStore<Cursor>>, Arc<RuntimeGraph>) {
    let facts: Arc<dyn FactStore<Cursor>> =
        Arc::new(SqliteFactStore::<Cursor>::open_file(path).unwrap());
    let store = SprfStore::new(facts.clone());
    let graph = Arc::new(RuntimeGraph::new(store, facts.clone()));
    (facts, graph)
}

/// `MatchOp`: read `path` (the input cursor's `FS` term) and emit one
/// child per `fn (?P<NAME>\w+)` match with `NAME`/`LO`/`HI`/`MATCH`.
/// Records the file as a recorded dependency under the SAME
/// owner/in_key the v3 driver's seam computes, so the memo probe is a
/// gen-compare (Stale on poke) rather than a miss. Counts its own
/// dispatch calls so test 2 can assert "op runs 0x on replay".
struct MatchOp {
    dispatches: Arc<AtomicUsize>,
}

impl MatchOp {
    fn new() -> (Arc<Self>, Arc<AtomicUsize>) {
        let d = Arc::new(AtomicUsize::new(0));
        (
            Arc::new(Self {
                dispatches: d.clone(),
            }),
            d,
        )
    }

    /// The v3 driver's owner digest:
    /// `blake3(pipe_hash ++ instance_id ++ depth ++ kind)`.
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
            // Record the file dep under the driver's owner/in_key so
            // the memo probe is a gen-compare (Phase 2 shape).
            if let Some(g) = &graph {
                let owner_hex = self.owner_hex(row);
                let in_hex = hex32(&c.content_hash());
                let sid = SourceId::for_file(&path);
                let gen = g.clock().current_gen(sid);
                let content_hex = std::fs::read(&path)
                    .ok()
                    .map(|b| blake3::hash(&b).to_hex().to_string())
                    .unwrap_or_default();
                g.record_memo_dep(&owner_hex, &in_hex, sid, gen, &content_hex, &path);
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

/// Terminal collector: records every row it sees so the test can read
/// the surviving set after a sweep.
struct Collect {
    seen: Arc<std::sync::Mutex<Vec<Cursor>>>,
}

impl Component for Collect {
    type Next = Cursor;
    fn kind(&self) -> &'static str {
        "test.collect"
    }
    fn dispatch(
        &self,
        _ctx: &RenderCtx,
        rows: &[QueueRow<Cursor>],
        _queue: &dyn QueueBackend<Cursor>,
    ) {
        let mut g = self.seen.lock().unwrap();
        for r in rows {
            g.push(r.value.as_ref().clone());
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
    seen: Arc<std::sync::Mutex<Vec<Cursor>>>,
    graph: Arc<RuntimeGraph>,
    seam: Arc<V4MemoSeam>,
    path: &str,
) {
    let collect: Arc<dyn Component<Next = Cursor>> = Arc::new(Collect { seen });
    let op_dyn: Arc<dyn Component<Next = Cursor>> = op;
    let pipe = PipeInstance::new(vec![op_dyn, collect]);
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
fn generic_non_rule_pipe_value_edit_one_retract_one_assert() {
    // Decision-1 coverage OFF the rule path: a plain `MatchOp >
    // collect` pipe. Edit one file changing exactly one match (a
    // length-neutral rename so later spans do not shift) → exactly
    // 1 Retract + 1 Assert from the seam.
    let tmp = tempdir().unwrap();
    let src = tmp.path().join("a.rs");
    std::fs::write(&src, "fn alpha() {}\nfn bravo() {}\nfn gamma() {}\n").unwrap();
    let db = tmp.path().join("rt.db");
    let (_facts, graph) = sqlite_graph(&db);
    let seam = V4MemoSeam::new(graph.clone()); // whole-cursor identity
    let path = src.to_str().unwrap().to_string();

    // Run 1: cold (Miss → run → reconcile(None, fresh) → all Assert).
    let (op, _d) = MatchOp::new();
    let seen1 = Arc::new(std::sync::Mutex::new(Vec::new()));
    run_pipe(op, seen1.clone(), graph.clone(), seam.clone(), &path);
    assert_eq!(seen1.lock().unwrap().len(), 3, "cold run emits 3 matches");
    assert_eq!(seam.assert_count(), 3, "cold: 3 Assert");
    assert_eq!(seam.retract_count(), 0, "cold: 0 Retract");

    // Edit: rename `bravo` → `bobby` (same byte length → `gamma`'s
    // span is unchanged, so only one logical row differs). Poke the
    // file's source generation (the event-layer bump).
    std::fs::write(&src, "fn alpha() {}\nfn bobby() {}\nfn gamma() {}\n").unwrap();
    graph.clock().bump(SourceId::for_file(&path));

    // Run 2: probe → memo entry + recorded dep, gen moved → Stale →
    // run → reconcile(prior, fresh). `alpha`/`gamma` rows are
    // byte-identical (same whole-cursor identity) → noop; the
    // `bravo`→`bobby` row changed → Retract(old)+Assert(new).
    let (op2, _d2) = MatchOp::new();
    let seen2 = Arc::new(std::sync::Mutex::new(Vec::new()));
    run_pipe(op2, seen2.clone(), graph.clone(), seam.clone(), &path);
    assert_eq!(
        seam.retract_count(),
        1,
        "value edit: exactly 1 Retract (the changed match)"
    );
    assert_eq!(
        seam.assert_count(),
        4,
        "3 cold + 1 for the re-asserted changed match"
    );
}

#[test]
fn generic_pipe_unchanged_source_zero_dispatch_replay() {
    // "replay works without a rule": same pipe, source unchanged on
    // re-run → the owning component's dispatch count stays 0 (the
    // seam returns Replay, the driver skips `comp.dispatch`).
    let tmp = tempdir().unwrap();
    let src = tmp.path().join("b.rs");
    std::fs::write(&src, "fn one() {}\nfn two() {}\n").unwrap();
    let db = tmp.path().join("rt.db");
    let (_facts, graph) = sqlite_graph(&db);
    let seam = V4MemoSeam::new(graph.clone());
    let path = src.to_str().unwrap().to_string();

    // Run 1: cold. The op dispatches once (Miss).
    let (op, d) = MatchOp::new();
    let seen1 = Arc::new(std::sync::Mutex::new(Vec::new()));
    run_pipe(op, seen1, graph.clone(), seam.clone(), &path);
    assert_eq!(d.load(Ordering::SeqCst), 1, "cold run dispatches once");

    // Run 2: source UNCHANGED, clock NOT bumped. Probe → deps equal →
    // Replay → driver skips dispatch entirely.
    let (op2, d2) = MatchOp::new();
    let seen2 = Arc::new(std::sync::Mutex::new(Vec::new()));
    run_pipe(op2, seen2.clone(), graph.clone(), seam.clone(), &path);
    assert_eq!(
        d2.load(Ordering::SeqCst),
        0,
        "unchanged source: replay, owning component dispatch == 0"
    );
    assert_eq!(
        seen2.lock().unwrap().len(),
        2,
        "replayed rows still flow downstream"
    );
}

#[test]
fn key_terms_value_only_change_keyed_by_stable_capture_name() {
    // Decision 2: seam configured with `re`'s value terms
    // (`["LO","HI","MATCH"]`). The captured NAME is identity (the
    // val_hash complement); LO/HI/MATCH are payload. An edit that
    // moves a match's SPAN but keeps its captured NAME stable is a
    // value-only change → Retract+Assert keyed by the stable NAME,
    // proving `re`-style keying is capture-name, not whole-cursor.
    let tmp = tempdir().unwrap();
    let src = tmp.path().join("c.rs");
    // `keep` is the only match. A leading comment line is added on the
    // edit: NAME (`keep`) is unchanged but LO/HI shift.
    std::fs::write(&src, "fn keep() {}\n").unwrap();
    let db = tmp.path().join("rt.db");
    let (_facts, graph) = sqlite_graph(&db);
    let seam = V4MemoSeam::with_value_terms(
        graph.clone(),
        vec!["LO".into(), "HI".into(), "MATCH".into()],
    );
    let path = src.to_str().unwrap().to_string();

    let (op, _d) = MatchOp::new();
    let seen1 = Arc::new(std::sync::Mutex::new(Vec::new()));
    run_pipe(op, seen1, graph.clone(), seam.clone(), &path);
    assert_eq!(seam.assert_count(), 1, "cold: 1 Assert (the `keep` match)");
    assert_eq!(seam.retract_count(), 0);

    // Edit: prepend a comment line. `keep`'s span moves (LO/HI
    // change) but the captured NAME is still `keep`. Poke the clock.
    std::fs::write(&src, "// shifted\nfn keep() {}\n").unwrap();
    graph.clock().bump(SourceId::for_file(&path));

    let (op2, _d2) = MatchOp::new();
    let seen2 = Arc::new(std::sync::Mutex::new(Vec::new()));
    run_pipe(op2, seen2, graph.clone(), seam.clone(), &path);
    // Same identity (NAME=keep) but payload (LO/HI/MATCH) moved → the
    // reconcile MATCHED the prior row by NAME and took the "value
    // moved" branch: exactly 1 Retract + 1 Assert keyed by the stable
    // NAME. `value_moved == 1` is the proof it keyed by capture name.
    assert_eq!(
        seam.value_moved_count(),
        1,
        "matched prior row by stable NAME (value-moved branch)"
    );
    assert_eq!(seam.retract_count(), 1, "span move, stable NAME: 1 Retract");
    assert_eq!(seam.assert_count(), 2, "1 cold + 1 re-assert");

    // CONTROL: whole-cursor keying over a fresh graph on the SAME
    // span-shift edit. The whole-cursor identity (which includes
    // LO/HI/MATCH) itself moves, so reconcile CANNOT match the prior
    // row — it takes orphan-Retract + brand-new-Assert, never the
    // value-moved branch. This is what `re` keying avoids.
    let db2 = tmp.path().join("rt2.db");
    let (_f2, graph2) = sqlite_graph(&db2);
    let whole = V4MemoSeam::new(graph2.clone()); // whole-cursor identity
    std::fs::write(&src, "fn keep() {}\n").unwrap();
    let (opc, _dc) = MatchOp::new();
    let sc = Arc::new(std::sync::Mutex::new(Vec::new()));
    run_pipe(opc, sc, graph2.clone(), whole.clone(), &path);
    std::fs::write(&src, "// shifted\nfn keep() {}\n").unwrap();
    graph2.clock().bump(SourceId::for_file(&path));
    let (opc2, _dc2) = MatchOp::new();
    let sc2 = Arc::new(std::sync::Mutex::new(Vec::new()));
    run_pipe(opc2, sc2, graph2.clone(), whole.clone(), &path);
    assert_eq!(
        whole.value_moved_count(),
        0,
        "whole-cursor keying cannot match a span-shifted row by identity"
    );
}
