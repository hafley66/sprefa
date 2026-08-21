// @comment-ok: the binary's usage contract, the one doc site for its flags.
// The Rust-arm harness for the emit_rust door. Reads an emitted module (a Rust
// source file whose PROGRAM_JSON raw string carries the ProgramJson document)
// and a schedule, then opens the SQLite seam, runs DDL + boot, and folds the
// schedule (one arrival batch per tick, drain ticks while carry_pending).
// stdout carries the tick log and nothing else: that is what gets byte-diffed
// against the oracle jsonl.
// @comment-ok: the flag contract, continued; this file is its one doc site.
// Usage: emit_rust_harness <program.rs> [<schedule.json>] [--live-hosts]
//   [--arrive <rel>=<value>[,<value>...]]... [--final] [--final-only]
//   [--final-tsv] [--final-rels <rel>[,<rel>...]] [--socket <path>]
// --arrive seeds the first tick from the command line; repeat it for more rows.
//   The schedule file becomes optional once one --arrive is given, and with both
//   the --arrive rows join the schedule's first batch.
// --final reads each rel through the IR's own final_select after the fold. A
//   rel whose `?` carries an `order by` tail prints in the cursor's own order.
//   --final-only drops the tick lines, --final-tsv prints rel<TAB>col... rows so
//   a shell reads columns with `read` and never parses JSON. --final-rels names
//   and orders the rels; without it every rel in final_select prints, sorted.
// --live-hosts runs `sh` decls live; a scripted __host_response_* row is then a defect.
// --socket keeps the folded program resident behind its rels on a socket file.

use std::env;

use sprefa_engine_rs::driver::{run_schedule, run_schedule_live};
use sprefa_engine_rs::program::GenProgram;
use sprefa_engine_rs::serve::{arrival_batch, ArrivalDto, ServeState};
use sprefa_engine_rs::sql::{SqlRunner, SqliteSeam};
use sprefa_engine_rs::types::{
    Arrival, ArrivalSign, ProgramJson, RowColumnType, SqlStatement, Value,
};

// Extract the JSON body from the emitted module's raw string literal.
fn extract_json(module_text: &str) -> String {
    let start = module_text.find("r#\"").expect("no r#\" delimiter") + 3;
    let end = module_text[start..]
        .find("\"#;")
        .expect("no \"#; delimiter")
        + start;
    module_text[start..end].to_string()
}

fn flag_value(args: &mut Vec<String>, flag: &str) -> Option<String> {
    let at = args.iter().position(|arg| arg == flag)?;
    if at + 1 >= args.len() {
        eprintln!("{flag} wants a value"); // @eprintln-ok CLI usage
        std::process::exit(2);
    }
    let value = args[at + 1].clone();
    args.drain(at..at + 2);
    Some(value)
}

fn take_switch(args: &mut Vec<String>, flag: &str) -> bool {
    let present = args.iter().any(|arg| arg == flag);
    args.retain(|arg| arg != flag);
    present
}

// Every --arrive occurrence, left to right, as (rel, cell texts).
fn arrive_arguments(args: &mut Vec<String>) -> Vec<(String, Vec<String>)> {
    let mut seeds = Vec::new();
    while let Some(spec) = flag_value(args, "--arrive") {
        let (rel, cells) = match spec.split_once('=') {
            Some(split) => split,
            None => {
                eprintln!("--arrive wants <rel>=<value>[,<value>...], got `{spec}`"); // @eprintln-ok CLI usage
                std::process::exit(2);
            }
        };
        seeds.push((
            rel.to_string(),
            cells.split(',').map(str::to_string).collect(),
        ));
    }
    seeds
}

// A CLI cell carries no type of its own, so the program's declared column type
// decides; an int column that cannot read its cell is a stop, never a 0.
fn arrival_row(program: &GenProgram, rel: &str, cells: &[String]) -> Vec<Value> {
    let types = program.rel_column_types.get(rel);
    if types.is_none() {
        eprintln!("--arrive names {rel}, which the program does not declare"); // @eprintln-ok CLI usage
        std::process::exit(2);
    }
    let types = types.expect("declared column types");
    if types.len() != cells.len() {
        eprintln!(
            "--arrive {rel} carries {} values and the rel declares {} columns",
            cells.len(),
            types.len()
        ); // @eprintln-ok CLI usage
        std::process::exit(2);
    }
    cells
        .iter()
        .zip(types.iter())
        .map(|(cell, column_type)| match column_type {
            RowColumnType::Int | RowColumnType::Ref | RowColumnType::RelationId => {
                match cell.parse::<i64>() {
                    Ok(number) => Value::Integer(number),
                    Err(_) => {
                        eprintln!("--arrive {rel} wants an integer, got `{cell}`"); // @eprintln-ok CLI usage
                        std::process::exit(2);
                    }
                }
            }
            RowColumnType::Float => match cell.parse::<f64>() {
                Ok(number) => Value::Real(number),
                Err(_) => {
                    eprintln!("--arrive {rel} wants a float, got `{cell}`"); // @eprintln-ok CLI usage
                    std::process::exit(2);
                }
            },
            RowColumnType::Bool => Value::Bool(cell == "true"),
            _ => Value::Text(cell.clone()),
        })
        .collect()
}

