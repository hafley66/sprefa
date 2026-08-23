use std::collections::HashMap;

use serde_json::Value as JsonValue;

use crate::sql::{column_index, SqlRunner, SqliteSeam};
use crate::types::{
    Arrival, ArrivalSign, BoundaryResult, IncrementalRelationPlan, Row, ScalarValue, SqlStatement,
    StructTypePlan, TextInternPlan, Value,
};

#[derive(Clone)]
enum CollectedField {
    Scalar(Value),
    Child(String),
}

#[derive(Clone)]
struct Collected {
    semantic: String,
    fields: Vec<CollectedField>,
}

fn parse_struct(value: &Value) -> JsonValue {
    let Value::Text(text) = value else {
        panic!("type_arrival_shape_mismatch: not_an_object")
    };
    serde_json::from_str(text)
        .unwrap_or_else(|_| panic!("type_arrival_shape_mismatch: not_an_object"))
}

fn canonical_text(value: &JsonValue) -> String {
    serde_json::to_string(value).expect("struct value serialization failed")
}

fn semantic_key(type_name: &str, rendered: &str) -> String {
    format!("{}{}", type_name, rendered)
}

fn json_scalar(value: &JsonValue) -> Value {
    match value {
        JsonValue::Number(number) => match number.as_i64() {
            Some(integer) => Value::Integer(integer),
            None => Value::Real(number.as_f64().expect("struct number is outside f64")),
        },
        JsonValue::Bool(flag) => Value::Bool(*flag),
        JsonValue::String(text) => Value::Text(text.clone()),
        JsonValue::Null => Value::Text(String::new()),
        JsonValue::Array(_) | JsonValue::Object(_) => Value::Text(canonical_text(value)),
    }
}

type CollectedBucket = (Vec<Collected>, HashMap<String, usize>);

fn collect(
    plan: &StructTypePlan,
    by_name: &HashMap<&str, &StructTypePlan>,
    value: &JsonValue,
    per_type: &mut HashMap<String, CollectedBucket>,
) -> String {
    let object = value
        .as_object()
        .unwrap_or_else(|| panic!("type_arrival_shape_mismatch: not_an_object({})", plan.name));
    for column in &plan.columns {
        if !object.contains_key(column) {
            panic!(
                "type_arrival_shape_mismatch: missing_key({}, {})",
                plan.name, column
            );
        }
    }
    for key in object.keys() {
        if !plan.columns.contains(key) {
            panic!(
                "type_arrival_shape_mismatch: unknown_key({}, {})",
                plan.name, key
            );
        }
    }
    let fields = plan
        .columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let field = &object[column];
            match plan.refs.get(index).and_then(Option::as_deref) {
                None => CollectedField::Scalar(json_scalar(field)),
                Some(child_name) => {
                    let child = by_name.get(child_name).unwrap_or_else(|| {
                        panic!("type_arrival_shape_mismatch: column_type_unknown({child_name})")
                    });
                    CollectedField::Child(collect(child, by_name, field, per_type))
                }
            }
        })
        .collect();
    let rendered = canonical_text(value);
    let semantic = semantic_key(&plan.name, &rendered);
    let bucket = per_type.entry(plan.name.clone()).or_default();
    let collected = Collected {
        semantic: semantic.clone(),
        fields,
    };
    COLLECT_PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    match bucket.1.get(&semantic) {
        Some(index) => bucket.0[*index] = collected,
        None => {
            bucket.1.insert(semantic.clone(), bucket.0.len());
            bucket.0.push(collected);
        }
    }
    semantic
}

/// One comparison per collected value under an index, one per pair under a scan.
static COLLECT_PROBES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn collect_probes() -> u64 {
    COLLECT_PROBES.load(std::sync::atomic::Ordering::Relaxed)
}

fn resolve_fields(collected: &Collected, ids: &HashMap<String, i64>) -> Row {
    collected
        .fields
        .iter()
        .map(|field| match field {
            CollectedField::Scalar(value) => value.clone(),
            CollectedField::Child(semantic) => Value::Integer(
                *ids.get(semantic)
                    .unwrap_or_else(|| panic!("relation reference child id missing: {semantic}")),
            ),
        })
        .collect()
}

fn encoded_rows(rows: &[Row]) -> BoundaryResult<ScalarValue> {
    let encoded = rows
        .iter()
        .map(|row| crate::incremental::json_array_text(row))
        .collect::<BoundaryResult<Vec<_>>>()?
        .join(",");
    Ok(ScalarValue::Text(format!("[{}]", encoded)))
}

