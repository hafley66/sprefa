// The tick engine, a port of v6/tsv2/runtime/1_incremental.ts. The seam is
// blocking rusqlite and the whole engine is plain sync (async is realized at
// the driver's spawn + channel + StreamExt); this is the v6 law "sync stays
// sync, in-memory row work is plain sync Vec" with the async boundary carried
// by tokio at the root.

use std::collections::HashMap;

use crate::sql::{result_rows, SqlRunner, SqliteSeam};
use crate::types::{
    Arrival, ArrivalSign, BoundaryError, BoundaryResult, IncrementalRelationPlan, RelationKind,
    Row, ScalarSeam, ScalarValue, SqlStatement, Value,
};
use crate::write_verbs::{write_verbs_for, TickBoundary};

#[derive(Clone)]
pub struct DeltaEvent {
    pub rel: String,
    pub sign: i8,
    pub sequence: u64,
    pub row: Row,
}

/// Columns in one probe statement. SQLite's default column ceiling is 2000 and
/// each column here is a shallow EXISTS.
const PROBE_WIDTH: usize = 800;

/// Columns `probe_columns` returns per rel.
const PROBE_COLUMNS: usize = 4;

/// Which level operator a head ran under, so the two passes over the same head
/// share one reading of "did my inputs move since I last ran".
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum LevelPhase {
    /// apply_levels_before_edges and apply_levels_after_edges: the delta insert.
    Insert,
    /// recompute_levels_before_edges and recompute_levels_after_edges: the
    /// from-base refcount re-derive.
    Recount,
}

/// Which rels hold transient rows this tick, so a phase touches only those.
/// Every write marks and nothing unmarks: over-approximating wastes a statement.
pub struct TickWork {
    /// Rel -> the tick clock reading at its last write. The key set is the moved
    /// set; a rel absent here has not been written this tick.
    moved_at: std::cell::RefCell<HashMap<String, u64>>,
    /// Rel -> the tick clock reading at its last deleted row.
    shrank_at: std::cell::RefCell<HashMap<String, u64>>,
    /// Rel -> the tick clock reading at its last added row.
    grew_at: std::cell::RefCell<HashMap<String, u64>>,
    /// Monotone inside one tick. A level operator's rows are a function of the
    /// rels it reads, so a re-run with no newer input restages nothing.
    clock: std::cell::Cell<u64>,
    ran_at: std::cell::RefCell<HashMap<(LevelPhase, String), u64>>,
    /// `unskipped`: no tick tables to read, so every gate answers yes.
    ungated: bool,
    carry: std::collections::HashSet<String>,
    stale: std::collections::HashSet<String>,
    departures: std::cell::RefCell<std::collections::HashSet<String>>,
    /// Rels this tick wrote a frontier row for; the merge and the promote move
    /// rows between exactly these tables and leave the rest empty.
    staged_current: std::cell::RefCell<std::collections::HashSet<String>>,
    staged_next: std::cell::RefCell<std::collections::HashSet<String>>,
}

fn probe_columns(relation: &IncrementalRelationPlan) -> [String; PROBE_COLUMNS] {
    let exists = |table: &str| format!("EXISTS(SELECT 1 FROM {})", quote_identifier(table));
    [
        exists(&relation.frontier_table_name),
        format!(
            "{} OR {}",
            exists(&relation.delta_table_name),
            exists(&relation.next_frontier_table_name)
        ),
        match &relation.departure_frontier_table_name {
            Some(table) => exists(table),
            None => "0".to_string(),
        },
        format!(
            "EXISTS(SELECT 1 FROM {} WHERE \"_sign\" = -1)",
            quote_identifier(&relation.delta_table_name)
        ),
    ]
}

impl TickWork {
    /// One chunked read at the top of the tick. Nothing but a tick writes these
    /// tables, so one reading holds until this tick's own writes mark.
    pub fn probe(seam: &SqliteSeam, relations: &[IncrementalRelationPlan]) -> TickWork {
        let mut work = TickWork {
            moved_at: std::cell::RefCell::new(HashMap::new()),
            shrank_at: std::cell::RefCell::new(HashMap::new()),
            grew_at: std::cell::RefCell::new(HashMap::new()),
            clock: std::cell::Cell::new(0),
            ran_at: std::cell::RefCell::new(HashMap::new()),
            ungated: false,
            carry: std::collections::HashSet::new(),
            stale: std::collections::HashSet::new(),
            departures: std::cell::RefCell::new(std::collections::HashSet::new()),
            staged_current: std::cell::RefCell::new(std::collections::HashSet::new()),
            staged_next: std::cell::RefCell::new(std::collections::HashSet::new()),
        };
        if relations.is_empty() {
            return work;
        }
        let columns: Vec<String> = relations.iter().flat_map(probe_columns).collect();
        let mut answers: Vec<bool> = Vec::with_capacity(columns.len());
        let _scope =
            crate::trace::Scope::verb("probe", "-", crate::write_verbs::strategy_name(relations));
        for chunk in columns.chunks(PROBE_WIDTH) {
            let result = seam
                .execute(&SqlStatement {
                    sql: format!("SELECT {}", chunk.join(", ")),
                    args: vec![],
                })
                .expect("tick probe failed");
            let row = result.rows.first().expect("tick probe row");
            answers.extend(row.iter().map(|value| value.as_i64().unwrap_or(0) != 0));
        }
        for (index, relation) in relations.iter().enumerate() {
            let at = index * PROBE_COLUMNS;
            if answers[at] {
                work.carry.insert(relation.rel.clone());
                work.mark_grew(&relation.rel);
            }
            if answers[at + 1] {
                work.stale.insert(relation.rel.clone());
            }
            if answers[at + 2] {
                work.departures.borrow_mut().insert(relation.rel.clone());
                work.mark_shrank(&relation.rel);
            }
            if answers[at + 3] {
                work.carry_shrink(&relation.rel);
            }
        }
        work
    }

    /// The maximal reading: every rel might have moved, so no phase skips. The
    /// door for a caller that drives one phase without a whole tick's tables.
    pub fn unskipped(relations: &[IncrementalRelationPlan]) -> TickWork {
        let names = || -> std::collections::HashSet<String> {
            relations
                .iter()
                .map(|relation| relation.rel.clone())
                .collect()
        };
        TickWork {
            moved_at: std::cell::RefCell::new(names().into_iter().map(|rel| (rel, 1u64)).collect()),
            shrank_at: std::cell::RefCell::new(HashMap::new()),
            grew_at: std::cell::RefCell::new(HashMap::new()),
            clock: std::cell::Cell::new(1),
            ran_at: std::cell::RefCell::new(HashMap::new()),
            ungated: true,
            carry: names(),
            stale: names(),
            departures: std::cell::RefCell::new(
                relations
                    .iter()
                    .filter(|relation| relation.departure_frontier_table_name.is_some())
                    .map(|relation| relation.rel.clone())
                    .collect(),
            ),
            staged_current: std::cell::RefCell::new(names()),
            staged_next: std::cell::RefCell::new(names()),
        }
    }

    pub fn mark(&self, rel: &str) {
        let clock = self.clock.get() + 1;
        self.clock.set(clock);
        stamp(&self.moved_at, rel, clock);
    }

    pub fn mark_shrank(&self, rel: &str) {
        self.mark(rel);
        stamp(&self.shrank_at, rel, self.clock.get());
    }

    pub fn mark_grew(&self, rel: &str) {
        self.mark(rel);
        stamp(&self.grew_at, rel, self.clock.get());
    }

    fn carry_shrink(&self, rel: &str) {
        let clock = self.clock.get() + 1;
        self.clock.set(clock);
        stamp(&self.shrank_at, rel, clock);
    }

    pub fn moved(&self, rel: &str) -> bool {
        self.moved_at.borrow().contains_key(rel)
    }

    /// A frontier row landed. `next` picks the carry table over the current one.
    fn note_frontier_write(&self, rel: &str, next: bool) {
        let mut staged = if next {
            self.staged_next.borrow_mut()
        } else {
            self.staged_current.borrow_mut()
        };
        if !staged.contains(rel) {
            staged.insert(rel.to_string());
        }
    }

    /// The carry table holds rows only where this tick wrote them; nothing else
    /// fills it and `prepare_tick` emptied it.
    fn carries(&self, rel: &str) -> bool {
        self.staged_next.borrow().contains(rel)
    }

    /// Either frontier table holds a row: this tick's writes plus the carry the
    /// tick before promoted.
    fn holds_frontier(&self, rel: &str) -> bool {
        self.carry.contains(rel)
            || self.staged_current.borrow().contains(rel)
            || self.staged_next.borrow().contains(rel)
    }

    /// True when one of `rels` was written after this head's last run under
    /// `phase`, and records this run's reading when it is.
    fn moved_since_run(&self, head: &str, phase: LevelPhase, rels: &[String]) -> bool {
        if self.ungated {
            return true;
        }
        let last = self
            .ran_at
            .borrow()
            .get(&(phase, head.to_string()))
            .copied()
            .unwrap_or(0);
        let fresh = {
            let moved_at = self.moved_at.borrow();
            rels.iter()
                .any(|rel| moved_at.get(rel).is_some_and(|at| *at > last))
        };
        if fresh {
            self.note_run(head, phase);
        }
        fresh
    }

    fn recount_needed(&self, reads: &LevelSources) -> bool {
        if self.ungated || reads.recount_always {
            return true;
        }
        let shrank_at = self.shrank_at.borrow();
        let grew_at = self.grew_at.borrow();
        reads.positive.iter().any(|rel| shrank_at.contains_key(rel))
            || reads
                .negated
                .iter()
                .any(|rel| grew_at.contains_key(rel) || shrank_at.contains_key(rel))
    }

    fn note_run(&self, head: &str, phase: LevelPhase) {
        if self.ungated {
            return;
        }
        self.ran_at
            .borrow_mut()
            .insert((phase, head.to_string()), self.clock.get());
    }

    fn departed(&self, rel: &str) -> bool {
        self.departures.borrow().contains(rel)
    }
}

fn stamp(table: &std::cell::RefCell<HashMap<String, u64>>, rel: &str, clock: u64) {
    let mut table = table.borrow_mut();
    match table.get_mut(rel) {
        Some(at) => *at = clock,
        None => {
            table.insert(rel.to_string(), clock);
        }
    }
}

/// Rows dedup by an index, never by scanning what is already collected. The
/// counter is what a COUNT test reads: one probe per row, never one per pair.
static DEDUP_PROBES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn dedup_probes() -> u64 {
    DEDUP_PROBES.load(std::sync::atomic::Ordering::Relaxed)
}

/// Equal rows render equal text because every float reaching here has been
/// normalized (-0.0 folded to 0.0) and validated finite.
pub fn dedup_key(row: &[Value]) -> String {
    DEDUP_PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{row:?}")
}

/// One comparison per probe under the index, one per (statement, relation)
/// pair under a scan.
/// Level statements a tick actually ran, so a COUNT test can read that a level
/// whose sources did not move paid nothing.
static LEVEL_RUNS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn level_runs() -> u64 {
    LEVEL_RUNS.load(std::sync::atomic::Ordering::Relaxed)
}

static PLAN_PROBES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn plan_probes() -> u64 {
    PLAN_PROBES.load(std::sync::atomic::Ordering::Relaxed)
}

fn plan_index(relations: &[IncrementalRelationPlan]) -> HashMap<&str, &IncrementalRelationPlan> {
    relations.iter().map(|r| (r.rel.as_str(), r)).collect()
}

fn plan_for<'a>(
    index: &HashMap<&str, &'a IncrementalRelationPlan>,
    rel: &str,
    missing: &str,
) -> &'a IncrementalRelationPlan {
    PLAN_PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    index
        .get(rel)
        .copied()
        .unwrap_or_else(|| panic!("{missing}"))
}

