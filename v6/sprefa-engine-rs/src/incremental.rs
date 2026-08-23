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
    index.get(rel).copied().unwrap_or_else(|| panic!("{missing}"))
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
fn staged_json(prefix: [i64; 2], events: &[&DeltaEvent], sequence_only: bool) -> BoundaryResult<String> {
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
) -> BoundaryResult<()> {
    if events.is_empty() {
        return Ok(());
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
    }
    Ok(())
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
        )?;
    }
    Ok(())
}

fn rows_equal(left: &Row, right: &Row) -> bool {
    left == right
}

// One statement per tick: the clock the oracle fixes for the whole tick.
pub fn advance_tick(seam: &SqliteSeam) {
    let _scope = crate::trace::Scope::phase("advance_tick");
    seam.execute_multiple("UPDATE \"__tick\" SET \"n\" = \"n\" + 1")
        .expect("advance_tick failed");
}

pub fn prepare_tick(seam: &SqliteSeam, relations: &[IncrementalRelationPlan]) {
    if relations.is_empty() {
        return;
    }
    let _scope = crate::trace::Scope::verb(
        "clear",
        "prepare",
        crate::write_verbs::strategy_name(relations),
    );
    let sql = write_verbs_for(relations)
        .clear(relations, TickBoundary::Prepare)
        .join(";\n");
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
            std::collections::hash_map::Entry::Occupied(seen) => {
                weights[*seen.get()].1 += weight
            }
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

pub fn read_boundary(
    seam: &SqliteSeam,
    relations: &[IncrementalRelationPlan],
) -> BoundaryResult<Vec<crate::types::RelDelta>> {
    relations
        .iter()
        .map(|relation| {
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
        statements.push(SqlStatement {
            sql: format!("DELETE FROM {}", quote_identifier(table_name)),
            args: vec![],
        });
        let departed = departures_by_rel
            .get(relation.rel.as_str())
            .copied()
            .unwrap_or_default();
        if departed.is_empty() {
            continue;
        }
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

pub fn stage_ordered_frontiers(
    seam: &SqliteSeam,
    relations: &[IncrementalRelationPlan],
    additions: &[crate::types::RelDelta],
) -> BoundaryResult<bool> {
    let mut events_by_rel: HashMap<&str, Vec<DeltaEvent>> = HashMap::new();
    let mut sequence = 0;
    for delta in additions {
        for row in &delta.add {
            events_by_rel
                .entry(delta.rel.as_str())
                .or_default()
                .push(DeltaEvent {
                    rel: delta.rel.clone(),
                    sign: 1,
                    sequence,
                    row: row.clone(),
                });
            sequence += 1;
        }
    }
    let mut statements = Vec::new();
    let mut carry_pending = false;
    for relation in relations {
        statements.push(SqlStatement {
            sql: format!(
                "DELETE FROM {}",
                quote_identifier(&relation.frontier_table_name)
            ),
            args: vec![],
        });
        statements.push(SqlStatement {
            sql: format!(
                "DELETE FROM {}",
                quote_identifier(&relation.next_frontier_table_name)
            ),
            args: vec![],
        });
        let Some(events) = events_by_rel.get(relation.rel.as_str()) else {
            continue;
        };
        carry_pending = true;
        let borrowed: Vec<&DeltaEvent> = events.iter().collect();
        statements.push(frontier_stage_statement(
            relation,
            &relation.frontier_table_name,
            0,
            &borrowed,
        )?);
    }
    if !statements.is_empty() {
        seam.batch(&statements)
            .expect("ordered frontier staging failed");
    }
    Ok(carry_pending)
}

// Port of promote_frontiers: read carry, promote next into current.
pub fn promote_frontiers(seam: &SqliteSeam, relations: &[IncrementalRelationPlan]) -> bool {
    if relations.is_empty() {
        return false;
    }
    let verbs = write_verbs_for(relations);
    let strategy = crate::write_verbs::strategy_name(relations);
    let promote_sql = verbs.clear(relations, TickBoundary::Promote).join(";\n");
    let promote = || {
        let _scope = crate::trace::Scope::verb("clear", "promote", strategy);
        if !promote_sql.is_empty() {
            seam.execute_multiple(&promote_sql).expect("promote failed");
        }
    };
    let carry_sql = verbs.read_staged(relations);
    if carry_sql.is_empty() {
        promote();
        return false;
    }
    let carry_pending = {
        let _scope = crate::trace::Scope::verb("read_staged", "-", strategy);
        seam.scalar(&carry_sql).expect("carry read failed") == 1
    };
    promote();
    carry_pending
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
) -> BoundaryResult<usize> {
    let relation = plan_for(
        plans,
        &statement.head_rel,
        "incremental level head relation missing",
    );
    if let Some(aggregate) = &statement.aggregate_sql {
        apply_aggregate_level_statement(
            seam,
            statement,
            aggregate,
            relation,
            after_edges,
            next_sequence,
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
    stage_events(seam, std::slice::from_ref(relation), &events, &copies)?;
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
    stage_events(seam, std::slice::from_ref(relation), &events, &copies)
}

pub fn apply_retention(
    seam: &SqliteSeam,
    statements: &[crate::types::IncrementalRetentionStatement],
    relations: &[IncrementalRelationPlan],
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
        stage_events(seam, std::slice::from_ref(relation), &events, &[])?;
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
        if closes_in_one_pass {
            let copies: Vec<(String, i64)> = relations
                .iter()
                .filter(|r| r.rel == statement.head_rel)
                .map(|r| (r.frontier_table_name.clone(), 2))
                .collect();
            reconcile_ref_count_statement(seam, statement, relations, &plans, &copies)
        } else {
            apply_level_statement(seam, statement, relations, &plans, false, &mut next_sequence)
        }
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

fn apply_log_edge(
    seam: &SqliteSeam,
    statement: &crate::types::IncrementalEdgeStatement,
    relation: &IncrementalRelationPlan,
    rows: &[Row],
) -> BoundaryResult<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let events: Vec<DeltaEvent> = rows
        .iter()
        .enumerate()
        .map(|(sequence, row)| DeltaEvent {
            rel: statement.head_rel.clone(),
            sign: 1,
            sequence: sequence as u64,
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
    stage_events(
        seam,
        std::slice::from_ref(relation),
        &events,
        &[(relation.next_frontier_table_name.clone(), 0)],
    )
}

fn apply_keyed_edge(
    seam: &SqliteSeam,
    statement: &crate::types::IncrementalEdgeStatement,
    relation: &IncrementalRelationPlan,
    projected_rows: &[Row],
) -> BoundaryResult<()> {
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
        return Ok(());
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
        return Ok(());
    }
    let mut events = Vec::new();
    for (sequence, row) in changed_rows.iter().enumerate() {
        let key = row_key(row, &statement.key_indices)?;
        if let Some(before) = before_by_key.get(&key) {
            events.push(DeltaEvent {
                rel: statement.head_rel.clone(),
                sign: -1,
                sequence: (sequence * 2) as u64,
                row: before.clone(),
            });
        }
        events.push(DeltaEvent {
            rel: statement.head_rel.clone(),
            sign: 1,
            sequence: (sequence * 2 + 1) as u64,
            row: row.clone(),
        });
    }
    {
        let mut scope = crate::trace::Scope::verb("edge_write", &statement.head_rel, strategy);
        scope.rows(changed_rows.len());
        seam.execute(&edge_keyed_write_statement(statement, &changed_rows)?)
            .expect("edge keyed write failed");
    }
    stage_events(
        seam,
        std::slice::from_ref(relation),
        &events,
        &[(relation.next_frontier_table_name.clone(), 0)],
    )
}

pub fn apply_edges(
    seam: &SqliteSeam,
    statements: &[crate::types::IncrementalEdgeStatement],
    relations: &[IncrementalRelationPlan],
) -> BoundaryResult<()> {
    let plans = plan_index(relations);
    for statement in statements {
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
        match statement.head_kind {
            RelationKind::Log => apply_log_edge(seam, statement, relation, &rows)?,
            RelationKind::Set => apply_keyed_edge(seam, statement, relation, &rows)?,
        }
    }
    Ok(())
}

pub fn merge_next_into_current(seam: &SqliteSeam, relations: &[IncrementalRelationPlan]) {
    if relations.is_empty() {
        return;
    }
    let _scope = crate::trace::Scope::verb(
        "clear",
        "merge",
        crate::write_verbs::strategy_name(relations),
    );
    let sql = write_verbs_for(relations)
        .clear(relations, TickBoundary::Merge)
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
        apply_level_statement(seam, statement, relations, &plans, true, &mut next_sequence)
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
            let relation = plan_for(
                &plans,
                &statement.head_rel,
                "incremental level head relation missing",
            );
            reconcile_ref_count_statement(
                seam,
                statement,
                relations,
                &plans,
                &[(relation.frontier_table_name.clone(), 2)],
            )
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
        let moved = moved_rows(&results, offset + fill_new_index)
            + moved_rows(&results, offset + collect_zero_index);
        scope.rows(moved);
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
    let moved = moved_rows(&results, fill_new_index) + moved_rows(&results, collect_zero_index);
    scope.rows(moved);
    Ok(moved)
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
                )?;
                return Ok(0);
            }
            reconcile_ref_count_statement(seam, statement, relations, &plans, &[])
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
