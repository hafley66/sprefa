//! `fact(:name, ...)` — facade lowering to relation ops (parse.md §14.5).
//!
//! Renamed from `tag` (alias period: `tag` and `tag?` still parse but emit
//! diag `tag/renamed-to-fact`). Internal delegation to relation sub-ops is
//! unchanged; only the user-facing op label changes.
//!
//! Surface (locked dispatch table from chat_log/20260426.7):
//!
//! | Spelling                                  | Lowers to            |
//! |-------------------------------------------|----------------------|
//! | `fact(:r, $X, $Y)` all-bound, raw         | [`WriteOp`]          |
//! | `fact(:r, ${X?}, ${Y?})` all-unbound, raw | [`QueryOp`]          |
//! | `fact?(...)` all-bound                    | [`ProbeOp`] (kcl)    |
//! | `fact?(...)` mixed                        | [`JoinOp`] (kcl)     |
//! | `fact?(...)` all-unbound                  | [`QueryOp`]          |
//!
//! Today: only the `?`-less surface is parsed; `fact?` is reserved for
//! step kcl (probe/join wire-up). Mixed bound/unbound on the raw side is
//! a lower error (`fact/mixed-mode`) — `?` makes it valid by routing to
//! join.

use std::sync::Arc;

use effect_runtime::{BoxFuture, RtCtx};
use futures::stream::BoxStream;

use crate::_0_cursor::Cursor;
use crate::_1_op::Op;
use crate::op_ctor::OpCtor;
use crate::ops::relation::{
    JoinOp, JoinSlot, ProbeMode, ProbeOp, QueryOp, RelationArg, WriteOp,
};
use crate::pattern_op::PatternDiagnostic;
use crate::value::{TermMode, Value};

/// Re-export so prior `use crate::ops::fact::FactArg` callers resolve.
pub type FactArg = RelationArg;

/// Facade enum: lowering chooses the variant; runtime delegates.
#[derive(Debug)]
pub enum FactOp {
    /// `fact(:r, $X, ...)` all-bound, raw — append a row.
    Write(WriteOp),
    /// `fact(:r, ${X?}, ...)` all-unbound, raw — drain + subscribe.
    Query(QueryOp),
    /// `fact?(:r, $X, ...)` all-bound, predicate — strict probe.
    Probe(ProbeOp),
    /// `fact?(:r, $X, ${Y?}, ...)` mixed, predicate — datalog filter+project.
    Join(JoinOp),
}

impl Op for FactOp {
    fn name(&self) -> &'static str { "fact" }

    fn pipe<'a>(&'a self, ctx: &'a RtCtx, batch: Arc<[Cursor]>) -> BoxFuture<'a, Arc<[Cursor]>> {
        match self {
            FactOp::Write(w) => w.pipe(ctx, batch),
            FactOp::Query(q) => q.pipe(ctx, batch),
            FactOp::Probe(p) => p.pipe(ctx, batch),
            FactOp::Join(j)  => j.pipe(ctx, batch),
        }
    }

    fn pipe_flat_map<'a>(
        &'a self,
        ctx: &'a RtCtx,
        upstream: BoxStream<'a, Arc<[Cursor]>>,
    ) -> BoxStream<'a, Arc<[Cursor]>> {
        match self {
            FactOp::Write(w) => w.pipe_flat_map(ctx, upstream),
            FactOp::Query(q) => q.pipe_flat_map(ctx, upstream),
            FactOp::Probe(p) => p.pipe_flat_map(ctx, upstream),
            FactOp::Join(j)  => j.pipe_flat_map(ctx, upstream),
        }
    }
}

impl OpCtor for FactOp {
    const NAME: &'static str = "fact";
    const BODY_GRAMMAR: &'static str = ":name, ${X|X?|literal}, ...";
    const DOC: &'static str = "\
**fact**(_:name_, _arg_, ...)

Append a row to (or read rows from) the named relational bag. First arg
is the bag name as an atom. Remaining args:

- all bound (`${X}` reads + literals) → write one row.
- all unbound (`${X?}`) → read: drain current rows + subscribe to future
  writes. Captures bind positionally by zip with no padding.

Mixed bound/unbound is a lower error.

Previously named `tag`; `tag(...)` still parses during the alias period
but emits diag `tag/renamed-to-fact`.
";

