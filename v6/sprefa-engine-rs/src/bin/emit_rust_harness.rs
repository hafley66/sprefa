// The Rust-arm harness for the emit_rust door. Includes the emitted module at
// build time, opens the SQLite seam, runs DDL + boot, then folds the schedule
// (one arrival batch per tick, drain ticks while carry_pending). stdout
// carries the tick log and nothing else: that is what gets byte-diffed against
// the oracle jsonl.
//
// Usage: emit_rust_harness <schedule.json>

use std::env;
use std::path::Path;

use sprefa_engine_rs::driver::run_schedule;
use sprefa_engine_rs::program::GenProgram;
use sprefa_engine_rs::sql::SqliteSeam;
use sprefa_engine_rs::types::{Arrival, ArrivalSign, Value};

include!(concat!(env!("CARGO_MANIFEST_DIR"), "/emitted/program.rs"));

#[derive(serde::Deserialize)]
struct ArrivalDto {
    rel: String,
    sign: String,
    row: Vec<serde_json::Value>,
}

fn value_from_json(v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Integer(i)
            } else {
                Value::Real(n.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::String(s) => Value::Text(s.clone()),
        serde_json::Value::Null => Value::Text(String::new()),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => Value::Text(v.to_string()),
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: emit_rust_harness <schedule.json>");
        std::process::exit(2);
    }
    let schedule_path = &args[1];
    let schedule_text = std::fs::read_to_string(schedule_path).expect("read schedule");
    let schedule_json: Vec<Vec<ArrivalDto>> =
        serde_json::from_str(&schedule_text).expect("parse schedule");
    let schedule: Vec<Vec<Arrival>> = schedule_json
        .iter()
        .map(|batch| {
            batch
                .iter()
                .map(|a| Arrival {
                    rel: a.rel.clone(),
                    sign: if a.sign == "add" {
                        ArrivalSign::Add
                    } else {
                        ArrivalSign::Del
                    },
                    row: a.row.iter().map(value_from_json).collect(),
                })
                .collect()
        })
        .collect();

    let _ = program();
    let json: sprefa_engine_rs::types::ProgramJson = program();
    let gen_program = GenProgram::from_json(json);
    let seam = SqliteSeam::in_memory().expect("open seam");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let fold = rt.block_on(run_schedule(&gen_program, &seam, &schedule, 100));
    for line in fold.lines {
        println!("{}", line);
    }
    let _ = Path::new(".");
}