/// One substring search per (statement, relation) pair the frontier scan walks.
static FRONTIER_PROBES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn frontier_probes() -> u64 {
    FRONTIER_PROBES.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn values_sql(row_count: usize, column_count: usize) -> String {
    let row = format!("({})", vec!["?"; column_count].join(", "));
    vec![row; row_count].join(", ")
}

pub fn json_array_text(items: &[Value]) -> BoundaryResult<String> {
    let parts: Vec<String> = items
        .iter()
        .map(value_to_json)
        .collect::<BoundaryResult<Vec<_>>>()?;
    Ok(format!("[{}]", parts.join(",")))
}

fn value_to_json(value: &Value) -> BoundaryResult<String> {
    if let Value::Bytes(bytes) = value {
        return Ok(format!(
            "{{\"$bytes\":{}}}",
            crate::ticklog::json_string(&crate::types::bytes_to_base64(bytes))
        ));
    }
    Ok(
        match ScalarValue::at_seam(value, ScalarSeam::ArrivalPayload)? {
            ScalarValue::Integer(v) => format!("{}", v),
            ScalarValue::Real(v) => crate::ticklog::js_float_text(v),
            ScalarValue::Bool(b) => (if b { "true" } else { "false" }).to_string(),
            ScalarValue::Text(v) => crate::ticklog::json_string(&v),
            ScalarValue::Bytes(_) => unreachable!("bytes cannot reach arrival JSON staging"),
        },
    )
}

fn bind_args(values: &[Value]) -> BoundaryResult<Vec<ScalarValue>> {
    values
        .iter()
        .map(
            |value| match ScalarValue::at_seam(value, ScalarSeam::SqlParameter)? {
                ScalarValue::Bool(b) => Ok(ScalarValue::Integer(if b { 1 } else { 0 })),
                other => Ok(other),
            },
        )
        .collect()
}

/// The staged JSON straight off the borrowed rows: no Vec<Value> is built per
/// event to carry the two leading integers.
fn staged_json(
    prefix: [i64; 2],
    events: &[&DeltaEvent],
    sequence_only: bool,
) -> BoundaryResult<String> {
    let mut out = String::new();
    out.push('[');
    for (index, event) in events.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('[');
        if !sequence_only {
            out.push_str(&format!("{},", prefix[0]));
        }
        out.push_str(&format!("{}", event.sequence as i64));
        for value in &event.row {
            out.push(',');
            out.push_str(&value_to_json(value)?);
        }
        out.push(']');
    }
    out.push(']');
    let _ = prefix[1];
    Ok(out)
}

pub(crate) fn boundary_stage_statement(
    relation: &IncrementalRelationPlan,
    events: &[&DeltaEvent],
) -> BoundaryResult<SqlStatement> {
    if relation
        .column_types
        .contains(&crate::types::RowColumnType::Bytes)
    {
        return direct_stage_statement(relation, &relation.delta_table_name, false, 0, events);
    }
    let mut columns = vec!["_sign".to_string(), "_sequence".to_string()];
    columns.extend(relation.columns.clone());
    let columns_text: Vec<String> = columns.iter().map(|c| quote_identifier(c)).collect();
    let value_expressions: Vec<String> = columns
        .iter()
        .enumerate()
        .map(|(index, _)| format!("json_extract(value, '$[{}]')", index))
        .collect();
    let mut encoded = String::new();
    encoded.push('[');
    for (index, event) in events.iter().enumerate() {
        if index > 0 {
            encoded.push(',');
        }
        encoded.push_str(&format!("[{},{}", event.sign as i64, event.sequence as i64));
        for value in &event.row {
            encoded.push(',');
            encoded.push_str(&value_to_json(value)?);
        }
        encoded.push(']');
    }
    encoded.push(']');
    Ok(SqlStatement {
        sql: format!(
            "INSERT INTO {} ({}) SELECT {} FROM json_each(?)",
            quote_identifier(&relation.delta_table_name),
            columns_text.join(", "),
            value_expressions.join(", ")
        ),
        args: vec![ScalarValue::Text(encoded)],
    })
}

// Shared-mode stage: resolve each event row to its durable __id and write
// (relation_id, phase, sequence, row_id), one batched statement per rel.
fn shared_frontier_stage_statement(
    relation: &IncrementalRelationPlan,
    table_name: &str,
    phase: i64,
    events: &[&DeltaEvent],
) -> BoundaryResult<SqlStatement> {
    let relation_id = relation
        .shared_frontier
        .as_ref()
        .expect("shared_frontier plan missing")
        .relation_id;
    let shared_table = if table_name == relation.next_frontier_table_name {
        "__next_frontier"
    } else {
        "__frontier"
    };
    let join_terms: Vec<String> = relation
        .columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            format!(
                "t.{} IS json_extract(je.value, '$[{}]')",
                quote_identifier(column),
                index + 1
            )
        })
        .collect();
    let on_sql = if join_terms.is_empty() {
        "1".to_string()
    } else {
        join_terms.join(" AND ")
    };
    let encoded = staged_json([0, 0], events, true)?;
    Ok(SqlStatement {
        sql: format!(
            "INSERT INTO {} (\"relation_id\", \"_phase\", \"_sequence\", \"row_id\") SELECT ?, ?, json_extract(je.value, '$[0]'), t.\"__id\" FROM json_each(?) je JOIN {} t ON {}",
            quote_identifier(shared_table),
            quote_identifier(&relation.table_name),
            on_sql
        ),
        args: vec![
            ScalarValue::Integer(relation_id),
            ScalarValue::Integer(phase),
            ScalarValue::Text(encoded),
        ],
    })
}

pub(crate) fn frontier_stage_statement(
    relation: &IncrementalRelationPlan,
    table_name: &str,
    phase: i64,
    events: &[&DeltaEvent],
) -> BoundaryResult<SqlStatement> {
    if relation.shared_frontier.is_some() {
        return shared_frontier_stage_statement(relation, table_name, phase, events);
    }
    if relation
        .column_types
        .contains(&crate::types::RowColumnType::Bytes)
    {
        return direct_stage_statement(relation, table_name, true, phase, events);
    }
    let mut columns = vec!["_phase".to_string(), "_sequence".to_string()];
    columns.extend(relation.columns.clone());
    let columns_text: Vec<String> = columns.iter().map(|c| quote_identifier(c)).collect();
    let value_expressions: Vec<String> = columns
        .iter()
        .enumerate()
        .map(|(index, _)| format!("json_extract(value, '$[{}]')", index))
        .collect();
    let encoded = staged_json([phase, 0], events, false)?;
    Ok(SqlStatement {
        sql: format!(
            "INSERT INTO {} ({}) SELECT {} FROM json_each(?)",
            quote_identifier(table_name),
            columns_text.join(", "),
            value_expressions.join(", ")
        ),
        args: vec![ScalarValue::Text(encoded)],
    })
}

fn direct_stage_statement(
    relation: &IncrementalRelationPlan,
    table_name: &str,
    frontier: bool,
    phase: i64,
    events: &[&DeltaEvent],
) -> BoundaryResult<SqlStatement> {
    let mut columns = if frontier {
        vec!["_phase".to_string(), "_sequence".to_string()]
    } else {
        vec!["_sign".to_string(), "_sequence".to_string()]
    };
    columns.extend(relation.columns.clone());
    let columns_text: Vec<String> = columns.iter().map(|c| quote_identifier(c)).collect();
    let mut args = Vec::new();
    for event in events {
        args.push(ScalarValue::Integer(if frontier {
            phase
        } else {
            event.sign as i64
        }));
        args.push(ScalarValue::Integer(event.sequence as i64));
        args.extend(bind_args(&event.row)?);
    }
    Ok(SqlStatement {
        sql: format!(
            "INSERT INTO {} ({}) VALUES {}",
            quote_identifier(table_name),
            columns_text.join(", "),
            values_sql(events.len(), columns.len())
        ),
        args,
    })
}

pub fn stage_events(
    seam: &SqliteSeam,
    relations: &[IncrementalRelationPlan],
    events: &[DeltaEvent],
    frontier_copies: &[(String, i64)],
    work: &TickWork,
) -> BoundaryResult<()> {
    if events.is_empty() {
        return Ok(());
    }
    for event in events {
        match event.sign {
            -1 => work.mark_shrank(&event.rel),
            _ => work.mark_grew(&event.rel),
        }
    }
    let relation_by_name: HashMap<&str, &IncrementalRelationPlan> =
        relations.iter().map(|r| (r.rel.as_str(), r)).collect();
    let mut events_by_rel: HashMap<&str, Vec<&DeltaEvent>> = HashMap::new();
    for event in events {
        events_by_rel
            .entry(event.rel.as_str())
            .or_default()
            .push(event);
    }
    let verbs = write_verbs_for(relations);
    let strategy = crate::write_verbs::strategy_name(relations);
    for (rel, grouped) in &events_by_rel {
        let relation = relation_by_name
            .get(rel)
            .expect("incremental delta relation missing");
        let mut scope = crate::trace::Scope::verb("stage", &relation.rel, strategy);
        scope.rows(grouped.len());
        let statements = verbs.stage(relation, grouped, frontier_copies)?;
        seam.batch(&statements).expect("stage_events batch failed");
        // The frontier copies carry ADDITIONS only, the way `stage` writes them.
        if grouped.iter().any(|event| event.sign == 1) {
            note_frontier_copies(relation, frontier_copies, work);
        }
    }
    Ok(())
}

fn note_frontier_copies(
    relation: &IncrementalRelationPlan,
    frontier_copies: &[(String, i64)],
    work: &TickWork,
) {
    for (table_name, _) in frontier_copies {
        work.note_frontier_write(
            &relation.rel,
            *table_name == relation.next_frontier_table_name,
        );
    }
}

fn storage_row(relation: &IncrementalRelationPlan, row: &Row) -> Row {
    row.iter()
        .enumerate()
        .map(|(index, value)| {
            if relation.column_types.get(index) == Some(&crate::types::RowColumnType::Bool) {
                if let Value::Bool(b) = value {
                    Value::Integer(if *b { 1 } else { 0 })
                } else {
                    value.clone()
                }
            } else {
                value.clone()
            }
        })
        .collect()
}

fn row_key(row: &Row, indices: &[usize]) -> BoundaryResult<String> {
    let values: Vec<String> = indices
        .iter()
        .map(|index| value_to_json(&row[*index]))
        .collect::<BoundaryResult<Vec<_>>>()?;
    Ok(format!("[{}]", values.join(",")))
}

fn keyed_arrival_rows_statement(
    relation: &IncrementalRelationPlan,
    entries: &[(u64, Row)],
    key_indices: &[usize],
) -> BoundaryResult<SqlStatement> {
    let columns_text: Vec<String> = relation
        .columns
        .iter()
        .map(|c| quote_identifier(c))
        .collect();
    let key_columns_text: Vec<String> = key_indices
        .iter()
        .map(|index| columns_text[*index].clone())
        .collect();
    let mut distinct_keys: Vec<Row> = Vec::new();
    let mut seen_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (_, row) in entries {
        let key: Row = key_indices
            .iter()
            .map(|index| row[*index].clone())
            .collect();
        if seen_keys.insert(dedup_key(&key)) {
            distinct_keys.push(key);
        }
    }
    let mut args: Vec<ScalarValue> = Vec::new();
    for key in &distinct_keys {
        args.extend(bind_args(key)?);
    }
    Ok(SqlStatement {
        sql: format!(
            "SELECT {} FROM {} WHERE ({}) IN ({})",
            columns_text.join(", "),
            quote_identifier(&relation.table_name),
            key_columns_text.join(", "),
            values_sql(distinct_keys.len(), key_columns_text.len())
        ),
        args,
    })
}

