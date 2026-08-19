//! Tagged enum values at the public boundary, integer endpoints in SQLite.

use std::collections::HashMap;

use crate::sql::{SqlRunner, SqliteSeam};
use crate::types::{
    Arrival, BoundaryResult, EnumRefColumns, EnumTypePlan, IncrementalRelationPlan, RelDelta, Row,
    SqlStatement, Value,
};

fn plans_by_name(plans: &[EnumTypePlan]) -> HashMap<&str, &EnumTypePlan> {
    plans
        .iter()
        .map(|plan| (plan.name.as_str(), plan))
        .collect()
}

fn tagged_object(value: &Value, enum_name: &str) -> serde_json::Map<String, serde_json::Value> {
    let Value::Text(text) = value else {
        panic!("enum_arrival_shape_mismatch: not_an_object({enum_name})")
    };
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_else(|| panic!("enum_arrival_shape_mismatch: not_an_object({enum_name})"))
}

fn json_value(value: &serde_json::Value) -> Value {
    match value {
        serde_json::Value::Number(number) => number
            .as_i64()
            .map(Value::Integer)
            .or_else(|| number.as_f64().map(Value::Real))
            .unwrap_or_else(|| panic!("enum_arrival_shape_mismatch: invalid_number")),
        serde_json::Value::Bool(value) => Value::Bool(*value),
        serde_json::Value::String(value) => Value::Text(value.clone()),
        serde_json::Value::Null => Value::Text(String::new()),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            Value::Text(serde_json::to_string(value).expect("enum value serialization failed"))
        }
    }
}

fn encode(
    plans: &HashMap<&str, &EnumTypePlan>,
    enum_name: &str,
    endpoint: i64,
    sign: crate::types::ArrivalSign,
    value: &Value,
    variant_arrivals: &mut Vec<Arrival>,
) -> Value {
    let plan = plans
        .get(enum_name)
        .unwrap_or_else(|| panic!("enum plan missing: {enum_name}"));
    let object = tagged_object(value, enum_name);
    let tag = object
        .get("tag")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("enum_arrival_shape_mismatch: missing_tag({enum_name})"));
    let variant = plan
        .variants
        .iter()
        .find(|variant| variant.tag == tag)
        .unwrap_or_else(|| panic!("enum_arrival_shape_mismatch: unknown_tag({enum_name}, {tag})"));
    for field in &variant.fields {
        if !object.contains_key(field) {
            panic!("enum_arrival_shape_mismatch: missing_key({enum_name}, {field})");
        }
    }
    for field in object.keys() {
        if field != "tag" && !variant.fields.contains(field) {
            panic!("enum_arrival_shape_mismatch: unknown_key({enum_name}, {field})");
        }
    }
    let mut row = vec![Value::Integer(endpoint)];
    for (index, field) in variant.fields.iter().enumerate() {
        let mut payload = json_value(&object[field]);
        if let Some(Some(nested)) = variant.field_enums.get(index) {
            payload = encode(plans, nested, endpoint, sign, &payload, variant_arrivals);
        }
        row.push(payload);
    }
    variant_arrivals.push(Arrival {
        rel: variant.rel.clone(),
        sign,
        row,
    });
    Value::Integer(endpoint)
}

/// Rewrites tagged public values to their integer endpoint and prepends the
/// generated variant arrivals. The endpoint comes from the owner column named
/// by the emitted schema, never from the tagged payload.
pub fn intern(
    enum_types: &[EnumTypePlan],
    ref_columns: &EnumRefColumns,
    arrivals: &[Arrival],
) -> BoundaryResult<Vec<Arrival>> {
    if enum_types.is_empty() || arrivals.is_empty() {
        return Ok(arrivals.to_vec());
    }
    let plans = plans_by_name(enum_types);
    let mut variants = Vec::new();
    let mut parents = Vec::with_capacity(arrivals.len());
    for arrival in arrivals {
        let Some(refs) = ref_columns.get(&arrival.rel) else {
            parents.push(arrival.clone());
            continue;
        };
        let mut row = arrival.row.clone();
        for (index, reference) in refs.iter().enumerate() {
            let Some(reference) = reference else { continue };
            let endpoint = arrival
                .row
                .get(reference.endpoint_index.unwrap_or(usize::MAX))
                .and_then(Value::as_i64)
                .unwrap_or_else(|| {
                    panic!(
                        "enum_arrival_shape_mismatch: ambiguous_owner_context({}, {})",
                        arrival.rel, reference.name
                    )
                });
            row[index] = encode(
                &plans,
                &reference.name,
                endpoint,
                arrival.sign,
                &arrival.row[index],
                &mut variants,
            );
        }
        parents.push(Arrival {
            rel: arrival.rel.clone(),
            sign: arrival.sign,
            row,
        });
    }
    variants.extend(parents);
    Ok(variants)
}

