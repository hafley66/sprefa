//! Batch-local `sql`` relation op.
//!
//! First executable slice of the V4 SQL rule query plan:
//!
//!   upstream cursor batch -> temp `input` table -> SQLite query -> cursors
//!
//! The component snapshots referenced fact tables through `FactStore`.
//! That keeps the first implementation trait-backed while the language
//! contract stays SQLite-shaped.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use effect_runtime::v2::{
    splice_into, table_dirty_key, Component, Diag, FactStore, Next, Node, Pipe, Purity,
    QueueBackend, QueueRow, RenderCtx, SqliteFactStore, Wake, TABLE_DOMAIN,
};
use rusqlite::types::{Value as SqlValue, ValueRef};
use rusqlite::{params_from_iter, Connection};

use crate::compile::lower::ctx::{LowerCtx, LowerError};
use crate::compile::lower::op_def::{DslBody, DslShape, InterpKind, OperatorDef};
use crate::compile::lower::value::{CallArg, Value, ValueKind};
use crate::fact::{FactWrite, WriteAssign, WriteValue};
use crate::mounted_query;
use crate::rule::{RuleInvokeAssign, RuleInvokeComponent, RuleInvokeValue};
use crate::runtime_graph::RuntimeGraph;
use crate::sprf_introspect::PipeIntrospect;
use crate::{Cursor, FOCAL_TERM};

pub struct SqlQueryComponent {
    store: Arc<dyn FactStore<Cursor>>,
    sql: Arc<str>,
    referenced_tables: Arc<Vec<String>>,
    cache: Mutex<BTreeMap<[u8; 32], Vec<Node<Cursor>>>>,
    terminal_fact: Option<TerminalFactWrite>,
}

const SQL_DISPATCH_ROW_CHUNK: usize = 2_048;

#[derive(Clone)]
struct TerminalFactWrite {
    store: Arc<dyn FactStore<Cursor>>,
    table: Arc<str>,
}

impl SqlQueryComponent {
    pub fn new(store: Arc<dyn FactStore<Cursor>>, sql: impl Into<Arc<str>>) -> Self {
        let sql = sql.into();
        let referenced_tables = referenced_fact_tables(sql.as_ref()).into_iter().collect();
        Self::with_referenced_tables(store, sql, referenced_tables)
    }

    pub fn with_referenced_tables(
        store: Arc<dyn FactStore<Cursor>>,
        sql: impl Into<Arc<str>>,
        referenced_tables: Vec<String>,
    ) -> Self {
        Self {
            store,
            sql: sql.into(),
            referenced_tables: Arc::new(referenced_tables),
            cache: Mutex::new(BTreeMap::new()),
            terminal_fact: None,
        }
    }

    pub fn sql(&self) -> &str {
        self.sql.as_ref()
    }

    pub fn with_terminal_fact_write(
        &self,
        store: Arc<dyn FactStore<Cursor>>,
        table: impl Into<Arc<str>>,
    ) -> Self {
        Self {
            store: self.store.clone(),
            sql: self.sql.clone(),
            referenced_tables: self.referenced_tables.clone(),
            cache: Mutex::new(BTreeMap::new()),
            terminal_fact: Some(TerminalFactWrite {
                store,
                table: table.into(),
            }),
        }
    }

    pub fn has_terminal_fact_write(&self) -> bool {
        self.terminal_fact.is_some()
    }
}

impl Component for SqlQueryComponent {
    type Next = Cursor;

    fn dispatch(
        &self,
        ctx: &RenderCtx,
        rows: &[QueueRow<Cursor>],
        queue: &dyn QueueBackend<Cursor>,
    ) {
        let active_len = if self.terminal_fact.is_some() {
            rows.len()
        } else if should_chunk_sql_dispatch(self.sql.as_ref()) {
            rows.len().min(SQL_DISPATCH_ROW_CHUNK)
        } else {
            rows.len()
        };
        let active_rows = &rows[..active_len];
        let inputs: Vec<&Cursor> = active_rows.iter().map(|r| r.value.as_ref()).collect();
        if let Some(terminal_fact) = &self.terminal_fact {
            match run_sql_terminal_output_cursors(
                self.store.as_ref(),
                self.sql.as_ref(),
                &self.referenced_tables,
                &inputs,
            ) {
                Ok(outputs) => {
                    mounted_query::record_runtime_sql_mount_count(
                        ctx,
                        self.sql.as_ref(),
                        &inputs,
                        &self.referenced_tables,
                        outputs.len(),
                    );
                    insert_terminal_fact_outputs(ctx, terminal_fact, outputs);
                }
                Err(e) => ctx.diag.emit(Diag::error("sql/runtime", e)),
            }
            enqueue_table_wakes(ctx, active_rows, queue, self.referenced_tables.as_ref());
            requeue_inactive(rows, active_len, queue);
            return;
        }

        let nodes = self.render_batch(ctx, &inputs);
        for (row, node) in active_rows.iter().zip(nodes) {
            splice_into(row, node, ctx.depth + 1, ctx.expand_tick, queue);
        }
        enqueue_table_wakes(ctx, active_rows, queue, self.referenced_tables.as_ref());
        requeue_inactive(rows, active_len, queue);
    }

    fn render_batch(&self, ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        if batch.is_empty() {
            return Vec::new();
        }

        let clock = ctx
            .runtime::<crate::runtime_graph::RuntimeGraph>()
            .map(|g| g.clock().clone());
        let key = sql_batch_cache_key(
            self.store.as_ref(),
            clock.as_deref(),
            self.sql.as_ref(),
            &self.referenced_tables,
            batch,
        );
        if let Some(cached) = self.cache.lock().unwrap().get(&key).cloned() {
            mounted_query::record_runtime_sql_mount(
                ctx,
                self.sql.as_ref(),
                batch,
                &self.referenced_tables,
                &cached,
            );
            return mounted_query::record_sql_outputs(
                &self.store,
                self.sql.as_ref(),
                ctx.expand_tick,
                batch,
                &self.referenced_tables,
                &cached,
            );
        }

        let result = match run_sql_batch(
            self.store.as_ref(),
            self.sql.as_ref(),
            &self.referenced_tables,
            batch,
        ) {
            Ok(grouped) => grouped,
            Err(e) => {
                ctx.diag.emit(Diag::error("sql/runtime", e));
                batch.iter().map(|_| Node::Done).collect()
            }
        };
        mounted_query::record_runtime_sql_mount(
            ctx,
            self.sql.as_ref(),
            batch,
            &self.referenced_tables,
            &result,
        );
        let output_count = mounted_query::output_count(&result);
        if !mounted_query::should_persist_mounted_outputs(output_count) {
            return result;
        }
        let added = mounted_query::record_sql_outputs(
            &self.store,
            self.sql.as_ref(),
            ctx.expand_tick,
            batch,
            &self.referenced_tables,
            &result,
        );
        self.cache.lock().unwrap().insert(key, result.clone());
        added
    }

