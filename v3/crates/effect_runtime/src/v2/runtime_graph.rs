use std::borrow::Cow;
use std::collections::BTreeSet;
use std::marker::PhantomData;
use std::sync::Arc;

use super::{FactStore, RenderCtx, Row, RuntimePut};

pub const RUNTIME_NODE: &str = "runtime_node";
pub const RUNTIME_EDGE: &str = "runtime_edge";
pub const RUNTIME_VALUE: &str = "runtime_value";
pub const RUNTIME_EDGE_VALUE: &str = "runtime_edge_value";
/// Dirty-owner worklist. Replaces the old RUNTIME_EVENT + RUNTIME_JOB
/// pair: one row per (owner, source) that still owes a re-render. A
/// wake marks rows here; the sweep drains and clears them. Additive on
/// disk: old fact DBs lack this table and gain it on first `declare`
/// (the schema registry handles the version bump implicitly, matching
/// the rest of this crate's no-ALTER convention). Stale RUNTIME_EVENT /
/// RUNTIME_JOB rows on old DBs become inert; nothing reads them.
pub const RUNTIME_DIRTY: &str = "runtime_dirty";
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

pub const DIRTY_OWNER_ID: &str = "owner_uri_id";
pub const DIRTY_SOURCE_ID: &str = "source_uri_id";

pub const CONT_OWNER_ID: &str = "owner_uri_id";
pub const CONT_PIPE_HASH: &str = "pipe_hash";
pub const CONT_INSTANCE_ID: &str = "instance_id";
pub const CONT_DEPTH: &str = "depth";
pub const CONT_INPUT_CURSOR: &str = "input_cursor";

/// Identity type carried by every runtime-graph node/edge field. The
/// graph never inspects the value; it only round-trips it through the
/// fact-store string column. `String` is the default so existing
/// callers (and the in-memory test store) compile unchanged; sprf
/// passes its interned `StringId` so the boundary stops paying the
/// `StringId.0.to_string()` / `parse::<u64>().ok()?` tax.
pub trait NodeId: Clone + Ord + std::hash::Hash + std::fmt::Debug {
    /// View as the on-disk string form. Borrowed when the type already
    /// holds a string; owned when it has to render (e.g. an integer id).
    fn as_id_str(&self) -> Cow<'_, str>;
    /// Reconstruct from the on-disk string form. Round-trips
    /// `as_id_str`. Unparseable input yields the type's natural
    /// fallback rather than panicking, matching the prior `.ok()?`
    /// callers that silently dropped malformed rows.
    fn from_id_str(s: &str) -> Self;
}

impl NodeId for String {
    fn as_id_str(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.as_str())
    }
    fn from_id_str(s: &str) -> Self {
        s.to_string()
    }
}

/// Compile-time tag for one kind of runtime-graph handle. `TAG` is the
/// on-disk `kind_id` string the handle projects onto (no schema change;
/// the same `RuntimeNode`/`RuntimeEdge` rows back every kind). `Extra`
/// carries whatever per-kind payload the handle needs beyond `(id,
/// uri)`: `()` for plain nodes/edges, an interned label for subscribe
/// edges, a row key for row nodes.
pub trait NodeKind {
    const TAG: &'static str;
    // `Ord` is required because the old mirror structs derived `Ord`
    // and several call sites key BTreeMap/BTreeSet on these handles.
    // All concrete `Extra` types (`()`, `StringId`, `Arc<str>`) satisfy
    // it, so this does not narrow the usable kinds.
    type Extra: Clone + Ord + std::hash::Hash + std::fmt::Debug;
}

/// Phantom-typed handle for a runtime-graph node or edge. One struct
/// replaces the per-kind mirror structs callers used to hand-roll: the
/// kind lives in the `K` type parameter so a `GraphRef<Source, _>`
/// cannot be passed where a `GraphRef<Owner, _>` is expected, and the
/// shared `(id, uri, extra)` shape projects onto the existing
/// `RuntimeNode`/`RuntimeEdge` rows via `K::TAG`. `K` is zero-size and
/// must NOT appear in the trait-impl bounds below, so the std traits
/// are hand-written rather than derived (a `#[derive]` would emit
/// `K: Clone` etc. through `PhantomData<K>` and over-constrain callers).
pub struct GraphRef<K: NodeKind, I: NodeId = String> {
    pub id: I,
    pub uri: Arc<str>,
    pub extra: K::Extra,
    _k: PhantomData<K>,
}

