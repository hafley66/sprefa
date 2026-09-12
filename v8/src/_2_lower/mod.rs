//! The DL7 lowerer. Port of `v7/src/2_comptime/0_lowerer.pl` and the JITI
//! stores in `v7/src/2_comptime/0_graph_lookup.pl`. Plan:
//! `plans/v8/2026-09-12-v8-lower.PLAN.md`. Oracle: `v8/oracle/lower/`.

#[path = "_0_api.rs"]
pub mod api;
#[path = "_0_cx.rs"]
pub mod cx;
#[path = "_2_declare.rs"]
pub mod declare;
#[path = "_5_derived.rs"]
pub mod derived;
#[path = "_7_execute.rs"]
pub mod execute;
#[path = "_8_express.rs"]
pub mod express;
#[path = "_1_forms.rs"]
pub mod forms;
#[path = "_3_host.rs"]
pub mod host;
#[path = "_10_index.rs"]
pub mod index;
#[path = "_11_json.rs"]
pub mod json;
#[path = "_9_kernel.rs"]
pub mod kernel;
#[path = "_6_partial.rs"]
pub mod partial;
#[path = "_4_promote.rs"]
pub mod promote;
#[path = "_1_slots.rs"]
pub mod slots;
#[path = "_12_units.rs"]
pub mod units;

pub use api::{lower_datalog, Lowered};
pub use cx::{CallPolicy, Cx};
pub use derived::Stop;
pub use units::{install_module_aliases, lower_compiler_units, merge_module_basements, Units};

use std::path::Path;
use std::process::ExitCode;

pub fn cli(input: &Path) -> ExitCode {
    let text = match std::fs::read_to_string(input) {
        Ok(text) => text,
        Err(e) => {
            tracing::error!(phase = "lower", error = %e, path = %input.display());
            return ExitCode::from(2);
        }
    };
    let case: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(e) => {
            tracing::error!(phase = "lower", error = %e, path = %input.display());
            return ExitCode::from(2);
        }
    };
    match json::run(&case) {
        Ok(out) => {
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
            let clean = out["diagnostics"].as_array().is_some_and(|d| d.is_empty());
            if clean {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => {
            tracing::error!(phase = "lower", error = %e);
            ExitCode::from(3)
        }
    }
}