struct FinalRequest {
    wanted: bool,
    only: bool,
    tsv: bool,
    rels: Option<Vec<String>>,
}

fn final_request(args: &mut Vec<String>) -> FinalRequest {
    let only = take_switch(args, "--final-only");
    let tsv = take_switch(args, "--final-tsv");
    let rels = flag_value(args, "--final-rels")
        .map(|list| list.split(',').map(str::to_string).collect::<Vec<_>>());
    let asked = take_switch(args, "--final");
    FinalRequest {
        wanted: asked || only || tsv || rels.is_some(),
        only,
        tsv,
        rels,
    }
}

// A `bytes` column has no text transport on either seam, so the final reader
// stops on one rather than invent an encoding the tick log does not use.
fn cell_text(value: &Value) -> String {
    match value {
        Value::Integer(number) => number.to_string(),
        Value::Real(number) => sprefa_engine_rs::ticklog::js_float_text(*number),
        Value::Bool(flag) => flag.to_string(),
        Value::Text(text) => text.clone(),
        Value::List(items) => serde_json::Value::Array(items.clone()).to_string(),
        Value::Bytes(_) => {
            eprintln!("--final cannot render a bytes column"); // @eprintln-ok CLI contract violation
            std::process::exit(2);
        }
    }
}

fn cell_json(value: &Value, column_type: Option<RowColumnType>) -> serde_json::Value {
    match value {
        Value::Integer(number) => serde_json::json!(number),
        Value::Real(number) => serde_json::json!(number),
        Value::Bool(flag) => serde_json::json!(flag),
        Value::List(items) => serde_json::Value::Array(items.clone()),
        Value::Bytes(_) => serde_json::json!(cell_text(value)),
        Value::Text(text) => {
            if column_type == Some(RowColumnType::Json) || column_type == Some(RowColumnType::Ref) {
                serde_json::from_str(text).unwrap_or_else(|_| serde_json::json!(text))
            } else {
                serde_json::json!(text)
            }
        }
    }
}

// A tab or a newline inside a value would forge a column, so the TSV writer
// stops on one rather than hand a shell a row it would mis-split.
fn tsv_cell(rel: &str, value: &Value) -> String {
    let text = cell_text(value);
    if text.contains('\t') || text.contains('\n') {
        eprintln!("--final-tsv cannot carry the tab or newline in a {rel} value"); // @eprintln-ok CLI contract violation
        std::process::exit(2);
    }
    text
}

fn print_final(program: &GenProgram, seam: &SqliteSeam, request: &FinalRequest) {
    let rels = match &request.rels {
        Some(named) => named.clone(),
        None => {
            let mut every: Vec<String> = program.final_select.keys().cloned().collect();
            every.sort();
            every
        }
    };
    for rel in rels {
        let select = match program.final_select.get(&rel) {
            Some(sql) => sql.clone(),
            None => {
                eprintln!("no final_select for {rel}"); // @eprintln-ok CLI usage
                std::process::exit(2);
            }
        };
        let columns = program.rel_columns.get(&rel).cloned().unwrap_or_default();
        let types = program.rel_column_types.get(&rel).cloned().unwrap_or_default();
        let cursor_orders = select.contains(" ORDER BY ");
        let result = seam
            .execute(&SqlStatement {
                sql: select,
                args: vec![],
            })
            .unwrap_or_else(|failure| panic!("final read of {rel}: {failure}"));
        // A `?` order tail rides final_select's own ORDER BY, so those rows are
        // already in the order asked for; the rest sort to stay reproducible.
        let mut rows = result.rows.clone();
        if !cursor_orders {
            rows.sort_by_key(|row| row.iter().map(cell_text).collect::<Vec<_>>());
        }
        if request.tsv {
            for row in &rows {
                let cells: Vec<String> = row.iter().map(|value| tsv_cell(&rel, value)).collect();
                println!("{}\t{}", rel, cells.join("\t"));
            }
        } else {
            let bodies: Vec<serde_json::Value> = rows
                .iter()
                .map(|row| {
                    serde_json::Value::Array(
                        row.iter()
                            .enumerate()
                            .map(|(index, value)| cell_json(value, types.get(index).copied()))
                            .collect(),
                    )
                })
                .collect();
            println!(
                "{}",
                serde_json::json!({ "rel": rel, "columns": columns, "rows": bodies })
            );
        }
    }
}

