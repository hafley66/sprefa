//! Durable query mount helpers.
//!
//! First slice: persist the output set from a batch-local SQL relation op
//! through the existing `FactStore`.

use std::sync::Arc;

use effect_runtime::v2::{FactStore, Next, Node, RenderCtx};

use crate::cursor_codec;
use crate::runtime_graph::{RuntimeGraph, SprfSubscribe, SprfSupportRows};
use crate::Cursor;

pub const OUTPUT_TABLE: &str = "mounted_query_output";
pub const CURSOR_TABLE: &str = "mounted_query_cursor";
pub const MOUNT_TABLE: &str = "mounted_query_mount";
pub const INPUT_TABLE: &str = "mounted_query_input";
pub const DEP_TABLE: &str = "mounted_query_dep";
pub const SUPPORT_TABLE: &str = "mounted_query_support";
pub const SUPPORT_CURSOR_ID: &str = "__support_cursor_id";
const MAX_RUNTIME_SQL_SUPPORT_ROWS: usize = 10_000;

const MOUNT_ID: &str = "mount_id";
const INPUT_KEY: &str = "input_key";
const GENERATION: &str = "generation";
const CURSOR_ID: &str = "cursor_id";
const CURSOR_BLOB: &str = "cursor_blob";
const CURSOR_IDX: &str = "cursor_idx";
const SQL: &str = "sql";
const DEP_TABLE_NAME: &str = "dep_table";
const SUPPORT_TABLE_NAME: &str = "support_table";
const SUPPORT_ROW_ID: &str = "support_row_id";
/// Phase 5: integer support multiplicity. A sink row is present IFF
/// `sum(SUPPORT_MULT) > 0` over all support rows for its
/// `(SUPPORT_TABLE_NAME, SUPPORT_ROW_ID)`. One physical support row per
/// `(SUPPORT_CURSOR_ID, SUPPORT_TABLE_NAME, SUPPORT_ROW_ID)` triple
/// carries that triple's current count; `add` increments, the DRed
/// teardown decrements.
const SUPPORT_MULT: &str = "support_mult";

/// Phase 5 SUPPORT schema. Wide for a future Counting upgrade
/// (Decision 2); DRed only uses the integer `SUPPORT_MULT` here.
const SUPPORT_COLS: &[&str] = &[
    SUPPORT_CURSOR_ID,
    SUPPORT_TABLE_NAME,
    SUPPORT_ROW_ID,
    SUPPORT_MULT,
];

pub trait MountedQueryStorage {
    fn record_sql_outputs(
        &self,
        sql: &str,
        generation: u64,
        batch: &[&Cursor],
        dep_tables: &[String],
        nodes: &[Node<Cursor>],
    ) -> Vec<Node<Cursor>>;
}

pub fn output_count(nodes: &[Node<Cursor>]) -> usize {
    nodes.iter().map(output_node_count).sum()
}

pub fn should_persist_mounted_outputs(count: usize) -> bool {
    count <= MAX_RUNTIME_SQL_SUPPORT_ROWS
}

pub struct FactMountedQueryStorage {
    store: Arc<dyn FactStore<Cursor>>,
}

struct PersistedCursor {
    cursor_id: String,
    blob_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MountedSqlSnapshot {
    pub sql: String,
    pub dep_tables: Vec<String>,
    pub inputs: Vec<Cursor>,
}

impl FactMountedQueryStorage {
    pub fn new(store: Arc<dyn FactStore<Cursor>>) -> Self {
        Self { store }
    }

    fn declare_tables(&self) {
        self.store.declare(CURSOR_TABLE, &[CURSOR_ID, CURSOR_BLOB]);
        self.store
            .declare(OUTPUT_TABLE, &[MOUNT_ID, INPUT_KEY, GENERATION, CURSOR_ID]);
        self.store
            .declare(MOUNT_TABLE, &[MOUNT_ID, INPUT_KEY, GENERATION, SQL]);
        self.store
            .declare(INPUT_TABLE, &[MOUNT_ID, INPUT_KEY, CURSOR_IDX, CURSOR_BLOB]);
        self.store
            .declare(DEP_TABLE, &[MOUNT_ID, INPUT_KEY, DEP_TABLE_NAME]);
        self.store.declare(SUPPORT_TABLE, SUPPORT_COLS);
    }

    fn existing_cursor_ids(
        &self,
        mount_id: &str,
        input_key: &str,
    ) -> std::collections::HashSet<String> {
        self.store
            .rows_of(OUTPUT_TABLE)
            .into_iter()
            .filter(|row| {
                row.get(MOUNT_ID) == Some(mount_id) && row.get(INPUT_KEY) == Some(input_key)
            })
            .filter_map(|row| row.get(CURSOR_ID).map(|s| s.to_string()))
            .collect()
    }

