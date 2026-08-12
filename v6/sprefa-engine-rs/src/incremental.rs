// The tick engine, a port of v6/tsv2/runtime/1_incremental.ts. The seam is
// blocking rusqlite and the whole engine is plain sync (async is realized at
// the driver's spawn + channel + StreamExt); this is the v6 law "sync stays
// sync, in-memory row work is plain sync Vec" with the async boundary carried
// by tokio at the root.

use std::collections::HashMap;

use crate::sql::{result_rows, SqlRunner, SqliteSeam};
use crate::types::{
    Arrival, ArrivalSign, IncrementalRelationPlan, RelationKind, Row, SqlStatement, Value,
};

#[derive(Clone)]
pub struct DeltaEvent {
    pub rel: String,
    pub sign: i8,
    pub sequence: u64,
    pub row: Row,
}

pub fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn values_sql(row_count: usize, column_count: usize) -> String {
    let row = format!("({})", vec!["?"; column_count].join(", "));
    vec![row; row_count].join(", ")
}

pub fn json_array_text(items: &[Value]) -> String {
    let parts: Vec<String> = items.iter().map(value_to_json).collect();
    format!("[{}]", parts.join(","))
}

fn value_to_json(value: &Value) -> String {
    match value {
        Value::Integer(v) => format!("{}", v),
        Value::Real(v) => crate::ticklog::js_float_text(*v),
        Value::Bool(b) => (if *b { "true" } else { "false" }).to_string(),
        Value::Text(v) => crate::ticklog::json_string(v),
    }
}

fn bind_args(values: &[Value]) -> Vec<Value> {
    values
        .iter()
        .map(|value| match value {
            Value::Bool(b) => Value::Integer(if *b { 1 } else { 0 }),
            other => other.clone(),
        })
        .collect()
}

fn boundary_stage_statement(
    relation: &IncrementalRelationPlan,
    events: &[DeltaEvent],
) -> SqlStatement {
    let mut columns = vec!["_sign".to_string(), "_sequence".to_string()];
    columns.extend(relation.columns.clone());
    let columns_text: Vec<String> = columns.iter().map(|c| quote_identifier(c)).collect();
    let value_expressions: Vec<String> = columns
        .iter()
        .enumerate()
        .map(|(index, _)| format!("json_extract(value, '$[{}]')", index))
        .collect();
    let encoded: String = events
        .iter()
        .map(|event| {
            let mut entry = Vec::new();
            entry.push(Value::Integer(event.sign as i64));
            entry.push(Value::Integer(event.sequence as i64));
            entry.extend(event.row.clone());
            json_array_text(&entry)
        })
        .collect::<Vec<_>>()
        .join(",");
    SqlStatement {
        sql: format!(
            "INSERT INTO {} ({}) SELECT {} FROM json_each(?)",
            quote_identifier(&relation.delta_table_name),
            columns_text.join(", "),
            value_expressions.join(", ")
        ),
        args: vec![Value::Text(format!("[{}]", encoded))],
    }
}

fn frontier_stage_statement(
    relation: &IncrementalRelationPlan,
    table_name: &str,
    phase: i64,
    events: &[DeltaEvent],
) -> SqlStatement {
    let mut columns = vec!["_phase".to_string(), "_sequence".to_string()];
    columns.extend(relation.columns.clone());
    let columns_text: Vec<String> = columns.iter().map(|c| quote_identifier(c)).collect();
    let value_expressions: Vec<String> = columns
        .iter()
        .enumerate()
        .map(|(index, _)| format!("json_extract(value, '$[{}]')", index))
        .collect();
    let encoded: String = events
        .iter()
        .map(|event| {
            let mut entry = vec![Value::Integer(phase), Value::Integer(event.sequence as i64)];
            entry.extend(event.row.clone());
            json_array_text(&entry)
        })
        .collect::<Vec<_>>()
        .join(",");
    SqlStatement {
        sql: format!(
            "INSERT INTO {} ({}) SELECT {} FROM json_each(?)",
            quote_identifier(table_name),
            columns_text.join(", "),
            value_expressions.join(", ")
        ),
        args: vec![Value::Text(format!("[{}]", encoded))],
    }
}