fn decode(
    seam: &SqliteSeam,
    plans: &HashMap<&str, &EnumTypePlan>,
    relations: &[IncrementalRelationPlan],
    enum_name: &str,
    endpoint: i64,
) -> BoundaryResult<Value> {
    let plan = plans
        .get(enum_name)
        .unwrap_or_else(|| panic!("enum plan missing: {enum_name}"));
    let mut matches = Vec::new();
    for variant in &plan.variants {
        if variant.select_sql.is_empty() {
            panic!("enum variant read missing: {}", variant.rel);
        }
        let sql = format!(
            "SELECT * FROM ({}) AS \"__enum_payload\" WHERE \"__enum_payload\".\"id\" = ?",
            variant.select_sql
        );
        let result = seam
            .execute(&SqlStatement {
                sql,
                args: vec![crate::types::ScalarValue::Integer(endpoint)],
            })
            .expect("enum payload read failed");
        for row in result.rows {
            matches.push((variant, row));
        }
    }
    if matches.len() != 1 {
        panic!("enum_boundary_shape_mismatch: ambiguous_endpoint({enum_name}, {endpoint})");
    }
    let (variant, row) = matches.pop().unwrap();
    let mut object = serde_json::Map::new();
    object.insert("tag".into(), serde_json::Value::String(variant.tag.clone()));
    for (index, field) in variant.fields.iter().enumerate() {
        let value = row
            .get(index + 1)
            .cloned()
            .unwrap_or(Value::Text(String::new()));
        let value = match variant.field_enums.get(index).and_then(Option::as_deref) {
            Some(nested) => match value.as_i64() {
                Some(endpoint) => match decode(seam, plans, relations, nested, endpoint)? {
                    Value::Text(text) => serde_json::from_str(&text).expect("nested enum JSON"),
                    _ => unreachable!(),
                },
                None => panic!("enum_boundary_shape_mismatch: nested_endpoint({nested})"),
            },
            None => public_value(value, variant.field_types.get(index).copied()),
        };
        object.insert(field.clone(), value);
    }
    Ok(Value::Text(serde_json::Value::Object(object).to_string()))
}

/// `select_sql` reads variant payloads through the same dictionary/list views
/// as final_select. This converts the canonical boundary cell to JSON only
/// after the storage id has been resolved.
fn public_value(value: Value, field_type: Option<crate::types::RowColumnType>) -> serde_json::Value {
    match value {
        Value::Integer(value) => serde_json::json!(value),
        Value::Real(value) => serde_json::json!(value),
        Value::Bool(value) => serde_json::json!(value),
        Value::List(value) => serde_json::Value::Array(value),
        Value::Bytes(value) => serde_json::json!(crate::types::bytes_to_base64(&value)),
        Value::Text(value) if matches!(field_type, Some(crate::types::RowColumnType::Json | crate::types::RowColumnType::Ref | crate::types::RowColumnType::List)) => {
            serde_json::from_str(&value).unwrap_or_else(|_| serde_json::json!(value))
        }
        Value::Text(value) => serde_json::json!(value),
    }
}

pub fn decode_deltas(
    seam: &SqliteSeam,
    enum_types: &[EnumTypePlan],
    ref_columns: &EnumRefColumns,
    relations: &[IncrementalRelationPlan],
    deltas: Vec<RelDelta>,
) -> BoundaryResult<Vec<RelDelta>> {
    if enum_types.is_empty() {
        return Ok(deltas);
    }
    let plans = plans_by_name(enum_types);
    Ok(deltas
        .into_iter()
        .map(|mut delta| {
            let Some(refs) = ref_columns.get(&delta.rel) else {
                return delta;
            };
            let decode_row = |row: &Row| -> Row {
                row.iter()
                    .enumerate()
                    .map(
                        |(index, value)| match refs.get(index).and_then(Option::as_ref) {
                            Some(reference) => decode(
                                seam,
                                &plans,
                                relations,
                                &reference.name,
                                value.as_i64().unwrap_or_else(|| {
                                    panic!(
                                        "enum_boundary_shape_mismatch: endpoint({})",
                                        reference.name
                                    )
                                }),
                            )
                            .expect("enum decode"),
                            None => value.clone(),
                        },
                    )
                    .collect()
            };
            delta.add = delta.add.iter().map(decode_row).collect();
            delta.del = delta.del.iter().map(decode_row).collect();
            delta
        })
        .collect())
}

