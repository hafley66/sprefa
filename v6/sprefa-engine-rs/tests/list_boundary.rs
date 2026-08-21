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

mod list_persistence_program {
    include!("fixtures/list_persistence.program.rs");
}

use sprefa_engine_rs::driver::drive_tick;
use sprefa_engine_rs::hosts::HostLiveRunner;
use sprefa_engine_rs::program::run_boot;
use sprefa_engine_rs::sql::SqlRunner;
use sprefa_engine_rs::types::{
    Arrival, ArrivalSign, BoundaryError, HostColumnPlan, HostPlanData, QueryResult, RelDelta,
    RowColumnType, ScalarSeam, ScalarValue, TickDeltas, Value,
};
use sprefa_engine_rs::GenProgram;
use sprefa_engine_rs::SqlStatement;

fn a_list() -> Value {
    Value::List(vec![serde_json::Value::String("usr".to_string())])
}

/// `list(T)` has a durable two-relation representation on the SQLite door:
/// the owner stores the list entity's integer id, the entity is keyed by its
/// canonical content, and members store `(list_id, idx, value)`. The member
/// unique key preserves duplicates at different indices while making a
/// repeated write at one index an idempotent replacement. The public array is
/// a TEMP view, so reopening the database recreates only that view and reads
/// the durable entity/member rows again. `result_rows` owns the boundary
/// conversion from the view's JSON text to `Value::List`; the SQL seam never
/// accepts a list as a scalar parameter.
#[test]
fn a_list_round_trip_preserves_type_order_duplicates_empty_and_restart() {
    let directory = tempfile::tempdir().expect("temporary list database directory");
    let path = directory.path().join("lists.sqlite");
    let ddl = vec![
        "CREATE TABLE __str (__id INTEGER PRIMARY KEY, content TEXT NOT NULL UNIQUE)".to_string(),
        "CREATE TABLE list_entity (__id INTEGER PRIMARY KEY, content INTEGER NOT NULL UNIQUE)".to_string(),
        "CREATE TABLE list_member (__id INTEGER PRIMARY KEY, list_id INTEGER NOT NULL, idx INTEGER NOT NULL, value INTEGER NOT NULL, UNIQUE (list_id, idx))".to_string(),
    ];
    let view = "CREATE TEMP VIEW list_values AS SELECT e.__id AS list_id, coalesce((SELECT json_group_array(ordered.value) FROM (SELECT s.content AS value FROM list_member m LEFT JOIN __str s ON s.__id = m.value WHERE m.list_id = e.__id ORDER BY m.idx) ordered), '[]') AS value_text FROM list_entity e";

    {
        let seam = sprefa_engine_rs::sql::SqliteSeam::open(path.to_str().unwrap())
            .expect("open durable list database");
        seam.run_ddl(&ddl).expect("create list storage");
        seam.execute(&SqlStatement {
            sql: "INSERT INTO __str (content) VALUES ('[\"beta\",\"alpha\",\"beta\"]'), ('beta'), ('alpha'), ('[]')".to_string(),
            args: vec![],
        }).expect("store canonical list and member values");
        seam.execute_multiple(
            "INSERT INTO list_entity (content) SELECT __id FROM __str WHERE content = '[\"beta\",\"alpha\",\"beta\"]';\nINSERT INTO list_entity (content) SELECT __id FROM __str WHERE content = '[]'",
        ).expect("store non-empty and empty list entities");
        seam.execute(&SqlStatement {
            sql: "INSERT INTO list_member (list_id, idx, value) SELECT 1, 0, __id FROM __str WHERE content = 'beta' UNION ALL SELECT 1, 1, __id FROM __str WHERE content = 'alpha' UNION ALL SELECT 1, 2, __id FROM __str WHERE content = 'beta'".to_string(),
            args: vec![],
        }).expect("store ordered duplicate members");
        seam.execute_multiple(view)
            .expect("create temporary list view");
        let rows = seam
            .execute(&SqlStatement {
                sql: "SELECT list_id, value_text FROM list_values ORDER BY list_id".to_string(),
                args: vec![],
            })
            .expect("read list view");
        assert_eq!(
            rows.rows.len(),
            2,
            "the entity outer relation gives empty lists a row"
        );
        assert_eq!(
            rows.rows[0][1],
            Value::Text("[\"beta\",\"alpha\",\"beta\"]".to_string())
        );
        assert_eq!(rows.rows[1][1], Value::Text("[]".to_string()));
    }

    let reopened = sprefa_engine_rs::sql::SqliteSeam::open(path.to_str().unwrap())
        .expect("reopen durable list database");
    reopened
        .execute_multiple(view)
        .expect("recreate temporary list view");
    let rows = reopened
        .execute(&SqlStatement {
            sql: "SELECT list_id, value_text FROM list_values ORDER BY list_id".to_string(),
            args: vec![],
        })
        .expect("read list view after restart");
    let values = sprefa_engine_rs::result_rows(
        &rows,
        &["list_id".to_string(), "value_text".to_string()],
        &[RowColumnType::Int, RowColumnType::List],
    )
    .expect("hydrate the ordered array at the Rust boundary");
    assert_eq!(values.len(), 2);
    assert_eq!(values[0][0], Value::Integer(1));
    assert_eq!(values[0][1], a_list_with(&["beta", "alpha", "beta"]));
    assert_eq!(values[1][0], Value::Integer(2));
    assert_eq!(values[1][1], Value::List(vec![]));
}

