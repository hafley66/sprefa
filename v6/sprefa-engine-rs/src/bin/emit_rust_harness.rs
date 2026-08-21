// @comment-ok: the binary's usage contract, the one doc site for its flags.
// The Rust-arm harness for the emit_rust door. Reads an emitted module (a Rust
// source file whose PROGRAM_JSON raw string carries the ProgramJson document)
// and a schedule, then opens the SQLite seam, runs DDL + boot, and folds the
// schedule (one arrival batch per tick, drain ticks while carry_pending).
// stdout carries the tick log and nothing else: that is what gets byte-diffed
// against the oracle jsonl.
//
// Usage: emit_rust_harness <program.rs> <schedule.json> [--live-hosts] [--socket <path>]
// --live-hosts runs `sh` decls live; a scripted __host_response_* row is then a defect.
// --socket keeps the folded program resident behind its rels on a socket file.

use std::env;

use sprefa_engine_rs::driver::{run_schedule, run_schedule_live};
use sprefa_engine_rs::program::GenProgram;
use sprefa_engine_rs::serve::{arrival_batch, ArrivalDto, ServeState};
use sprefa_engine_rs::sql::SqliteSeam;
use sprefa_engine_rs::types::{Arrival, ProgramJson};

// Extract the JSON body from the emitted module's raw string literal.
fn extract_json(module_text: &str) -> String {
    let start = module_text.find("r#\"").expect("no r#\" delimiter") + 3;
    let end = module_text[start..]
        .find("\"#;")
        .expect("no \"#; delimiter")
        + start;
    module_text[start..end].to_string()
}

fn socket_argument(args: &mut Vec<String>) -> Option<String> {
    let flag = args.iter().position(|arg| arg == "--socket")?;
    if flag + 1 >= args.len() {
        eprintln!("--socket wants a path"); // @eprintln-ok CLI usage
        std::process::exit(2);
    }
    let path = args[flag + 1].clone();
    args.drain(flag..flag + 2);
    Some(path)
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
    let live_hosts = args.iter().any(|arg| arg == "--live-hosts");
    args.retain(|arg| arg != "--live-hosts");
    let socket = socket_argument(&mut args);
    if args.len() != 3 {
        eprintln!(
            "usage: emit_rust_harness <program.rs> <schedule.json> [--live-hosts] [--socket <path>]"
        ); // @eprintln-ok CLI usage
        std::process::exit(2);
    }
    let module_text = std::fs::read_to_string(&args[1]).expect("read program.rs");
    let program_json: ProgramJson =
        serde_json::from_str(&extract_json(&module_text)).expect("parse program json");
    let gen_program = GenProgram::from_json(program_json);

    let schedule_text = std::fs::read_to_string(&args[2]).expect("read schedule");
    let schedule_json: Vec<Vec<ArrivalDto>> =
        serde_json::from_str(&schedule_text).expect("parse schedule");
    let schedule: Vec<Vec<Arrival>> = schedule_json
        .into_iter()
        .map(|batch| arrival_batch(batch).unwrap_or_else(|failure| panic!("{failure}")))
        .collect();

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
    for line in fold.lines {
        println!("{}", line);
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
