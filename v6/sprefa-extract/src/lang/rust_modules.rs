//! @comment-ok: module header, the seam list every lang file opens with
//! The Rust module plane: the language's OWN name resolution (`use`, `pub
//! use`, `mod`), run once per file set, so `Resolve<CallF>`/`Resolve<TypeF>`
//! bind an imported name through it instead of a corpus-wide name guess.
//! Mirrors `ts_resolve.rs`'s ECMAScript ResolveExport plane.
//!
//! A dedicated second parse, gated behind `--resolve` like ts's
//! `module_facts`: phase 1's flat `Specifier` rows (`rust.rs:1642`) collapse
//! `use a::b::{self}` and `use a::b::*` onto one (name, module,
//! kind=Reexport) shape, and an inline `mod x { .. }` mints NO row at all
//! (`rust.rs:1690`, documented `NO ROW`). This file re-walks the AST once and
//! reuses the module-path text math already landed for kink 4
//! (`module_segments` / `module_target` / `crate_root_of` / `mod_path_attr`,
//! `rust.rs`) instead of duplicating it.

use std::collections::{BTreeSet, HashMap};
use std::sync::Mutex;

use crate::seams::DefIndex;
use crate::shape::{ContentId, FamilyTag, Span, ZERO_CONTENT_ID};

use super::rust::{build_line_starts, mod_path_attr, module_segments, module_target};
use super::rust_receivers::{impl_facts, ImplEntry};

// ── phase-2 facts: one dedicated parse per file ──────────────────────────────

/// What one `use` leaf binds a local name to. `qualifier` is the source
/// module's path as written (`crate`/`self`/`super` kept literal).
#[derive(Clone, Debug, PartialEq, Eq)]
struct UseBinding {
    local: String,
    qualifier: Vec<String>,
    asked: String,
    reexport: bool,
}

/// A bare `use a::b::*;` / `pub use a::b::*;`: no local name, only a star hop
/// candidate for names the qualifier module's own resolve asks.
#[derive(Clone, Debug, PartialEq, Eq)]
struct StarImport {
    qualifier: Vec<String>,
    reexport: bool,
}

/// One file's `use`/`mod` facts off a dedicated parse; phase 1's syn arena is
/// gone by the time the module plane builds.
#[derive(Clone, Debug, Default)]
pub struct RustModuleFacts {
    uses: Vec<UseBinding>,
    stars: Vec<StarImport>,
    /// `mod x { .. }`: no file of its own, so `use x::f` binds in this blob.
    inline_mods: BTreeSet<String>,
    /// `mod x;` / `#[path = "y.rs"] mod x;`: name plus the path literal.
    mod_decls: Vec<(String, Option<String>)>,
    /// Every impl block's (self type, fn name, fn def span), for the corpus
    /// receiver leg's (T, m) table.
    impls: Vec<ImplEntry>,
}

/// `None` for a non-`.rs` path or a parse that fails: the plane then simply
/// carries no facts for that file.
pub fn rust_module_facts(path: &str, content: &[u8]) -> Option<RustModuleFacts> {
    if !path.ends_with(".rs") {
        return None;
    }
    let text = std::str::from_utf8(content).ok()?;
    let parsed = syn::parse_file(text).ok()?;
    let mut facts = RustModuleFacts::default();
    let line_starts = build_line_starts(text);
    facts.impls = impl_facts(&parsed, &line_starts);
    collect(&parsed.items, &mut facts);
    Some(facts)
}

fn collect(items: &[syn::Item], facts: &mut RustModuleFacts) {
    for item in items {
        match item {
            syn::Item::Use(use_item) => {
                let reexport = !matches!(use_item.vis, syn::Visibility::Inherited);
                let mut prefix = Vec::new();
                walk_use_tree(&use_item.tree, reexport, &mut prefix, facts);
            }
            syn::Item::Mod(mod_item) => match &mod_item.content {
                Some((_, inner)) => {
                    facts.inline_mods.insert(mod_item.ident.to_string());
                    collect(inner, facts);
                }
                None => {
                    let name = mod_item.ident.to_string();
                    let path_attr = mod_path_attr(&mod_item.attrs);
                    facts.mod_decls.push((name, path_attr));
                }
            },
            _ => {}
        }
    }
}

