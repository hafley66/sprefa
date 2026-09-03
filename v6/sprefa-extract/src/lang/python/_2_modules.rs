//! @comment-ok: module header, the seam list every lang file opens with
//! The python module plane: import statements resolved against the supplied
//! file set only (never `sys.path`, never site-packages), so `import_facts`
//! writes `resolved_import` rows for python the way `ts_resolve.rs` does for
//! ts. A dedicated second parse, gated behind `--resolve` like
//! `go_modules.rs`: phase 1's `py_module_specifiers` only exists when the
//! CALL arm's mask asks for it and flattens the relative-import dots into
//! text.
//!
//! Module names come from the package walk PEP 420 skips: a file's module is
//! its path from the nearest ancestor directory WITHOUT an `__init__.py` in
//! the supplied set (`src/flask/app.py` with `src/flask/__init__.py` is
//! `flask.app`), a script beside no `__init__.py` is its own bare stem.
//! Relative imports walk directories from the importing file; absolute ones
//! go through the module index. A name asked of a module resolves local
//! declarations first, then the module's own `from x import name` re-exports
//! (kind=indirect), then a submodule of a package (kind=namespace), then its
//! `from x import *` arms (kind=star, ambiguous on disagreement, no row).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::lang::ts_resolve::{ImportRow, ResolvedImportKind};
use crate::shape::Span;

use super::_0_source::{node_span, py_parse, py_text};

// ── phase-2 facts: one dedicated parse per file ─────────────────────────────

/// One import statement's clause, the module spelled as written (relative
/// dots kept).
#[derive(Clone, Debug, PartialEq, Eq)]
enum PyImport {
    /// `import a.b [as c]`: `local` is `c`, else `a.b` itself.
    Module {
        span: Span,
        module: String,
        local: String,
    },
    /// `from m import n [as c]`: `local` is `c`, else `n`.
    Named {
        span: Span,
        module: String,
        name: String,
        local: String,
    },
    /// `from m import *`.
    Star { span: Span, module: String },
}

impl PyImport {
    fn module(&self) -> &str {
        match self {
            PyImport::Module { module, .. }
            | PyImport::Named { module, .. }
            | PyImport::Star { module, .. } => module,
        }
    }
}

/// One file's import clauses plus the names its module scope declares.
#[derive(Clone, Debug, Default)]
pub struct PyModuleFacts {
    imports: Vec<PyImport>,
    /// `def`, `class`, and assignment targets at module depth: the names a
    /// `from m import name` can bind without a further hop.
    top_level: HashSet<String>,
}

/// `None`: a non-python path, or a parse that fails.
pub fn py_module_facts(path: &str, content: &[u8]) -> Option<PyModuleFacts> {
    if !(path.ends_with(".py") || path.ends_with(".pyi")) {
        return None;
    }
    let text = std::str::from_utf8(content).ok()?;
    let tree = py_parse(text)?;
    let root = tree.root_node();
    let src = text.as_bytes();
    let mut facts = PyModuleFacts::default();
    walk_imports(root, src, &mut facts.imports);
    collect_top_level(root, src, &mut facts.top_level);
    Some(facts)
}

/// Every import clause at any depth (a function-local import binds the same
/// file edge), in source order.
fn walk_imports(node: tree_sitter::Node, src: &[u8], out: &mut Vec<PyImport>) {
    match node.kind() {
        "import_statement" => {
            let mut cursor = node.walk();
            for item in node.named_children(&mut cursor) {
                match item.kind() {
                    "dotted_name" => {
                        let module = py_text(item, src).to_string();
                        out.push(PyImport::Module {
                            span: node_span(item),
                            local: module.clone(),
                            module,
                        });
                    }
                    "aliased_import" => {
                        let Some(module) = item.child_by_field_name("name") else {
                            continue;
                        };
                        let module = py_text(module, src).to_string();
                        let local = item.child_by_field_name("alias").map_or_else(
                            || module.clone(),
                            |alias| py_text(alias, src).to_string(),
                        );
                        out.push(PyImport::Module {
                            span: node_span(item),
                            module,
                            local,
                        });
                    }
                    _ => {}
                }
            }
        }
        "import_from_statement" => {
            let module = node
                .child_by_field_name("module_name")
                .map(|module| py_text(module, src).to_string())
                .unwrap_or_default();
            let mut cursor = node.walk();
            let mut saw_name = false;
            for item in node.children_by_field_name("name", &mut cursor) {
                saw_name = true;
                let (name, local) = match item.kind() {
                    "dotted_name" => {
                        let name = py_text(item, src).to_string();
                        (name.clone(), name)
                    }
                    "aliased_import" => {
                        let Some(name) = item.child_by_field_name("name") else {
                            continue;
                        };
                        let name = py_text(name, src).to_string();
                        let local = item
                            .child_by_field_name("alias")
                            .map_or_else(|| name.clone(), |alias| py_text(alias, src).to_string());
                        (name, local)
                    }
                    _ => continue,
                };
                out.push(PyImport::Named {
                    span: node_span(item),
                    module: module.clone(),
                    name,
                    local,
                });
            }
            if !saw_name {
                out.push(PyImport::Star {
                    span: node_span(node),
                    module,
                });
            }
        }
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_imports(child, src, out);
    }
}