    fn from_values(values: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        let mut iter = values.into_iter();
        let first = iter.next().ok_or_else(|| {
            vec![PatternDiagnostic {
                code:       "fact/missing-name",
                message:    "fact requires a bag name as its first arg: fact(:bag, ...)".into(),
                byte_range: 0..0,
            }]
        })?;
        let name = match first {
            Value::Atom(s) => s,
            _ => {
                return Err(vec![PatternDiagnostic {
                    code:       "fact/non-atom-name",
                    message:    "fact's first arg must be an atom: fact(:bag, ...)".into(),
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
                        code:       "fact/op-arg-unsupported",
                        message:    "fact does not accept op invocations as args; use literals or `${X}` / `${X?}` terms".into(),
                        byte_range: 0..0,
                    }]);
                }
            }
        }

        if have_bound && have_unbound {
            return Err(vec![PatternDiagnostic {
                code:       "fact/mixed-mode",
                message:    "fact args must be all bound (write) or all unbound (read); mixing is not allowed".into(),
                byte_range: 0..0,
            }]);
        }

        if have_unbound {
            let holes: Vec<Arc<str>> = rest
                .into_iter()
                .map(|v| match v {
                    Value::Term { name, .. } => name,
                    _ => unreachable!("classification said all unbound"),
                })
                .collect();
            return Ok(FactOp::Query(QueryOp::new(name, Arc::from(holes))));
        }

        let args: Vec<RelationArg> = rest
            .into_iter()
            .map(|v| match v {
                Value::Term { name, mode: TermMode::Read } => RelationArg::Capture(name),
                Value::Atom(s) | Value::Str(s) => RelationArg::Literal(s),
                Value::Int(n) => RelationArg::Literal(Arc::from(n.to_string())),
                Value::Float(n) => RelationArg::Literal(Arc::from(n.to_string())),
                _ => unreachable!("classification rejected mixed/op"),
            })
            .collect();
        Ok(FactOp::Write(WriteOp::new(name, args)))
    }
}

/// `fact?(...)` — predicate-side facade. Same args, classified by bound /
/// mixed / unbound, dispatches to ProbeOp / JoinOp / QueryOp respectively.
///
/// Registered under [`FactPredicateOp::NAME`] = `"fact?"`. The host lowerer
/// flips the registry name when [`OpInvocation::predicate`] is true
/// (see `server::run_and_print`, `backend.rs::resolve_pipeline_name`).
#[derive(Debug)]
pub struct FactPredicateOp(pub FactOp);

impl Op for FactPredicateOp {
    fn name(&self) -> &'static str { "fact" }

    fn pipe<'a>(&'a self, ctx: &'a RtCtx, batch: Arc<[Cursor]>) -> BoxFuture<'a, Arc<[Cursor]>> {
        self.0.pipe(ctx, batch)
    }

    fn pipe_flat_map<'a>(
        &'a self,
        ctx: &'a RtCtx,
        upstream: BoxStream<'a, Arc<[Cursor]>>,
    ) -> BoxStream<'a, Arc<[Cursor]>> {
        self.0.pipe_flat_map(ctx, upstream)
    }
}

impl OpCtor for FactPredicateOp {
    const NAME: &'static str = "fact?";
    const BODY_GRAMMAR: &'static str = ":name, $X|${X?}|literal, ...";
    const DOC: &'static str = "\
**fact?**(_:name_, _arg_, ...)

Pull-side counterpart of `fact`. Args classify the action:

- all bound (`${X}` reads + literals) → strict probe; cursor passes if a
  matching row exists, else dropped.
- mixed bound + `${X?}` introducer → datalog filter+project; bound
  positions filter, unbound positions project the matched row column
  into a fresh capture. Fans out one cursor per matching row.
- all unbound (`${X?}`) → drain bag + subscribe to future writes
  (same as bare `fact(:r, ${X?})` today).

Previously named `tag?`; `tag?(...)` still parses during the alias period
but emits diag `tag/renamed-to-fact`.
";

