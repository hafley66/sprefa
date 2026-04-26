//! `tag(:name, ...)` — relational bag op (parse.md §14.5).
//!
//! First arg is the bag name as an atom. Remaining args classify the op:
//!   * all bound (`${X}` Read terms or atom/string/int/float literals)
//!     → WRITE: append one row to bag `:name`.
//!   * all unbound (`${X?}` introducer terms) → READ: drain current
//!     rows + perpetually subscribe to future writes.
//!   * mixed → lower error `tag/mixed-mode`.
//!
//! Bag rows are dynamic-arity `Vec<Vec<Arc<str>>>` — writers may use any
//! column count; readers bind by zip with no padding (user discipline).
//!
//! ## Allocation/timing
//!
//! Read-side perpetual subscription uses
//! [`SubjectRegistry::subscribe`][effect_runtime::SubjectRegistry::subscribe]
//! and [`RelationStore::snapshot_or_subscribe`][crate::relation_store::RelationStore::snapshot_or_subscribe]
//! atomically. Writer fires `registry.next(key)` once per parked waiter
//! after pushing the row; the read loop wakes, re-snapshots from
//! `last_idx`, emits a batch per cursor, re-subscribes, repeats until
//! [`Unsubscribed`][effect_runtime::Unsubscribed] (caller drop, runtime
//! cancel, or external `unsubscribe`).

use std::sync::Arc;

use futures::stream::{self, BoxStream, StreamExt};

use effect_runtime::{BoxFuture, CancellationToken, RtCtx, SubjectRegistry};

use crate::_0_cursor::{Capture, Cursor};
use crate::_1_op::{per_cursor, Op, DEFAULT_PIPE_CONCURRENCY};
use crate::relation_store::{RelationStore, RelationWake, Row, SnapshotOrSubscribed, WriteEffect};
use crate::op_ctor::OpCtor;
use crate::pattern_op::PatternDiagnostic;
use crate::value::{TermMode, Value};

#[derive(Debug, Clone)]
pub enum TagArg {
    /// `${X}` — bind value at write time from `cursor.capture(X)`.
    Capture(Arc<str>),
    /// Literal atom / string / number — already a string at lower time.
    Literal(Arc<str>),
}

#[derive(Debug)]
pub enum TagOp {
    Write { name: Arc<str>, args: Vec<TagArg> },
    Read  { name: Arc<str>, holes: Arc<[Arc<str>]> },
}

impl Op for TagOp {
    fn name(&self) -> &'static str { "tag" }

    fn pipe<'a>(&'a self, ctx: &'a RtCtx, batch: Arc<[Cursor]>) -> BoxFuture<'a, Arc<[Cursor]>> {
        match self {
            TagOp::Write { name, args } => {
                Box::pin(per_cursor(batch, move |c| async move {
                    let row = materialize_row(args, &c);
                    let _ = ctx.put(WriteEffect {
                        name: name.clone(),
                        row,
                    }).await;
                    vec![c]
                }))
            }
            // pipe never called for Read — pipe_flat_map takes over.
            TagOp::Read { .. } => Box::pin(async move { batch }),
        }
    }

    fn pipe_flat_map<'a>(
        &'a self,
        ctx: &'a RtCtx,
        upstream: BoxStream<'a, Arc<[Cursor]>>,
    ) -> BoxStream<'a, Arc<[Cursor]>> {
        match self {
            // Default mergeMap behavior over `pipe`.
            TagOp::Write { .. } => Box::pin(
                upstream
                    .map(move |b| self.pipe(ctx, b))
                    .buffer_unordered(DEFAULT_PIPE_CONCURRENCY),
            ),
            // Each upstream cursor opens its own perpetual subscription;
            // merge them concurrently via flat_map_unordered.
            TagOp::Read { name, holes } => {
                let store = ctx.store::<RelationStore>()
                    .expect("RelationStore not registered on RtCtx");
                let registry = ctx.store::<SubjectRegistry<RelationWake>>()
                    .expect("SubjectRegistry<RelationWake> not registered on RtCtx");
                let cancel = ctx.root_cancel();
                Box::pin(
                    upstream
                        .flat_map(|batch| {
                            let cursors: Vec<Cursor> = batch.iter().cloned().collect();
                            stream::iter(cursors)
                        })
                        .flat_map_unordered(None, move |c| {
                            subscribe_stream(
                                store.clone(),
                                registry.clone(),
                                cancel.clone(),
                                name.clone(),
                                holes.clone(),
                                c,
                            )
                        }),
                )
            }
        }
    }
}

