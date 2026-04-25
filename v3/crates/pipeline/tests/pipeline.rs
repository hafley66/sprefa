//! Integration tests for the pipeline crate.
//!
//! Covers:
//!   1. Seq: N ops threaded linearly; slot carries between ops.
//!   2. Fork: two arms, each produces one cursor per input, path-tagged.
//!   3. Effect call inside an op (ctx.put -> typed response).
//!   4. Rebase clears slots; captures and path survive.
//!   5. Object-safe Op storage in Pipeline::Op(Box<dyn Op>).

use effect_runtime::{Batcher, BoxFuture, CancellationToken, EffectKind, RtCtx, RtCtxBuilder};
use pipeline::{Cursor, Op, PathSeg, Pipeline};
use std::sync::Arc;

// --- a tiny effect (local to the test) ---

#[derive(Debug)]
struct LenOf {
    bytes: Vec<u8>,
}
impl EffectKind for LenOf {
    type Response = usize;
}

#[derive(Debug)]
struct DirectLen;
impl Batcher<LenOf> for DirectLen {
    fn run(&self, req: LenOf, _cancel: CancellationToken) -> BoxFuture<'static, usize> {
        Box::pin(async move { req.bytes.len() })
    }
}

// --- ops ---

/// Writes a typed slot derived from current content length.
#[derive(Debug)]
struct StampLenOp;
#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamped(usize);
impl Op for StampLenOp {
    fn name(&self) -> &'static str { "stamp_len" }
    fn pipe<'a>(&'a self, _ctx: &'a RtCtx, mut c: Cursor) -> BoxFuture<'a, Vec<Cursor>> {
        Box::pin(async move {
            let n = c.active().len();
            c.put_slot(Stamped(n));
            vec![c]
        })
    }
}

/// Reads the upstream slot.
#[derive(Debug)]
struct ReadStampOp;
impl Op for ReadStampOp {
    fn name(&self) -> &'static str { "read_stamp" }
    fn pipe<'a>(&'a self, _ctx: &'a RtCtx, c: Cursor) -> BoxFuture<'a, Vec<Cursor>> {
        Box::pin(async move {
            let s = c.get_slot::<Stamped>().expect("upstream StampLenOp must have run");
            assert!(s.0 > 0, "stamp should be non-empty for non-empty content");
            vec![c]
        })
    }
}

/// Calls into the effect layer, stashes the response into a slot.
#[derive(Clone, Debug, PartialEq, Eq)]
struct LenFromEffect(usize);
#[derive(Debug)]
struct LenEffectOp;
impl Op for LenEffectOp {
    fn name(&self) -> &'static str { "len_effect" }
    fn pipe<'a>(&'a self, ctx: &'a RtCtx, mut c: Cursor) -> BoxFuture<'a, Vec<Cursor>> {
        Box::pin(async move {
            let n = ctx.put(LenOf { bytes: c.active().to_vec() }).await;
            c.put_slot(LenFromEffect(n));
            vec![c]
        })
    }
}

/// Two-output op, for Fork fan-in cross-check (outputs duplicate the
/// input with a suffix narrowing).
#[derive(Debug)]
struct HalveOp;
impl Op for HalveOp {
    fn name(&self) -> &'static str { "halve" }
    fn pipe<'a>(&'a self, _ctx: &'a RtCtx, c: Cursor) -> BoxFuture<'a, Vec<Cursor>> {
        Box::pin(async move {
            let n = c.active().len();
            let mid = n / 2;
            let left = c.narrow(c.byte_range.start..c.byte_range.start + mid);
            let right = c.narrow(c.byte_range.start + mid..c.byte_range.end);
            vec![left, right]
        })
    }
}

// --- helpers ---

fn ctx_with_len_effect() -> RtCtx {
    RtCtxBuilder::new()
        .register::<LenOf, _>(DirectLen)
        .build()
}

fn cursor_from(s: &str) -> Cursor {
    Cursor::new(Arc::from(s.as_bytes()))
}

