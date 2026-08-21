use std::collections::HashMap;

use sprefa_engine_rs::driver::run_schedule;
use sprefa_engine_rs::hosts::HostLiveRunner;
use sprefa_engine_rs::{
    tick_line, Arrival, ArrivalSign, GenProgram, HostColumnPlan, HostPlanData, RelDelta,
    RowColumnType, ScalarValue, SqlRunner, SqlStatement, SqliteSeam, TickDeltas, Value,
};

#[path = "fixtures/bytes_type_system.program.rs"]
mod bytes_type_system_program;

#[test]
fn sqlite_blob_and_tick_boundary_preserve_native_bytes() {
    let seam = SqliteSeam::in_memory().expect("sqlite");
    seam.run_ddl(&[
        "CREATE TABLE payload (value BLOB NOT NULL CHECK(typeof(value) = 'blob'))".to_string(),
    ])
    .expect("ddl");
    let bytes = vec![0x00, 0x7f, 0x80, 0xff];
    seam.execute(&SqlStatement {
        sql: "INSERT INTO payload(value) VALUES (?)".to_string(),
        args: vec![ScalarValue::Bytes(bytes.clone())],
    })
    .expect("blob bind");
    let result = seam
        .execute(&SqlStatement {
            sql: "SELECT typeof(value) AS kind, value FROM payload".to_string(),
            args: vec![],
        })
        .expect("blob read");
    assert_eq!(result.rows[0][0], Value::Text("blob".to_string()));
    assert_eq!(result.rows[0][1], Value::Bytes(bytes.clone()));

    let deltas = TickDeltas {
        rels: vec![sprefa_engine_rs::RelDelta {
            rel: "payload".to_string(),
            add: vec![vec![Value::Bytes(bytes)]],
            del: vec![],
        }],
        carry_pending: false,
    };
    let mut types = HashMap::new();
    types.insert("payload".to_string(), vec![RowColumnType::Bytes]);
    assert_eq!(
        tick_line(1, &deltas, &types, &HashMap::new()),
        "{\"tick\":1,\"deltas\":{\"payload\":{\"add\":[[{\"$bytes\":\"AH+A/w==\"}]],\"del\":[]}}}"
    );
    let tagged =
        serde_json::to_string(&Value::Bytes(vec![0x00, 0x7f, 0x80, 0xff])).expect("tagged bytes");
    assert_eq!(tagged, r#"{"$bytes":"AH+A/w=="}"#);
    assert_eq!(
        serde_json::from_str::<Value>(&tagged).expect("decode tagged bytes"),
        Value::Bytes(vec![0x00, 0x7f, 0x80, 0xff])
    );
    assert_eq!(
        serde_json::to_string(&Value::Bytes(vec![])).expect("empty bytes"),
        r#"{"$bytes":""}"#
    );
    assert!(serde_json::from_str::<Value>(r#"{"$bytes":"AB=="}"#).is_err());
    assert!(serde_json::from_str::<Value>(r#"{"$bytes":"AAB="}"#).is_err());
}

#[tokio::test]
async fn authored_dl6_program_runs_native_bytes_through_rust_runtime() {
    let program = GenProgram::from_json(bytes_type_system_program::program());
    let seam = SqliteSeam::in_memory().expect("sqlite");
    let same = vec![0x00, 0x7f, 0x80, 0xff];
    let changed = vec![0x00, 0x7f, 0x81, 0xff];
    let schedule = vec![
        vec![
            Arrival {
                rel: "byte_source".to_string(),
                sign: ArrivalSign::Add,
                row: vec![Value::Bytes(vec![])],
            },
            Arrival {
                rel: "byte_source".to_string(),
                sign: ArrivalSign::Add,
                row: vec![Value::Bytes(same.clone())],
            },
            Arrival {
                rel: "byte_source".to_string(),
                sign: ArrivalSign::Add,
                row: vec![Value::Bytes(same.clone())],
            },
            Arrival {
                rel: "byte_source".to_string(),
                sign: ArrivalSign::Add,
                row: vec![Value::Bytes(changed.clone())],
            },
        ],
        vec![Arrival {
            rel: "byte_source".to_string(),
            sign: ArrivalSign::Del,
            row: vec![Value::Bytes(same.clone())],
        }],
    ];
    let fold = run_schedule(&program, &seam, &schedule, 100)
        .await
        .expect("authored bytes fold");
    assert_eq!(fold.lines.len(), 2);
    assert!(fold.lines[0].contains("AH+A/w=="));
    assert!(fold.lines[0].contains("AH+B/w=="));
    assert!(fold.lines[1].contains("AH+A/w=="));
    assert!(!fold.lines[1].contains("AH+B/w=="));
    let final_rows = seam
        .execute(&SqlStatement {
            sql: program.final_select["byte_source"].clone(),
            args: vec![],
        })
        .expect("final byte source");
    assert_eq!(
        final_rows.rows,
        vec![vec![Value::Bytes(vec![])], vec![Value::Bytes(changed)]]
    );
    let blob_type = seam
        .execute(&SqlStatement {
            // The rel is authored `byte_source`; the table carries this
            // compilation unit's module prefix and the storage-name digest
            // (PR #364).
            sql: "SELECT typeof(value) FROM bytes_type_system_byte_source_0f21d1268d90 ORDER BY value"
                .to_string(),
            args: vec![],
        })
        .expect("blob type");
    assert!(blob_type
        .rows
        .iter()
        .all(|row| row[0] == Value::Text("blob".to_string())));
}

#[test]
fn byte_host_transport_refuses_before_executor_process() {
    let marker = std::env::temp_dir().join(format!("sprefa-bytes-host-{}", std::process::id()));
    let _ = std::fs::remove_file(&marker);
    let plan = HostPlanData {
        name: "bytes_host".to_string(),
        inputs: vec![HostColumnPlan {
            name: "payload".to_string(),
            column_type: "bytes".to_string(),
        }],
        outputs: vec![],
        template: format!("touch {} # {{payload}}", marker.display()),
        demand_rel: "__host_demand_bytes".to_string(),
        response_rel: "__host_response_bytes".to_string(),
        execution: "/extract/records".to_string(),
        request_type: None,
        response_type: None,
    };
    let rel_columns = HashMap::from([(
        "__host_demand_bytes".to_string(),
        vec!["payload".to_string(), "witness_digest".to_string()],
    )]);
    let plans = [plan];
    let mut runner = HostLiveRunner::new(&plans, &rel_columns).expect("linked plan");
    let failure = runner
        .collect(&TickDeltas {
            rels: vec![RelDelta {
                rel: "__host_demand_bytes".to_string(),
                add: vec![vec![Value::Bytes(vec![0xff]), Value::Text("w".to_string())]],
                del: vec![],
            }],
            carry_pending: false,
        })
        .expect_err("byte host transport must stop before execution");
    assert_eq!(failure.host, "bytes_host");
    assert_eq!(failure.message, "bytes_host_transport_unsupported");
    assert!(
        !marker.exists(),
        "no executor may receive byte input"
    );
}