fn main() {
    // stdout is the tick log and is byte-diffed against the oracle, so every
    // span goes to stderr. Silent unless RUST_LOG asks:
    //   RUST_LOG=sprefa_engine_rs=info emit_rust_harness ...
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("off")),
        )
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .init();
    let mut args: Vec<String> = env::args().collect();
    let live_hosts = take_switch(&mut args, "--live-hosts");
    let socket = flag_value(&mut args, "--socket");
    let seeds = arrive_arguments(&mut args);
    let final_wanted = final_request(&mut args);
    let usage = "usage: emit_rust_harness <program.rs> [<schedule.json>] [--live-hosts] \
[--arrive <rel>=<value>[,<value>...]] [--final] [--final-only] [--final-tsv] \
[--final-rels <rel>[,<rel>...]] [--socket <path>]";
    if args.len() < 2 || args.len() > 3 || (args.len() == 2 && seeds.is_empty()) {
        eprintln!("{usage}"); // @eprintln-ok CLI usage
        std::process::exit(2);
    }
    let module_text = std::fs::read_to_string(&args[1]).expect("read program.rs");
    let program_json: ProgramJson =
        serde_json::from_str(&extract_json(&module_text)).expect("parse program json");
    let gen_program = GenProgram::from_json(program_json);

    let mut schedule: Vec<Vec<Arrival>> = match args.get(2) {
        Some(path) => {
            let schedule_text = std::fs::read_to_string(path).expect("read schedule");
            let schedule_json: Vec<Vec<ArrivalDto>> =
                serde_json::from_str(&schedule_text).expect("parse schedule");
            schedule_json
                .into_iter()
                .map(|batch| arrival_batch(batch).unwrap_or_else(|failure| panic!("{failure}")))
                .collect()
        }
        None => Vec::new(),
    };
    if !seeds.is_empty() {
        let seeded: Vec<Arrival> = seeds
            .iter()
            .map(|(rel, cells)| Arrival {
                rel: rel.clone(),
                sign: ArrivalSign::Add,
                row: arrival_row(&gen_program, rel, cells),
            })
            .collect();
        match schedule.first_mut() {
            Some(first) => first.extend(seeded),
            None => schedule.push(seeded),
        }
    }

    if live_hosts {
        if let Some(scripted) = schedule
            .iter()
            .flatten()
            .find(|arrival| arrival.rel.starts_with("__host_response_"))
        {
            eprintln!(
                "--live-hosts forbids the scripted response row {} in the schedule; the runtime produces it",
                scripted.rel
            ); // @eprintln-ok CLI contract violation before any tick runs
            std::process::exit(2);
        }
    }

    let seam = SqliteSeam::in_memory().expect("open seam");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let fold = if live_hosts {
        rt.block_on(run_schedule_live(&gen_program, &seam, &schedule, 100))
            .unwrap_or_else(|failure| panic!("{failure}"))
    } else {
        rt.block_on(run_schedule(&gen_program, &seam, &schedule, 100))
            .unwrap_or_else(|failure| panic!("{failure}"))
    };
    let ticks_folded = fold.lines.len();
    if !final_wanted.only {
        for line in fold.lines {
            println!("{}", line);
        }
    }
    if final_wanted.wanted {
        print_final(&gen_program, &seam, &final_wanted);
    }

    // No --socket is the one-shot run: the fold is the whole program. With one,
    // the same folded seam stays resident behind its rels until SIGINT.
    if let Some(path) = socket {
        let cancel = tokio_util::sync::CancellationToken::new();
        let shutdown = cancel.clone();
        let state = ServeState::resume(gen_program, seam, ticks_folded);
        let served = rt.block_on(async move {
            tokio::spawn(async move {
                let _ = tokio::signal::ctrl_c().await;
                shutdown.cancel();
            });
            sprefa_engine_rs::serve::serve_unix(state, std::path::Path::new(&path), cancel).await
        });
        if let Err(failure) = served {
            eprintln!("{failure:#}"); // @eprintln-ok CLI exit path, no tracing subscriber here
            std::process::exit(1);
        }
    }
}
