//! `rev(...)` — filter or bind on `cursor.rev` (parse.md §14.5c).
//!
//! Mechanically identical to [`super::repo::RepoOp`] but with one
//! additional v2-inherited guard: each distinct rev potentially
//! materializes its own worktree under the real Reader, so unbounded
//! matches (`*`, `**`, `**/*`, `*/*`, `$V`) are rejected at
//! construction time. Bounded globs (`v1.*`, `main`, `*-stable`) pass.

use std::sync::Arc;

use effect_runtime::{BoxFuture, RtCtx};
use futures::stream::{self, StreamExt};

use crate::_0_cursor::{Capture, Cursor};
use crate::_1_op::Op;
use crate::op_ctor::OpCtor;
use crate::pattern_op::PatternDiagnostic;
use crate::value::{ArgSpec, Value};

const REV_ARG_SPEC: &[ArgSpec] = &[ArgSpec::Any];

#[derive(Debug)]
pub struct RevOp {
    pub mode: RevMode,
}

#[derive(Debug)]
pub enum RevMode {
    Filter(Arc<dyn Op>),
    Bind(Arc<str>),
    /// sprefa-4iv: arg-pipe producer. Per input cursor, run the sub-op
    /// and for each output cursor read its `last_bound` capture as the
    /// new rev coordinate.
    ArgPipe(Arc<dyn Op>),
}

const UNBOUNDED_WILDCARDS: &[&str] = &["*", "**", "**/*", "*/*"];

impl RevOp {
    pub fn from_source(src: &str) -> Result<Self, Vec<PatternDiagnostic>> {
        let arg = src.trim();
        if arg.is_empty() {
            return Err(vec![PatternDiagnostic {
                code: "rev/missing-arg",
                message: "rev requires a glob pattern or capture (e.g. rev(main) or rev(v1.*))".into(),
                byte_range: 0..src.len(),
            }]);
        }
        if let Some(name) = super::repo::parse_capture(arg) {
            return Err(vec![PatternDiagnostic {
                code: "rev/unbounded-capture",
                message: format!(
                    "rev(${name}) binds every rev unconditionally — \
                     each rev materializes a git worktree on first query. \
                     Bind against a bounded filter upstream or inline, e.g. \
                     rev(v1.*) > rev(${name})"
                ),
                byte_range: 0..src.len(),
            }]);
        }
        if UNBOUNDED_WILDCARDS.iter().any(|w| *w == arg) {
            return Err(vec![PatternDiagnostic {
                code: "rev/unbounded-wildcard",
                message: format!(
                    "rev pattern `{arg}` matches every rev unconditionally. \
                     Narrow with a prefix/suffix: rev(v1.*), rev(*-stable), rev(main)."
                ),
                byte_range: 0..src.len(),
            }]);
        }
        let pat = super::glob::compile_str(arg)?;
        Ok(RevOp { mode: RevMode::Filter(Arc::new(pat)) })
    }

    pub fn from_values(values: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        match values.as_slice() {
            [Value::Term { name, .. }] => Err(vec![PatternDiagnostic {
                code: "rev/unbounded-capture",
                message: format!(
                    "rev(${name}) binds every rev unconditionally — \
                     each rev materializes a git worktree on first query. \
                     Bind against a bounded filter upstream or inline, e.g. \
                     rev(v1.*) > rev(${name})"
                ),
                byte_range: 0..0,
            }]),
            [Value::Atom(s)] | [Value::Str(s)] => {
                if UNBOUNDED_WILDCARDS.iter().any(|w| *w == s.as_ref()) {
                    return Err(vec![PatternDiagnostic {
                        code: "rev/unbounded-wildcard",
                        message: format!(
                            "rev pattern `{s}` matches every rev unconditionally. \
                             Narrow with a prefix/suffix: rev(v1.*), rev(*-stable), rev(main)."
                        ),
                        byte_range: 0..0,
                    }]);
                }
                let pat = super::glob::compile_str(s)?;
                Ok(RevOp { mode: RevMode::Filter(Arc::new(pat)) })
            }
            [Value::Op(op)] if op.try_raw_regex().is_some() => {
                Ok(RevOp { mode: RevMode::Filter(op.clone()) })
            }
            [Value::Op(op)] => {
                // sprefa-4iv: arg-pipe producer. The pipe enumeration
                // bounds the rev set, so unbounded-capture concerns
                // don't apply here.
                Ok(RevOp { mode: RevMode::ArgPipe(op.clone()) })
            }
            [] => Err(vec![PatternDiagnostic {
                code: "rev/missing-arg",
                message: "rev requires a glob pattern or capture (e.g. rev(main) or rev(v1.*))".into(),
                byte_range: 0..0,
            }]),
            _ => Err(vec![PatternDiagnostic {
                code: "value/arity-mismatch",
                message: format!("rev takes 1 argument, got {}", values.len()),
                byte_range: 0..0,
            }]),
        }
    }

