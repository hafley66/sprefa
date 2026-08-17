use std::os::unix::fs::PermissionsExt;

use sprefa_engine_rs::hosts::HostLiveRunner;
use sprefa_engine_rs::program::run_boot;
use sprefa_engine_rs::sql::SqliteSeam;
use sprefa_engine_rs::types::{Arrival, ArrivalSign, TickDeltas, Value};
use sprefa_engine_rs::GenProgram;
use tempfile::TempDir;

#[path = "fixtures/boop-concatmap.program.rs"]
mod boop_concatmap_program;

fn add(rel: &str, row: Vec<Value>) -> Arrival {
    Arrival {
        rel: rel.to_string(),
        sign: ArrivalSign::Add,
        row,
    }
}

fn text(value: &str) -> Value {
    Value::Text(value.to_string())
}

fn adds<'a>(deltas: &'a TickDeltas, rel: &str) -> &'a [Vec<Value>] {
    deltas
        .rels
        .iter()
        .find(|delta| delta.rel == rel)
        .map(|delta| delta.add.as_slice())
        .unwrap_or(&[])
}

fn fake_boop() -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().expect("fake Boop directory");
    let path = dir.path().join("boop");
    std::fs::write(
        &path,
        r#"#!/bin/sh
exec python3 -c 'import json,sys
r=json.load(sys.stdin)
out="stable rewrite" if r["prompt"] != "stable rewrite" else r["prompt"]
json.dump({"request_id":r["request_id"],"outcome":"ok","output":out,"detail":""},sys.stdout,separators=(",",":"))'
"#,
    )
    .expect("write fake Boop");
    let mut permissions = std::fs::metadata(&path)
        .expect("fake metadata")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).expect("make fake executable");
    (dir, path)
}

#[test]
fn compiled_dl6_pairs_turns_calls_boop_and_reaches_a_stable_receipt() {
    let (_fake_dir, fake_path) = fake_boop();
    std::env::set_var("SPREFA_BOOP_HOST", &fake_path);

    let program = GenProgram::from_json(boop_concatmap_program::program());
    let seam = SqliteSeam::in_memory().expect("program seam");
    seam.run_ddl(&program.ddl).expect("program DDL");
    run_boot(&seam, &program.boot);
    let mut runner =
        HostLiveRunner::new(&program.host_plans, &program.rel_columns).expect("generated hosts");

    let first_demand = program
        .run_tick(
            &seam,
            &[
                add(
                    "map_config",
                    vec![
                        text("job"),
                        text("fixture/model"),
                        text("P:"),
                        Value::Integer(2),
                    ],
                ),
                add(
                    "turn",
                    vec![
                        text("s"),
                        Value::Integer(1),
                        Value::Integer(10),
                        text("assistant"),
                        text("old"),
                    ],
                ),
                add(
                    "turn",
                    vec![
                        text("s"),
                        Value::Integer(2),
                        Value::Integer(20),
                        text("assistant"),
                        text("latest"),
                    ],
                ),
                add(
                    "turn",
                    vec![
                        text("s"),
                        Value::Integer(3),
                        Value::Integer(30),
                        text("user"),
                        text("question"),
                    ],
                ),
            ],
        )
        .expect("pair tick");
    assert_eq!(
        adds(&first_demand, "contact_pair"),
        &[vec![
            text("s"),
            Value::Integer(3),
            text("latest"),
            text("question")
        ]]
    );
    assert_eq!(adds(&first_demand, "__host_demand_boop_oneshot").len(), 1);

    let first_response = runner.collect(&first_demand).expect("first Boop response");
    let changed = program
        .run_tick(&seam, &first_response)
        .expect("changed response tick");
    let continuation = adds(&changed, "rewrite_continuation");
    assert_eq!(continuation.len(), 1);
    assert_eq!(continuation[0][3], Value::Integer(1));
    assert_eq!(continuation[0][5], text("stable rewrite"));
    assert!(adds(&changed, "rewrite_receipt").is_empty());

    let retry = add("rewrite_retry", continuation[0].clone());
    let second_demand = program.run_tick(&seam, &[retry]).expect("retry tick");
    assert_eq!(adds(&second_demand, "__host_demand_boop_oneshot").len(), 1);
    let second_response = runner
        .collect(&second_demand)
        .expect("second Boop response");
    let stable = program
        .run_tick(&seam, &second_response)
        .expect("stable response tick");
    assert_eq!(
        adds(&stable, "rewrite_receipt"),
        &[vec![
            text("job"),
            text("s"),
            Value::Integer(3),
            text("job:s:3:1"),
            text("stable"),
            text("stable rewrite"),
            text(""),
        ]]
    );

    std::env::remove_var("SPREFA_BOOP_HOST");
}
