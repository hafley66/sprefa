//! The driver's byte edge. `type_prelude_paths/1` (`2_compiler.pl:313`) and
//! `standard_macrotime_paths/1` (`:331`) are `include_str!` here; §4 of the plan.

use std::io;
use std::path::Path;

/// `v7/prelude/*.dl7` at `f5018ad23`, in `sort/2` order.
pub const PRELUDE: [&str; 6] = [
    include_str!("../../prelude/0_constructors.dl7"),
    include_str!("../../prelude/1_declarations.dl7"),
    include_str!("../../prelude/2_constructor_rules.dl7"),
    include_str!("../../prelude/3_derived_rules.dl7"),
    include_str!("../../prelude/4_type_algebra.dl7"),
    include_str!("../../prelude/6_modules.dl7"),
];

/// `macrotime/*.dl7`: the v7 standard library at `f5018ad23`, then the caret
/// macro.
pub const MACROTIME: [&str; 2] = [
    include_str!("../../macrotime/0_standard.dl7"),
    include_str!("../../macrotime/1_caret.dl7"),
];

/// The wave loop as rules, appended to `MACROTIME` under `DL8_MACROTIME=engine`.
pub const WAVES: &str = include_str!("../../macrotime/3_waves.dl7");

/// `std/*.dl7`, bundled in the binary like the prelude. The first field is the
/// tail of `@std/<name>`.
pub const STD: [(&str, &str); 8] = [
    ("dl6", include_str!("../../std/dl6.dl7")),
    ("doc", include_str!("../../std/doc.dl7")),
    ("fs", include_str!("../../std/fs.dl7")),
    ("git", include_str!("../../std/git.dl7")),
    ("http", include_str!("../../std/http.dl7")),
    ("lsp", include_str!("../../std/lsp.dl7")),
    ("oai", include_str!("../../std/oai.dl7")),
    ("tsi", include_str!("../../std/tsi.dl7")),
];

/// The one magic import prefix.
pub const STD_PREFIX: &str = "@std/";

pub fn std_text(name: &str) -> Option<&'static str> {
    STD.iter()
        .find(|(bundled, _)| *bundled == name)
        .map(|(_, text)| *text)
}

/// `join_prelude_texts/2` at `:371`: one newline between texts.
pub fn join(texts: &[&str]) -> String {
    texts.join("\n")
}

pub fn prelude_text() -> String {
    join(&PRELUDE)
}

pub fn macrotime_text() -> String {
    match crate::_1_macrotime::macrotime_mode() {
        crate::_1_macrotime::MacrotimeMode::Rust => join(&MACROTIME),
        crate::_1_macrotime::MacrotimeMode::Engine => {
            let mut texts = MACROTIME.to_vec();
            texts.push(WAVES);
            join(&texts)
        }
    }
}

/// `absolute_file_name/3` then `read_file_to_string/3`.
pub fn program_text(path: &Path) -> io::Result<(String, String)> {
    let canonical = std::fs::canonicalize(path)?;
    let text = std::fs::read_to_string(&canonical)?;
    tracing::debug!(target: "dl8::io", path = %canonical.display(), bytes = text.len());
    Ok((canonical.display().to_string(), text))
}

pub fn canonical_dir(path: &Path) -> io::Result<String> {
    Ok(std::fs::canonicalize(path)?.display().to_string())
}