    pub fn filter(pat: Arc<dyn Op>) -> Self {
        RevOp { mode: RevMode::Filter(pat) }
    }

    pub fn bind(name: impl Into<Arc<str>>) -> Self {
        RevOp { mode: RevMode::Bind(name.into()) }
    }
}

impl OpCtor for RevOp {
    const NAME: &'static str = "rev";
    const BODY_GRAMMAR: &'static str = "literal rev or `$NAME`";
    const DOC:  &'static str = "\
**rev**(_string_ | `$NAME`)

Filter or bind on `cursor.rev`. Accepts a literal revision; rejects
wildcards like `*`/`**` and bare captures without prior context.

- `rev(HEAD)` / `rev(main)` — filter.
- `rev($V)` — bind.
";

    fn from_values(values: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        Self::from_values(values)
    }
}

impl Op for RevOp {
    fn name(&self) -> &'static str { "rev" }

    fn pipe<'a>(&'a self, ctx: &'a RtCtx, batch: Arc<[Cursor]>) -> BoxFuture<'a, Arc<[Cursor]>> {
        Box::pin(crate::_1_op::per_cursor(batch, move |c| async move {
            match &self.mode {
                RevMode::Filter(op) => {
                    let re = match op.try_raw_regex() {
                        Some(r) => r,
                        None => return vec![],
                    };
                    if re.is_match(c.rev.as_bytes()) { vec![c] } else { vec![] }
                }
                RevMode::Bind(name) => {
                    let mut out = c;
                    let v = out.rev.clone();
                    out.captures.push(Capture::synthesized(name.clone(), v));
                    out.last_bound = Some(name.clone());
                    vec![out]
                }
                RevMode::ArgPipe(op) => {
                    // Snapshot drain — see RepoMode::ArgPipe for rationale.
                    let upstream = stream::iter(vec![Arc::<[Cursor]>::from(vec![c.clone()])]);
                    let mut s = op.pipe_flat_map(ctx, Box::pin(upstream));
                    let mut emitted: Vec<Cursor> = Vec::new();
                    if let Some(batch) = s.next().await {
                        for o in batch.iter() {
                            let value = match o.last_bound.as_ref()
                                .and_then(|n| o.capture(n))
                            {
                                Some(cap) => cap.bytes(&o.content).to_vec(),
                                None => continue,
                            };
                            let new_rev: Arc<str> = match std::str::from_utf8(&value) {
                                Ok(s)  => Arc::from(s),
                                Err(_) => continue,
                            };
                            let mut nc = o.clone();
                            nc.rev = new_rev;
                            emitted.push(nc);
                        }
                    }
                    emitted
                }
            }
        }))
    }

    fn language(&self) -> Option<tree_sitter::Language> {
        crate::op_languages::language_of("glob")
    }

    fn arg_spec(&self) -> &[ArgSpec] { REV_ARG_SPEC }