    fn purity(&self) -> Purity {
        Purity::Read
    }

    fn describe(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
}

fn enqueue_table_wakes(
    ctx: &RenderCtx,
    active_rows: &[QueueRow<Cursor>],
    queue: &dyn QueueBackend<Cursor>,
    referenced_tables: &[String],
) {
    for row in active_rows {
        if ctx.runtime::<RuntimeGraph>().is_some() {
            continue;
        }
        for table in referenced_tables {
            queue.enqueue_replacing_parked(QueueRow {
                id: 0,
                parent_id: Some(row.id),
                batch_idx: row.batch_idx,
                path: row.path.clone(),
                pipe_hash: row.pipe_hash,
                instance_id: row.instance_id,
                depth: row.depth,
                value: row.value.clone(),
                wake: Wake::Key {
                    domain: TABLE_DOMAIN.into(),
                    key: table_dirty_key(table),
                },
                expand_tick: ctx.expand_tick,
                enqueued_at_ns: row.enqueued_at_ns,
            });
        }
    }
}

fn requeue_inactive(
    rows: &[QueueRow<Cursor>],
    active_len: usize,
    queue: &dyn QueueBackend<Cursor>,
) {
    for row in &rows[active_len..] {
        let mut row = row.clone();
        row.id = 0;
        queue.enqueue(row);
    }
}

fn insert_terminal_fact_outputs(
    ctx: &RenderCtx,
    terminal_fact: &TerminalFactWrite,
    outputs: Vec<Arc<Cursor>>,
) {
    if outputs.is_empty() {
        return;
    }
    let row_count = outputs.len();
    let support_input_count = outputs
        .iter()
        .filter(|cursor| cursor.get(mounted_query::SUPPORT_CURSOR_ID).is_some())
        .count();
    let support_rows: Vec<(String, String, String)> =
        if mounted_query::should_record_fact_support_count(support_input_count) {
            outputs
                .iter()
                .filter_map(|cursor| {
                    cursor
                        .get(mounted_query::SUPPORT_CURSOR_ID)
                        .map(|support_id| {
                            (
                                support_id.to_string(),
                                terminal_fact.table.to_string(),
                                terminal_fact
                                    .store
                                    .row_id_for(terminal_fact.table.as_ref(), cursor.as_ref()),
                            )
                        })
                })
                .collect()
        } else {
            Vec::new()
        };
    let insert_t0 = std::time::Instant::now();
    terminal_fact
        .store
        .insert_batch(terminal_fact.table.as_ref(), outputs);
    if let Some(graph) = ctx.runtime::<RuntimeGraph>() {
        graph.notify_table_inserted(terminal_fact.table.as_ref(), ctx.expand_tick);
    }
    mounted_query::record_fact_support_batch(terminal_fact.store.as_ref(), &support_rows);
    tracing::info!(
        target: "sprefa::sql",
        table = %terminal_fact.table,
        rows = row_count,
        insert_ms = insert_t0.elapsed().as_secs_f64() * 1000.0,
        "terminal_sql_fact_insert"
    );
}

fn should_chunk_sql_dispatch(sql: &str) -> bool {
    let sql = sql.to_ascii_lowercase();
    sql.contains(" join ") && !sql.contains("not exists")
}

fn sql_batch_cache_key(
    store: &dyn FactStore<Cursor>,
    clock: Option<&crate::source_clock::FactStoreClock>,
    sql: &str,
    referenced_tables: &[String],
    batch: &[&Cursor],
) -> [u8; 32] {
    use crate::source_clock::{SourceClock, SourceId};
    let mut h = blake3::Hasher::new();
    h.update(sql.as_bytes());
    h.update(b"\0input\0");
    for cursor in batch {
        h.update(&cursor.content_hash());
    }
    h.update(b"\0tables\0");
    for table in referenced_tables {
        h.update(table.as_bytes());
        h.update(b"\0");
        // Phase 1: fold the unified source clock (the fused-SQL write
        // path bumps it). Keep folding the fact store's table_version
        // too — direct insert/delete/commit paths still bump that and
        // the cache must invalidate on those as well.
        let clock_gen = clock
            .map(|c| c.current_gen(SourceId::for_table(table)))
            .unwrap_or(0);
        h.update(&clock_gen.to_le_bytes());
        h.update(b"\0");
        h.update(&store.table_version(table).to_le_bytes());
        h.update(b"\0");
    }
    *h.finalize().as_bytes()
}

pub fn run_sql_batch(
    store: &dyn FactStore<Cursor>,
    sql: &str,
    referenced_tables: &[String],
    batch: &[&Cursor],
) -> Result<Vec<Node<Cursor>>, String> {
    if let Some(sqlite) = store.as_any().downcast_ref::<SqliteFactStore<Cursor>>() {
        return run_sql_batch_on_sqlite_fact_store(sqlite, sql, referenced_tables, batch);
    }

    let mut conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
    materialize_input(&mut conn, batch)?;

    for table in referenced_tables {
        let rows = store.rows_of(&table);
        let declared_cols = store.declared_cols(&table).unwrap_or_default();
        materialize_fact_table(&mut conn, &table, &declared_cols, &rows)?;
    }

    query_sql_rows(&mut conn, sql, batch)
}

fn run_sql_terminal_output_cursors(
    store: &dyn FactStore<Cursor>,
    sql: &str,
    referenced_tables: &[String],
    batch: &[&Cursor],
) -> Result<Vec<Arc<Cursor>>, String> {
    if let Some(sqlite) = store.as_any().downcast_ref::<SqliteFactStore<Cursor>>() {
        return run_sql_terminal_output_cursors_on_sqlite_fact_store(
            sqlite,
            sql,
            referenced_tables,
            batch,
        );
    }

    let mut conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
    materialize_input(&mut conn, batch)?;

    for table in referenced_tables {
        let rows = store.rows_of(table);
        let declared_cols = store.declared_cols(table).unwrap_or_default();
        materialize_fact_table(&mut conn, table, &declared_cols, &rows)?;
    }

    query_sql_output_cursors(&mut conn, sql, batch)
}

fn run_sql_batch_on_sqlite_fact_store(
    store: &SqliteFactStore<Cursor>,
    sql: &str,
    referenced_tables: &[String],
    batch: &[&Cursor],
) -> Result<Vec<Node<Cursor>>, String> {
    store.with_connection(|conn| {
        drop_sql_workspace(conn, referenced_tables)?;
        materialize_input(conn, batch)?;
        for table in referenced_tables {
            create_fact_view(conn, store, table)?;
        }
        let result = query_sql_rows(conn, sql, batch);
        drop_sql_workspace(conn, referenced_tables)?;
        result
    })
}

fn run_sql_terminal_output_cursors_on_sqlite_fact_store(
    store: &SqliteFactStore<Cursor>,
    sql: &str,
    referenced_tables: &[String],
    batch: &[&Cursor],
) -> Result<Vec<Arc<Cursor>>, String> {
    store.with_connection(|conn| {
        drop_sql_workspace(conn, referenced_tables)?;
        materialize_input(conn, batch)?;
        for table in referenced_tables {
            create_fact_view(conn, store, table)?;
        }
        let result = query_sql_output_cursors(conn, sql, batch);
        drop_sql_workspace(conn, referenced_tables)?;
        result
    })
}

fn query_sql_rows(
    conn: &mut Connection,
    sql: &str,
    batch: &[&Cursor],
) -> Result<Vec<Node<Cursor>>, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let col_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let col_count = col_names.len();