    fn intern_output_cursors(&self, outputs: &[PersistedCursor]) {
        let rows: Vec<Arc<Cursor>> = outputs
            .iter()
            .map(|persisted| {
                let mut row = Cursor::default();
                row.set(CURSOR_ID, persisted.cursor_id.clone());
                row.set(CURSOR_BLOB, persisted.blob_hex.clone());
                Arc::new(row)
            })
            .collect();
        self.store.insert_batch(CURSOR_TABLE, rows);
    }

    fn record_mount(
        &self,
        sql: &str,
        generation: u64,
        mount_id: &str,
        input_key: &str,
        batch: &[&Cursor],
        dep_tables: &[String],
    ) {
        self.store
            .delete_matching(MOUNT_TABLE, &[(MOUNT_ID, mount_id), (INPUT_KEY, input_key)]);
        self.store
            .delete_matching(INPUT_TABLE, &[(MOUNT_ID, mount_id), (INPUT_KEY, input_key)]);
        self.store
            .delete_matching(DEP_TABLE, &[(MOUNT_ID, mount_id), (INPUT_KEY, input_key)]);

        let mut mount = Cursor::default();
        mount.set(MOUNT_ID, mount_id);
        mount.set(INPUT_KEY, input_key);
        mount.set(GENERATION, generation.to_string());
        mount.set(SQL, sql);
        self.store.insert_batch(MOUNT_TABLE, vec![Arc::new(mount)]);

        let input_rows: Vec<Arc<Cursor>> = batch
            .iter()
            .enumerate()
            .map(|(idx, cursor)| {
                let mut row = Cursor::default();
                row.set(MOUNT_ID, mount_id);
                row.set(INPUT_KEY, input_key);
                row.set(CURSOR_IDX, idx.to_string());
                row.set(CURSOR_BLOB, hex(&cursor_codec::encode(cursor)));
                Arc::new(row)
            })
            .collect();
        self.store.insert_batch(INPUT_TABLE, input_rows);

        if dep_tables.is_empty() {
            return;
        }

        let rows: Vec<Arc<Cursor>> = dep_tables
            .iter()
            .map(|dep_table| {
                let mut row = Cursor::default();
                row.set(MOUNT_ID, mount_id);
                row.set(INPUT_KEY, input_key);
                row.set(DEP_TABLE_NAME, dep_table.as_str());
                Arc::new(row)
            })
            .collect();
        self.store.insert_batch(DEP_TABLE, rows);
    }
}

impl MountedQueryStorage for FactMountedQueryStorage {
    fn record_sql_outputs(
        &self,
        sql: &str,
        generation: u64,
        batch: &[&Cursor],
        dep_tables: &[String],
        nodes: &[Node<Cursor>],
    ) -> Vec<Node<Cursor>> {
        let count = output_count(nodes);
        if !should_persist_mounted_outputs(count) {
            tracing::debug!(
                target: "sprefa::runtime_graph",
                sql,
                rows = count,
                limit = MAX_RUNTIME_SQL_SUPPORT_ROWS,
                "skipping large mounted SQL output persistence"
            );
            return nodes.to_vec();
        }
        let outputs = output_cursors(nodes);
        let persisted = persist_output_cursors(outputs);
        let stamped_nodes = stamp_support_nodes(nodes);
        self.declare_tables();

        // `mount_id` folds the per-site tag carried in the SQL (a
        // leading `-- sprf-site:N` comment, see
        // `sql::with_query_site_tag`), so two syntactically-identical
        // `r?()` query occurrences get distinct mount lineages and no
        // longer collide on one `(mount_id, input_key)` mount. The
        // "added since existing" diff stays correct per-site: a genuine
        // same-site reactive re-run still suppresses already-emitted
        // rows; a second independent occurrence is its own mount and
        // yields the full materialized set.
        let mount_id = mount_id_for_sql(sql);
        let input_key = input_key_for_batch(batch);
        self.record_mount(sql, generation, &mount_id, &input_key, batch, dep_tables);
        let existing_cursor_ids = self.existing_cursor_ids(&mount_id, &input_key);
        let current_cursor_ids: std::collections::HashSet<String> =
            persisted.iter().map(|p| p.cursor_id.clone()).collect();
        let removed_cursor_ids: Vec<String> = existing_cursor_ids
            .difference(&current_cursor_ids)
            .cloned()
            .collect();
        cascade_retract(self.store.as_ref(), &removed_cursor_ids);
        self.store.delete_matching(
            OUTPUT_TABLE,
            &[
                (MOUNT_ID, mount_id.as_str()),
                (INPUT_KEY, input_key.as_str()),
            ],
        );

        if persisted.is_empty() {
            return nodes_added_since(&stamped_nodes, &existing_cursor_ids);
        }

        self.intern_output_cursors(&persisted);

        let generation = generation.to_string();
        let mut rows = Vec::with_capacity(persisted.len());

        for persisted in &persisted {
            let mut row = Cursor::default();
            row.set(MOUNT_ID, mount_id.clone());
            row.set(INPUT_KEY, input_key.clone());
            row.set(GENERATION, generation.clone());
            row.set(CURSOR_ID, persisted.cursor_id.clone());
            rows.push(Arc::new(row));
        }

        self.store.insert_batch(OUTPUT_TABLE, rows);
        nodes_added_since(&stamped_nodes, &existing_cursor_ids)
    }
}

pub fn record_sql_outputs(
    store: &Arc<dyn FactStore<Cursor>>,
    sql: &str,
    generation: u64,
    batch: &[&Cursor],
    dep_tables: &[String],
    nodes: &[Node<Cursor>],
) -> Vec<Node<Cursor>> {
    FactMountedQueryStorage::new(store.clone())
        .record_sql_outputs(sql, generation, batch, dep_tables, nodes)
}

pub fn dirty_tables_for_sql_outputs(
    store: &dyn FactStore<Cursor>,
    sql: &str,
    batch: &[&Cursor],
    nodes: &[Node<Cursor>],
) -> Vec<String> {
    let mount_id = mount_id_for_sql(sql);
    let input_key = input_key_for_batch(batch);
    let existing_cursor_ids: std::collections::HashSet<String> = store
        .rows_of(OUTPUT_TABLE)
        .into_iter()
        .filter(|row| {
            row.get(MOUNT_ID) == Some(mount_id.as_str())
                && row.get(INPUT_KEY) == Some(input_key.as_str())
        })
        .filter_map(|row| row.get(CURSOR_ID).map(str::to_string))
        .collect();
    let current_cursor_ids: std::collections::HashSet<String> =
        persist_output_cursors(output_cursors(nodes))
            .into_iter()
            .map(|p| p.cursor_id)
            .collect();
    let removed_cursor_ids: std::collections::HashSet<&str> = existing_cursor_ids
        .difference(&current_cursor_ids)
        .map(|id| id.as_str())
        .collect();
    if removed_cursor_ids.is_empty() {
        return Vec::new();
    }

    let support_rows = store.rows_of(SUPPORT_TABLE);
    let mut dirty = Vec::new();
    for row in &support_rows {
        let Some(support_id) = row.get(SUPPORT_CURSOR_ID) else {
            continue;
        };
        if !removed_cursor_ids.contains(support_id) {
            continue;
        }
        let Some(table) = row.get(SUPPORT_TABLE_NAME) else {
            continue;
        };
        let Some(row_id) = row.get(SUPPORT_ROW_ID) else {
            continue;
        };
        let still_supported = support_rows.iter().any(|other| {
            other
                .get(SUPPORT_CURSOR_ID)
                .map(|id| !removed_cursor_ids.contains(id))
                .unwrap_or(false)
                && other.get(SUPPORT_TABLE_NAME) == Some(table)
                && other.get(SUPPORT_ROW_ID) == Some(row_id)
        });
        if !still_supported {
            dirty.push(table.to_string());
        }
    }
    dirty.sort();
    dirty.dedup();
    dirty
}

pub fn load_mounted_sql_snapshot(
    store: &dyn FactStore<Cursor>,
    mount_id: &str,
    input_key: &str,
) -> Option<MountedSqlSnapshot> {
    let mount = store
        .read_where(MOUNT_TABLE, MOUNT_ID, mount_id)
        .into_iter()
        .find(|row| row.get(INPUT_KEY) == Some(input_key))?;
    let sql = mount.get(SQL)?.to_string();

    let mut dep_tables: Vec<String> = store
        .read_where(DEP_TABLE, MOUNT_ID, mount_id)
        .into_iter()
        .filter(|row| row.get(INPUT_KEY) == Some(input_key))
        .filter_map(|row| row.get(DEP_TABLE_NAME).map(str::to_string))
        .collect();
    dep_tables.sort();
    dep_tables.dedup();

    let mut input_rows: Vec<(usize, Cursor)> = store
        .read_where(INPUT_TABLE, MOUNT_ID, mount_id)
        .into_iter()
        .filter(|row| row.get(INPUT_KEY) == Some(input_key))
        .filter_map(|row| {
            let idx = row.get(CURSOR_IDX)?.parse().ok()?;
            let bytes = unhex(row.get(CURSOR_BLOB)?)?;
            let cursor = cursor_codec::decode(&bytes).ok()?;
            Some((idx, cursor))
        })
        .collect();
    input_rows.sort_by_key(|(idx, _)| *idx);
    let inputs = input_rows.into_iter().map(|(_, cursor)| cursor).collect();

    Some(MountedSqlSnapshot {
        sql,
        dep_tables,
        inputs,
    })
}

pub fn record_runtime_sql_mount(
    ctx: &RenderCtx,
    sql: &str,
    batch: &[&Cursor],
    dep_tables: &[String],
    nodes: &[Node<Cursor>],
) {
    let Some(graph) = ctx.runtime::<RuntimeGraph>() else {
        return;
    };
    let mount_id = mount_id_for_sql(sql);
    let input_key = input_key_for_batch(batch);
    let owner = graph.declare_owner(
        &format!("sprf://ast/sql/{mount_id}"),
        None,
        input_key.as_str(),
        mount_id.as_str(),
        "sql",
        ctx.expand_tick,
    );

    for dep_table in dep_tables {
        let source =
            graph.declare_source(&format!("sprf://source/table/{dep_table}"), ctx.expand_tick);
        ctx.put(SprfSubscribe::new(
            owner.clone(),
            dep_table.as_str(),
            source,
            ctx.expand_tick,
        ));

        // Phase 2: a referenced fact/rule table is a SourceId. Record
        // the read so the memo can gen-compare it (Phase 3).
        {
            use crate::source_clock::{SourceClock, SourceId};
            let sid = SourceId::for_table(dep_table);
            let gen_seen = graph.clock().current_gen(sid);
            ctx.record_read(sid.0, gen_seen);
        }
    }
    {
        let deps: Vec<(crate::source_clock::SourceId, u64)> = ctx
            .take_deps()
            .into_iter()
            .map(|(digest, gen)| (crate::source_clock::SourceId(digest), gen))
            .collect();
        graph.record_memo_deps(
            &format!("sprf://ast/sql/{mount_id}"),
            input_key.as_str(),
            &deps,
        );
    }

    let row_count = output_count(nodes);
    if !should_persist_mounted_outputs(row_count) {
        tracing::debug!(
            target: "sprefa::sql",
            rows = row_count,
            max = MAX_RUNTIME_SQL_SUPPORT_ROWS,
            "skip_large_runtime_sql_support_rows"
        );
        return;
    }
    let rows: Vec<Cursor> = output_cursors(nodes)
        .into_iter()
        .map(|cursor| cursor_without_support(cursor.as_ref()))
        .collect();
    let output =
        graph.declare_output_table(&format!("sprf://output/sql/{mount_id}"), ctx.expand_tick);
    ctx.put(SprfSupportRows::new(owner, output, rows, ctx.expand_tick));
}

pub fn record_runtime_sql_mount_count(
    ctx: &RenderCtx,
    sql: &str,
    batch: &[&Cursor],
    dep_tables: &[String],
    row_count: usize,
) {
    let Some(graph) = ctx.runtime::<RuntimeGraph>() else {
        return;
    };
    let mount_id = mount_id_for_sql(sql);
    let input_key = input_key_for_batch(batch);
    let owner = graph.declare_owner(
        &format!("sprf://ast/sql/{mount_id}"),
        None,
        input_key.as_str(),
        mount_id.as_str(),
        "sql",
        ctx.expand_tick,
    );

    for dep_table in dep_tables {
        let source =
            graph.declare_source(&format!("sprf://source/table/{dep_table}"), ctx.expand_tick);
        ctx.put(SprfSubscribe::new(
            owner.clone(),
            dep_table.as_str(),
            source,
            ctx.expand_tick,
        ));

        // Phase 2: a referenced fact/rule table is a SourceId. Record
        // the read so the memo can gen-compare it (Phase 3).
        {
            use crate::source_clock::{SourceClock, SourceId};
            let sid = SourceId::for_table(dep_table);
            let gen_seen = graph.clock().current_gen(sid);
            ctx.record_read(sid.0, gen_seen);
        }
    }
    {
        let deps: Vec<(crate::source_clock::SourceId, u64)> = ctx
            .take_deps()
            .into_iter()
            .map(|(digest, gen)| (crate::source_clock::SourceId(digest), gen))
            .collect();
        graph.record_memo_deps(
            &format!("sprf://ast/sql/{mount_id}"),
            input_key.as_str(),
            &deps,
        );
    }

    if !should_persist_mounted_outputs(row_count) {
        tracing::debug!(
            target: "sprefa::sql",
            rows = row_count,
            max = MAX_RUNTIME_SQL_SUPPORT_ROWS,
            "skip_large_runtime_sql_support_rows"
        );
    }
}

pub fn mount_id_for_sql(sql: &str) -> String {
    hex(blake3::hash(sql.as_bytes()).as_bytes())
}

pub fn input_key_for_batch(batch: &[&Cursor]) -> String {
    let mut h = blake3::Hasher::new();
    for cursor in batch {
        h.update(&cursor.content_hash());
        h.update(b"\0");
    }
    hex(h.finalize().as_bytes())
}

fn output_cursors(nodes: &[Node<Cursor>]) -> Vec<Arc<Cursor>> {
    let mut out = Vec::new();
    for node in nodes {
        collect_node_outputs(node, &mut out);
    }
    out
}

fn output_node_count(node: &Node<Cursor>) -> usize {
    match node {
        Node::Emit(_) => 1,
        Node::Many(nodes) => nodes.iter().map(output_node_count).sum(),
        Node::Yield { .. } | Node::Done => 0,
    }
}

fn nodes_added_since(
    nodes: &[Node<Cursor>],
    existing_cursor_ids: &std::collections::HashSet<String>,
) -> Vec<Node<Cursor>> {
    nodes
        .iter()
        .map(|node| filter_node_added_since(node, existing_cursor_ids))
        .collect()
}

fn filter_node_added_since(
    node: &Node<Cursor>,
    existing_cursor_ids: &std::collections::HashSet<String>,
) -> Node<Cursor> {
    match node {
        Node::Emit(cursor) => {
            let (cursor_id, _) = cursor_storage_parts(cursor);
            if existing_cursor_ids.contains(&cursor_id) {
                Node::Done
            } else {
                Node::Emit(cursor.clone())
            }
        }
        Node::Many(nodes) => {
            let kept: Vec<Node<Cursor>> = nodes
                .iter()
                .map(|node| filter_node_added_since(node, existing_cursor_ids))
                .filter(|node| !matches!(node, Node::Done))
                .collect();
            match kept.len() {
                0 => Node::Done,
                1 => kept.into_iter().next().unwrap(),
                _ => Node::Many(kept),
            }
        }
        Node::Yield { value, wake } => Node::Yield {
            value: value.clone(),
            wake: wake.clone(),
        },
        Node::Done => Node::Done,
    }
}

fn cursor_storage_parts(cursor: &Cursor) -> (String, Vec<u8>) {
    let encoded = cursor_codec::encode(&cursor_without_support(cursor));
    let cursor_id = hex(blake3::hash(&encoded).as_bytes());
    (cursor_id, encoded)
}

fn cursor_without_support(cursor: &Cursor) -> Cursor {
    let mut cursor = cursor.clone();
    cursor.unset(SUPPORT_CURSOR_ID);
    cursor
}

fn stamp_support_nodes(nodes: &[Node<Cursor>]) -> Vec<Node<Cursor>> {
    nodes.iter().map(stamp_support_node).collect()
}

fn stamp_support_node(node: &Node<Cursor>) -> Node<Cursor> {
    match node {
        Node::Emit(cursor) => {
            let (cursor_id, _) = cursor_storage_parts(cursor);
            let mut stamped = cursor.as_ref().clone();
            stamped.set(SUPPORT_CURSOR_ID, cursor_id);
            Node::Emit(Arc::new(stamped))
        }
        Node::Many(nodes) => Node::Many(nodes.iter().map(stamp_support_node).collect()),
        Node::Yield { value, wake } => Node::Yield {
            value: value.clone(),
            wake: wake.clone(),
        },
        Node::Done => Node::Done,
    }
}

pub fn record_fact_supports(
    store: &dyn FactStore<Cursor>,
    support_cursor_id: &str,
    table: &str,
    row_id: &str,
) {
    record_fact_support_batch(
        store,
        &[(
            support_cursor_id.to_string(),
            table.to_string(),
            row_id.to_string(),
        )],
    );
}

pub fn should_record_fact_support_count(count: usize) -> bool {
    should_persist_mounted_outputs(count)
}

pub fn record_fact_support_batch(
    store: &dyn FactStore<Cursor>,
    supports: &[(String, String, String)],
) {
    if supports.is_empty() {
        return;
    }
    let ledger = SupportLedger::new(store);
    for (support_cursor_id, table, row_id) in supports {
        ledger.add(support_cursor_id, table, row_id);
    }
}

/// Phase 5: counted (DRed) teardown for the generic memo path. Given
/// the PRIOR output cursors that the new render no longer produces,
/// compute their support cursor ids and run `cascade_retract` — the
/// DRed delete half: decrement each support's `mult`; a sink row is
/// deleted (and its support-children descended) only when its
/// `sum(mult)` reaches 0. A row still derived by another
/// `(owner, in_key)` path survives.
pub fn retract_memo_rows(
    store: &dyn FactStore<Cursor>,
    _owner_hex: &str,
    _in_hex: &str,
    removed_cursors: &[Arc<Cursor>],
) {
    if removed_cursors.is_empty() {
        return;
    }
    let ids: Vec<String> = removed_cursors
        .iter()
        .map(|c| cursor_storage_parts(c.as_ref()).0)
        .collect();
    cascade_retract(store, &ids);
}

/// Phase-5 SUPPORT-table support ledger. SQLite-backed (no in-RAM
/// ledger): the hot tier is the store's existing `StripedLru`. One
/// physical row per `(cursor_id, table, row_id)` triple carries that
/// triple's integer `mult`; `add` increments it, `dec` decrements it.
///
/// The plan's Layer-1 `SupportLedger::add(row, by_owner, in_key,
/// dep_source)` collapses to this three-column key: `cursor_id` is the
/// content hash of the deriving output cursor, which already folds
/// `(owner, in_key, dep_source)` in the existing v4 vocabulary
/// (`fact.rs` stamps it via `stamp_support_node`). Keeping that
/// vocabulary preserves colocated consistency with the mounted-query
/// path; the schema stays wide (`SUPPORT_COLS`) for a future Counting
/// upgrade (Decision 2).
pub struct SupportLedger<'a> {
    store: &'a dyn FactStore<Cursor>,
}