fn walk_use_tree(
    tree: &syn::UseTree,
    reexport: bool,
    prefix: &mut Vec<String>,
    facts: &mut RustModuleFacts,
) {
    match tree {
        syn::UseTree::Path(segment) => {
            prefix.push(segment.ident.to_string());
            walk_use_tree(&segment.tree, reexport, prefix, facts);
            prefix.pop();
        }
        syn::UseTree::Group(group) => {
            for member in &group.items {
                walk_use_tree(member, reexport, prefix, facts);
            }
        }
        syn::UseTree::Name(leaf) => {
            let segment = leaf.ident.to_string();
            push_leaf(prefix, &segment, None, reexport, facts);
        }
        syn::UseTree::Rename(leaf) => {
            let segment = leaf.ident.to_string();
            let alias = leaf.rename.to_string();
            push_leaf(prefix, &segment, Some(alias), reexport, facts);
        }
        syn::UseTree::Glob(_) => facts.stars.push(StarImport {
            qualifier: prefix.clone(),
            reexport,
        }),
    }
}

/// `self` re-affirms the prefix's own last segment, so `use a::b::self;`
/// reduces to the plain leaf case one segment shorter (qualifier `a`, name `b`).
fn push_leaf(
    prefix: &[String],
    segment: &str,
    alias: Option<String>,
    reexport: bool,
    facts: &mut RustModuleFacts,
) {
    let (qualifier, asked): (Vec<String>, String) = if segment == "self" {
        let Some((last, rest)) = prefix.split_last() else {
            return;
        };
        (rest.to_vec(), last.clone())
    } else {
        (prefix.to_vec(), segment.to_string())
    };
    facts.uses.push(UseBinding {
        local: alias.unwrap_or_else(|| asked.clone()),
        qualifier,
        asked,
        reexport,
    });
}

fn parent_dir(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// `dir` joined with a `#[path = "lit"]` literal, `.`/`..` collapsed.
fn normalize_join(dir: &str, literal: &str) -> String {
    let combined = if dir.is_empty() {
        literal.to_string()
    } else {
        format!("{dir}/{literal}")
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in combined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

// ── the module plane proper ──────────────────────────────────────────────────

/// How an import binding reached its target. Rust has no `default` export
/// form, so this arm's wire vocabulary stops at four values.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ResolvedImportKind {
    Local,
    Indirect,
    Star,
    Namespace,
}

impl ResolvedImportKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            ResolvedImportKind::Local => "local",
            ResolvedImportKind::Indirect => "indirect",
            ResolvedImportKind::Star => "star",
            ResolvedImportKind::Namespace => "namespace",
        }
    }

    /// One more hop's arm folded in: star outranks indirect outranks local.
    fn promote(self, other: ResolvedImportKind) -> ResolvedImportKind {
        use ResolvedImportKind::*;
        match (self, other) {
            (Star, _) | (_, Star) => Star,
            (Indirect, _) | (_, Indirect) => Indirect,
            _ => Local,
        }
    }
}

/// One `use` binding resolved to a corpus declaration (or a whole module for
/// a namespace binding). What `Resolve<CallF>`/`Resolve<TypeF>` bind through.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedImport {
    pub local: String,
    /// The name asked of the source module; `"*"` for a namespace binding.
    pub name: String,
    pub target_path: String,
    pub target_blob: ContentId,
    /// `Span::anchor(0)` for a namespace binding: no one def span applies.
    pub target_span: Span,
    pub target_name: Option<String>,
    pub kind: ResolvedImportKind,
    pub hops: u32,
}

/// The `resolved_import` wire row: `ResolvedImport` with blob/span dropped.
pub struct ImportRow {
    pub local: String,
    pub name: String,
    pub target_path: String,
    pub target_name: Option<String>,
    pub kind: ResolvedImportKind,
    pub hops: u32,
}

