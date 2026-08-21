// @comment-ok: the binary's usage contract, the one doc site for its flags.
// The Rust-arm harness for the emit_rust door. Reads an emitted module (a Rust
// source file whose PROGRAM_JSON raw string carries the ProgramJson document)
// @comment-ok: the fold contract, continued.
// and a schedule, then folds it through `run::run_once`, the one implementation
// `dl6 run` and every built binary share. stdout carries the tick log and
// nothing else: that is what gets byte-diffed against the oracle jsonl.
// @comment-ok: the flag contract, continued; this file is its one doc site.
// Usage: emit_rust_harness <program.rs> [<schedule.json>] [--live-hosts]
//   [--arrive <rel>=<value>[,<value>...]]... [--final] [--final-only]
//   [--final-tsv] [--final-rels <rel>[,<rel>...]] [--socket <path>]
//   [--db <file>] [--fail-on <query>]
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
// --db folds into a plain SQLite file a cold `sqlite3` reads afterwards.
// --fail-on names a `?` query whose non-empty answer makes the process exit 1.

use std::env;
use std::path::PathBuf;
use std::str::FromStr;

use sprefa_engine_rs::run::{
    self, FinalRequest, RunOptions, SeedSpec, DRAIN_CAP,
};
use sprefa_engine_rs::serve::{arrival_batch, ArrivalDto, ServeState};
use sprefa_engine_rs::types::Arrival;

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

// Every --arrive occurrence, left to right.
fn arrive_arguments(args: &mut Vec<String>) -> Vec<SeedSpec> {
    let mut seeds = Vec::new();
    while let Some(spec) = flag_value(args, "--arrive") {
        seeds.push(stop_on(SeedSpec::from_str(&spec)));
    }
    seeds
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

// The harness answers a contract violation with exit 2, the same code the hand
// parsing above uses, so a script reads one number for "you asked wrong".
fn stop_on<T>(answer: anyhow::Result<T>) -> T {
    match answer {
        Ok(value) => value,
        Err(failure) => {
            eprintln!("{failure:#}"); // @eprintln-ok CLI contract violation
            std::process::exit(2);
        }
    }
}

fn read_schedule(path: &str) -> Vec<Vec<Arrival>> {
    let text = std::fs::read_to_string(path).expect("read schedule");
    let batches: Vec<Vec<ArrivalDto>> = serde_json::from_str(&text).expect("parse schedule");
    batches
        .into_iter()
        .map(|batch| arrival_batch(batch).unwrap_or_else(|failure| panic!("{failure}")))
        .collect()
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
    let db = flag_value(&mut args, "--db").map(PathBuf::from);
    let fail_on = flag_value(&mut args, "--fail-on");
    let seeds = arrive_arguments(&mut args);
    let finals = final_request(&mut args);
    let usage = "usage: emit_rust_harness <program.rs> [<schedule.json>] [--live-hosts] \
[--arrive <rel>=<value>[,<value>...]] [--final] [--final-only] [--final-tsv] \
[--final-rels <rel>[,<rel>...]] [--socket <path>] [--db <file>] [--fail-on <query>]";
    if args.len() < 2 || args.len() > 3 || (args.len() == 2 && seeds.is_empty()) {
        eprintln!("{usage}"); // @eprintln-ok CLI usage
        std::process::exit(2);
    }
    let loaded = stop_on(run::load_program(std::path::Path::new(&args[1])));
    let program = loaded.program;
    let schedule = match args.get(2) {
        Some(path) => read_schedule(path),
        None => Vec::new(),
    };

    // A CLI contract violation exits 2 here, the code the hand parsing above
    // uses; a fold that stops mid-tick is still a panic.
    if live_hosts {
        stop_on(run::reject_scripted_responses(&schedule));
    }
    let options = RunOptions {
        schedule,
        live_hosts,
        finals: finals.clone(),
        db,
        fail_on,
        drain_cap: DRAIN_CAP,
    };
    let seeded = stop_on(run::seed_arrivals(&program, &seeds));
    let outcome = match run::run_once(&program, seeded, options) {
        Ok(outcome) => outcome,
        Err(failure) => panic!("{failure:#}"),
    };
    stop_on(run::print_outcome(&program, &outcome, &finals));
    let failed = outcome.failed();

    // No --socket is the one-shot run: the fold is the whole program. With one,
    // the same folded seam stays resident behind its rels until SIGINT.
    if let Some(path) = socket {
        let runtime = stop_on(run::current_thread_runtime());
        let cancel = tokio_util::sync::CancellationToken::new();
        let shutdown = cancel.clone();
        let state = ServeState::resume(program, outcome.seam, outcome.ticks);
        let served = runtime.block_on(async move {
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
    if failed {
        std::process::exit(1);
    }
}