impl<'a> SupportLedger<'a> {
    pub fn new(store: &'a dyn FactStore<Cursor>) -> Self {
        store.declare(SUPPORT_TABLE, SUPPORT_COLS);
        Self { store }
    }

    fn triple_mult(&self, cursor_id: &str, table: &str, row_id: &str) -> i64 {
        self.store
            .rows_of(SUPPORT_TABLE)
            .iter()
            .filter(|r| {
                r.get(SUPPORT_CURSOR_ID) == Some(cursor_id)
                    && r.get(SUPPORT_TABLE_NAME) == Some(table)
                    && r.get(SUPPORT_ROW_ID) == Some(row_id)
            })
            .filter_map(|r| r.get(SUPPORT_MULT).and_then(|m| m.parse::<i64>().ok()))
            .sum()
    }

    fn write_triple(&self, cursor_id: &str, table: &str, row_id: &str, mult: i64) {
        self.store.delete_matching(
            SUPPORT_TABLE,
            &[
                (SUPPORT_CURSOR_ID, cursor_id),
                (SUPPORT_TABLE_NAME, table),
                (SUPPORT_ROW_ID, row_id),
            ],
        );
        if mult <= 0 {
            return;
        }
        let mut row = Cursor::default();
        row.set(SUPPORT_CURSOR_ID, cursor_id);
        row.set(SUPPORT_TABLE_NAME, table);
        row.set(SUPPORT_ROW_ID, row_id);
        row.set(SUPPORT_MULT, mult.to_string());
        self.store
            .insert_batch(SUPPORT_TABLE, vec![Arc::new(row)]);
    }

