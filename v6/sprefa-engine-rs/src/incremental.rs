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
            .collect();
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
