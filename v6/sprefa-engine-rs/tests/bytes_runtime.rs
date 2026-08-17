use std::collections::HashMap;

use sprefa_engine_rs::{
    tick_line, RowColumnType, ScalarValue, SqlRunner, SqlStatement, SqliteSeam, TickDeltas, Value,
};

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
}