pub(crate) fn direct_arrival_statement(
    relation: &IncrementalRelationPlan,
    sign: i8,
    rows: &[&Row],
) -> BoundaryResult<SqlStatement> {
    let columns: Vec<String> = relation
        .columns
        .iter()
        .map(|c| quote_identifier(c))
        .collect();
    let args = flat_bind_args(rows)?;
    if sign < 0 {
        return Ok(SqlStatement {
            sql: format!(
                "DELETE FROM {} WHERE ({}) IN ({}) RETURNING {}",
                quote_identifier(&relation.table_name),
                columns.join(", "),
                values_sql(rows.len(), columns.len()),
                columns.join(", ")
            ),
            args,
        });
    }
    let (prefix, conflict) = if relation.key_indices.is_empty() {
        ("INSERT OR IGNORE", String::new())
    } else {
        let keys: Vec<String> = relation
            .key_indices
            .iter()
            .map(|index| columns[*index].clone())
            .collect();
        let non_keys: Vec<String> = columns
            .iter()
            .enumerate()
            .filter(|(index, _)| !relation.key_indices.contains(index))
            .map(|(_, col)| col.clone())
            .collect();
        let conflict = if non_keys.is_empty() {
            format!(" ON CONFLICT({}) DO NOTHING", keys.join(", "))
        } else {
            format!(
                " ON CONFLICT({}) DO UPDATE SET {}",
                keys.join(", "),
                non_keys
                    .iter()
                    .map(|column| format!("{} = excluded.{}", column, column))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        ("INSERT", conflict)
    };
    Ok(SqlStatement {
        sql: format!(
            "{} INTO {} ({}) VALUES {}{} RETURNING {}",
            prefix,
            quote_identifier(&relation.table_name),
            columns.join(", "),
            values_sql(rows.len(), columns.len()),
            conflict,
            columns.join(", ")
        ),
        args,
    })
}

// Port of IncrementalRuntime.apply_arrivals. Groups consecutive same-rel/sign
// arrivals and writes them through the relation's arrival_add/arrival_del SQL.
/// The margin covers the handful of non-row placeholders a statement can carry
/// beyond its rows.
const VARIABLE_BUDGET_MARGIN: usize = 2_766;

pub fn apply_arrivals(
    seam: &SqliteSeam,
    arrivals: &[Arrival],
    relations: &[IncrementalRelationPlan],
    work: &TickWork,
) -> BoundaryResult<()> {
    if arrivals.is_empty() {
        return Ok(());
    }
    let relation_by_name: HashMap<&str, &IncrementalRelationPlan> =
        relations.iter().map(|r| (r.rel.as_str(), r)).collect();
    // Group consecutive same rel+sign.
    type ArrivalGroup<'a> = (&'a IncrementalRelationPlan, i8, Vec<(u64, Row)>);

    // Runs of consecutive same rel+sign, NOT one group per (rel, sign). Folding
    // every run of a rel together was measured and is slower: it took the
    // dead-module rail from 1787 statements to 1027 and from 1.63s to 1.72s,
    // because one IN-list of 15000 placeholders costs more than the fifty
    // small statements it replaced. Statement count is not the thing to
    // minimize here.
    let mut groups: Vec<ArrivalGroup> = Vec::new();
    for (sequence, arrival) in arrivals.iter().enumerate() {
        let relation = relation_by_name
            .get(arrival.rel.as_str())
            .expect("incremental arrival relation missing");
        let sign: i8 = match arrival.sign {
            ArrivalSign::Add => 1,
            ArrivalSign::Del => -1,
        };
        let entry = (sequence as u64, storage_row(relation, &arrival.row));
        if let Some(last) = groups.last_mut() {
            if last.0.rel == relation.rel && last.1 == sign {
                last.2.push(entry);
                continue;
            }
        }
        groups.push((relation, sign, vec![entry]));
    }
    // SQLite binds at most SQLITE_MAX_VARIABLE_NUMBER placeholders per
    // statement (32766 since 3.32). Both writers below expand one placeholder
    // per column per row, so a group has a ROW ceiling set by its widest
    // statement. Exceeding it is a hard `too many SQL variables` stop, not a
    // slow path: 22163 rows keyed on two columns is 44326 placeholders. The
    // former run-based grouping never hit it only because interleaved arrivals
    // happened to keep every run small.
    let verbs = write_verbs_for(relations);
    let variable_budget = seam
        .variable_limit()
        .saturating_sub(VARIABLE_BUDGET_MARGIN)
        .max(1);
    // Groups are one per (rel, sign), chunked to the variable budget. The ratio
    // to arrivals.len() is the thing to read.
    let span = tracing::info_span!(
        "apply_arrivals",
        arrivals = arrivals.len(),
        groups = groups.len()
    );
    let _entered = span.enter();
    // A run is still unbounded, so the variable budget is enforced whatever the
    // arrival order does. Chunked here rather than by rebuilding the group
    // list, so no row is copied a second time.
    let chunked = groups.into_iter().flat_map(|(relation, sign, entries)| {
        let widest = relation
            .columns
            .len()
            .max(relation.key_indices.len())
            .max(1);
        let rows_per_statement = (variable_budget / widest).max(1);
        let mut pieces = Vec::new();
        let mut rest = entries;
        while rest.len() > rows_per_statement {
            let tail = rest.split_off(rows_per_statement);
            pieces.push((relation, sign, rest));
            rest = tail;
        }
        pieces.push((relation, sign, rest));
        pieces
    });
    let strategy = crate::write_verbs::strategy_name(relations);
    for (relation, sign, entries) in chunked {
        let mut scope = crate::trace::Scope::verb("arrive", &relation.rel, strategy);
        scope.rows(entries.len());
        let write_statement = {
            let _encode = crate::trace::Scope::phase("arrive_encode");
            verbs.arrive(
                relation,
                sign,
                &entries.iter().map(|(_, row)| row).collect::<Vec<&Row>>(),
            )?
        };
        let key_indices = relation.key_indices.clone();
        if relation.kind == RelationKind::Set && sign == 1 && !key_indices.is_empty() {
            let before_result = seam
                .execute(&keyed_arrival_rows_statement(
                    relation,
                    &entries,
                    &key_indices,
                )?)
                .expect("keyed arrival rows lookup failed");
            let _diff = crate::trace::Scope::phase("arrive_diff");
            let before_rows =
                result_rows(&before_result, &relation.columns, &relation.column_types)?;
            let mut current_by_key: HashMap<String, Row> = HashMap::new();
            for row in &before_rows {
                current_by_key.insert(row_key(row, &key_indices)?, row.clone());
            }
            let mut events = Vec::new();
            for (sequence, row) in &entries {
                let key = row_key(row, &key_indices)?;
                let before = current_by_key.get(&key);
                let same = before.map(|b| rows_equal(b, row)).unwrap_or(false);
                if same {
                    continue;
                }
                if let Some(before) = before {
                    events.push(DeltaEvent {
                        rel: relation.rel.clone(),
                        sign: -1,
                        sequence: sequence * 2,
                        row: before.clone(),
                    });
                }
                events.push(DeltaEvent {
                    rel: relation.rel.clone(),
                    sign: 1,
                    sequence: sequence * 2 + 1,
                    row: row.clone(),
                });
                current_by_key.insert(key, row.clone());
            }
            drop(_diff);
            let result = seam
                .execute(&write_statement)
                .expect("arrival write failed");
            let _ = result.rows_affected;
            stage_events(
                seam,
                std::slice::from_ref(relation),
                &events,
                &[(relation.frontier_table_name.clone(), 1)],
                work,
            )?;
            continue;
        }
        let result = seam
            .execute(&write_statement)
            .expect("arrival write failed");
        let mut events = Vec::new();
        if relation.kind == RelationKind::Log && sign == 1 {
            for (sequence, row) in &entries {
                events.push(DeltaEvent {
                    rel: relation.rel.clone(),
                    sign: 1,
                    sequence: *sequence,
                    row: row.clone(),
                });
            }
            stage_events(
                seam,
                std::slice::from_ref(relation),
                &events,
                &[(relation.frontier_table_name.clone(), 1)],
                work,
            )?;
            continue;
        }
        let _diff = crate::trace::Scope::phase("arrive_diff");
        let changed_rows = result_rows(&result, &relation.columns, &relation.column_types)?;
        let mut staged_rows: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (index, (sequence, _)) in entries.iter().enumerate() {
            let stored_row = changed_rows.get(index);
            let Some(stored_row) = stored_row else {
                continue;
            };
            let row_text = json_array_text(stored_row)?;
            if !staged_rows.insert(row_text) {
                continue;
            }
            events.push(DeltaEvent {
                rel: relation.rel.clone(),
                sign,
                sequence: *sequence,
                row: stored_row.clone(),
            });
        }
        drop(_diff);
        stage_events(
            seam,
            std::slice::from_ref(relation),
            &events,
            &[(relation.frontier_table_name.clone(), 1)],
            work,
        )?;
    }
    Ok(())
}

fn rows_equal(left: &Row, right: &Row) -> bool {
    left == right
}

/// The `pre/1` plane, filled once per tick after the arrivals land and before
/// the level phase: `pre(level_head(..))` reads last tick's settled rows.
pub fn snapshot_pre(
    seam: &SqliteSeam,
    pre_rels: &[String],
    relations: &[IncrementalRelationPlan],
) -> BoundaryResult<()> {
    if pre_rels.is_empty() {
        return Ok(());
    }
    let plans = plan_index(relations);
    let mut statements = Vec::new();
    for rel in pre_rels {
        let relation = plan_for(&plans, rel, "pre snapshot relation missing");
        let columns = relation
            .columns
            .iter()
            .map(|column| quote_identifier(column))
            .collect::<Vec<_>>()
            .join(", ");
        let pre_table = quote_identifier(&format!("__pre_{}", relation.table_name));
        statements.push(format!("DELETE FROM {pre_table}"));
        statements.push(format!(
            "INSERT INTO {} ({}) SELECT {} FROM {}",
            pre_table,
            columns,
            columns,
            quote_identifier(&relation.table_name)
        ));
    }
    let _scope = crate::trace::Scope::verb(
        "snapshot_pre",
        "-",
        crate::write_verbs::strategy_name(relations),
    );
    seam.execute_multiple(&statements.join(";\n"))
        .expect("pre snapshot failed");
    Ok(())
}

// One statement per tick: the clock the oracle fixes for the whole tick.
pub fn advance_tick(seam: &SqliteSeam) {
    let _scope = crate::trace::Scope::phase("advance_tick");
    seam.execute_multiple("UPDATE \"__tick\" SET \"n\" = \"n\" + 1")
        .expect("advance_tick failed");
}

// Only a rel whose delta or next frontier held rows has anything to empty.
pub fn prepare_tick(seam: &SqliteSeam, relations: &[IncrementalRelationPlan], work: &TickWork) {
    let stale: Vec<IncrementalRelationPlan> = relations
        .iter()
        .filter(|relation| work.stale.contains(&relation.rel))
        .cloned()
        .collect();
    if stale.is_empty() {
        return;
    }
    let _scope = crate::trace::Scope::verb(
        "clear",
        "prepare",
        crate::write_verbs::strategy_name(relations),
    );
    let sql = write_verbs_for(relations)
        .clear(&stale, TickBoundary::Prepare)
        .join(";\n");
    if sql.is_empty() {
        return;
    }
    seam.execute_multiple(&sql).expect("prepare_tick failed");
}

// Port of boundary_delta: sum sign*count over each relation's delta table and
// split into add/del rows.
pub fn boundary_delta(
    relation: &IncrementalRelationPlan,
    result: &crate::types::QueryResult,
) -> BoundaryResult<crate::types::RelDelta> {
    let sign_index = result.columns.iter().position(|c| c == "__sign");
    let count_index = result.columns.iter().position(|c| c == "__count");
    let mut weights: Vec<(Row, i64)> = Vec::new();
    let mut weight_index: HashMap<String, usize> = HashMap::new();
    for row in &result.rows {
        let mut values = Vec::new();
        for (index, _column) in relation.columns.iter().enumerate() {
            let value = row
                .get(index)
                .cloned()
                .unwrap_or(Value::Text(String::new()));
            values.push(normalize_boundary(
                value,
                relation.column_types.get(index).copied(),
            )?);
        }
        let sign = sign_index
            .and_then(|i| row.get(i))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let count = count_index
            .and_then(|i| row.get(i))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let weight = sign * count;
        match weight_index.entry(dedup_key(&values)) {
            std::collections::hash_map::Entry::Occupied(seen) => weights[*seen.get()].1 += weight,
            std::collections::hash_map::Entry::Vacant(fresh) => {
                fresh.insert(weights.len());
                weights.push((values, weight));
            }
        }
    }
    let mut add = Vec::new();
    let mut del = Vec::new();
    for (row, weight) in weights {
        for _ in 0..weight.max(0) {
            add.push(row.clone());
        }
        for _ in 0..(-weight).max(0) {
            del.push(row.clone());
        }
    }
    Ok(crate::types::RelDelta {
        rel: relation.rel.clone(),
        add,
        del,
    })
}

fn normalize_boundary(
    value: Value,
    ty: Option<crate::types::RowColumnType>,
) -> BoundaryResult<Value> {
    match (ty, value) {
        (Some(crate::types::RowColumnType::Bool), Value::Integer(v)) => Ok(Value::Bool(v != 0)),
        (Some(crate::types::RowColumnType::Bool), v) => Ok(v),
        (Some(crate::types::RowColumnType::Float), Value::Real(v)) => {
            if !v.is_finite() {
                panic!("float column crossed SQLite with non-finite value");
            }
            Ok(Value::Real(if v == 0.0 { 0.0 } else { v }))
        }
        // F3 mirror of the SELECT boundary in sql.rs: a list column hands the
        // consumer Vec<Value>, never the array text.
        (Some(crate::types::RowColumnType::List), Value::Text(text)) => {
            match serde_json::from_str::<Vec<serde_json::Value>>(&text) {
                Ok(items) => Ok(Value::List(items)),
                Err(error) => Err(crate::types::BoundaryError::ListColumnNotAnArray {
                    text,
                    detail: error.to_string(),
                }),
            }
        }
        (Some(crate::types::RowColumnType::Bytes), Value::Bytes(bytes)) => Ok(Value::Bytes(bytes)),
        (_, v) => Ok(v),
    }
}

// A rel nothing wrote this tick has an empty delta table, and its delta is
// empty without a read.
pub fn read_boundary(
    seam: &SqliteSeam,
    relations: &[IncrementalRelationPlan],
    work: &TickWork,
) -> BoundaryResult<Vec<crate::types::RelDelta>> {
    relations
        .iter()
        .map(|relation| {
            if !work.moved(&relation.rel) {
                return Ok(crate::types::RelDelta {
                    rel: relation.rel.clone(),
                    add: vec![],
                    del: vec![],
                });
            }
            let mut scope = crate::trace::Scope::verb(
                "publish",
                &relation.rel,
                crate::write_verbs::strategy_name(relations),
            );
            let result = seam
                .execute(&write_verbs_for(relations).publish(relation))
                .expect("boundary read failed");
            scope.rows(result.rows.len());
            boundary_delta(relation, &result)
        })
        .collect()
}

pub fn stage_departures(
    seam: &SqliteSeam,
    relations: &[IncrementalRelationPlan],
    deltas: &[crate::types::RelDelta],
    work: &TickWork,
) -> BoundaryResult<()> {
    let departures_by_rel: HashMap<&str, &[Row]> = deltas
        .iter()
        .map(|delta| {
            PLAN_PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            (delta.rel.as_str(), delta.del.as_slice())
        })
        .collect();
    let mut statements = Vec::new();
    for relation in relations {
        PLAN_PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let Some(table_name) = &relation.departure_frontier_table_name else {
            continue;
        };
        let departed = departures_by_rel
            .get(relation.rel.as_str())
            .copied()
            .unwrap_or_default();
        // Nothing to clear and nothing to write is a statement about an empty
        // table.
        if departed.is_empty() && !work.departed(&relation.rel) {
            continue;
        }
        statements.push(SqlStatement {
            sql: format!("DELETE FROM {}", quote_identifier(table_name)),
            args: vec![],
        });
        if departed.is_empty() {
            work.departures.borrow_mut().remove(&relation.rel);
            continue;
        }
        work.departures.borrow_mut().insert(relation.rel.clone());
        let mut columns = vec!["_phase".to_string(), "_sequence".to_string()];
        columns.extend(relation.columns.clone());
        let quoted_columns: Vec<String> = columns
            .iter()
            .map(|column| quote_identifier(column))
            .collect();
        let value_expressions: Vec<String> = columns
            .iter()
            .enumerate()
            .map(|(index, _)| format!("json_extract(value, '$[{}]')", index))
            .collect();
        let encoded = departed
            .iter()
            .enumerate()
            .map(|(sequence, row)| {
                let mut staged = vec![Value::Integer(0), Value::Integer(sequence as i64)];
                staged.extend(row.clone());
                json_array_text(&staged)
            })
            .collect::<BoundaryResult<Vec<_>>>()?
            .join(",");
        statements.push(SqlStatement {
            sql: format!(
                "INSERT INTO {} ({}) SELECT {} FROM json_each(?)",
                quote_identifier(table_name),
                quoted_columns.join(", "),
                value_expressions.join(", ")
            ),
            args: vec![ScalarValue::Text(format!("[{}]", encoded))],
        });
    }
    if !statements.is_empty() {
        let _scope = crate::trace::Scope::verb(
            "stage_departures",
            "-",
            crate::write_verbs::strategy_name(relations),
        );
        seam.batch(&statements).expect("departure staging failed");
    }
    Ok(())
}

// Port of promote_frontiers: read carry, promote next into current. Only a rel
// whose frontier or next frontier holds rows has anything to move.
pub fn promote_frontiers(
    seam: &SqliteSeam,
    relations: &[IncrementalRelationPlan],
    work: &TickWork,
) -> bool {
    let moved: Vec<IncrementalRelationPlan> = relations
        .iter()
        .filter(|relation| work.holds_frontier(&relation.rel))
        .cloned()
        .collect();
    if moved.is_empty() {
        return !work.departures.borrow().is_empty();
    }
    let verbs = write_verbs_for(relations);
    let strategy = crate::write_verbs::strategy_name(relations);
    let promote_sql = verbs.clear(&moved, TickBoundary::Promote).join(";\n");
    let promote = || {
        let _scope = crate::trace::Scope::verb("clear", "promote", strategy);
        if !promote_sql.is_empty() {
            seam.execute_multiple(&promote_sql).expect("promote failed");
        }
    };
    let carry_sql = verbs.read_staged(&moved);
    if carry_sql.is_empty() {
        promote();
        return !work.departures.borrow().is_empty();
    }
    let carry_pending = {
        let _scope = crate::trace::Scope::verb("read_staged", "-", strategy);
        seam.scalar(&carry_sql).expect("carry read failed") == 1
    };
    promote();
    carry_pending || !work.departures.borrow().is_empty()
}

// ═══ level phases (port of 1_incremental.ts apply_level_* + reconcile_*) ═══

// Both statements read the same rows, so the intern arm runs in the same
// ordered batch as the statement whose write depends on it.
fn intern_then_execute(
    seam: &SqliteSeam,
    intern_sql: Option<&Vec<String>>,
    statement: &SqlStatement,
) -> crate::types::QueryResult {
    let Some(intern_sql) = intern_sql.filter(|sqls| !sqls.is_empty()) else {
        return seam.execute(statement).expect("statement failed");
    };
    let mut batch: Vec<SqlStatement> = intern_sql
        .iter()
        .map(|sql| SqlStatement {
            sql: sql.clone(),
            args: statement.args.clone(),
        })
        .collect();
    batch.push(statement.clone());
    let mut results = seam.batch(&batch).expect("intern batch failed");
    results.pop().expect("intern batch produced no result")
}

fn to_statements(texts: &[String]) -> Vec<SqlStatement> {
    texts
        .iter()
        .map(|text| SqlStatement {
            sql: text.clone(),
            args: vec![],
        })
        .collect()
}

/// Which rels one level head reads, and whether it reads a table belonging to no
/// rel at all, which is the case that never skips.
#[derive(Clone)]
pub struct LevelSources {
    pub rels: Vec<String>,
    pub positive: Vec<String>,
    pub negated: Vec<String>,
    pub recount_always: bool,
    pub always: bool,
    /// The head reads its own frontier, directly or around a cycle, so the rows
    /// one round stages are the next round's input rather than its output.
    pub self_feeding: bool,
}

/// A table that owns no rel and never forces a run: the global text intern, the
/// clock, and the per-program meta row.
fn global_table(name: &str) -> bool {
    name == "__str" || name == "__tick" || name == "__meta" || name.starts_with("__str_")
}

fn level_statement_texts(statement: &crate::types::IncrementalLevelStatement) -> Vec<&str> {
    let mut texts: Vec<&str> = Vec::new();
    texts.extend(statement.insert_sql.as_deref());
    for group in [
        statement.support_sql.as_ref(),
        statement.support_intern_sql.as_ref(),
        statement.intern_sql.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        texts.extend(group.iter().map(String::as_str));
    }
    if let Some(plan) = &statement.support_count_sql {
        texts.push(&plan.clear_sql);
        texts.extend(plan.write_sqls.iter().map(String::as_str));
    }
    if let Some(aggregate) = &statement.aggregate_sql {
        texts.push(&aggregate.scope_clear_sql);
        texts.push(&aggregate.delete_scoped_sql);
        texts.extend(aggregate.scope_seed_sql.iter().map(String::as_str));
        texts.extend(aggregate.insert_scoped_sql.iter().map(String::as_str));
        if let Some(intern) = &aggregate.intern_sql {
            texts.extend(intern.iter().map(String::as_str));
        }
    }
    texts
}

fn quoted_tables(sql: &str) -> Vec<&str> {
    let mut names = Vec::new();
    for keyword in ["FROM ", "JOIN ", "INTO ", "UPDATE "] {
        for (at, _) in sql.match_indices(keyword) {
            let rest = sql[at + keyword.len()..].trim_start();
            let Some(rest) = rest.strip_prefix('"') else {
                continue;
            };
            if let Some(end) = rest.find('"') {
                names.push(&rest[..end]);
            }
        }
    }
    names
}

fn negated_spans(sql: &str) -> Option<Vec<std::ops::Range<usize>>> {
    let bytes = sql.as_bytes();
    let mut spans = Vec::new();
    for (at, _) in sql.match_indices("NOT EXISTS") {
        let mut cursor = at + "NOT EXISTS".len();
        while bytes.get(cursor) == Some(&b' ') {
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&b'(') {
            continue;
        }
        let open = cursor;
        let mut depth = 0usize;
        let mut quoted = false;
        while cursor < bytes.len() {
            match bytes[cursor] {
                b'\'' => quoted = !quoted,
                b'(' if !quoted => depth += 1,
                b')' if !quoted => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            cursor += 1;
        }
        if depth != 0 {
            return None;
        }
        spans.push(open..cursor + 1);
    }
    Some(spans)
}

/// Every table name a rel owns, so a level's read of any of them counts as a
/// read of that rel.
fn table_owners(relations: &[IncrementalRelationPlan]) -> HashMap<String, &str> {
    let mut owners = HashMap::new();
    for relation in relations {
        let rel = relation.rel.as_str();
        let mut names = vec![
            relation.table_name.clone(),
            relation.delta_table_name.clone(),
            relation.frontier_table_name.clone(),
            relation.next_frontier_table_name.clone(),
        ];
        names.extend(relation.departure_frontier_table_name.clone());
        for prefix in [
            "__txt_",
            "__new_",
            "__pre_",
            "__support_",
            "__support_next_",
            "__agg_scope_",
            "__avg_",
        ] {
            names.push(format!("{prefix}{}", relation.table_name));
        }
        for name in names {
            owners.insert(name, rel);
        }
    }
    owners
}

/// One substring pass per program, never per tick: the answer is metadata.
/// `cyclic` is `recursive_heads`, taken as an argument so the frontier scan
/// behind it stays one scan per program.
pub fn level_sources(
    statements: &[crate::types::IncrementalLevelStatement],
    relations: &[IncrementalRelationPlan],
    cyclic: &[String],
) -> HashMap<String, LevelSources> {
    let owners = table_owners(relations);
    let mut sources: HashMap<String, LevelSources> = HashMap::new();
    for statement in statements {
        let self_feeding = cyclic.contains(&statement.head_rel);
        let entry = sources
            .entry(statement.head_rel.clone())
            .or_insert_with(|| LevelSources {
                rels: Vec::new(),
                positive: Vec::new(),
                negated: Vec::new(),
                recount_always: false,
                always: false,
                self_feeding,
            });
        for text in level_statement_texts(statement) {
            for table in quoted_tables(text) {
                match owners.get(table) {
                    Some(rel) => entry.rels.push((*rel).to_string()),
                    None if global_table(table) => {}
                    None => entry.always = true,
                }
            }
        }
        entry.rels.sort();
        entry.rels.dedup();
        if let Some(rederive) = statement.support_sql.as_ref().and_then(|sql| sql.get(1)) {
            let Some(spans) = negated_spans(rederive) else {
                entry.recount_always = true;
                continue;
            };
            for relation in relations {
                let table = quote_identifier(&relation.table_name);
                for (at, _) in rederive.match_indices(&table) {
                    if spans.iter().any(|span| span.contains(&at)) {
                        entry.negated.push(relation.rel.clone());
                    } else {
                        entry.positive.push(relation.rel.clone());
                    }
                }
            }
        }
        entry.positive.sort();
        entry.positive.dedup();
        entry.negated.sort();
        entry.negated.dedup();
    }
    sources
}

fn level_runs_this_tick(
    sources: &HashMap<String, LevelSources>,
    statement: &crate::types::IncrementalLevelStatement,
    work: &TickWork,
    phase: LevelPhase,
) -> bool {
    let head = statement.head_rel.as_str();
    match sources.get(head) {
        Some(reads) if !reads.always => work.moved_since_run(head, phase, &reads.rels),
        _ => {
            work.note_run(head, phase);
            true
        }
    }
}

fn recount_runs_this_tick(
    sources: &HashMap<String, LevelSources>,
    statement: &crate::types::IncrementalLevelStatement,
    work: &TickWork,
) -> bool {
    let head = statement.head_rel.as_str();
    let runs = std::env::var_os("DL_NO_SHRINK_GATE").is_some()
        || match sources.get(head) {
            Some(reads) => work.recount_needed(reads),
            None => true,
        };
    if runs {
        work.note_run(head, LevelPhase::Recount);
    }
    runs
}

/// The rows a run stages are its own output, so the reading it records is taken
/// after it. Only a head reading its own frontier takes them back as input.
fn settle_level_run(
    sources: &HashMap<String, LevelSources>,
    statement: &crate::types::IncrementalLevelStatement,
    work: &TickWork,
    phase: LevelPhase,
) {
    let head = statement.head_rel.as_str();
    if sources.get(head).is_some_and(|reads| reads.self_feeding) {
        return;
    }
    work.note_run(head, phase);
}

/// A head reads a rel when one of `tables` (quoted table name, rel) occurs in
/// its SQL: frontier tables for recursion, base tables for the ordered dirty set.
pub fn reads_by_head<'a>(
    heads: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
    tables: &[(String, &str)],
) -> Vec<(String, Vec<String>)> {
    let mut reads: Vec<(String, Vec<String>)> = Vec::new();
    let mut source_index: HashMap<&str, usize> = HashMap::new();
    for (head_rel, sql) in heads {
        let mut sources = Vec::new();
        if let Some(sql) = sql {
            for (table, rel) in tables {
                FRONTIER_PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if sql.contains(table.as_str()) {
                    sources.push((*rel).to_string());
                }
            }
        }
        match source_index.get(head_rel) {
            Some(index) => reads[*index].1.extend(sources),
            None => {
                source_index.insert(head_rel, reads.len());
                reads.push((head_rel.to_string(), sources));
            }
        }
    }
    reads
}

/// Program metadata, so the substring pass over every insert text runs at
/// construction and never once per tick.
pub fn recursive_heads(
    statements: &[crate::types::IncrementalLevelStatement],
    relations: &[IncrementalRelationPlan],
) -> Vec<String> {
    let frontiers: Vec<(String, &str)> = relations
        .iter()
        .map(|relation| {
            (
                quote_identifier(&relation.frontier_table_name),
                relation.rel.as_str(),
            )
        })
        .collect();
    let reads_frontier_of = reads_by_head(
        statements
            .iter()
            .map(|statement| (statement.head_rel.as_str(), statement.insert_sql.as_deref())),
        &frontiers,
    );
    fn reaches(
        from: &str,
        target: &str,
        seen: &mut Vec<String>,
        map: &[(String, Vec<String>)],
    ) -> bool {
        if seen.iter().any(|s| s == from) {
            return false;
        }
        seen.push(from.to_string());
        for source in map
            .iter()
            .find(|e| e.0 == from)
            .map(|e| &e.1)
            .unwrap_or(&vec![])
        {
            if source == target {
                return true;
            }
            if reaches(source, target, seen, map) {
                return true;
            }
        }
        false
    }
    let mut heads = Vec::new();
    for (head, _) in &reads_frontier_of {
        if reaches(head, head, &mut Vec::new(), &reads_frontier_of) {
            heads.push(head.clone());
        }
    }
    heads
}

// Maximal runs of consecutive statements sharing one `recursion_group.group`.
// A statement off any cycle is its own run and pays exactly one pass.
fn level_statement_runs(
    statements: &[&crate::types::IncrementalLevelStatement],
) -> Vec<std::ops::Range<usize>> {
    let mut runs: Vec<std::ops::Range<usize>> = Vec::new();
    let mut current_group: Option<u64> = None;
    for (index, statement) in statements.iter().enumerate() {
        let group = statement.recursion_group.as_ref().map(|plan| plan.group);
        if group.is_some() && group == current_group {
            runs.last_mut().expect("run started").end = index + 1;
            continue;
        }
        runs.push(index..index + 1);
        current_group = group;
    }
    runs
}

// OUTER ROUNDS: a mutual cycle has no statement order that reaches the least
// fixpoint in one pass, so the group's pass repeats until no row moves.
fn sequence_level_rounds(
    statements: &[&crate::types::IncrementalLevelStatement],
    mut run: impl FnMut(&crate::types::IncrementalLevelStatement) -> BoundaryResult<usize>,
) -> BoundaryResult<()> {
    for range in level_statement_runs(statements) {
        let group = &statements[range.clone()];
        let plan = group[0].recursion_group.clone();
        let Some(plan) = plan else {
            run(group[0])?;
            continue;
        };
        for round in 0..plan.round_cap {
            let round_span = crate::trace::round_span(round as u64);
            let _entered = round_span.enter();
            let mut moved = 0usize;
            for statement in group {
                moved += run(statement)?;
            }
            round_span.record("delta_rows", moved);
            if moved == 0 {
                break;
            }
            if round + 1 >= plan.round_cap {
                return Err(BoundaryError::DivergingMeasureRecursion {
                    rel: plan.heads.clone(),
                    round_cap: plan.round_cap,
                });
            }
        }
    }
    Ok(())
}

// The insert path: runs the level's insert_sql (with RETURNING) and stages the
// produced rows as +1 events into the frontier for this pass.
fn apply_level_statement(
    seam: &SqliteSeam,
    statement: &crate::types::IncrementalLevelStatement,
    relations: &[IncrementalRelationPlan],
    plans: &HashMap<&str, &IncrementalRelationPlan>,
    after_edges: bool,
    next_sequence: &mut dyn FnMut() -> u64,
    work: &TickWork,
) -> BoundaryResult<usize> {
    let relation = plan_for(
        plans,
        &statement.head_rel,
        "incremental level head relation missing",
    );
    LEVEL_RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if let Some(aggregate) = &statement.aggregate_sql {
        apply_aggregate_level_statement(
            seam,
            statement,
            aggregate,
            relation,
            after_edges,
            next_sequence,
            work,
        )?;
        return Ok(0);
    }
    let insert_sql = statement
        .insert_sql
        .as_ref()
        .expect("level statement has no insert_sql");
    let mut scope = crate::trace::Scope::verb(
        "level_insert",
        &statement.head_rel,
        crate::write_verbs::strategy_name(relations),
    );
    let result = intern_then_execute(
        seam,
        statement.intern_sql.as_ref(),
        &SqlStatement {
            sql: insert_sql.clone(),
            args: vec![],
        },
    );
    let rows = result_rows(
        &result,
        &statement.head_columns,
        &statement.head_column_types,
    )?;
    scope.rows(rows.len());
    drop(scope);
    if rows.is_empty() {
        return Ok(0);
    }
    let events: Vec<DeltaEvent> = rows
        .iter()
        .map(|row| DeltaEvent {
            rel: statement.head_rel.clone(),
            sign: 1,
            sequence: next_sequence(),
            row: row.clone(),
        })
        .collect();
    let copies = level_frontier_copies(relation, after_edges);
    stage_events(seam, std::slice::from_ref(relation), &events, &copies, work)?;
    Ok(rows.len())
}

// The DELETE and every INSERT return only the rows of the AFFECTED GROUPS, so
// the head re-derives without a full-table read on either side of the seam.
fn apply_aggregate_level_statement(
    seam: &SqliteSeam,
    statement: &crate::types::IncrementalLevelStatement,
    aggregate: &crate::types::AggregateLevelPlan,
    relation: &IncrementalRelationPlan,
    after_edges: bool,
    next_sequence: &mut dyn FnMut() -> u64,
    work: &TickWork,
) -> BoundaryResult<()> {
    // The intern arm reads the scope table, so it follows the seed inside the
    // same ordered batch and precedes the insert that looks its ids back up.
    let mut scope = crate::trace::Scope::verb(
        "aggregate",
        &statement.head_rel,
        crate::write_verbs::relation_strategy(relation),
    );
    let mut scope_texts = vec![aggregate.scope_clear_sql.clone()];
    scope_texts.extend(aggregate.scope_seed_sql.clone());
    scope_texts.extend(aggregate.intern_sql.clone().unwrap_or_default());
    seam.batch(&to_statements(&scope_texts))
        .expect("aggregate scope batch failed");
    let delete_result = seam
        .execute(&SqlStatement {
            sql: aggregate.delete_scoped_sql.clone(),
            args: vec![],
        })
        .expect("aggregate scoped delete failed");
    let removed_rows = result_rows(
        &delete_result,
        &statement.head_columns,
        &statement.head_column_types,
    )?;
    let insert_results = seam
        .batch(&to_statements(&aggregate.insert_scoped_sql))
        .expect("aggregate scoped insert failed");
    let mut events: Vec<DeltaEvent> = removed_rows
        .iter()
        .map(|row| DeltaEvent {
            rel: statement.head_rel.clone(),
            sign: -1,
            sequence: next_sequence(),
            row: row.clone(),
        })
        .collect();
    for insert_result in &insert_results {
        for row in result_rows(
            insert_result,
            &statement.head_columns,
            &statement.head_column_types,
        )? {
            events.push(DeltaEvent {
                rel: statement.head_rel.clone(),
                sign: 1,
                sequence: next_sequence(),
                row,
            });
        }
    }
    scope.rows(events.len());
    drop(scope);
    if events.is_empty() {
        return Ok(());
    }
    let copies = level_frontier_copies(relation, after_edges);
    stage_events(seam, std::slice::from_ref(relation), &events, &copies, work)
}

pub fn apply_retention(
    seam: &SqliteSeam,
    statements: &[crate::types::IncrementalRetentionStatement],
    relations: &[IncrementalRelationPlan],
    work: &TickWork,
) -> BoundaryResult<()> {
    let plans = plan_index(relations);
    let mut sequence = 0u64;
    for statement in statements {
        let relation = plan_for(
            &plans,
            &statement.rel,
            "incremental retention relation missing",
        );
        let mut scope = crate::trace::Scope::verb(
            "retention",
            &statement.rel,
            crate::write_verbs::strategy_name(relations),
        );
        let result = seam
            .execute(&SqlStatement {
                sql: statement.delete_sql.clone(),
                args: vec![],
            })
            .expect("retention delete failed");
        let rows = result_rows(&result, &relation.columns, &relation.column_types)?;
        scope.rows(rows.len());
        drop(scope);
        if rows.is_empty() {
            continue;
        }
        let events: Vec<DeltaEvent> = rows
            .iter()
            .map(|row| {
                let current = sequence;
                sequence += 1;
                DeltaEvent {
                    rel: statement.rel.clone(),
                    sign: -1,
                    sequence: current,
                    row: row.clone(),
                }
            })
            .collect();
        stage_events(seam, std::slice::from_ref(relation), &events, &[], work)?;
    }
    Ok(())
}

// After the edge boundary a level row must reach BOTH the current frontier
// (this pass) and the next one (the carry), the way TICK PHASE ALIGNMENT sets.
fn level_frontier_copies(
    relation: &IncrementalRelationPlan,
    after_edges: bool,
) -> Vec<(String, i64)> {
    if after_edges {
        vec![
            (relation.frontier_table_name.clone(), 2),
            (relation.next_frontier_table_name.clone(), 1),
        ]
    } else {
        vec![(relation.frontier_table_name.clone(), 2)]
    }
}

// apply_levels_before_edges: dependency-ordered insert pass. A head on a level
// cycle uses the support-count reconcile so the cycle closes in one pass.
pub fn apply_levels_before_edges(
    seam: &SqliteSeam,
    statements: &[crate::types::IncrementalLevelStatement],
    relations: &[IncrementalRelationPlan],
    feeds_another_round: &[String],
    work: &TickWork,
    sources: &HashMap<String, LevelSources>,
) -> BoundaryResult<()> {
    if statements.is_empty() {
        return Ok(());
    }
    let plans = plan_index(relations);
    let mut sequence = 0u64;
    let mut next_sequence = || {
        let current = sequence;
        sequence += 1;
        current
    };
    let ordered: Vec<&crate::types::IncrementalLevelStatement> = statements.iter().collect();
    sequence_level_rounds(&ordered, |statement| {
        let closes_in_one_pass = feeds_another_round.iter().any(|h| h == &statement.head_rel)
            && statement.support_sql.is_some();
        let phase = if closes_in_one_pass {
            LevelPhase::Recount
        } else {
            LevelPhase::Insert
        };
        if !level_runs_this_tick(sources, statement, work, phase) {
            return Ok(0);
        }
        let moved = if closes_in_one_pass {
            let copies: Vec<(String, i64)> = relations
                .iter()
                .filter(|r| r.rel == statement.head_rel)
                .map(|r| (r.frontier_table_name.clone(), 2))
                .collect();
            reconcile_ref_count_statement(seam, statement, relations, &plans, &copies, work)?
        } else {
            apply_level_statement(
                seam,
                statement,
                relations,
                &plans,
                false,
                &mut next_sequence,
                work,
            )?
        };
        settle_level_run(sources, statement, work, phase);
        Ok(moved)
    })
}

// ═══ edge phases (port of 1_incremental.ts apply_edges + merge/after) ═══════

fn edge_columns_text(statement: &crate::types::IncrementalEdgeStatement) -> Vec<String> {
    statement
        .head_columns
        .iter()
        .map(|column| quote_identifier(column))
        .collect()
}

fn edge_keyed_rows_sql(
    statement: &crate::types::IncrementalEdgeStatement,
    row_count: usize,
) -> String {
    let columns = edge_columns_text(statement);
    let key_columns: Vec<String> = statement
        .key_indices
        .iter()
        .map(|index| columns[*index].clone())
        .collect();
    format!(
        "SELECT {} FROM {} WHERE ({}) IN ({})",
        columns.join(", "),
        quote_identifier(&statement.head_table_name),
        key_columns.join(", "),
        values_sql(row_count, key_columns.len())
    )
}

fn edge_keyed_write_statement(
    statement: &crate::types::IncrementalEdgeStatement,
    rows: &[Row],
) -> BoundaryResult<SqlStatement> {
    let columns = edge_columns_text(statement);
    let key_columns: Vec<String> = statement
        .key_indices
        .iter()
        .map(|index| columns[*index].clone())
        .collect();
    let non_key_columns: Vec<String> = columns
        .iter()
        .enumerate()
        .filter(|(index, _)| !statement.key_indices.contains(index))
        .map(|(_, column)| column.clone())
        .collect();
    let conflict = if non_key_columns.is_empty() {
        format!("ON CONFLICT({}) DO NOTHING", key_columns.join(", "))
    } else {
        let sets: Vec<String> = non_key_columns
            .iter()
            .map(|column| format!("{} = excluded.{}", column, column))
            .collect();
        format!(
            "ON CONFLICT({}) DO UPDATE SET {}",
            key_columns.join(", "),
            sets.join(", ")
        )
    };
    Ok(SqlStatement {
        sql: format!(
            "INSERT INTO {} ({}) VALUES {} {}",
            quote_identifier(&statement.head_table_name),
            columns.join(", "),
            values_sql(rows.len(), columns.len()),
            conflict
        ),
        args: flat_bind_args(rows)?,
    })
}

fn flat_bind_args<R: AsRef<[Value]>>(rows: &[R]) -> BoundaryResult<Vec<ScalarValue>> {
    let mut args = Vec::new();
    for row in rows {
        args.extend(bind_args(row.as_ref())?);
    }
    Ok(args)
}

fn edge_log_write_statement(
    statement: &crate::types::IncrementalEdgeStatement,
    rows: &[Row],
) -> BoundaryResult<SqlStatement> {
    let columns = edge_columns_text(statement);
    Ok(SqlStatement {
        sql: format!(
            "INSERT INTO {} ({}) VALUES {}",
            quote_identifier(&statement.head_table_name),
            columns.join(", "),
            values_sql(rows.len(), columns.len())
        ),
        args: flat_bind_args(rows)?,
    })
}

/// Where an edge write's events go. A sequenced arm collects, because a row it
/// writes and then overwrites inside the same tick must not carry.
enum EdgeSink<'a> {
    Stage,
    Collect(&'a mut Vec<DeltaEvent>),
}

impl EdgeSink<'_> {
    fn take(
        &mut self,
        seam: &SqliteSeam,
        relation: &IncrementalRelationPlan,
        events: Vec<DeltaEvent>,
        work: &TickWork,
    ) -> BoundaryResult<()> {
        match self {
            EdgeSink::Stage => stage_events(
                seam,
                std::slice::from_ref(relation),
                &events,
                &[(relation.next_frontier_table_name.clone(), 0)],
                work,
            ),
            EdgeSink::Collect(collected) => {
                for event in &events {
                    match event.sign {
                        -1 => work.mark_shrank(&event.rel),
                        _ => work.mark_grew(&event.rel),
                    }
                }
                collected.extend(events);
                Ok(())
            }
        }
    }
}

