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

use super::rust::{mod_path_attr, module_segments, module_target};

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

    /// The coordinate two star arms must agree on to not be ambiguous.
    fn seat(&self) -> Option<(&str, u32, u32)> {
        match self {
            Resolution::Binding { span, name, .. } => Some((name, span.start, span.len)),
            Resolution::Module { file, .. } => Some((file, u32::MAX, u32::MAX)),
            _ => None,
        }
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
    /// (file, name) -> resolution, filled on first ask; a cycle's partial
    /// answer is never cached.
    tables: Mutex<HashMap<(String, String), Resolution>>,
}

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
        index.facts = files.into_iter().collect();
        index
    }

    /// Every `use` binding `path` writes, resolved; an ambiguous or
    /// corpus-external binding has no row.
    pub fn bindings(&self, path: &str) -> Vec<ImportRow> {
        let Some(facts) = self.facts.get(path) else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for binding in &facts.uses {
            if let Ok(Some(found)) = self.resolve_use(path, &binding.local) {
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

    /// The (blob, span) a resolve arm binds a bare callee/type name through.
    /// `None` for a namespace binding: a module is not callable.
    pub fn target(&self, path: &str, local: &str) -> Option<(ContentId, Span)> {
        match self.resolve_use(path, local) {
            Ok(Some(found)) if found.kind != ResolvedImportKind::Namespace => {
                Some((found.target_blob, found.target_span))
            }
            _ => None,
        }
    }

    /// `local`'s binding in `path`. `Err(())` is AMBIGUOUS: a glob-hop
    /// conflict the drops channel carries.
    fn resolve_use(&self, path: &str, local: &str) -> Result<Option<ResolvedImport>, ()> {
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
        let candidates: Vec<&String> = self
            .by_last_segment
            .get(qualifier.last().expect("qualifier checked non-empty above"))
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

    /// A local def first, then an explicit re-exporting `use` (indirect),
    /// then every re-export glob (star, ambiguous on disagreement).
    fn resolve_in_module(&self, file: &str, name: &str, stack: &mut Vec<String>) -> (Resolution, bool) {
        let key = (file.to_string(), name.to_string());
        if let Some(cached) = self.tables.lock().expect("rust module tables").get(&key) {
            return (cached.clone(), true);
        }
        if stack.iter().any(|open| open == file) {
            return (Resolution::None, false);
        }
        let Some(facts) = self.facts.get(file) else {
            return (Resolution::None, true);
        };
        if let Some((span, def_name)) = self.def_named_in(file, name) {
            let found = Resolution::Binding {
                blob: self.blobs[file].clone(),
                span,
                name: def_name,
                kind: ResolvedImportKind::Local,
                hops: 0,
            };
            self.remember(key, found.clone());
            return (found, true);
        }
        stack.push(file.to_string());
        let (result, complete) = self.resolve_hops(file, facts, name, stack);
        stack.pop();
        if complete {
            self.remember(key, result.clone());
        }
        (result, complete)
    }

    fn resolve_hops(
        &self,
        file: &str,
        facts: &RustModuleFacts,
        name: &str,
        stack: &mut Vec<String>,
    ) -> (Resolution, bool) {
        let mut complete = true;
        for reexport in facts.uses.iter().filter(|binding| binding.reexport && binding.local == name) {
            let (sub, sub_complete) =
                self.resolve_qualified(file, &reexport.qualifier, &reexport.asked, stack);
            complete &= sub_complete;
            if !matches!(sub, Resolution::None) {
                return (sub.promoted(ResolvedImportKind::Indirect), complete);
            }
        }
        let mut starred: Option<Resolution> = None;
        for star in facts.stars.iter().filter(|star| star.reexport) {
            let sub = match self.home_file(file, &star.qualifier) {
                HomeFile::Unique(target) => {
                    let (sub, sub_complete) = self.resolve_in_module(&target, name, stack);
                    complete &= sub_complete;
                    sub
                }
                HomeFile::None => continue,
                HomeFile::Ambiguous => Resolution::Ambiguous,
            };
            if matches!(sub, Resolution::None) {
                continue;
            }
            let sub = sub.promoted(ResolvedImportKind::Star);
            starred = Some(match starred {
                None => sub,
                Some(incumbent)
                    if matches!(incumbent, Resolution::Ambiguous)
                        || matches!(sub, Resolution::Ambiguous)
                        || incumbent.seat() != sub.seat() =>
                {
                    Resolution::Ambiguous
                }
                Some(incumbent) => incumbent,
            });
        }
        (starred.unwrap_or(Resolution::None), complete)
    }

    fn remember(&self, key: (String, String), value: Resolution) {
        self.tables.lock().expect("rust module tables").insert(key, value);
    }

    /// CallF facet preferred at a shared name (mirrors `call_name_match_in`).
    fn def_named_in(&self, file: &str, name: &str) -> Option<(Span, String)> {
        let blob = self.blobs.get(file)?;
        let sites = self.defs.get(blob)?;
        let hit = sites
            .iter()
            .filter(|(_, site_name, _)| site_name == name)
            .min_by_key(|(_, _, family)| *family != FamilyTag::Call)?;
        Some((hit.0, hit.1.clone()))
    }
}
