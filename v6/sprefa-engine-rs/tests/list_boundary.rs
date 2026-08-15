// The `list` boundary type on the Rust door: what the read seam hands a
// consumer, and what an unknown type name does.
//
// FAIL-PRE-FIX: `RowColumnType::parse` answered `Text` for every name it did
// not know, so a NEW emitted program run by an OLD runtime degraded silently
// to the array TEXT instead of failing (list-ergonomics plan, fork F6). That
// function had no caller anywhere in the tree -- the plan JSON reaches the
// runtime through serde -- so it is gone and the behaviour it was supposed to
// have is serde's, pinned here.

// FAIL-PRE-FIX: the four scalar seams and the two list-column parses answered a
// `panic!` (PR #256). The seam arms are now unrepresentable and the parses
// return BoundaryError, so every assert below reached an abort before the fix.

use std::collections::HashMap;

use sprefa_engine_rs::hosts::HostLiveRunner;
use sprefa_engine_rs::types::{
    BoundaryError, HostColumnPlan, HostPlanData, QueryResult, RelDelta, RowColumnType, ScalarSeam,
    ScalarValue, TickDeltas, Value,
};

fn a_list() -> Value {
    Value::List(vec![serde_json::Value::String("usr".to_string())])
}

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

#[test]
fn every_scalar_seam_names_itself_when_a_list_reaches_it() {
    let seams = [
        (ScalarSeam::SqlParameter, "a SQL parameter"),
        (ScalarSeam::HostTemplateArgument, "a host template argument"),
        (ScalarSeam::ArrivalPayload, "an arrival payload"),
        (ScalarSeam::TextIntern, "the text intern plane"),
    ];
    for (seam, spelling) in seams {
        let crossed = ScalarValue::at_seam(&a_list(), seam);
        assert_eq!(
            crossed,
            Err(BoundaryError::ListAtScalarSeam(seam)),
            "{spelling} must answer a typed error",
        );
        assert_eq!(
            crossed.unwrap_err().to_string(),
            format!("a list value reached {spelling}"),
        );
    }
}

#[test]
fn a_scalar_crosses_every_seam_untouched() {
    for seam in [ScalarSeam::SqlParameter, ScalarSeam::ArrivalPayload] {
        assert_eq!(
            ScalarValue::at_seam(&Value::Integer(7), seam),
            Ok(ScalarValue::Integer(7)),
        );
    }
}

#[test]
fn an_arrival_payload_encoder_reports_a_list_rather_than_aborting() {
    let encoded = sprefa_engine_rs::incremental::json_array_text(&[Value::Integer(1), a_list()]);
    assert_eq!(
        encoded,
        Err(BoundaryError::ListAtScalarSeam(ScalarSeam::ArrivalPayload)),
    );
}

#[test]
fn a_list_column_holding_non_array_text_is_a_typed_error_not_an_abort() {
    let result = QueryResult {
        rows: vec![vec![Value::Text("not-an-array".to_string())]],
        columns: vec!["items".to_string()],
        rows_affected: 0,
    };
    let failure =
        sprefa_engine_rs::result_rows(&result, &["items".to_string()], &[RowColumnType::List])
            .err()
            .expect("non-array text at a list column must be an error");
    let BoundaryError::ListColumnNotAnArray { text, .. } = &failure else {
        panic!("wrong boundary error: {failure:?}");
    };
    assert_eq!(text, "not-an-array");
    assert!(
        failure
            .to_string()
            .starts_with("list column crossed SQLite with non-array text not-an-array: "),
        "{failure}",
    );
}

#[test]
fn a_list_column_holding_array_text_still_parses_to_its_elements() {
    let result = QueryResult {
        rows: vec![vec![Value::Text("[\"usr\"]".to_string())]],
        columns: vec!["items".to_string()],
        rows_affected: 0,
    };
    assert_eq!(
        sprefa_engine_rs::result_rows(&result, &["items".to_string()], &[RowColumnType::List]),
        Ok(vec![vec![a_list()]]),
    );
}

#[test]
fn a_list_wired_into_a_host_input_is_a_named_host_error() {
    let plans = vec![HostPlanData {
        name: "look".to_string(),
        inputs: vec![HostColumnPlan {
            name: "path".to_string(),
            column_type: "text".to_string(),
        }],
        outputs: vec![HostColumnPlan {
            name: "start".to_string(),
            column_type: "int".to_string(),
        }],
        template: "printf '{\"start\":1}' # {path}".to_string(),
        demand_rel: "__host_demand_look".to_string(),
        response_rel: "__host_response_look".to_string(),
        execution: "shell".to_string(),
    }];
    let mut rel_columns = HashMap::new();
    rel_columns.insert(
        "__host_demand_look".to_string(),
        vec!["path".to_string(), "witness_digest".to_string()],
    );
    let mut runner = HostLiveRunner::new(&plans, &rel_columns).expect("shell executor is known");
    let deltas = TickDeltas {
        rels: vec![RelDelta {
            rel: "__host_demand_look".to_string(),
            add: vec![vec![a_list(), Value::Text("digest-1".to_string())]],
            del: vec![],
        }],
        carry_pending: false,
    };
    let failure = runner
        .collect(&deltas)
        .err()
        .expect("a list at a template argument must be an error");
    assert_eq!(failure.host, "look");
    assert_eq!(
        failure.message,
        "a list value reached a host template argument"
    );
}