// `base` is 0 for a set-at-once arm and the running occurrence counter for a
// sequenced one, so the frontier keeps arrival order across the whole walk.
fn apply_log_edge(
    seam: &SqliteSeam,
    statement: &crate::types::IncrementalEdgeStatement,
    relation: &IncrementalRelationPlan,
    rows: &[Row],
    base: u64,
    sink: &mut EdgeSink,
    work: &TickWork,
) -> BoundaryResult<u64> {
    if rows.is_empty() {
        return Ok(base);
    }
    let events: Vec<DeltaEvent> = rows
        .iter()
        .enumerate()
        .map(|(sequence, row)| DeltaEvent {
            rel: statement.head_rel.clone(),
            sign: 1,
            sequence: base + sequence as u64,
            row: row.clone(),
        })
        .collect();
    {
        let mut scope = crate::trace::Scope::verb(
            "edge_write",
            &statement.head_rel,
            crate::write_verbs::relation_strategy(relation),
        );
        scope.rows(rows.len());
        seam.execute(&edge_log_write_statement(statement, rows)?)
            .expect("edge log write failed");
    }
    sink.take(seam, relation, events, work)?;
    Ok(base + rows.len() as u64)
}

fn apply_keyed_edge(
    seam: &SqliteSeam,
    statement: &crate::types::IncrementalEdgeStatement,
    relation: &IncrementalRelationPlan,
    projected_rows: &[Row],
    base: u64,
    sink: &mut EdgeSink,
    work: &TickWork,
) -> BoundaryResult<u64> {
    let mut resolved: Vec<Row> = Vec::new();
    let mut resolved_index: HashMap<String, usize> = HashMap::new();
    for row in projected_rows {
        let key = row_key(row, &statement.key_indices)?;
        match resolved_index.get(&key) {
            Some(index) => resolved[*index] = row.clone(),
            None => {
                resolved_index.insert(key, resolved.len());
                resolved.push(row.clone());
            }
        }
    }
    let rows: Vec<Row> = resolved;
    if rows.is_empty() {
        return Ok(base);
    }
    let mut key_args: Vec<ScalarValue> = Vec::new();
    for row in &rows {
        let key: Row = statement
            .key_indices
            .iter()
            .map(|index| row[*index].clone())
            .collect();
        key_args.extend(bind_args(&key)?);
    }
    let strategy = crate::write_verbs::relation_strategy(relation);
    let before_result = {
        let mut scope = crate::trace::Scope::verb("edge_lookup", &statement.head_rel, strategy);
        scope.rows(rows.len());
        seam.execute(&SqlStatement {
            sql: edge_keyed_rows_sql(statement, rows.len()),
            args: key_args,
        })
        .expect("edge keyed rows lookup failed")
    };
    let head_types = relation.column_types.clone();
    let before_rows = result_rows(&before_result, &statement.head_columns, &head_types)?;
    let mut before_by_key: HashMap<String, Row> = HashMap::new();
    for row in &before_rows {
        before_by_key.insert(row_key(row, &statement.key_indices)?, row.clone());
    }
    let mut changed_rows: Vec<Row> = Vec::new();
    for row in rows {
        let unchanged = match before_by_key.get(&row_key(&row, &statement.key_indices)?) {
            None => false,
            Some(before) => rows_equal(before, &row),
        };
        if !unchanged {
            changed_rows.push(row);
        }
    }
    if changed_rows.is_empty() {
        return Ok(base);
    }
    let mut events = Vec::new();
    for (sequence, row) in changed_rows.iter().enumerate() {
        let key = row_key(row, &statement.key_indices)?;
        if let Some(before) = before_by_key.get(&key) {
            events.push(DeltaEvent {
                rel: statement.head_rel.clone(),
                sign: -1,
                sequence: base + (sequence * 2) as u64,
                row: before.clone(),
            });
        }
        events.push(DeltaEvent {
            rel: statement.head_rel.clone(),
            sign: 1,
            sequence: base + (sequence * 2 + 1) as u64,
            row: row.clone(),
        });
    }
    {
        let mut scope = crate::trace::Scope::verb("edge_write", &statement.head_rel, strategy);
        scope.rows(changed_rows.len());
        seam.execute(&edge_keyed_write_statement(statement, &changed_rows)?)
            .expect("edge keyed write failed");
    }
    sink.take(seam, relation, events, work)?;
    Ok(base + (changed_rows.len() * 2) as u64)
}