/// One qualifier's home file: the corpus file whose module path IS it.
/// `Ambiguous` when 2+ files tie (the kink-4 discipline).
enum HomeFile {
    Unique(String),
    None,
    Ambiguous,
}

/// One name's resolution inside a module: a declaration, a whole submodule
/// (`use a::b;` where `b` is a module, not an item), a star tie, or nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Resolution {
    Binding {
        blob: ContentId,
        span: Span,
        name: String,
        kind: ResolvedImportKind,
        hops: u32,
    },
    Module {
        file: String,
        hops: u32,
    },
    Ambiguous,
    None,
}

impl Resolution {
    fn promoted(self, arm: ResolvedImportKind) -> Resolution {
        match self {
            Resolution::Binding {
                blob,
                span,
                name,
                kind,
                hops,
            } => Resolution::Binding {
                blob,
                span,
                name,
                kind: kind.promote(arm),
                hops: hops + 1,
            },
            Resolution::Module { file, hops } => Resolution::Module {
                file,
                hops: hops + 1,
            },
            other => other,
        }
    }

    /// `promoted`, but a hop that reached nothing carries nothing forward
    /// (an absent name from one star must not shadow a later star's hit).
    fn promoted_option(self, arm: ResolvedImportKind) -> Option<Resolution> {
        (!matches!(self, Resolution::None)).then(|| self.promoted(arm))
    }
}

/// Whether two star arms offer the SAME target: (blob, span) for a binding,
/// the file path for a module.
fn same_target(a: &Resolution, b: &Resolution) -> bool {
    match (a, b) {
        (
            Resolution::Binding { blob: b1, span: s1, .. },
            Resolution::Binding { blob: b2, span: s2, .. },
        ) => b1 == b2 && s1 == s2,
        (Resolution::Module { file: f1, .. }, Resolution::Module { file: f2, .. }) => f1 == f2,
        _ => false,
    }
}

/// THE corpus Rust module plane, built ONCE per refresh in `resolve_project`.
#[derive(Default)]
pub struct RustModuleIndex {
    facts: HashMap<String, RustModuleFacts>,
    blobs: HashMap<String, ContentId>,
    paths: HashMap<ContentId, String>,
    /// Corpus-relative module path per file, `#[path]` overrides applied.
    module_paths: HashMap<String, Vec<String>>,
    /// Module path's LAST segment -> candidate files, the fan-out filter
    /// before the full suffix check (`ModuleTarget::covers`).
    by_last_segment: HashMap<String, Vec<String>>,
    /// blob -> (span, name, family) of every def in it.
    defs: HashMap<ContentId, Vec<(Span, String, FamilyTag)>>,
    /// file -> its WHOLE export table, built once and reused by every
    /// importer (a per-name walk over a many-star barrel is quadratic).
    tables: Mutex<HashMap<String, std::sync::Arc<ExportTable>>>,
    /// file -> its WHOLE local scope (the export table plus non-reexport
    /// globs), for a bare name with no explicit `use` in the SAME file.
    scope_tables: Mutex<HashMap<String, std::sync::Arc<ExportTable>>>,
    /// (self type, fn name) -> the ONE corpus def site the impl block names.
    /// 2+ impls of the same pair is the ambiguity this table declines.
    impl_methods: HashMap<(String, String), Vec<(ContentId, Span)>>,
}

type ExportTable = HashMap<String, Resolution>;

