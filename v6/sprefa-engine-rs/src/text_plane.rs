// Two statements per non-empty batch, flat in the DISTINCT arriving value
// count. Runs before apply_arrivals: content must not reach an id column.

use std::collections::HashMap;

use crate::sql::{SqlRunner, SqliteSeam};
use crate::types::{Arrival, Row, SqlStatement, TextInternPlan, Value};

// A number reaching a text column interns as its rendering, the text the
// column carried before the storage flip.
fn content_of(value: &Value) -> String {
    match value {
        Value::Text(text) => text.clone(),
        Value::Integer(number) => format!("{}", number),
        Value::Real(number) => crate::ticklog::js_float_text(*number),
        Value::Bool(flag) => (if *flag { "true" } else { "false" }).to_string(),
    }
}

fn collect_values(plan: &TextInternPlan, arrivals: &[Arrival]) -> Vec<String> {
    let mut values: Vec<String> = Vec::new();
    for arrival in arrivals {
        let Some(flags) = plan.rel_columns.get(&arrival.rel) else {
            continue;
        };
        for (index, value) in arrival.row.iter().enumerate() {
            if flags.get(index) != Some(&true) {
                continue;
            }
            let content = content_of(value);
            if !values.contains(&content) {
                values.push(content);
            }
        }
    }
    values
}

fn rewrite_row(row: &Row, flags: &[bool], ids: &HashMap<String, i64>) -> Row {
    row.iter()
        .enumerate()
        .map(|(index, value)| {
            if flags.get(index) != Some(&true) {
                return value.clone();
            }
            let content = content_of(value);
            let id = ids
                .get(&content)
                .unwrap_or_else(|| panic!("text intern lost the id for {:?}", content));
            Value::Integer(*id)
        })
        .collect()
}

pub fn intern(seam: &SqliteSeam, plan: &TextInternPlan, arrivals: &[Arrival]) -> Vec<Arrival> {
    if arrivals.is_empty() {
        return arrivals.to_vec();
    }
    let values = collect_values(plan, arrivals);
    if values.is_empty() {
        return arrivals.to_vec();
    }
    let encoded = Value::Text(crate::incremental::json_array_text(
        &values
            .iter()
            .map(|content| Value::Text(content.clone()))
            .collect::<Vec<_>>(),
    ));
    seam.execute(&SqlStatement {
        sql: plan.intern_sql.clone(),
        args: vec![encoded.clone()],
    })
    .expect("text intern write failed");
    let result = seam
        .execute(&SqlStatement {
            sql: plan.lookup_sql.clone(),
            args: vec![encoded],
        })
        .expect("text intern lookup failed");
    let lookup_index = crate::sql::column_index(&result, "__lookup");
    let id_index = crate::sql::column_index(&result, "__id");
    let mut ids: HashMap<String, i64> = HashMap::new();
    if let (Some(lookup_index), Some(id_index)) = (lookup_index, id_index) {
        for row in &result.rows {
            let (Some(content), Some(id)) = (row.get(lookup_index), row.get(id_index)) else {
                continue;
            };
            ids.insert(content_of(content), id.as_i64().unwrap_or_default());
        }
    }
    arrivals
        .iter()
        .map(|arrival| match plan.rel_columns.get(&arrival.rel) {
            None => arrival.clone(),
            Some(flags) => Arrival {
                rel: arrival.rel.clone(),
                sign: arrival.sign,
                row: rewrite_row(&arrival.row, flags, &ids),
            },
        })
        .collect()
}
