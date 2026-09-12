//! The DL7 checker. Port of `v7/src/2_comptime/1_checker.pl` and the two
//! checker-only lookups in `v7/src/2_comptime/0_graph_lookup.pl`. Plan:
//! `plans/v8/2026-09-12-v8-check.PLAN.md`. Oracle: `v8/oracle/check/`.

#[path = "_0_api.rs"]
pub mod api;
#[path = "_3_bind.rs"]
pub mod bind;
#[path = "_1_graph.rs"]
pub mod graph;
#[path = "_8_json.rs"]
pub mod json;
#[path = "_5_kernel.rs"]
pub mod kernel;
#[path = "_4_mode.rs"]
pub mod mode;
#[path = "_2_resolve.rs"]
pub mod resolve;
#[path = "_7_resolved.rs"]
pub mod resolved;
#[path = "_6_strata.rs"]
pub mod strata;

pub use api::{check_datalog, Checked, Stop};
pub use mode::check_goal_sequence;
pub use resolved::check_resolved_rules;

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
            ExitCode::from(code as u8)
        }
        Err(e) => {
            eprintln!("dl8 check: {e}"); // @eprintln-ok
            ExitCode::from(3)
        }
    }
}
