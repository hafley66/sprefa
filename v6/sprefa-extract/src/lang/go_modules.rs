//! @comment-ok: module header, the seam list every lang file opens with
//! The go module plane: the package's own name resolution (import specs +
//! exported identifiers), run once per file set, so `Resolve<CallF>` /
//! `Resolve<TypeF>` bind a `pkg.Name` selector through the target package's
//! REAL name rather than a corpus-wide name guess. Go has no `use`/`mod`
//! chain and no re-export: one flat namespace per package DIRECTORY, and
//! exportedness is capitalization, never a keyword — the plane is a
//! directory index, not a resolve chain (mirrors `rust_modules.rs`'s
//! discipline, Go's own simpler spec).
//!
//! A dedicated second parse, gated behind `--resolve`: phase 1's
//! `CallFAux.specifiers` (`go.rs` `go_module_specifiers`) only exists when
//! the CALL arm's mask requests it, and its qualifier map
//! (`go_import_bindings`) guesses the local binding from the import path's
//! LAST SEGMENT — wrong whenever the target package's own name differs
//! (`import "gopkg.in/yaml.v3"` binds `yaml`, not `yaml.v3`). This file
//! re-parses the package clause + import specs independent of any arm mask,
//! and the corpus-wide directory index gives the REAL package name a
//! qualifier binds.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::family::SpecifierKind;
use crate::seams::{corpus_defs, DefIndex, DefSite};
use crate::shape::{ContentId, Span, Strings};
use crate::types::PathIndex;

use super::go::{
    go_file_facts_of_source, go_is_method_def, go_module_of, go_node_span, go_package_dir,
    go_parse_shared, go_text, go_walk_import_specs, same_dir, unique_blob, GoFileFacts,
};

// ── phase-2 facts: one dedicated parse per file ─────────────────────────────

/// One import spec, `Specifier`'s NameIds resolved to owned text (the
/// dedicated `Strings` arena is gone by the time the plane builds).
#[derive(Clone, Debug, PartialEq, Eq)]
struct GoSpecifier {
    span: Span,
    kind: SpecifierKind,
    /// The alias/path text as written (the table at `go.rs`'s
    /// `go_module_specifiers`): the path itself for a path-only form.
    name: String,
    /// `Some(real path)` only for the aliased form, mirroring `Specifier`.
    module: Option<String>,
}

/// One file's package clause + import specs, off the SHARED parse (the same
/// tree `dispatch` produced for the extract pass, handed over through
/// `go_parse_shared`); phase 1's CallF bundle (and its mask gate) is gone by
/// the time the plane builds. The same parse also collects the file's
/// method/interface facts (`go.rs`'s `GoFileFacts`) for the resolve arms.
#[derive(Clone, Default)]
pub struct GoModuleFacts {
    package_name: Option<String>,
    specifiers: Vec<GoSpecifier>,
    /// This file's `type` declaration spans. The corpus `DefIndex` carries no
    /// entity kind, and a go method shares the name index with its type.
    type_decls: HashSet<Span>,
    /// The resolve arms' per-file facts, collected in this one pass.
    file_facts: Option<std::sync::Arc<GoFileFacts>>,
}

impl std::fmt::Debug for GoModuleFacts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GoModuleFacts")
            .field("package_name", &self.package_name)
            .field("specifiers", &self.specifiers)
            .field("type_decls", &self.type_decls)
            .finish_non_exhaustive()
    }
}

impl GoModuleFacts {
    /// The resolve arms' per-file facts, when this file contributed them.
    pub(crate) fn file_facts(&self) -> Option<&std::sync::Arc<GoFileFacts>> {
        self.file_facts.as_ref()
    }
}

/// `None`: a non-`.go` path, or a parse that fails.
pub fn go_module_facts(path: &str, content: &[u8]) -> Option<GoModuleFacts> {
    if !path.ends_with(".go") {
        return None;
    }
    let text = std::str::from_utf8(content).ok()?;
    let tree = go_parse_shared(text)?;
    let root = tree.root_node();
    let src = text.as_bytes();
    let package_name = package_clause_name(root, src);
    let mut strings = Strings::new();
    let mut raw = Vec::new();
    go_walk_import_specs(root, src, &mut strings, &mut raw);
    let specifiers = raw
        .into_iter()
        .map(|spec| GoSpecifier {
            span: spec.span,
            kind: spec.kind,
            name: strings.lookup(spec.name).to_string(),
            module: spec.module.map(|id| strings.lookup(id).to_string()),
        })
        .collect();
    let mut type_decls = HashSet::new();
    walk_type_decls(root, &mut type_decls);
    let file_facts = std::sync::Arc::new(go_file_facts_of_source(&tree, text));
    Some(GoModuleFacts {
        package_name,
        specifiers,
        type_decls,
        file_facts: Some(file_facts),
    })
}

