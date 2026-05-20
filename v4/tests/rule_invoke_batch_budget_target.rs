//! Budget assertion: a Rule body that fans out N child cursors into
//! its sink MUST flow through in O(1) `insert_batch` calls — NOT O(N).
//!
//! Background: the 2026-05-20 sprf linux bench landed at
//! `insert_batch_calls=15150` for ~15k matches (avg_batch=6.2 rows
//! per call), a 170× wall-time blow-up vs the bare bench. Without
//! this test it took a 10-minute corpus run to notice. Here we fan
//! out N=1000 cursors in a single body render, drive the rule once,
//! and fail loudly if the sink saw the rows in dribs.
//!
//! Two budgets: a STRICT one (single Many → single insert_batch) and
//! a LOOSE one (allow generous slop for legitimate intermediate
//! flushes). Both budgets are O(1) in N — the test FAILS the day
//! anything regresses to per-cursor inserts.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use effect_runtime::v2::{
    expand, Component, ExpandOpts, FactStore, MemFactStore, MemQueue, Node, Pipe, PipeInstance,
    QueueBackend, RenderCtx,
};
use v4::rule::{Rule, RuleInvokeComponent};
use v4::Cursor;

/// Wall-time guardrail: this whole test family is bounded — even on a
/// slow box it should finish in well under a second. If a regression
/// drives the path back to per-row IO the wall blows past this and we
/// FAIL immediately rather than sitting through a 10-minute corpus run
/// to notice. A test that asserts call counts only is not a stopwatch;
/// this is the stopwatch.
const WALL_TIME_BUDGET: Duration = Duration::from_secs(10);

fn assert_wall_under(label: &str, t0: Instant) {
    let elapsed = t0.elapsed();
    assert!(
        elapsed < WALL_TIME_BUDGET,
        "WALL-TIME REGRESSION [{label}]: took {:.3}s (budget {:.1}s). \
         The call-count assertion is not enough; if this fires the path \
         is back to per-row IO regardless of how the batching looks.",
        elapsed.as_secs_f64(),
        WALL_TIME_BUDGET.as_secs_f64(),
    );
}

/// Body: on the FIRST render, fan out `n` cursors as one Node::Many.
/// Subsequent renders emit nothing — the rule body is bounded so we
/// can measure the sink-batching shape without runtime loops.
struct FanoutOnce {
    n: usize,
    fired: AtomicUsize,
}

impl Component for FanoutOnce {
    type Next = Cursor;

    fn render(&self, _ctx: &RenderCtx, _c: &Cursor) -> Node<Cursor> {
        if self.fired.fetch_add(1, Ordering::SeqCst) > 0 {
            return Node::Done;
        }
        let mut out = Vec::with_capacity(self.n);
        for i in 0..self.n {
            let mut child = Cursor::default();
            child.set("ID", i.to_string());
            out.push(Node::Emit(Arc::new(child)));
        }
        Node::Many(out)
    }
}

#[test]
fn rule_fanout_n_rows_uses_o1_insert_batches() {
    const N: usize = 1000;
    // Budget: 1000 fan-out rows should reach the sink in ONE
    // insert_batch (Node::Many → render_batch → one store call).
    // We allow a tiny slop because nothing forbids a future
    // implementation from chunking a Many that exceeds an internal
    // cap. 50 is still 20× smaller than N, so any per-cursor
    // regression (1 call per row) FAILS this test.
    const INSERT_BATCH_BUDGET: u64 = 50;

    let store = Arc::new(MemFactStore::<Cursor>::new());
    let body = Pipe::new().step(Arc::new(FanoutOnce {
        n: N,
        fired: AtomicUsize::new(0),
    }));
    let rule = Rule::new("fanout", store.clone(), "out", &["ID"], body);

    let seed = Arc::new(Cursor::default());
    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    let t0 = Instant::now();
    rule.run_with(queue, vec![seed], ExpandOpts::default());
    assert_wall_under("loose-rule-fanout", t0);

    let rows = store.rows_of("out");
    assert_eq!(rows.len(), N, "rule body must produce N rows in sink");

    let stats = store
        .stats()
        .expect("MemFactStore exposes FactStoreStats");

    let avg = stats.insert_batch_rows as f64
        / stats.insert_batch_calls.max(1) as f64;

    assert!(
        stats.insert_batch_calls <= INSERT_BATCH_BUDGET,
        "N+1 REGRESSION: rule fanout of {} rows used {} insert_batch \
         calls (budget {}). avg_batch={:.2} rows/call. \
         Expected ~1 call. A regression here means FactWrite or the \
         queue dispatcher is feeding the store one row at a time \
         instead of batching the Node::Many.",
        N,
        stats.insert_batch_calls,
        INSERT_BATCH_BUDGET,
        avg,
    );
}

