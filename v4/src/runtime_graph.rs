use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex};

use effect_runtime::v2::{
    FactRuntimeGraph, FactStore, RenderCtx, RuntimeEdge as RuntimeEdgeRow,
    RuntimeNode as RuntimeNodeRow, RuntimePut,
};

use crate::cursor_codec;
use crate::store::SprfStore;
use crate::telemetry::{WriteStats, WriteStatsSnapshot};
use crate::{Cursor, StringId, FOCAL_TERM};

pub const RUNTIME_NODE: &str = "runtime_node";
pub const RUNTIME_EDGE: &str = "runtime_edge";
pub const RUNTIME_EVENT: &str = "runtime_event";
pub const RUNTIME_JOB: &str = "runtime_job";
pub const RUNTIME_CONTINUATION: &str = "runtime_continuation";

const NODE_URI_ID: &str = "node_uri_id";
const INPUT_KEY: &str = "input_key";
const GENERATION: &str = "generation";

const EDGE_URI_ID: &str = "edge_uri_id";
const EDGE_KIND_ID: &str = "edge_kind_id";
const FROM_URI_ID: &str = "from_uri_id";
const TO_URI_ID: &str = "to_uri_id";
const LABEL_ID: &str = "label_id";

const EVENT_URI_ID: &str = "event_uri_id";
const EVENT_SOURCE_URI_ID: &str = "source_uri_id";
const EVENT_VALUE_URI_ID: &str = "value_uri_id";
const CONSUMED: &str = "consumed";


const CONT_OWNER_URI_ID: &str = "owner_uri_id";
const CONT_PIPE_HASH: &str = "pipe_hash";
const CONT_INSTANCE_ID: &str = "instance_id";
const CONT_DEPTH: &str = "depth";
const CONT_INPUT_CURSOR: &str = "input_cursor";

const KIND_OWNER: &str = "owner";
const KIND_SOURCE: &str = "source";
const KIND_OUTPUT: &str = "output";
const KIND_ROW: &str = "row";
const KIND_SUBSCRIBE: &str = "subscribe";
const KIND_SUPPORT: &str = "support";
const KIND_MEMBER_OF: &str = "member_of";

#[derive(Clone)]
pub struct RuntimeGraph {
    pub store: Arc<SprfStore>,
    pub facts: Arc<dyn FactStore<Cursor>>,
    core: FactRuntimeGraph<Cursor>,
    compact_sources: Option<Arc<CompactSourceGraph>>,
    rule_memo: Arc<Mutex<RuleMemo>>,
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
            QuiescenceError::IterationCap { iters, handled } => {
                write!(
                    f,
                    "run_to_quiescence hit iteration cap (iters={iters}, handled={handled})"
                )
            }
        }
    }
}

impl std::error::Error for QuiescenceError {}

/// Canonical key for one positional rule-call argument. Literal carries
/// the interned StringId so equality is `u64==u64`. Unbound means the
/// caller left this position as `?` for the body/SQL to fill.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ArgKey {
    Literal(StringId),
    Unbound,
}

/// Memo from (rule-name, arg-tuple) to the OwnerNode declared on first
/// fire. Same input tuple => same OwnerNode (durable identity via the
/// underlying runtime-graph URI machinery).
#[derive(Default)]
pub struct RuleMemo {
    table: HashMap<(StringId, Vec<ArgKey>), OwnerNode>,
}

impl RuleMemo {
    pub fn new() -> Self {
        Self::default()
    }

    /// Lookup or insert. `build` runs only on miss; the closure is
    /// responsible for declaring the owner so the memo always wraps the
    /// graph-side identity.
    pub fn get_or_insert<F>(&mut self, rule: StringId, args: Vec<ArgKey>, build: F) -> OwnerNode
    where
        F: FnOnce() -> OwnerNode,
    {
        self.table
            .entry((rule, args))
            .or_insert_with(build)
            .clone()
    }

    pub fn get(&self, rule: StringId, args: &[ArgKey]) -> Option<OwnerNode> {
        self.table.get(&(rule, args.to_vec())).cloned()
    }