    let mut grouped: Vec<Vec<Node<Cursor>>> = vec![Vec::new(); batch.len()];
    let mut synthetic: Vec<Node<Cursor>> = Vec::new();

    let rows = stmt
        .query_map([], |row| {
            let mut out = BTreeMap::new();
            for (idx, name) in col_names.iter().enumerate().take(col_count) {
                let value = sql_value_to_string(row.get_ref(idx)?);
                out.insert(name.clone(), value);
            }
            Ok(out)
        })
        .map_err(|e| e.to_string())?;

    for row in rows {
        let row = row.map_err(|e| e.to_string())?;
        let cursor_idx = row
            .get("__cursor_idx")
            .and_then(|s| s.parse::<usize>().ok());

        let mut child = match cursor_idx.and_then(|idx| batch.get(idx).copied()) {
            Some(source) => source.clone(),
            None => Cursor::default(),
        };

        for (name, value) in &row {
            if name == "__cursor_idx" {
                continue;
            }
            if name == "value" {
                child.value = Arc::<str>::from(value.as_str());
                continue;
            }
            child.set(name, value.as_str());
        }

        let node = Node::Emit(Arc::new(child));
        match cursor_idx {
            Some(idx) if idx < grouped.len() => grouped[idx].push(node),
            _ => synthetic.push(node),
        }
    }

    let mut out: Vec<Node<Cursor>> = grouped.into_iter().map(nodes_to_node).collect();
    if !synthetic.is_empty() {
        if out.is_empty() {
            out.push(nodes_to_node(synthetic));
        } else {
            match &mut out[0] {
                Node::Done => out[0] = nodes_to_node(synthetic),
                Node::Many(existing) => existing.extend(synthetic),
                other => {
                    let previous = other.clone();
                    let mut nodes = vec![previous];
                    nodes.extend(synthetic);
                    out[0] = Node::Many(nodes);
                }
            }
        }
    }
    Ok(out)
}

fn query_sql_output_cursors(
    conn: &mut Connection,
    sql: &str,
    batch: &[&Cursor],
) -> Result<Vec<Arc<Cursor>>, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let col_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let cursor_idx_col = col_names.iter().position(|name| name == "__cursor_idx");
    let mut out = Vec::new();
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let cursor_idx = match cursor_idx_col {
            Some(idx) => {
                let value = sql_value_to_string(row.get_ref(idx).map_err(|e| e.to_string())?);
                value.parse::<usize>().ok()
            }
            None => None,
        };

        let mut child = match cursor_idx.and_then(|idx| batch.get(idx).copied()) {
            Some(source) => source.clone(),
            None => Cursor::default(),
        };

        for (idx, name) in col_names.iter().enumerate() {
            if Some(idx) == cursor_idx_col {
                continue;
            }
            let value = sql_value_to_string(row.get_ref(idx).map_err(|e| e.to_string())?);
            if name == "value" {
                child.value = Arc::<str>::from(value.as_str());
                continue;
            }
            child.set(name, value.as_str());
        }
        out.push(Arc::new(child));
    }
    Ok(out)
}