impl RustModuleIndex {
    /// `files` is every `.rs` input's facts; `corpus` is EVERY input's (path,
    /// blob), any language, so target lookups stay lang-agnostic.
    pub fn build(
        files: Vec<(String, RustModuleFacts)>,
        corpus: &[(String, ContentId)],
        def_index: &DefIndex,
    ) -> RustModuleIndex {
        let mut index = RustModuleIndex::default();
        for (path, blob) in corpus {
            index.blobs.insert(path.clone(), blob.clone());
            index.paths.entry(blob.clone()).or_insert_with(|| path.clone());
            if path.ends_with(".rs") {
                index.module_paths.insert(path.clone(), module_segments(path));
            }
        }
        for (path, facts) in &files {
            let dir = parent_dir(path);
            for (name, path_attr) in &facts.mod_decls {
                let Some(literal) = path_attr else { continue };
                let target = normalize_join(dir, literal);
                if index.blobs.contains_key(&target) {
                    let mut segments = module_segments(path);
                    segments.push(name.clone());
                    index.module_paths.insert(target, segments);
                }
            }
        }
        for (path, segments) in &index.module_paths {
            if let Some(last) = segments.last() {
                index
                    .by_last_segment
                    .entry(last.clone())
                    .or_default()
                    .push(path.clone());
            }
        }
        for (name, sites) in &def_index.map {
            for site in sites {
                index.defs.entry(site.blob.clone()).or_default().push((
                    site.span,
                    name.clone(),
                    site.family,
                ));
            }
        }
        for (path, facts) in &files {
            let Some(blob) = index.blobs.get(path) else {
                continue;
            };
            for entry in &facts.impls {
                for (name, span) in &entry.methods {
                    index
                        .impl_methods
                        .entry((entry.self_type.clone(), name.clone()))
                        .or_default()
                        .push((blob.clone(), *span));
                }
            }
        }
        index.facts = files.into_iter().collect();
        index
    }

    /// The ONE def site an impl block names for (self type, fn); 2+ corpus
    /// impls of the pair is an ambiguity this tier does not settle.
    pub(crate) fn impl_target(&self, self_type: &str, method: &str) -> Option<(ContentId, Span)> {
        match self.impl_methods.get(&(self_type.to_string(), method.to_string()))?.as_slice() {
            [only] => Some(only.clone()),
            _ => None,
        }
    }

    /// Every `use` binding `path` writes, resolved; an ambiguous or
    /// corpus-external binding has no row.
    pub fn bindings(&self, path: &str) -> Vec<ImportRow> {
        let Some(facts) = self.facts.get(path) else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for binding in &facts.uses {
            if let Ok(Some(found)) = self.explicit_binding(path, &binding.local) {
                rows.push(ImportRow {
                    local: found.local,
                    name: found.name,
                    target_path: found.target_path,
                    target_name: found.target_name,
                    kind: found.kind,
                    hops: found.hops,
                });
            }
        }
        rows
    }

    /// A bare name: an explicit `use` binding, else any glob's wildcard scope.
    pub fn target(&self, path: &str, local: &str) -> Option<(ContentId, Span)> {
        match self.explicit_binding(path, local) {
            Ok(Some(found)) => return callable_target(found),
            Err(()) => return None,
            Ok(None) => {}
        }
        let mut stack = Vec::new();
        let resolution = self.wildcard_scope(path, local, &mut stack);
        self.finish(local, local, resolution).ok().flatten().and_then(callable_target)
    }

    /// `local`'s EXPLICIT `use` binding in `path`. `Err(())` is AMBIGUOUS: a
    /// glob-hop conflict the drops channel carries.
    fn explicit_binding(&self, path: &str, local: &str) -> Result<Option<ResolvedImport>, ()> {
        let Some(facts) = self.facts.get(path) else {
            return Ok(None);
        };
        let Some(binding) = facts.uses.iter().find(|use_binding| use_binding.local == local)
        else {
            return Ok(None);
        };
        let mut stack = Vec::new();
        let resolution =
            self.resolve_qualified(path, &binding.qualifier, &binding.asked, &mut stack).0;
        self.finish(local, &binding.asked, resolution)
    }

    /// `name` as ANY glob in `path` brings it into scope, reexported or not
    /// (unlike `export_table`'s star leg, which only follows a REEXPORT glob).
    fn wildcard_scope(&self, path: &str, name: &str, stack: &mut Vec<String>) -> Resolution {
        self.local_scope_table(path, stack).0.get(name).cloned().unwrap_or(Resolution::None)
    }