    pub fn len(&self) -> usize {
        self.table.len()
    }

    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OwnerNode {
    pub uri_id: StringId,
    pub uri: Arc<str>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceNode {
    pub uri_id: StringId,
    pub uri: Arc<str>,
}

pub struct SourceSubscriptionInput<'a> {
    pub ast_uri: String,
    pub input_key: String,
    pub source_uri: String,
    pub label: String,
    pub pipe_hash: u64,
    pub instance_id: u64,
    pub depth: u32,
    pub input: &'a Cursor,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OutputNode {
    pub uri_id: StringId,
    pub uri: Arc<str>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RowNode {
    pub uri_id: StringId,
    pub uri: Arc<str>,
    pub row_key: Arc<str>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SubscribeEdge {
    pub uri_id: StringId,
    pub uri: Arc<str>,
    pub label_id: StringId,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SupportEdge {
    pub uri_id: StringId,
    pub uri: Arc<str>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuntimeEvent {
    pub uri_id: StringId,
    pub source_uri_id: StringId,
    pub value_uri_id: StringId,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuntimeJob {
    pub uri_id: StringId,
    pub event_uri_id: StringId,
    pub owner_uri_id: StringId,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeContinuation {
    pub owner_uri_id: StringId,
    pub pipe_hash: u64,
    pub instance_id: u64,
    pub depth: u32,
    pub input: Cursor,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnerDescriptor {
    pub owner: OwnerNode,
    pub ast_uri: Arc<str>,
    pub input_key: Arc<str>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VisibleDelta {
    pub inserted: Vec<RowNode>,
    pub retracted: Vec<RowNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceWake {
    pub source_uri: Arc<str>,
    pub generation: u64,
}

impl SourceWake {
    pub fn dirty(source_uri: impl Into<Arc<str>>, generation: u64) -> Self {
        Self {
            source_uri: source_uri.into(),
            generation,
        }
    }
}

pub trait IntoSourceWakes {
    fn into_source_wakes(self, generation: u64) -> Vec<SourceWake>;
}

pub struct SprfSubscribe {
    pub owner: OwnerNode,
    pub label: Arc<str>,
    pub source: SourceNode,
    pub generation: u64,
}

impl SprfSubscribe {
    pub fn new(
        owner: OwnerNode,
        label: impl Into<Arc<str>>,
        source: SourceNode,
        generation: u64,
    ) -> Self {
        Self {
            owner,
            label: label.into(),
            source,
            generation,
        }
    }
}

impl RuntimePut for SprfSubscribe {
    type Output = SubscribeEdge;

    fn apply(self, ctx: &RenderCtx) -> Self::Output {
        let graph = ctx
            .runtime::<RuntimeGraph>()
            .expect("sprf runtime graph missing from RenderCtx");
        graph.subscribe(
            &self.owner,
            self.label.as_ref(),
            &self.source,
            self.generation,
        )
    }
}

pub struct SprfSupportRows {
    pub owner: OwnerNode,
    pub table: OutputNode,
    pub rows: Vec<Cursor>,
    pub generation: u64,
}

impl SprfSupportRows {
    pub fn new(owner: OwnerNode, table: OutputNode, rows: Vec<Cursor>, generation: u64) -> Self {
        Self {
            owner,
            table,
            rows,
            generation,
        }
    }
}

impl RuntimePut for SprfSupportRows {
    // mounted_query and other production callers drop the delta; the
    // direct `replace_supports` method keeps returning VisibleDelta for
    // smoke-test assertions until slice 6 removes it entirely.
    type Output = ();

    fn apply(self, ctx: &RenderCtx) -> Self::Output {
        let graph = ctx
            .runtime::<RuntimeGraph>()
            .expect("sprf runtime graph missing from RenderCtx");
        let _ = graph.replace_supports(&self.owner, &self.table, &self.rows, self.generation);
    }
}

impl RuntimeGraph {
    pub fn new(store: Arc<SprfStore>, facts: Arc<dyn FactStore<Cursor>>) -> Self {
        let core = FactRuntimeGraph::new(facts.clone());
        Self {
            store,
            facts,
            core,
            compact_sources: None,
            rule_memo: Arc::new(Mutex::new(RuleMemo::new())),
        }
    }

    pub fn new_with_compact_sources(
        store: Arc<SprfStore>,
        facts: Arc<dyn FactStore<Cursor>>,
        path: impl AsRef<Path>,
    ) -> Self {
        let core = FactRuntimeGraph::new(facts.clone());
        Self {
            store,
            facts,
            core,
            compact_sources: Some(Arc::new(CompactSourceGraph::open(path.as_ref()))),
            rule_memo: Arc::new(Mutex::new(RuleMemo::new())),
        }
    }

    /// Memoize the OwnerNode for a rule call by (rule_name, arg_keys).
    /// First call with the tuple declares an owner via the supplied
    /// `build` closure; later calls with the same tuple return the same
    /// OwnerNode. Callers pass a build that uses a deterministic
    /// `input_key` so durable identity matches the in-memory memo.
    pub fn rule_memo_owner<F>(&self, rule: StringId, args: Vec<ArgKey>, build: F) -> OwnerNode
    where
        F: FnOnce() -> OwnerNode,
    {
        self.rule_memo
            .lock()
            .unwrap()
            .get_or_insert(rule, args, build)
    }

    pub fn rule_memo_get(&self, rule: StringId, args: &[ArgKey]) -> Option<OwnerNode> {
        self.rule_memo.lock().unwrap().get(rule, args)
    }

    pub fn rule_memo_len(&self) -> usize {
        self.rule_memo.lock().unwrap().len()
    }

    pub fn stats(&self) -> Option<WriteStatsSnapshot> {
        self.compact_sources.as_ref().map(|compact| compact.stats())
    }

    pub fn declare_owner(
        &self,
        ast_uri: &str,
        parent_owner_uri: Option<&str>,
        input_key: &str,
        source_hash: &str,
        mode: &str,
        generation: u64,
    ) -> OwnerNode {
        let uri = owner_uri(ast_uri, parent_owner_uri, input_key, source_hash, mode);
        let uri_id = self.intern_uri(uri.as_ref());
        let node = OwnerNode { uri_id, uri };
        self.insert_node(
            node.uri_id,
            KIND_OWNER,
            ast_uri,
            parent_owner_uri,
            input_key,
            source_hash,
            mode,
            generation,
        );
        node
    }

    /// Declare a source node keyed by a fact-table name. Deterministic
    /// URI shape `sprf://source/table/<table>`. Used by the table-source
    /// watcher: subscribers attach to this node, callers of
    /// `notify_table_inserted` dispatch wakes against it.
    pub fn declare_table_source(&self, table: &str, generation: u64) -> SourceNode {
        let uri = table_source_uri(table);
        self.declare_source(&uri, generation)
    }

    /// Notify the graph that a fact table has new inserts. Emits one
    /// dirty wake on the canonical table source, enqueues jobs for every
    /// subscribed owner, and returns just the newly created jobs.
    pub fn notify_table_inserted(&self, table: &str, generation: u64) -> Vec<RuntimeJob> {
        let source = self.declare_table_source(table, generation);
        let before: BTreeSet<u64> = self.pending_jobs().into_iter().map(|j| j.uri_id.0).collect();
        let _ = self.dispatch_dirty(&source, generation);
        self.pending_jobs()
            .into_iter()
            .filter(|job| !before.contains(&job.uri_id.0))
            .collect()
    }

    /// Drive pending jobs to fixed point. Each iteration drains the
    /// current `pending_jobs()` snapshot, calls `handler(&job)` for each
    /// job, then `mark_job_done(&job)`. The handler may notify table
    /// inserts (which enqueues new jobs) during the loop; those jobs
    /// are picked up on the next iteration.
    ///
    /// Returns the total number of jobs run. Errors out with
    /// `QuiescenceError::IterationCap` once the iteration count exceeds
    /// `max_iters` — catches infinite cascades.
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
                self.mark_job_done(job);
                handled += 1;
            }
            // Sanity: don't loop forever if a job re-enqueues itself.
            if iter + 1 == max_iters && !self.pending_jobs().is_empty() {
                return Err(QuiescenceError::IterationCap {
                    iters: max_iters,
                    handled,
                });
            }
        }
        Ok(handled)
    }

    pub fn declare_source(&self, source_uri: &str, generation: u64) -> SourceNode {
        let uri = Arc::<str>::from(source_uri);
        let uri_id = self.intern_uri(uri.as_ref());
        self.insert_node(
            uri_id,
            KIND_SOURCE,
            "",
            None,
            "",
            &hash_text(source_uri),
            "",
            generation,
        );
        SourceNode { uri_id, uri }
    }

    pub fn declare_output_table(&self, table_uri: &str, generation: u64) -> OutputNode {
        let uri = Arc::<str>::from(table_uri);
        let uri_id = self.intern_uri(uri.as_ref());
        self.insert_node(
            uri_id,
            KIND_OUTPUT,
            "",
            None,
            "",
            &hash_text(table_uri),
            "",
            generation,
        );
        OutputNode { uri_id, uri }
    }

    pub fn declare_row(
        &self,
        table: &OutputNode,
        row_payload: &Cursor,
        generation: u64,
    ) -> RowNode {
        let row_key = row_key(table.uri.as_ref(), row_payload);
        let uri = row_uri(table.uri.as_ref(), row_key.as_ref());
        let uri_id = self.intern_uri(uri.as_ref());
        self.insert_node(
            uri_id,
            KIND_ROW,
            "",
            Some(table.uri.as_ref()),
            row_key.as_ref(),
            "",
            "",
            generation,
        );
        self.insert_edge(
            KIND_MEMBER_OF,
            uri_id,
            table.uri_id,
            table.uri.as_ref(),
            generation,
        );
        RowNode {
            uri_id,
            uri,
            row_key,
        }
    }

    pub fn subscribe(
        &self,
        owner: &OwnerNode,
        label: &str,
        source: &SourceNode,
        generation: u64,
    ) -> SubscribeEdge {
        let label_id = self.intern_uri(label);
        let edge = self.insert_edge(
            KIND_SUBSCRIBE,
            owner.uri_id,
            source.uri_id,
            label,
            generation,
        );
        SubscribeEdge {
            uri_id: edge.uri_id,
            uri: edge.uri,
            label_id,
        }
    }

    pub fn record_source_subscriptions(
        &self,
        subscriptions: &[SourceSubscriptionInput<'_>],
        generation: u64,
    ) {
        if subscriptions.is_empty() {
            return;
        }
        if let Some(compact) = &self.compact_sources {
            compact.record(subscriptions, generation);
            return;
        }
        self.facts.declare(
            RUNTIME_CONTINUATION,
            &[
                CONT_OWNER_URI_ID,
                CONT_PIPE_HASH,
                CONT_INSTANCE_ID,
                CONT_DEPTH,
                CONT_INPUT_CURSOR,
                GENERATION,
            ],
        );

        let owner_kind_id = self.intern_uri(KIND_OWNER).0.to_string();
        let source_kind_id = self.intern_uri(KIND_SOURCE).0.to_string();
        let subscribe_kind_id = self.intern_uri(KIND_SUBSCRIBE).0.to_string();
        let mode_id = self.intern_uri("ast").0.to_string();
        let empty_id = self.intern_uri("").0.to_string();
        let generation_text = generation.to_string();

        let mut nodes = Vec::with_capacity(subscriptions.len() * 2);
        let mut edges = Vec::with_capacity(subscriptions.len());
        let mut continuations = Vec::with_capacity(subscriptions.len());

        for subscription in subscriptions {
            let source_uri = Arc::<str>::from(subscription.source_uri.as_str());
            let source_uri_id = self.intern_uri(source_uri.as_ref());
            let source_hash = hash_text(subscription.source_uri.as_str());
            let owner_uri = owner_uri(
                subscription.ast_uri.as_str(),
                None,
                subscription.input_key.as_str(),
                subscription.source_uri.as_str(),
                "ast",
            );
            let owner_uri_id = self.intern_uri(owner_uri.as_ref());
            let ast_uri_id = self.intern_uri(subscription.ast_uri.as_str());
            let edge_uri_id = self.intern_uri(
                edge_uri(
                    KIND_SUBSCRIBE,
                    owner_uri_id,
                    source_uri_id,
                    self.intern_uri(subscription.label.as_str()),
                )
                .as_ref(),
            );

            let mut owner = Cursor::default();
            owner.set(NODE_URI_ID, owner_uri_id.0.to_string());
            owner.set("node_kind_id", owner_kind_id.as_str());
            owner.set("ast_uri_id", ast_uri_id.0.to_string());
            owner.set("parent_uri_id", "");
            owner.set(INPUT_KEY, subscription.input_key.as_str());
            owner.set("source_hash", subscription.source_uri.as_str());
            owner.set("mode_id", mode_id.as_str());
            owner.set(GENERATION, generation_text.as_str());
            nodes.push(Arc::new(owner));

            let mut source = Cursor::default();
            source.set(NODE_URI_ID, source_uri_id.0.to_string());
            source.set("node_kind_id", source_kind_id.as_str());
            source.set("ast_uri_id", empty_id.as_str());
            source.set("parent_uri_id", "");
            source.set(INPUT_KEY, "");
            source.set("source_hash", source_hash.as_ref());
            source.set("mode_id", empty_id.as_str());
            source.set(GENERATION, generation_text.as_str());
            nodes.push(Arc::new(source));

            let mut edge = Cursor::default();
            edge.set(EDGE_URI_ID, edge_uri_id.0.to_string());
            edge.set(EDGE_KIND_ID, subscribe_kind_id.as_str());
            edge.set(FROM_URI_ID, owner_uri_id.0.to_string());
            edge.set(TO_URI_ID, source_uri_id.0.to_string());
            edge.set(
                LABEL_ID,
                self.intern_uri(subscription.label.as_str()).0.to_string(),
            );
            edge.set(GENERATION, generation_text.as_str());
            edges.push(Arc::new(edge));

            let owner_id_text = owner_uri_id.0.to_string();
            self.facts.delete_matching(
                RUNTIME_CONTINUATION,
                &[(CONT_OWNER_URI_ID, owner_id_text.as_str())],
            );
            let mut continuation = Cursor::default();
            continuation.set(CONT_OWNER_URI_ID, owner_id_text);
            continuation.set(CONT_PIPE_HASH, subscription.pipe_hash.to_string());
            continuation.set(CONT_INSTANCE_ID, subscription.instance_id.to_string());
            continuation.set(CONT_DEPTH, subscription.depth.to_string());
            continuation.set(
                CONT_INPUT_CURSOR,
                hex(&cursor_codec::encode(subscription.input)),
            );
            continuation.set(GENERATION, generation_text.as_str());
            continuations.push(Arc::new(continuation));
        }

        self.facts.insert_batch(RUNTIME_NODE, nodes);
        self.facts.insert_batch(RUNTIME_EDGE, edges);
        self.facts.insert_batch(RUNTIME_CONTINUATION, continuations);
    }

    pub fn append_source_event(&self, source: &SourceNode, generation: u64) {
        // Event identity is (source, generation). Dropped the per-event
        // RuntimeValue join target in slice 2; RUNTIME_EVENT stays until
        // slice 6 collapses event/job into a dirty bit on the subscribe edge.
        let uri = runtime_uri(
            "event",
            &[source.uri.as_ref(), &generation.to_string()],
        );
        let uri_id = self.intern_uri(uri.as_ref());
        self.core.append_event(
            uri_id.0.to_string().as_str(),
            source.uri_id.0.to_string().as_str(),
            "",
            generation,
        );
    }

    pub fn enqueue_jobs_for_unconsumed_events(&self, generation: u64) -> Vec<RuntimeJob> {
        let mut jobs = BTreeMap::new();
        for event in self.unconsumed_events() {
            let Some(source_uri) = self.store.lookup_string(event.source_uri_id) else {
                continue;
            };
            let source = SourceNode {
                uri_id: event.source_uri_id,
                uri: source_uri,
            };
            for incoming in self.incoming_subscribe_edges(&source) {
                let job = self.insert_job(&event, &incoming.owner, generation);
                jobs.insert(job.uri_id.0, job);
            }
            if let Some(compact) = &self.compact_sources {
                for owner in compact.owners_for_source(source.uri_id) {
                    let job = self.insert_job(&event, &owner, generation);
                    jobs.insert(job.uri_id.0, job);
                }
            }
        }
        jobs.into_values().collect()
    }

    pub fn pending_jobs(&self) -> Vec<RuntimeJob> {
        // Read through effect_runtime's generic job table; convert opaque
        // string IDs to sprf StringId for callers.
        self.core
            .pending_jobs()
            .into_iter()
            .filter_map(|row| {
                Some(RuntimeJob {
                    uri_id: StringId(row.job_id.parse().ok()?),
                    event_uri_id: StringId(row.event_id.parse().ok()?),
                    owner_uri_id: StringId(row.owner_id.parse().ok()?),
                    generation: row.generation,
                })
            })
            .collect()
    }

    pub fn mark_job_done(&self, job: &RuntimeJob) {
        self.core.mark_job_done(job.uri_id.0.to_string().as_str());
    }

    pub fn owner_descriptor(&self, owner_uri_id: StringId) -> Option<OwnerDescriptor> {
        let owner_id = owner_uri_id.0.to_string();
        let Some(row) = self
            .facts
            .read_where(RUNTIME_NODE, NODE_URI_ID, owner_id.as_str())
            .into_iter()
            .next()
        else {
            return self
                .compact_sources
                .as_ref()
                .and_then(|compact| compact.owner_descriptor(owner_uri_id));
        };
        let owner_uri = self.store.lookup_string(owner_uri_id)?;
        let ast_id = row.get("ast_uri_id").and_then(parse_u64)?;
        let ast_uri = self.store.lookup_string(StringId(ast_id))?;
        let input_key = row
            .get(INPUT_KEY)
            .map(Arc::<str>::from)
            .unwrap_or_else(|| Arc::<str>::from(""));
        Some(OwnerDescriptor {
            owner: OwnerNode {
                uri_id: owner_uri_id,
                uri: owner_uri,
            },
            ast_uri,
            input_key,
        })
    }

    pub fn record_continuation(
        &self,
        owner: &OwnerNode,
        pipe_hash: u64,
        instance_id: u64,
        depth: u32,
        input: &Cursor,
        generation: u64,
    ) {
        self.facts.declare(
            RUNTIME_CONTINUATION,
            &[
                CONT_OWNER_URI_ID,
                CONT_PIPE_HASH,
                CONT_INSTANCE_ID,
                CONT_DEPTH,
                CONT_INPUT_CURSOR,
                GENERATION,
            ],
        );
        let owner_id = owner.uri_id.0.to_string();
        self.facts.delete_matching(
            RUNTIME_CONTINUATION,
            &[(CONT_OWNER_URI_ID, owner_id.as_str())],
        );
        let mut row = Cursor::default();
        row.set(CONT_OWNER_URI_ID, owner_id);
        row.set(CONT_PIPE_HASH, pipe_hash.to_string());
        row.set(CONT_INSTANCE_ID, instance_id.to_string());
        row.set(CONT_DEPTH, depth.to_string());
        row.set(CONT_INPUT_CURSOR, hex(&cursor_codec::encode(input)));
        row.set(GENERATION, generation.to_string());
        self.facts
            .insert_batch(RUNTIME_CONTINUATION, vec![Arc::new(row)]);
    }

    pub fn continuation_for_owner(&self, owner_uri_id: StringId) -> Option<RuntimeContinuation> {
        let owner_id = owner_uri_id.0.to_string();
        let Some(row) = self
            .facts
            .read_where(RUNTIME_CONTINUATION, CONT_OWNER_URI_ID, owner_id.as_str())
            .into_iter()
            .next()
        else {
            return self
                .compact_sources
                .as_ref()
                .and_then(|compact| compact.continuation_for_owner(owner_uri_id));
        };
        let input_hex = row.get(CONT_INPUT_CURSOR)?;
        let input = cursor_codec::decode(&unhex(input_hex)?).ok()?;
        Some(RuntimeContinuation {
            owner_uri_id,
            pipe_hash: row.get(CONT_PIPE_HASH).and_then(parse_u64)?,
            instance_id: row.get(CONT_INSTANCE_ID).and_then(parse_u64)?,
            depth: row.get(CONT_DEPTH).and_then(parse_u64)? as u32,
            input,
            generation: row.get(GENERATION).and_then(parse_u64)?,
        })
    }

    pub fn dispatch_dirty(&self, source: &SourceNode, generation: u64) -> Vec<OwnerNode> {
        self.dispatch_wake(source, generation)
    }

    pub fn dispatch_source_wake(&self, wake: SourceWake) -> Vec<OwnerNode> {
        let source = self.declare_source(wake.source_uri.as_ref(), wake.generation);
        self.dispatch_wake(&source, wake.generation)
    }

    pub fn dispatch_wake(&self, source: &SourceNode, generation: u64) -> Vec<OwnerNode> {
        self.append_source_event(source, generation);
        let mut owners = BTreeMap::new();
        for edge in self.incoming_subscribe_edges(source) {
            owners.insert(edge.owner.uri_id.0, edge.owner);
        }
        if let Some(compact) = &self.compact_sources {
            for owner in compact.owners_for_source(source.uri_id) {
                owners.insert(owner.uri_id.0, owner);
            }
        }
        self.enqueue_jobs_for_unconsumed_events(generation);
        owners.into_values().collect()
    }

    pub fn unconsumed_events(&self) -> Vec<RuntimeEvent> {
        let mut out: Vec<RuntimeEvent> = self
            .facts
            .rows_of(RUNTIME_EVENT)
            .into_iter()
            .filter(|row| row.get(CONSUMED) == Some("0"))
            .filter_map(|row| {
                // value_uri_id column is written as "" since slice 2;
                // tolerate the empty case until slice 6 drops the column.
                let value_uri_id = row
                    .get(EVENT_VALUE_URI_ID)
                    .and_then(parse_u64)
                    .unwrap_or(0);
                Some(RuntimeEvent {
                    uri_id: StringId(row.get(EVENT_URI_ID).and_then(parse_u64)?),
                    source_uri_id: StringId(row.get(EVENT_SOURCE_URI_ID).and_then(parse_u64)?),
                    value_uri_id: StringId(value_uri_id),
                    generation: row.get(GENERATION).and_then(parse_u64)?,
                })
            })
            .collect();
        out.sort();
        out
    }

    pub fn owners_for_unconsumed_events(&self) -> Vec<OwnerNode> {
        let mut owners = BTreeMap::new();
        for event in self.unconsumed_events() {
            let Some(source_uri) = self.store.lookup_string(event.source_uri_id) else {
                continue;
            };
            let source = SourceNode {
                uri_id: event.source_uri_id,
                uri: source_uri,
            };
            for incoming in self.incoming_subscribe_edges(&source) {
                owners.insert(incoming.owner.uri_id.0, incoming.owner);
            }
        }
        owners.into_values().collect()
    }

    pub fn mark_event_consumed(&self, event: &RuntimeEvent) {
        self.core
            .mark_event_consumed(event.uri_id.0.to_string().as_str());
    }

    pub fn replace_supports(
        &self,
        owner: &OwnerNode,
        table: &OutputNode,
        rows: &[Cursor],
        generation: u64,
    ) -> VisibleDelta {
        let old = self.supported_rows_for_owner(owner, table);
        let mut new = BTreeMap::new();
        for row in rows {
            let node = self.declare_row(table, row, generation);
            new.insert(node.uri_id.0, node);
        }

        let old_keys: BTreeSet<u64> = old.keys().copied().collect();
        let new_keys: BTreeSet<u64> = new.keys().copied().collect();
        let mut inserted = Vec::new();
        let mut retracted = Vec::new();

        for row_id in old_keys.difference(&new_keys) {
            self.delete_support(owner, table, *row_id);
            if !self.row_has_support(*row_id) {
                if let Some(row) = old.get(row_id) {
                    retracted.push(row.clone());
                }
            }
        }

        for row_id in new_keys.difference(&old_keys) {
            let already_visible = self.row_has_support(*row_id);
            let row = new.get(row_id).unwrap();
            self.insert_edge(
                KIND_SUPPORT,
                owner.uri_id,
                row.uri_id,
                table.uri.as_ref(),
                generation,
            );
            if !already_visible {
                inserted.push(row.clone());
            }
        }

        VisibleDelta {
            inserted,
            retracted,
        }
    }

    fn insert_node(
        &self,
        node_uri_id: StringId,
        kind: &str,
        ast_uri: &str,
        parent_uri: Option<&str>,
        input_key: &str,
        source_hash: &str,
        mode: &str,
        generation: u64,
    ) {
        self.core.insert_node(RuntimeNodeRow {
            node_id: node_uri_id.0.to_string(),
            kind_id: self.intern_uri(kind).0.to_string(),
            ast_id: Some(self.intern_uri(ast_uri).0.to_string()),
            parent_id: parent_uri.map(|uri| self.intern_uri(uri).0.to_string()),
            input_key: input_key.to_string(),
            source_key: source_hash.to_string(),
            mode_id: Some(self.intern_uri(mode).0.to_string()),
            generation,
        });
    }

    fn insert_edge(
        &self,
        kind: &str,
        from_uri_id: StringId,
        to_uri_id: StringId,
        label: &str,
        generation: u64,
    ) -> EdgeRecord {
        let label_id = self.intern_uri(label);
        let uri = edge_uri(kind, from_uri_id, to_uri_id, label_id);
        let uri_id = self.intern_uri(uri.as_ref());
        self.core.insert_edge(RuntimeEdgeRow {
            edge_id: uri_id.0.to_string(),
            kind_id: self.intern_uri(kind).0.to_string(),
            from_id: from_uri_id.0.to_string(),
            to_id: to_uri_id.0.to_string(),
            label_id: label_id.0.to_string(),
            generation,
        });
        EdgeRecord { uri_id, uri }
    }

    fn insert_job(&self, event: &RuntimeEvent, owner: &OwnerNode, generation: u64) -> RuntimeJob {
        let uri = runtime_uri(
            "job",
            &[&event.uri_id.0.to_string(), &owner.uri_id.0.to_string()],
        );
        let uri_id = self.intern_uri(uri.as_ref());
        let event_id_text = event.uri_id.0.to_string();
        let owner_id_text = owner.uri_id.0.to_string();
        self.core.insert_job(
            uri_id.0.to_string().as_str(),
            event_id_text.as_str(),
            owner_id_text.as_str(),
            generation,
        );
        RuntimeJob {
            uri_id,
            event_uri_id: event.uri_id,
            owner_uri_id: owner.uri_id,
            generation,
        }
    }

    fn supported_rows_for_owner(
        &self,
        owner: &OwnerNode,
        table: &OutputNode,
    ) -> BTreeMap<u64, RowNode> {
        let support_kind = self.intern_uri(KIND_SUPPORT).0.to_string();
        let owner_id = owner.uri_id.0.to_string();
        let table_label = table.uri_id.0.to_string();
        let mut out = BTreeMap::new();

        for edge in self.facts.rows_of(RUNTIME_EDGE) {
            if edge.get(EDGE_KIND_ID) != Some(support_kind.as_str())
                || edge.get(FROM_URI_ID) != Some(owner_id.as_str())
                || edge.get(LABEL_ID) != Some(table_label.as_str())
            {
                continue;
            }
            let Some(row_id) = edge.get(TO_URI_ID).and_then(parse_u64) else {
                continue;
            };
            if let Some(row) = self.row_node(row_id) {
                out.insert(row_id, row);
            }
        }

        out
    }

    fn row_node(&self, row_id: u64) -> Option<RowNode> {
        let row_id_str = row_id.to_string();
        let row = self
            .facts
            .read_where(RUNTIME_NODE, NODE_URI_ID, row_id_str.as_str())
            .into_iter()
            .next()?;
        let uri = self.store.lookup_string(StringId(row_id))?;
        let row_key = row
            .get(INPUT_KEY)
            .map(Arc::<str>::from)
            .unwrap_or_else(|| Arc::<str>::from(""));
        Some(RowNode {
            uri_id: StringId(row_id),
            uri,
            row_key,
        })
    }

    fn delete_support(&self, owner: &OwnerNode, table: &OutputNode, row_id: u64) {
        let edge_uri_id = self
            .intern_uri(
                edge_uri(KIND_SUPPORT, owner.uri_id, StringId(row_id), table.uri_id).as_ref(),
            )
            .0
            .to_string();
        self.facts
            .delete_matching(RUNTIME_EDGE, &[(EDGE_URI_ID, edge_uri_id.as_str())]);
    }

    fn row_has_support(&self, row_id: u64) -> bool {
        let support_kind = self.intern_uri(KIND_SUPPORT).0.to_string();
        let row_id = row_id.to_string();
        self.facts.rows_of(RUNTIME_EDGE).iter().any(|edge| {
            edge.get(EDGE_KIND_ID) == Some(support_kind.as_str())
                && edge.get(TO_URI_ID) == Some(row_id.as_str())
        })
    }

    fn incoming_subscribe_edges(&self, source: &SourceNode) -> Vec<IncomingSubscribe> {
        let subscribe_kind = self.intern_uri(KIND_SUBSCRIBE).0.to_string();
        let source_id = source.uri_id.0.to_string();
        let mut out = Vec::new();

        for row in self.facts.rows_of(RUNTIME_EDGE) {
            if row.get(EDGE_KIND_ID) != Some(subscribe_kind.as_str())
                || row.get(TO_URI_ID) != Some(source_id.as_str())
            {
                continue;
            }
            let Some(edge_id) = row.get(EDGE_URI_ID).and_then(parse_u64) else {
                continue;
            };
            let Some(owner_id) = row.get(FROM_URI_ID).and_then(parse_u64) else {
                continue;
            };
            let Some(owner_uri) = self.store.lookup_string(StringId(owner_id)) else {
                continue;
            };
            out.push(IncomingSubscribe {
                edge_uri_id: StringId(edge_id),
                owner: OwnerNode {
                    uri_id: StringId(owner_id),
                    uri: owner_uri,
                },
            });
        }

        out.sort_by_key(|incoming| incoming.edge_uri_id.0);
        out
    }

    fn intern_uri(&self, uri: &str) -> StringId {
        self.store.intern_string(uri)
    }
}

struct EdgeRecord {
    uri_id: StringId,
    uri: Arc<str>,
}

struct IncomingSubscribe {
    edge_uri_id: StringId,
    owner: OwnerNode,
}

struct CompactSourceGraph {
    conn: Mutex<rusqlite::Connection>,
    write_stats: WriteStats,
}

const COMPACT_FS_CURSOR_MAGIC: &[u8] = b"sprf:fs-cursor:v1\0";

fn encode_runtime_cursor(cursor: &Cursor) -> Vec<u8> {
    if is_plain_fs_cursor(cursor) {
        let mut bytes = Vec::with_capacity(COMPACT_FS_CURSOR_MAGIC.len() + cursor.value.len());
        bytes.extend_from_slice(COMPACT_FS_CURSOR_MAGIC);
        bytes.extend_from_slice(cursor.value.as_bytes());
        return bytes;
    }
    cursor_codec::encode(cursor)
}

fn decode_runtime_cursor(bytes: &[u8]) -> Result<Cursor, String> {
    if let Some(path) = bytes.strip_prefix(COMPACT_FS_CURSOR_MAGIC) {
        let path = std::str::from_utf8(path)
            .map_err(|err| format!("decode compact fs cursor utf8: {err}"))?;
        let mut cursor = Cursor::default();
        cursor.set_value(path);
        cursor.set("FS", path);
        return Ok(cursor);
    }
    cursor_codec::decode(bytes).map_err(str::to_string)
}

fn is_plain_fs_cursor(cursor: &Cursor) -> bool {
    if cursor.terms.len() != 2 {
        return false;
    }
    let mut has_focal = false;
    let mut has_fs = false;
    for term in &cursor.terms {
        if term.name.as_ref() == FOCAL_TERM && term.value.as_ref() == cursor.value.as_ref() {
            has_focal = true;
        } else if term.name.as_ref() == "FS" && term.value.as_ref() == cursor.value.as_ref() {
            has_fs = true;
        } else {
            return false;
        }
    }
    has_focal && has_fs
}

impl CompactSourceGraph {
    fn open(path: &Path) -> Self {
        let conn = rusqlite::Connection::open(path).expect("open compact runtime graph sqlite");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS runtime_source_subscription_compact (
                owner_id TEXT PRIMARY KEY,
                source_id TEXT NOT NULL,
                pipe_hash INTEGER NOT NULL,
                instance_id INTEGER NOT NULL,
                depth INTEGER NOT NULL,
                input_cursor BLOB NOT NULL,
                generation INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS runtime_source_subscription_compact_source
                ON runtime_source_subscription_compact(source_id);",
        )
        .expect("create compact source graph tables");
        Self {
            conn: Mutex::new(conn),
            write_stats: WriteStats::new("runtime_graph.compact_source_subscriptions"),
        }
    }

    fn record(&self, subscriptions: &[SourceSubscriptionInput<'_>], generation: u64) {
        let rows: Vec<_> = subscriptions
            .iter()
            .map(|subscription| {
                let owner_uri = owner_uri(
                    subscription.ast_uri.as_str(),
                    None,
                    subscription.input_key.as_str(),
                    subscription.source_uri.as_str(),
                    "ast",
                );
                (
                    StringId::of(owner_uri.as_ref()).0.to_string(),
                    StringId::of(subscription.source_uri.as_str()).0.to_string(),
                    subscription.pipe_hash.to_string(),
                    subscription.instance_id.to_string(),
                    subscription.depth.to_string(),
                    encode_runtime_cursor(subscription.input),
                    generation.to_string(),
                )
            })
            .collect();
        let byte_count = rows.iter().map(|row| row.5.len() as u64).sum();
        self.write_stats
            .record_timed(rows.len() as u64, byte_count, 1, || {
                let mut conn = self.conn.lock().unwrap();
                let tx = conn
                    .transaction()
                    .expect("compact source subscription transaction");
                {
                    let mut insert = tx
                        .prepare(
                            "INSERT OR REPLACE INTO runtime_source_subscription_compact
                            (owner_id, source_id, pipe_hash, instance_id, depth,
                             input_cursor, generation)
                            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        )
                        .expect("prepare compact source subscription insert");
                    for row in rows {
                        insert
                            .execute(rusqlite::params![
                                row.0, row.1, row.2, row.3, row.4, row.5, row.6,
                            ])
                            .expect("insert compact source subscription");
                    }
                }
                tx.commit()
                    .expect("commit compact source subscription transaction");
            });
    }

    fn stats(&self) -> WriteStatsSnapshot {
        self.write_stats.snapshot()
    }

    fn owners_for_source(&self, source_uri_id: StringId) -> Vec<OwnerNode> {
        let conn = self.conn.lock().unwrap();
        let source_id = source_uri_id.0.to_string();
        let mut stmt = conn
            .prepare(
                "SELECT owner_id
                 FROM runtime_source_subscription_compact
                 WHERE source_id = ?1
                 ORDER BY owner_id",
            )
            .expect("prepare compact source owner lookup");
        let rows = stmt
            .query_map([source_id], |row| {
                let owner_id: String = row.get(0)?;
                let owner_uri = format!("sprf://runtime/owner/{owner_id}");
                Ok(OwnerNode {
                    uri_id: StringId(owner_id.parse().unwrap_or(0)),
                    uri: Arc::from(owner_uri),
                })
            })
            .expect("query compact source owners");
        rows.filter_map(Result::ok).collect()
    }

    fn owner_descriptor(&self, owner_uri_id: StringId) -> Option<OwnerDescriptor> {
        let conn = self.conn.lock().unwrap();
        let owner_id = owner_uri_id.0.to_string();
        let mut stmt = conn
            .prepare(
                "SELECT owner_id
                 FROM runtime_source_subscription_compact
                 WHERE owner_id = ?1",
            )
            .ok()?;
        stmt.query_row([owner_id], |row| {
            let owner_id: String = row.get(0)?;
            let owner_uri = format!("sprf://runtime/owner/{owner_id}");
            Ok(OwnerDescriptor {
                owner: OwnerNode {
                    uri_id: owner_uri_id,
                    uri: Arc::from(owner_uri),
                },
                ast_uri: Arc::from(""),
                input_key: Arc::from(""),
            })
        })
        .ok()
    }

    fn continuation_for_owner(&self, owner_uri_id: StringId) -> Option<RuntimeContinuation> {
        let conn = self.conn.lock().unwrap();
        let owner_id = owner_uri_id.0.to_string();
        let mut stmt = conn
            .prepare(
                "SELECT pipe_hash, instance_id, depth, input_cursor, generation
                 FROM runtime_source_subscription_compact
                 WHERE owner_id = ?1",
            )
            .ok()?;
        stmt.query_row([owner_id], |row| {
            let pipe_hash: String = row.get(0)?;
            let instance_id: String = row.get(1)?;
            let depth: String = row.get(2)?;
            let input_cursor: Vec<u8> = row.get(3)?;
            let generation: String = row.get(4)?;
            let input = decode_runtime_cursor(&input_cursor).map_err(|err| {
                rusqlite::Error::FromSqlConversionFailure(
                    input_cursor.len(),
                    rusqlite::types::Type::Blob,
                    Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, err)),
                )
            })?;
            Ok(RuntimeContinuation {
                owner_uri_id,
                pipe_hash: pipe_hash.parse().unwrap_or(0),
                instance_id: instance_id.parse().unwrap_or(0),
                depth: depth.parse().unwrap_or(0),
                input,
                generation: generation.parse().unwrap_or(0),
            })
        })
        .ok()
    }
}

/// Canonical source URI for a user fact table. Pure fn of the table
/// name so callers can build the URI without holding a graph handle.
pub fn table_source_uri(table: &str) -> Arc<str> {
    Arc::<str>::from(format!("sprf://source/table/{table}"))
}

fn owner_uri(
    ast_uri: &str,
    parent_owner_uri: Option<&str>,
    input_key: &str,
    source_hash: &str,
    mode: &str,
) -> Arc<str> {
    runtime_uri(
        "owner",
        &[
            ast_uri,
            parent_owner_uri.unwrap_or(""),
            input_key,
            source_hash,
            mode,
        ],
    )
}

fn row_uri(table_uri: &str, row_key: &str) -> Arc<str> {
    runtime_uri("row", &[table_uri, row_key])
}

fn edge_uri(
    kind: &str,
    from_uri_id: StringId,
    to_uri_id: StringId,
    label_id: StringId,
) -> Arc<str> {
    runtime_uri(
        "edge",
        &[
            kind,
            &from_uri_id.0.to_string(),
            &to_uri_id.0.to_string(),
            &label_id.0.to_string(),
        ],
    )
}

fn row_key(table_uri: &str, row_payload: &Cursor) -> Arc<str> {
    let mut h = blake3::Hasher::new();
    h.update(table_uri.as_bytes());
    h.update(b"\0");
    h.update(&cursor_codec::encode(row_payload));
    Arc::<str>::from(h.finalize().to_hex().to_string())
}

fn runtime_uri(kind: &str, parts: &[&str]) -> Arc<str> {
    let mut h = blake3::Hasher::new();
    h.update(kind.as_bytes());
    for part in parts {
        h.update(&(part.len() as u64).to_be_bytes());
        h.update(part.as_bytes());
    }
    Arc::<str>::from(format!("sprf://runtime/{kind}/{}", h.finalize().to_hex()))
}

fn hash_text(text: &str) -> Arc<str> {
    hash_bytes(text.as_bytes())
}

fn hash_bytes(bytes: &[u8]) -> Arc<str> {
    Arc::<str>::from(blake3::hash(bytes).to_hex().to_string())
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() / 2);
    let bytes = text.as_bytes();
    for i in (0..bytes.len()).step_by(2) {
        let hi = hex_digit(bytes[i])?;
        let lo = hex_digit(bytes[i + 1])?;
        out.push((hi << 4) | lo);
    }
    Some(out)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn parse_u64(text: &str) -> Option<u64> {
    text.parse().ok()
}