    fn cache_key(&self, h: &mut blake3::Hasher) -> bool {
        h.update(self.name().as_bytes());
        match &self.mode {
            RevMode::Filter(op) => {
                h.update(b"\x00filter\x00");
                let _ = op.cache_key(h);
            }
            RevMode::Bind(name) => {
                h.update(b"\x00bind\x00");
                h.update(name.as_bytes());
            }
            RevMode::ArgPipe(op) => {
                h.update(b"\x00arg_pipe\x00");
                let _ = op.cache_key(h);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_cursor(rev: &str) -> Cursor {
        let mut c = Cursor::default();
        c.rev = Arc::from(rev);
        c
    }

    #[tokio::test]
    async fn filter_and_bind_scenarios() {
        let ctx = RtCtx::default();

        let op = RevOp::from_source("v1.*").unwrap();
        assert_eq!(op.pipe(&ctx, Arc::from(vec![mk_cursor("v1.2.3")])).await.len(), 1);
        assert_eq!(op.pipe(&ctx, Arc::from(vec![mk_cursor("v2.0")])).await.len(), 0);

        let op = RevOp::from_source("main").unwrap();
        assert_eq!(op.pipe(&ctx, Arc::from(vec![mk_cursor("main")])).await.len(), 1);
        assert_eq!(op.pipe(&ctx, Arc::from(vec![mk_cursor("develop")])).await.len(), 0);

        let op = RevOp::bind("V");
        let out = op.pipe(&ctx, Arc::from(vec![mk_cursor("main")])).await;
        assert_eq!(out.len(), 1);
        assert_eq!(&*out[0].captures[0].name, "V");
        assert_eq!(out[0].captures[0].bytes(&out[0].content), b"main");
    }

    /// sprefa-4iv: smoke that ArgPipe drives a sub-op via pipe_flat_map
    /// and rebinds cursor.rev from each output's last_bound capture.
    #[tokio::test]
    async fn argpipe_rebinds_rev_from_subop_last_bound() {
        use crate::_0_cursor::Capture;
        use effect_runtime::{BoxFuture, RtCtx};
        use futures::stream::{BoxStream};

        // A trivial sub-op: emits two cursors, each with a synthesized
        // "REV" capture and last_bound="REV" — same shape that tag's
        // QueryOp produces from a 2-row bag.
        #[derive(Debug)]
        struct FakeRevSource;
        impl Op for FakeRevSource {
            fn name(&self) -> &'static str { "fake_rev_source" }
            fn pipe<'a>(&'a self, _ctx: &'a RtCtx, _batch: Arc<[Cursor]>) -> BoxFuture<'a, Arc<[Cursor]>> {
                let mut a = Cursor::default();
                a.captures.push(Capture::synthesized(Arc::from("REV"), Arc::from("HEAD")));
                a.last_bound = Some(Arc::from("REV"));
                let mut b = Cursor::default();
                b.captures.push(Capture::synthesized(Arc::from("REV"), Arc::from("v1.2.3")));
                b.last_bound = Some(Arc::from("REV"));
                Box::pin(async move { Arc::from(vec![a, b]) })
            }
            fn pipe_flat_map<'a>(
                &'a self, ctx: &'a RtCtx, _upstream: BoxStream<'a, Arc<[Cursor]>>,
            ) -> BoxStream<'a, Arc<[Cursor]>> {
                use futures::stream::{self, StreamExt};
                let fut = self.pipe(ctx, Arc::from(vec![]));
                Box::pin(stream::once(fut))
            }
        }

        let ctx = RtCtx::default();
        let op = RevOp { mode: RevMode::ArgPipe(Arc::new(FakeRevSource)) };
        let out = op.pipe(&ctx, Arc::from(vec![mk_cursor("seed")])).await;
        assert_eq!(out.len(), 2, "ArgPipe should fan out one cursor per sub-op output");
        assert_eq!(&*out[0].rev, "HEAD");
        assert_eq!(&*out[1].rev, "v1.2.3");
    }

    #[test]
    fn unbounded_forms_rejected_at_parse() {
        fn err_of(src: &str) -> Vec<PatternDiagnostic> {
            match RevOp::from_source(src) {
                Err(e) => e,
                Ok(_) => panic!("expected rejection for `{src}`"),
            }
        }
        for bare in UNBOUNDED_WILDCARDS {
            let err = err_of(bare);
            assert_eq!(err.len(), 1);
            assert_eq!(err[0].code, "rev/unbounded-wildcard",
                "unexpected code for `{bare}`: {}", err[0].code);
        }
        assert_eq!(err_of("$V")[0].code, "rev/unbounded-capture");
        assert_eq!(err_of("  ")[0].code, "rev/missing-arg");
    }
}
