//! `dl7_text_unit/5` (`0_reader/2_embedder.pl:20`) and `parse_program_texts/7`
//! (`2_compiler.pl:181`).

use crate::_0_read::digest::sha256_hex;
use crate::_0_read::expand::{expand_dl7, Stop};
use crate::_0_read::read;
use crate::_1_macrotime::_1_reify::reify_terms;
use crate::_6_eval::term::{TermId, Universe};

pub struct Unit {
    pub unit: TermId,
    pub diagnostics: Vec<TermId>,
}

/// `2_embedder.pl:23`. The syntax graph is built only to surface its
/// diagnostics; the reader expansion is what survives into the unit.
pub fn text_unit(
    u: &mut Universe,
    origin: TermId,
    reader_path: &str,
    text: &str,
) -> Result<Unit, Stop> {
    let digest = u.atom(&sha256_hex(text));
    let digest = u.compound("content_sha256", vec![digest]);
    let result = read(u, reader_path, text);
    let mut diagnostics = result.diagnostics;
    if diagnostics.is_empty() {
        let (_, reify_diagnostics) = reify_terms(u, &result.forms, &result.source_rows);
        diagnostics = reify_diagnostics;
    }
    let (forms, source_rows, expansion_rows) = if diagnostics.is_empty() {
        let expanded = expand_dl7(u, &result.forms, &result.source_rows)?;
        diagnostics = expanded.diagnostics;
        (
            expanded.forms,
            expanded.source_rows,
            expanded.expansion_rows,
        )
    } else {
        (Vec::new(), Vec::new(), Vec::new())
    };
    let forms = u.list(&forms);
    let source_rows = u.list(&source_rows);
    let expansion_rows = u.list(&expansion_rows);
    Ok(Unit {
        unit: u.compound(
            "dl7_unit",
            vec![origin, digest, forms, source_rows, expansion_rows],
        ),
        diagnostics,
    })
}

/// `:300`, read under the atom `prelude`, never a filename.
pub fn prelude_unit(u: &mut Universe, text: &str) -> Result<Unit, Stop> {
    let origin = u.atom("prelude");
    text_unit(u, origin, "prelude", text)
}

/// `:427`, read under the atom `macrotime`.
pub fn macrotime_unit(u: &mut Universe, text: &str) -> Result<Unit, Stop> {
    let origin = u.atom("macrotime");
    text_unit(u, origin, "macrotime", text)
}

/// `3_file_loader.pl:14`: origin `file(CanonicalPath)`, reader path the same
/// canonical path.
pub fn file_unit(u: &mut Universe, canonical: &str, text: &str) -> Result<Unit, Stop> {
    let path = u.atom(canonical);
    let origin = u.compound("file", vec![path]);
    text_unit(u, origin, canonical, text)
}