    fn from_values(values: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        let mut iter = values.into_iter();
        let first = iter.next().ok_or_else(|| {
            vec![PatternDiagnostic {
                code:       "fact/missing-name",
                message:    "fact? requires a bag name as its first arg: fact?(:bag, ...)".into(),
                byte_range: 0..0,
            }]
        })?;
        let name = match first {
            Value::Atom(s) => s,
            _ => {
                return Err(vec![PatternDiagnostic {
                    code:       "fact/non-atom-name",
                    message:    "fact?'s first arg must be an atom: fact?(:bag, ...)".into(),
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
                        code:       "fact/op-arg-unsupported",
                        message:    "fact? does not accept op invocations as args".into(),
                        byte_range: 0..0,
                    }]);
                }
            }
        }

        // All-unbound: same drain+subscribe as `fact(...)` raw query.
        if have_unbound && !have_bound {
            let holes: Vec<Arc<str>> = rest
                .into_iter()
                .map(|v| match v {
                    Value::Term { name, .. } => name,
                    _ => unreachable!(),
                })
                .collect();
            return Ok(FactPredicateOp(FactOp::Query(QueryOp::new(name, Arc::from(holes)))));
        }

        // All-bound: strict probe.
        if have_bound && !have_unbound {
            let key: Vec<RelationArg> = rest
                .into_iter()
                .map(|v| match v {
                    Value::Term { name, mode: TermMode::Read } => RelationArg::Capture(name),
                    Value::Atom(s) | Value::Str(s) => RelationArg::Literal(s),
                    Value::Int(n) => RelationArg::Literal(Arc::from(n.to_string())),
                    Value::Float(n) => RelationArg::Literal(Arc::from(n.to_string())),
                    _ => unreachable!(),
                })
                .collect();
            return Ok(FactPredicateOp(FactOp::Probe(ProbeOp::new(
                name,
                key,
                ProbeMode::Static,
            ))));
        }

        // Mixed: datalog filter+project.
        let slots: Vec<JoinSlot> = rest
            .into_iter()
            .map(|v| match v {
                Value::Term { name, mode: TermMode::Unbound } => JoinSlot::Project(name),
                Value::Term { name, mode: TermMode::Read } => JoinSlot::Bound(RelationArg::Capture(name)),
                Value::Atom(s) | Value::Str(s) => JoinSlot::Bound(RelationArg::Literal(s)),
                Value::Int(n) => JoinSlot::Bound(RelationArg::Literal(Arc::from(n.to_string()))),
                Value::Float(n) => JoinSlot::Bound(RelationArg::Literal(Arc::from(n.to_string()))),
                _ => unreachable!(),
            })
            .collect();
        // Empty arg list is also `have_unbound == false && have_bound == false`;
        // treat as a no-op probe (no key, always passes).
        Ok(FactPredicateOp(FactOp::Join(JoinOp::new(name, slots))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::_0_cursor::Capture;
    use crate::_2_pipeline::Pipeline;
    use crate::relation_store::{RelationStore, RelationWake, WriteBatcher, WriteEffect};
    use effect_runtime::{RtCtx, RtCtxBuilder, SubjectRegistry, Yield, YieldBatcher};
    use futures::stream::{self, BoxStream, StreamExt};

    fn atom(s: &str) -> Value { Value::Atom(Arc::from(s)) }
    fn term_read(name: &str) -> Value {
        Value::Term { name: Arc::from(name), mode: TermMode::Read }
    }
    fn term_unbound(name: &str) -> Value {
        Value::Term { name: Arc::from(name), mode: TermMode::Unbound }
    }
    fn s(x: &str) -> Value { Value::Str(Arc::from(x)) }

    fn build_fact_ctx() -> (RtCtx, Arc<RelationStore>, Arc<SubjectRegistry<RelationWake>>) {
        let store = Arc::new(RelationStore::new());
        let registry = Arc::new(SubjectRegistry::<RelationWake>::new());
        let ctx = RtCtxBuilder::new()
            .with_store(store.clone())
            .with_store(registry.clone())
            .register::<Yield<RelationWake>, _>(YieldBatcher::new(registry.clone()))
            .register::<WriteEffect, _>(WriteBatcher::new(store.clone(), registry.clone()))
            .build();
        (ctx, store, registry)
    }

    fn collect_first_batch<'a>(
        op:   &'a FactOp,
        ctx:  &'a RtCtx,
        seed: Cursor,
    ) -> impl std::future::Future<Output = Arc<[Cursor]>> + 'a {
        async move {
            let upstream: BoxStream<'_, Arc<[Cursor]>> =
                Box::pin(stream::iter(vec![Arc::<[Cursor]>::from(vec![seed])]));
            let mut s = op.pipe_flat_map(ctx, upstream);
            s.next().await.expect("first batch")
        }
    }