/// The names bound at module depth: `def`/`class` (decorated or not) and the
/// identifier targets of assignments, `if`/`try` bodies included.
fn collect_top_level(node: tree_sitter::Node, src: &[u8], out: &mut HashSet<String>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "function_definition" | "class_definition" => {
                if let Some(name) = child.child_by_field_name("name") {
                    out.insert(py_text(name, src).to_string());
                }
            }
            "decorated_definition" => {
                if let Some(definition) = child.child_by_field_name("definition") {
                    if let Some(name) = definition.child_by_field_name("name") {
                        out.insert(py_text(name, src).to_string());
                    }
                }
            }
            "expression_statement" => {
                let mut items = child.walk();
                for item in child.named_children(&mut items) {
                    if matches!(item.kind(), "assignment" | "augmented_assignment") {
                        if let Some(left) = item.child_by_field_name("left") {
                            collect_assignment_targets(left, src, out);
                        }
                    }
                }
            }
            "if_statement" | "try_statement" | "else_clause" | "elif_clause" | "except_clause"
            | "finally_clause" | "block" => collect_top_level(child, src, out),
            _ => {}
        }
    }
}

fn collect_assignment_targets(node: tree_sitter::Node, src: &[u8], out: &mut HashSet<String>) {
    match node.kind() {
        "identifier" => {
            out.insert(py_text(node, src).to_string());
        }
        "pattern_list" | "tuple_pattern" | "list_pattern" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                collect_assignment_targets(child, src, out);
            }
        }
        _ => {}
    }
}

// ── the module plane proper ──────────────────────────────────────────────────

fn is_init(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .is_some_and(|name| name == "__init__.py" || name == "__init__.pyi")
}

fn module_stem(path: &str) -> Option<String> {
    let name = Path::new(path).file_name()?.to_str()?;
    let stem = name
        .strip_suffix(".pyi")
        .or_else(|| name.strip_suffix(".py"))?;
    Some(stem.to_string())
}

fn join(dir: &Path, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

/// One name asked of one module, answered.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PyFound {
    target_path: String,
    target_name: Option<String>,
    kind: ResolvedImportKind,
    hops: u32,
}

/// THE corpus python module plane, built ONCE per refresh in `resolve_project`.
#[derive(Default)]
pub struct PyModuleIndex {
    facts: HashMap<String, PyModuleFacts>,
    /// Every supplied python path, the set relative and package resolution
    /// probe against.
    files: HashSet<String>,
    /// Absolute module name -> the files spelling it; a name two roots both
    /// spell is ambiguous and binds nothing.
    module_files: HashMap<String, Vec<String>>,
}

impl PyModuleIndex {
    /// `files` is every python input's facts.
    pub fn build(files: Vec<(String, PyModuleFacts)>) -> PyModuleIndex {
        let mut index = PyModuleIndex::default();
        index.files = files.iter().map(|(path, _)| path.clone()).collect();
        for (path, _) in &files {
            if let Some(module) = index.module_name(path) {
                index
                    .module_files
                    .entry(module)
                    .or_default()
                    .push(path.clone());
            }
        }
        for paths in index.module_files.values_mut() {
            paths.sort();
        }
        index.facts = files.into_iter().collect();
        index
    }

