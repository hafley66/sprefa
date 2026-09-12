//! The three project fact loaders: a directory tree, a TSI stream and a bag of
//! source observations, each into module basements the compiler can see.
//! Port of `v7/src/2_comptime/0b_filesystem_grapher.pl`,
//! `0c_extract_loader.pl` and `0d_source_fact_loader.pl`. Plan:
//! `plans/v8/2026-09-12-v8-load.PLAN.md`. Oracle: `v8/oracle/load/`.

#[path = "_0_api.rs"]
pub mod api;
#[path = "_4_identity.rs"]
pub mod identity;
#[path = "_8_json.rs"]
pub mod json;
#[path = "_1_paths.rs"]
pub mod paths;
#[path = "_2_project.rs"]
pub mod project;
#[path = "_7_read.rs"]
pub mod read;
#[path = "_6_source.rs"]
pub mod source;
#[path = "_5_graph.rs"]
pub mod tsi;
#[path = "_3_wire.rs"]
pub mod wire;

pub use api::{Installed, Loaded};
pub use project::install_project_graph;
pub use source::{install_source_fact_graph, load_source_fact_texts};
pub use tsi::{install_tsi_graph, tsi_expression_environment};
pub use wire::{accepted_rows, load_tsi_lines};

use std::path::Path;
use std::process::ExitCode;

pub fn cli(input: &Path) -> ExitCode {
    let text = match std::fs::read_to_string(input) {
        Ok(text) => text,
        Err(e) => {
            tracing::error!(phase = "load", error = %e, path = %input.display());
            return ExitCode::from(2);
        }
    };
    let case: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(e) => {
            tracing::error!(phase = "load", error = %e, path = %input.display());
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
            tracing::error!(phase = "load", error = %e);
            ExitCode::from(2)
        }
    }
}
