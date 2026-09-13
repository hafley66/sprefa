//! The logical program reifier, grapher and artifact-emitter front door. Port
//! of `v7/src/3_emit/0_logical_program_reifier.pl`,
//! `0a_logical_program_grapher.pl` and `1_artifact_emitter.pl`. Plan:
//! `plans/v8/2026-09-12-v8-reify.PLAN.md`. Oracle: `v8/oracle/reify/`.
//!
//! Pure: slices in, owned rows out. In the redux shape the `Effect` is `Never`,
//! so no `fx` channel is carried. `&mut Universe` is the term arena, the same
//! parameter `_3_check` and `_6_eval` take.

#[path = "_0_api.rs"]
pub mod api;
#[path = "_2_calls.rs"]
pub mod calls;
#[path = "_4_emit.rs"]
pub mod emit;
#[path = "_3_graph.rs"]
pub mod graph;
#[path = "_6_json.rs"]
pub mod json;
#[path = "_1_rows.rs"]
pub mod rows;
#[path = "_5_validate.rs"]
pub mod validate;

pub use api::{Calls, CompilerView, Emitted, Stop};
pub use calls::{logical_program_calls, logical_program_rows_calls};
pub use emit::{compiler_view, emit_compiled, Emitter};
pub use graph::{logical_program_graph_calls, logical_program_graph_rows};
pub use rows::{logical_program_rows, logical_program_rows_term};
pub use validate::validate_functional_rows;

use std::path::Path;
use std::process::ExitCode;

pub fn cli(input: &Path) -> ExitCode {
    let text = match std::fs::read_to_string(input) {
        Ok(text) => text,
        Err(e) => {
            tracing::error!(phase = "reify", error = %e, path = %input.display());
            return ExitCode::from(2);
        }
    };
    let case: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(e) => {
            tracing::error!(phase = "reify", error = %e, path = %input.display());
            return ExitCode::from(2);
        }
    };
    match json::run(&case) {
        Ok((out, code)) => {
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
            ExitCode::from(code as u8)
        }
        Err(e) => {
            tracing::error!(phase = "reify", error = %e);
            ExitCode::from(3)
        }
    }
}
