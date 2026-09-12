//! The one subscribe: reads a program, runs the pipe, prints the result.

use clap::{Parser, Subcommand};
use dl8::_6_eval::json::{closure_to_json, program_from_json};
use dl8::_6_eval::{evaluate, Trace, Universe};
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
    Expand { case: PathBuf },
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
    /// Evaluate a checked-goal program (JSON) and print its closure as JSON.
    Eval {
        program: PathBuf,
        /// Print one line per stratum and round to stderr.
        #[arg(long)]
        trace: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Read { file } => dl8::_0_read::cli(&file),
        Command::Expand { case } => dl8::_1_macrotime::cli(&case),
        Command::Lower { case } => dl8::_2_lower::cli(&case),
        Command::Check { case } => dl8::_3_check::cli(&case),
        Command::Load { case } => dl8::_4_comptime::load::cli(&case),
        Command::Comptime { case } => dl8::_4_comptime::cli(&case),
        Command::Reify { case } => dl8::_5_reify::cli(&case),
        Command::Eval { program, trace } => {
            let text = match std::fs::read_to_string(&program) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("dl8: cannot read {}: {e}", program.display()); // @eprintln-ok
                    return ExitCode::from(2);
                }
            };
            let value: serde_json::Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("dl8: {}: {e}", program.display()); // @eprintln-ok
                    return ExitCode::from(2);
                }
            };
            let mut u = Universe::new();
            let program = match program_from_json(&mut u, &value) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("dl8: {e}"); // @eprintln-ok
                    return ExitCode::from(2);
                }
            };
            let mut fx = |t: Trace| {
                if trace {
                    eprintln!("{t:?}"); // @eprintln-ok
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
