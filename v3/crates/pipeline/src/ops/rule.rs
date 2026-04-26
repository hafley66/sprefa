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
use crate::pattern_op::PatternDiagnostic;
use crate::relation_store::{RelationStore, RuleRow};
use crate::value::{TermMode, Value};

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

/// `<rule>?(...)` — predicate dispatch over rule_rows.
///
/// Args zip by position with the rule's declared params (looked up at runtime
/// via [`RelationStore::params`]). Classification mirrors `tag?`:
///
///   * all-bound  → [`RuleMode::Probe`]  (filter; cursor passes if any row
///     matches every (param_name, arg_value) pair)
///   * mixed      → [`RuleMode::Join`]   (filter on bound positions, project
///     unbound positions' row column into a fresh capture; fan out one
///     cursor per matching row)
///   * all-unbound → [`RuleMode::Query`] (drain rule_rows snapshot, fan out
///     one cursor per row, bind hole captures to the row's value for each
///     param column)
///
/// Snapshot semantics: rule_rows is read once per batch. v0 has no
/// subscribe/wake path for rule rows (rules auto-fire at run start; later
/// reactive rule evaluation lands behind a separate card).
///
/// Lookup into [`RuleRow`] is by capture name (param name). Last-occurrence
/// wins on collision (rev-iter find), matching the SQLite drain's
/// HashMap-overwrite semantics.
#[derive(Debug)]
pub struct RulePredicateOp {
    pub name: Arc<str>,
    pub mode: RuleMode,
}

#[derive(Debug)]
pub enum RuleMode {
    Probe { args: Vec<RelationArg> },
    Join  { slots: Vec<RuleSlot> },
    Query { holes: Vec<Arc<str>> },
}

#[derive(Debug, Clone)]
pub enum RuleSlot {
    Bound(RelationArg),
    Project(Arc<str>),
}

impl RulePredicateOp {
    /// Classify call-site values into Probe / Join / Query. The rule name
    /// is supplied separately by the lower (the bare-name `foo?(...)` form
    /// has no `:name` first-arg; the rule name is `inv.name`).
    pub fn classify(name: Arc<str>, values: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        let mut have_bound   = false;
        let mut have_unbound = false;
        for v in &values {
            match v {
                Value::Term { mode: TermMode::Read, .. } => have_bound = true,
                Value::Term { mode: TermMode::Unbound, .. } => have_unbound = true,
                Value::Atom(_) | Value::Str(_) | Value::Int(_) | Value::Float(_) => have_bound = true,
                Value::Op(_) => {
                    return Err(vec![PatternDiagnostic {
                        code:       "rule/op-arg-unsupported",
                        message:    "rule? does not accept op invocations as args".into(),
                        byte_range: 0..0,
                    }]);
                }
            }
        }

        // Empty arg list: all-arity-0 rule. Treat as Probe with empty key —
        // hits if rule_rows[name] is non-empty (rule fired at least once).
        if !have_bound && !have_unbound {
            return Ok(Self { name, mode: RuleMode::Probe { args: Vec::new() } });
        }

        if have_unbound && !have_bound {
            let holes: Vec<Arc<str>> = values
                .into_iter()
                .map(|v| match v {
                    Value::Term { name, .. } => name,
                    _ => unreachable!("classification said all unbound"),
                })
                .collect();
            return Ok(Self { name, mode: RuleMode::Query { holes } });
        }

        if have_bound && !have_unbound {
            let args: Vec<RelationArg> = values
                .into_iter()
                .map(|v| value_to_relation_arg(v))
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(Self { name, mode: RuleMode::Probe { args } });
        }

        let slots: Vec<RuleSlot> = values
            .into_iter()
            .map(|v| match v {
                Value::Term { name, mode: TermMode::Unbound } => Ok(RuleSlot::Project(name)),
                v => value_to_relation_arg(v).map(RuleSlot::Bound),
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { name, mode: RuleMode::Join { slots } })
    }
}

fn value_to_relation_arg(v: Value) -> Result<RelationArg, Vec<PatternDiagnostic>> {
    Ok(match v {
        Value::Term { name, mode: TermMode::Read } => RelationArg::Capture(name),
        Value::Atom(s) | Value::Str(s) => RelationArg::Literal(s),
        Value::Int(n) => RelationArg::Literal(Arc::from(n.to_string())),
        Value::Float(n) => RelationArg::Literal(Arc::from(n.to_string())),
        Value::Term { mode: TermMode::Unbound, .. } | Value::Op(_) => {
            return Err(vec![PatternDiagnostic {
                code:       "rule/bad-bound-arg",
                message:    "rule? bound arg must be capture-read or literal".into(),
                byte_range: 0..0,
            }]);
        }
    })
}

/// Last-occurrence-wins lookup of a capture-named value in a [`RuleRow`].
fn rule_row_lookup<'a>(row: &'a RuleRow, key: &str) -> Option<&'a Arc<str>> {
    row.iter().rev().find_map(|(n, v)| if &**n == key { Some(v) } else { None })
}