    /// `path`'s absolute module name: the package walk up to the first
    /// ancestor directory with no `__init__.py` in the supplied set.
    fn module_name(&self, path: &str) -> Option<String> {
        let mut segments: Vec<String> = Vec::new();
        if !is_init(path) {
            segments.push(module_stem(path)?);
        }
        let mut dir = Path::new(path).parent()?.to_path_buf();
        while self.has_init(&dir) {
            let name = dir.file_name()?.to_str()?.to_string();
            segments.push(name);
            dir = dir.parent()?.to_path_buf();
        }
        if segments.is_empty() {
            return None;
        }
        segments.reverse();
        Some(segments.join("."))
    }

    fn has_init(&self, dir: &Path) -> bool {
        self.files.contains(&join(dir, "__init__.py"))
            || self.files.contains(&join(dir, "__init__.pyi"))
    }

    /// The file a dotted module name spells, `.py` preferred over `.pyi`,
    /// `None` when no supplied file or more than one spells it.
    fn absolute_module_file(&self, module: &str) -> Option<&str> {
        let paths = self.module_files.get(module)?;
        if paths.len() == 1 {
            return paths.first().map(String::as_str);
        }
        let mut sources = paths.iter().filter(|path| path.ends_with(".py"));
        let first = sources.next()?;
        sources.next().is_none().then_some(first.as_str())
    }

    /// The file inside `dir` a bare module segment names: the package's
    /// `__init__.py` first, else the module file.
    fn file_in_dir(&self, dir: &Path, segment: &str) -> Option<String> {
        let package = dir.join(segment);
        [
            join(&package, "__init__.py"),
            join(&package, "__init__.pyi"),
            join(dir, &format!("{segment}.py")),
            join(dir, &format!("{segment}.pyi")),
        ]
        .into_iter()
        .find(|candidate| self.files.contains(candidate))
    }

    /// The corpus file a module spelled in `importer` names, relative dots
    /// walked from the importer's own package directory.
    fn module_file(&self, importer: &str, module: &str) -> Option<String> {
        let dots = module.chars().take_while(|ch| *ch == '.').count();
        if dots == 0 {
            return self.absolute_module_file(module).map(str::to_string);
        }
        let mut dir: PathBuf = Path::new(importer).parent()?.to_path_buf();
        for _ in 1..dots {
            dir = dir.parent()?.to_path_buf();
        }
        let rest = &module[dots..];
        if rest.is_empty() {
            return [join(&dir, "__init__.py"), join(&dir, "__init__.pyi")]
                .into_iter()
                .find(|candidate| self.files.contains(candidate));
        }
        let mut segments = rest.split('.');
        let mut file = self.file_in_dir(&dir, segments.next()?)?;
        for segment in segments {
            if !is_init(&file) {
                return None;
            }
            let package_dir = Path::new(&file).parent()?;
            file = self.file_in_dir(package_dir, segment)?;
        }
        Some(file)
    }

    /// `name` asked of the module at `path`. `stack` is the re-export chain
    /// open above this call, so a cycle answers nothing instead of looping.
    fn resolve_name(&self, path: &str, name: &str, stack: &mut Vec<String>) -> Option<PyFound> {
        if stack.iter().any(|open| open == path) {
            return None;
        }
        let facts = self.facts.get(path)?;
        if facts.top_level.contains(name) {
            return Some(PyFound {
                target_path: path.to_string(),
                target_name: Some(name.to_string()),
                kind: ResolvedImportKind::Local,
                hops: 1,
            });
        }
        stack.push(path.to_string());
        let found = self.resolve_through_imports(path, facts, name, stack);
        stack.pop();
        found
    }

