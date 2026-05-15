use std::collections::BTreeSet;
use std::marker::PhantomData;
use std::sync::Arc;

use super::{FactStore, RenderCtx, Row, RuntimePut};

pub const RUNTIME_NODE: &str = "runtime_node";
pub const RUNTIME_EDGE: &str = "runtime_edge";
pub const RUNTIME_VALUE: &str = "runtime_value";
pub const RUNTIME_EDGE_VALUE: &str = "runtime_edge_value";
pub const RUNTIME_EVENT: &str = "runtime_event";
pub const RUNTIME_JOB: &str = "runtime_job";
pub const RUNTIME_CONTINUATION: &str = "runtime_continuation";

pub const NODE_ID: &str = "node_uri_id";
pub const NODE_KIND_ID: &str = "node_kind_id";
pub const AST_ID: &str = "ast_uri_id";
pub const PARENT_ID: &str = "parent_uri_id";
pub const INPUT_KEY: &str = "input_key";
pub const SOURCE_KEY: &str = "source_hash";
pub const MODE_ID: &str = "mode_id";
pub const GENERATION: &str = "generation";

pub const EDGE_ID: &str = "edge_uri_id";
pub const EDGE_KIND_ID: &str = "edge_kind_id";
pub const FROM_ID: &str = "from_uri_id";
pub const TO_ID: &str = "to_uri_id";
pub const LABEL_ID: &str = "label_id";

pub const VALUE_ID: &str = "value_uri_id";
pub const VALUE_KEY: &str = "value_key";
pub const VALUE_KIND_ID: &str = "value_kind_id";
pub const VALUE_REF: &str = "value_ref_id";
pub const VALUE_BLOB: &str = "value_blob";
pub const STATE_ID: &str = "state_id";

pub const EVENT_ID: &str = "event_uri_id";
pub const EVENT_SOURCE_ID: &str = "source_uri_id";
pub const EVENT_VALUE_ID: &str = "value_uri_id";
pub const CONSUMED: &str = "consumed";

pub const JOB_ID: &str = "job_uri_id";
pub const JOB_EVENT_ID: &str = "event_uri_id";
pub const JOB_OWNER_ID: &str = "owner_uri_id";
pub const JOB_STATE: &str = "state";
pub const JOB_PENDING: &str = "pending";
pub const JOB_DONE: &str = "done";

