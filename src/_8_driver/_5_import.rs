//! `@std/<name>` reads a bundled `STD` text; every other path resolves against
//! the compiler's working directory, with no search list and no shadowing.

use super::_0_read::{program_text, std_text, STD_PREFIX};
use super::_1_unit::text_unit;
use super::Stop;
use crate::_0_read::expand::Stop as ReadStop;
use crate::_2_lower::forms;
use crate::_6_eval::term::{Term, TermId, Universe};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// One `(<name>: (import "<path>"))` line.
pub struct Import {
    /// The local name the module binds to.
    pub name: TermId,
    pub path: String,
}

/// What an import path names.
pub enum Source {
    Std(String),
    File(PathBuf),
}

/// Units the import chain added, and one `import/3` row per binding.
#[derive(Default)]
pub struct Imported {
    pub units: Vec<TermId>,
    pub rows: Vec<TermId>,
    pub diagnostics: Vec<TermId>,
}

pub fn origin_of(u: &mut Universe, source: &Source) -> Result<(TermId, String), String> {
    match source {
        Source::Std(name) => {
            let atom = u.atom(name);
            let origin = u.compound("std", vec![atom]);
            let text = std_text(name).ok_or_else(|| format!("@std/{name} is not bundled"))?;
            Ok((origin, text.to_string()))
        }
        Source::File(path) => {
            let (canonical, text) = program_text(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let atom = u.atom(&canonical);
            Ok((u.compound("file", vec![atom]), text))
        }
    }
}

/// `@std/<name>`, else the path joined to the compiler's working directory.
pub fn resolve(cwd: &Path, path: &str) -> Source {
    match path.strip_prefix(STD_PREFIX) {
        Some(name) => Source::Std(name.to_string()),
        None => Source::File(cwd.join(path)),
    }
}

/// The reader path a bundled std text is read under.
fn std_reader_path(name: &str) -> String {
    format!("{STD_PREFIX}{name}")
}

fn string_of(u: &Universe, id: TermId) -> Option<String> {
    match u.get(id) {
        Term::Str(sym) => Some(u.sym_str(*sym).to_string()),
        _ => None,
    }
}

/// Every top-level `(: <atom> (import "<text>"))` of a unit, in source order.
pub fn unit_imports(u: &mut Universe, unit: TermId) -> Vec<Import> {
    let Some(parts) = u.args::<5>(unit, "dl7_unit") else {
        return Vec::new();
    };
    let forms = u.as_list(parts[2]).unwrap_or_default();
    let mut out = Vec::new();
    for item in forms {
        let Some(bind) = forms::bind_form(u, item) else {
            continue;
        };
        let Some(path) = import_call_path(u, bind.target) else {
            continue;
        };
        out.push(Import {
            name: bind.name,
            path,
        });
    }
    out
}

/// `(import "<text>")` as a call with one string argument.
fn import_call_path(u: &Universe, target: TermId) -> Option<String> {
    let node = forms::node(u, target)?;
    let items = forms::form(u, node.payload)?;
    if items.len() != 2 {
        return None;
    }
    let head = forms::form_head_atom(u, &items)?;
    if u.functor_or_atom(head).map(|(n, _)| n) != Some("import") {
        return None;
    }
    let argument = forms::node(u, items[1])?;
    string_of(u, forms::literal_value(u, argument.payload)?)
}

/// Every top-level `(<local>.<label> "<text>")` call of a unit, where `<local>`
/// is the name an import bound. The seed a loader reads its own input from.
pub fn unit_member_seeds(u: &mut Universe, unit: TermId, local: TermId, label: &str) -> Vec<String> {
    let Some(parts) = u.args::<5>(unit, "dl7_unit") else {
        return Vec::new();
    };
    let forms = u.as_list(parts[2]).unwrap_or_default();
    let mut out = Vec::new();
    for item in forms {
        let Some(text) = member_call_text(u, item, local, label) else {
            continue;
        };
        out.push(text);
    }
    out
}

fn member_call_text(u: &Universe, item: TermId, local: TermId, label: &str) -> Option<String> {
    let node = forms::node(u, item)?;
    let items = forms::form(u, node.payload)?;
    if items.len() != 2 {
        return None;
    }
    let segments = crate::_2_lower::express::path_segments(u, items[0])?;
    if segments.len() != 2 {
        return None;
    }
    if crate::_2_lower::express::path_segment_atom(u, segments[0])? != local {
        return None;
    }
    let member = crate::_2_lower::express::path_segment_atom(u, segments[1])?;
    if u.functor_or_atom(member).map(|(n, _)| n) != Some(label) {
        return None;
    }
    let argument = forms::node(u, items[1])?;
    string_of(u, forms::literal_value(u, argument.payload)?)
}

/// `import(ImporterOrigin, Name, ImportedOrigin)`.
fn import_row(u: &mut Universe, importer: TermId, name: TermId, imported: TermId) -> TermId {
    u.compound("import", vec![importer, name, imported])
}

/// Every unit the roots reach. A path already read is bound again and never
/// read twice, so a cycle terminates here and stays the prelude's question.
pub fn load_import_chain(
    u: &mut Universe,
    cwd: &Path,
    roots: &[TermId],
) -> Result<Imported, Stop> {
    let mut seen: HashSet<TermId> = HashSet::new();
    for unit in roots {
        if let Some(parts) = u.args::<5>(*unit, "dl7_unit") {
            seen.insert(parts[0]);
        }
    }
    let mut out = Imported::default();
    let mut queue: Vec<TermId> = roots.to_vec();
    while let Some(unit) = queue.pop() {
        let Some(parts) = u.args::<5>(unit, "dl7_unit") else {
            continue;
        };
        let importer = parts[0];
        for import in unit_imports(u, unit) {
            let source = resolve(cwd, &import.path);
            let (origin, text) = match origin_of(u, &source) {
                Ok(found) => found,
                Err(e) => return Err(Stop::Io(e)),
            };
            out.rows.push(import_row(u, importer, import.name, origin));
            if !seen.insert(origin) {
                continue;
            }
            let reader_path = match &source {
                Source::Std(name) => std_reader_path(name),
                Source::File(_) => match u.unary(origin, "file") {
                    Some(atom) => u.functor_or_atom(atom).map(|(n, _)| n.to_string()),
                    None => None,
                }
                .unwrap_or_else(|| import.path.clone()),
            };
            let read = text_unit(u, origin, &reader_path, &text).map_err(read_stop)?;
            out.diagnostics.extend(read.diagnostics);
            out.units.push(read.unit);
            queue.push(read.unit);
        }
    }
    Ok(out)
}

fn read_stop(e: ReadStop) -> Stop {
    Stop::Read(e)
}

/// The local name each unit bound `@std/<module>` to, if any.
fn std_local(u: &Universe, rows: &[TermId], importer: TermId, module: &str) -> Option<TermId> {
    rows.iter().find_map(|row| {
        let [row_importer, name, imported] = u.args::<3>(*row, "import")?;
        let atom = u.unary(imported, "std")?;
        (row_importer == importer && u.functor_or_atom(atom).map(|(n, _)| n) == Some(module))
            .then_some(name)
    })
}

/// Every `<local>.document "<path>"` line of a unit that imported `@std/oai`,
/// resolved against the importing file's own directory.
pub fn oai_document_paths(u: &mut Universe, units: &[TermId], rows: &[TermId]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for unit in units {
        let Some(parts) = u.args::<5>(*unit, "dl7_unit") else {
            continue;
        };
        let Some(local) = std_local(u, rows, parts[0], "oai") else {
            continue;
        };
        let Some(file) = u.unary(parts[0], "file") else {
            continue;
        };
        let Some((path, _)) = u.functor_or_atom(file) else {
            continue;
        };
        let directory = Path::new(path).parent().map(Path::to_path_buf);
        let Some(directory) = directory else {
            continue;
        };
        for spec in unit_member_seeds(u, *unit, local, "document") {
            out.push(directory.join(spec));
        }
    }
    out
}