pub fn decode_row(
    seam: &SqliteSeam,
    enum_types: &[EnumTypePlan],
    ref_columns: &EnumRefColumns,
    relations: &[IncrementalRelationPlan],
    rel: &str,
    row: &Row,
) -> BoundaryResult<Row> {
    let Some(refs) = ref_columns.get(rel) else {
        return Ok(row.clone());
    };
    let plans = plans_by_name(enum_types);
    row.iter()
        .enumerate()
        .map(
            |(index, value)| match refs.get(index).and_then(Option::as_ref) {
                Some(reference) => decode(
                    seam,
                    &plans,
                    relations,
                    &reference.name,
                    value.as_i64().unwrap_or_else(|| {
                        panic!("enum_boundary_shape_mismatch: endpoint({})", reference.name)
                    }),
                ),
                None => Ok(value.clone()),
            },
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ArrivalSign, EnumRefColumn, EnumVariantPlan};

    fn plans() -> (Vec<EnumTypePlan>, EnumRefColumns) {
        (
            vec![EnumTypePlan {
                name: "choice".into(),
                variants: vec![
                    EnumVariantPlan {
                        tag: "ok".into(),
                        rel: "choice_ok".into(),
                        fields: vec!["value".into()],
                        field_types: vec![crate::types::RowColumnType::Text],
                        field_enums: vec![None],
                        select_sql: "SELECT \"id\", \"value\" FROM \"choice_ok\"".into(),
                    },
                    EnumVariantPlan {
                        tag: "err".into(),
                        rel: "choice_err".into(),
                        fields: vec!["reason".into()],
                        field_types: vec![crate::types::RowColumnType::Text],
                        field_enums: vec![None],
                        select_sql: "SELECT \"id\", \"reason\" FROM \"choice_err\"".into(),
                    },
                ],
            }],
            HashMap::from([(
                "resident".into(),
                vec![
                    None,
                    Some(EnumRefColumn {
                        name: "choice".into(),
                        endpoint_index: Some(0),
                    }),
                ],
            )]),
        )
    }

    fn resident(value: &str) -> Arrival {
        Arrival {
            rel: "resident".into(),
            sign: ArrivalSign::Add,
            row: vec![Value::Integer(7), Value::Text(value.into())],
        }
    }

    #[test]
    fn tagged_payload_materializes_variant_then_integer_parent_endpoint() {
        let (plans, refs) = plans();
        let rows = intern(&plans, &refs, &[resident(r#"{"tag":"ok","value":"yes"}"#)]).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].rel, "choice_ok");
        assert_eq!(
            rows[0].row,
            vec![Value::Integer(7), Value::Text("yes".into())]
        );
        assert_eq!(rows[1].row, vec![Value::Integer(7), Value::Integer(7)]);
    }

    #[test]
    fn tagged_delete_materializes_a_delete_variant_and_parent() {
        let (plans, refs) = plans();
        let mut deleted = resident(r#"{"tag":"ok","value":"yes"}"#);
        deleted.sign = ArrivalSign::Del;
        let rows = intern(&plans, &refs, &[deleted]).unwrap();
        assert_eq!(rows.iter().map(|row| row.sign).collect::<Vec<_>>(), vec![ArrivalSign::Del, ArrivalSign::Del]);
    }

    #[test]
    fn nullary_variant_decodes_from_its_endpoint() {
        let seam = SqliteSeam::in_memory().unwrap();
        seam.execute_multiple("CREATE TABLE choice_none (id INTEGER); INSERT INTO choice_none VALUES (7)").unwrap();
        let plans = vec![EnumTypePlan { name: "choice".into(), variants: vec![EnumVariantPlan {
            tag: "none".into(), rel: "choice_none".into(), fields: vec![], field_types: vec![], field_enums: vec![], select_sql: "SELECT id FROM choice_none".into(),
        }]}];
        let refs = HashMap::from([("resident".into(), vec![Some(EnumRefColumn { name: "choice".into(), endpoint_index: Some(0) })])]);
        assert_eq!(decode_row(&seam, &plans, &refs, &[], "resident", &vec![Value::Integer(7)]).unwrap(), vec![Value::Text(r#"{"tag":"none"}"#.into())]);
    }

    #[test]
    fn tagged_shape_failures_are_named() {
        let (plans, refs) = plans();
        for value in [
            r#"{"tag":"nope"}"#,
            r#"{"tag":"ok"}"#,
            r#"{"tag":"ok","value":"yes","extra":1}"#,
        ] {
            assert!(
                std::panic::catch_unwind(|| intern(&plans, &refs, &[resident(value)])).is_err()
            );
        }
    }

    #[test]
    fn missing_owner_endpoint_is_named() {
        let (plans, mut refs) = plans();
        refs.insert(
            "resident".into(),
            vec![
                None,
                Some(EnumRefColumn {
                    name: "choice".into(),
                    endpoint_index: Some(9),
                }),
            ],
        );
        assert!(std::panic::catch_unwind(|| intern(
            &plans,
            &refs,
            &[resident(r#"{"tag":"ok","value":"yes"}"#)]
        ))
        .is_err());
    }

    #[test]
    fn endpoint_round_trips_to_tagged_value() {
        let seam = SqliteSeam::in_memory().unwrap();
        seam.execute_multiple("CREATE TABLE choice_ok (id INTEGER, value TEXT); CREATE TABLE choice_err (id INTEGER, reason TEXT); INSERT INTO choice_ok VALUES (7, 'yes')").unwrap();
        let (plans, refs) = plans();
        let relations = vec![
            IncrementalRelationPlan {
                rel: "choice_ok".into(),
                kind: crate::types::RelationKind::Set,
                table_name: "choice_ok".into(),
                delta_table_name: "d".into(),
                frontier_table_name: "f".into(),
                next_frontier_table_name: "n".into(),
                departure_frontier_table_name: None,
                columns: vec!["id".into(), "value".into()],
                column_types: vec![],
                key_indices: vec![],
                arrival_add_sql: None,
                arrival_del_sql: None,
                boundary_sql: String::new(),
            },
            IncrementalRelationPlan {
                rel: "choice_err".into(),
                kind: crate::types::RelationKind::Set,
                table_name: "choice_err".into(),
                delta_table_name: "d".into(),
                frontier_table_name: "f".into(),
                next_frontier_table_name: "n".into(),
                departure_frontier_table_name: None,
                columns: vec!["id".into(), "reason".into()],
                column_types: vec![],
                key_indices: vec![],
                arrival_add_sql: None,
                arrival_del_sql: None,
                boundary_sql: String::new(),
            },
        ];
        let row = decode_row(
            &seam,
            &plans,
            &refs,
            &relations,
            "resident",
            &vec![Value::Integer(7), Value::Integer(7)],
        )
        .unwrap();
        assert_eq!(
            row,
            vec![
                Value::Integer(7),
                Value::Text(r#"{"tag":"ok","value":"yes"}"#.into())
            ]
        );
    }

    #[test]
    fn payload_reads_use_public_select_and_decode_lists() {
        let seam = SqliteSeam::in_memory().unwrap();
        seam.execute_multiple("CREATE TABLE choice_ok (id INTEGER, tags TEXT); INSERT INTO choice_ok VALUES (7, '[1,2]')").unwrap();
        let plans = vec![EnumTypePlan {
            name: "choice".into(),
            variants: vec![EnumVariantPlan {
                tag: "ok".into(),
                rel: "choice_ok".into(),
                fields: vec!["tags".into()],
                field_types: vec![crate::types::RowColumnType::List],
                field_enums: vec![None],
                select_sql: "SELECT id, tags FROM choice_ok".into(),
            }],
        }];
        let refs = HashMap::from([(
            "resident".into(),
            vec![Some(EnumRefColumn { name: "choice".into(), endpoint_index: Some(0) })],
        )]);
        let row = decode_row(
            &seam, &plans, &refs, &[], "resident", &vec![Value::Integer(7)],
        ).unwrap();
        assert_eq!(row, vec![Value::Text(r#"{"tag":"ok","tags":[1,2]}"#.into())]);
    }

    #[test]
    fn nested_tagged_payloads_decode_as_objects() {
        let seam = SqliteSeam::in_memory().unwrap();
        seam.execute_multiple("CREATE TABLE outer_some (id INTEGER, value INTEGER); CREATE TABLE inner_ok (id INTEGER, value TEXT); INSERT INTO outer_some VALUES (7, 7); INSERT INTO inner_ok VALUES (7, 'yes')").unwrap();
        let plans = vec![
            EnumTypePlan {
                name: "outer".into(),
                variants: vec![EnumVariantPlan {
                    tag: "some".into(), rel: "outer_some".into(), fields: vec!["value".into()],
                    field_types: vec![crate::types::RowColumnType::RelationId], field_enums: vec![Some("inner".into())],
                    select_sql: "SELECT id, value FROM outer_some".into(),
                }],
            },
            EnumTypePlan {
                name: "inner".into(),
                variants: vec![EnumVariantPlan {
                    tag: "ok".into(), rel: "inner_ok".into(), fields: vec!["value".into()],
                    field_types: vec![crate::types::RowColumnType::Text], field_enums: vec![None],
                    select_sql: "SELECT id, value FROM inner_ok".into(),
                }],
            },
        ];
        let refs = HashMap::from([(
            "resident".into(),
            vec![Some(EnumRefColumn { name: "outer".into(), endpoint_index: Some(0) })],
        )]);
        let row = decode_row(&seam, &plans, &refs, &[], "resident", &vec![Value::Integer(7)]).unwrap();
        assert_eq!(row, vec![Value::Text(r#"{"tag":"some","value":{"tag":"ok","value":"yes"}}"#.into())]);
    }
}