pub fn stage_events(
    seam: &SqliteSeam,
    relations: &[IncrementalRelationPlan],
    events: &[DeltaEvent],
    frontier_copies: &[(String, i64)],
) {
    if events.is_empty() {
        return;
    }
    let relation_by_name: HashMap<&str, &IncrementalRelationPlan> =
        relations.iter().map(|r| (r.rel.as_str(), r)).collect();
    let mut events_by_rel: HashMap<&str, Vec<DeltaEvent>> = HashMap::new();
    for event in events {
        events_by_rel
            .entry(event.rel.as_str())
            .or_default()
            .push(event.clone());
    }
    let mut statements = Vec::new();
    for (rel, grouped) in &events_by_rel {
        let relation = relation_by_name
            .get(rel)
            .expect("incremental delta relation missing");
        let additions: Vec<DeltaEvent> = grouped.iter().filter(|e| e.sign == 1).cloned().collect();
        statements.push(boundary_stage_statement(relation, grouped));
        if !additions.is_empty() {
            for (table_name, phase) in frontier_copies {
                statements.push(frontier_stage_statement(
                    relation, table_name, *phase, &additions,
                ));
            }
        }
    }
    seam.batch(&statements).expect("stage_events batch failed");
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

fn row_key(row: &Row, indices: &[usize]) -> String {
    let values: Vec<String> = indices
        .iter()
        .map(|index| value_to_json(&row[*index]))
        .collect();
    format!("[{}]", values.join(","))
}

fn keyed_arrival_rows_statement(
    relation: &IncrementalRelationPlan,
    entries: &[(u64, Row)],
    key_indices: &[usize],
) -> SqlStatement {
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
    for (_, row) in entries {
        let key: Row = key_indices
            .iter()
            .map(|index| row[*index].clone())
            .collect();
        if !distinct_keys.contains(&key) {
            distinct_keys.push(key);
        }
    }
    let args: Vec<Value> = distinct_keys
        .iter()
        .flat_map(|key| bind_args(key))
        .collect();
    SqlStatement {
        sql: format!(
            "SELECT {} FROM {} WHERE ({}) IN ({})",
            columns_text.join(", "),
            quote_identifier(&relation.table_name),
            key_columns_text.join(", "),
            values_sql(distinct_keys.len(), key_columns_text.len())
        ),
        args,
    }
}

// Port of IncrementalRuntime.apply_arrivals. Groups consecutive same-rel/sign
// arrivals and writes them through the relation's arrival_add/arrival_del SQL.
pub fn apply_arrivals(
    seam: &SqliteSeam,
    arrivals: &[Arrival],
    relations: &[IncrementalRelationPlan],
) {
    if arrivals.is_empty() {
        return;
    }
    let relation_by_name: HashMap<&str, &IncrementalRelationPlan> =
        relations.iter().map(|r| (r.rel.as_str(), r)).collect();
    // Group consecutive same rel+sign.
    type ArrivalGroup<'a> = (&'a IncrementalRelationPlan, i8, Vec<(u64, Row)>);

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
    for (relation, sign, entries) in groups {
        let sql = if sign == 1 {
            relation
                .arrival_add_sql
                .clone()
                .expect("incremental add statement missing")
        } else {
            relation
                .arrival_del_sql
                .clone()
                .expect("incremental delete statement missing")
        };
        let encoded_rows: String = entries
            .iter()
            .map(|(_, row)| json_array_text(row))
            .collect::<Vec<_>>()
            .join(",");
        let write_statement = SqlStatement {
            sql,
            args: vec![Value::Text(format!("[{}]", encoded_rows))],
        };
        let key_indices = relation.key_indices.clone();
        if relation.kind == RelationKind::Set && sign == 1 && !key_indices.is_empty() {
            let before_result = seam
                .execute(&keyed_arrival_rows_statement(
                    relation,
                    &entries,
                    &key_indices,
                ))
                .expect("keyed arrival rows lookup failed");
            let before_rows =
                result_rows(&before_result, &relation.columns, &relation.column_types);
            let mut current_by_key: HashMap<String, Row> = before_rows
                .iter()
                .map(|row| (row_key(row, &key_indices), row.clone()))
                .collect();
            let mut events = Vec::new();
            for (sequence, row) in &entries {
                let key = row_key(row, &key_indices);
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
            let result = seam
                .execute(&write_statement)
                .expect("arrival write failed");
            let _ = result.rows_affected;
            stage_events(
                seam,
                relations,
                &events,
                &[(relation.frontier_table_name.clone(), 1)],
            );
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
                relations,
                &events,
                &[(relation.frontier_table_name.clone(), 1)],
            );
            continue;
        }
        let changed_rows = result_rows(&result, &relation.columns, &relation.column_types);
        let mut staged_rows: Vec<String> = Vec::new();
        for (index, (sequence, _)) in entries.iter().enumerate() {
            let stored_row = changed_rows.get(index);
            let Some(stored_row) = stored_row else {
                continue;
            };
            let row_text = json_array_text(stored_row);
            if staged_rows.contains(&row_text) {
                continue;
            }
            staged_rows.push(row_text);
            events.push(DeltaEvent {
                rel: relation.rel.clone(),
                sign,
                sequence: *sequence,
                row: stored_row.clone(),
            });
        }
        stage_events(
            seam,
            relations,
            &events,
            &[(relation.frontier_table_name.clone(), 1)],
        );
    }
}

