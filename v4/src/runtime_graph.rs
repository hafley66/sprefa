use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use effect_runtime::v2::{
    FactRuntimeGraph, FactStore, RenderCtx, RuntimeEdge as RuntimeEdgeRow,
    RuntimeNode as RuntimeNodeRow, RuntimePut, RuntimeValue as RuntimeValueRow,
    RuntimeValuePayload,
};

use crate::cursor_codec;
use crate::store::SprfStore;
use crate::{Cursor, StringId, WhereBytesId};

pub const RUNTIME_NODE: &str = "runtime_node";
pub const RUNTIME_EDGE: &str = "runtime_edge";
pub const RUNTIME_VALUE: &str = "runtime_value";
pub const RUNTIME_EDGE_VALUE: &str = "runtime_edge_value";
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

const VALUE_URI_ID: &str = "value_uri_id";
const VALUE_KEY: &str = "value_key";

const EVENT_URI_ID: &str = "event_uri_id";
const EVENT_SOURCE_URI_ID: &str = "source_uri_id";
const EVENT_VALUE_URI_ID: &str = "value_uri_id";
const CONSUMED: &str = "consumed";

const JOB_URI_ID: &str = "job_uri_id";
const JOB_EVENT_URI_ID: &str = "event_uri_id";
const JOB_OWNER_URI_ID: &str = "owner_uri_id";
const JOB_STATE: &str = "state";

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
const KIND_ACTIVE: &str = "active";
const KIND_DIRTY: &str = "dirty";
const KIND_CURSOR_BLOB: &str = "cursor_blob";
const KIND_WHERE_BYTES: &str = "where_bytes";
const STATE_READY: &str = "ready";
const JOB_PENDING: &str = "pending";
const JOB_DONE: &str = "done";

