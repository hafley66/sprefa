//! Not built yet. Owned by the _0_read lane.

use std::path::Path;
use std::process::ExitCode;

pub fn cli(_input: &Path) -> ExitCode {
    eprintln!("dl8 read: not built yet"); // @eprintln-ok
    ExitCode::from(3)
}