pub const CONT_OWNER_ID: &str = "owner_uri_id";
pub const CONT_PIPE_HASH: &str = "pipe_hash";
pub const CONT_INSTANCE_ID: &str = "instance_id";
pub const CONT_DEPTH: &str = "depth";
pub const CONT_INPUT_CURSOR: &str = "input_cursor";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeNode {
    pub node_id: String,
    pub kind_id: String,
    pub ast_id: Option<String>,
    pub parent_id: Option<String>,
    pub input_key: String,
    pub source_key: String,
    pub mode_id: Option<String>,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeEdge {
    pub edge_id: String,
    pub kind_id: String,
    pub from_id: String,
    pub to_id: String,
    pub label_id: String,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeValue {
    pub value_id: String,
    pub value_key: String,
    pub kind_id: String,
    pub payload: RuntimeValuePayload,
    pub state_id: String,
    pub generation: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum RuntimeValuePayload {
    #[default]
    None,
    Ref(String),
    InlineCell(String),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RuntimeEvent {
    pub event_id: String,
    pub source_id: String,
    pub value_id: String,
    pub generation: u64,
    pub consumed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RuntimeJob {
    pub job_id: String,
    pub event_id: String,
    pub owner_id: String,
    pub generation: u64,
}

/// Reasons `run_to_quiescence` may bail out before draining.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuiescenceError {
    /// Hit the iteration cap with non-empty `pending_jobs`. Likely a
    /// cycle in the rule graph (A wakes B wakes A).
    IterationCap { iters: usize, handled: usize },
}

impl std::fmt::Display for QuiescenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuiescenceError::IterationCap { iters, handled } => write!(
                f,
                "run_to_quiescence hit iteration cap (iters={iters}, handled={handled})"
            ),
        }
    }
}

impl std::error::Error for QuiescenceError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeContinuation {
    pub owner_id: String,
    pub pipe_hash: u64,
    pub instance_id: u64,
    pub depth: u32,
    /// Opaque hex-encoded input snapshot. Encoding choice belongs to
    /// the caller (sprf side stores `cursor_codec::encode` output).
    pub input_hex: String,
    pub generation: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VisibleDelta {
    pub inserted: Vec<String>,
    pub retracted: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubResult {
    pub edge_id: String,
    pub latest: Option<String>,
}

pub struct Subscribe<R: Row> {
    pub owner_id: String,
    pub label_id: String,
    pub source_id: String,
    pub edge_id: String,
    pub subscribe_kind_id: String,
    pub generation: u64,
    _row: PhantomData<fn() -> R>,
}

impl<R: Row> Subscribe<R> {
    pub fn new(
        owner_id: impl Into<String>,
        label_id: impl Into<String>,
        source_id: impl Into<String>,
        edge_id: impl Into<String>,
        subscribe_kind_id: impl Into<String>,
        generation: u64,
    ) -> Self {
        Self {
            owner_id: owner_id.into(),
            label_id: label_id.into(),
            source_id: source_id.into(),
            edge_id: edge_id.into(),
            subscribe_kind_id: subscribe_kind_id.into(),
            generation,
            _row: PhantomData,
        }
    }
}

impl<R: Row> RuntimePut for Subscribe<R> {
    type Output = SubResult;

    fn apply(self, ctx: &RenderCtx) -> Self::Output {
        let graph = ctx
            .runtime::<FactRuntimeGraph<R>>()
            .expect("runtime graph missing from RenderCtx");
        graph.insert_edge(RuntimeEdge {
            edge_id: self.edge_id.clone(),
            kind_id: self.subscribe_kind_id,
            from_id: self.owner_id,
            to_id: self.source_id,
            label_id: self.label_id,
            generation: self.generation,
        });
        let latest = graph
            .edge_values(&[self.edge_id.clone()])
            .into_iter()
            .next()
            .flatten();
        SubResult {
            edge_id: self.edge_id,
            latest,
        }
    }
}

pub struct SupportRows<R: Row> {
    pub owner_id: String,
    pub table_label_id: String,
    pub row_ids: Vec<String>,
    pub support_kind_id: String,
    pub generation: u64,
    _row: PhantomData<fn() -> R>,
}

impl<R: Row> SupportRows<R> {
    pub fn new(
        owner_id: impl Into<String>,
        table_label_id: impl Into<String>,
        row_ids: Vec<String>,
        support_kind_id: impl Into<String>,
        generation: u64,
    ) -> Self {
        Self {
            owner_id: owner_id.into(),
            table_label_id: table_label_id.into(),
            row_ids,
            support_kind_id: support_kind_id.into(),
            generation,
            _row: PhantomData,
        }
    }
}

impl<R: Row> RuntimePut for SupportRows<R> {
    type Output = VisibleDelta;

    fn apply(self, ctx: &RenderCtx) -> Self::Output {
        let graph = ctx
            .runtime::<FactRuntimeGraph<R>>()
            .expect("runtime graph missing from RenderCtx");
        graph.replace_supports(
            self.owner_id.as_str(),
            self.table_label_id.as_str(),
            &self.row_ids,
            self.support_kind_id.as_str(),
            self.generation,
        )
    }
}

pub struct ActiveChild<R: Row> {
    pub owner_id: String,
    pub label_id: String,
    pub child_id: String,
    pub active_kind_id: String,
    pub edge_id: String,
    pub generation: u64,
    _row: PhantomData<fn() -> R>,
}

impl<R: Row> ActiveChild<R> {
    pub fn new(
        owner_id: impl Into<String>,
        label_id: impl Into<String>,
        child_id: impl Into<String>,
        active_kind_id: impl Into<String>,
        edge_id: impl Into<String>,
        generation: u64,
    ) -> Self {
        Self {
            owner_id: owner_id.into(),
            label_id: label_id.into(),
            child_id: child_id.into(),
            active_kind_id: active_kind_id.into(),
            edge_id: edge_id.into(),
            generation,
            _row: PhantomData,
        }
    }
}

impl<R: Row> RuntimePut for ActiveChild<R> {
    type Output = Option<String>;

    fn apply(self, ctx: &RenderCtx) -> Self::Output {
        let graph = ctx
            .runtime::<FactRuntimeGraph<R>>()
            .expect("runtime graph missing from RenderCtx");
        graph.replace_active_child(
            self.owner_id.as_str(),
            self.label_id.as_str(),
            self.child_id.as_str(),
            self.active_kind_id.as_str(),
            self.edge_id.as_str(),
            self.generation,
        );
        graph.active_child(
            self.owner_id.as_str(),
            self.label_id.as_str(),
            self.active_kind_id.as_str(),
        )
    }
}

pub struct EmitValue<R: Row> {
    pub value: RuntimeValue,
    _row: PhantomData<fn() -> R>,
}

impl<R: Row> EmitValue<R> {
    pub fn new(value: RuntimeValue) -> Self {
        Self {
            value,
            _row: PhantomData,
        }
    }
}

impl<R: Row> RuntimePut for EmitValue<R> {
    type Output = String;

    fn apply(self, ctx: &RenderCtx) -> Self::Output {
        let graph = ctx
            .runtime::<FactRuntimeGraph<R>>()
            .expect("runtime graph missing from RenderCtx");
        let value_id = self.value.value_id.clone();
        graph.insert_value(self.value);
        value_id
    }
}

#[derive(Clone)]
pub struct FactRuntimeGraph<R: Row> {
    facts: Arc<dyn FactStore<R>>,
}

impl<R: Row> FactRuntimeGraph<R> {
    pub fn new(facts: Arc<dyn FactStore<R>>) -> Self {
        let graph = Self { facts };
        graph.declare_tables();
        graph
    }

    pub fn facts(&self) -> &Arc<dyn FactStore<R>> {
        &self.facts
    }

    pub fn insert_node(&self, node: RuntimeNode) {
        if self.has_row(RUNTIME_NODE, NODE_ID, node.node_id.as_str()) {
            return;
        }
        let mut row = R::default();
        row.set(NODE_ID, node.node_id.as_str());
        row.set(NODE_KIND_ID, node.kind_id.as_str());
        row.set(AST_ID, node.ast_id.as_deref().unwrap_or(""));
        row.set(PARENT_ID, node.parent_id.as_deref().unwrap_or(""));
        row.set(INPUT_KEY, node.input_key.as_str());
        row.set(SOURCE_KEY, node.source_key.as_str());
        row.set(MODE_ID, node.mode_id.as_deref().unwrap_or(""));
        row.set(GENERATION, node.generation.to_string().as_str());
        self.facts.insert_batch(RUNTIME_NODE, vec![Arc::new(row)]);
    }

    pub fn insert_edge(&self, edge: RuntimeEdge) {
        if self.has_row(RUNTIME_EDGE, EDGE_ID, edge.edge_id.as_str()) {
            return;
        }
        let mut row = R::default();
        row.set(EDGE_ID, edge.edge_id.as_str());
        row.set(EDGE_KIND_ID, edge.kind_id.as_str());
        row.set(FROM_ID, edge.from_id.as_str());
        row.set(TO_ID, edge.to_id.as_str());
        row.set(LABEL_ID, edge.label_id.as_str());
        row.set(GENERATION, edge.generation.to_string().as_str());
        self.facts.insert_batch(RUNTIME_EDGE, vec![Arc::new(row)]);
    }

    pub fn edges_where(
        &self,
        kind_id: Option<&str>,
        from_id: Option<&str>,
        to_id: Option<&str>,
        label_id: Option<&str>,
    ) -> Vec<RuntimeEdge> {
        let mut edges: Vec<RuntimeEdge> = self
            .facts
            .rows_of(RUNTIME_EDGE)
            .into_iter()
            .filter(|row| {
                kind_id
                    .map(|v| row.get(EDGE_KIND_ID) == Some(v))
                    .unwrap_or(true)
                    && from_id.map(|v| row.get(FROM_ID) == Some(v)).unwrap_or(true)
                    && to_id.map(|v| row.get(TO_ID) == Some(v)).unwrap_or(true)
                    && label_id
                        .map(|v| row.get(LABEL_ID) == Some(v))
                        .unwrap_or(true)
            })
            .filter_map(edge_from_row)
            .collect();
        edges.sort_by(|a, b| a.edge_id.cmp(&b.edge_id));
        edges
    }

    pub fn delete_edge(&self, edge_id: &str) -> usize {
        self.facts
            .delete_matching(RUNTIME_EDGE, &[(EDGE_ID, edge_id)])
    }

    pub fn insert_value(&self, value: RuntimeValue) {
        if self.has_row(RUNTIME_VALUE, VALUE_ID, value.value_id.as_str()) {
            return;
        }
        let mut row = R::default();
        row.set(VALUE_ID, value.value_id.as_str());
        row.set(VALUE_KEY, value.value_key.as_str());
        row.set(VALUE_KIND_ID, value.kind_id.as_str());
        match value.payload {
            RuntimeValuePayload::None => {
                row.set(VALUE_REF, "");
                row.set(VALUE_BLOB, "");
            }
            RuntimeValuePayload::Ref(value_ref) => {
                row.set(VALUE_REF, value_ref.as_str());
                row.set(VALUE_BLOB, "");
            }
            RuntimeValuePayload::InlineCell(blob) => {
                row.set(VALUE_REF, "");
                row.set(VALUE_BLOB, blob.as_str());
            }
        }
        row.set(STATE_ID, value.state_id.as_str());
        row.set(GENERATION, value.generation.to_string().as_str());
        self.facts.insert_batch(RUNTIME_VALUE, vec![Arc::new(row)]);
    }

    pub fn set_edge_value(&self, edge_id: &str, label_id: &str, value_id: &str, generation: u64) {
        self.facts.delete_matching(
            RUNTIME_EDGE_VALUE,
            &[(EDGE_ID, edge_id), (LABEL_ID, label_id)],
        );
        let mut row = R::default();
        row.set(EDGE_ID, edge_id);
        row.set(LABEL_ID, label_id);
        row.set(VALUE_ID, value_id);
        row.set(GENERATION, generation.to_string().as_str());
        self.facts
            .insert_batch(RUNTIME_EDGE_VALUE, vec![Arc::new(row)]);
    }

    pub fn append_event(&self, event_id: &str, source_id: &str, value_id: &str, generation: u64) {
        if self.has_row(RUNTIME_EVENT, EVENT_ID, event_id) {
            return;
        }
        let mut row = R::default();
        row.set(EVENT_ID, event_id);
        row.set(EVENT_SOURCE_ID, source_id);
        row.set(EVENT_VALUE_ID, value_id);
        row.set(GENERATION, generation.to_string().as_str());
        row.set(CONSUMED, "0");
        self.facts.insert_batch(RUNTIME_EVENT, vec![Arc::new(row)]);
    }

    /// Idempotent job insert. Caller computes `job_id` (sprf side uses a
    /// hash of event_id+owner_id). The row lands in PENDING state on
    /// first insert and stays untouched on duplicate calls.
    pub fn insert_job(&self, job_id: &str, event_id: &str, owner_id: &str, generation: u64) {
        if self.has_row(RUNTIME_JOB, JOB_ID, job_id) {
            return;
        }
        let mut row = R::default();
        row.set(JOB_ID, job_id);
        row.set(JOB_EVENT_ID, event_id);
        row.set(JOB_OWNER_ID, owner_id);
        row.set(JOB_STATE, JOB_PENDING);
        row.set(GENERATION, generation.to_string().as_str());
        self.facts.insert_batch(RUNTIME_JOB, vec![Arc::new(row)]);
    }

    pub fn pending_jobs(&self) -> Vec<RuntimeJob> {
        let mut jobs: Vec<RuntimeJob> = self
            .facts
            .rows_of(RUNTIME_JOB)
            .into_iter()
            .filter(|row| row.get(JOB_STATE) == Some(JOB_PENDING))
            .filter_map(|row| {
                Some(RuntimeJob {
                    job_id: row.get(JOB_ID)?.to_string(),
                    event_id: row.get(JOB_EVENT_ID)?.to_string(),
                    owner_id: row.get(JOB_OWNER_ID)?.to_string(),
                    generation: row.get(GENERATION)?.parse().ok()?,
                })
            })
            .collect();
        jobs.sort();
        jobs
    }

    /// Flip the job row to DONE. If no PENDING job remains for the same
    /// event, mark that event consumed too (matches the prior v4
    /// semantics that ratchet event lifecycle on the last job done).
    pub fn mark_job_done(&self, job_id: &str) {
        let Some(existing) = self
            .facts
            .read_where(RUNTIME_JOB, JOB_ID, job_id)
            .into_iter()
            .next()
        else {
            return;
        };
        let event_id = existing.get(JOB_EVENT_ID).unwrap_or("").to_string();
        let owner_id = existing.get(JOB_OWNER_ID).unwrap_or("").to_string();
        let generation = existing
            .get(GENERATION)
            .and_then(|g| g.parse::<u64>().ok())
            .unwrap_or(0);

        self.facts
            .delete_matching(RUNTIME_JOB, &[(JOB_ID, job_id)]);
        let mut row = R::default();
        row.set(JOB_ID, job_id);
        row.set(JOB_EVENT_ID, event_id.as_str());
        row.set(JOB_OWNER_ID, owner_id.as_str());
        row.set(JOB_STATE, JOB_DONE);
        row.set(GENERATION, generation.to_string().as_str());
        self.facts.insert_batch(RUNTIME_JOB, vec![Arc::new(row)]);

        let has_pending = self
            .facts
            .read_where(RUNTIME_JOB, JOB_EVENT_ID, event_id.as_str())
            .iter()
            .any(|row| row.get(JOB_STATE) == Some(JOB_PENDING));
        if !has_pending {
            self.mark_event_consumed(event_id.as_str());
        }
    }

    /// Drive pending jobs to fixed point. Each iteration drains the
    /// current `pending_jobs()` snapshot, calls `handler(&job)` for each
    /// job, then `mark_job_done(job_id)`. The handler may enqueue new
    /// jobs (via `notify_table_inserted` / `dispatch_wake`) during the
    /// loop; those jobs are picked up on the next iteration.
    ///
    /// Returns the total number of jobs run. Errors out with
    /// `QuiescenceError::IterationCap` once the iteration count exceeds
    /// `max_iters`.
    pub fn run_to_quiescence<F>(
        &self,
        max_iters: usize,
        mut handler: F,
    ) -> Result<usize, QuiescenceError>
    where
        F: FnMut(&RuntimeJob),
    {
        let mut handled = 0usize;
        for iter in 0..max_iters {
            let jobs = self.pending_jobs();
            if jobs.is_empty() {
                return Ok(handled);
            }
            for job in &jobs {
                handler(job);
                self.mark_job_done(job.job_id.as_str());
                handled += 1;
            }
            if iter + 1 == max_iters && !self.pending_jobs().is_empty() {
                return Err(QuiescenceError::IterationCap {
                    iters: max_iters,
                    handled,
                });
            }
        }
        Ok(handled)
    }

    /// Record (or replace) the continuation for `owner_id`. The
    /// `input_hex` blob is opaque to the generic layer; callers control
    /// the encoding.
    pub fn record_continuation(&self, cont: RuntimeContinuation) {
        self.facts.delete_matching(
            RUNTIME_CONTINUATION,
            &[(CONT_OWNER_ID, cont.owner_id.as_str())],
        );
        let mut row = R::default();
        row.set(CONT_OWNER_ID, cont.owner_id.as_str());
        row.set(CONT_PIPE_HASH, cont.pipe_hash.to_string().as_str());
        row.set(CONT_INSTANCE_ID, cont.instance_id.to_string().as_str());
        row.set(CONT_DEPTH, cont.depth.to_string().as_str());
        row.set(CONT_INPUT_CURSOR, cont.input_hex.as_str());
        row.set(GENERATION, cont.generation.to_string().as_str());
        self.facts
            .insert_batch(RUNTIME_CONTINUATION, vec![Arc::new(row)]);
    }

    pub fn continuation_for_owner(&self, owner_id: &str) -> Option<RuntimeContinuation> {
        let row = self
            .facts
            .read_where(RUNTIME_CONTINUATION, CONT_OWNER_ID, owner_id)
            .into_iter()
            .next()?;
        Some(RuntimeContinuation {
            owner_id: owner_id.to_string(),
            pipe_hash: row.get(CONT_PIPE_HASH)?.parse().ok()?,
            instance_id: row.get(CONT_INSTANCE_ID)?.parse().ok()?,
            depth: row.get(CONT_DEPTH)?.parse::<u64>().ok()? as u32,
            input_hex: row.get(CONT_INPUT_CURSOR)?.to_string(),
            generation: row.get(GENERATION)?.parse().ok()?,
        })
    }

    pub fn mark_event_consumed(&self, event_id: &str) {
        let Some(event) = self
            .unconsumed_events()
            .into_iter()
            .find(|event| event.event_id == event_id)
        else {
            return;
        };
        self.facts
            .delete_matching(RUNTIME_EVENT, &[(EVENT_ID, event_id)]);
        let mut row = R::default();
        row.set(EVENT_ID, event.event_id.as_str());
        row.set(EVENT_SOURCE_ID, event.source_id.as_str());
        row.set(EVENT_VALUE_ID, event.value_id.as_str());
        row.set(GENERATION, event.generation.to_string().as_str());
        row.set(CONSUMED, "1");
        self.facts.insert_batch(RUNTIME_EVENT, vec![Arc::new(row)]);
    }

    pub fn dispatch_wake(
        &self,
        source_id: &str,
        value_id: &str,
        event_id: &str,
        subscribe_kind_id: &str,
        generation: u64,
    ) -> Vec<String> {
        self.append_event(event_id, source_id, value_id, generation);
        let mut owners = BTreeSet::new();
        for edge in self.incoming_edges(subscribe_kind_id, source_id) {
            self.set_edge_value(
                edge.edge_id.as_str(),
                edge.label_id.as_str(),
                value_id,
                generation,
            );
            owners.insert(edge.from_id);
        }
        owners.into_iter().collect()
    }

    pub fn replace_supports(
        &self,
        owner_id: &str,
        table_label_id: &str,
        row_ids: &[String],
        support_kind_id: &str,
        generation: u64,
    ) -> VisibleDelta {
        let old = self.supported_rows_for_owner(owner_id, table_label_id, support_kind_id);
        let new: BTreeSet<String> = row_ids.iter().cloned().collect();
        let mut inserted = Vec::new();
        let mut retracted = Vec::new();

        for row_id in old.difference(&new) {
            let edge_id = support_edge_id(owner_id, row_id, table_label_id, support_kind_id);
            self.delete_edge(edge_id.as_str());
            if !self.row_has_support(row_id, support_kind_id) {
                retracted.push(row_id.clone());
            }
        }

        for row_id in new.difference(&old) {
            let already_visible = self.row_has_support(row_id, support_kind_id);
            self.insert_edge(RuntimeEdge {
                edge_id: support_edge_id(owner_id, row_id, table_label_id, support_kind_id),
                kind_id: support_kind_id.to_string(),
                from_id: owner_id.to_string(),
                to_id: row_id.clone(),
                label_id: table_label_id.to_string(),
                generation,
            });
            if !already_visible {
                inserted.push(row_id.clone());
            }
        }

        VisibleDelta {
            inserted,
            retracted,
        }
    }

    pub fn replace_active_child(
        &self,
        owner_id: &str,
        label_id: &str,
        child_id: &str,
        active_kind_id: &str,
        edge_id: &str,
        generation: u64,
    ) {
        self.facts.delete_matching(
            RUNTIME_EDGE,
            &[
                (EDGE_KIND_ID, active_kind_id),
                (FROM_ID, owner_id),
                (LABEL_ID, label_id),
            ],
        );
        self.insert_edge(RuntimeEdge {
            edge_id: edge_id.to_string(),
            kind_id: active_kind_id.to_string(),
            from_id: owner_id.to_string(),
            to_id: child_id.to_string(),
            label_id: label_id.to_string(),
            generation,
        });
    }

    pub fn active_child(
        &self,
        owner_id: &str,
        label_id: &str,
        active_kind_id: &str,
    ) -> Option<String> {
        self.facts
            .rows_of(RUNTIME_EDGE)
            .into_iter()
            .find_map(|row| {
                if row.get(EDGE_KIND_ID) == Some(active_kind_id)
                    && row.get(FROM_ID) == Some(owner_id)
                    && row.get(LABEL_ID) == Some(label_id)
                {
                    return row.get(TO_ID).map(str::to_string);
                }
                None
            })
    }

    pub fn edge_values(&self, edge_ids: &[String]) -> Vec<Option<String>> {
        edge_ids
            .iter()
            .map(|edge_id| {
                self.facts
                    .read_where(RUNTIME_EDGE_VALUE, EDGE_ID, edge_id.as_str())
                    .into_iter()
                    .next()
                    .and_then(|row| row.get(VALUE_ID).map(str::to_string))
            })
            .collect()
    }

    pub fn unconsumed_events(&self) -> Vec<RuntimeEvent> {
        let mut events: Vec<RuntimeEvent> = self
            .facts
            .rows_of(RUNTIME_EVENT)
            .into_iter()
            .filter(|row| row.get(CONSUMED) == Some("0"))
            .filter_map(|row| {
                Some(RuntimeEvent {
                    event_id: row.get(EVENT_ID)?.to_string(),
                    source_id: row.get(EVENT_SOURCE_ID)?.to_string(),
                    value_id: row.get(EVENT_VALUE_ID)?.to_string(),
                    generation: row.get(GENERATION)?.parse().ok()?,
                    consumed: false,
                })
            })
            .collect();
        events.sort();
        events
    }

    pub fn owners_for_unconsumed_events(&self, subscribe_kind_id: &str) -> Vec<String> {
        let mut owners = BTreeSet::new();
        for event in self.unconsumed_events() {
            for edge in self.incoming_edges(subscribe_kind_id, event.source_id.as_str()) {
                owners.insert(edge.from_id);
            }
        }
        owners.into_iter().collect()
    }

    fn declare_tables(&self) {
        self.facts.declare(
            RUNTIME_NODE,
            &[
                NODE_ID,
                NODE_KIND_ID,
                AST_ID,
                PARENT_ID,
                INPUT_KEY,
                SOURCE_KEY,
                MODE_ID,
                GENERATION,
            ],
        );
        self.facts.declare(
            RUNTIME_EDGE,
            &[EDGE_ID, EDGE_KIND_ID, FROM_ID, TO_ID, LABEL_ID, GENERATION],
        );
        self.facts.declare(
            RUNTIME_VALUE,
            &[
                VALUE_ID,
                VALUE_KEY,
                VALUE_KIND_ID,
                VALUE_REF,
                VALUE_BLOB,
                STATE_ID,
                GENERATION,
            ],
        );
        self.facts.declare(
            RUNTIME_EDGE_VALUE,
            &[EDGE_ID, LABEL_ID, VALUE_ID, GENERATION],
        );
        self.facts.declare(
            RUNTIME_EVENT,
            &[
                EVENT_ID,
                EVENT_SOURCE_ID,
                EVENT_VALUE_ID,
                GENERATION,
                CONSUMED,
            ],
        );
        self.facts.declare(
            RUNTIME_JOB,
            &[JOB_ID, JOB_EVENT_ID, JOB_OWNER_ID, JOB_STATE, GENERATION],
        );
        self.facts.declare(
            RUNTIME_CONTINUATION,
            &[
                CONT_OWNER_ID,
                CONT_PIPE_HASH,
                CONT_INSTANCE_ID,
                CONT_DEPTH,
                CONT_INPUT_CURSOR,
                GENERATION,
            ],
        );
    }

    fn has_row(&self, table: &str, col: &str, value: &str) -> bool {
        !self.facts.read_where(table, col, value).is_empty()
    }

    fn incoming_edges(&self, kind_id: &str, to_id: &str) -> Vec<RuntimeEdge> {
        let mut edges: Vec<RuntimeEdge> = self
            .facts
            .rows_of(RUNTIME_EDGE)
            .into_iter()
            .filter(|row| row.get(EDGE_KIND_ID) == Some(kind_id) && row.get(TO_ID) == Some(to_id))
            .filter_map(edge_from_row)
            .collect();
        edges.sort_by(|a, b| a.edge_id.cmp(&b.edge_id));
        edges
    }

    fn supported_rows_for_owner(
        &self,
        owner_id: &str,
        table_label_id: &str,
        support_kind_id: &str,
    ) -> BTreeSet<String> {
        self.facts
            .rows_of(RUNTIME_EDGE)
            .into_iter()
            .filter(|row| {
                row.get(EDGE_KIND_ID) == Some(support_kind_id)
                    && row.get(FROM_ID) == Some(owner_id)
                    && row.get(LABEL_ID) == Some(table_label_id)
            })
            .filter_map(|row| row.get(TO_ID).map(str::to_string))
            .collect()
    }

    fn row_has_support(&self, row_id: &str, support_kind_id: &str) -> bool {
        self.facts.rows_of(RUNTIME_EDGE).iter().any(|row| {
            row.get(EDGE_KIND_ID) == Some(support_kind_id) && row.get(TO_ID) == Some(row_id)
        })
    }
}

fn edge_from_row<R: Row>(row: Arc<R>) -> Option<RuntimeEdge> {
    Some(RuntimeEdge {
        edge_id: row.get(EDGE_ID)?.to_string(),
        kind_id: row.get(EDGE_KIND_ID)?.to_string(),
        from_id: row.get(FROM_ID)?.to_string(),
        to_id: row.get(TO_ID)?.to_string(),
        label_id: row.get(LABEL_ID)?.to_string(),
        generation: row.get(GENERATION)?.parse().ok()?,
    })
}

fn support_edge_id(
    owner_id: &str,
    row_id: &str,
    table_label_id: &str,
    support_kind_id: &str,
) -> String {
    let mut h = blake3::Hasher::new();
    for part in [support_kind_id, owner_id, row_id, table_label_id] {
        h.update(&(part.len() as u64).to_be_bytes());
        h.update(part.as_bytes());
    }
    h.finalize().to_hex().to_string()
}