#[derive(Clone)]
pub struct RuntimeGraph {
    pub store: Arc<SprfStore>,
    pub facts: Arc<dyn FactStore<Cursor>>,
    core: FactRuntimeGraph<Cursor>,
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
pub struct ActiveEdge {
    pub uri_id: StringId,
    pub uri: Arc<str>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuntimeValue {
    pub uri_id: StringId,
    pub uri: Arc<str>,
    pub value_key: Arc<str>,
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
pub enum RuntimeValueInput {
    Dirty,
    CursorBlob(Cursor),
    WhereBytes(WhereBytesId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceWake {
    pub source_uri: Arc<str>,
    pub value: RuntimeValueInput,
    pub generation: u64,
}

impl SourceWake {
    pub fn dirty(source_uri: impl Into<Arc<str>>, generation: u64) -> Self {
        Self {
            source_uri: source_uri.into(),
            value: RuntimeValueInput::Dirty,
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
    type Output = VisibleDelta;

    fn apply(self, ctx: &RenderCtx) -> Self::Output {
        let graph = ctx
            .runtime::<RuntimeGraph>()
            .expect("sprf runtime graph missing from RenderCtx");
        graph.replace_supports(&self.owner, &self.table, &self.rows, self.generation)
    }
}

pub struct SprfActiveChild {
    pub owner: OwnerNode,
    pub label: Arc<str>,
    pub child: OwnerNode,
    pub generation: u64,
}

impl SprfActiveChild {
    pub fn new(
        owner: OwnerNode,
        label: impl Into<Arc<str>>,
        child: OwnerNode,
        generation: u64,
    ) -> Self {
        Self {
            owner,
            label: label.into(),
            child,
            generation,
        }
    }
}

impl RuntimePut for SprfActiveChild {
    type Output = ActiveEdge;

    fn apply(self, ctx: &RenderCtx) -> Self::Output {
        let graph = ctx
            .runtime::<RuntimeGraph>()
            .expect("sprf runtime graph missing from RenderCtx");
        graph.replace_active_child(
            &self.owner,
            self.label.as_ref(),
            &self.child,
            self.generation,
        )
    }
}

impl RuntimeGraph {
    pub fn new(store: Arc<SprfStore>, facts: Arc<dyn FactStore<Cursor>>) -> Self {
        let core = FactRuntimeGraph::new(facts.clone());
        Self { store, facts, core }
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

    pub fn replace_active_child(
        &self,
        owner: &OwnerNode,
        label: &str,
        child: &OwnerNode,
        generation: u64,
    ) -> ActiveEdge {
        let kind_id = self.intern_uri(KIND_ACTIVE).0.to_string();
        let from_id = owner.uri_id.0.to_string();
        let label_id = self.intern_uri(label).0.to_string();
        let old_children: Vec<OwnerNode> = self
            .facts
            .rows_of(RUNTIME_EDGE)
            .into_iter()
            .filter(|row| {
                row.get(EDGE_KIND_ID) == Some(kind_id.as_str())
                    && row.get(FROM_URI_ID) == Some(from_id.as_str())
                    && row.get(LABEL_ID) == Some(label_id.as_str())
            })
            .filter_map(|row| {
                let id = row.get(TO_URI_ID).and_then(parse_u64)?;
                if id == child.uri_id.0 {
                    return None;
                }
                let uri = self.store.lookup_string(StringId(id))?;
                Some(OwnerNode {
                    uri_id: StringId(id),
                    uri,
                })
            })
            .collect();
        for old_child in old_children {
            self.retract_owner_runtime(&old_child);
        }
        self.facts.delete_matching(
            RUNTIME_EDGE,
            &[
                (EDGE_KIND_ID, kind_id.as_str()),
                (FROM_URI_ID, from_id.as_str()),
                (LABEL_ID, label_id.as_str()),
            ],
        );
        let edge = self.insert_edge(KIND_ACTIVE, owner.uri_id, child.uri_id, label, generation);
        ActiveEdge {
            uri_id: edge.uri_id,
            uri: edge.uri,
        }
    }

    pub fn runtime_value_dirty(&self, source: &SourceNode, generation: u64) -> RuntimeValue {
        let uri = runtime_uri(
            "value/dirty",
            &[source.uri.as_ref(), &generation.to_string()],
        );
        self.insert_runtime_value(
            uri.as_ref(),
            KIND_DIRTY,
            "",
            RuntimeValuePayload::None,
            STATE_READY,
            generation,
        )
    }

    pub fn runtime_value_cursor_blob(
        &self,
        value_uri: &str,
        cursor: &Cursor,
        state: &str,
        generation: u64,
    ) -> RuntimeValue {
        let bytes = cursor_codec::encode(cursor);
        let value_key = hash_bytes(&bytes);
        self.insert_runtime_value(
            value_uri,
            KIND_CURSOR_BLOB,
            value_key.as_ref(),
            RuntimeValuePayload::InlineCell(hex(&bytes)),
            state,
            generation,
        )
    }

    pub fn runtime_value_where_bytes(
        &self,
        value_uri: &str,
        where_bytes: WhereBytesId,
        state: &str,
        generation: u64,
    ) -> RuntimeValue {
        let value_key = hash_text(&format!("where-bytes:{}", where_bytes.0));
        self.insert_runtime_value(
            value_uri,
            KIND_WHERE_BYTES,
            value_key.as_ref(),
            RuntimeValuePayload::Ref(where_bytes.0.to_string()),
            state,
            generation,
        )
    }

    pub fn record_edge_value(
        &self,
        edge: &SubscribeEdge,
        label: &str,
        value: &RuntimeValue,
        generation: u64,
    ) {
        let edge_id = edge.uri_id.0.to_string();
        let label_id = self.intern_uri(label).0.to_string();
        self.core.set_edge_value(
            edge_id.as_str(),
            label_id.as_str(),
            value.uri_id.0.to_string().as_str(),
            generation,
        );
    }

    pub fn append_source_event(&self, source: &SourceNode, value: &RuntimeValue, generation: u64) {
        let uri = runtime_uri(
            "event",
            &[
                source.uri.as_ref(),
                value.uri.as_ref(),
                value.value_key.as_ref(),
                &generation.to_string(),
            ],
        );
        let uri_id = self.intern_uri(uri.as_ref());
        self.core.append_event(
            uri_id.0.to_string().as_str(),
            source.uri_id.0.to_string().as_str(),
            value.uri_id.0.to_string().as_str(),
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
        }
        jobs.into_values().collect()
    }

    pub fn pending_jobs(&self) -> Vec<RuntimeJob> {
        let mut out: Vec<RuntimeJob> = self
            .facts
            .rows_of(RUNTIME_JOB)
            .into_iter()
            .filter(|row| row.get(JOB_STATE) == Some(JOB_PENDING))
            .filter_map(|row| {
                Some(RuntimeJob {
                    uri_id: StringId(row.get(JOB_URI_ID).and_then(parse_u64)?),
                    event_uri_id: StringId(row.get(JOB_EVENT_URI_ID).and_then(parse_u64)?),
                    owner_uri_id: StringId(row.get(JOB_OWNER_URI_ID).and_then(parse_u64)?),
                    generation: row.get(GENERATION).and_then(parse_u64)?,
                })
            })
            .collect();
        out.sort();
        out
    }

    pub fn mark_job_done(&self, job: &RuntimeJob) {
        let job_id = job.uri_id.0.to_string();
        self.facts
            .delete_matching(RUNTIME_JOB, &[(JOB_URI_ID, job_id.as_str())]);
        self.insert_job_row(job, JOB_DONE);

        let event_id = job.event_uri_id.0.to_string();
        let has_pending = self
            .facts
            .read_where(RUNTIME_JOB, JOB_EVENT_URI_ID, event_id.as_str())
            .iter()
            .any(|row| row.get(JOB_STATE) == Some(JOB_PENDING));
        if !has_pending {
            self.core.mark_event_consumed(event_id.as_str());
        }
    }

    pub fn owner_descriptor(&self, owner_uri_id: StringId) -> Option<OwnerDescriptor> {
        let owner_id = owner_uri_id.0.to_string();
        let row = self
            .facts
            .read_where(RUNTIME_NODE, NODE_URI_ID, owner_id.as_str())
            .into_iter()
            .next()?;
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
        let row = self
            .facts
            .read_where(RUNTIME_CONTINUATION, CONT_OWNER_URI_ID, owner_id.as_str())
            .into_iter()
            .next()?;
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
        let value = self.runtime_value_dirty(source, generation);
        self.dispatch_wake(source, &value, generation)
    }

    pub fn dispatch_source_wake(&self, wake: SourceWake) -> Vec<OwnerNode> {
        let source = self.declare_source(wake.source_uri.as_ref(), wake.generation);
        match wake.value {
            RuntimeValueInput::Dirty => self.dispatch_dirty(&source, wake.generation),
            RuntimeValueInput::CursorBlob(cursor) => {
                let cursor_hash = hash_bytes(&cursor_codec::encode(&cursor));
                let value_uri =
                    runtime_uri("value/cursor", &[source.uri.as_ref(), cursor_hash.as_ref()]);
                let value = self.runtime_value_cursor_blob(
                    value_uri.as_ref(),
                    &cursor,
                    STATE_READY,
                    wake.generation,
                );
                self.dispatch_wake(&source, &value, wake.generation)
            }
            RuntimeValueInput::WhereBytes(where_bytes) => {
                let value_uri = runtime_uri(
                    "value/where-bytes",
                    &[source.uri.as_ref(), &where_bytes.0.to_string()],
                );
                let value = self.runtime_value_where_bytes(
                    value_uri.as_ref(),
                    where_bytes,
                    STATE_READY,
                    wake.generation,
                );
                self.dispatch_wake(&source, &value, wake.generation)
            }
        }
    }

    pub fn dispatch_wake(
        &self,
        source: &SourceNode,
        value: &RuntimeValue,
        generation: u64,
    ) -> Vec<OwnerNode> {
        self.append_source_event(source, value, generation);
        let mut owners = BTreeMap::new();
        for edge in self.incoming_subscribe_edges(source) {
            self.record_edge_value(&edge.edge, edge.label.as_ref(), value, generation);
            owners.insert(edge.owner.uri_id.0, edge.owner);
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
                Some(RuntimeEvent {
                    uri_id: StringId(row.get(EVENT_URI_ID).and_then(parse_u64)?),
                    source_uri_id: StringId(row.get(EVENT_SOURCE_URI_ID).and_then(parse_u64)?),
                    value_uri_id: StringId(row.get(EVENT_VALUE_URI_ID).and_then(parse_u64)?),
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

    pub fn edge_values(&self, edges: &[SubscribeEdge]) -> Vec<Option<RuntimeValue>> {
        edges
            .iter()
            .map(|edge| {
                let edge_id = edge.uri_id.0.to_string();
                let row = self
                    .facts
                    .read_where(RUNTIME_EDGE_VALUE, EDGE_URI_ID, edge_id.as_str())
                    .into_iter()
                    .next()?;
                let value_id = row.get(VALUE_URI_ID).and_then(parse_u64)?;
                self.runtime_value_by_id(StringId(value_id))
            })
            .collect()
    }

    pub fn active_child(&self, owner: &OwnerNode, label: &str) -> Option<OwnerNode> {
        let kind_id = self.intern_uri(KIND_ACTIVE).0.to_string();
        let owner_id = owner.uri_id.0.to_string();
        let label_id = self.intern_uri(label).0.to_string();
        let row = self.facts.rows_of(RUNTIME_EDGE).into_iter().find(|row| {
            row.get(EDGE_KIND_ID) == Some(kind_id.as_str())
                && row.get(FROM_URI_ID) == Some(owner_id.as_str())
                && row.get(LABEL_ID) == Some(label_id.as_str())
        })?;
        let child_id = row.get(TO_URI_ID).and_then(parse_u64)?;
        let uri = self.store.lookup_string(StringId(child_id))?;
        Some(OwnerNode {
            uri_id: StringId(child_id),
            uri,
        })
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

    fn insert_runtime_value(
        &self,
        value_uri: &str,
        value_kind: &str,
        value_key: &str,
        payload: RuntimeValuePayload,
        state: &str,
        generation: u64,
    ) -> RuntimeValue {
        let uri = Arc::<str>::from(value_uri);
        let uri_id = self.intern_uri(uri.as_ref());
        let key = if value_key.is_empty() {
            Arc::<str>::from(hash_text(value_uri))
        } else {
            Arc::<str>::from(value_key)
        };

        self.core.insert_value(RuntimeValueRow {
            value_id: uri_id.0.to_string(),
            value_key: key.to_string(),
            kind_id: self.intern_uri(value_kind).0.to_string(),
            payload,
            state_id: self.intern_uri(state).0.to_string(),
            generation,
        });

        RuntimeValue {
            uri_id,
            uri,
            value_key: key,
        }
    }

    fn insert_job(&self, event: &RuntimeEvent, owner: &OwnerNode, generation: u64) -> RuntimeJob {
        let uri = runtime_uri(
            "job",
            &[&event.uri_id.0.to_string(), &owner.uri_id.0.to_string()],
        );
        let job = RuntimeJob {
            uri_id: self.intern_uri(uri.as_ref()),
            event_uri_id: event.uri_id,
            owner_uri_id: owner.uri_id,
            generation,
        };
        let job_id = job.uri_id.0.to_string();
        let existing = self
            .facts
            .read_where(RUNTIME_JOB, JOB_URI_ID, job_id.as_str());
        if existing.is_empty() {
            self.insert_job_row(&job, JOB_PENDING);
        }
        job
    }

    fn insert_job_row(&self, job: &RuntimeJob, state: &str) {
        self.facts.declare(
            RUNTIME_JOB,
            &[
                JOB_URI_ID,
                JOB_EVENT_URI_ID,
                JOB_OWNER_URI_ID,
                JOB_STATE,
                GENERATION,
            ],
        );
        let mut row = Cursor::default();
        row.set(JOB_URI_ID, job.uri_id.0.to_string());
        row.set(JOB_EVENT_URI_ID, job.event_uri_id.0.to_string());
        row.set(JOB_OWNER_URI_ID, job.owner_uri_id.0.to_string());
        row.set(JOB_STATE, state);
        row.set(GENERATION, job.generation.to_string());
        self.facts.insert_batch(RUNTIME_JOB, vec![Arc::new(row)]);
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
            let Some(label_id) = row.get(LABEL_ID).and_then(parse_u64) else {
                continue;
            };
            let Some(edge_uri) = self.store.lookup_string(StringId(edge_id)) else {
                continue;
            };
            let Some(owner_uri) = self.store.lookup_string(StringId(owner_id)) else {
                continue;
            };
            let Some(label) = self.store.lookup_string(StringId(label_id)) else {
                continue;
            };
            out.push(IncomingSubscribe {
                edge: SubscribeEdge {
                    uri_id: StringId(edge_id),
                    uri: edge_uri,
                    label_id: StringId(label_id),
                },
                owner: OwnerNode {
                    uri_id: StringId(owner_id),
                    uri: owner_uri,
                },
                label,
            });
        }

        out.sort_by_key(|incoming| incoming.edge.uri_id.0);
        out
    }

    fn intern_uri(&self, uri: &str) -> StringId {
        let id = self.store.intern_string(uri);
        self.store.flush();
        id
    }

    fn runtime_value_by_id(&self, value_id: StringId) -> Option<RuntimeValue> {
        let value_id_text = value_id.0.to_string();
        let row = self
            .facts
            .read_where(RUNTIME_VALUE, VALUE_URI_ID, value_id_text.as_str())
            .into_iter()
            .next()?;
        let value_key = row
            .get(VALUE_KEY)
            .map(Arc::<str>::from)
            .unwrap_or_else(|| Arc::<str>::from(""));
        let uri = self.store.lookup_string(value_id)?;
        Some(RuntimeValue {
            uri_id: value_id,
            uri,
            value_key,
        })
    }

    fn retract_owner_runtime(&self, owner: &OwnerNode) {
        let owner_id = owner.uri_id.0.to_string();
        let support_kind = self.intern_uri(KIND_SUPPORT).0.to_string();
        let subscribe_kind = self.intern_uri(KIND_SUBSCRIBE).0.to_string();
        let active_kind = self.intern_uri(KIND_ACTIVE).0.to_string();
        let outgoing: Vec<Arc<Cursor>> = self
            .facts
            .rows_of(RUNTIME_EDGE)
            .into_iter()
            .filter(|row| row.get(FROM_URI_ID) == Some(owner_id.as_str()))
            .collect();

        for edge in outgoing
            .iter()
            .filter(|row| row.get(EDGE_KIND_ID) == Some(active_kind.as_str()))
        {
            let Some(child_id) = edge.get(TO_URI_ID).and_then(parse_u64) else {
                continue;
            };
            let Some(child_uri) = self.store.lookup_string(StringId(child_id)) else {
                continue;
            };
            self.retract_owner_runtime(&OwnerNode {
                uri_id: StringId(child_id),
                uri: child_uri,
            });
        }

        for edge in outgoing {
            let Some(edge_id) = edge.get(EDGE_URI_ID).map(str::to_string) else {
                continue;
            };
            if edge.get(EDGE_KIND_ID) == Some(subscribe_kind.as_str()) {
                self.facts
                    .delete_matching(RUNTIME_EDGE_VALUE, &[(EDGE_URI_ID, edge_id.as_str())]);
            }
            if edge.get(EDGE_KIND_ID) == Some(support_kind.as_str()) {
                self.facts
                    .delete_matching(RUNTIME_EDGE, &[(EDGE_URI_ID, edge_id.as_str())]);
                continue;
            }
            self.facts
                .delete_matching(RUNTIME_EDGE, &[(EDGE_URI_ID, edge_id.as_str())]);
        }
    }
}

struct EdgeRecord {
    uri_id: StringId,
    uri: Arc<str>,
}

struct IncomingSubscribe {
    edge: SubscribeEdge,
    owner: OwnerNode,
    label: Arc<str>,
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
