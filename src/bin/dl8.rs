//! The one subscribe: reads a program, runs the pipe, prints the result.

use clap::{Parser, Subcommand};
use dl8::_5_reify::{emit_sqlite, ir_emit::IR_TARGETS};
use dl8::_6_eval::evaluate::{Evaluate, Store};
use dl8::_6_eval::ir_bundle;
use dl8::_6_eval::ir_lower;
use dl8::_6_eval::json::{
    closure_to_json, diagnostic_to_json, program_from_json, program_names, program_to_json,
    serve_relations,
};
use dl8::_6_eval::json::{term_from_json, term_to_json};
use dl8::_6_eval::{Arg, Row, Term, TermId, Trace, Universe};
use dl8::_7_effect::Slice;
use dl8::_8_driver::{Event, Stop};
use dl8::_9_runtime::executors::{executors_for, query_executors_for};
use dl8::_9_runtime::{IRowStore, Reconciler, SqliteRowStore, StoreError};
use hafley_observe::{Config, OutputFormat};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, IsTerminal, Write};
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
    /// Compile, then write every `fs.file` row of the settled comptime fixpoint
    /// and print the paths as JSON.
    Build {
        /// Source files; more than one, or `--project`, selects the project door.
        #[arg(required = true)]
        file: Vec<PathBuf>,
        #[arg(long)]
        project: Option<PathBuf>,
        #[arg(long)]
        tsi: Vec<PathBuf>,
        /// Directory relative paths are written under.
        #[arg(long, default_value = ".")]
        out: PathBuf,
        /// Write nothing; exit 1 when a file differs from its row.
        #[arg(long)]
        check: bool,
    },
    /// Lower a `dl8 compile` output to one target and print the artifact as JSON.
    #[command(long_about = "Lower a `dl8 compile` output to one target and print the artifact")]
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
        /// `git.history`, `fs.at`, `fs.watch`, `extract`.
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
        /// Apply signed source changes as successive frontiers in one live IR engine.
        #[arg(long, hide = true)]
        frontiers: Option<PathBuf>,
    },
    /// Compile one query against a repository, settle its sources once and print
    /// the root file's derived rows.
    Query {
        file: PathBuf,
        /// Repository used as the working directory for relative source roots.
        #[arg(long)]
        repo: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = QueryFormat::Table)]
        format: QueryFormat,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum QueryFormat {
    Json,
    Table,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum EmitTarget {
    /// One `sqlite_ivm` view per derived relation over the `eval --db` tables.
    Sqlite,
    /// TypeScript module using the RxJS IVM runtime.
    IrRxjs,
    /// IVM program, first frontier and term table as JSON.
    IrJson,
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
    let exit = match dl8::_6_eval::ir_eval::configured_backend()
        .and(dl8::_1_macrotime::init_macrotime_mode().map(|_| ()))
    {
        Ok(_) => run(cli.command),
        Err(error) => {
            tracing::error!(phase = "engine", error = %error);
            ExitCode::from(2)
        }
    };
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
        Command::Build {
            file,
            project,
            tsi,
            out,
            check,
        } => build_cli(&file, project.as_deref(), &tsi, &out, check),
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
            frontiers,
        } => run_cli(
            &program,
            &serve,
            trace,
            db.as_deref(),
            max_ticks.unwrap_or(usize::MAX),
            frontiers.as_deref(),
        ),
        Command::Query { file, repo, format } => query_cli(&file, repo.as_deref(), format),
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
    if !matches!(target, EmitTarget::Sqlite) {
        let program = output_slice(&u, program, &names);
        let lowered = match ir_lower::lower(&mut u, &program) {
            Ok(lowered) => lowered,
            Err(diagnostic) => {
                tracing::error!(phase = "emit", diagnostic = %diagnostic_to_json(&u, &diagnostic));
                return ExitCode::from(3);
            }
        };
        let emitter = match target {
            EmitTarget::IrRxjs => &IR_TARGETS[0],
            EmitTarget::IrJson => &IR_TARGETS[1],
            EmitTarget::Sqlite => unreachable!(),
        };
        let artifact = ir_bundle::bundle(&u, &lowered).and_then(|bundle| (emitter.emit)(&bundle));
        match artifact {
            Ok(artifact) => {
                println!("{artifact}");
                return ExitCode::SUCCESS;
            }
            Err(error) => {
                tracing::error!(phase = "emit", target = emitter.name, %error);
                return ExitCode::from(3);
            }
        }
    }
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

/// The IR emit targets expose relations declared by source files. A rule is
/// needed when its head is one of those relations or feeds a needed rule.
fn output_slice(
    u: &Universe,
    mut program: dl8::_6_eval::Program,
    names: &HashMap<String, TermId>,
) -> dl8::_6_eval::Program {
    let std_dir = format!("{}/std/", env!("CARGO_MANIFEST_DIR"));
    let mut needed: HashSet<TermId> = names
        .values()
        .copied()
        .filter(|rel| {
            let Some(inner) = u.unary(*rel, "ref") else {
                return false;
            };
            let Some([file, _]) = u.args::<2>(inner, "owner") else {
                return false;
            };
            let Some(path) = u.unary(file, "file") else {
                return false;
            };
            match u.get(path) {
                Term::Atom(symbol) | Term::Str(symbol) => !u.sym_str(*symbol).starts_with(&std_dir),
                _ => false,
            }
        })
        .collect();
    if needed.is_empty() {
        return program;
    }
    let rule_heads: HashSet<_> = program.rules.iter().map(|rule| rule.rel).collect();
    let mut queue: Vec<_> = needed.iter().copied().collect();
    while let Some(relation) = queue.pop() {
        for rule in &program.rules {
            if rule.rel == relation {
                for goal in &rule.body {
                    if needed.insert(goal.rel) {
                        queue.push(goal.rel);
                    }
                    for argument in &goal.args {
                        if let Arg::Ground(relation) = argument {
                            if rule_heads.contains(relation) && needed.insert(*relation) {
                                queue.push(*relation);
                            }
                        }
                    }
                }
            }
        }
    }
    program.rules.retain(|rule| needed.contains(&rule.rel));
    program
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
    let mut out = closure_to_json(&u, &closure.rows, &closure.diagnostics);
    if let Some(counters) = dl8::_6_eval::ir_eval::take_counters() {
        let mut delta_rows = serde_json::Map::new();
        for (name, value) in [
            ("filter", counters.delta_rows.filter),
            ("join", counters.delta_rows.join),
            ("antijoin", counters.delta_rows.antijoin),
            ("reduce", counters.delta_rows.reduce),
            ("topk", counters.delta_rows.topk),
            ("window", counters.delta_rows.window),
            ("mint", counters.delta_rows.mint),
        ] {
            if let Some(value) = value {
                delta_rows.insert(name.into(), value.into());
            }
        }
        let mut values = serde_json::Map::from_iter([
            ("rows_written".into(), counters.rows_written.into()),
            ("delta_rows".into(), delta_rows.into()),
        ]);
        for (name, value) in [
            ("rounds", counters.rounds),
            ("interned", counters.interned),
            ("statements", counters.statements),
        ] {
            if let Some(value) = value {
                values.insert(name.into(), value.into());
            }
        }
        out["counters"] = values.into();
    }
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
    frontiers_path: Option<&Path>,
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
    if let Some(frontiers_path) = frontiers_path.filter(|path| *path != Path::new("-")) {
        let backend = match dl8::_6_eval::ir_eval::configured_backend() {
            Ok(backend) => backend,
            Err(e) => return fail(&e),
        };
        let frontier_value: serde_json::Value = match std::fs::read_to_string(frontiers_path)
            .map_err(|e| e.to_string())
            .and_then(|text| serde_json::from_str(&text).map_err(|e| e.to_string()))
        {
            Ok(value) => value,
            Err(e) => return fail(&e),
        };
        let Some(frontier_values) = frontier_value.as_array() else {
            return fail(&"frontiers JSON must be an array");
        };
        let mut frontiers = Vec::with_capacity(frontier_values.len());
        for frontier in frontier_values {
            let Some(changes) = frontier.get("changes").and_then(|value| value.as_array()) else {
                return fail(&"each frontier must have a changes array");
            };
            let mut parsed = Vec::with_capacity(changes.len());
            for change in changes {
                // A program name, or a relation term as seeds spell it (kernel
                // relations such as `edge_snapshot` have no program name).
                let rel = match change.get("rel") {
                    Some(serde_json::Value::String(relation_name)) => {
                        match names.get(relation_name.as_str()) {
                            Some(&rel) => rel,
                            None => {
                                return fail(&format!("unknown frontier relation {relation_name}"))
                            }
                        }
                    }
                    Some(term @ serde_json::Value::Object(_)) => match term_from_json(&mut u, term)
                    {
                        Ok(rel) => rel,
                        Err(e) => return fail(&e),
                    },
                    _ => return fail(&"frontier change rel must be a name or a relation term"),
                };
                let Some(args) = change.get("args").and_then(|value| value.as_array()) else {
                    return fail(&"frontier change args must be an array");
                };
                let args = match args
                    .iter()
                    .map(|value| term_from_json(&mut u, value))
                    .collect::<Result<Vec<_>, _>>()
                {
                    Ok(args) => args,
                    Err(e) => return fail(&e),
                };
                let Some(weight) = change.get("w").and_then(|value| value.as_i64()) else {
                    return fail(&"frontier change w must be an integer");
                };
                parsed.push(dl8::_6_eval::ir_eval::SignedRow {
                    row: Row { rel, args },
                    weight,
                });
            }
            frontiers.push(parsed);
        }
        let public_names: std::collections::HashMap<TermId, String> = names
            .iter()
            .map(|(name, relation)| (*relation, name.clone()))
            .collect();
        let (closure, results) = match reconciler.run_frontiers(
            &mut u,
            &program,
            backend,
            &frontiers,
            &public_names,
            &mut fx,
        ) {
            Ok(result) => result,
            Err(diagnostic) => {
                let out = closure_to_json(&u, &[], &[diagnostic]);
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
                return ExitCode::from(1);
            }
        };
        let mut out = closure_to_json(&u, &closure.rows, &closure.diagnostics);
        out["frontiers"] = serde_json::Value::Array(
            results
                .into_iter()
                .map(|result| {
                    let closure = closure_to_json(
                        &u,
                        &result.closure.rows,
                        &result.closure.diagnostics,
                    );
                    let mut delta_rows = serde_json::Map::new();
                    for (name, value) in [
                        ("filter", result.counters.delta_rows.filter),
                        ("join", result.counters.delta_rows.join),
                        ("antijoin", result.counters.delta_rows.antijoin),
                        ("reduce", result.counters.delta_rows.reduce),
                        ("topk", result.counters.delta_rows.topk),
                        ("window", result.counters.delta_rows.window),
                        ("mint", result.counters.delta_rows.mint),
                    ] {
                        if let Some(value) = value {
                            delta_rows.insert(name.into(), value.into());
                        }
                    }
                    let mut counters = serde_json::Map::from_iter([
                        (
                            "rows_written".into(),
                            result.counters.rows_written.into(),
                        ),
                        ("delta_rows".into(), delta_rows.into()),
                    ]);
                    for (name, value) in [
                        ("rounds", result.counters.rounds),
                        ("interned", result.counters.interned),
                        ("statements", result.counters.statements),
                    ] {
                        if let Some(value) = value {
                            counters.insert(name.into(), value.into());
                        }
                    }
                    let deltas: Vec<_> = result
                        .deltas
                        .into_iter()
                        .map(|delta| {
                            serde_json::json!({
                                "rel": delta.relation,
                                "args": delta.args.iter().map(|term| term_to_json(&u, *term)).collect::<Vec<_>>(),
                                "w": delta.weight,
                            })
                        })
                        .collect();
                    serde_json::json!({
                        "closure": closure,
                        "deltas": deltas,
                        "counters": counters,
                    })
                })
                .collect(),
        );
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
        return if closure.diagnostics.is_empty() {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        };
    }
    // Without a stdin frontier stream, the tick loop below prints one closure.
    if frontiers_path == Some(Path::new("-")) {
        let backend = match dl8::_6_eval::ir_eval::configured_backend() {
            Ok(backend) => backend,
            Err(e) => return fail(&e),
        };
        let stream = if frontiers_path == Some(Path::new("-")) {
            let (sender, receiver) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                for line in std::io::stdin().lock().lines() {
                    if sender.send(line).is_err() {
                        break;
                    }
                }
            });
            Some(receiver)
        } else {
            None
        };
        let mut input_error = None;
        let mut external = |u: &mut Universe| -> Option<Vec<dl8::_6_eval::ir_eval::SignedRow>> {
            let Some(receiver) = &stream else {
                return None;
            };
            match receiver.try_recv() {
                Ok(Ok(line)) => {
                    let value: serde_json::Value = match serde_json::from_str(&line) {
                        Ok(value) => value,
                        Err(error) => {
                            input_error = Some(error.to_string());
                            return None;
                        }
                    };
                    let Some(changes) = value.get("changes").and_then(|v| v.as_array()) else {
                        input_error = Some("frontier changes must be an array".into());
                        return None;
                    };
                    let mut parsed = Vec::with_capacity(changes.len());
                    for change in changes {
                        let Some(relation_name) = change.get("rel").and_then(|v| v.as_str()) else {
                            input_error = Some("frontier change rel must be a string".into());
                            return None;
                        };
                        let Some(&rel) = names.get(relation_name) else {
                            input_error =
                                Some(format!("unknown frontier relation {relation_name}"));
                            return None;
                        };
                        let Some(args) = change.get("args").and_then(|v| v.as_array()) else {
                            input_error = Some("frontier change args must be an array".into());
                            return None;
                        };
                        let args = match args
                            .iter()
                            .map(|v| term_from_json(u, v))
                            .collect::<Result<Vec<_>, _>>()
                        {
                            Ok(args) => args,
                            Err(error) => {
                                input_error = Some(error.to_string());
                                return None;
                            }
                        };
                        let Some(weight) = change.get("w").and_then(|v| v.as_i64()) else {
                            input_error = Some("frontier change w must be an integer".into());
                            return None;
                        };
                        parsed.push(dl8::_6_eval::ir_eval::SignedRow {
                            row: Row { rel, args },
                            weight,
                        });
                    }
                    Some(parsed)
                }
                Ok(Err(error)) => {
                    input_error = Some(error.to_string());
                    None
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => Some(Vec::new()),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => None,
            }
        };
        let mut tick = 0usize;
        let public_names: std::collections::HashMap<TermId, String> = names
            .iter()
            .map(|(name, relation)| (*relation, name.clone()))
            .collect();
        let mut on_settle = |u: &Universe,
                             closure: &dl8::_6_eval::evaluate::Closure,
                             deltas: &[dl8::_6_eval::ir_eval::NamedDelta],
                             counters: &ivm_engine::Counters| {
            let mut out = closure_to_json(u, &closure.rows, &closure.diagnostics);
            out["ticks"] = serde_json::json!(tick);
            out["deltas"] = serde_json::json!(deltas.iter().map(|delta| {
                    serde_json::json!({
                        "rel": delta.relation,
                        "args": delta.args.iter().map(|term| term_to_json(u, *term)).collect::<Vec<_>>(),
                        "w": delta.weight,
                    })
                }).collect::<Vec<_>>());
            out["counters"] = serde_json::json!({
                "rows_written": counters.rows_written,
                "delta_rows": {
                    "filter": counters.delta_rows.filter,
                    "join": counters.delta_rows.join,
                    "antijoin": counters.delta_rows.antijoin,
                    "reduce": counters.delta_rows.reduce,
                    "topk": counters.delta_rows.topk,
                    "window": counters.delta_rows.window,
                    "mint": counters.delta_rows.mint,
                },
                "rounds": counters.rounds,
                "interned": counters.interned,
                "statements": counters.statements,
            });
            println!("{}", serde_json::to_string(&out).unwrap());
            let _ = std::io::stdout().flush();
            tick += 1;
        };
        let result = reconciler.run_live(
            &mut u,
            &program,
            backend,
            &public_names,
            max_ticks,
            &mut external,
            &mut on_settle,
            &mut fx,
        );
        if let Some(error) = input_error {
            return fail(&error);
        }
        return match result {
            Ok(closure) if closure.diagnostics.is_empty() => ExitCode::SUCCESS,
            Ok(_) => ExitCode::from(1),
            Err(diagnostic) => {
                tracing::error!(phase = "run", error = ?diagnostic);
                ExitCode::from(1)
            }
        };
    }
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

fn query_cli(path: &Path, repo: Option<&Path>, format: QueryFormat) -> ExitCode {
    let fail = |error: &dyn std::fmt::Display| {
        tracing::error!(phase = "query", error = %error, path = %path.display());
        ExitCode::from(2)
    };
    if let Err(e) = dl8::_6_eval::ir_eval::configured_backend() {
        return fail(&e);
    }

    let path = match std::fs::canonicalize(path) {
        Ok(path) => path,
        Err(e) => return fail(&e),
    };
    let repo = repo.unwrap_or_else(|| Path::new("."));
    let repo = match std::fs::canonicalize(repo) {
        Ok(repo) => repo,
        Err(e) => return fail(&e),
    };
    if let Err(e) = std::env::set_current_dir(&repo) {
        return fail(&e);
    }

    let mut u = Universe::new();
    let mut event = |_event: Event| {};
    let compiled = match compile_input(&mut u, std::slice::from_ref(&path), None, &[], &mut event) {
        Ok(compiled) => compiled,
        Err(Stop::Io(message)) => return fail(&message),
        Err(other) => return fail(&format!("{other:?}")),
    };
    if !compiled.diagnostics.is_empty() {
        let diagnostics: Vec<_> = compiled
            .diagnostics
            .iter()
            .map(|term| term_to_json(&u, *term))
            .collect();
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({ "diagnostics": diagnostics })).unwrap()
        );
        return ExitCode::from(1);
    }
    let encoded = match program_to_json(&mut u, compiled.runtime_program) {
        Ok(program) => program,
        Err(diagnostic) => {
            println!(
                "{}",
                serde_json::to_string(&serde_json::json!({
                    "diagnostics": [term_to_json(&u, diagnostic)]
                }))
                .unwrap()
            );
            return ExitCode::from(1);
        }
    };
    let (mut program, names) = match program_from_json(&mut u, &encoded)
        .and_then(|program| Ok((program, program_names(&mut u, &encoded)?)))
    {
        Ok(parts) => parts,
        Err(e) => return fail(&e),
    };

    let source_names = ["fs.at", "fs.json", "git.refs", "git.history", "extract"];
    let serve: Vec<String> = source_names
        .iter()
        .filter(|name| names.contains_key(**name))
        .map(|name| (*name).to_string())
        .collect();
    let diagnostics = serve_relations(&mut u, &mut program, &names, &serve);
    if !diagnostics.is_empty() {
        let out = closure_to_json(&u, &[], &diagnostics);
        println!("{}", serde_json::to_string(&out).unwrap());
        return ExitCode::from(1);
    }
    let live_serve: Vec<String> = serve
        .iter()
        .filter(|name| name.as_str() == "git.refs")
        .cloned()
        .collect();
    let executors = match query_executors_for(&mut u, &names, &live_serve) {
        Ok(executors) => executors,
        Err(diagnostics) => {
            let out = closure_to_json(&u, &[], &diagnostics);
            println!("{}", serde_json::to_string(&out).unwrap());
            return ExitCode::from(1);
        }
    };
    let mut reconciler = match Reconciler::new(&mut u, &names, executors) {
        Ok(reconciler) => reconciler,
        Err(diagnostics) => {
            let out = closure_to_json(&u, &[], &diagnostics);
            println!("{}", serde_json::to_string(&out).unwrap());
            return ExitCode::from(1);
        }
    };
    let mut rows = Store::default();
    seed_compiler_rows(&u, &mut rows, &compiled.compiler_rows);
    let mut trace = |_trace: Trace| {};
    let reconciled = match reconciler.run(&mut u, &program, &mut rows, None, 1, &mut trace) {
        Ok(reconciled) => reconciled,
        Err(e) => return fail(&e),
    };
    if !reconciled.closure.diagnostics.is_empty() {
        let out = closure_to_json(&u, &[], &reconciled.closure.diagnostics);
        println!("{}", serde_json::to_string(&out).unwrap());
        return ExitCode::from(1);
    }

    let root = path.display().to_string();
    let derived: std::collections::HashSet<TermId> = program
        .rules
        .iter()
        .map(|rule| rule.rel)
        .filter(|rel| relation_is_from_file(&u, *rel, &root))
        .collect();
    let relation_names: std::collections::HashMap<TermId, &str> = names
        .iter()
        .filter(|(_, rel)| derived.contains(rel))
        .map(|(name, rel)| (*rel, name.as_str()))
        .collect();
    let selected: Vec<(&str, &Row)> = reconciled
        .closure
        .rows
        .iter()
        .filter_map(|row| relation_names.get(&row.rel).map(|name| (*name, row)))
        .collect();
    print_query_rows(&u, &selected, format);
    ExitCode::SUCCESS
}