fn drop_sql_workspace(conn: &mut Connection, referenced_tables: &[String]) -> Result<(), String> {
    conn.execute("DROP TABLE IF EXISTS temp.input", [])
        .map_err(|e| e.to_string())?;
    for table in referenced_tables {
        conn.execute(
            format!("DROP VIEW IF EXISTS temp.{}", quote_ident(table)).as_str(),
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn create_fact_view(
    conn: &mut Connection,
    store: &SqliteFactStore<Cursor>,
    table: &str,
) -> Result<(), String> {
    let cols = store.declared_cols(table).unwrap_or_default();
    let mut select = Vec::with_capacity(cols.len().max(1));
    let mut joins = Vec::with_capacity(cols.len());
    for col in &cols {
        let sql_col = fact_col_sql(col);
        let alias = format!("s_{sql_col}");
        select.push(format!(
            "{}.value AS {}",
            quote_ident(&alias),
            quote_ident(col),
        ));
        joins.push(format!(
            "JOIN sprf_strings {} ON {}.id = t.{}",
            quote_ident(&alias),
            quote_ident(&alias),
            quote_ident(format!("{sql_col}_id").as_str()),
        ));
    }
    if select.is_empty() {
        select.push("t._id AS _id".to_string());
    }
    let sql = format!(
        "CREATE TEMP VIEW {} AS SELECT {} FROM {} t {}",
        quote_ident(table),
        select.join(", "),
        quote_ident(format!("{table}_facts").as_str()),
        joins.join(" "),
    );
    conn.execute(&sql, []).map_err(|e| e.to_string())?;
    Ok(())
}

fn nodes_to_node(nodes: Vec<Node<Cursor>>) -> Node<Cursor> {
    let nodes = dedupe_nodes_by_cursor_hash(nodes);
    match nodes.len() {
        0 => Node::Done,
        1 => nodes.into_iter().next().unwrap(),
        _ => Node::Many(nodes),
    }
}

fn dedupe_nodes_by_cursor_hash(nodes: Vec<Node<Cursor>>) -> Vec<Node<Cursor>> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for node in nodes {
        match &node {
            Node::Emit(cursor) => {
                if seen.insert(cursor.content_hash()) {
                    out.push(node);
                }
            }
            _ => out.push(node),
        }
    }
    out
}

fn materialize_input(conn: &mut Connection, batch: &[&Cursor]) -> Result<(), String> {
    let mut cols = BTreeSet::new();
    for cursor in batch {
        for term in cursor
            .terms
            .iter()
            .filter(|t| t.name.as_ref() != FOCAL_TERM)
        {
            if term.name.as_ref() != "__cursor_idx" && term.name.as_ref() != "value" {
                cols.insert(term.name.to_string());
            }
        }
    }

    let mut ordered = vec!["__cursor_idx".to_string(), "value".to_string()];
    ordered.extend(cols);
    create_text_table(conn, "input", &ordered, Some("__cursor_idx"))?;
    insert_rows(
        conn,
        "input",
        &ordered,
        batch.iter().enumerate().map(|(idx, cursor)| {
            ordered
                .iter()
                .map(|col| {
                    if col == "__cursor_idx" {
                        SqlValue::Integer(idx as i64)
                    } else if col == "value" {
                        SqlValue::Text(cursor.value.to_string())
                    } else {
                        SqlValue::Text(cursor.get(col).unwrap_or("").to_string())
                    }
                })
                .collect::<Vec<_>>()
        }),
    )?;
    create_text_indexes(conn, "input", &ordered)?;

    Ok(())
}

fn materialize_fact_table(
    conn: &mut Connection,
    table: &str,
    declared_cols: &[String],
    rows: &[Arc<Cursor>],
) -> Result<(), String> {
    let mut cols = BTreeSet::new();
    for col in declared_cols {
        if col != "value" {
            cols.insert(col.to_string());
        }
    }
    for row in rows {
        for term in row.terms.iter().filter(|t| t.name.as_ref() != FOCAL_TERM) {
            if term.name.as_ref() != "value" {
                cols.insert(term.name.to_string());
            }
        }
    }

    let mut ordered = vec!["value".to_string()];
    ordered.extend(cols);
    create_text_table(conn, table, &ordered, None)?;
    insert_rows(
        conn,
        table,
        &ordered,
        rows.iter().map(|row| {
            ordered
                .iter()
                .map(|col| {
                    if col == "value" {
                        SqlValue::Text(row.value.to_string())
                    } else {
                        SqlValue::Text(row.get(col).unwrap_or("").to_string())
                    }
                })
                .collect::<Vec<_>>()
        }),
    )?;
    create_text_indexes(conn, table, &ordered)?;

    Ok(())
}

fn create_text_table(
    conn: &Connection,
    table: &str,
    cols: &[String],
    integer_col: Option<&str>,
) -> Result<(), String> {
    let defs: Vec<String> = cols
        .iter()
        .map(|col| {
            let ty = if Some(col.as_str()) == integer_col {
                "INTEGER"
            } else {
                "TEXT"
            };
            format!("{} {ty}", quote_ident(col))
        })
        .collect();
    let sql = format!(
        "CREATE TEMP TABLE {} ({})",
        quote_ident(table),
        defs.join(", ")
    );
    conn.execute(&sql, []).map_err(|e| e.to_string())?;
    Ok(())
}

fn insert_rows(
    conn: &mut Connection,
    table: &str,
    cols: &[String],
    rows: impl IntoIterator<Item = Vec<SqlValue>>,
) -> Result<(), String> {
    let col_sql = cols
        .iter()
        .map(|c| quote_ident(c))
        .collect::<Vec<_>>()
        .join(", ");
    let placeholders = (0..cols.len()).map(|_| "?").collect::<Vec<_>>().join(", ");
    let sql = format!(
        "INSERT INTO {} ({col_sql}) VALUES ({placeholders})",
        quote_ident(table),
    );
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    {
        let mut stmt = tx.prepare(&sql).map_err(|e| e.to_string())?;
        for values in rows {
            stmt.execute(params_from_iter(values.iter()))
                .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

fn create_text_indexes(conn: &Connection, table: &str, cols: &[String]) -> Result<(), String> {
    for col in cols {
        if col == "value" || col == "__cursor_idx" {
            continue;
        }
        let sql = format!(
            "CREATE INDEX {} ON {} ({})",
            quote_ident(format!("{table}_{col}_idx").as_str()),
            quote_ident(table),
            quote_ident(col),
        );
        conn.execute(&sql, []).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn sql_value_to_string(value: ValueRef<'_>) -> String {
    match value {
        ValueRef::Null => String::new(),
        ValueRef::Integer(i) => i.to_string(),
        ValueRef::Real(f) => f.to_string(),
        ValueRef::Text(bytes) => String::from_utf8_lossy(bytes).to_string(),
        ValueRef::Blob(bytes) => String::from_utf8_lossy(bytes).to_string(),
    }
}

pub fn quote_ident(s: &str) -> String {
    let escaped = s.replace('"', "\"\"");
    format!("\"{escaped}\"")
}

pub fn fact_col_sql(s: &str) -> &str {
    s.strip_prefix(':').unwrap_or(s)
}

fn referenced_fact_tables(sql: &str) -> BTreeSet<String> {
    let tokens = sql_tokens(sql);
    let mut out = BTreeSet::new();
    let mut expect_table = false;

    for token in tokens {
        let upper = token.to_ascii_uppercase();
        if expect_table {
            expect_table = false;
            if token == "(" || upper == "SELECT" {
                continue;
            }
            if token != "input" && is_ident(&token) {
                out.insert(token);
            }
            continue;
        }
        if upper == "FROM" || upper.ends_with("JOIN") {
            expect_table = true;
        }
    }

    out
}

fn sql_tokens(sql: &str) -> Vec<String> {
    let bytes = sql.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' | b'"' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == quote {
                        i += 1;
                        if i < bytes.len() && bytes[i] == quote {
                            i += 1;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
            }
            b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'-' => {
                i += 2;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
            }
            b if b.is_ascii_alphabetic() || b == b'_' => {
                let lo = i;
                i += 1;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                out.push(sql[lo..i].to_string());
            }
            b'(' => {
                out.push("(".to_string());
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

fn is_ident(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    if !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_') {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

fn rewrite_sql_interps(dsl: &DslBody) -> Result<String, LowerError> {
    let mut interps = dsl.interps.clone();
    interps.sort_by_key(|i| i.range.lo);

    let mut out = String::with_capacity(dsl.raw.len());
    let mut cursor = 0usize;
    for interp in interps {
        let lo = interp.range.lo as usize;
        let hi = interp.range.hi as usize;
        if lo < cursor || hi > dsl.raw.len() {
            return Err(LowerError::Unknown(
                "sql: interpolation span out of bounds".into(),
            ));
        }
        out.push_str(&dsl.raw[cursor..lo]);
        match &interp.kind {
            // Bind (`${X?}`) and Read (`${X}`) lower the same way: a SQL
            // body has no notion of "introducing a new binding"; the rule
            // head's `X?` is what introduces. The `?` here is purely the
            // universal surface marker.
            InterpKind::Term { mode: _, field } => {
                if interp.name.as_ref() == "&" {
                    if field.as_ref().map(|f| f.as_ref()) == Some("value") {
                        out.push_str("\"input\".\"value\"");
                    } else {
                        return Err(LowerError::Unknown(
                            "sql: only ${&.value} focal interpolation is supported".into(),
                        ));
                    }
                } else if field.is_none() {
                    out.push_str("\"input\".");
                    out.push_str(&quote_ident(&interp.name));
                } else {
                    return Err(LowerError::Unknown(
                        "sql: field interpolation must be explicit SQL over input columns".into(),
                    ));
                }
            }
            InterpKind::SubPipe { .. } => {
                return Err(LowerError::Unknown(
                    "sql: identifier or sub-pipe interpolation is rejected".into(),
                ));
            }
        }
        cursor = hi;
    }
    out.push_str(&dsl.raw[cursor..]);
    Ok(out)
}

/// Prepend a stable per-site marker so two syntactically-identical
/// rule QUERY / `sql` occurrences hash to distinct `mount_id`s in the
/// durable mount table. The marker is a SQL line comment: `sql_tokens`
/// and SQLite both skip it, `referenced_fact_tables` is unaffected,
/// and it round-trips through the persisted mount snapshot so the
/// reactive replay path reconstructs the same `mount_id`.
///
/// Without this, identical `r?()` queries collide on one
/// `(mount_id, input_key)` mount and the second occurrence is deduped
/// to zero rows by the "added since existing" diff in
/// `mounted_query::record_sql_outputs`.
fn with_query_site_tag(ctx: &LowerCtx, sql: String) -> String {
    let site = ctx.next_query_site();
    format!("-- sprf-site:{site}\n{sql}")
}

pub struct SqlDef;

impl OperatorDef for SqlDef {
    fn name(&self) -> &'static str {
        "sql"
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }

    fn lower(
        &self,
        ctx: &LowerCtx,
        _flow: Option<Value>,
        _args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let dsl = dsl.ok_or_else(|| LowerError::Unknown("sql: dsl body required".into()))?;
        let sql = with_query_site_tag(ctx, rewrite_sql_interps(dsl)?);
        Ok(Pipe::new().step(Arc::new(SqlQueryComponent::new(ctx.store.clone(), sql))))
    }
}

pub fn rule_table_call_pipe(
    ctx: &LowerCtx,
    table: &str,
    args: &[CallArg],
) -> Result<Pipe<Cursor>, LowerError> {
    // Phase A: file-scoped sink-table prefix. The user-facing `table`
    // atom maps to a physical name keyed by the source URI. CLI/test
    // paths get back the bare name; LSP/ingest paths get the prefix.
    let physical = ctx.rule_table(table);
    let cols = ctx
        .store
        .declared_cols(&physical)
        .ok_or_else(|| LowerError::Unknown(format!("rule table `{table}` is not declared")))?;
    if args.len() > cols.len() {
        return Err(LowerError::Unknown(format!(
            "rule table `{table}` has {} column(s), call passed {} arg(s)",
            cols.len(),
            args.len(),
        )));
    }

    #[derive(Debug)]
    enum ArgMode {
        BoundTerm { col: String, term: String },
        BoundLiteral { col: String, value: String },
        Project { col: String, term: String },
    }

    let resolved = resolve_rule_args(table, &cols, args)?;
    let mut modes = Vec::with_capacity(resolved.len());
    for (col, arg) in resolved {
        match arg.kind() {
            ValueKind::Atom(value) => {
                if value.as_ref() == "&.value" {
                    modes.push(ArgMode::BoundTerm {
                        col,
                        term: "value".to_string(),
                    });
                } else {
                    modes.push(ArgMode::BoundLiteral {
                        col,
                        value: value.to_string(),
                    });
                }
            }
            ValueKind::Pipe(pipe) => {
                if let Some(term) = pipe.binds_terms().first() {
                    modes.push(ArgMode::Project {
                        col,
                        term: term.to_string(),
                    });
                } else if let Some(term) = pipe.reads_terms().first() {
                    modes.push(ArgMode::BoundTerm {
                        col,
                        term: term.to_string(),
                    });
                } else {
                    return Err(LowerError::Unknown(
                        "rule call args must be atoms or terms".into(),
                    ));
                }
            }
            ValueKind::Callable(_) => {
                return Err(LowerError::Unknown(
                    "rule call args cannot be a callable".into(),
                ));
            }
        }
    }

    let rule_alias = "__rule";
    let mut select_cols = vec!["input.__cursor_idx".to_string()];
    if modes.is_empty() {
        for col in &cols {
            select_cols.push(format!(
                "{}.{} AS {}",
                quote_ident(rule_alias),
                quote_ident(col),
                quote_ident(col),
            ));
        }
    } else {
        for mode in &modes {
            match mode {
                ArgMode::Project { col, term } => {
                    select_cols.push(format!(
                        "{}.{} AS {}",
                        quote_ident(rule_alias),
                        quote_ident(col),
                        quote_ident(term),
                    ));
                }
                ArgMode::BoundLiteral { col, value } => {
                    select_cols.push(format!(
                        "{} AS {}",
                        quote_sql_literal(value),
                        quote_ident(col),
                    ));
                }
                ArgMode::BoundTerm { .. } => {}
            }
        }
    }

    // `?`-then-same-name-ref desugar: a term BOUND by an earlier
    // `Project` arg (the `?` form) and then READ by a later arg in the
    // SAME call is an INTRA-ROW self-equality (`__rule.colB =
    // __rule.colA`), not a correlated read against the streaming
    // `input` cursor (`__rule.colB = input.term`) — the term is born
    // in this very read, so there is no prior cursor column. The map
    // is populated ONLY by `Project` (the Term-Bind subset; literals,
    // `&.value`, and pipe values are inert) and consulted ONLY by
    // `BoundTerm`. Forward scan honors arg ORDER: ref-before-`?`
    // (`r?(N, N?)`) keeps the legacy outer-cursor correlation.
    let mut projected_term_col: std::collections::HashMap<&str, &str> =
        std::collections::HashMap::new();
    let mut predicates = Vec::new();
    for mode in &modes {
        match mode {
            ArgMode::BoundTerm { col, term } => {
                if let Some(bound_col) = projected_term_col.get(term.as_str()) {
                    predicates.push(format!(
                        "{}.{} = {}.{}",
                        quote_ident(rule_alias),
                        quote_ident(col),
                        quote_ident(rule_alias),
                        quote_ident(bound_col),
                    ));
                    continue;
                }
                predicates.push(format!(
                    "{}.{} = input.{}",
                    quote_ident(rule_alias),
                    quote_ident(col),
                    quote_ident(term),
                ));
            }
            ArgMode::BoundLiteral { col, value } => {
                predicates.push(format!(
                    "{}.{} = {}",
                    quote_ident(rule_alias),
                    quote_ident(col),
                    quote_sql_literal(value),
                ));
            }
            ArgMode::Project { col, term } => {
                projected_term_col.insert(term.as_str(), col.as_str());
            }
        }
    }

    let all_grounded =
        !modes.is_empty() && modes.iter().all(|m| !matches!(m, ArgMode::Project { .. }));
    let select_prefix = if all_grounded {
        "SELECT DISTINCT"
    } else {
        "SELECT"
    };
    let mut sql = format!(
        "{select_prefix} {}\nFROM input JOIN {} AS {} ON 1=1",
        select_cols.join(", "),
        quote_ident(&physical),
        rule_alias,
    );
    if !predicates.is_empty() {
        sql.push_str("\nWHERE ");
        sql.push_str(&predicates.join(" AND "));
    }

    let sql = with_query_site_tag(ctx, sql);
    Ok(
        Pipe::new().step(Arc::new(SqlQueryComponent::with_referenced_tables(
            ctx.store.clone(),
            sql,
            vec![physical],
        ))),
    )
}

pub fn rule_apply_write_pipe(
    ctx: &LowerCtx,
    table: &str,
    args: &[CallArg],
) -> Result<Pipe<Cursor>, LowerError> {
    // Phase A: file-scoped physical table; same prefix as the matching
    // rule decl. See `LowerCtx::rule_table`.
    let physical = ctx.rule_table(table);
    let cols = ctx
        .store
        .declared_cols(&physical)
        .ok_or_else(|| LowerError::Unknown(format!("rule table `{table}` is not declared")))?;
    let resolved = resolve_rule_args(table, &cols, args)?;
    if resolved.is_empty() {
        return Ok(Pipe::new().step(Arc::new(FactWrite::new(
            ctx.store.clone(),
            Arc::<str>::from(physical),
        ))));
    }

    let mut assignments = Vec::with_capacity(resolved.len());
    for (col, value) in resolved {
        let value = grounded_write_value(table, value)?;
        assignments.push(WriteAssign {
            col: Arc::<str>::from(col),
            value,
        });
    }

    Ok(Pipe::new().step(Arc::new(FactWrite::projected(
        ctx.store.clone(),
        Arc::<str>::from(physical),
        assignments,
    ))))
}

pub fn rule_write_pipe(ctx: &LowerCtx, args: &[CallArg]) -> Result<Pipe<Cursor>, LowerError> {
    let Some((first, rest)) = args.split_first() else {
        return Err(LowerError::Unknown(
            "rule write requires a :table arg".into(),
        ));
    };
    if first.keyword.is_some() {
        return Err(LowerError::Unknown(
            "rule write table arg must be positional".into(),
        ));
    }
    let table = match first.value.kind() {
        ValueKind::Atom(s) => s.clone(),
        _ => {
            return Err(LowerError::Unknown(
                "rule write first arg must be a :table atom".into(),
            ))
        }
    };
    // Phase A: file-scoped physical name (same as the decl side).
    let physical = ctx.rule_table(&table);
    let full_extent = ctx.pipe_full_extent.get();
    if rest.is_empty() {
        return Ok(Pipe::new().step(Arc::new(
            FactWrite::new(ctx.store.clone(), physical).with_full_extent(full_extent),
        )));
    }
    let cols = ctx
        .store
        .declared_cols(&physical)
        .ok_or_else(|| LowerError::Unknown(format!("rule table `{table}` is not declared")))?;
    let resolved = resolve_rule_args(&table, &cols, rest)?;
    let mut assignments = Vec::with_capacity(resolved.len());
    for (col, value) in resolved {
        let value = match value.kind() {
            ValueKind::Atom(value) if value.as_ref() == "&.value" => WriteValue::Value,
            ValueKind::Atom(value) => WriteValue::Literal(value.clone()),
            ValueKind::Pipe(pipe) => {
                if let Some(term) = pipe.reads_terms().first() {
                    WriteValue::Term(term.clone())
                } else if let Some(term) = pipe.binds_terms().first() {
                    WriteValue::Term(term.clone())
                } else {
                    return Err(LowerError::Unknown(
                        "rule write args must be atoms or terms".into(),
                    ));
                }
            }
            ValueKind::Callable(_) => {
                return Err(LowerError::Unknown(
                    "rule write args cannot be a callable".into(),
                ));
            }
        };
        assignments.push(WriteAssign {
            col: Arc::<str>::from(col),
            value,
        });
    }
    Ok(Pipe::new().step(Arc::new(
        FactWrite::projected(ctx.store.clone(), physical, assignments)
            .with_full_extent(full_extent),
    )))
}

pub fn rule_body_call_pipe(
    ctx: &LowerCtx,
    table: &str,
    force: bool,
    args: &[CallArg],
) -> Result<Pipe<Cursor>, LowerError> {
    let rule = ctx
        .get_rule(table)
        .ok_or_else(|| LowerError::Unknown(format!("bodied rule `{table}` is not declared")))?;
    let cols: Vec<String> = rule.sink_cols.iter().map(|col| col.to_string()).collect();
    let resolved = resolve_rule_args(table, &cols, args)?;
    let mut assignments = Vec::new();

    for (col, value) in resolved {
        let Some(value) = rule_invoke_value(value)? else {
            continue;
        };
        assignments.push(RuleInvokeAssign {
            col: Arc::<str>::from(col),
            value,
        });
    }

    // Apply-cache bypass: stamp a per-call sequence value into the
    // synthetic `_call_seq` term on every seeded cursor. The body's
    // FactWrite sees a fresh, distinct cursor content each invocation,
    // so content-id-keyed stores (Mem/Sqlite) don't fold repeated
    // `r!.(X, Y)` calls into one row. Bare apply (force=false) keeps
    // the deterministic content-id semantics.
    if force {
        let seq = ctx.next_apply_seq();
        assignments.push(RuleInvokeAssign {
            col: Arc::<str>::from("_call_seq"),
            value: RuleInvokeValue::Literal(Arc::<str>::from(seq.to_string())),
        });
    }

    Ok(Pipe::new().step(Arc::new(RuleInvokeComponent::new(rule, assignments, force))))
}

fn grounded_write_value(table: &str, value: Value) -> Result<WriteValue, LowerError> {
    match value.kind() {
        ValueKind::Atom(value) if value.as_ref() == "&.value" => Ok(WriteValue::Value),
        ValueKind::Atom(value) => Ok(WriteValue::Literal(value.clone())),
        ValueKind::Pipe(pipe) => {
            if let Some(term) = pipe.reads_terms().first() {
                Ok(WriteValue::Term(term.clone()))
            } else if pipe.binds_terms().first().is_some() {
                Err(LowerError::Unknown(format!(
                    "{table}(...): rule apply args must be grounded; TERM? holes are only valid in relation queries"
                )))
            } else {
                Err(LowerError::Unknown(
                    "rule apply args must be atoms or terms".into(),
                ))
            }
        }
        ValueKind::Callable(_) => Err(LowerError::Unknown(
            "rule apply args cannot be a callable".into(),
        )),
    }
}

fn rule_invoke_value(value: Value) -> Result<Option<RuleInvokeValue>, LowerError> {
    match value.kind() {
        ValueKind::Atom(value) if value.as_ref() == "&.value" => Ok(Some(RuleInvokeValue::Value)),
        ValueKind::Atom(value) => Ok(Some(RuleInvokeValue::Literal(value.clone()))),
        ValueKind::Pipe(pipe) => {
            if let Some(term) = pipe.reads_terms().first() {
                Ok(Some(RuleInvokeValue::Term(term.clone())))
            } else if pipe.binds_terms().first().is_some() {
                Err(LowerError::Unknown(
                    "bodied rule apply args must be grounded; TERM? holes are only valid in relation queries".into(),
                ))
            } else {
                Err(LowerError::Unknown(
                    "bodied rule apply args must be atoms or terms".into(),
                ))
            }
        }
        ValueKind::Callable(_) => Err(LowerError::Unknown(
            "bodied rule apply args cannot be a callable".into(),
        )),
    }
}

fn resolve_rule_args(
    table: &str,
    cols: &[String],
    args: &[CallArg],
) -> Result<Vec<(String, Value)>, LowerError> {
    let mut out: Vec<Option<Value>> = vec![None; cols.len()];
    let mut positional_idx = 0usize;
    let mut saw_kw = false;

    for arg in args {
        match &arg.keyword {
            None => {
                let shorthand_idx = same_name_rule_arg_col(cols, &arg.value);
                if saw_kw && shorthand_idx.is_none() {
                    return Err(LowerError::Unknown(format!(
                        "{table}: positional arg after keyword arg"
                    )));
                }
                let idx = match shorthand_idx {
                    Some(idx) => {
                        // Same-name shorthand still consumes the
                        // positional slot. Without bumping
                        // positional_idx past it, the next positional
                        // arg would land back on this column.
                        positional_idx = positional_idx.max(idx + 1);
                        idx
                    }
                    None => {
                        if positional_idx >= cols.len() {
                            return Err(LowerError::Unknown(format!(
                                "rule table `{table}` has {} column(s), call passed too many positional arg(s)",
                                cols.len(),
                            )));
                        }
                        let idx = positional_idx;
                        positional_idx += 1;
                        idx
                    }
                };
                if out[idx].is_some() {
                    return Err(LowerError::Unknown(format!(
                        "{table}: column `{}` assigned more than once",
                        cols[idx],
                    )));
                }
                out[idx] = Some(arg.value.clone());
            }
            Some(keyword) => {
                saw_kw = true;
                let Some(idx) = cols.iter().position(|col| col == keyword.as_ref()) else {
                    return Err(LowerError::Unknown(format!(
                        "{table}: unknown column `{keyword}`"
                    )));
                };
                if out[idx].is_some() {
                    return Err(LowerError::Unknown(format!(
                        "{table}: column `{keyword}` assigned more than once"
                    )));
                }
                out[idx] = Some(arg.value.clone());
            }
        }
    }

    Ok(cols
        .iter()
        .cloned()
        .zip(out)
        .filter_map(|(col, value)| value.map(|value| (col, value)))
        .collect())
}

fn same_name_rule_arg_col(cols: &[String], value: &Value) -> Option<usize> {
    let ValueKind::Pipe(pipe) = value.kind() else {
        return None;
    };
    let binds = pipe.binds_terms();
    let reads = pipe.reads_terms();
    let term = binds.first().or_else(|| reads.first())?;
    cols.iter().position(|col| col == term.as_ref())
}

fn quote_sql_literal(s: &str) -> String {
    let escaped = s.replace('\'', "''");
    format!("'{escaped}'")
}

#[cfg(test)]
mod tests {
    use super::*;
    use effect_runtime::v2::MemFactStore;

    fn cursor(value: &str, kvs: &[(&str, &str)]) -> Cursor {
        let mut c = Cursor::default();
        c.set_value(Arc::<str>::from(value));
        for (k, v) in kvs {
            c.set(k, *v);
        }
        c
    }

    #[test]
    fn referenced_fact_table_scan_reads_from_and_join() {
        let got = referenced_fact_tables(
            "SELECT input.OP FROM input WHERE NOT EXISTS \
             (SELECT 1 FROM frontend_hooks WHERE frontend_hooks.OP = input.OP) \
             JOIN other_rule ON other_rule.OP = input.OP",
        );
        assert_eq!(
            got.into_iter().collect::<Vec<_>>(),
            vec!["frontend_hooks".to_string(), "other_rule".to_string()],
        );
    }

    #[test]
    fn anti_join_emits_only_missing_input_rows() {
        let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
        store.insert(
            "frontend_hooks",
            Arc::new(cursor("hook", &[("OP", "getUser")])),
        );

        let present = cursor("present", &[("OP", "getUser")]);
        let missing = cursor("missing", &[("OP", "listPets")]);
        let batch = vec![&present, &missing];
        let nodes = run_sql_batch(
            store.as_ref(),
            "SELECT input.__cursor_idx, input.OP \
             FROM input \
             WHERE NOT EXISTS ( \
               SELECT 1 FROM frontend_hooks \
               WHERE frontend_hooks.OP = input.OP \
             )",
            &vec!["frontend_hooks".to_string()],
            &batch,
        )
        .unwrap();

        assert_eq!(nodes.len(), 2);
        assert!(matches!(nodes[0], Node::Done));
        match &nodes[1] {
            Node::Emit(c) => assert_eq!(c.get("OP"), Some("listPets")),
            other => panic!("expected one emitted cursor, got {other:?}"),
        }
    }

    #[test]
    fn anti_join_against_declared_empty_table_keeps_declared_columns() {
        let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
        store.declare("frontend_hooks", &["OP"]);

        let missing = cursor("missing", &[("OP", "listPets")]);
        let batch = vec![&missing];
        let nodes = run_sql_batch(
            store.as_ref(),
            "SELECT input.__cursor_idx, input.OP \
             FROM input \
             WHERE NOT EXISTS ( \
               SELECT 1 FROM frontend_hooks \
               WHERE frontend_hooks.OP = input.OP \
             )",
            &vec!["frontend_hooks".to_string()],
            &batch,
        )
        .unwrap();

        match &nodes[0] {
            Node::Emit(c) => assert_eq!(c.get("OP"), Some("listPets")),
            other => panic!("expected one emitted cursor, got {other:?}"),
        }
    }

    // ── Phase 1: SQL batch cache key folds the source clock ─────────

    #[test]
    fn cache_key_invalidates_on_table_clock_bump() {
        use crate::source_clock::{FactStoreClock, SourceClock, SourceId};

        let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
        let clock = FactStoreClock::new(store.clone());
        let c = cursor("x", &[("OP", "getUser")]);
        let batch = vec![&c];
        let tables = vec!["imports".to_string()];
        let sql = "SELECT input.OP FROM input JOIN imports ON imports.OP = input.OP";

        let k0 = sql_batch_cache_key(store.as_ref(), Some(&clock), sql, &tables, &batch);

        // Writing the rule/fact table bumps its source gen → key moves.
        clock.bump(SourceId::for_table("imports"));
        let k1 = sql_batch_cache_key(store.as_ref(), Some(&clock), sql, &tables, &batch);
        assert_ne!(k0, k1, "table clock bump must invalidate the SQL cache key");

        // An unrelated source bump does not move this key.
        clock.bump(SourceId::for_file("/unrelated.rs"));
        let k2 = sql_batch_cache_key(store.as_ref(), Some(&clock), sql, &tables, &batch);
        assert_eq!(k1, k2, "unrelated source bump must not move the key");
    }

    #[test]
    fn file_edit_and_table_write_bump_distinct_source_gens() {
        use crate::source_clock::{FactStoreClock, SourceClock, SourceId};

        let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
        let clock = FactStoreClock::new(store);

        let file = SourceId::for_file("/src/a.rs");
        let table = SourceId::for_table("imports");

        assert_eq!(clock.current_gen(file), 0);
        assert_eq!(clock.current_gen(table), 0);

        // Editing a file bumps that file's gen only.
        clock.bump(file);
        assert_eq!(clock.current_gen(file), 1);
        assert_eq!(clock.current_gen(table), 0);

        // Writing a rule/fact table bumps that table's gen only.
        clock.bump(table);
        assert_eq!(clock.current_gen(table), 1);
        assert_eq!(clock.current_gen(file), 1);
    }
}