    /// `path`'s own export table, plus every name a non-reexport glob adds
    /// that the export table does not already carry. Built ONCE per file.
    fn local_scope_table(&self, path: &str, stack: &mut Vec<String>) -> (std::sync::Arc<ExportTable>, bool) {
        if let Some(hit) = self.scope_tables.lock().expect("rust scope tables").get(path) {
            return (hit.clone(), true);
        }
        let (public, mut complete) = self.export_table(path, stack);
        let Some(facts) = self.facts.get(path) else {
            return (public, complete);
        };
        let mut table = (*public).clone();
        let starred = self.star_contributions(path, facts.stars.iter().filter(|star| !star.reexport), &table, stack, &mut complete);
        for (name, resolution) in starred {
            table.entry(name).or_insert(resolution);
        }
        let table = std::sync::Arc::new(table);
        if complete {
            self.scope_tables
                .lock()
                .expect("rust scope tables")
                .insert(path.to_string(), table.clone());
        }
        (table, complete)
    }

    fn finish(
        &self,
        local: &str,
        asked: &str,
        resolution: Resolution,
    ) -> Result<Option<ResolvedImport>, ()> {
        match resolution {
            Resolution::Ambiguous => Err(()),
            Resolution::None => Ok(None),
            Resolution::Module { file, hops } => Ok(Some(ResolvedImport {
                local: local.to_string(),
                name: "*".to_string(),
                target_blob: self.blobs.get(&file).cloned().unwrap_or(ZERO_CONTENT_ID),
                target_path: file,
                target_span: Span::anchor(0),
                target_name: None,
                kind: ResolvedImportKind::Namespace,
                hops,
            })),
            Resolution::Binding {
                blob,
                span,
                name,
                kind,
                hops,
            } => Ok(Some(ResolvedImport {
                local: local.to_string(),
                name: asked.to_string(),
                target_path: self.paths.get(&blob).cloned().unwrap_or_default(),
                target_blob: blob,
                target_span: span,
                target_name: Some(name),
                kind,
                hops,
            })),
        }
    }

    /// Namespace wins first (`asked` itself names a submodule of
    /// `qualifier`); else `asked` is an item of `qualifier`'s home module.
    fn resolve_qualified(
        &self,
        from: &str,
        qualifier: &[String],
        asked: &str,
        stack: &mut Vec<String>,
    ) -> (Resolution, bool) {
        let mut full = qualifier.to_vec();
        full.push(asked.to_string());
        if let HomeFile::Unique(file) = self.home_file(from, &full) {
            return (Resolution::Module { file, hops: 1 }, true);
        }
        match self.home_file(from, qualifier) {
            HomeFile::Unique(file) => self.resolve_in_module(&file, asked, stack),
            HomeFile::None => (Resolution::None, true),
            HomeFile::Ambiguous => (Resolution::Ambiguous, true),
        }
    }

    /// A one-segment qualifier naming THIS file's own inline `mod` is a
    /// same-blob hit; else a corpus-wide suffix search on the module path.
    fn home_file(&self, from: &str, qualifier: &[String]) -> HomeFile {
        if qualifier.is_empty() {
            return HomeFile::None;
        }
        if let [only] = qualifier {
            if self.facts.get(from).is_some_and(|facts| facts.inline_mods.contains(only)) {
                return HomeFile::Unique(from.to_string());
            }
        }
        let refs: Vec<&str> = qualifier.iter().map(String::as_str).collect();
        let Some(target) = module_target(from, &refs) else {
            return HomeFile::None;
        };
        // Bucket by the RESOLVED target's own last segment: `super`/`self`/
        // `crate` never appear in a file's own module path.
        let Some(last) = target.suffix.last() else {
            return HomeFile::None;
        };
        let candidates: Vec<&String> = self
            .by_last_segment
            .get(last)
            .into_iter()
            .flatten()
            .filter(|path| {
                self.module_paths
                    .get(*path)
                    .is_some_and(|segments| target.covers(segments))
            })
            .collect();
        match candidates.as_slice() {
            [] => HomeFile::None,
            [only] => HomeFile::Unique((*only).clone()),
            _ => HomeFile::Ambiguous,
        }
    }