fn rows_equal(left: &Row, right: &Row) -> bool {
    left == right
}

// One statement per tick: the clock the oracle fixes for the whole tick.
pub fn advance_tick(seam: &SqliteSeam) {
    seam.execute_multiple("UPDATE \"__tick\" SET \"n\" = \"n\" + 1")
        .expect("advance_tick failed");
}

pub fn prepare_tick(seam: &SqliteSeam, relations: &[IncrementalRelationPlan]) {
    if relations.is_empty() {
        return;
    }
    let mut statements = Vec::new();
    for relation in relations {
        statements.push(format!(
            "DELETE FROM {}",
            quote_identifier(&relation.delta_table_name)
        ));
        statements.push(format!(
            "DELETE FROM {}",
            quote_identifier(&relation.next_frontier_table_name)
        ));
    }
    let sql = statements.join(";\n");
    seam.execute_multiple(&sql).expect("prepare_tick failed");
}

// Port of boundary_delta: sum sign*count over each relation's delta table and
// split into add/del rows.
pub fn boundary_delta(
    relation: &IncrementalRelationPlan,
    result: &crate::types::QueryResult,
) -> crate::types::RelDelta {
    let sign_index = result.columns.iter().position(|c| c == "__sign");
    let count_index = result.columns.iter().position(|c| c == "__count");
    let mut weights: Vec<(Row, i64)> = Vec::new();
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
            ));
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
        if let Some(existing) = weights
            .iter_mut()
            .find(|(existing_row, _)| existing_row == &values)
        {
            existing.1 += weight;
        } else {
            weights.push((values, weight));
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
    crate::types::RelDelta {
        rel: relation.rel.clone(),
        add,
        del,
    }
}

fn normalize_boundary(value: Value, ty: Option<crate::types::RowColumnType>) -> Value {
    match (ty, value) {
        (Some(crate::types::RowColumnType::Bool), Value::Integer(v)) => Value::Bool(v != 0),
        (Some(crate::types::RowColumnType::Bool), v) => v,
        (Some(crate::types::RowColumnType::Float), Value::Real(v)) => {
            if !v.is_finite() {
                panic!("float column crossed SQLite with non-finite value");
            }
            Value::Real(if v == 0.0 { 0.0 } else { v })
        }
        (_, v) => v,
    }
}

pub fn read_boundary(
    seam: &SqliteSeam,
    relations: &[IncrementalRelationPlan],
) -> Vec<crate::types::RelDelta> {
    relations
        .iter()
        .map(|relation| {
            let result = seam
                .execute(&SqlStatement {
                    sql: relation.boundary_sql.clone(),
                    args: vec![],
                })
                .expect("boundary read failed");
            boundary_delta(relation, &result)
        })
        .collect()
}

pub fn stage_departures(
    seam: &SqliteSeam,
    relations: &[IncrementalRelationPlan],
    deltas: &[crate::types::RelDelta],
) {
    let mut statements = Vec::new();
    for relation in relations {
        let Some(table_name) = &relation.departure_frontier_table_name else {
            continue;
        };
        statements.push(SqlStatement {
            sql: format!("DELETE FROM {}", quote_identifier(table_name)),
            args: vec![],
        });
        let departed = deltas
            .iter()
            .find(|delta| delta.rel == relation.rel)
            .map(|delta| delta.del.as_slice())
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
            .collect::<Vec<_>>()
            .join(",");
        statements.push(SqlStatement {
            sql: format!(
                "INSERT INTO {} ({}) SELECT {} FROM json_each(?)",
                quote_identifier(table_name),
                quoted_columns.join(", "),
                value_expressions.join(", ")
            ),
            args: vec![Value::Text(format!("[{}]", encoded))],
        });
    }
    if !statements.is_empty() {
        seam.batch(&statements).expect("departure staging failed");
    }
}

