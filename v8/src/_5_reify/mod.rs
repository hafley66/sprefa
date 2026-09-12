//! Not built yet. Owned by the _5_reify lane.

use std::path::Path;
use std::process::ExitCode;

pub fn cli(_input: &Path) -> ExitCode {
    eprintln!("dl8 reify: not built yet"); // @eprintln-ok
    ExitCode::from(3)
}