    /// `name`'s resolution inside `file`'s WHOLE export table (built once).
    fn resolve_in_module(&self, file: &str, name: &str, stack: &mut Vec<String>) -> (Resolution, bool) {
        let (table, complete) = self.export_table(file, stack);
        (table.get(name).cloned().unwrap_or(Resolution::None), complete)
    }

    /// `file`'s WHOLE export table, each name settled ONCE regardless of how
    /// many importers ask; cached outside any re-export cycle.
    fn export_table(&self, file: &str, stack: &mut Vec<String>) -> (std::sync::Arc<ExportTable>, bool) {
        if let Some(hit) = self.tables.lock().expect("rust module tables").get(file) {
            return (hit.clone(), true);
        }
        if stack.iter().any(|open| open == file) {
            return (std::sync::Arc::new(ExportTable::new()), false);
        }
        let Some(facts) = self.facts.get(file) else {
            return (std::sync::Arc::new(ExportTable::new()), true);
        };
        let mut table = ExportTable::new();
        if let Some(blob) = self.blobs.get(file) {
            for (span, name, family) in self.defs.get(blob).into_iter().flatten() {
                let better = table
                    .get(name)
                    .is_none_or(|existing| !matches!(existing, Resolution::Binding { .. }))
                    || *family == FamilyTag::Call;
                if better {
                    table.insert(
                        name.clone(),
                        Resolution::Binding {
                            blob: blob.clone(),
                            span: *span,
                            name: name.clone(),
                            kind: ResolvedImportKind::Local,
                            hops: 0,
                        },
                    );
                }
            }
        }
        stack.push(file.to_string());
        let mut complete = true;
        for reexport in facts.uses.iter().filter(|binding| binding.reexport) {
            if table.contains_key(&reexport.local) {
                continue;
            }
            let (sub, sub_complete) =
                self.resolve_qualified(file, &reexport.qualifier, &reexport.asked, stack);
            complete &= sub_complete;
            if let Some(found) = sub.promoted_option(ResolvedImportKind::Indirect) {
                table.insert(reexport.local.clone(), found);
            }
        }
        let starred =
            self.star_contributions(file, facts.stars.iter().filter(|star| star.reexport), &table, stack, &mut complete);
        for (name, resolution) in starred {
            table.entry(name).or_insert(resolution);
        }
        stack.pop();
        let table = std::sync::Arc::new(table);
        if complete {
            self.tables.lock().expect("rust module tables").insert(file.to_string(), table.clone());
        }
        (table, complete)
    }

    /// Every name a set of star imports contributes, ambiguous where two
    /// disagree, EXCLUDING names `existing` already settles.
    fn star_contributions<'a>(
        &self,
        file: &str,
        stars: impl Iterator<Item = &'a StarImport>,
        existing: &ExportTable,
        stack: &mut Vec<String>,
        complete: &mut bool,
    ) -> ExportTable {
        let mut starred = ExportTable::new();
        for star in stars {
            let HomeFile::Unique(target) = self.home_file(file, &star.qualifier) else {
                continue;
            };
            let (sub_table, sub_complete) = self.export_table(&target, stack);
            *complete &= sub_complete;
            for (name, resolution) in sub_table.iter() {
                if existing.contains_key(name) {
                    continue;
                }
                let Some(promoted) = resolution.clone().promoted_option(ResolvedImportKind::Star) else {
                    continue;
                };
                starred.insert(
                    name.clone(),
                    match starred.get(name) {
                        None => promoted,
                        Some(incumbent)
                            if matches!(incumbent, Resolution::Ambiguous)
                                || matches!(promoted, Resolution::Ambiguous)
                                || !same_target(incumbent, &promoted) =>
                        {
                            Resolution::Ambiguous
                        }
                        Some(incumbent) => incumbent.clone(),
                    },
                );
            }
        }
        starred
    }
}

/// A namespace binding names a module, not a def: no call/type site binds
/// through one.
fn callable_target(found: ResolvedImport) -> Option<(ContentId, Span)> {
    (found.kind != ResolvedImportKind::Namespace).then_some((found.target_blob, found.target_span))
}