    fn resolve_through_imports(
        &self,
        path: &str,
        facts: &PyModuleFacts,
        name: &str,
        stack: &mut Vec<String>,
    ) -> Option<PyFound> {
        for import in &facts.imports {
            match import {
                PyImport::Named {
                    module,
                    name: imported,
                    local,
                    ..
                } if local == name => {
                    let target = self.module_file(path, module)?;
                    let found = self.resolve_member(&target, imported, stack)?;
                    return Some(PyFound {
                        kind: promote(found.kind, ResolvedImportKind::Indirect),
                        hops: found.hops + 1,
                        ..found
                    });
                }
                PyImport::Module { module, local, .. } if local == name => {
                    let target = self.module_file(path, module)?;
                    return Some(PyFound {
                        target_path: target,
                        target_name: None,
                        kind: ResolvedImportKind::Namespace,
                        hops: 2,
                    });
                }
                _ => {}
            }
        }
        if let Some(submodule) = self.submodule_file(path, name) {
            return Some(PyFound {
                target_path: submodule,
                target_name: None,
                kind: ResolvedImportKind::Namespace,
                hops: 1,
            });
        }
        let mut found: Option<PyFound> = None;
        for import in &facts.imports {
            let PyImport::Star { module, .. } = import else {
                continue;
            };
            let Some(target) = self.module_file(path, module) else {
                continue;
            };
            let Some(hit) = self.resolve_name(&target, name, stack) else {
                continue;
            };
            let hit = PyFound {
                kind: promote(hit.kind, ResolvedImportKind::Star),
                hops: hit.hops + 1,
                ..hit
            };
            match &found {
                None => found = Some(hit),
                Some(existing) if existing.target_path != hit.target_path => return None,
                _ => {}
            }
        }
        found
    }

    /// `name` asked of module `path` by a `from path import name` clause: a
    /// declared name, else the package's submodule of that name.
    fn resolve_member(&self, path: &str, name: &str, stack: &mut Vec<String>) -> Option<PyFound> {
        self.resolve_name(path, name, stack).or_else(|| {
            self.submodule_file(path, name).map(|submodule| PyFound {
                target_path: submodule,
                target_name: None,
                kind: ResolvedImportKind::Namespace,
                hops: 1,
            })
        })
    }

    /// The submodule `name` of the package whose `__init__.py` is `path`.
    fn submodule_file(&self, path: &str, name: &str) -> Option<String> {
        if !is_init(path) {
            return None;
        }
        self.file_in_dir(Path::new(path).parent()?, name)
    }

    /// Every import clause `path` writes: one `module` row per clause whose
    /// module names a corpus file, plus one binding row when a name binds.
    pub fn bindings(&self, path: &str) -> Vec<ImportRow> {
        let Some(facts) = self.facts.get(path) else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        let mut file_edges: HashSet<(Span, String)> = HashSet::new();
        for import in &facts.imports {
            let Some(target) = self.module_file(path, import.module()) else {
                continue;
            };
            let span = match import {
                PyImport::Module { span, .. }
                | PyImport::Named { span, .. }
                | PyImport::Star { span, .. } => *span,
            };
            if file_edges.insert((span, target.clone())) {
                rows.push(ImportRow {
                    local: String::new(),
                    name: import.module().to_string(),
                    target_path: target.clone(),
                    target_name: None,
                    kind: ResolvedImportKind::Module,
                    hops: 1,
                });
            }
            match import {
                PyImport::Module { local, .. } => rows.push(ImportRow {
                    local: local.clone(),
                    name: "*".to_string(),
                    target_path: target,
                    target_name: None,
                    kind: ResolvedImportKind::Namespace,
                    hops: 1,
                }),
                PyImport::Named { name, local, .. } => {
                    if let Some(found) = self.resolve_member(&target, name, &mut Vec::new()) {
                        rows.push(ImportRow {
                            local: local.clone(),
                            name: name.clone(),
                            target_path: found.target_path,
                            target_name: found.target_name,
                            kind: found.kind,
                            hops: found.hops,
                        });
                    }
                }
                PyImport::Star { .. } => rows.push(ImportRow {
                    local: "*".to_string(),
                    name: "*".to_string(),
                    target_path: target,
                    target_name: None,
                    kind: ResolvedImportKind::Star,
                    hops: 1,
                }),
            }
        }
        rows
    }
}

/// The chain's kind after one more hop, `ts_resolve.rs`'s precedence:
/// namespace > star > indirect > default > local.
fn promote(inner: ResolvedImportKind, hop: ResolvedImportKind) -> ResolvedImportKind {
    use ResolvedImportKind::{Indirect, Local, Namespace, Star};
    match (inner, hop) {
        (Namespace, _) | (_, Namespace) => Namespace,
        (Star, _) | (_, Star) => Star,
        (Indirect, _) | (_, Indirect) => Indirect,
        (Local, other) => other,
        (other, _) => other,
    }
}
