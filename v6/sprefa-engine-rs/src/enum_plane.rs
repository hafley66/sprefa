//! An enum-typed column holds a REFERENCE: the referenced instance's integer
//! id, the same carrier `commit__reviewed_by(commit_id, person_id)` uses for a
//! rel-typed reference column. The value form is the variant constructor at
//! construction position, and the compiler lowers it there; it never reaches
//! this door. Both directions carry the integer unchanged, and the emitted
//! `rel_declared_column_types` entry for such a column is already `int`, so the
//! arrival door's own `field_not_int` check is the reference check.
//!
//! Receipts: conformance/fixtures/0_enum_variants.pl enum_name_is_a_column_type
//! feeds `picked(101, 401)` and reads `picked: add [[101,401]]`;
//! 17_recursive_enum.pl carries `tree_branch(2, 1, 3)`; 0_option_type.pl
//! option_text_column_reads_through_tag_join carries `user_profile(1, 501)`.

use crate::sql::SqliteSeam;
use crate::types::{
    Arrival, BoundaryResult, EnumRefColumns, EnumTypePlan, IncrementalRelationPlan, RelDelta, Row,
};

fn check_reference(rel: &str, refs: &[Option<crate::types::EnumRefColumn>], row: &Row) {
    for (index, reference) in refs.iter().enumerate() {
        let Some(reference) = reference else { continue };
        let Some(value) = row.get(index) else {
            continue;
        };
        if value.as_i64().is_none() {
            panic!(
                "enum_arrival_shape_mismatch: not_a_reference({rel}, {})",
                reference.name
            );
        }
    }
}

pub fn intern<'a>(
    _seam: &SqliteSeam,
    _enum_types: &[EnumTypePlan],
    ref_columns: &EnumRefColumns,
    arrivals: std::borrow::Cow<'a, [Arrival]>,
) -> BoundaryResult<std::borrow::Cow<'a, [Arrival]>> {
    for arrival in arrivals.iter() {
        if let Some(refs) = ref_columns.get(&arrival.rel) {
            check_reference(&arrival.rel, refs, &arrival.row);
        }
    }
    Ok(arrivals)
}

pub fn decode_deltas(
    _seam: &SqliteSeam,
    _enum_types: &[EnumTypePlan],
    _ref_columns: &EnumRefColumns,
    _relations: &[IncrementalRelationPlan],
    deltas: Vec<RelDelta>,
) -> BoundaryResult<Vec<RelDelta>> {
    Ok(deltas)
}

pub fn decode_row(
    _seam: &SqliteSeam,
    _enum_types: &[EnumTypePlan],
    _ref_columns: &EnumRefColumns,
    _relations: &[IncrementalRelationPlan],
    _rel: &str,
    row: &Row,
) -> BoundaryResult<Row> {
    Ok(row.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ArrivalSign, EnumRefColumn, Value};
    use std::collections::HashMap;

    fn refs() -> EnumRefColumns {
        HashMap::from([(
            "resident".into(),
            vec![
                None,
                Some(EnumRefColumn {
                    name: "choice".into(),
                    endpoint_index: Some(0),
                }),
            ],
        )])
    }

    fn resident(value: Value) -> Arrival {
        Arrival {
            rel: "resident".into(),
            sign: ArrivalSign::Add,
            row: vec![Value::Integer(7), value],
        }
    }

    #[test]
    fn a_reference_column_carries_its_endpoint_id_unchanged() {
        let seam = SqliteSeam::in_memory().unwrap();
        let rows = intern(
            &seam,
            &[],
            &refs(),
            std::borrow::Cow::Owned(vec![resident(Value::Integer(401))]),
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].row, vec![Value::Integer(7), Value::Integer(401)]);
    }

    #[test]
    fn a_tagged_value_in_a_reference_column_is_named() {
        let seam = SqliteSeam::in_memory().unwrap();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| intern(
                &seam,
                &[],
                &refs(),
                std::borrow::Cow::Owned(vec![resident(Value::Text(
                    r#"{"tag":"ok","value":"yes"}"#.into()
                ))])
            )))
            .is_err()
        );
    }

    #[test]
    fn the_boundary_reads_the_reference_back_as_the_same_id() {
        let seam = SqliteSeam::in_memory().unwrap();
        let row = decode_row(
            &seam,
            &[],
            &refs(),
            &[],
            "resident",
            &vec![Value::Integer(7), Value::Integer(401)],
        )
        .unwrap();
        assert_eq!(row, vec![Value::Integer(7), Value::Integer(401)]);
    }
}
