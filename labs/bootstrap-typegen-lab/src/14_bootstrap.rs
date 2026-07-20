use std::path::Path;

use crate::codegen_rust::emit_models;
use crate::store::Store;

pub fn bootstrap(store: &Store, output: &Path) -> Result<String, std::io::Error> {
    std::fs::create_dir_all(output)?;
    std::fs::write(output.join("models.rs"), emit_models(store))?;
    let report = "stage zero generated semantic model Rust types; stage-one self-regeneration stops at the parser/emitter boundary because the parser and emitters are still trusted Rust modules, and copying their bodies would violate the bootstrap requirement\n";
    std::fs::write(output.join("bootstrap-boundary.txt"), report)?;
    Ok(report.to_owned())
}
