//! The one subscribe: reads a program, runs the pipe, prints the result.

use clap::{Parser, Subcommand};
use dl8::_5_reify::emit_sqlite;
use dl8::_6_eval::evaluate::{Evaluate, Store};
use dl8::_6_eval::json::term_to_json;
use dl8::_6_eval::json::{
    closure_to_json, diagnostic_to_json, program_from_json, program_names, program_to_json,
    serve_relations,
};
use dl8::_6_eval::{Trace, Universe};
use dl8::_7_effect::Slice;
use dl8::_8_driver::{Event, Stop};
use dl8::_9_runtime::executors::executors_for;
use dl8::_9_runtime::{IRowStore, Reconciler, SqliteRowStore, StoreError};
use hafley_observe::{Config, OutputFormat};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "dl8", about = "DL7, compiled in Rust")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Read one .dl7 file and print its forms, source rows and diagnostics as JSON.
    Read { file: PathBuf },
    /// Expand macrotime over a case (JSON: syntax graph rows + macro program).
    Expand {
        case: PathBuf,
        /// Print one line per wave to stderr.
        #[arg(long)]
        trace: bool,
    },
    /// Lower a case (JSON) to the checked-goal program.
    Lower { case: PathBuf },
    /// Check a lowered case (JSON).
    Check { case: PathBuf },
    /// Load project facts: filesystem graph, TSI stream, source facts (JSON case).
    Load { case: PathBuf },
    /// Run comptime rounds over a checked case (JSON).
    Comptime { case: PathBuf },
    /// Reify a checked program (JSON) to logical program rows.
    Reify { case: PathBuf },
    /// Compile one `.dl7` file (or a project) and print compiler rows, the
    /// runtime program and diagnostics as JSON.
    Compile {
        /// Source files; more than one, or `--project`, selects the project door.
        #[arg(required = true)]
        file: Vec<PathBuf>,
        /// Project root; `compile_dl7_project/5` instead of `compile_dl7/4`.
        #[arg(long)]
        project: Option<PathBuf>,
        /// A TSI JSONL stream to load before lowering.
        #[arg(long)]
        tsi: Vec<PathBuf>,
        /// Print every wave and round to stderr.
        #[arg(long)]
        trace: bool,
    },
    /// Lower a `dl8 compile` output to one target and print the artifact as JSON.
    Emit {
        target: EmitTarget,
        program: PathBuf,
    },
    /// Evaluate a checked-goal program (JSON) and print its closure as JSON.
    Eval {
        program: PathBuf,
        /// Relations the outside settles; a miss on one writes an `effect` row.
        #[arg(long, value_delimiter = ',')]
        serve: Vec<String>,
        /// Print one line per stratum and round to stderr.
        #[arg(long)]
        trace: bool,
        /// Persist the closure into this SQLite file, and load it back on the
        /// next run. The program's table prefix is the JSON file's stem.
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Run a program as a process: each tick evaluates, hands the `effect` rows
    /// of served relations to their executors, and inserts the answers.
    Run {
        program: PathBuf,
        /// Relations an executor settles: `timer`, `fetch_json`, `git.refs`,
        /// `git.history`, `fs.at`, `extract`.
        #[arg(long, value_delimiter = ',')]
        serve: Vec<String>,
        /// Print one line per stratum, round and tick to stderr.
        #[arg(long)]
        trace: bool,
        /// Persist every tick into this SQLite file and continue from it.
        #[arg(long)]
        db: Option<PathBuf>,
        /// Stop after this many ticks past tick 0; unset runs until settled.
        #[arg(long)]
        max_ticks: Option<usize>,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum EmitTarget {
    /// One `sqlite_ivm` view per derived relation over the `eval --db` tables.
    Sqlite,
}

fn trace_requested(command: &Command) -> bool {
    match command {
        Command::Expand { trace, .. }
        | Command::Compile { trace, .. }
        | Command::Eval { trace, .. }
        | Command::Run { trace, .. } => *trace,
        _ => false,
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let ansi = std::io::stderr().is_terminal();
    let default_filter = if trace_requested(&cli.command) {
        "dl8=debug"
    } else {
        "warn"
    };
    let config = Config::from_env("dl8", env!("CARGO_PKG_VERSION"), default_filter, ansi)
        .unwrap_or(Config {
            service_name: "dl8",
            service_version: env!("CARGO_PKG_VERSION"),
            default_filter: "warn",
            format: OutputFormat::Human,
            ansi,
        });
    let _ = hafley_observe::init(config);
    let exit = run(cli.command);
    hafley_observe::shutdown();
    hafley_observe::finish_trace();
    exit
}

fn run(command: Command) -> ExitCode {
    match command {
        Command::Read { file } => dl8::_0_read::cli(&file),
        Command::Expand { case, trace } => dl8::_1_macrotime::cli(&case, trace),
        Command::Lower { case } => dl8::_2_lower::cli(&case),
        Command::Check { case } => dl8::_3_check::cli(&case),
        Command::Load { case } => dl8::_4_comptime::load::cli(&case),
        Command::Comptime { case } => dl8::_4_comptime::cli(&case),
        Command::Reify { case } => dl8::_5_reify::cli(&case),
        Command::Compile {
            file,
            project,
            tsi,
            trace,
        } => compile_cli(&file, project.as_deref(), &tsi, trace),
        Command::Emit { target, program } => emit_cli(target, &program),
        Command::Eval {
            program,
            serve,
            trace,
            db,
        } => eval_cli(&program, &serve, trace, db.as_deref()),
        Command::Run {
            program,
            serve,
            trace,
            db,
            max_ticks,
        } => run_cli(
            &program,
            &serve,
            trace,
            db.as_deref(),
            max_ticks.unwrap_or(usize::MAX),
        ),
    }
}

/// The store's table prefix. One db holds many programs, so the prefix is the
/// program's own name and never the file the rows landed in.
fn program_name(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| "program".to_string())
}

fn attach(file: &Path, program: &Path) -> Result<SqliteRowStore, StoreError> {
    let mut store = SqliteRowStore::at(file)?;
    store.open(&program_name(program))?;
    Ok(store)
}

/// One transaction per tick: the arena and the rows never disagree after a kill.
fn persist(store: &mut SqliteRowStore, u: &Universe, rows: &Store) -> Result<usize, StoreError> {
    store.begin_tick()?;
    let from = store.watermark();
    let written = store
        .commit_arena(u, from)
        .and_then(|_| store.commit_rows(u, rows));
    match written {
        Ok(written) => {
            store.commit_tick()?;
            Ok(written)
        }
        Err(e) => {
            let _ = store.rollback_tick();
            Err(e)
        }
    }
}

/// The table prefix is `program_name`, the one `eval --db` gives the same file.
fn emit_cli(target: EmitTarget, path: &Path) -> ExitCode {
    let EmitTarget::Sqlite = target;
    let value: serde_json::Value = match std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|text| serde_json::from_str(&text).map_err(|e| e.to_string()))
    {
        Ok(value) => value,
        Err(e) => {
            tracing::error!(phase = "emit", error = %e, path = %path.display());
            return ExitCode::from(2);
        }
    };
    let mut u = Universe::new();
    let body = value.get("program").unwrap_or(&value);
    let parsed = program_from_json(&mut u, body)
        .and_then(|program| Ok((program, program_names(&mut u, body)?)));
    let (program, names) = match parsed {
        Ok(parsed) => parsed,
        Err(e) => {
            tracing::error!(phase = "emit", error = %e);
            return ExitCode::from(2);
        }
    };
    let artifact = match emit_sqlite(&mut u, &program, &names, &program_name(path)) {
        Ok(artifact) => artifact,
        Err(stop) => {
            tracing::error!(phase = "emit", error = ?stop);
            return ExitCode::from(3);
        }
    };
    let views: Vec<serde_json::Value> = artifact
        .views
        .iter()
        .map(|view| {
            serde_json::json!({
                "relation": view.relation,
                "stratum": view.stratum,
                "ddl": view.ddl,
            })
        })
        .collect();
    let diagnostics: Vec<serde_json::Value> = artifact
        .diagnostics
        .iter()
        .map(|d| diagnostic_to_json(&u, d))
        .collect();
    let out = serde_json::json!({ "views": views, "diagnostics": diagnostics });
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
    if artifact.diagnostics.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn eval_cli(path: &Path, serve: &[String], trace: bool, db: Option<&Path>) -> ExitCode {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(phase = "eval", error = %e, path = %path.display());
            return ExitCode::from(2);
        }
    };
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(phase = "eval", error = %e, path = %path.display());
            return ExitCode::from(2);
        }
    };
    let mut u = Universe::new();
    let mut rows = Store::default();
    let mut store = match db.map(|file| attach(file, path)) {
        None => None,
        Some(Ok(store)) => Some(store),
        Some(Err(e)) => {
            tracing::error!(phase = "eval", error = %e);
            return ExitCode::from(2);
        }
    };
    if let Some(store) = store.as_mut() {
        let loaded = store
            .load_arena(&mut u)
            .and_then(|_| store.load_rows(&mut u, &mut rows));
        match loaded {
            Ok(loaded) => tracing::info!(target: "dl8::store", phase = "load", rows = loaded),
            Err(e) => {
                tracing::error!(phase = "eval", error = %e);
                return ExitCode::from(2);
            }
        }
    }
    let body = value.get("program").unwrap_or(&value);
    let mut program = match program_from_json(&mut u, body) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(phase = "eval", error = %e);
            return ExitCode::from(2);
        }
    };
    let names = match program_names(&mut u, body) {
        Ok(n) => n,
        Err(e) => {
            tracing::error!(phase = "eval", error = %e);
            return ExitCode::from(2);
        }
    };
    if let Some(store) = store.as_mut() {
        store.name_relations(&names);
    }
    let unknown = serve_relations(&mut u, &mut program, &names, serve);
    if !unknown.is_empty() {
        let out = closure_to_json(&u, &[], &unknown);
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
        return ExitCode::from(1);
    }
    rows.mark_all();
    let mut fx = |t: Trace| {
        if trace {
            tracing::debug!(target: "dl8::trace", event = ?t);
        }
    };
    let closure = Evaluate::reduce(&mut rows, (&mut u, &program), &mut fx);
    if let Some(store) = store.as_mut() {
        match persist(store, &u, &rows) {
            Ok(written) => tracing::info!(target: "dl8::store", phase = "commit", rows = written),
            Err(e) => {
                tracing::error!(phase = "eval", error = %e);
                return ExitCode::from(2);
            }
        }
    }
    let out = closure_to_json(&u, &closure.rows, &closure.diagnostics);
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
    if closure.diagnostics.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn run_cli(
    path: &Path,
    serve: &[String],
    trace: bool,
    db: Option<&Path>,
    max_ticks: usize,
) -> ExitCode {
    let fail = |error: &dyn std::fmt::Display| {
        tracing::error!(phase = "run", error = %error, path = %path.display());
        ExitCode::from(2)
    };
    let value: serde_json::Value = match std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|text| serde_json::from_str(&text).map_err(|e| e.to_string()))
    {
        Ok(value) => value,
        Err(e) => return fail(&e),
    };
    let mut u = Universe::new();
    let mut rows = Store::default();
    let mut store = match db.map(|file| attach(file, path)) {
        None => None,
        Some(Ok(store)) => Some(store),
        Some(Err(e)) => return fail(&e),
    };
    if let Some(store) = store.as_mut() {
        match store
            .load_arena(&mut u)
            .and_then(|_| store.load_rows(&mut u, &mut rows))
        {
            Ok(loaded) => tracing::info!(target: "dl8::store", phase = "load", rows = loaded),
            Err(e) => return fail(&e),
        }
    }
    let body = value.get("program").unwrap_or(&value);
    let (mut program, names) = match program_from_json(&mut u, body)
        .and_then(|program| Ok((program, program_names(&mut u, body)?)))
    {
        Ok(parts) => parts,
        Err(e) => return fail(&e),
    };
    if let Some(store) = store.as_mut() {
        store.name_relations(&names);
    }
    let mut unknown = serve_relations(&mut u, &mut program, &names, serve);
    let reconciler = match unknown.is_empty() {
        true => executors_for(&mut u, &names, serve)
            .and_then(|executors| Reconciler::new(&mut u, &names, executors)),
        false => Err(Vec::new()),
    };
    let mut reconciler = match reconciler {
        Ok(reconciler) => reconciler,
        Err(diagnostics) => {
            unknown.extend(diagnostics);
            let out = closure_to_json(&u, &[], &unknown);
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
            return ExitCode::from(1);
        }
    };
    let mut fx = |t: Trace| {
        if trace {
            tracing::debug!(target: "dl8::trace", event = ?t);
        }
    };
    let durable = store.as_mut().map(|s| s as &mut dyn IRowStore);
    let reconciled = match reconciler.run(&mut u, &program, &mut rows, durable, max_ticks, &mut fx)
    {
        Ok(reconciled) => reconciled,
        Err(e) => return fail(&e),
    };
    drop(reconciler);
    let closure = reconciled.closure;
    let mut out = closure_to_json(&u, &closure.rows, &closure.diagnostics);
    out["ticks"] = serde_json::json!(reconciled.ticks);
    if let Some(store) = store.as_ref() {
        out["insert_statements"] = serde_json::json!(store.insert_statements());
    }
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
    if closure.diagnostics.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn compile_cli(
    files: &[PathBuf],
    project: Option<&std::path::Path>,
    streams: &[PathBuf],
    trace: bool,
) -> ExitCode {
    let mut u = Universe::new();
    let mut fx = |e: Event| {
        if trace {
            tracing::debug!(target: "dl8::trace", event = ?e);
        }
    };
    let paths: Vec<&std::path::Path> = files.iter().map(|p| p.as_path()).collect();
    let streams: Vec<&std::path::Path> = streams.iter().map(|p| p.as_path()).collect();
    let compiled = match (project, paths.as_slice()) {
        (None, [one]) if streams.is_empty() => dl8::compile(&mut u, one, &mut fx),
        (root, _) => {
            let root = root.map(|r| r.to_path_buf()).unwrap_or_else(|| {
                paths[0]
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."))
            });
            dl8::compile_project(&mut u, &root, &paths, &streams, &mut fx)
        }
    };
    let compiled = match compiled {
        Ok(compiled) => compiled,
        Err(Stop::Io(message)) => {
            tracing::error!(phase = "compile", error = %message);
            return ExitCode::from(2);
        }
        Err(other) => {
            tracing::error!(phase = "compile", error = ?other);
            return ExitCode::from(3);
        }
    };
    let mut diagnostics = compiled.diagnostics.clone();
    let empty = u
        .as_list(compiled.runtime_program)
        .is_some_and(|l| l.is_empty());
    let program = match empty {
        true => serde_json::Value::Null,
        false => match program_to_json(&mut u, compiled.runtime_program) {
            Ok(program) => program,
            Err(payload) => {
                diagnostics.push(payload);
                serde_json::Value::Null
            }
        },
    };
    let encode = |ids: &[dl8::_6_eval::TermId]| {
        serde_json::Value::Array(ids.iter().map(|t| term_to_json(&u, *t)).collect())
    };
    let mut out = serde_json::Map::new();
    out.insert("compiler_rows".into(), encode(&compiled.compiler_rows));
    out.insert(
        "runtime_program".into(),
        term_to_json(&u, compiled.runtime_program),
    );
    out.insert("diagnostics".into(), encode(&diagnostics));
    out.insert("program".into(), program);
    println!(
        "{}",
        serde_json::to_string(&serde_json::Value::Object(out)).unwrap()
    );
    if diagnostics.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