/// Comptime already settled every Once source. Carry its ground closure into
/// the query store so the runtime frontier does not read those sources again.
fn seed_compiler_rows(u: &Universe, rows: &mut Store, compiler_rows: &[TermId]) {
    for term in compiler_rows {
        let Some([relation, arguments]) = u.args::<2>(*term, "call") else {
            continue;
        };
        let Some(arguments) = u.as_list(arguments) else {
            continue;
        };
        rows.insert(relation, arguments.into_boxed_slice());
    }
}

fn relation_is_from_file(u: &Universe, relation: TermId, path: &str) -> bool {
    let Some(target) = u.unary(relation, "ref") else {
        return false;
    };
    let Some([unit, _]) = u.args::<2>(target, "owner") else {
        return false;
    };
    let Some(file) = u.unary(unit, "file") else {
        return false;
    };
    match u.get(file) {
        Term::Atom(sym) | Term::Str(sym) => u.sym_str(*sym) == path,
        _ => false,
    }
}

fn print_query_rows(u: &Universe, rows: &[(&str, &Row)], format: QueryFormat) {
    match format {
        QueryFormat::Json => {
            let rows: Vec<_> = rows
                .iter()
                .map(|(relation, row)| {
                    serde_json::json!({
                        "relation": relation,
                        "args": row.args.iter().map(|arg| term_to_json(u, *arg)).collect::<Vec<_>>()
                    })
                })
                .collect();
            println!("{}", serde_json::to_string(&rows).unwrap());
        }
        QueryFormat::Table => {
            for (relation, row) in rows {
                print!("({relation}");
                for arg in &row.args {
                    let value = u.unary(*arg, "const").unwrap_or(*arg);
                    print!(" {}", u.display(value));
                }
                println!(")");
            }
        }
    }
}