pub fn stage_ordered_frontiers(
    seam: &SqliteSeam,
    relations: &[IncrementalRelationPlan],
    additions: &[crate::types::RelDelta],
) -> bool {
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
        statements.push(frontier_stage_statement(
            relation,
            &relation.frontier_table_name,
            0,
            events,
        ));
    }
    if !statements.is_empty() {
        seam.batch(&statements)
            .expect("ordered frontier staging failed");
    }
    carry_pending
}

// Port of promote_frontiers: read carry, promote next into current.
pub fn promote_frontiers(seam: &SqliteSeam, relations: &[IncrementalRelationPlan]) -> bool {
    if relations.is_empty() {
        return false;
    }
    let mut carry_terms = Vec::new();
    for relation in relations {
        carry_terms.push(format!(
            "EXISTS (SELECT 1 FROM {} LIMIT 1)",
            quote_identifier(&relation.next_frontier_table_name)
        ));
        if let Some(table_name) = &relation.departure_frontier_table_name {
            carry_terms.push(format!(
                "EXISTS (SELECT 1 FROM {} LIMIT 1)",
                quote_identifier(table_name)
            ));
        }
    }
    let mut promote_sql = Vec::new();
    for relation in relations {
        let mut columns = vec!["_phase".to_string(), "_sequence".to_string()];
        columns.extend(relation.columns.clone());
        let columns_text: Vec<String> = columns.iter().map(|c| quote_identifier(c)).collect();
        let joined = columns_text.join(", ");
        promote_sql.push(format!(
            "DELETE FROM {}",
            quote_identifier(&relation.frontier_table_name)
        ));
        promote_sql.push(format!(
            "INSERT INTO {} ({}) SELECT {} FROM {}",
            quote_identifier(&relation.frontier_table_name),
            joined,
            joined,
            quote_identifier(&relation.next_frontier_table_name)
        ));
        promote_sql.push(format!(
            "DELETE FROM {}",
            quote_identifier(&relation.next_frontier_table_name)
        ));
    }
    let promote = || {
        seam.execute_multiple(&promote_sql.join(";\n"))
            .expect("promote failed");
    };
    if carry_terms.is_empty() {
        promote();
        return false;
    }
    let carry_sql = format!(
        "SELECT CASE WHEN {} THEN 1 ELSE 0 END AS carry_pending",
        carry_terms.join(" OR ")
    );
    let carry_pending = seam.scalar(&carry_sql).expect("carry read failed") == 1;
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

fn recursive_heads(
    statements: &[crate::types::IncrementalLevelStatement],
    relations: &[IncrementalRelationPlan],
) -> Vec<String> {
    let mut reads_frontier_of: Vec<(String, Vec<String>)> = Vec::new();
    for statement in statements {
        let mut sources = Vec::new();
        for relation in relations {
            let frontier = quote_identifier(&relation.frontier_table_name);
            if let Some(insert_sql) = &statement.insert_sql {
                if insert_sql.contains(&frontier) {
                    sources.push(relation.rel.clone());
                }
            }
        }
        if let Some(entry) = reads_frontier_of
            .iter_mut()
            .find(|e| e.0 == statement.head_rel)
        {
            entry.1.extend(sources);
        } else {
            reads_frontier_of.push((statement.head_rel.clone(), sources));
        }
    }
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

// The insert path: runs the level's insert_sql (with RETURNING) and stages the
// produced rows as +1 events into the frontier for this pass.
fn apply_level_statement(
    seam: &SqliteSeam,
    statement: &crate::types::IncrementalLevelStatement,
    relations: &[IncrementalRelationPlan],
    after_edges: bool,
    next_sequence: &mut dyn FnMut() -> u64,
) -> usize {
    let relation = relations
        .iter()
        .find(|r| r.rel == statement.head_rel)
        .expect("incremental level head relation missing");
    if let Some(aggregate) = &statement.aggregate_sql {
        apply_aggregate_level_statement(
            seam,
            statement,
            aggregate,
            relation,
            after_edges,
            next_sequence,
        );
        return 0;
    }
    let insert_sql = statement
        .insert_sql
        .as_ref()
        .expect("level statement has no insert_sql");
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
    );
    if rows.is_empty() {
        return 0;
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
    stage_events(seam, std::slice::from_ref(relation), &events, &copies);
    rows.len()
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
) {
    // The intern arm reads the scope table, so it follows the seed inside the
    // same ordered batch and precedes the insert that looks its ids back up.
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
    );
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
        ) {
            events.push(DeltaEvent {
                rel: statement.head_rel.clone(),
                sign: 1,
                sequence: next_sequence(),
                row,
            });
        }
    }
    if events.is_empty() {
        return;
    }
    let copies = level_frontier_copies(relation, after_edges);
    stage_events(seam, std::slice::from_ref(relation), &events, &copies);
}