/// Every `type` declaration's entity span, at any nesting depth. The recursion
/// and the `type_spec`/`type_alias` span mirror `go.rs`'s `walk_go_entities`.
fn walk_type_decls(node: tree_sitter::Node, out: &mut HashSet<Span>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "type_declaration" {
            let mut specs = child.walk();
            for spec in child.children(&mut specs) {
                if matches!(spec.kind(), "type_spec" | "type_alias")
                    && spec.child_by_field_name("name").is_some()
                {
                    out.insert(go_node_span(spec));
                }
            }
        }
        walk_type_decls(child, out);
    }
}

/// `package_clause`'s one child, no field name in tree-sitter-go's grammar.
fn package_clause_name(root: tree_sitter::Node, src: &[u8]) -> Option<String> {
    let mut cursor = root.walk();
    let clause = root
        .children(&mut cursor)
        .find(|node| node.kind() == "package_clause")?;
    Some(go_text(clause.named_child(0)?, src).to_string())
}

fn import_path_of(spec: &GoSpecifier) -> &str {
    spec.module.as_deref().unwrap_or(&spec.name)
}

/// Go's own export rule: visible outside its package iff the first rune of a
/// top-level name is upper-case. `pub(crate)`: `go.rs`'s import-qualified
/// fallback leg gates on the same rule.
pub(crate) fn is_exported(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

/// A `_test.go` external test package shares its directory with the primary
/// package it tests; importers always bind the primary one.
fn is_test_package(name: &str) -> bool {
    name.ends_with("_test")
}

/// The best package-name guess for an import path with no corpus file in this
/// invocation: the path's last segment (phase 1's own guess).
fn import_path_tail(import_path: &str) -> &str {
    import_path.rsplit('/').next().unwrap_or(import_path)
}

// ── the module plane proper ──────────────────────────────────────────────────

/// Go has no re-export chain and no default export: one hop, direct or
/// namespace.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GoImportKind {
    Local,
    Namespace,
}

impl GoImportKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            GoImportKind::Local => "local",
            GoImportKind::Namespace => "namespace",
        }
    }
}

/// The `resolved_import` wire row. A blank (`_`) import binds nothing (no
/// row); an external one is `GoModuleIndex::external_drops`'s.
pub struct GoImportRow {
    pub local: String,
    pub name: String,
    pub target_path: String,
    pub target_name: Option<String>,
    pub kind: GoImportKind,
}

