//! `dl8 build`: the `file` sink of `@std/fs`, found by its `:` row under the
//! module node, never by the name an importer gives the module.

use crate::_6_eval::{Term, TermId, Universe};
use std::collections::BTreeMap;

/// The `file` relation of module `@std/fs`, when the program imports it.
pub fn file_sink(u: &mut Universe, compiler_rows: &[TermId]) -> Option<TermId> {
    let fs = u.atom("fs");
    let std = u.compound("std", vec![fs]);
    let module = u.compound("module", vec![std]);
    let module = u.compound("ref", vec![module]);
    let file = u.atom("file");
    let label = u.compound("const", vec![file]);
    let colon = u.atom(":");
    let kernel = u.compound("kernel", vec![colon]);
    let colon = u.compound("ref", vec![kernel]);
    compiler_rows.iter().find_map(|row| {
        let [rel, args] = u.args::<2>(*row, "call")?;
        let args = u.as_list(args)?;
        (rel == colon && args.len() == 4 && args[0] == module && args[1] == label).then_some(args[2])
    })
}

fn text(u: &Universe, term: TermId) -> Option<String> {
    match u.get(u.unary(term, "const")?) {
        Term::Str(sym) => Some(u.sym_str(*sym).to_owned()),
        _ => None,
    }
}

/// Every `(fs.file Path Text)` row, by path. Two texts for one path is
/// `fs_file_conflict(Path)`; a non-string cell is `fs_file_not_text(Row)`.
pub fn files(
    u: &mut Universe,
    compiler_rows: &[TermId],
) -> Result<BTreeMap<String, String>, Vec<TermId>> {
    let mut out = BTreeMap::new();
    let mut diagnostics = Vec::new();
    let Some(sink) = file_sink(u, compiler_rows) else {
        return Ok(out);
    };
    for row in compiler_rows {
        let Some([rel, args]) = u.args::<2>(*row, "call") else {
            continue;
        };
        if rel != sink {
            continue;
        }
        let cells = u.as_list(args).unwrap_or_default();
        let parsed = match cells[..] {
            [path, body] => text(u, path).zip(text(u, body)),
            _ => None,
        };
        let Some((path, body)) = parsed else {
            diagnostics.push(u.compound("fs_file_not_text", vec![*row]));
            continue;
        };
        match out.get(&path) {
            Some(existing) if *existing != body => {
                let path = u.string(&path);
                diagnostics.push(u.compound("fs_file_conflict", vec![path]));
            }
            _ => {
                out.insert(path, body);
            }
        }
    }
    match diagnostics.is_empty() {
        true => Ok(out),
        false => Err(diagnostics),
    }
}