pub fn apply_retention(
    seam: &SqliteSeam,
    statements: &[crate::types::IncrementalRetentionStatement],
    relations: &[IncrementalRelationPlan],
) {
    let mut sequence = 0u64;
    for statement in statements {
        let relation = relations
            .iter()
            .find(|r| r.rel == statement.rel)
            .expect("incremental retention relation missing");
        let result = seam
            .execute(&SqlStatement {
                sql: statement.delete_sql.clone(),
                args: vec![],
            })
            .expect("retention delete failed");
        let rows = result_rows(&result, &relation.columns, &relation.column_types);
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
        stage_events(seam, std::slice::from_ref(relation), &events, &[]);
    }
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
) {
    if statements.is_empty() {
        return;
    }
    let feeds_another_round = recursive_heads(statements, relations);
    let mut sequence = 0u64;
    let mut next_sequence = || {
        let current = sequence;
        sequence += 1;
        current
    };
    for statement in statements {
        let closes_in_one_pass = feeds_another_round.iter().any(|h| h == &statement.head_rel)
            && statement.support_sql.is_some();
        if closes_in_one_pass {
            let copies: Vec<(String, i64)> = relations
                .iter()
                .filter(|r| r.rel == statement.head_rel)
                .map(|r| (r.frontier_table_name.clone(), 2))
                .collect();
            reconcile_ref_count_statement(seam, statement, relations, &copies);
        } else {
            apply_level_statement(seam, statement, relations, false, &mut next_sequence);
        }
    }
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
) -> SqlStatement {
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
    SqlStatement {
        sql: format!(
            "INSERT INTO {} ({}) VALUES {} {}",
            quote_identifier(&statement.head_table_name),
            columns.join(", "),
            values_sql(rows.len(), columns.len()),
            conflict
        ),
        args: rows.iter().flat_map(|row| bind_args(row)).collect(),
    }
}

fn edge_log_write_statement(
    statement: &crate::types::IncrementalEdgeStatement,
    rows: &[Row],
) -> SqlStatement {
    let columns = edge_columns_text(statement);
    SqlStatement {
        sql: format!(
            "INSERT INTO {} ({}) VALUES {}",
            quote_identifier(&statement.head_table_name),
            columns.join(", "),
            values_sql(rows.len(), columns.len())
        ),
        args: rows.iter().flat_map(|row| bind_args(row)).collect(),
    }
}