// --- tests ---

#[tokio::test]
async fn seq_threads_slot_between_ops() {
    let ctx = RtCtx::default();
    let pipe = Pipeline::Seq(vec![
        Pipeline::Op(std::sync::Arc::new(StampLenOp)),
        Pipeline::Op(std::sync::Arc::new(ReadStampOp)),
    ]);
    let out = pipe.run(&ctx, vec![cursor_from("hello world")]).await;
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].get_slot::<Stamped>().map(|a| (*a).clone()), Some(Stamped(11)));
}

#[tokio::test]
async fn fork_distributes_and_tags_arms() {
    let ctx = RtCtx::default();
    let pipe = Pipeline::Fork(vec![
        Pipeline::Op(std::sync::Arc::new(StampLenOp)),
        Pipeline::Op(std::sync::Arc::new(StampLenOp)),
    ]);
    let out = pipe.run(&ctx, vec![cursor_from("abcd")]).await;
    assert_eq!(out.len(), 2, "two arms, one input cursor each = 2 outputs");
    for (i, c) in out.iter().enumerate() {
        let leaf = c.path.last().expect("every output has at least one path seg");
        match leaf {
            PathSeg::ForkArm { index } => assert_eq!(*index, i),
            _ => panic!("leaf seg should be ForkArm, got {:?}", leaf),
        }
    }
}

#[tokio::test]
async fn op_calls_effect_and_stashes_response() {
    let ctx = ctx_with_len_effect();
    let pipe = Pipeline::Op(std::sync::Arc::new(LenEffectOp));
    let out = pipe.run(&ctx, vec![cursor_from("abc")]).await;
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].get_slot::<LenFromEffect>().map(|a| (*a).clone()), Some(LenFromEffect(3)));
}

#[tokio::test]
async fn rebase_clears_slots_preserves_captures_and_path() {
    let ctx = RtCtx::default();
    let pipe = Pipeline::Op(std::sync::Arc::new(StampLenOp));
    let out = pipe.run(&ctx, vec![cursor_from("xyz")]).await;
    let c = &out[0];
    assert!(c.get_slot::<Stamped>().is_some(), "slot set by op");

    let new_content: Arc<[u8]> = Arc::from(b"completely different".as_slice());
    let rebased = c.rebase(new_content, 0..20);
    assert!(rebased.get_slot::<Stamped>().is_none(), "rebase clears slots");
    assert_eq!(rebased.path.len(), c.path.len(), "rebase preserves path");
    assert_eq!(rebased.captures.len(), c.captures.len(), "rebase preserves captures");
}

#[tokio::test]
async fn halve_multiplies_cursors_downstream_sees_all() {
    // Seq(Halve, StampLen) — one input → two outputs, each stamped.
    let ctx = RtCtx::default();
    let pipe = Pipeline::Seq(vec![
        Pipeline::Op(std::sync::Arc::new(HalveOp)),
        Pipeline::Op(std::sync::Arc::new(StampLenOp)),
    ]);
    let out = pipe.run(&ctx, vec![cursor_from("abcdef")]).await;
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].get_slot::<Stamped>().map(|a| (*a).clone()), Some(Stamped(3)));
    assert_eq!(out[1].get_slot::<Stamped>().map(|a| (*a).clone()), Some(Stamped(3)));
}

#[tokio::test]
async fn path_tags_leaf_first_op_then_nothing() {
    // Sanity: a single Op adds one PathSeg::Op to the input.
    let ctx = RtCtx::default();
    let pipe = Pipeline::Op(std::sync::Arc::new(StampLenOp));
    let out = pipe.run(&ctx, vec![cursor_from("a")]).await;
    match out[0].path.last().unwrap() {
        PathSeg::Op { name, step } => {
            assert_eq!(*name, "stamp_len");
            assert_eq!(*step, 0);
        }
        seg => panic!("expected Op seg, got {:?}", seg),
    }
}