    /// `sum(mult)` over every support of sink `(table, row_id)`. The
    /// support-iff-mult invariant: the row is present in `table` IFF
    /// this is `> 0`.
    pub fn sink_mult(&self, table: &str, row_id: &str) -> i64 {
        self.store
            .rows_of(SUPPORT_TABLE)
            .iter()
            .filter(|r| {
                r.get(SUPPORT_TABLE_NAME) == Some(table)
                    && r.get(SUPPORT_ROW_ID) == Some(row_id)
            })
            .filter_map(|r| r.get(SUPPORT_MULT).and_then(|m| m.parse::<i64>().ok()))
            .sum()
    }

    /// `add(row, by_owner, in_key, dep_source)` — register one
    /// derivation path for the sink row.
    ///
    /// A *path* is one distinct `(cursor_id, table, row_id)` triple:
    /// `cursor_id` already folds `(owner, in_key, dep_source)`. Two
    /// distinct supports of the same sink row are two distinct triples;
    /// `sink_mult` sums them. Re-rendering the SAME deriving cursor
    /// re-adds the SAME triple and is idempotent (mult stays 1) — the
    /// triple's existence IS its one unit of support. This keeps the
    /// presence semantics the mounted-query path relied on while making
    /// `sink_mult` a true count of distinct paths (plan: support-iff-
    /// mult; a row survives losing one of N paths). DRed `dec` removes
    /// the triple (mult → 0).
    pub fn add(&self, cursor_id: &str, table: &str, row_id: &str) {
        if self.triple_mult(cursor_id, table, row_id) > 0 {
            return; // idempotent: same path re-rendered
        }
        self.write_triple(cursor_id, table, row_id, 1);
    }