/// Build the per-cursor perpetual stream for tag-read.
///
/// Holds the shared `RelationStore` + `SubjectRegistry`; per call: snapshot
/// past `last_idx`, if rows present emit a batch and advance; else
/// subscribe atomically and await the next write or unsubscribe.
fn subscribe_stream(
    store:    Arc<RelationStore>,
    registry: Arc<SubjectRegistry<RelationWake>>,
    cancel:   CancellationToken,
    name:     Arc<str>,
    holes:    Arc<[Arc<str>]>,
    cursor:   Cursor,
) -> BoxStream<'static, Arc<[Cursor]>> {
    struct State {
        store:    Arc<RelationStore>,
        registry: Arc<SubjectRegistry<RelationWake>>,
        cancel:   CancellationToken,
        name:     Arc<str>,
        holes:    Arc<[Arc<str>]>,
        cursor:   Cursor,
        last_idx: usize,
    }
    let state = State { store, registry, cancel, name, holes, cursor, last_idx: 0 };
    Box::pin(stream::unfold(state, |mut s| async move {
        loop {
            if s.cancel.is_cancelled() { return None; }
            let key = s.registry.fresh_key();
            match s.store.snapshot_or_subscribe(&s.name, s.last_idx, key, &s.registry) {
                SnapshotOrSubscribed::Rows(rows) => {
                    // The fresh `key` was never inserted in pending; no
                    // unsubscribe needed.
                    let n = rows.len();
                    let batch = rows_to_batch(&s.cursor, &s.holes, rows);
                    s.last_idx += n;
                    return Some((batch, s));
                }
                SnapshotOrSubscribed::Subscribed(fut) => {
                    let cancel = s.cancel.clone();
                    let resolved = tokio::select! {
                        r = fut => r,
                        _ = cancel.cancelled() => Err(effect_runtime::Unsubscribed),
                    };
                    match resolved {
                        Ok(row_arc) => {
                            // Wake delivered the row directly; emit it
                            // without re-locking the store. Loop will
                            // re-snapshot on the next iteration to drain
                            // any further rows accumulated meanwhile.
                            let row: Row = (*row_arc).clone();
                            let batch = rows_to_batch(&s.cursor, &s.holes, vec![row]);
                            s.last_idx += 1;
                            return Some((batch, s));
                        }
                        Err(_) => return None,
                    }
                }
            }
        }
    }))
}

/// Build one Arc<[Cursor]> with one cursor per row. Each cursor inherits
/// `cursor`'s identity and adds `holes[i]` as a `Synthesized` capture
/// holding `row[i]`. Extra row columns and extra holes are silently
/// dropped (zip semantics).
fn rows_to_batch(
    cursor: &Cursor,
    holes:  &[Arc<str>],
    rows:   Vec<Vec<Arc<str>>>,
) -> Arc<[Cursor]> {
    let mut out: Vec<Cursor> = Vec::with_capacity(rows.len());
    for row in rows {
        let mut c = cursor.clone();
        for (hole, value) in holes.iter().zip(row.into_iter()) {
            c.captures.push(Capture::synthesized(hole.clone(), value));
        }
        out.push(c);
    }
    Arc::from(out)
}

/// Resolve write-side args to a row of strings. Capture refs read
/// `cursor.capture(name)`; missing captures resolve to empty string
/// (user discipline — no diagnostic at runtime).
fn materialize_row(args: &[TagArg], c: &Cursor) -> Vec<Arc<str>> {
    let mut out: Vec<Arc<str>> = Vec::with_capacity(args.len());
    for a in args {
        match a {
            TagArg::Literal(s) => out.push(s.clone()),
            TagArg::Capture(name) => {
                match c.capture(name) {
                    Some(cap) => {
                        let bytes = cap.bytes(&c.content);
                        let s = std::str::from_utf8(bytes).unwrap_or("");
                        out.push(Arc::from(s));
                    }
                    None => out.push(Arc::from("")),
                }
            }
        }
    }
    out
}

impl OpCtor for TagOp {
    const NAME: &'static str = "tag";
    const BODY_GRAMMAR: &'static str = ":name, ${X|X?|literal}, ...";
    const DOC: &'static str = "\
**tag**(_:name_, _arg_, ...)

Append a row to (or read rows from) the named relational bag. First arg
is the bag name as an atom. Remaining args:

- all bound (`${X}` reads + literals) → write one row.
- all unbound (`${X?}`) → read: drain current rows + subscribe to future
  writes. Captures bind positionally by zip with no padding.

Mixed bound/unbound is a lower error.
";

