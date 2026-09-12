//! Not built yet. Owned by the _4_comptime/_0_load lane.

use std::path::Path;
use std::process::ExitCode;

pub fn cli(_input: &Path) -> ExitCode {
    eprintln!("dl8 load: not built yet"); // @eprintln-ok
    ExitCode::from(3)
}
