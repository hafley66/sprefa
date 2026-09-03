//! @comment-ok: module header, the seam list every lang file opens with
//! The kotlin module plane: `import` headers resolved against the supplied
//! file set only, so `import_facts` writes `resolved_import` rows for kotlin
//! the way `ts_resolve.rs` does for ts. A dedicated second parse, gated
//! behind `--resolve` like `go_modules.rs`.
//!
//! A kotlin package maps to a directory by convention only, so the plane
//! indexes the supplied files' own `package` headers: `import a.b.C` binds
//! `C` to the file declaring a top-level class/object/fun/typealias/val of
//! that name in package `a.b` (kind=local); `import a.b.*` is one star row
//! per file declaring package `a.b`. Kotlin has no re-export, so no chain
//! and no `indirect` kind; a name two files of one package both declare is
//! ambiguous and binds nothing.

use std::collections::{BTreeSet, HashMap};

use crate::family::SpecifierKind;
use crate::lang::ts_resolve::{ImportRow, ResolvedImportKind};
use crate::shape::Strings;

use super::kotlin::{kt_child_kind, kt_first_child, kt_parse, kt_text, kt_walk_import_headers};

// ── phase-2 facts: one dedicated parse per file ─────────────────────────────

/// One `import` header, `Specifier`'s NameIds resolved to owned text.
#[derive(Clone, Debug, PartialEq, Eq)]
struct KtImport {
    /// The dotted path as written, `.*` stripped for a wildcard.
    path: String,
    /// The bound name: the alias, else the path's last segment.
    local: String,
    wildcard: bool,
}

/// One file's `package` header, import headers, and top-level declared
/// names.
#[derive(Clone, Debug, Default)]
pub struct KtModuleFacts {
    package: Option<String>,
    imports: Vec<KtImport>,
    top_level: BTreeSet<String>,
}

/// `None`: a non-kotlin path, or a parse that fails.
pub fn kt_module_facts(path: &str, content: &[u8]) -> Option<KtModuleFacts> {
    if !(path.ends_with(".kt") || path.ends_with(".kts")) {
        return None;
    }
    let text = std::str::from_utf8(content).ok()?;
    let tree = kt_parse(text)?;
    let root = tree.root_node();
    let src = text.as_bytes();
    let package = kt_child_kind(root, "package_header")
        .and_then(|header| kt_child_kind(header, "identifier"))
        .map(|identifier| kt_text(identifier, src).to_string());
    let mut strings = Strings::new();
    let mut raw = Vec::new();
    kt_walk_import_headers(root, src, &mut strings, &mut raw);
    let imports = raw
        .into_iter()
        .filter_map(|spec| {
            let path = strings.lookup(spec.module?).to_string();
            Some(KtImport {
                local: strings.lookup(spec.name).to_string(),
                path,
                wildcard: spec.kind == SpecifierKind::Namespace,
            })
        })
        .collect();
    let mut top_level = BTreeSet::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if let Some(name) = decl_name(child, src) {
            top_level.insert(name);
        }
    }
    Some(KtModuleFacts {
        package,
        imports,
        top_level,
    })
}

/// The name a top-level declaration binds, backticks stripped.
fn decl_name(node: tree_sitter::Node, src: &[u8]) -> Option<String> {
    let identifier = match node.kind() {
        "class_declaration" | "object_declaration" | "type_alias" => {
            kt_first_child(node, "type_identifier")?
        }
        "function_declaration" => kt_first_child(node, "simple_identifier")?,
        "property_declaration" => {
            let variable = kt_first_child(node, "variable_declaration")?;
            kt_first_child(variable, "simple_identifier")?
        }
        _ => return None,
    };
    Some(kt_text(identifier, src).trim_matches('`').to_string())
}

// ── the module plane proper ──────────────────────────────────────────────────

/// THE corpus kotlin module plane, built ONCE per refresh in `resolve_project`.
#[derive(Default)]
pub struct KtModuleIndex {
    facts: HashMap<String, KtModuleFacts>,
    /// package -> the files declaring it, sorted so a star import's rows are
    /// byte-stable whatever order the inputs arrive in.
    package_files: HashMap<String, Vec<String>>,
}

impl KtModuleIndex {
    /// `files` is every kotlin input's facts.
    pub fn build(files: Vec<(String, KtModuleFacts)>) -> KtModuleIndex {
        let mut index = KtModuleIndex::default();
        for (path, facts) in &files {
            if let Some(package) = &facts.package {
                index
                    .package_files
                    .entry(package.clone())
                    .or_default()
                    .push(path.clone());
            }
        }
        for paths in index.package_files.values_mut() {
            paths.sort();
        }
        index.facts = files.into_iter().collect();
        index
    }

    /// The one file in `package` declaring `name` at top level; two files
    /// declaring it is ambiguous.
    fn declaring_file(&self, package: &str, name: &str) -> Option<&str> {
        let mut hits = self.package_files.get(package)?.iter().filter(|path| {
            self.facts
                .get(*path)
                .is_some_and(|facts| facts.top_level.contains(name))
        });
        let first = hits.next()?;
        hits.next().is_none().then_some(first.as_str())
    }

    /// `path`'s longest package prefix a supplied file declares, and the
    /// top-level name the next segment spells (`a.b.Outer.Inner` binds `Outer`).
    fn split_import(&self, path: &str) -> Option<(String, String)> {
        let segments: Vec<&str> = path.split('.').collect();
        (1..segments.len()).rev().find_map(|split| {
            let package = segments[..split].join(".");
            self.package_files
                .contains_key(&package)
                .then(|| (package, segments[split].to_string()))
        })
    }

    /// Every import header `path` writes: one `module` row per header whose
    /// package a corpus file declares, plus one binding row when a name binds.
    pub fn bindings(&self, path: &str) -> Vec<ImportRow> {
        let Some(facts) = self.facts.get(path) else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for import in &facts.imports {
            if import.wildcard {
                let Some(files) = self.package_files.get(&import.path) else {
                    continue;
                };
                for target in files {
                    rows.push(ImportRow {
                        local: String::new(),
                        name: format!("{}.*", import.path),
                        target_path: target.clone(),
                        target_name: None,
                        kind: ResolvedImportKind::Module,
                        hops: 1,
                    });
                    rows.push(ImportRow {
                        local: "*".to_string(),
                        name: "*".to_string(),
                        target_path: target.clone(),
                        target_name: None,
                        kind: ResolvedImportKind::Star,
                        hops: 1,
                    });
                }
                continue;
            }
            let Some((package, name)) = self.split_import(&import.path) else {
                continue;
            };
            let Some(target) = self.declaring_file(&package, &name) else {
                continue;
            };
            rows.push(ImportRow {
                local: String::new(),
                name: import.path.clone(),
                target_path: target.to_string(),
                target_name: None,
                kind: ResolvedImportKind::Module,
                hops: 1,
            });
            rows.push(ImportRow {
                local: import.local.clone(),
                name: import.path[package.len() + 1..].to_string(),
                target_path: target.to_string(),
                target_name: Some(name),
                kind: ResolvedImportKind::Local,
                hops: 1,
            });
        }
        rows
    }
}