impl<K: NodeKind, I: NodeId> GraphRef<K, I> {
    pub fn new(id: I, uri: Arc<str>, extra: K::Extra) -> Self {
        Self {
            id,
            uri,
            extra,
            _k: PhantomData,
        }
    }

    pub fn id(&self) -> &I {
        &self.id
    }

    pub fn uri(&self) -> &Arc<str> {
        &self.uri
    }

    pub fn extra(&self) -> &K::Extra {
        &self.extra
    }

    /// The on-disk `kind_id` string this handle projects onto.
    pub fn tag() -> &'static str {
        K::TAG
    }
}

impl<K: NodeKind, I: NodeId> Clone for GraphRef<K, I> {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            uri: self.uri.clone(),
            extra: self.extra.clone(),
            _k: PhantomData,
        }
    }
}

impl<K: NodeKind, I: NodeId> std::fmt::Debug for GraphRef<K, I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphRef")
            .field("kind", &K::TAG)
            .field("id", &self.id)
            .field("uri", &self.uri)
            .field("extra", &self.extra)
            .finish()
    }
}

impl<K: NodeKind, I: NodeId> PartialEq for GraphRef<K, I> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.uri == other.uri && self.extra == other.extra
    }
}

impl<K: NodeKind, I: NodeId> Eq for GraphRef<K, I> {}

impl<K: NodeKind, I: NodeId> std::hash::Hash for GraphRef<K, I> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.uri.hash(state);
        self.extra.hash(state);
    }
}

