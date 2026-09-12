//! The compiler fixpoint. Port of `v7/src/2_comptime/2_compiler.pl:700-1591`,
//! `1a_generated_program_assembler.pl` and `1d_host_planner.pl`.

#[path = "_1_api.rs"]
pub mod api;
#[path = "_3_assemble.rs"]
pub mod assemble;
#[path = "_5_finish.rs"]
pub mod finish;
#[path = "_4_host.rs"]
pub mod host;
#[path = "_6_json.rs"]
pub mod json;
#[path = "_0_load/mod.rs"]
pub mod load;
#[path = "_2_rounds.rs"]
pub mod rounds;

pub use api::{evaluate_checked, Compiled, Generated, Refreeze, Sources};
pub use assemble::assemble_generated_program;
pub use rounds::{Outcome, Round, RoundState};

use std::path::Path;
use std::process::ExitCode;

pub fn cli(input: &Path) -> ExitCode {
    let text = match std::fs::read_to_string(input) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("dl8: cannot read {}: {e}", input.display()); // @eprintln-ok
            return ExitCode::from(2);
        }
    };
    let case: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(e) => {
            eprintln!("dl8: {}: {e}", input.display()); // @eprintln-ok
            return ExitCode::from(2);
        }
    };
    match json::run(&case) {
        Ok((out, code)) => {
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
            ExitCode::from(code)
        }
        Err(e) => {
            eprintln!("dl8 comptime: {e}"); // @eprintln-ok
            ExitCode::from(3)
        }
    }
}