    /// Decrement the triple's `mult` (the DRed teardown half). Returns
    /// the sink `(table, row_id)`'s new `sum(mult)` so the caller knows
    /// whether the last path is gone.
    fn dec(&self, cursor_id: &str, table: &str, row_id: &str) -> i64 {
        let next = self.triple_mult(cursor_id, table, row_id) - 1;
        self.write_triple(cursor_id, table, row_id, next);
        self.sink_mult(table, row_id)
    }
}

/// `cascade_retract`: the plan's DRed delete half. `stack = removed
/// support cursor ids`. Pop `r`; for every support whose
/// `SUPPORT_CURSOR_ID == r`, `dec` its `mult` for the retracting
/// `(table, row_id)`; if that sink `sum(mult) == 0` delete the row
/// from its sink table and push the row id (it may itself be the
/// support cursor of transitively-derived rows) onto the stack; if
/// `sum(mult) > 0` keep the row (another path still derives it) and
/// stop descending that branch (over-delete + stop-at-supported).
pub fn cascade_retract(store: &dyn FactStore<Cursor>, removed_cursor_ids: &[String]) {
    if removed_cursor_ids.is_empty() {
        return;
    }
    let ledger = SupportLedger::new(store);
    let mut stack: Vec<String> = removed_cursor_ids.to_vec();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    while let Some(r) = stack.pop() {
        if !seen.insert(r.clone()) {
            continue;
        }
        // Supports r derives: every triple whose SUPPORT_CURSOR_ID == r.
        let children: Vec<(String, String)> = store
            .rows_of(SUPPORT_TABLE)
            .iter()
            .filter(|row| row.get(SUPPORT_CURSOR_ID) == Some(r.as_str()))
            .filter_map(|row| {
                Some((
                    row.get(SUPPORT_TABLE_NAME)?.to_string(),
                    row.get(SUPPORT_ROW_ID)?.to_string(),
                ))
            })
            .collect();

        for (table, row_id) in children {
            let remaining = ledger.dec(&r, &table, &row_id);
            debug_assert!(
                remaining >= 0,
                "support-iff-mult: sink ({table}, {row_id}) sum(mult) went negative"
            );
            if remaining == 0 {
                // Last path gone → delete the row from its sink table…
                store.delete_matching(&table, &[(effect_runtime::v2::ID_COL, row_id.as_str())]);
                debug_assert_eq!(
                    ledger.sink_mult(&table, &row_id),
                    0,
                    "support-iff-mult: sink ({table}, {row_id}) deleted while still supported"
                );
                // …and descend: a deleted sink row may itself be the
                // support cursor of transitively-derived rows.
                stack.push(row_id);
            }
            // remaining > 0: another (owner,in_key) still derives the
            // row — keep it, stop descending this branch.
        }
    }
}