fn compile_input(
    u: &mut Universe,
    files: &[PathBuf],
    project: Option<&Path>,
    streams: &[PathBuf],
    fx: &mut dyn FnMut(Event),
) -> Result<dl8::Compile, Stop> {
    let paths: Vec<&Path> = files.iter().map(PathBuf::as_path).collect();
    let streams: Vec<&Path> = streams.iter().map(PathBuf::as_path).collect();
    match (project, paths.as_slice()) {
        (None, [one]) if streams.is_empty() => dl8::compile(u, one, fx),
        (root, _) => {
            let root = root.map(Path::to_path_buf).unwrap_or_else(|| {
                paths[0]
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from("."))
            });
            dl8::compile_project(u, &root, &paths, &streams, fx)
        }
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
    let compiled = compile_input(&mut u, files, project, streams, &mut fx);
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

fn build_cli(
    files: &[PathBuf],
    project: Option<&std::path::Path>,
    streams: &[PathBuf],
    out: &Path,
    check: bool,
) -> ExitCode {
    let mut u = Universe::new();
    let compiled = match compile_input(&mut u, files, project, streams, &mut |_: Event| {}) {
        Ok(compiled) => compiled,
        Err(Stop::Io(message)) => {
            tracing::error!(phase = "build", error = %message);
            return ExitCode::from(2);
        }
        Err(other) => {
            tracing::error!(phase = "build", error = ?other);
            return ExitCode::from(3);
        }
    };
    let report = |u: &Universe, diagnostics: &[TermId], written: serde_json::Value| {
        let diagnostics: Vec<serde_json::Value> =
            diagnostics.iter().map(|t| term_to_json(u, *t)).collect();
        let out = serde_json::json!({"diagnostics": diagnostics, "files": written});
        println!("{}", serde_json::to_string(&out).unwrap());
    };
    if !compiled.diagnostics.is_empty() {
        report(&u, &compiled.diagnostics, serde_json::json!([]));
        return ExitCode::from(1);
    }
    let rows = match dl8::_8_driver::_6_build::files(&mut u, &compiled.compiler_rows) {
        Ok(rows) => rows,
        Err(diagnostics) => {
            report(&u, &diagnostics, serde_json::json!([]));
            return ExitCode::from(1);
        }
    };
    let mut written = Vec::with_capacity(rows.len());
    let mut stale = false;
    for (path, text) in &rows {
        let target = out.join(path);
        let same = std::fs::read(&target).is_ok_and(|bytes| bytes == text.as_bytes());
        stale |= !same;
        if !check && !same {
            let made = target
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|_| std::fs::write(&target, text));
            if let Err(e) = made {
                tracing::error!(phase = "build", path = %target.display(), error = %e);
                return ExitCode::from(2);
            }
        }
        written.push(serde_json::json!({"path": path, "bytes": text.len(), "changed": !same}));
    }
    report(&u, &[], serde_json::Value::Array(written));
    match check && stale {
        true => ExitCode::from(1),
        false => ExitCode::SUCCESS,
    }
}