impl Op for RulePredicateOp {
    fn name(&self) -> &'static str { "rule_predicate" }

    fn pipe<'a>(&'a self, ctx: &'a RtCtx, batch: Arc<[Cursor]>) -> BoxFuture<'a, Arc<[Cursor]>> {
        Box::pin(async move {
            if batch.is_empty() {
                return batch;
            }
            let store = ctx
                .store::<RelationStore>()
                .expect("RelationStore not registered on RtCtx");
            let params = store.params(&self.name);
            let snapshot = store.rule_rows_snapshot();
            let rows: Vec<RuleRow> = snapshot.get(&self.name).cloned().unwrap_or_default();

            let mut out: Vec<Cursor> = Vec::new();
            match &self.mode {
                RuleMode::Probe { args } => {
                    for cursor in batch.iter() {
                        let want = materialize_row(args, cursor);
                        let hit = rows.iter().any(|row| {
                            params.iter().zip(want.iter()).all(|(pname, want_v)| {
                                rule_row_lookup(row, pname)
                                    .map(|got| got == want_v)
                                    .unwrap_or(false)
                            })
                        });
                        // Empty-args probe: pass if any row exists at all.
                        let hit = if args.is_empty() { !rows.is_empty() } else { hit };
                        if hit {
                            out.push(cursor.clone());
                        }
                    }
                }
                RuleMode::Join { slots } => {
                    for cursor in batch.iter() {
                        let bound: Vec<(usize, Arc<str>)> = slots
                            .iter()
                            .enumerate()
                            .filter_map(|(i, slot)| match slot {
                                RuleSlot::Bound(arg) => {
                                    let v = materialize_row(std::slice::from_ref(arg), cursor)
                                        .into_iter()
                                        .next()
                                        .unwrap_or_else(|| Arc::from(""));
                                    Some((i, v))
                                }
                                RuleSlot::Project(_) => None,
                            })
                            .collect();
                        for row in rows.iter() {
                            let pass = bound.iter().all(|(i, want)| {
                                let Some(pname) = params.get(*i) else { return false };
                                rule_row_lookup(row, pname)
                                    .map(|got| got == want)
                                    .unwrap_or(false)
                            });
                            if !pass {
                                continue;
                            }
                            let mut c = cursor.clone();
                            for (i, slot) in slots.iter().enumerate() {
                                if let RuleSlot::Project(hole) = slot {
                                    let Some(pname) = params.get(i) else { continue };
                                    if let Some(val) = rule_row_lookup(row, pname) {
                                        c.captures
                                            .push(Capture::synthesized(hole.clone(), val.clone()));
                                    }
                                }
                            }
                            out.push(c);
                        }
                    }
                }
                RuleMode::Query { holes } => {
                    for cursor in batch.iter() {
                        for row in rows.iter() {
                            let mut c = cursor.clone();
                            for (i, hole) in holes.iter().enumerate() {
                                let Some(pname) = params.get(i) else { continue };
                                if let Some(val) = rule_row_lookup(row, pname) {
                                    c.captures
                                        .push(Capture::synthesized(hole.clone(), val.clone()));
                                }
                            }
                            out.push(c);
                        }
                    }
                }
            }
            Arc::from(out)
        })
    }
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