fn apply_log_edge(
    seam: &SqliteSeam,
    statement: &crate::types::IncrementalEdgeStatement,
    relation: &IncrementalRelationPlan,
    rows: &[Row],
) {
    if rows.is_empty() {
        return;
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
    seam.execute(&edge_log_write_statement(statement, rows))
        .expect("edge log write failed");
    stage_events(
        seam,
        std::slice::from_ref(relation),
        &events,
        &[(relation.next_frontier_table_name.clone(), 0)],
    );
}

fn apply_keyed_edge(
    seam: &SqliteSeam,
    statement: &crate::types::IncrementalEdgeStatement,
    relation: &IncrementalRelationPlan,
    projected_rows: &[Row],
) {
    let mut resolved: Vec<(String, Row)> = Vec::new();
    for row in projected_rows {
        let key = row_key(row, &statement.key_indices);
        match resolved.iter_mut().find(|(existing, _)| existing == &key) {
            Some(entry) => entry.1 = row.clone(),
            None => resolved.push((key, row.clone())),
        }
    }
    let rows: Vec<Row> = resolved.into_iter().map(|(_, row)| row).collect();
    if rows.is_empty() {
        return;
    }
    let key_args: Vec<Value> = rows
        .iter()
        .flat_map(|row| {
            let key: Row = statement
                .key_indices
                .iter()
                .map(|index| row[*index].clone())
                .collect();
            bind_args(&key)
        })
        .collect();
    let before_result = seam
        .execute(&SqlStatement {
            sql: edge_keyed_rows_sql(statement, rows.len()),
            args: key_args,
        })
        .expect("edge keyed rows lookup failed");
    let head_types = relation.column_types.clone();
    let before_rows = result_rows(&before_result, &statement.head_columns, &head_types);
    let mut before_by_key: HashMap<String, Row> = HashMap::new();
    for row in &before_rows {
        before_by_key.insert(row_key(row, &statement.key_indices), row.clone());
    }
    let changed_rows: Vec<Row> = rows
        .into_iter()
        .filter(
            |row| match before_by_key.get(&row_key(row, &statement.key_indices)) {
                None => true,
                Some(before) => !rows_equal(before, row),
            },
        )
        .collect();
    if changed_rows.is_empty() {
        return;
    }
    let mut events = Vec::new();
    for (sequence, row) in changed_rows.iter().enumerate() {
        let key = row_key(row, &statement.key_indices);
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
    seam.execute(&edge_keyed_write_statement(statement, &changed_rows))
        .expect("edge keyed write failed");
    stage_events(
        seam,
        std::slice::from_ref(relation),
        &events,
        &[(relation.next_frontier_table_name.clone(), 0)],
    );
}

pub fn apply_edges(
    seam: &SqliteSeam,
    statements: &[crate::types::IncrementalEdgeStatement],
    relations: &[IncrementalRelationPlan],
) {
    for statement in statements {
        let relation = relations
            .iter()
            .find(|r| r.rel == statement.head_rel)
            .expect("incremental edge head relation missing");
        let result = intern_then_execute(
            seam,
            statement.intern_sql.as_ref(),
            &SqlStatement {
                sql: statement.project_sql.clone(),
                args: vec![],
            },
        );
        let rows = result_rows(&result, &statement.head_columns, &relation.column_types);
        match statement.head_kind {
            RelationKind::Log => apply_log_edge(seam, statement, relation, &rows),
            RelationKind::Set => apply_keyed_edge(seam, statement, relation, &rows),
        }
    }
}

pub fn merge_next_into_current(seam: &SqliteSeam, relations: &[IncrementalRelationPlan]) {
    if relations.is_empty() {
        return;
    }
    let sql: Vec<String> = relations
        .iter()
        .map(|relation| {
            let mut columns = vec!["_phase".to_string(), "_sequence".to_string()];
            columns.extend(relation.columns.clone());
            let columns_text: Vec<String> = columns.iter().map(|c| quote_identifier(c)).collect();
            let joined = columns_text.join(", ");
            format!(
                "INSERT INTO {} ({}) SELECT {} FROM {}",
                quote_identifier(&relation.frontier_table_name),
                joined,
                joined,
                quote_identifier(&relation.next_frontier_table_name)
            )
        })
        .collect();
    seam.execute_multiple(&sql.join(";\n"))
        .expect("merge_next_into_current failed");
}

pub fn apply_levels_after_edges(
    seam: &SqliteSeam,
    statements: &[crate::types::IncrementalLevelStatement],
    relations: &[IncrementalRelationPlan],
) {
    let mut sequence = 0u64;
    let mut next_sequence = || {
        let current = sequence;
        sequence += 1;
        current
    };
    for statement in statements {
        apply_level_statement(seam, statement, relations, true, &mut next_sequence);
    }
}

// The frozen mid-tick level plane: a level row an arrival retracted this tick
// must be gone before an edge body reads it.
pub fn recompute_levels_before_edges(
    seam: &SqliteSeam,
    statements: &[crate::types::IncrementalLevelStatement],
    relations: &[IncrementalRelationPlan],
    reconcile_every_tick: bool,
    arrival_count: usize,
) {
    if arrival_count == 0 || relations.is_empty() {
        return;
    }
    let ref_count_statements: Vec<&crate::types::IncrementalLevelStatement> = statements
        .iter()
        .filter(|statement| statement.aggregate_sql.is_none())
        .collect();
    if ref_count_statements.is_empty() {
        return;
    }
    let reconcile = |seam: &SqliteSeam| {
        for statement in &ref_count_statements {
            let relation = relations
                .iter()
                .find(|r| r.rel == statement.head_rel)
                .expect("incremental level head relation missing");
            reconcile_ref_count_statement(
                seam,
                statement,
                relations,
                &[(relation.frontier_table_name.clone(), 2)],
            );
        }
    };
    if reconcile_every_tick {
        reconcile(seam);
        return;
    }
    let guard = retraction_guard_sql(relations);
    if seam.scalar(&guard).expect("retraction guard read failed") == 1 {
        reconcile(seam);
    }
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
    frontier_copies: &[(String, i64)],
) {
    let support_sql = statement
        .support_sql
        .as_ref()
        .expect("level statement has no support_sql");
    let relation = relations
        .iter()
        .find(|r| r.rel == statement.head_rel)
        .expect("incremental level head relation missing");
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
    let mut tail = to_statements(&tail_texts);
    for (table_name, phase) in frontier_copies {
        let stage = if *table_name == relation.next_frontier_table_name {
            stage_next_frontier.clone()
        } else {
            stage_frontier.clone()
        };
        tail.push(SqlStatement {
            sql: stage,
            args: vec![Value::Integer(*phase)],
        });
    }
    tail.push(SqlStatement {
        sql: insert_new.clone(),
        args: vec![],
    });

    if statement.expand_sql.is_none() {
        let mut head = Vec::new();
        head.push(SqlStatement {
            sql: clear.clone(),
            args: vec![],
        });
        head.extend(to_statements(&support_interns));
        head.push(SqlStatement {
            sql: statement.support_sql.as_ref().unwrap()[1].clone(),
            args: vec![],
        });
        head.extend(tail);
        seam.batch(&head).expect("reconcile batch failed");
        return;
    }
    // The expand (wavefront) path joins in a later widening step. Until then a
    // plain support close keeps the plan correct for additive deps.
    let mut head = Vec::new();
    head.push(SqlStatement {
        sql: clear.clone(),
        args: vec![],
    });
    head.extend(to_statements(&support_interns));
    head.push(SqlStatement {
        sql: statement.support_sql.as_ref().unwrap()[1].clone(),
        args: vec![],
    });
    head.extend(to_statements(&tail_texts));
    head.push(SqlStatement {
        sql: insert_new.clone(),
        args: vec![],
    });
    seam.batch(&head)
        .expect("reconcile expand-placeholder failed");
}

// Port of recompute_levels_after_edges (1_incremental.ts:1243). Guarded by the
// retraction guard; a purely additive tick skips the reconcile entirely.
pub fn recompute_levels_after_edges(
    seam: &SqliteSeam,
    statements: &[crate::types::IncrementalLevelStatement],
    relations: &[IncrementalRelationPlan],
    reconcile_every_tick: bool,
) {
    if statements.is_empty() {
        return;
    }
    // No frontier copies on either arm: a reconcile row is a correction inside
    // the same closure, never post-write growth, so it must not carry.
    let reconcile = |seam: &SqliteSeam| {
        let mut sequence = 0u64;
        let mut next_sequence = || {
            let current = sequence;
            sequence += 1;
            current
        };
        for statement in statements {
            let relation = relations
                .iter()
                .find(|r| r.rel == statement.head_rel)
                .expect("incremental level head relation missing");
            if let Some(aggregate) = &statement.aggregate_sql {
                if aggregate.delta_maintained {
                    continue;
                }
                apply_aggregate_level_statement(
                    seam,
                    statement,
                    aggregate,
                    relation,
                    false,
                    &mut next_sequence,
                );
                continue;
            }
            reconcile_ref_count_statement(seam, statement, relations, &[]);
        }
    };
    if reconcile_every_tick {
        reconcile(seam);
        return;
    }
    if relations.is_empty() {
        return;
    }
    let guard = retraction_guard_sql(relations);
    let has_retraction = seam.scalar(&guard).expect("retraction guard read failed") == 1;
    if has_retraction {
        reconcile(seam);
    }
}