    // --- from_values classification ---

    #[test]
    fn from_values_classifies_all_bound_as_write() {
        let op = FactOp::from_values(vec![atom("a"), term_read("X"), s("lit")]).unwrap();
        match op {
            FactOp::Write(w) => {
                assert_eq!(&*w.name, "a");
                assert_eq!(w.args.len(), 2);
                assert!(matches!(&w.args[0], RelationArg::Capture(n) if &**n == "X"));
                assert!(matches!(&w.args[1], RelationArg::Literal(s) if &**s == "lit"));
            }
            _ => panic!("expected Write"),
        }
    }

    #[test]
    fn from_values_classifies_all_unbound_as_read() {
        let op = FactOp::from_values(vec![atom("a"), term_unbound("X"), term_unbound("Y")]).unwrap();
        match op {
            FactOp::Query(q) => {
                assert_eq!(&*q.name, "a");
                assert_eq!(q.holes.len(), 2);
                assert_eq!(&*q.holes[0], "X");
                assert_eq!(&*q.holes[1], "Y");
            }
            _ => panic!("expected Query"),
        }
    }

    #[test]
    fn from_values_rejects_mixed_mode() {
        let err = FactOp::from_values(vec![atom("a"), term_read("X"), term_unbound("Y")]).unwrap_err();
        assert_eq!(err[0].code, "fact/mixed-mode");
    }

    #[test]
    fn from_values_rejects_missing_name() {
        let err = FactOp::from_values(vec![]).unwrap_err();
        assert_eq!(err[0].code, "fact/missing-name");
    }

    #[test]
    fn from_values_rejects_non_atom_name() {
        let err = FactOp::from_values(vec![s("a"), term_read("X")]).unwrap_err();
        assert_eq!(err[0].code, "fact/non-atom-name");
    }

    // --- runtime ---

    #[tokio::test]
    async fn write_then_read_drains_two_rows() {
        let (ctx, store, _reg) = build_fact_ctx();
        let name: Arc<str> = Arc::from("a");
        ctx.put(WriteEffect::one(name.clone(), vec![Arc::from("x")])).await;
        ctx.put(WriteEffect::one(name.clone(), vec![Arc::from("y")])).await;
        assert_eq!(store.rows_len("a"), 2);

        let read = FactOp::from_values(vec![atom("a"), term_unbound("A")]).unwrap();
        let batch = collect_first_batch(&read, &ctx, Cursor::default()).await;
        assert_eq!(batch.len(), 2);
        assert_eq!(
            batch[0]
                .capture("A")
                .map(|c| std::str::from_utf8(c.bytes(&batch[0].content)).unwrap().to_string()),
            Some("x".to_string())
        );
        assert_eq!(
            batch[1]
                .capture("A")
                .map(|c| std::str::from_utf8(c.bytes(&batch[1].content)).unwrap().to_string()),
            Some("y".to_string())
        );
    }

    #[tokio::test]
    async fn read_dynamic_arity_zips_per_row() {
        let (ctx, _store, _reg) = build_fact_ctx();
        let name: Arc<str> = Arc::from("a");
        ctx.put(WriteEffect::one(name.clone(), vec![Arc::from("a1"), Arc::from("b1")])).await;
        ctx.put(WriteEffect::one(name.clone(), vec![Arc::from("a2"), Arc::from("b2"), Arc::from("c2")])).await;

        let read =
            FactOp::from_values(vec![atom("a"), term_unbound("A"), term_unbound("B")]).unwrap();
        let batch = collect_first_batch(&read, &ctx, Cursor::default()).await;
        assert_eq!(batch.len(), 2);
        let bytes = |c: &Cursor, n: &str| -> String {
            c.capture(n)
                .map(|cap| std::str::from_utf8(cap.bytes(&c.content)).unwrap().to_string())
                .unwrap_or_default()
        };
        assert_eq!(bytes(&batch[0], "A"), "a1");
        assert_eq!(bytes(&batch[0], "B"), "b1");
        assert_eq!(bytes(&batch[1], "A"), "a2");
        assert_eq!(bytes(&batch[1], "B"), "b2");
    }

