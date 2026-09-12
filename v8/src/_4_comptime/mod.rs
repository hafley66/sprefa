//! Not built yet. `_0_load` is owned by the loaders lane; every other file
//! here by the comptime lane.

use std::path::Path;
use std::process::ExitCode;

#[path = "_0_load/mod.rs"]
pub mod load;

pub fn cli(_input: &Path) -> ExitCode {
    eprintln!("dl8 comptime: not built yet"); // @eprintln-ok
    ExitCode::from(3)
}
