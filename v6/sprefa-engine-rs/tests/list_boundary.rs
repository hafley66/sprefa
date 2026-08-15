// The `list` boundary type on the Rust door: what the read seam hands a
// consumer, and what an unknown type name does.
//
// FAIL-PRE-FIX: `RowColumnType::parse` answered `Text` for every name it did
// not know, so a NEW emitted program run by an OLD runtime degraded silently
// to the array TEXT instead of failing (list-ergonomics plan, fork F6). That
// function had no caller anywhere in the tree -- the plan JSON reaches the
// runtime through serde -- so it is gone and the behaviour it was supposed to
// have is serde's, pinned here.

use sprefa_engine_rs::types::{RowColumnType, Value};

#[test]
fn the_list_boundary_type_has_a_name_on_the_wire() {
    let parsed: RowColumnType = serde_json::from_str("\"list\"").expect("list is a boundary type");
    assert_eq!(parsed, RowColumnType::List);
}

#[test]
fn an_unknown_boundary_type_name_fails_rather_than_degrading_to_text() {
    let parsed = serde_json::from_str::<RowColumnType>("\"listy\"");
    assert!(
        parsed.is_err(),
        "an unknown boundary type must be named, got {parsed:?}",
    );
}

#[test]
fn a_list_value_serializes_as_a_json_array_never_as_its_text() {
    let value = Value::List(vec![
        serde_json::Value::String("usr".to_string()),
        serde_json::Value::String("local".to_string()),
    ]);
    assert_eq!(
        serde_json::to_string(&value).expect("a list value serializes"),
        "[\"usr\",\"local\"]",
    );
}
