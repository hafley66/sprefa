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

use effect_runtime::v2::{Component, Diag, FactStore, Next, Node, Pipe, Purity, RenderCtx};
use rusqlite::types::{Value as SqlValue, ValueRef};
use rusqlite::{params_from_iter, Connection};

use crate::compile::lower::ctx::{LowerCtx, LowerError};
use crate::compile::lower::op_def::{DslBody, DslShape, InterpKind, InterpMode, OperatorDef};
use crate::compile::lower::value::{CallArg, Value};
use crate::fact::{FactWrite, WriteAssign, WriteValue};
use crate::mounted_query;
use crate::rule::{RuleInvokeAssign, RuleInvokeComponent, RuleInvokeValue};
use crate::sprf_introspect::PipeIntrospect;
use crate::Cursor;

pub struct SqlQueryComponent {
    store: Arc<dyn FactStore<Cursor>>,
    sql: Arc<str>,
    cache: Mutex<BTreeMap<[u8; 32], Vec<Node<Cursor>>>>,
}

impl SqlQueryComponent {
    pub fn new(store: Arc<dyn FactStore<Cursor>>, sql: impl Into<Arc<str>>) -> Self {
        Self {
            store,
            sql: sql.into(),
            cache: Mutex::new(BTreeMap::new()),
        }
    }
}

impl Component for SqlQueryComponent {
    type Next = Cursor;

    fn render_batch(&self, ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        if batch.is_empty() {
            return Vec::new();
        }

        let key = sql_batch_cache_key(self.store.as_ref(), self.sql.as_ref(), batch);
        if let Some(cached) = self.cache.lock().unwrap().get(&key).cloned() {
            mounted_query::record_sql_outputs(
                &self.store,
                self.sql.as_ref(),
                ctx.expand_tick,
                batch,
                &cached,
            );
            return cached;
        }

        let result = match run_sql_batch(self.store.as_ref(), self.sql.as_ref(), batch) {
            Ok(grouped) => grouped,
            Err(e) => {
                ctx.diag.emit(Diag::error("sql/runtime", e));
                batch.iter().map(|_| Node::Done).collect()
            }
        };
        mounted_query::record_sql_outputs(
            &self.store,
            self.sql.as_ref(),
            ctx.expand_tick,
            batch,
            &result,
        );
        self.cache.lock().unwrap().insert(key, result.clone());
        result
    }

    fn purity(&self) -> Purity {
        Purity::Read
    }
}

fn sql_batch_cache_key(
    store: &dyn FactStore<Cursor>,
    sql: &str,
    batch: &[&Cursor],
) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(sql.as_bytes());
    h.update(b"\0input\0");
    for cursor in batch {
        h.update(&cursor.content_hash());
    }
    h.update(b"\0tables\0");
    for table in referenced_fact_tables(sql) {
        h.update(table.as_bytes());
        h.update(b"\0");
        for row in store.rows_of(&table) {
            if let Some(id) = row.get("_id") {
                h.update(id.as_bytes());
            } else {
                h.update(&row.content_hash());
            }
            h.update(b"\0");
        }
    }
    *h.finalize().as_bytes()
}