fn write_head_rows(
    seam: &SqliteSeam,
    statement: &crate::types::IncrementalEdgeStatement,
    relation: &IncrementalRelationPlan,
    rows: &[Row],
    base: u64,
    sink: &mut EdgeSink,
    work: &TickWork,
) -> BoundaryResult<u64> {
    match statement.head_kind {
        RelationKind::Log => apply_log_edge(seam, statement, relation, rows, base, sink, work),
        RelationKind::Set => apply_keyed_edge(seam, statement, relation, rows, base, sink, work),
    }
}

/// The tick's NET per rel: a row written and then overwritten inside the walk
/// leaves the store unchanged and must not reach the carry.
fn net_additions<'a>(events: &[&'a DeltaEvent]) -> Vec<&'a DeltaEvent> {
    let mut weights: HashMap<String, i64> = HashMap::new();
    for event in events {
        *weights.entry(dedup_key(&event.row)).or_insert(0) += event.sign as i64;
    }
    let mut additions = Vec::new();
    for event in events {
        if event.sign != 1 {
            continue;
        }
        let weight = weights
            .get_mut(&dedup_key(&event.row))
            .expect("weight recorded above");
        if *weight <= 0 {
            continue;
        }
        *weight -= 1;
        additions.push(*event);
    }
    additions
}