fn dir_key(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|dir| dir.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// THE corpus go module plane, built ONCE per refresh in `resolve_project`.
#[derive(Default)]
pub struct GoModuleIndex {
    facts: HashMap<String, GoModuleFacts>,
    // directory -> its declared package name; first file supplied wins.
    package_of_dir: HashMap<String, String>,
    // file -> (directory, package clause): a per-site package-membership
    // lookup is one HashMap get, never a same_dir component walk.
    package_of_file: HashMap<String, (String, String)>,
}

impl GoModuleIndex {
    /// `files` is every `.go` input's facts.
    pub fn build(files: Vec<(String, GoModuleFacts)>) -> GoModuleIndex {
        let mut index = GoModuleIndex::default();
        for (path, facts) in &files {
            if let Some(name) = &facts.package_name {
                let key = dir_key(path);
                // The primary package wins the directory: an external test
                // package (`foo_test`) never binds an importer, whatever order
                // the directory's files arrive in.
                match index.package_of_dir.get(&key) {
                    None => {
                        index.package_of_dir.insert(key.clone(), name.clone());
                    }
                    Some(existing) if is_test_package(existing) && !is_test_package(name) => {
                        index.package_of_dir.insert(key.clone(), name.clone());
                    }
                    _ => {}
                }
                index
                    .package_of_file
                    .insert(path.clone(), (key, name.clone()));
            }
        }
        index.facts = files.into_iter().collect();
        index
    }

    /// The package name declared at `dir`, if any corpus file sits there.
    pub fn package_name(&self, dir: &Path) -> Option<&str> {
        self.package_of_dir
            .get(&dir.to_string_lossy().into_owned())
            .map(String::as_str)
    }

    /// Every import spec `file` writes, resolved to a `resolved_import` row.
    pub fn bindings(&self, file: &str) -> Vec<GoImportRow> {
        let Some(facts) = self.facts.get(file) else {
            return Vec::new();
        };
        let Some(module) = go_module_of(file) else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for spec in &facts.specifiers {
            if spec.kind == SpecifierKind::SideEffect {
                continue;
            }
            let import_path = import_path_of(spec);
            // The directory an in-module import names exists whether or not
            // its files share this invocation; the row's name falls back to
            // the path's last segment when no corpus file can declare it.
            let Some(dir) = go_package_dir(&module, import_path) else {
                continue;
            };
            let pkg_name = self
                .package_name(&dir)
                .map(str::to_string)
                .unwrap_or_else(|| import_path_tail(import_path).to_string());
            let (local, kind) = match spec.kind {
                SpecifierKind::Namespace => (".".to_string(), GoImportKind::Namespace),
                // aliased: `name` carries the alias text; path-only: `name`
                // carries the path itself, so the REAL package name binds.
                _ if spec.module.is_some() => (spec.name.clone(), GoImportKind::Local),
                _ => (pkg_name.to_string(), GoImportKind::Local),
            };
            rows.push(GoImportRow {
                local,
                name: "*".to_string(),
                target_path: dir.to_string_lossy().into_owned(),
                target_name: Some(pkg_name.to_string()),
                kind,
            });
        }
        rows
    }

    /// `(span, import path)` per non-blank spec whose import path names no
    /// directory inside the file's module (stdlib or third-party): the
    /// `unresolved` reason `external` leg.
    pub fn external_drops(&self, file: &str) -> Vec<(Span, String)> {
        let Some(facts) = self.facts.get(file) else {
            return Vec::new();
        };
        let module = go_module_of(file);
        facts
            .specifiers
            .iter()
            .filter(|spec| spec.kind != SpecifierKind::SideEffect)
            .filter_map(|spec| {
                let import_path = import_path_of(spec);
                let resolved = module
                    .as_ref()
                    .is_some_and(|module| go_package_dir(module, import_path).is_some());
                (!resolved).then(|| (spec.span, import_path.to_string()))
            })
            .collect()
    }

    /// Is the entity at `(path, span)` a `type` declaration? A path with no
    /// facts (no `.go` file supplied it) answers false.
    pub fn is_type_decl(&self, path: &str, span: Span) -> bool {
        self.facts
            .get(path)
            .is_some_and(|facts| facts.type_decls.contains(&span))
    }

    /// `dir`'s exported decl named `name`, joined through the `DefIndex`.
    pub fn resolve_in_dir(
        &self,
        dir: &Path,
        def_index: &DefIndex,
        paths: &PathIndex,
        name: &str,
    ) -> Option<(ContentId, Span)> {
        if !is_exported(name) {
            return None;
        }
        unique_blob(&self.free_sites_in_dir(dir, def_index, paths, name))
    }

    /// The bare-name leg through the referring file's OWN package directory,
    /// which Go's package block resolves before any corpus-wide name guess.
    pub fn resolve_call_in_own_dir(
        &self,
        dir: &Path,
        def_index: &DefIndex,
        paths: &PathIndex,
        name: &str,
    ) -> Option<(ContentId, Span)> {
        unique_blob(&self.free_sites_in_dir(dir, def_index, paths, name))
    }

    /// The corpus-unique tier scoped to `file`'s own package: the directory's
    /// files declaring the same package clause, not the dir alone.
    pub fn free_sites_in_own_package<'a>(
        &self,
        file: &str,
        def_index: &'a DefIndex,
        paths: &PathIndex,
        name: &str,
    ) -> Vec<&'a DefSite> {
        let Some(own) = self.package_of_file.get(file) else {
            return Vec::new();
        };
        corpus_defs(def_index, name)
            .iter()
            .filter(|site| {
                paths.get(&site.blob).is_some_and(|site_path| {
                    self.package_of_file
                        .get(site_path)
                        .is_some_and(|site_pkg| site_pkg == own)
                        && !go_is_method_def(site_path, site.span)
                })
            })
            .collect()
    }

    /// `sites_in_dir` with method defs cut: a `pkg.F()` or bare `F()` call
    /// names a package-level func, never `func (r T) F()` sharing the name.
    fn free_sites_in_dir<'a>(
        &self,
        dir: &Path,
        def_index: &'a DefIndex,
        paths: &PathIndex,
        name: &str,
    ) -> Vec<&'a DefSite> {
        self.sites_in_dir(dir, def_index, paths, name)
            .into_iter()
            .filter(|site| {
                paths
                    .get(&site.blob)
                    .is_none_or(|path| !go_is_method_def(path, site.span))
            })
            .collect()
    }

    /// `resolve_in_dir` narrowed to `type` declarations, for the type arm's
    /// qualified `pkg.Type` leg.
    pub fn resolve_type_in_dir(
        &self,
        dir: &Path,
        def_index: &DefIndex,
        paths: &PathIndex,
        name: &str,
    ) -> Option<(ContentId, Span)> {
        if !is_exported(name) {
            return None;
        }
        self.type_decl_in_dir(dir, def_index, paths, name)
    }

    /// The referring file's OWN package directory. No export gate: a
    /// package-level name is visible to its own package whatever its case.
    pub fn resolve_type_in_own_dir(
        &self,
        dir: &Path,
        def_index: &DefIndex,
        paths: &PathIndex,
        name: &str,
    ) -> Option<(ContentId, Span)> {
        self.type_decl_in_dir(dir, def_index, paths, name)
    }

    fn type_decl_in_dir(
        &self,
        dir: &Path,
        def_index: &DefIndex,
        paths: &PathIndex,
        name: &str,
    ) -> Option<(ContentId, Span)> {
        let sites: Vec<&DefSite> = self
            .sites_in_dir(dir, def_index, paths, name)
            .into_iter()
            .filter(|site| {
                paths
                    .get(&site.blob)
                    .is_some_and(|path| self.is_type_decl(path, site.span))
            })
            .collect();
        unique_blob(&sites)
    }

    fn sites_in_dir<'a>(
        &self,
        dir: &Path,
        def_index: &'a DefIndex,
        paths: &PathIndex,
        name: &str,
    ) -> Vec<&'a DefSite> {
        corpus_defs(def_index, name)
            .iter()
            .filter(|site| {
                paths
                    .get(&site.blob)
                    .and_then(|path| Path::new(path).parent())
                    .is_some_and(|parent| same_dir(parent, dir))
            })
            .collect()
    }

    /// `qualifier`'s import path: an alias matches literally, else it must
    /// equal the target directory's REAL package name, never the last segment.
    pub fn import_path_for(&self, file: &str, qualifier: &str) -> Option<String> {
        let facts = self.facts.get(file)?;
        let module = go_module_of(file)?;
        for spec in &facts.specifiers {
            if spec.kind != SpecifierKind::Named {
                continue;
            }
            let import_path = import_path_of(spec);
            let matches = if spec.module.is_some() {
                spec.name == qualifier
            } else {
                go_package_dir(&module, import_path)
                    .and_then(|dir| self.package_name(&dir).map(str::to_string))
                    .is_some_and(|name| name == qualifier)
            };
            if matches {
                return Some(import_path.to_string());
            }
        }
        None
    }

    /// The bare-name leg through `file`'s dot imports, tried only AFTER
    /// same-file/corpus-unique matching declines (so a local decl shadows it).
    pub fn resolve_dot_imported(
        &self,
        file: &str,
        def_index: &DefIndex,
        paths: &PathIndex,
        name: &str,
    ) -> Option<(ContentId, Span)> {
        let facts = self.facts.get(file)?;
        let module = go_module_of(file)?;
        let mut found: Option<(ContentId, Span)> = None;
        for spec in &facts.specifiers {
            if spec.kind != SpecifierKind::Namespace {
                continue;
            }
            let Some(dir) = go_package_dir(&module, import_path_of(spec)) else {
                continue;
            };
            let Some(hit) = self.resolve_in_dir(&dir, def_index, paths, name) else {
                continue;
            };
            match &found {
                None => found = Some(hit),
                Some(existing) if *existing != hit => return None,
                _ => {}
            }
        }
        found
    }
}