    fn from_values(values: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        let mut iter = values.into_iter();
        let first = iter.next().ok_or_else(|| vec![PatternDiagnostic {
            code:       "tag/missing-name",
            message:    "tag requires a bag name as its first arg: tag(:bag, ...)".into(),
            byte_range: 0..0,
        }])?;
        let name = match first {
            Value::Atom(s) => s,
            _ => {
                return Err(vec![PatternDiagnostic {
                    code:       "tag/non-atom-name",
                    message:    "tag's first arg must be an atom: tag(:bag, ...)".into(),
                    byte_range: 0..0,
                }]);
            }
        };

        let rest: Vec<Value> = iter.collect();
        let mut have_bound   = false;
        let mut have_unbound = false;
        for v in &rest {
            match v {
                Value::Term { mode: TermMode::Read, .. } => have_bound = true,
                Value::Term { mode: TermMode::Unbound, .. } => have_unbound = true,
                Value::Atom(_) | Value::Str(_) | Value::Int(_) | Value::Float(_) => have_bound = true,
                Value::Op(_) => {
                    return Err(vec![PatternDiagnostic {
                        code:       "tag/op-arg-unsupported",
                        message:    "tag does not accept op invocations as args; use literals or `${X}` / `${X?}` terms".into(),
                        byte_range: 0..0,
                    }]);
                }
            }
        }

        if have_bound && have_unbound {
            return Err(vec![PatternDiagnostic {
                code:       "tag/mixed-mode",
                message:    "tag args must be all bound (write) or all unbound (read); mixing is not allowed".into(),
                byte_range: 0..0,
            }]);
        }

        if have_unbound {
            // Read: collect hole names.
            let holes: Vec<Arc<str>> = rest.into_iter().map(|v| match v {
                Value::Term { name, .. } => name,
                _ => unreachable!("classification said all unbound"),
            }).collect();
            return Ok(TagOp::Read { name, holes: Arc::from(holes) });
        }

        // Write (default for empty arg list too — empty row).
        let args: Vec<TagArg> = rest.into_iter().map(|v| match v {
            Value::Term { name, mode: TermMode::Read } => TagArg::Capture(name),
            Value::Atom(s) | Value::Str(s) => TagArg::Literal(s),
            Value::Int(n) => TagArg::Literal(Arc::from(n.to_string())),
            Value::Float(n) => TagArg::Literal(Arc::from(n.to_string())),
            _ => unreachable!("classification rejected mixed/op"),
        }).collect();
        Ok(TagOp::Write { name, args })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::_0_cursor::Capture;
    use effect_runtime::{RtCtxBuilder, SubjectRegistry, Yield, YieldBatcher};
    use crate::relation_store::WriteBatcher;
    use crate::_2_pipeline::Pipeline;
    use futures::StreamExt;

    fn atom(s: &str) -> Value { Value::Atom(Arc::from(s)) }
    fn term_read(name: &str) -> Value {
        Value::Term { name: Arc::from(name), mode: TermMode::Read }
    }
    fn term_unbound(name: &str) -> Value {
        Value::Term { name: Arc::from(name), mode: TermMode::Unbound }
    }
    fn s(x: &str) -> Value { Value::Str(Arc::from(x)) }

    /// Build an RtCtx with both stores + Yield + TagWrite batchers wired
    /// to one shared `RelationStore` + `SubjectRegistry`. Returns
    /// `(ctx, store, registry)` so tests can drive writes by calling
    /// `ctx.put(WriteEffect)` and assert via the store directly.
    fn build_tag_ctx() -> (RtCtx, Arc<RelationStore>, Arc<SubjectRegistry<RelationWake>>) {
        let store = Arc::new(RelationStore::new());
        let registry = Arc::new(SubjectRegistry::<RelationWake>::new());
        let ctx = RtCtxBuilder::new()
            .with_store(store.clone())
            .with_store(registry.clone())
            .register::<Yield<RelationWake>, _>(YieldBatcher::new(registry.clone()))
            .register::<WriteEffect, _>(
                WriteBatcher::new(store.clone(), registry.clone()),
            )
            .build();
        (ctx, store, registry)
    }

    fn collect_first_batch<'a>(
        op:  &'a TagOp,
        ctx: &'a RtCtx,
        seed: Cursor,
    ) -> impl std::future::Future<Output = Arc<[Cursor]>> + 'a {
        async move {
            let upstream: BoxStream<'_, Arc<[Cursor]>> = Box::pin(
                stream::iter(vec![Arc::<[Cursor]>::from(vec![seed])]),
            );
            let mut s = op.pipe_flat_map(ctx, upstream);
            s.next().await.expect("first batch")
        }
    }

    // --- from_values classification ---

    #[test]
    fn from_values_classifies_all_bound_as_write() {
        let op = TagOp::from_values(vec![atom("a"), term_read("X"), s("lit")]).unwrap();
        match op {
            TagOp::Write { name, args } => {
                assert_eq!(&*name, "a");
                assert_eq!(args.len(), 2);
                assert!(matches!(&args[0], TagArg::Capture(n) if &**n == "X"));
                assert!(matches!(&args[1], TagArg::Literal(s) if &**s == "lit"));
            }
            _ => panic!("expected Write"),
        }
    }

    #[test]
    fn from_values_classifies_all_unbound_as_read() {
        let op = TagOp::from_values(vec![atom("a"), term_unbound("X"), term_unbound("Y")]).unwrap();
        match op {
            TagOp::Read { name, holes } => {
                assert_eq!(&*name, "a");
                assert_eq!(holes.len(), 2);
                assert_eq!(&*holes[0], "X");
                assert_eq!(&*holes[1], "Y");
            }
            _ => panic!("expected Read"),
        }
    }

    #[test]
    fn from_values_rejects_mixed_mode() {
        let err = TagOp::from_values(vec![atom("a"), term_read("X"), term_unbound("Y")]).unwrap_err();
        assert_eq!(err[0].code, "tag/mixed-mode");
    }

    #[test]
    fn from_values_rejects_missing_name() {
        let err = TagOp::from_values(vec![]).unwrap_err();
        assert_eq!(err[0].code, "tag/missing-name");
    }

    #[test]
    fn from_values_rejects_non_atom_name() {
        let err = TagOp::from_values(vec![s("a"), term_read("X")]).unwrap_err();
        assert_eq!(err[0].code, "tag/non-atom-name");
    }

    // --- runtime ---

    #[tokio::test]
    async fn write_then_read_drains_two_rows() {
        let (ctx, store, _reg) = build_tag_ctx();
        let name: Arc<str> = Arc::from("a");
        // Write 2 rows directly via the effect.
        ctx.put(WriteEffect { name: name.clone(), row: vec![Arc::from("x")] }).await;
        ctx.put(WriteEffect { name: name.clone(), row: vec![Arc::from("y")] }).await;
        assert_eq!(store.rows_len("a"), 2);

        let read = TagOp::from_values(vec![atom("a"), term_unbound("A")]).unwrap();
        let batch = collect_first_batch(&read, &ctx, Cursor::default()).await;
        assert_eq!(batch.len(), 2);
        assert_eq!(batch[0].capture("A").map(|c| std::str::from_utf8(c.bytes(&batch[0].content)).unwrap().to_string()),
                   Some("x".to_string()));
        assert_eq!(batch[1].capture("A").map(|c| std::str::from_utf8(c.bytes(&batch[1].content)).unwrap().to_string()),
                   Some("y".to_string()));
    }

    #[tokio::test]
    async fn read_dynamic_arity_zips_per_row() {
        let (ctx, _store, _reg) = build_tag_ctx();
        let name: Arc<str> = Arc::from("a");
        ctx.put(WriteEffect { name: name.clone(),
            row: vec![Arc::from("a1"), Arc::from("b1")] }).await;
        ctx.put(WriteEffect { name: name.clone(),
            row: vec![Arc::from("a2"), Arc::from("b2"), Arc::from("c2")] }).await;

        // Read with 2 captures.
        let read = TagOp::from_values(vec![atom("a"), term_unbound("A"), term_unbound("B")]).unwrap();
        let batch = collect_first_batch(&read, &ctx, Cursor::default()).await;
        assert_eq!(batch.len(), 2);
        let bytes = |c: &Cursor, n: &str| -> String {
            c.capture(n).map(|cap| std::str::from_utf8(cap.bytes(&c.content)).unwrap().to_string())
                .unwrap_or_default()
        };
        assert_eq!(bytes(&batch[0], "A"), "a1");
        assert_eq!(bytes(&batch[0], "B"), "b1");
        assert_eq!(bytes(&batch[1], "A"), "a2");
        assert_eq!(bytes(&batch[1], "B"), "b2");
    }

    #[tokio::test]
    async fn read_then_write_subscribes_and_resolves() {
        // The race-relevant case: the read op subscribes on an empty
        // bag, then a write fires. The pre-registered SubjectRegistry
        // entry guarantees the wake lands.
        let (ctx, _store, _reg) = build_tag_ctx();
        let read = TagOp::from_values(vec![atom("a"), term_unbound("A")]).unwrap();

        let upstream: BoxStream<'_, Arc<[Cursor]>> = Box::pin(
            stream::iter(vec![Arc::<[Cursor]>::from(vec![Cursor::default()])]),
        );
        let mut s = read.pipe_flat_map(&ctx, upstream);

        let writer_ctx = ctx.clone();
        let writer = tokio::spawn(async move {
            // Give the subscriber time to register.
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            writer_ctx.put(WriteEffect {
                name: Arc::from("a"),
                row:  vec![Arc::from("late")],
            }).await;
        });

        let batch = s.next().await.expect("first batch");
        assert_eq!(batch.len(), 1);
        let v = std::str::from_utf8(batch[0].capture("A").unwrap().bytes(&batch[0].content)).unwrap();
        assert_eq!(v, "late");
        writer.await.unwrap();
    }

    #[tokio::test]
    async fn unsubscribe_via_runtime_cancel_terminates_stream() {
        let (ctx, _store, _reg) = build_tag_ctx();
        let read = TagOp::from_values(vec![atom("a"), term_unbound("A")]).unwrap();

        let upstream: BoxStream<'_, Arc<[Cursor]>> = Box::pin(
            stream::iter(vec![Arc::<[Cursor]>::from(vec![Cursor::default()])]),
        );
        // Bag is empty + no writes will happen — the read op parks.
        let s = read.pipe_flat_map(&ctx, upstream);

        let cancel_ctx = ctx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            cancel_ctx.cancel_all();
        });

        // Stream completes (yields nothing, terminates).
        let collected: Vec<Arc<[Cursor]>> = s.collect().await;
        assert!(collected.is_empty(), "expected empty stream after cancel, got {} batches", collected.len());
    }

    #[tokio::test]
    async fn write_then_read_via_seq_pipeline() {
        // End-to-end through Pipeline::Seq: cursor flows through two
        // tag-writes then a tag-read. The Read leg emits rows for the
        // (then-present) snapshot, validating that the runner's
        // BoxStream surface composes write + read correctly.
        let (ctx, _store, _reg) = build_tag_ctx();
        let pipe = Pipeline::Seq(vec![
            Pipeline::Op(Arc::new(
                TagOp::from_values(vec![atom("a"), s("x")]).unwrap(),
            )),
            Pipeline::Op(Arc::new(
                TagOp::from_values(vec![atom("a"), s("y")]).unwrap(),
            )),
            Pipeline::Op(Arc::new(
                TagOp::from_values(vec![atom("a"), term_unbound("A")]).unwrap(),
            )),
        ]);
        let seed: BoxStream<'_, Arc<[Cursor]>> = Box::pin(
            stream::iter(vec![Arc::<[Cursor]>::from(vec![Cursor::default()])]),
        );
        let mut out = pipe.run(&ctx, seed);
        let batch = out.next().await.expect("first batch from read");
        assert_eq!(batch.len(), 2);
        let s0 = std::str::from_utf8(batch[0].capture("A").unwrap().bytes(&batch[0].content)).unwrap().to_string();
        let s1 = std::str::from_utf8(batch[1].capture("A").unwrap().bytes(&batch[1].content)).unwrap().to_string();
        assert_eq!(s0, "x");
        assert_eq!(s1, "y");
    }

    #[tokio::test]
    async fn write_op_pushes_row_with_capture_value() {
        // Writer materializes captures from the cursor at write time.
        let (ctx, store, _reg) = build_tag_ctx();
        let mut c = Cursor::new(Arc::from(b"".as_slice()));
        c.captures.push(Capture::synthesized(Arc::from("X"), Arc::from("from-cap")));

        let write = TagOp::from_values(vec![atom("a"), term_read("X"), s("trail")]).unwrap();
        let _ = write.pipe(&ctx, Arc::from(vec![c])).await;

        assert_eq!(store.rows_len("a"), 1);
        // Inspect via a read.
        let read = TagOp::from_values(vec![atom("a"), term_unbound("A"), term_unbound("B")]).unwrap();
        let batch = collect_first_batch(&read, &ctx, Cursor::default()).await;
        assert_eq!(batch.len(), 1);
        let a = std::str::from_utf8(batch[0].capture("A").unwrap().bytes(&batch[0].content)).unwrap();
        let b = std::str::from_utf8(batch[0].capture("B").unwrap().bytes(&batch[0].content)).unwrap();
        assert_eq!(a, "from-cap");
        assert_eq!(b, "trail");
    }
}