fn stage_collected_events(
    seam: &SqliteSeam,
    relations: &[IncrementalRelationPlan],
    events: &[DeltaEvent],
    work: &TickWork,
) -> BoundaryResult<()> {
    if events.is_empty() {
        return Ok(());
    }
    stage_events(seam, relations, events, &[], work)?;
    let plans = plan_index(relations);
    let mut rels: Vec<&str> = events.iter().map(|event| event.rel.as_str()).collect();
    rels.sort_unstable();
    rels.dedup();
    for rel in rels {
        let relation = plan_for(&plans, rel, "sequenced edge head relation missing");
        let grouped: Vec<&DeltaEvent> = events.iter().filter(|event| event.rel == rel).collect();
        let additions = net_additions(&grouped);
        if additions.is_empty() {
            continue;
        }
        let mut scope = crate::trace::Scope::verb(
            "stage",
            rel,
            crate::write_verbs::relation_strategy(relation),
        );
        scope.rows(additions.len());
        seam.execute(&frontier_stage_statement(
            relation,
            &relation.next_frontier_table_name,
            0,
            &additions,
        )?)
        .expect("sequenced frontier staging failed");
        work.note_frontier_write(&relation.rel, true);
    }
    Ok(())
}

/// One trigger row a sequenced arm consumes, carrying the frontier index the
/// tick's pick order reads (ruling one_pick_order).
struct Occurrence {
    rel: String,
    kind: crate::types::TriggerKind,
    row: Row,
    phase: i64,
    sequence: i64,
}