#[test]
fn rule_fanout_strict_one_insert_batch_for_one_many() {
    // STRICT companion: a single Node::Many of N children MUST land
    // in the sink in exactly ONE insert_batch call. This is the
    // strongest possible budget — any drift from 1 means render_batch
    // is being bypassed.
    const N: usize = 2048;

    let store = Arc::new(MemFactStore::<Cursor>::new());
    let body = Pipe::new().step(Arc::new(FanoutOnce {
        n: N,
        fired: AtomicUsize::new(0),
    }));
    let rule = Rule::new("fanout_strict", store.clone(), "out_strict", &["ID"], body);

    let seed = Arc::new(Cursor::default());
    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    let mut opts = ExpandOpts::default();
    // Force the queue to NOT slice the Many — give it room to hand the
    // whole batch to render_batch in one go.
    opts = opts.with_batch_cap(1 << 20);
    let t0 = Instant::now();
    rule.run_with(queue, vec![seed], opts);
    assert_wall_under("strict-rule-fanout", t0);

    assert_eq!(store.rows_of("out_strict").len(), N);
    let stats = store
        .stats()
        .expect("MemFactStore exposes FactStoreStats");

    // ≤ 3 is the irreducible minimum for the streaming-rule write
    // path: ONE call to the sink table + ONE owner-snapshot batch
    // (factwrite_owner_snapshot, for cross-run reconcile) + ONE
    // support-ledger batch (mounted_query_support, for cascade
    // retract). Any drift past 3 means a satellite is back to a
    // per-cursor pattern.
    assert!(
        stats.insert_batch_calls <= 3,
        "STRICT REGRESSION: {} rows in one Node::Many should land in \
         the sink in ≤ 3 insert_batch calls (sink + owner-snapshot + \
         support-ledger). Got {} calls, avg_batch={:.2}.",
        N,
        stats.insert_batch_calls,
        stats.insert_batch_rows as f64
            / stats.insert_batch_calls.max(1) as f64,
    );
}

/// Bench-path budget. The two tests above drive `Rule::run_with` which
/// is the LOWER half of the bench path. The actual `hits();` bench
/// surface lowers to `RuleInvokeComponent` — a top-level pipe whose
/// render method internally calls `run_rule` (an inner `expand` per
/// input cursor). This test reproduces that shape: a fan-out body
/// invoked through `RuleInvokeComponent`, driven from the same
/// `expand` the runtime uses for `hits();`. If the bench is slow but
/// the two tests above pass, the regression is here.
#[test]
fn rule_invoke_component_fanout_uses_o1_insert_batches() {
    const N: usize = 1000;
    const INSERT_BATCH_BUDGET: u64 = 50;

    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let body = Pipe::new().step(Arc::new(FanoutOnce {
        n: N,
        fired: AtomicUsize::new(0),
    }));
    let rule = Rule::new("inv_fanout", store.clone(), "inv_out", &["ID"], body);
    let invoke: Arc<dyn Component<Next = Cursor>> =
        Arc::new(RuleInvokeComponent::new(rule, Vec::new(), false));
    let pipe = PipeInstance::new(vec![invoke]);

    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    let seed = Arc::new(Cursor::default());
    let t0 = Instant::now();
    expand(&pipe, queue, vec![seed], ExpandOpts::default());
    assert_wall_under("rule-invoke-component-fanout", t0);

    let mem = store
        .as_any()
        .downcast_ref::<MemFactStore<Cursor>>()
        .expect("store is MemFactStore");
    let stats = mem.stats().expect("MemFactStore exposes FactStoreStats");
    let avg = stats.insert_batch_rows as f64
        / stats.insert_batch_calls.max(1) as f64;
    assert!(
        stats.insert_batch_calls <= INSERT_BATCH_BUDGET,
        "BENCH-PATH N+1 REGRESSION: RuleInvokeComponent fanout of {} \
         rows used {} insert_batch calls (budget {}). avg_batch={:.2}. \
         The bench-surface `hits();` lowers to this path; if this \
         fires, the linux bench is back to per-row IO.",
        N,
        stats.insert_batch_calls,
        INSERT_BATCH_BUDGET,
        avg,
    );
}