    #[tokio::test]
    async fn read_then_write_subscribes_and_resolves() {
        let (ctx, _store, _reg) = build_fact_ctx();
        let read = FactOp::from_values(vec![atom("a"), term_unbound("A")]).unwrap();

        let upstream: BoxStream<'_, Arc<[Cursor]>> =
            Box::pin(stream::iter(vec![Arc::<[Cursor]>::from(vec![Cursor::default()])]));
        let mut s = read.pipe_flat_map(&ctx, upstream);

        let writer_ctx = ctx.clone();
        let writer = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            writer_ctx
                .put(WriteEffect::one(Arc::from("a"), vec![Arc::from("late")]))
                .await;
        });

        let batch = s.next().await.expect("first batch");
        assert_eq!(batch.len(), 1);
        let v = std::str::from_utf8(batch[0].capture("A").unwrap().bytes(&batch[0].content))
            .unwrap();
        assert_eq!(v, "late");
        writer.await.unwrap();
    }

    #[tokio::test]
    async fn unsubscribe_via_runtime_cancel_terminates_stream() {
        let (ctx, _store, _reg) = build_fact_ctx();
        let read = FactOp::from_values(vec![atom("a"), term_unbound("A")]).unwrap();

        let upstream: BoxStream<'_, Arc<[Cursor]>> =
            Box::pin(stream::iter(vec![Arc::<[Cursor]>::from(vec![Cursor::default()])]));
        let s = read.pipe_flat_map(&ctx, upstream);

        let cancel_ctx = ctx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            cancel_ctx.cancel_all();
        });

        let collected: Vec<Arc<[Cursor]>> = s.collect().await;
        assert!(
            collected.is_empty(),
            "expected empty stream after cancel, got {} batches",
            collected.len()
        );
    }

    #[tokio::test]
    async fn write_then_read_via_seq_pipeline() {
        let (ctx, _store, _reg) = build_fact_ctx();
        let pipe = Pipeline::Seq(vec![
            Pipeline::Op(Arc::new(FactOp::from_values(vec![atom("a"), s("x")]).unwrap())),
            Pipeline::Op(Arc::new(FactOp::from_values(vec![atom("a"), s("y")]).unwrap())),
            Pipeline::Op(Arc::new(
                FactOp::from_values(vec![atom("a"), term_unbound("A")]).unwrap(),
            )),
        ]);
        let seed: BoxStream<'_, Arc<[Cursor]>> =
            Box::pin(stream::iter(vec![Arc::<[Cursor]>::from(vec![Cursor::default()])]));
        let mut out = pipe.run(&ctx, seed);
        let batch = out.next().await.expect("first batch from read");
        assert_eq!(batch.len(), 2);
        let s0 = std::str::from_utf8(batch[0].capture("A").unwrap().bytes(&batch[0].content))
            .unwrap()
            .to_string();
        let s1 = std::str::from_utf8(batch[1].capture("A").unwrap().bytes(&batch[1].content))
            .unwrap()
            .to_string();
        assert_eq!(s0, "x");
        assert_eq!(s1, "y");
    }

    #[tokio::test]
    async fn write_op_pushes_row_with_capture_value() {
        let (ctx, store, _reg) = build_fact_ctx();
        let mut c = Cursor::new(Arc::from(b"".as_slice()));
        c.captures.push(Capture::synthesized(Arc::from("X"), Arc::from("from-cap")));

        let write = FactOp::from_values(vec![atom("a"), term_read("X"), s("trail")]).unwrap();
        let _ = write.pipe(&ctx, Arc::from(vec![c])).await;

        assert_eq!(store.rows_len("a"), 1);
        let read =
            FactOp::from_values(vec![atom("a"), term_unbound("A"), term_unbound("B")]).unwrap();
        let batch = collect_first_batch(&read, &ctx, Cursor::default()).await;
        assert_eq!(batch.len(), 1);
        let a = std::str::from_utf8(batch[0].capture("A").unwrap().bytes(&batch[0].content))
            .unwrap();
        let b = std::str::from_utf8(batch[0].capture("B").unwrap().bytes(&batch[0].content))
            .unwrap();
        assert_eq!(a, "from-cap");
        assert_eq!(b, "trail");
    }
}
