//! The one subscribe: reads a program, runs the pipe, prints the result.

use clap::{Parser, Subcommand};
use dl8::_6_eval::json::term_to_json;
use dl8::_6_eval::json::{closure_to_json, program_from_json};
use dl8::_6_eval::{evaluate, Trace, Universe};
use dl8::_8_driver::{Event, Stop};
use hafley_observe::{Config, OutputFormat};
use std::io::IsTerminal;
use std::path::PathBuf;
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
    /// Evaluate a checked-goal program (JSON) and print its closure as JSON.
    Eval {
        program: PathBuf,
        /// Print one line per stratum and round to stderr.
        #[arg(long)]
        trace: bool,
    },
}

fn trace_requested(command: &Command) -> bool {
    match command {
        Command::Expand { trace, .. }
        | Command::Compile { trace, .. }
        | Command::Eval { trace, .. } => *trace,
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
    match cli.command {
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
        Command::Eval { program, trace } => {
            let text = match std::fs::read_to_string(&program) {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!(phase = "eval", error = %e, path = %program.display());
                    return ExitCode::from(2);
                }
            };
            let value: serde_json::Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!(phase = "eval", error = %e, path = %program.display());
                    return ExitCode::from(2);
                }
            };
            let mut u = Universe::new();
            let program = match program_from_json(&mut u, value.get("program").unwrap_or(&value)) {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!(phase = "eval", error = %e);
                    return ExitCode::from(2);
                }
            };
            let mut fx = |t: Trace| {
                if trace {
                    tracing::debug!(target: "dl8::trace", event = ?t);
                }
            };
            let closure = evaluate(&mut u, &program, &mut fx);
            let out = closure_to_json(&u, &closure.rows, &closure.diagnostics);
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
            if closure.diagnostics.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
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
    let encode = |ids: &[dl8::_6_eval::TermId]| {
        serde_json::Value::Array(ids.iter().map(|t| term_to_json(&u, *t)).collect())
    };
    let mut out = serde_json::Map::new();
    out.insert("compiler_rows".into(), encode(&compiled.compiler_rows));
    out.insert(
        "runtime_program".into(),
        term_to_json(&u, compiled.runtime_program),
    );
    out.insert("diagnostics".into(), encode(&compiled.diagnostics));
    println!(
        "{}",
        serde_json::to_string(&serde_json::Value::Object(out)).unwrap()
    );
    if compiled.diagnostics.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
