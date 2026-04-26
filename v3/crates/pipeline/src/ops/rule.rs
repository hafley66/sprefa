//! `rule` — user-defined relation with a body pipeline.
//!
//! Surface (chat_log/20260426.7 + 20260426.8 design):
//!   * `rule(:foo, ${A?}, ${B?}) { body }` — top-level definition. Lower
//!     binds the body pipeline + param list onto [`RelationStore`].
//!   * `foo($X, $Y)` — pipe call. This op (`RuleCallOp`) is what the
//!     two-pass lower emits when a bare-name pipe step matches a known
//!     rule name.
//!
//! Runtime shape:
//!   * Per cursor in the input batch, materialize call args under
//!     `cursor.captures`, seed a sub-cursor with `Synthesized` captures
//!     bound to the body's param names, run the body sub-pipeline, drain
//!     terminal cursors, push each one's full capture set as a `RuleRow`
//!     into [`RelationStore::push_rule_row`].
//!   * The op is a side-effecting sink at the call site: input cursors
//!     pass through unchanged so downstream ops see the caller's stream.
//!
//! Why the body runs inside `pipe` rather than as a separate effect:
//! the body is just another `Pipeline` and the runner is reentrant.
//! Wrapping in an effect would mean re-routing through the dispatcher
//! and losing access to the local `RtCtx` borrow; the direct call
//! inherits the same ctx and stream surface.

use std::sync::Arc;

use futures::stream::{self, BoxStream, StreamExt};

use effect_runtime::{BoxFuture, RtCtx};

use crate::_0_cursor::{Capture, Cursor};
use crate::_1_op::Op;
use crate::relation_store::{RelationStore, RuleRow};

use super::relation::{materialize_row, RelationArg};

/// Pipe call to a user-defined rule. `name` resolves at runtime against
/// the [`RelationStore`] body / params maps populated by the lower's
/// pass-1 walk.
#[derive(Debug)]
pub struct RuleCallOp {
    pub name: Arc<str>,
    pub args: Vec<RelationArg>,
}

impl RuleCallOp {
    pub fn new(name: Arc<str>, args: Vec<RelationArg>) -> Self {
        Self { name, args }
    }
}

impl Op for RuleCallOp {
    fn name(&self) -> &'static str { "rule_call" }

    fn pipe<'a>(&'a self, ctx: &'a RtCtx, batch: Arc<[Cursor]>) -> BoxFuture<'a, Arc<[Cursor]>> {
        Box::pin(async move {
            if batch.is_empty() {
                return batch;
            }
            let store = ctx
                .store::<RelationStore>()
                .expect("RelationStore not registered on RtCtx");
            let Some(body) = store.body(&self.name) else {
                eprintln!("rule_call: unknown rule {}", self.name);
                return batch;
            };
            let params = store.params(&self.name);

            for c in batch.iter() {
                let row = materialize_row(&self.args, c);
                let seed = bind_params(c.clone(), &params, &row);
                let upstream: BoxStream<'_, Arc<[Cursor]>> = Box::pin(stream::iter(vec![
                    Arc::<[Cursor]>::from(vec![seed]),
                ]));
                let mut term = body.run(ctx, upstream);
                while let Some(out_batch) = term.next().await {
                    for term_c in out_batch.iter() {
                        store.push_rule_row(&self.name, capture_row(term_c));
                    }
                }
            }
            batch
        })
    }
}

/// Synthesize one capture per param, value taken from the call-site
/// materialization. Extra params (more than args) bind empty bytes; this
/// matches `materialize_row`'s missing-capture convention.
fn bind_params(mut c: Cursor, params: &[Arc<str>], row: &[Arc<str>]) -> Cursor {
    for (i, name) in params.iter().enumerate() {
        let value = row.get(i).cloned().unwrap_or_else(|| Arc::from(""));
        c.captures.push(Capture::synthesized(name.clone(), value));
    }
    c
}

/// Read every capture off `c` as a `RuleRow`, plus the cursor's own
/// `repo` / `rev` / `fs` fields as synthetic columns when present.
/// Synthetic columns lead so they sit at stable positions across rows
/// regardless of how many user-bound captures are in flight; user
/// captures follow in their cursor-emit order.
///
/// Naming: synthetic columns are `repo` / `rev` / `fs`. A user capture
/// named `repo` would collide and write twice — keyed-map dedup at
/// the SQLite drain picks the LAST occurrence (HashMap semantics), so
/// user-bound captures override the cursor field.
fn capture_row(c: &Cursor) -> RuleRow {
    let mut row: RuleRow = Vec::with_capacity(c.captures.len() + 3);
    if !c.repo.is_empty() {
        row.push((Arc::from("repo"), c.repo.clone()));
    }
    if !c.rev.is_empty() {
        row.push((Arc::from("rev"), c.rev.clone()));
    }
    if let Some(fs) = &c.fs {
        let s: String = fs.to_string_lossy().into_owned();
        row.push((Arc::from("fs"), Arc::from(s)));
    }
    for cap in &c.captures {
        let bytes = cap.bytes(&c.content);
        let value: Arc<str> = match std::str::from_utf8(bytes) {
            Ok(s) => Arc::from(s),
            Err(_) => Arc::from(String::from_utf8_lossy(bytes).as_ref()),
        };
        row.push((cap.name.clone(), value));
    }
    row
}
