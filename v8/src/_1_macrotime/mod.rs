//! Not built yet. Owned by the _1_macrotime lane.

use std::path::Path;
use std::process::ExitCode;

pub fn cli(_input: &Path) -> ExitCode {
    eprintln!("dl8 expand: not built yet"); // @eprintln-ok
    ExitCode::from(3)
}
