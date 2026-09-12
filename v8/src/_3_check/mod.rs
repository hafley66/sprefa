//! Not built yet. Owned by the _3_check lane.

use std::path::Path;
use std::process::ExitCode;

pub fn cli(_input: &Path) -> ExitCode {
    eprintln!("dl8 check: not built yet"); // @eprintln-ok
    ExitCode::from(3)
}