fn occurrence_read_sql(
    relation: &IncrementalRelationPlan,
    kind: crate::types::TriggerKind,
) -> Option<String> {
    let table = match kind {
        crate::types::TriggerKind::Arrival => relation.frontier_table_name.clone(),
        crate::types::TriggerKind::Departure => relation.departure_frontier_table_name.clone()?,
    };
    let columns: Vec<String> = relation
        .columns
        .iter()
        .map(|column| quote_identifier(column))
        .collect();
    // Declared columns first: result_rows reads a row by POSITION, so the two
    // index columns ride behind them.
    let projection = if columns.is_empty() {
        String::new()
    } else {
        format!("{}, ", columns.join(", "))
    };
    Some(format!(
        "SELECT {}\"_phase\", \"_sequence\" FROM {} ORDER BY \"_phase\", \"_sequence\"",
        projection,
        quote_identifier(&table)
    ))
}

// The frontier IS the occurrence list: last tick's carry, this tick's arrivals
// and this tick's before-edge level rows, already in (_phase, _sequence) order.
fn read_occurrences(
    seam: &SqliteSeam,
    statements: &[&crate::types::IncrementalEdgeStatement],
    plans: &HashMap<&str, &IncrementalRelationPlan>,
    work: &TickWork,
) -> BoundaryResult<Vec<Occurrence>> {
    let mut triggers: Vec<(&str, crate::types::TriggerKind)> = statements
        .iter()
        .map(|statement| (statement.trigger_rel.as_str(), statement.trigger_kind))
        .collect();
    triggers.sort_unstable_by_key(|(rel, kind)| (*rel, *kind as u8));
    triggers.dedup();
    let mut occurrences = Vec::new();
    for (rel, kind) in triggers {
        if !work.moved(rel) {
            continue;
        }
        let relation = plan_for(plans, rel, "sequenced edge trigger relation missing");
        let Some(sql) = occurrence_read_sql(relation, kind) else {
            continue;
        };
        let _scope = crate::trace::Scope::verb(
            "read_staged",
            rel,
            crate::write_verbs::relation_strategy(relation),
        );
        let result = seam
            .execute(&SqlStatement { sql, args: vec![] })
            .expect("sequenced occurrence read failed");
        let phase_index = crate::sql::column_index(&result, "_phase");
        let sequence_index = crate::sql::column_index(&result, "_sequence");
        let rows = result_rows(&result, &relation.columns, &relation.column_types)?;
        for (result_row, row) in result.rows.iter().zip(rows) {
            occurrences.push(Occurrence {
                rel: rel.to_string(),
                kind,
                row,
                phase: phase_index
                    .and_then(|index| result_row.get(index))
                    .and_then(|value| value.as_i64())
                    .unwrap_or(0),
                sequence: sequence_index
                    .and_then(|index| result_row.get(index))
                    .and_then(|value| value.as_i64())
                    .unwrap_or(0),
            });
        }
    }
    occurrences.sort_by_key(|occurrence| (occurrence.phase, occurrence.sequence));
    Ok(occurrences)
}