/// The Rust door consumes the same generated ProgramJson as the emitter's
/// TS door. The fixture tick creates the list entity/member rows; the delete
/// tick retracts only the owner row; restart rebuilds TEMP views and boot
/// recomputes the owner from the durable source rows.
#[tokio::test]
async fn generated_program_preserves_list_order_delete_and_restart() {
    let directory = tempfile::tempdir().expect("temporary generated list database");
    let path = directory.path().join("generated.sqlite");
    let program = GenProgram::from_json(list_persistence_program::program());

    let seam = sprefa_engine_rs::sql::SqliteSeam::open(path.to_str().unwrap())
        .expect("open generated list database");
    seam.run_ddl(&program.ddl).expect("generated ddl");
    run_boot(&seam, &program.boot);
    drive_tick(
        &program,
        &seam,
        vec![Arrival {
            rel: "row_text".to_string(),
            sign: ArrivalSign::Add,
            row: vec![
                Value::Text("ordered".to_string()),
                Value::Text("beta/alpha/beta".to_string()),
            ],
        }],
    )
    .await
    .expect("generated source add tick");
    drive_tick(
        &program,
        &seam,
        vec![Arrival {
            rel: "__gen__list_text_df210f232c1299bd".to_string(),
            sign: ArrivalSign::Add,
            row: vec![Value::Text("[]".to_string())],
        }],
    )
    .await
    .expect("generated empty-list tick");
    // The rel name a schedule names has no module prefix; the table the view
    // reads does (compile.pl relation_storage_names/4).
    let view = "__list_split_value_is_the_interned_list_id___gen__list_text_df210f232c1299bd";
    let rows = seam
        .execute(&SqlStatement {
            sql: format!("SELECT value_text FROM \"{view}\" ORDER BY list_id"),
            args: vec![],
        })
        .expect("generated list view before restart");
    assert!(rows
        .rows
        .iter()
        .any(|row| row[0] == Value::Text("[\"beta\",\"alpha\",\"beta\"]".to_string())));
    // A list id with no member rows has no view row and reads as the empty
    // list through the same coalesce the boundary render uses.
    let empty = seam
        .execute(&SqlStatement {
            sql: format!(
                "SELECT coalesce((SELECT value_text FROM \"{view}\" WHERE list_id = -1), '[]')"
            ),
            args: vec![],
        })
        .expect("generated empty list read");
    assert_eq!(empty.rows[0][0], Value::Text("[]".to_string()));
    drop(seam);

    let seam = sprefa_engine_rs::sql::SqliteSeam::open(path.to_str().unwrap())
        .expect("reopen generated list database");
    let temporary_ddl = program
        .ddl
        .iter()
        .filter(|statement| statement.starts_with("CREATE TEMP"))
        .cloned()
        .collect::<Vec<_>>();
    seam.run_ddl(&temporary_ddl)
        .expect("recreate generated temp ddl");
    run_boot(&seam, &program.boot);
    let rows = seam
        .execute(&SqlStatement {
            sql: format!("SELECT value_text FROM \"{view}\" ORDER BY list_id"),
            args: vec![],
        })
        .expect("generated list view after restart");
    assert!(rows
        .rows
        .iter()
        .any(|row| row[0] == Value::Text("[\"beta\",\"alpha\",\"beta\"]".to_string())));

    drive_tick(
        &program,
        &seam,
        vec![Arrival {
            rel: "row_text".to_string(),
            sign: ArrivalSign::Del,
            row: vec![
                Value::Text("ordered".to_string()),
                Value::Text("beta/alpha/beta".to_string()),
            ],
        }],
    )
    .await
    .expect("generated source delete tick");
    let owner = seam
        .execute(&SqlStatement {
            sql: program.final_select.get("row_parts").unwrap().clone(),
            args: vec![],
        })
        .expect("generated owner boundary after delete");
    assert!(!owner
        .rows
        .iter()
        .any(|row| row[0] == Value::Text("ordered".to_string())));
}

fn a_list_with(items: &[&str]) -> Value {
    Value::List(
        items
            .iter()
            .map(|item| serde_json::Value::String((*item).to_string()))
            .collect(),
    )
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

    let nested = QueryResult {
        rows: vec![vec![Value::Text("[[{\"name\":\"ada\"}],[]]".to_string())]],
        columns: vec!["items".to_string()],
        rows_affected: 0,
    };
    assert_eq!(
        sprefa_engine_rs::result_rows(&nested, &["items".to_string()], &[RowColumnType::List]),
        Ok(vec![vec![Value::List(vec![
            serde_json::json!([{"name": "ada"}]),
            serde_json::json!([]),
        ])]]),
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
        execution: "/extract/records".to_string(),
        request_type: None,
        response_type: None,
    }];
    let mut rel_columns = HashMap::new();
    rel_columns.insert(
        "__host_demand_look".to_string(),
        vec!["path".to_string(), "witness_digest".to_string()],
    );
    let mut runner = HostLiveRunner::new(&plans, &rel_columns).expect("linked executor is known");
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