fn persist_output_cursors(outputs: Vec<Arc<Cursor>>) -> Vec<PersistedCursor> {
    outputs
        .into_iter()
        .map(|cursor| {
            let (cursor_id, encoded) = cursor_storage_parts(&cursor);
            PersistedCursor {
                cursor_id,
                blob_hex: hex(&encoded),
            }
        })
        .collect()
}

fn collect_node_outputs(node: &Node<Cursor>, out: &mut Vec<Arc<Cursor>>) {
    match node {
        Node::Emit(cursor) => out.push(cursor.clone()),
        Node::Many(nodes) => {
            for child in nodes {
                collect_node_outputs(child, out);
            }
        }
        Node::Yield { .. } => {}
        Node::Done => {}
    }
}

fn hex(bytes: &[u8]) -> String {
    const CHARS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(CHARS[(byte >> 4) as usize] as char);
        out.push(CHARS[(byte & 0x0f) as usize] as char);
    }
    out
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() / 2);
    let bytes = text.as_bytes();
    for pair in bytes.chunks_exact(2) {
        let hi = hex_nibble(pair[0])?;
        let lo = hex_nibble(pair[1])?;
        out.push((hi << 4) | lo);
    }
    Some(out)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use effect_runtime::v2::{FactStore, MemFactStore, Node};

    use super::*;

    fn cursor(value: &str) -> Cursor {
        let mut c = Cursor::default();
        c.value = Arc::from(value);
        c
    }

    #[test]
    fn record_sql_outputs_replaces_same_mount_and_input_key() {
        let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
        let input = cursor("seed");
        let batch = vec![&input];
        let first = vec![Node::Emit(Arc::new(cursor("old")))];
        let second = vec![Node::Emit(Arc::new(cursor("new")))];

        let deps = Vec::new();
        let added = record_sql_outputs(&store, "SELECT value FROM input", 1, &batch, &deps, &first);
        assert!(matches!(added.as_slice(), [Node::Emit(_)]));
        assert_eq!(store.rows_of(OUTPUT_TABLE).len(), 1);

        let added =
            record_sql_outputs(&store, "SELECT value FROM input", 2, &batch, &deps, &second);
        assert!(matches!(added.as_slice(), [Node::Emit(_)]));
        let rows = store.rows_of(OUTPUT_TABLE);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get(GENERATION), Some("2"));
        assert!(rows[0].get(CURSOR_ID).is_some());
        assert_eq!(store.rows_of(CURSOR_TABLE).len(), 2);
    }

    #[test]
    fn record_sql_outputs_removes_same_mount_when_new_result_is_empty() {
        let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
        let input = cursor("seed");
        let batch = vec![&input];
        let first = vec![Node::Emit(Arc::new(cursor("old")))];
        let empty = vec![Node::Done];

        let deps = Vec::new();
        let added = record_sql_outputs(&store, "SELECT value FROM input", 1, &batch, &deps, &first);
        assert!(matches!(added.as_slice(), [Node::Emit(_)]));
        assert_eq!(store.rows_of(OUTPUT_TABLE).len(), 1);

        let added = record_sql_outputs(&store, "SELECT value FROM input", 2, &batch, &deps, &empty);
        assert!(matches!(added.as_slice(), [Node::Done]));
        assert_eq!(store.rows_of(OUTPUT_TABLE).len(), 0);
        assert_eq!(store.rows_of(CURSOR_TABLE).len(), 1);
    }

    #[test]
    fn record_sql_outputs_returns_only_new_outputs_for_same_mount() {
        let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
        let input = cursor("seed");
        let batch = vec![&input];
        let old = Arc::new(cursor("old"));
        let new = Arc::new(cursor("new"));

        let first = vec![Node::Emit(old.clone())];
        let deps = Vec::new();
        let added = record_sql_outputs(&store, "SELECT value FROM input", 1, &batch, &deps, &first);
        assert!(matches!(added.as_slice(), [Node::Emit(_)]));

        let rerun = vec![Node::Many(vec![Node::Emit(old), Node::Emit(new.clone())])];
        let added = record_sql_outputs(&store, "SELECT value FROM input", 2, &batch, &deps, &rerun);
        assert_eq!(store.rows_of(OUTPUT_TABLE).len(), 2);
        assert_eq!(store.rows_of(CURSOR_TABLE).len(), 2);
        match added.as_slice() {
            [Node::Emit(cursor)] => assert_eq!(cursor.value.as_ref(), "new"),
            other => panic!("expected only the new output cursor, got {other:?}"),
        }
    }

    #[test]
    fn record_sql_outputs_records_mount_and_dependencies() {
        let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
        let input = cursor("seed");
        let batch = vec![&input];
        let deps = vec!["frontend_hooks".to_string(), "openapi_ops".to_string()];
        let outputs = vec![Node::Emit(Arc::new(cursor("hit")))];

        let added = record_sql_outputs(
            &store,
            "SELECT value FROM input JOIN frontend_hooks ON 1=1",
            7,
            &batch,
            &deps,
            &outputs,
        );

        assert!(matches!(added.as_slice(), [Node::Emit(_)]));
        let mounts = store.rows_of(MOUNT_TABLE);
        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].get(GENERATION), Some("7"));
        assert_eq!(
            mounts[0].get(SQL),
            Some("SELECT value FROM input JOIN frontend_hooks ON 1=1"),
        );

        let deps = store.rows_of(DEP_TABLE);
        let dep_names: std::collections::BTreeSet<String> = deps
            .iter()
            .filter_map(|row| row.get(DEP_TABLE_NAME).map(|value| value.to_string()))
            .collect();
        assert_eq!(
            dep_names,
            std::collections::BTreeSet::from([
                "frontend_hooks".to_string(),
                "openapi_ops".to_string(),
            ]),
        );
    }
}