impl<K: NodeKind, I: NodeId> PartialOrd for GraphRef<K, I> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<K: NodeKind, I: NodeId> Ord for GraphRef<K, I> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id
            .cmp(&other.id)
            .then_with(|| self.uri.cmp(&other.uri))
            .then_with(|| self.extra.cmp(&other.extra))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeNode<I: NodeId = String> {
    pub node_id: I,
    pub kind_id: I,
    pub ast_id: Option<I>,
    pub parent_id: Option<I>,
    pub input_key: String,
    pub source_key: String,
    pub mode_id: Option<I>,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeEdge<I: NodeId = String> {
    pub edge_id: I,
    pub kind_id: I,
    pub from_id: I,
    pub to_id: I,
    pub label_id: I,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeValue<I: NodeId = String> {
    pub value_id: I,
    pub value_key: String,
    pub kind_id: I,
    pub payload: RuntimeValuePayload,
    pub state_id: I,
    pub generation: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum RuntimeValuePayload {
    #[default]
    None,
    Ref(String),
    InlineCell(String),
}

/// One outstanding re-render: a `(owner, source)` pair the sweep has
/// not yet drained. `job_id` is a deterministic hash of the triple so
/// callers can de-dup work the same way the old RUNTIME_JOB row id did;
/// it carries no extra state. Generic over `I: NodeId` to match the
/// rest of the graph; owner/source are interned ids, not bare strings.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DirtyOwner<I: NodeId = String> {
    pub job_id: String,
    pub owner_id: I,
    pub source_id: I,
    pub generation: u64,
}

/// Order in which `sweep_to_quiescence` visits dirty owners. The dirty
/// table is order-agnostic; the sweep picks the discipline. Default
/// `BreadthFirst` reproduces the old wave-drain `run_to_quiescence`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TraversalOrder {
    /// Drain the whole current dirty snapshot, then re-check. Cascades
    /// surface one level per outer iteration.
    #[default]
    BreadthFirst,
    /// LIFO: a freshly-dirtied owner runs before any sibling still on
    /// the worklist. Follows one cascade chain to its tip first.
    DepthFirst,
    /// Pull exactly one dirty owner per call. The caller decides when
    /// to re-enter; the sweep never loops internally.
    Streaming,
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
pub struct RuntimeContinuation<I: NodeId = String> {
    pub owner_id: I,
    pub pipe_hash: u64,
    pub instance_id: u64,
    pub depth: u32,
    /// Opaque hex-encoded input snapshot. Encoding choice belongs to
    /// the caller (sprf side stores `cursor_codec::encode` output).
    pub input_hex: String,
    pub generation: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VisibleDelta<I: NodeId = String> {
    pub inserted: Vec<I>,
    pub retracted: Vec<I>,
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
pub struct FactRuntimeGraph<R: Row, I: NodeId = String> {
    facts: Arc<dyn FactStore<R>>,
    _id: PhantomData<fn() -> I>,
}

impl<R: Row, I: NodeId> FactRuntimeGraph<R, I> {
    pub fn new(facts: Arc<dyn FactStore<R>>) -> Self {
        let graph = Self {
            facts,
            _id: PhantomData,
        };
        graph.declare_tables();
        graph
    }

    pub fn facts(&self) -> &Arc<dyn FactStore<R>> {
        &self.facts
    }

    pub fn insert_node(&self, node: RuntimeNode<I>) {
        let node_id = node.node_id.as_id_str();
        if self.has_row(RUNTIME_NODE, NODE_ID, node_id.as_ref()) {
            return;
        }
        let mut row = R::default();
        row.set(NODE_ID, node_id.as_ref());
        row.set(NODE_KIND_ID, node.kind_id.as_id_str().as_ref());
        row.set(
            AST_ID,
            node.ast_id
                .as_ref()
                .map(|v| v.as_id_str())
                .as_deref()
                .unwrap_or(""),
        );
        row.set(
            PARENT_ID,
            node.parent_id
                .as_ref()
                .map(|v| v.as_id_str())
                .as_deref()
                .unwrap_or(""),
        );
        row.set(INPUT_KEY, node.input_key.as_str());
        row.set(SOURCE_KEY, node.source_key.as_str());
        row.set(
            MODE_ID,
            node.mode_id
                .as_ref()
                .map(|v| v.as_id_str())
                .as_deref()
                .unwrap_or(""),
        );
        row.set(GENERATION, node.generation.to_string().as_str());
        self.facts.insert_batch(RUNTIME_NODE, vec![Arc::new(row)]);
    }

    pub fn insert_edge(&self, edge: RuntimeEdge<I>) {
        let edge_id = edge.edge_id.as_id_str();
        if self.has_row(RUNTIME_EDGE, EDGE_ID, edge_id.as_ref()) {
            return;
        }
        let mut row = R::default();
        row.set(EDGE_ID, edge_id.as_ref());
        row.set(EDGE_KIND_ID, edge.kind_id.as_id_str().as_ref());
        row.set(FROM_ID, edge.from_id.as_id_str().as_ref());
        row.set(TO_ID, edge.to_id.as_id_str().as_ref());
        row.set(LABEL_ID, edge.label_id.as_id_str().as_ref());
        row.set(GENERATION, edge.generation.to_string().as_str());
        self.facts.insert_batch(RUNTIME_EDGE, vec![Arc::new(row)]);
    }

    pub fn edges_where(
        &self,
        kind_id: Option<&str>,
        from_id: Option<&str>,
        to_id: Option<&str>,
        label_id: Option<&str>,
    ) -> Vec<RuntimeEdge<I>> {
        let mut edges: Vec<RuntimeEdge<I>> = self
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

    pub fn insert_value(&self, value: RuntimeValue<I>) {
        let value_id = value.value_id.as_id_str();
        if self.has_row(RUNTIME_VALUE, VALUE_ID, value_id.as_ref()) {
            return;
        }
        let mut row = R::default();
        row.set(VALUE_ID, value_id.as_ref());
        row.set(VALUE_KEY, value.value_key.as_str());
        row.set(VALUE_KIND_ID, value.kind_id.as_id_str().as_ref());
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
        row.set(STATE_ID, value.state_id.as_id_str().as_ref());
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

    /// Mark `owner` dirty for `source`. Idempotent per `(owner, source,
    /// generation)`: a second mark before the sweep drains it is a
    /// no-op (matches the old "one event per (gen, source), one job per
    /// (event, owner)" dedup, minus the two-table indirection). A later
    /// `clear_dirty` followed by a fresh mark re-creates the row.
    pub fn mark_dirty(&self, owner_id: &str, source_id: &str, generation: u64) {
        if self
            .facts
            .read_where(RUNTIME_DIRTY, DIRTY_OWNER_ID, owner_id)
            .iter()
            .any(|row| {
                row.get(DIRTY_SOURCE_ID) == Some(source_id)
                    && row.get(GENERATION).and_then(|g| g.parse::<u64>().ok())
                        == Some(generation)
            })
        {
            return;
        }
        let mut row = R::default();
        row.set(DIRTY_OWNER_ID, owner_id);
        row.set(DIRTY_SOURCE_ID, source_id);
        row.set(GENERATION, generation.to_string().as_str());
        self.facts.insert_batch(RUNTIME_DIRTY, vec![Arc::new(row)]);
    }

    /// Snapshot of every owner that still owes a re-render. Stable order
    /// (sorted by deterministic job id) so callers can de-dup; the
    /// traversal discipline lives in `sweep_to_quiescence`.
    pub fn dirty_owners(&self) -> Vec<DirtyOwner<I>> {
        let mut jobs: Vec<DirtyOwner<I>> = self
            .facts
            .rows_of(RUNTIME_DIRTY)
            .into_iter()
            .filter_map(|row| {
                let owner_id = row.get(DIRTY_OWNER_ID)?;
                let source_id = row.get(DIRTY_SOURCE_ID)?;
                let generation = row.get(GENERATION)?.parse().ok()?;
                Some(DirtyOwner {
                    job_id: dirty_job_id(owner_id, source_id, generation),
                    owner_id: I::from_id_str(owner_id),
                    source_id: I::from_id_str(source_id),
                    generation,
                })
            })
            .collect();
        jobs.sort();
        jobs
    }

    /// Clear every dirty row for `owner_id`. Called once the owner has
    /// been re-rendered; one sweep of the worklist for that owner.
    pub fn clear_dirty(&self, owner_id: &str) {
        self.facts
            .delete_matching(RUNTIME_DIRTY, &[(DIRTY_OWNER_ID, owner_id)]);
    }

    /// Clear exactly the dirty row drained by the sweep, not every row
    /// for the owner. Without this, work the handler enqueues for the
    /// same owner during its own re-render would be wiped (the old
    /// per-job `mark_job_done` only retired the one job it ran).
    fn clear_dirty_one(&self, owner_id: &str, source_id: &str, generation: u64) {
        let gen_text = generation.to_string();
        self.facts.delete_matching(
            RUNTIME_DIRTY,
            &[
                (DIRTY_OWNER_ID, owner_id),
                (DIRTY_SOURCE_ID, source_id),
                (GENERATION, gen_text.as_str()),
            ],
        );
    }

    /// Drive the dirty worklist to a fixed point under `order`. The
    /// handler may dirty more owners (via `notify_table_inserted` /
    /// `dispatch_wake`) while running; those are picked up per `order`.
    /// Returns the count of owners run, or `IterationCap` once the work
    /// has not settled within `max_iters` rounds.
    ///
    /// `BreadthFirst` reproduces the old wave-drain. `DepthFirst` walks
    /// one cascade chain to its tip before siblings. `Streaming` runs a
    /// single owner per call and returns immediately.
    pub fn sweep_to_quiescence<F>(
        &self,
        max_iters: usize,
        order: TraversalOrder,
        mut handler: F,
    ) -> Result<usize, QuiescenceError>
    where
        F: FnMut(&DirtyOwner<I>),
    {
        let mut handled = 0usize;
        match order {
            TraversalOrder::Streaming => {
                if let Some(job) = self.dirty_owners().into_iter().next() {
                    handler(&job);
                    self.clear_dirty_one(
                        job.owner_id.as_id_str().as_ref(),
                        job.source_id.as_id_str().as_ref(),
                        job.generation,
                    );
                    handled += 1;
                }
                Ok(handled)
            }
            TraversalOrder::BreadthFirst => {
                for iter in 0..max_iters {
                    let jobs = self.dirty_owners();
                    if jobs.is_empty() {
                        return Ok(handled);
                    }
                    for job in &jobs {
                        handler(job);
                        self.clear_dirty_one(
                            job.owner_id.as_id_str().as_ref(),
                            job.source_id.as_id_str().as_ref(),
                            job.generation,
                        );
                        handled += 1;
                    }
                    if iter + 1 == max_iters && !self.dirty_owners().is_empty() {
                        return Err(QuiescenceError::IterationCap {
                            iters: max_iters,
                            handled,
                        });
                    }
                }
                Ok(handled)
            }
            TraversalOrder::DepthFirst => {
                for iter in 0..max_iters {
                    // Re-snapshot each round and take the most recently
                    // dirtied owner (max job id) so a cascade child runs
                    // before older siblings still on the worklist.
                    let Some(job) = self.dirty_owners().into_iter().next_back() else {
                        return Ok(handled);
                    };
                    handler(&job);
                    self.clear_dirty_one(
                        job.owner_id.as_id_str().as_ref(),
                        job.source_id.as_id_str().as_ref(),
                        job.generation,
                    );
                    handled += 1;
                    if iter + 1 == max_iters && !self.dirty_owners().is_empty() {
                        return Err(QuiescenceError::IterationCap {
                            iters: max_iters,
                            handled,
                        });
                    }
                }
                Ok(handled)
            }
        }
    }

    /// Back-compat wave-drain. Thin alias for `sweep_to_quiescence`
    /// with `TraversalOrder::BreadthFirst`; preserves the historical
    /// `run_to_quiescence` name and semantics for existing callers.
    pub fn run_to_quiescence<F>(
        &self,
        max_iters: usize,
        handler: F,
    ) -> Result<usize, QuiescenceError>
    where
        F: FnMut(&DirtyOwner<I>),
    {
        self.sweep_to_quiescence(max_iters, TraversalOrder::BreadthFirst, handler)
    }

    /// Record (or replace) the continuation for `owner_id`. The
    /// `input_hex` blob is opaque to the generic layer; callers control
    /// the encoding.
    pub fn record_continuation(&self, cont: RuntimeContinuation<I>) {
        let owner_id = cont.owner_id.as_id_str();
        self.facts.delete_matching(
            RUNTIME_CONTINUATION,
            &[(CONT_OWNER_ID, owner_id.as_ref())],
        );
        let mut row = R::default();
        row.set(CONT_OWNER_ID, owner_id.as_ref());
        row.set(CONT_PIPE_HASH, cont.pipe_hash.to_string().as_str());
        row.set(CONT_INSTANCE_ID, cont.instance_id.to_string().as_str());
        row.set(CONT_DEPTH, cont.depth.to_string().as_str());
        row.set(CONT_INPUT_CURSOR, cont.input_hex.as_str());
        row.set(GENERATION, cont.generation.to_string().as_str());
        self.facts
            .insert_batch(RUNTIME_CONTINUATION, vec![Arc::new(row)]);
    }

    pub fn continuation_for_owner(&self, owner_id: &str) -> Option<RuntimeContinuation<I>> {
        let row = self
            .facts
            .read_where(RUNTIME_CONTINUATION, CONT_OWNER_ID, owner_id)
            .into_iter()
            .next()?;
        Some(RuntimeContinuation {
            owner_id: I::from_id_str(owner_id),
            pipe_hash: row.get(CONT_PIPE_HASH)?.parse().ok()?,
            instance_id: row.get(CONT_INSTANCE_ID)?.parse().ok()?,
            depth: row.get(CONT_DEPTH)?.parse::<u64>().ok()? as u32,
            input_hex: row.get(CONT_INPUT_CURSOR)?.to_string(),
            generation: row.get(GENERATION)?.parse().ok()?,
        })
    }

    /// Mark every owner subscribed to `source_id` dirty for it, and
    /// refresh the per-edge latest-value slot. Replaces the old
    /// append-event / enqueue-job pair: the wake now writes straight to
    /// the dirty worklist. `_event_id` is retained for call-site
    /// compatibility; the dirty row IS the work item now. Returns the
    /// woken owners.
    pub fn dispatch_wake(
        &self,
        source_id: &str,
        value_id: &str,
        _event_id: &str,
        subscribe_kind_id: &str,
        generation: u64,
    ) -> Vec<I> {
        let mut owners = BTreeSet::new();
        for edge in self.incoming_edges(subscribe_kind_id, source_id) {
            self.set_edge_value(
                edge.edge_id.as_id_str().as_ref(),
                edge.label_id.as_id_str().as_ref(),
                value_id,
                generation,
            );
            self.mark_dirty(edge.from_id.as_id_str().as_ref(), source_id, generation);
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
    ) -> VisibleDelta<I> {
        let old = self.supported_rows_for_owner(owner_id, table_label_id, support_kind_id);
        let new: BTreeSet<String> = row_ids.iter().cloned().collect();
        let mut inserted = Vec::new();
        let mut retracted = Vec::new();

        // Phase 5: support `mult` is the count of distinct support
        // edges pointing at a row (each `(owner, label, kind)` is one
        // path). `add` on assert = insert one edge; `dec` on retract =
        // delete one edge. The support-iff-mult invariant — a row is
        // visible IFF `mult > 0` — is enforced by the debug_asserts at
        // this teardown/insert seam. Ids stay opaque (the v3↔v4 wall:
        // only `[u8;32]`/primitive ids cross).
        for row_id in old.difference(&new) {
            let edge_id = support_edge_id(owner_id, row_id, table_label_id, support_kind_id);
            self.delete_edge(edge_id.as_str());
            let mult = self.support_mult(row_id, support_kind_id);
            debug_assert!(
                mult >= 0,
                "support-iff-mult: row {row_id:?} support mult went negative"
            );
            if mult == 0 {
                debug_assert!(
                    !self.row_has_support(row_id, support_kind_id),
                    "support-iff-mult: row {row_id:?} retracted while still supported"
                );
                retracted.push(I::from_id_str(row_id));
            }
        }

        for row_id in new.difference(&old) {
            let already_visible = self.support_mult(row_id, support_kind_id) > 0;
            self.insert_edge(RuntimeEdge {
                edge_id: I::from_id_str(&support_edge_id(
                    owner_id,
                    row_id,
                    table_label_id,
                    support_kind_id,
                )),
                kind_id: I::from_id_str(support_kind_id),
                from_id: I::from_id_str(owner_id),
                to_id: I::from_id_str(row_id),
                label_id: I::from_id_str(table_label_id),
                generation,
            });
            debug_assert!(
                self.support_mult(row_id, support_kind_id) > 0,
                "support-iff-mult: row {row_id:?} asserted but mult == 0"
            );
            if !already_visible {
                inserted.push(I::from_id_str(row_id));
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
            edge_id: I::from_id_str(edge_id),
            kind_id: I::from_id_str(active_kind_id),
            from_id: I::from_id_str(owner_id),
            to_id: I::from_id_str(child_id),
            label_id: I::from_id_str(label_id),
            generation,
        });
    }

    pub fn active_child(
        &self,
        owner_id: &str,
        label_id: &str,
        active_kind_id: &str,
    ) -> Option<I> {
        self.facts
            .rows_of(RUNTIME_EDGE)
            .into_iter()
            .find_map(|row| {
                if row.get(EDGE_KIND_ID) == Some(active_kind_id)
                    && row.get(FROM_ID) == Some(owner_id)
                    && row.get(LABEL_ID) == Some(label_id)
                {
                    return row.get(TO_ID).map(I::from_id_str);
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
            RUNTIME_DIRTY,
            &[DIRTY_OWNER_ID, DIRTY_SOURCE_ID, GENERATION],
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

    fn incoming_edges(&self, kind_id: &str, to_id: &str) -> Vec<RuntimeEdge<I>> {
        let mut edges: Vec<RuntimeEdge<I>> = self
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

    /// Phase 5: integer support multiplicity = count of distinct
    /// support edges pointing at `row_id` for `support_kind_id`. Each
    /// edge is one derivation path; the row is visible IFF this is
    /// `> 0` (support-iff-mult, asserted at the `replace_supports`
    /// seam).
    fn support_mult(&self, row_id: &str, support_kind_id: &str) -> i64 {
        self.facts
            .rows_of(RUNTIME_EDGE)
            .iter()
            .filter(|row| {
                row.get(EDGE_KIND_ID) == Some(support_kind_id)
                    && row.get(TO_ID) == Some(row_id)
            })
            .count() as i64
    }
}

fn edge_from_row<R: Row, I: NodeId>(row: Arc<R>) -> Option<RuntimeEdge<I>> {
    Some(RuntimeEdge {
        edge_id: I::from_id_str(row.get(EDGE_ID)?),
        kind_id: I::from_id_str(row.get(EDGE_KIND_ID)?),
        from_id: I::from_id_str(row.get(FROM_ID)?),
        to_id: I::from_id_str(row.get(TO_ID)?),
        label_id: I::from_id_str(row.get(LABEL_ID)?),
        generation: row.get(GENERATION)?.parse().ok()?,
    })
}

/// Deterministic work-item id for a dirty `(owner, source, generation)`
/// triple. Same triple => same id, so callers can diff "new work since
/// last snapshot" exactly as they diffed RUNTIME_JOB row ids before.
fn dirty_job_id(owner_id: &str, source_id: &str, generation: u64) -> String {
    let mut h = blake3::Hasher::new();
    for part in [owner_id, source_id] {
        h.update(&(part.len() as u64).to_be_bytes());
        h.update(part.as_bytes());
    }
    h.update(&generation.to_be_bytes());
    h.finalize().to_hex().to_string()
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