/// A `pre/1` head's evolving mirror: occurrence N+1 reads what occurrence N
/// wrote, which is the whole reason this arm is sequenced.
fn write_pre_rows(
    statement: &crate::types::IncrementalEdgeStatement,
    rows: &[Row],
) -> BoundaryResult<Option<SqlStatement>> {
    if rows.is_empty() {
        return Ok(None);
    }
    let table = quote_identifier(&format!("__pre_{}", statement.head_table_name));
    let columns = edge_columns_text(statement);
    let conflict = if statement.head_kind == RelationKind::Log {
        String::new()
    } else {
        let key_columns: Vec<String> = statement
            .key_indices
            .iter()
            .map(|index| columns[*index].clone())
            .collect();
        let non_key_columns: Vec<String> = columns
            .iter()
            .enumerate()
            .filter(|(index, _)| !statement.key_indices.contains(index))
            .map(|(_, column)| column.clone())
            .collect();
        if non_key_columns.is_empty() {
            format!(" ON CONFLICT({}) DO NOTHING", key_columns.join(", "))
        } else {
            format!(
                " ON CONFLICT({}) DO UPDATE SET {}",
                key_columns.join(", "),
                non_key_columns
                    .iter()
                    .map(|column| format!("{} = excluded.{}", column, column))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    };
    Ok(Some(SqlStatement {
        sql: format!(
            "INSERT INTO {} ({}) VALUES {}{}",
            table,
            columns.join(", "),
            values_sql(rows.len(), columns.len()),
            conflict
        ),
        args: flat_bind_args(rows)?,
    }))
}

// Occurrence-major across every sequenced arm, because two arms on different
// trigger rels can fold the same key and arrival order decides the winner.
fn apply_sequenced_edges(
    seam: &SqliteSeam,
    statements: &[&crate::types::IncrementalEdgeStatement],
    relations: &[IncrementalRelationPlan],
    plans: &HashMap<&str, &IncrementalRelationPlan>,
    work: &TickWork,
) -> BoundaryResult<()> {
    let occurrences = read_occurrences(seam, statements, plans, work)?;
    if occurrences.is_empty() {
        return Ok(());
    }
    let strategy = crate::write_verbs::strategy_name(relations);
    let mut sequence = 0u64;
    let mut collected: Vec<DeltaEvent> = Vec::new();
    for occurrence in &occurrences {
        let args = ScalarValue::row_at_seam(&occurrence.row, ScalarSeam::SqlParameter)?;
        // Every arm of one occurrence reads the store as the occurrence found
        // it, so the whole projection pass precedes the whole write pass.
        let mut projected: Vec<(usize, Row)> = Vec::new();
        for (index, statement) in statements.iter().enumerate() {
            if statement.trigger_rel != occurrence.rel || statement.trigger_kind != occurrence.kind
            {
                continue;
            }
            let relation = plan_for(
                plans,
                &statement.head_rel,
                "incremental edge head relation missing",
            );
            let sql = statement
                .occurrence_project_sql
                .as_ref()
                .expect("sequenced arm without an occurrence projection");
            let mut scope =
                crate::trace::Scope::verb("edge_project", &statement.head_rel, strategy);
            let result = intern_then_execute(
                seam,
                statement.occurrence_intern_sql.as_ref(),
                &SqlStatement {
                    sql: sql.clone(),
                    args: args.clone(),
                },
            );
            let rows = result_rows(&result, &statement.head_columns, &relation.column_types)?;
            scope.rows(rows.len());
            drop(scope);
            projected.extend(rows.into_iter().map(|row| (index, row)));
        }
        sequence = write_occurrence(
            seam,
            statements,
            plans,
            &projected,
            sequence,
            strategy,
            &mut collected,
            work,
        )?;
    }
    stage_collected_events(seam, relations, &collected, work)
}

// Two arms deriving the same row for the same head in one occurrence is one
// write; two rows on one key with different values is a program defect.
fn write_occurrence(
    seam: &SqliteSeam,
    statements: &[&crate::types::IncrementalEdgeStatement],
    plans: &HashMap<&str, &IncrementalRelationPlan>,
    projected: &[(usize, Row)],
    mut sequence: u64,
    strategy: &'static str,
    collected: &mut Vec<DeltaEvent>,
    work: &TickWork,
) -> BoundaryResult<u64> {
    let mut seen_rows: std::collections::HashSet<(&str, String)> = std::collections::HashSet::new();
    let mut keyed: HashMap<String, &Row> = HashMap::new();
    let mut accepted: Vec<(usize, Row)> = Vec::new();
    for (index, row) in projected {
        let statement = statements[*index];
        DEDUP_PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if !seen_rows.insert((statement.head_rel.as_str(), dedup_key(row))) {
            continue;
        }
        if statement.head_kind == RelationKind::Set {
            let key = format!(
                "{}:{}",
                statement.head_rel,
                row_key(row, &statement.key_indices)?
            );
            match keyed.get(&key) {
                Some(prior) if *prior != row => {
                    panic!(
                        "keyed conflict in one occurrence for {}",
                        statement.head_rel
                    )
                }
                Some(_) => {}
                None => {
                    keyed.insert(key, row);
                }
            }
        }
        accepted.push((*index, row.clone()));
    }
    let mut at = 0usize;
    while at < accepted.len() {
        let index = accepted[at].0;
        let mut end = at;
        while end < accepted.len() && accepted[end].0 == index {
            end += 1;
        }
        let rows: Vec<Row> = accepted[at..end]
            .iter()
            .map(|(_, row)| row.clone())
            .collect();
        let statement = statements[index];
        let relation = plan_for(
            plans,
            &statement.head_rel,
            "incremental edge head relation missing",
        );
        sequence = write_head_rows(
            seam,
            statement,
            relation,
            &rows,
            sequence,
            &mut EdgeSink::Collect(collected),
            work,
        )?;
        if statement.evolves_pre {
            if let Some(pre) = write_pre_rows(statement, &rows)? {
                let _scope = crate::trace::Scope::verb("edge_write", &statement.head_rel, strategy);
                seam.execute(&pre).expect("pre plane write failed");
            }
        }
        at = end;
    }
    Ok(sequence)
}

/// An arm whose trigger frontier holds nothing derives nothing. An IR emitted
/// before the field existed carries no trigger and always runs.
fn arm_has_work(statement: &crate::types::IncrementalEdgeStatement, work: &TickWork) -> bool {
    statement.trigger_rel.is_empty() || work.moved(&statement.trigger_rel)
}

pub fn apply_edges(
    seam: &SqliteSeam,
    statements: &[crate::types::IncrementalEdgeStatement],
    relations: &[IncrementalRelationPlan],
    work: &TickWork,
) -> BoundaryResult<()> {
    let plans = plan_index(relations);
    let sequenced: Vec<&crate::types::IncrementalEdgeStatement> = statements
        .iter()
        .filter(|statement| statement.schedule == crate::types::ArmSchedule::Sequenced)
        .filter(|statement| arm_has_work(statement, work))
        .collect();
    // The sequenced group runs where its first arm sits in emission order, so a
    // set-at-once arm keeps its position relative to it.
    let sequenced_at = statements.iter().position(|statement| {
        statement.schedule == crate::types::ArmSchedule::Sequenced && arm_has_work(statement, work)
    });
    for (index, statement) in statements.iter().enumerate() {
        if Some(index) == sequenced_at {
            apply_sequenced_edges(seam, &sequenced, relations, &plans, work)?;
        }
        if statement.schedule == crate::types::ArmSchedule::Sequenced
            || !arm_has_work(statement, work)
        {
            continue;
        }
        let relation = plan_for(
            &plans,
            &statement.head_rel,
            "incremental edge head relation missing",
        );
        let strategy = crate::write_verbs::strategy_name(relations);
        let mut scope = crate::trace::Scope::verb("edge_project", &statement.head_rel, strategy);
        let result = intern_then_execute(
            seam,
            statement.intern_sql.as_ref(),
            &SqlStatement {
                sql: statement.project_sql.clone(),
                args: vec![],
            },
        );
        let rows = result_rows(&result, &statement.head_columns, &relation.column_types)?;
        scope.rows(rows.len());
        drop(scope);
        write_head_rows(
            seam,
            statement,
            relation,
            &rows,
            0,
            &mut EdgeSink::Stage,
            work,
        )?;
    }
    Ok(())
}

pub fn merge_next_into_current(
    seam: &SqliteSeam,
    relations: &[IncrementalRelationPlan],
    work: &TickWork,
) {
    let carrying: Vec<IncrementalRelationPlan> = relations
        .iter()
        .filter(|relation| work.carries(&relation.rel))
        .cloned()
        .collect();
    if carrying.is_empty() {
        return;
    }
    let _scope = crate::trace::Scope::verb(
        "clear",
        "merge",
        crate::write_verbs::strategy_name(relations),
    );
    let sql = write_verbs_for(relations)
        .clear(&carrying, TickBoundary::Merge)
        .join(";\n");
    if sql.is_empty() {
        return;
    }
    seam.execute_multiple(&sql)
        .expect("merge_next_into_current failed");
}

pub fn apply_levels_after_edges(
    seam: &SqliteSeam,
    statements: &[crate::types::IncrementalLevelStatement],
    relations: &[IncrementalRelationPlan],
    work: &TickWork,
    sources: &HashMap<String, LevelSources>,
) -> BoundaryResult<()> {
    let plans = plan_index(relations);
    let mut sequence = 0u64;
    let mut next_sequence = || {
        let current = sequence;
        sequence += 1;
        current
    };
    let ordered: Vec<&crate::types::IncrementalLevelStatement> = statements.iter().collect();
    sequence_level_rounds(&ordered, |statement| {
        if !level_runs_this_tick(sources, statement, work, LevelPhase::Insert) {
            return Ok(0);
        }
        let moved = apply_level_statement(
            seam,
            statement,
            relations,
            &plans,
            true,
            &mut next_sequence,
            work,
        )?;
        settle_level_run(sources, statement, work, LevelPhase::Insert);
        Ok(moved)
    })
}

// The frozen mid-tick level plane: a level row an arrival retracted this tick
// must be gone before an edge body reads it.
pub fn recompute_levels_before_edges(
    seam: &SqliteSeam,
    statements: &[crate::types::IncrementalLevelStatement],
    relations: &[IncrementalRelationPlan],
    reconcile_every_tick: bool,
    arrival_count: usize,
    work: &TickWork,
    sources: &HashMap<String, LevelSources>,
) -> BoundaryResult<()> {
    if arrival_count == 0 || relations.is_empty() {
        return Ok(());
    }
    let ref_count_statements: Vec<&crate::types::IncrementalLevelStatement> = statements
        .iter()
        .filter(|statement| statement.aggregate_sql.is_none())
        .collect();
    if ref_count_statements.is_empty() {
        return Ok(());
    }
    let plans = plan_index(relations);
    let reconcile = |seam: &SqliteSeam| -> BoundaryResult<()> {
        sequence_level_rounds(&ref_count_statements, |statement| {
            if !recount_runs_this_tick(sources, statement, work) {
                return Ok(0);
            }
            let relation = plan_for(
                &plans,
                &statement.head_rel,
                "incremental level head relation missing",
            );
            let moved = reconcile_ref_count_statement(
                seam,
                statement,
                relations,
                &plans,
                &[(relation.frontier_table_name.clone(), 2)],
                work,
            )?;
            settle_level_run(sources, statement, work, LevelPhase::Recount);
            Ok(moved)
        })
    };
    if reconcile_every_tick {
        return reconcile(seam);
    }
    let guard = retraction_guard_sql(relations);
    let has_retraction = {
        let _scope = crate::trace::Scope::verb(
            "retraction_guard",
            "-",
            crate::write_verbs::strategy_name(relations),
        );
        seam.scalar(&guard).expect("retraction guard read failed") == 1
    };
    if has_retraction {
        return reconcile(seam);
    }
    Ok(())
}

fn retraction_guard_sql(relations: &[IncrementalRelationPlan]) -> String {
    let terms: Vec<String> = relations
        .iter()
        .map(|relation| {
            format!(
                "EXISTS (SELECT 1 FROM {} WHERE \"_sign\" = -1 LIMIT 1)",
                quote_identifier(&relation.delta_table_name)
            )
        })
        .collect();
    if terms.is_empty() {
        return "SELECT 0 AS has_retraction".to_string();
    }
    format!(
        "SELECT CASE WHEN {} THEN 1 ELSE 0 END AS has_retraction",
        terms.join(" OR ")
    )
}

// Port of reconcile_ref_count_statement (1_incremental.ts:553). Non-skipped,
// non-expand, non-dred path: reseed support from base tables, subtract into the
// head's refcount, stage what fell to zero as -1 and what is newly derivable as
// +1.
fn reconcile_ref_count_statement(
    seam: &SqliteSeam,
    statement: &crate::types::IncrementalLevelStatement,
    relations: &[IncrementalRelationPlan],
    plans: &HashMap<&str, &IncrementalRelationPlan>,
    frontier_copies: &[(String, i64)],
    work: &TickWork,
) -> BoundaryResult<usize> {
    let support_sql = statement
        .support_sql
        .as_ref()
        .expect("level statement has no support_sql");
    let relation = plan_for(
        plans,
        &statement.head_rel,
        "incremental level head relation missing",
    );
    let mut scope = crate::trace::Scope::verb(
        "recount",
        &statement.head_rel,
        crate::write_verbs::strategy_name(relations),
    );
    let clear = &support_sql[0];
    let update = &support_sql[2];
    let stage_retract = &support_sql[3];
    let collect_zero = &support_sql[4];
    let clear_new = &support_sql[5];
    let fill_new = &support_sql[6];
    let stage_add = &support_sql[7];
    let stage_frontier = &support_sql[8];
    let stage_next_frontier = &support_sql[9];
    let insert_new = &support_sql[10];

    let tail_texts = vec![
        update.clone(),
        stage_retract.clone(),
        collect_zero.clone(),
        clear_new.clone(),
        fill_new.clone(),
        stage_add.clone(),
    ];
    let support_interns = statement.support_intern_sql.clone().unwrap_or_default();
    // Indices into tail_texts, summed into the returned moved count: a round
    // that only RETRACTS still has to run again for the cycle's peers.
    let collect_zero_index = 2usize;
    let fill_new_index = 4usize;
    let mut tail = to_statements(&tail_texts);
    for (table_name, phase) in frontier_copies {
        let stage = if *table_name == relation.next_frontier_table_name {
            stage_next_frontier.clone()
        } else {
            stage_frontier.clone()
        };
        tail.push(SqlStatement {
            sql: stage,
            args: vec![ScalarValue::Integer(*phase)],
        });
    }
    tail.push(SqlStatement {
        sql: insert_new.clone(),
        args: vec![],
    });
    tail.extend(to_statements(
        &write_verbs_for(relations).recount(statement),
    ));

    let Some(expand) = &statement.expand_sql else {
        let mut head = Vec::new();
        head.push(SqlStatement {
            sql: clear.clone(),
            args: vec![],
        });
        head.extend(to_statements(&support_interns));
        head.push(SqlStatement {
            sql: support_sql[1].clone(),
            args: vec![],
        });
        head.extend(tail);
        let results = seam.batch(&head).expect("reconcile batch failed");
        let offset = 2 + support_interns.len();
        let fresh = moved_rows(&results, offset + fill_new_index);
        let retracted = moved_rows(&results, offset + collect_zero_index);
        let moved = fresh + retracted;
        scope.rows(moved);
        mark_reconciled(work, &statement.head_rel, fresh, retracted);
        // The frontier arms of the tail read `__new_`, so they wrote exactly
        // when the fill did.
        if fresh > 0 {
            note_frontier_copies(relation, frontier_copies, work);
        }
        return Ok(moved);
    };
    // Port of the rx expand wavefront (1_incremental.ts:610). The CTE seed
    // reaches the same fixpoint inside SQLite, where no round is countable.
    let mut seed_wave = vec![SqlStatement {
        sql: clear.clone(),
        args: vec![],
    }];
    seed_wave.extend(to_statements(&support_interns));
    seed_wave.extend(to_statements(&[
        expand.clear_a_sql.clone(),
        expand.clear_b_sql.clone(),
    ]));
    seed_wave.extend(to_statements(&expand.seed_sqls));
    seed_wave.push(SqlStatement {
        sql: expand.absorb_a_sql.clone(),
        args: vec![],
    });
    seam.batch(&seed_wave).expect("expand seed batch failed");
    let round = |fills_b: bool| -> i64 {
        let texts = if fills_b {
            [
                &expand.clear_b_sql,
                &expand.hop_ab_sql,
                &expand.absorb_b_sql,
            ]
        } else {
            [
                &expand.clear_a_sql,
                &expand.hop_ba_sql,
                &expand.absorb_a_sql,
            ]
        };
        let statements: Vec<SqlStatement> = texts
            .iter()
            .map(|text| SqlStatement {
                sql: (*text).clone(),
                args: vec![],
            })
            .collect();
        seam.batch(&statements).expect("expand hop batch failed")[1].rows_affected
    };
    let mut fills_b = true;
    for index in 0..expand.round_cap {
        let round_span = crate::trace::round_span(index);
        let _entered = round_span.enter();
        let moved = round(fills_b);
        round_span.record("delta_rows", moved);
        if moved == 0 {
            break;
        }
        if index + 1 >= expand.round_cap {
            return Err(BoundaryError::DivergingMeasureRecursion {
                rel: statement.head_rel.clone(),
                round_cap: expand.round_cap,
            });
        }
        fills_b = !fills_b;
    }
    let mut close = to_statements(&tail_texts);
    close.push(SqlStatement {
        sql: insert_new.clone(),
        args: vec![],
    });
    let results = seam.batch(&close).expect("expand close batch failed");
    let fresh = moved_rows(&results, fill_new_index);
    let retracted = moved_rows(&results, collect_zero_index);
    let moved = fresh + retracted;
    scope.rows(moved);
    mark_reconciled(work, &statement.head_rel, fresh, retracted);
    if fresh > 0 {
        note_frontier_copies(relation, frontier_copies, work);
    }
    Ok(moved)
}

fn mark_reconciled(work: &TickWork, head: &str, fresh: usize, retracted: usize) {
    if retracted > 0 {
        work.mark_shrank(head);
    }
    if fresh > 0 {
        work.mark_grew(head);
    }
}

fn moved_rows(results: &[crate::types::QueryResult], index: usize) -> usize {
    results
        .get(index)
        .map(|result| result.rows_affected.max(0) as usize)
        .unwrap_or(0)
}

// Port of recompute_levels_after_edges (1_incremental.ts:1243). Guarded by the
// retraction guard; a purely additive tick skips the reconcile entirely.
pub fn recompute_levels_after_edges(
    seam: &SqliteSeam,
    statements: &[crate::types::IncrementalLevelStatement],
    relations: &[IncrementalRelationPlan],
    reconcile_every_tick: bool,
    work: &TickWork,
    sources: &HashMap<String, LevelSources>,
) -> BoundaryResult<()> {
    if statements.is_empty() {
        return Ok(());
    }
    // No frontier copies on either arm: a reconcile row is a correction inside
    // the same closure, never post-write growth, so it must not carry.
    let plans = plan_index(relations);
    let reconcile = |seam: &SqliteSeam| -> BoundaryResult<()> {
        let mut sequence = 0u64;
        let mut next_sequence = || {
            let current = sequence;
            sequence += 1;
            current
        };
        let ordered: Vec<&crate::types::IncrementalLevelStatement> = statements.iter().collect();
        sequence_level_rounds(&ordered, |statement| {
            let runs = match &statement.aggregate_sql {
                Some(_) => level_runs_this_tick(sources, statement, work, LevelPhase::Recount),
                None => recount_runs_this_tick(sources, statement, work),
            };
            if !runs {
                return Ok(0);
            }
            let relation = plan_for(
                &plans,
                &statement.head_rel,
                "incremental level head relation missing",
            );
            if let Some(aggregate) = &statement.aggregate_sql {
                if aggregate.delta_maintained {
                    return Ok(0);
                }
                apply_aggregate_level_statement(
                    seam,
                    statement,
                    aggregate,
                    relation,
                    false,
                    &mut next_sequence,
                    work,
                )?;
                settle_level_run(sources, statement, work, LevelPhase::Recount);
                return Ok(0);
            }
            let moved =
                reconcile_ref_count_statement(seam, statement, relations, &plans, &[], work)?;
            settle_level_run(sources, statement, work, LevelPhase::Recount);
            Ok(moved)
        })
    };
    if reconcile_every_tick {
        return reconcile(seam);
    }
    if relations.is_empty() {
        return Ok(());
    }
    let guard = retraction_guard_sql(relations);
    let _scope = crate::trace::Scope::verb(
        "retraction_guard",
        "-",
        crate::write_verbs::strategy_name(relations),
    );
    let has_retraction = seam.scalar(&guard).expect("retraction guard read failed") == 1;
    drop(_scope);
    if has_retraction {
        return reconcile(seam);
    }
    Ok(())
}
