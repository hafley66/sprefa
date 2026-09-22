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
            let (canonical, text) =
                program_text(path).map_err(|e| format!("{}: {e}", path.display()))?;
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

/// `import(ImporterOrigin, Name, ImportedOrigin)`.
fn import_row(u: &mut Universe, importer: TermId, name: TermId, imported: TermId) -> TermId {
    u.compound("import", vec![importer, name, imported])
}

/// Every unit the roots reach. A path already read is bound again and never
/// read twice, so a cycle terminates here and stays the prelude's question.
pub fn load_import_chain(u: &mut Universe, cwd: &Path, roots: &[TermId]) -> Result<Imported, Stop> {
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

/// `import(Origin, Local, std(Module))` read backwards: the unit that bound
/// `@std/<module>` under `Local`.
fn std_importer(u: &Universe, rows: &[TermId], local: TermId, module: &str) -> Option<TermId> {
    rows.iter().find_map(|row| {
        let [importer, name, imported] = u.args::<3>(*row, "import")?;
        let atom = u.unary(imported, "std")?;
        (name == local && u.functor_or_atom(atom).map(|(n, _)| n) == Some(module))
            .then_some(importer)
    })
}

/// The seed list of every lowered basement, in basement order.
fn basement_seeds(u: &Universe, basements: &[TermId]) -> Vec<TermId> {
    let mut out = Vec::new();
    for row in basements {
        let Some([_, program]) = u.args::<2>(*row, "module_basement") else {
            continue;
        };
        let Some([_, datalog]) = u.args::<2>(program, "basement_program") else {
            continue;
        };
        let Some([_, seeds, _]) = u.args::<3>(datalog, "datalog_program") else {
            continue;
        };
        out.extend(u.as_list(seeds).unwrap_or_default());
    }
    out
}

/// A lowered seed `call(name(name(module(Origin), Local), Member), Arguments)`:
/// the dot walk `lower_path_call` writes for `(<local>.<member> ...)`.
fn member_seed(u: &Universe, seed: TermId, member: &str) -> Option<(TermId, TermId, TermId)> {
    let [callable, arguments] = u.args::<2>(seed, "call")?;
    let [scope, label] = u.args::<2>(callable, "name")?;
    if u.functor_or_atom(label).map(|(n, _)| n) != Some(member) {
        return None;
    }
    let [owner, local] = u.args::<2>(scope, "name")?;
    let origin = u.unary(owner, "module")?;
    let first = u.as_list(arguments)?.into_iter().next()?;
    Some((origin, local, first))
}

/// Every document an `@std/oai` member seed names, against the importing
/// unit's directory. Before the fixpoint only a `const` argument is a value.
pub fn oai_document_paths(u: &Universe, basements: &[TermId], imports: &[TermId]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for seed in basement_seeds(u, basements) {
        let Some((origin, local, argument)) = member_seed(u, seed, "document") else {
            continue;
        };
        if std_importer(u, imports, local, "oai") != Some(origin) {
            continue;
        }
        let Some(spec) = u.unary(argument, "const").and_then(|v| string_of(u, v)) else {
            continue;
        };
        let Some(path) = u.unary(origin, "file").and_then(|f| u.functor_or_atom(f)) else {
            continue;
        };
        if let Some(directory) = Path::new(path.0).parent() {
            let resolved = directory.join(spec);
            tracing::debug!(
                target: "dl8::io",
                seed = %crate::_6_eval::json::term_to_json(u, seed),
                path = %resolved.display(),
            );
            out.push(resolved);
        }
    }
    out
}