fn intern_type(
    work: &crate::incremental::TickWork,
    seam: &SqliteSeam,
    plan: &StructTypePlan,
    collected: &[Collected],
    ids: &mut HashMap<String, i64>,
    relations: &[IncrementalRelationPlan],
    text_plan: Option<&TextInternPlan>,
) -> BoundaryResult<()> {
    let rows: Vec<Row> = collected
        .iter()
        .map(|entry| resolve_fields(entry, ids))
        .collect();
    let arrivals: Vec<Arrival> = rows
        .iter()
        .map(|row| Arrival {
            rel: plan.name.clone(),
            sign: ArrivalSign::Add,
            row: row.clone(),
        })
        .collect();
    let staged = match text_plan {
        Some(text_plan) => {
            crate::text_plane::intern(seam, text_plan, std::borrow::Cow::Owned(arrivals))?
        }
        None => std::borrow::Cow::Owned(arrivals),
    };
    let staged_rows: Vec<Row> = staged.iter().map(|arrival| arrival.row.clone()).collect();
    let encoded = encoded_rows(&staged_rows)?;
    let conflict = seam
        .execute(&SqlStatement {
            sql: plan.conflict_sql.clone(),
            args: vec![encoded.clone()],
        })
        .expect("relation reference conflict query failed");
    if !conflict.rows.is_empty() {
        panic!("relation_reference_conflict({})", plan.name);
    }
    crate::incremental::apply_arrivals(seam, &staged, relations, work)?;
    let lookup = seam
        .execute(&SqlStatement {
            sql: plan.lookup_sql.clone(),
            args: vec![encoded],
        })
        .expect("relation reference lookup failed");
    let lookup_index = column_index(&lookup, "__lookup").expect("struct lookup column missing");
    let id_index = column_index(&lookup, "__id").expect("struct id column missing");
    let mut semantics_by_tuple: HashMap<String, &str> = HashMap::new();
    for (entry, row) in collected.iter().zip(staged_rows.iter()) {
        semantics_by_tuple.insert(crate::incremental::json_array_text(row)?, &entry.semantic);
    }
    for row in &lookup.rows {
        let tuple = match row.get(lookup_index) {
            Some(Value::Text(tuple)) => tuple,
            _ => panic!("struct lookup tuple missing"),
        };
        let id = row
            .get(id_index)
            .and_then(Value::as_i64)
            .expect("struct lookup id missing");
        let semantic = semantics_by_tuple
            .get(tuple)
            .unwrap_or_else(|| panic!("relation reference lookup returned an unknown row {tuple}"));
        ids.insert((*semantic).to_string(), id);
    }
    Ok(())
}

fn rewrite_row(row: &Row, refs: &[Option<String>], ids: &HashMap<String, i64>) -> Row {
    row.iter()
        .enumerate()
        .map(
            |(index, value)| match refs.get(index).and_then(Option::as_deref) {
                None => value.clone(),
                Some(type_name) => {
                    let decoded = parse_struct(value);
                    let rendered = canonical_text(&decoded);
                    let semantic = semantic_key(type_name, &rendered);
                    Value::Integer(*ids.get(&semantic).unwrap_or_else(|| {
                        panic!("relation reference normalization lost the id for {semantic}")
                    }))
                }
            },
        )
        .collect()
}

/// The distinct struct values one batch names, per type name. Public so a
/// COUNT test can read `collect_probes` over it without a seam.
pub fn distinct_struct_values(
    types: &[StructTypePlan],
    ref_columns: &HashMap<String, Vec<Option<String>>>,
    arrivals: &[Arrival],
) -> usize {
    let by_name: HashMap<&str, &StructTypePlan> = types
        .iter()
        .map(|plan| (plan.name.as_str(), plan))
        .collect();
    collect_arrivals(&by_name, ref_columns, arrivals)
        .values()
        .map(|bucket| bucket.0.len())
        .sum()
}

fn collect_arrivals(
    by_name: &HashMap<&str, &StructTypePlan>,
    ref_columns: &HashMap<String, Vec<Option<String>>>,
    arrivals: &[Arrival],
) -> HashMap<String, CollectedBucket> {
    let mut per_type: HashMap<String, CollectedBucket> = HashMap::new();
    for arrival in arrivals.iter() {
        let Some(refs) = ref_columns.get(&arrival.rel) else {
            continue;
        };
        for (index, value) in arrival.row.iter().enumerate() {
            let Some(type_name) = refs.get(index).and_then(Option::as_deref) else {
                continue;
            };
            let plan = by_name
                .get(type_name)
                .unwrap_or_else(|| panic!("struct type plan missing: {type_name}"));
            let decoded = parse_struct(value);
            collect(plan, by_name, &decoded, &mut per_type);
        }
    }
    per_type
}

pub fn intern<'a>(
    seam: &SqliteSeam,
    types: &[StructTypePlan],
    ref_columns: &HashMap<String, Vec<Option<String>>>,
    arrivals: std::borrow::Cow<'a, [Arrival]>,
    relations: &[IncrementalRelationPlan],
    text_plan: Option<&TextInternPlan>,
    work: &crate::incremental::TickWork,
) -> BoundaryResult<std::borrow::Cow<'a, [Arrival]>> {
    if types.is_empty() || arrivals.is_empty() {
        return Ok(arrivals);
    }
    let by_name: HashMap<&str, &StructTypePlan> = types
        .iter()
        .map(|plan| (plan.name.as_str(), plan))
        .collect();
    let per_type = collect_arrivals(&by_name, ref_columns, &arrivals);
    if per_type.is_empty() {
        return Ok(arrivals);
    }
    let mut ids = HashMap::new();
    for plan in types {
        if let Some(collected) = per_type.get(&plan.name) {
            intern_type(
                work,
                seam,
                plan,
                &collected.0,
                &mut ids,
                relations,
                text_plan,
            )?;
        }
    }
    let rewritten: Vec<Arrival> = arrivals
        .iter()
        .map(|arrival| match ref_columns.get(&arrival.rel) {
            None => arrival.clone(),
            Some(refs) => Arrival {
                rel: arrival.rel.clone(),
                sign: arrival.sign,
                row: rewrite_row(&arrival.row, refs, &ids),
            },
        })
        .collect();
    Ok(std::borrow::Cow::Owned(rewritten))
}