fn run_sql_batch(
    store: &dyn FactStore<Cursor>,
    sql: &str,
    batch: &[&Cursor],
) -> Result<Vec<Node<Cursor>>, String> {
    let conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
    materialize_input(&conn, batch)?;

    for table in referenced_fact_tables(sql) {
        let rows = store.rows_of(&table);
        let declared_cols = store.declared_cols(&table).unwrap_or_default();
        materialize_fact_table(&conn, &table, &declared_cols, &rows)?;
    }

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

fn materialize_input(conn: &Connection, batch: &[&Cursor]) -> Result<(), String> {
    let mut cols = BTreeSet::new();
    for cursor in batch {
        for (name, _) in &cursor.raw_terms {
            if name.as_ref() != "__cursor_idx" && name.as_ref() != "value" {
                cols.insert(name.to_string());
            }
        }
    }

    let mut ordered = vec!["__cursor_idx".to_string(), "value".to_string()];
    ordered.extend(cols);
    create_text_table(conn, "input", &ordered, Some("__cursor_idx"))?;

    for (idx, cursor) in batch.iter().enumerate() {
        let values: Vec<SqlValue> = ordered
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
            .collect();
        insert_row(conn, "input", &ordered, values)?;
    }

    Ok(())
}

fn materialize_fact_table(
    conn: &Connection,
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
        for (name, _) in &row.raw_terms {
            if name.as_ref() != "value" {
                cols.insert(name.to_string());
            }
        }
    }

    let mut ordered = vec!["value".to_string()];
    ordered.extend(cols);
    create_text_table(conn, table, &ordered, None)?;

    for row in rows {
        let values: Vec<SqlValue> = ordered
            .iter()
            .map(|col| {
                if col == "value" {
                    SqlValue::Text(row.value.to_string())
                } else {
                    SqlValue::Text(row.get(col).unwrap_or("").to_string())
                }
            })
            .collect();
        insert_row(conn, table, &ordered, values)?;
    }

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

fn insert_row(
    conn: &Connection,
    table: &str,
    cols: &[String],
    values: Vec<SqlValue>,
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
    conn.execute(&sql, params_from_iter(values.iter()))
        .map_err(|e| e.to_string())?;
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

fn quote_ident(s: &str) -> String {
    let escaped = s.replace('"', "\"\"");
    format!("\"{escaped}\"")
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
            InterpKind::Term {
                mode: InterpMode::Read,
                field,
            } => {
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
            InterpKind::Term {
                mode: InterpMode::Bind,
                ..
            } => {
                return Err(LowerError::Unknown(
                    "sql: bind interpolation is not valid inside sql bodies".into(),
                ));
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
        let sql = rewrite_sql_interps(dsl)?;
        Ok(Pipe::new().step(Arc::new(SqlQueryComponent::new(ctx.store.clone(), sql))))
    }
}

pub fn rule_table_call_pipe(
    ctx:       &LowerCtx,
    table:     &str,
    predicate: bool,
    args:      &[CallArg],
) -> Result<Pipe<Cursor>, LowerError> {
    if predicate {
        return Err(LowerError::Unknown(format!(
            "{table}?(...): rule predicate syntax is outside the locked V4 surface; use a grounded {table}(...) relation query"
        )));
    }
    let cols = ctx.store.declared_cols(table).ok_or_else(|| {
        LowerError::Unknown(format!("rule table `{table}` is not declared"))
    })?;
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
        match arg {
            Value::Atom(value) => {
                if value.as_ref() == "&.value" {
                    modes.push(ArgMode::BoundTerm { col, term: "value".to_string() });
                } else {
                    modes.push(ArgMode::BoundLiteral {
                        col,
                        value: value.to_string(),
                    });
                }
            }
            Value::Pipe(pipe) => {
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
            if let ArgMode::Project { col, term } = mode {
                select_cols.push(format!(
                    "{}.{} AS {}",
                    quote_ident(rule_alias),
                    quote_ident(col),
                    quote_ident(term),
                ));
            }
        }
    }

    let mut predicates = Vec::new();
    for mode in &modes {
        match mode {
            ArgMode::BoundTerm { col, term } => {
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
            ArgMode::Project { .. } => {}
        }
    }

    let all_grounded = !modes.is_empty() && modes.iter().all(|m| !matches!(m, ArgMode::Project { .. }));
    let select_prefix = if all_grounded { "SELECT DISTINCT" } else { "SELECT" };
    let mut sql = format!(
        "{select_prefix} {}\nFROM input JOIN {} AS {} ON 1=1",
        select_cols.join(", "),
        table,
        rule_alias,
    );
    if !predicates.is_empty() {
        sql.push_str("\nWHERE ");
        sql.push_str(&predicates.join(" AND "));
    }

    Ok(Pipe::new().step(Arc::new(SqlQueryComponent::new(
        ctx.store.clone(),
        sql,
    ))))
}

pub fn rule_apply_write_pipe(
    ctx:   &LowerCtx,
    table: &str,
    args:  &[CallArg],
) -> Result<Pipe<Cursor>, LowerError> {
    let cols = ctx.store.declared_cols(table).ok_or_else(|| {
        LowerError::Unknown(format!("rule table `{table}` is not declared"))
    })?;
    let resolved = resolve_rule_args(table, &cols, args)?;
    if resolved.is_empty() {
        return Ok(Pipe::new().step(Arc::new(FactWrite::new(
            ctx.store.clone(),
            Arc::<str>::from(table),
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
        Arc::<str>::from(table),
        assignments,
    ))))
}

pub fn rule_write_pipe(
    ctx:  &LowerCtx,
    args: &[CallArg],
) -> Result<Pipe<Cursor>, LowerError> {
    let Some((first, rest)) = args.split_first() else {
        return Err(LowerError::Unknown("rule write requires a :table arg".into()));
    };
    if first.keyword.is_some() {
        return Err(LowerError::Unknown("rule write table arg must be positional".into()));
    }
    let table = match &first.value {
        Value::Atom(s) => s.clone(),
        _ => return Err(LowerError::Unknown("rule write first arg must be a :table atom".into())),
    };
    if rest.is_empty() {
        return Ok(Pipe::new().step(Arc::new(FactWrite::new(
            ctx.store.clone(),
            table,
        ))));
    }
    let cols = ctx.store.declared_cols(&table).ok_or_else(|| {
        LowerError::Unknown(format!("rule table `{table}` is not declared"))
    })?;
    let resolved = resolve_rule_args(&table, &cols, rest)?;
    let mut assignments = Vec::with_capacity(resolved.len());
    for (col, value) in resolved {
        let value = match value {
            Value::Atom(value) if value.as_ref() == "&.value" => WriteValue::Value,
            Value::Atom(value) => WriteValue::Literal(value),
            Value::Pipe(pipe) => {
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
        };
        assignments.push(WriteAssign {
            col: Arc::<str>::from(col),
            value,
        });
    }
    Ok(Pipe::new().step(Arc::new(FactWrite::projected(
        ctx.store.clone(),
        table,
        assignments,
    ))))
}

pub fn rule_body_call_pipe(
    ctx: &LowerCtx,
    table: &str,
    force: bool,
    args: &[CallArg],
) -> Result<Pipe<Cursor>, LowerError> {
    let rule = ctx.get_rule(table).ok_or_else(|| {
        LowerError::Unknown(format!("bodied rule `{table}` is not declared"))
    })?;
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

    Ok(Pipe::new().step(Arc::new(RuleInvokeComponent::new(
        rule,
        assignments,
        force,
    ))))
}

fn grounded_write_value(table: &str, value: Value) -> Result<WriteValue, LowerError> {
    match value {
        Value::Atom(value) if value.as_ref() == "&.value" => Ok(WriteValue::Value),
        Value::Atom(value) => Ok(WriteValue::Literal(value)),
        Value::Pipe(pipe) => {
            if let Some(term) = pipe.reads_terms().first() {
                Ok(WriteValue::Term(term.clone()))
            } else if pipe.binds_terms().first().is_some() {
                Err(LowerError::Unknown(format!(
                    "{table}.(...): rule apply args must be grounded; TERM? holes are only valid in relation queries"
                )))
            } else {
                Err(LowerError::Unknown(
                    "rule apply args must be atoms or terms".into(),
                ))
            }
        }
    }
}

fn rule_invoke_value(value: Value) -> Result<Option<RuleInvokeValue>, LowerError> {
    match value {
        Value::Atom(value) if value.as_ref() == "&.value" => Ok(Some(RuleInvokeValue::Value)),
        Value::Atom(value) => Ok(Some(RuleInvokeValue::Literal(value))),
        Value::Pipe(pipe) => {
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
                if saw_kw {
                    return Err(LowerError::Unknown(format!(
                        "{table}: positional arg after keyword arg"
                    )));
                }
                if positional_idx >= cols.len() {
                    return Err(LowerError::Unknown(format!(
                        "rule table `{table}` has {} column(s), call passed too many positional arg(s)",
                        cols.len(),
                    )));
                }
                if out[positional_idx].is_some() {
                    return Err(LowerError::Unknown(format!(
                        "{table}: column `{}` assigned more than once",
                        cols[positional_idx],
                    )));
                }
                out[positional_idx] = Some(arg.value.clone());
                positional_idx += 1;
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

    Ok(cols.iter()
        .cloned()
        .zip(out)
        .filter_map(|(col, value)| value.map(|value| (col, value)))
        .collect())
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
        let mut c = Cursor {
            value: Arc::from(value),
            ..Default::default()
        };
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
            &batch,
        )
        .unwrap();

        match &nodes[0] {
            Node::Emit(c) => assert_eq!(c.get("OP"), Some("listPets")),
            other => panic!("expected one emitted cursor, got {other:?}"),
        }
    }
}
