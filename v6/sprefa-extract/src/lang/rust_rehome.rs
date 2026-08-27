//! `impl Rehome for RustSource`: every question `extract move` asks a language,
//! answered for Rust. `mod` declarations, `#[path]` literals and `include!`
//! arguments come off the same `syn` parse `lang/rust.rs` already carries;
//! `Cargo.toml` targets off the tree-sitter-toml parse this crate already links.
//! @comment-ok: module header, the seam list every lang file opens with
//!
//! BUY NOTE. A manifest edit here is a byte-span Replace through soopy, never a
//! document round-trip, so span accuracy is the criterion and formatting
//! preservation is not one. `tree-sitter-toml-ng` is ALREADY a direct dependency
//! (the `data` plane's third grammar) and reports byte spans: zero new crates,
//! and the same shape `ts_rehome` uses on `package.json` through
//! tree-sitter-json. REJECTED: `toml_edit` 0.25, +4 crates against this lock
//! (itself, toml_datetime, toml_parser, toml_write; indexmap and winnow are
//! already here) for a formatting guarantee a byte Replace never uses;
//! `basic-toml`, already a dependency, deserializes without spans, so it cannot
//! name the bytes to rewrite; `toml` + `serde_spanned` is the same +4 plus
//! line/col spans that would need a second bridge.
//!
//! Two trait methods stay at their defaults, by decision rather than omission:
//! `shim` (`--shim` answers `rust has no shim form` at `0_move.rs:129`; a
//! `pub use` reexport left at the old path is expressible and is NOT built) and
//! `text_spellings` (a `target/` compiled path is a build's, not a spelling the
//! corpus carries, so the `--text-refs` scan has nothing stable to look for).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use rayon::prelude::*;
use syn::spanned::Spanned;

use super::rust::{build_line_starts, syn_span, RustSource};
use crate::move_cx::{dirname, join_rel, relative_between, MoveCx};
use crate::project::extract_pool;
use crate::types::{ImportRef, Rehome, Respell, Span};

/// The macros whose first argument names a file, resolved against the directory
/// of the file that writes them.
const INCLUDE_MACROS: [&str; 3] = ["include", "include_str", "include_bytes"];

/// The tables whose `path` names a `.rs` file. `[workspace] members` is NOT one:
/// it names a directory whose `Cargo.toml` no `.rs`-only move can carry along.
const TARGET_TABLES: [&str; 5] = ["lib", "bin", "test", "bench", "example"];

impl Rehome for RustSource {
    fn import_refs(&self, cx: &MoveCx) -> Vec<ImportRef> {
        let roots = crate_roots(cx);
        let names = moved_names(cx, self);
        let corpus = cx.files_of(self);

        // Read and parse fan out; the merge below stays sequential over `corpus`
        // in path order, so the ref order is rel order.
        let scans: Vec<Option<FileScan>> = extract_pool().install(|| {
            corpus
                .par_iter()
                .map(|rel| {
                    let bytes = cx.read(rel)?;
                    // A moved file is parsed unconditionally: every path it
                    // writes is re-aimed from its new directory.
                    if cx.destination(rel).is_none() && !carries_name(&bytes, &names) {
                        return None;
                    }
                    scan_file(&String::from_utf8(bytes).ok()?)
                })
                .collect()
        });

        // Every decl resolves first: a `use` path is judged against the module
        // names this batch moves, and those are known only once the decls are.
        let mut resolved: Vec<(&str, &ModDecl, String)> = Vec::new();
        let mut moved_modules: BTreeMap<String, String> = BTreeMap::new();
        for (rel, scan) in corpus.iter().zip(&scans) {
            let Some(scan) = scan else { continue };
            for decl in &scan.decls {
                let Some(target) = resolve_decl(cx, roots, rel, decl) else {
                    continue;
                };
                if cx.destination(&target).is_some() {
                    moved_modules.insert(decl.name.clone(), target.clone());
                }
                resolved.push((rel, decl, target));
            }
        }

        let plan = relocate_plan(cx);
        let mut refs = Vec::new();
        for (rel, decl, target) in &resolved {
            // A relocated decl is lifted, never re-aimed, so the `#[path]` arm
            // never sees it.
            if plan.relocated.contains(target) {
                continue;
            }
            if cx.destination(target).is_none() && cx.destination(rel).is_none() {
                continue;
            }
            refs.push(decl_ref(rel, decl, target));
        }
        for edit in plan.edits.values() {
            refs.push(ImportRef {
                importer: edit.importer.clone(),
                literal: edit.span,
                text: edit.text.clone(),
                target: edit.target.clone(),
                kind: edit.kind,
            });
        }
        for (rel, scan) in corpus.iter().zip(&scans) {
            let Some(scan) = scan else { continue };
            let moving = cx.destination(rel).is_some();
            for include in &scan.includes {
                let target = join_rel(dirname(rel), &include.value);
                if !cx.contains(&target) || (!moving && cx.destination(&target).is_none()) {
                    continue;
                }
                refs.push(ImportRef {
                    importer: rel.to_string(),
                    literal: include.span,
                    text: include.text.clone(),
                    target,
                    kind: "include",
                });
            }
            for item in &scan.uses {
                // ONE ref per `use` item: two moved modules in one path would
                // otherwise claim one span, and `(file, span)` names one edit.
                let Some(target) = item
                    .segments
                    .iter()
                    .find_map(|segment| moved_modules.get(segment))
                else {
                    continue;
                };
                refs.push(ImportRef {
                    importer: rel.to_string(),
                    literal: item.span,
                    text: item.text.clone(),
                    target: target.clone(),
                    kind: "use_path",
                });
            }
        }
        tracing::debug!(corpus = corpus.len(), refs = refs.len(), "move rust refs");
        refs
    }

    fn respell(&self, cx: &MoveCx, reference: &ImportRef) -> Option<Respell> {
        let (text, receipt) = match reference.kind {
            "mod_decl" | "path_attr" => (mod_respell(cx, reference)?, None),
            "include" => (include_respell(cx, reference)?, None),
            // The module tree names a module, never its path on disk, and this
            // arm keeps that tree by writing `#[path]`, never by re-parenting.
            "use_path" => return None,
            "manifest_target" => manifest_respell(cx, reference)?,
            "mod_relocate_out" | "mod_relocate_in" | "mod_path" => {
                let edit = relocate_plan(cx)
                    .edits
                    .get(&(reference.importer.clone(), reference.literal.start))?;
                (edit.replacement.clone(), edit.receipt.clone())
            }
            _ => return None,
        };
        (text != reference.text).then(|| Respell {
            file: reference.importer.clone(),
            span: reference.literal,
            text,
            receipt,
        })
    }

    fn plan_errors(&self, cx: &MoveCx) -> Vec<String> {
        relocate_plan(cx).errors.clone()
    }

    fn manifests(&self, cx: &MoveCx) -> Vec<String> {
        cx.files()
            .iter()
            .filter(|rel| is_manifest(rel))
            .cloned()
            .collect()
    }

    fn manifest_refs(&self, cx: &MoveCx) -> Vec<ImportRef> {
        let mut refs = Vec::new();
        for manifest in self.manifests(cx) {
            let package_dir = dirname(&manifest);
            // A package holding no moved file is never parsed, so it is never
            // opened for writing either.
            if !cx.moved().keys().any(|old| under(old, package_dir)) {
                continue;
            }
            let Some(text) = cx.text(&manifest) else {
                continue;
            };
            for leaf in manifest_leaves(&text) {
                refs.push(ImportRef {
                    importer: manifest.clone(),
                    literal: leaf.span,
                    text: leaf.literal,
                    target: leaf.field,
                    kind: "manifest_target",
                });
            }
        }
        refs
    }
}

// ── the syn scan ────────────────────────────────────────────────────────────

#[derive(Default)]
struct FileScan {
    decls: Vec<ModDecl>,
    includes: Vec<IncludeLit>,
    uses: Vec<UseItem>,
    /// Where the file's first item opens, attributes included. Inner attributes
    /// and `//!` docs sit above it, so it is the first offset an item may take.
    first_item: Option<u32>,
}

/// One `mod name;` declaration. `item` covers the whole item, attributes
/// included, so a decl with no `#[path]` can grow one in a single Replace.
struct ModDecl {
    item: Span,
    text: String,
    name: String,
    /// The inline `mod x { .. }` blocks enclosing the decl, outermost first.
    chain: Vec<String>,
    /// The `#[path = ".."]` literal's span and value, when the decl carries one.
    attr: Option<(Span, String)>,
    /// The visibility as written, `""` for a private decl.
    vis: String,
}

struct IncludeLit {
    span: Span,
    text: String,
    value: String,
}

struct UseItem {
    span: Span,
    text: String,
    segments: BTreeSet<String>,
}

fn scan_file(text: &str) -> Option<FileScan> {
    scan_with(text, false).map(|(scan, _)| scan)
}

/// ONE parse. `runs` adds the crate-wide path walk `--relocate-mod` needs and
/// every other caller pays nothing for it.
fn scan_with(text: &str, runs: bool) -> Option<(FileScan, Vec<SegRun>)> {
    let parsed = syn::parse_file(text).ok()?;
    let line_starts = build_line_starts(text);
    let mut scan = FileScan::default();
    collect_items(
        &parsed.items,
        text,
        &line_starts,
        &mut Vec::new(),
        &mut scan,
    );
    scan.first_item = parsed
        .items
        .first()
        .map(|item| syn_span(&line_starts, item.span()).start);
    let mut includes = IncludeScan {
        source: text,
        line_starts: &line_starts,
        out: Vec::new(),
    };
    syn::visit::Visit::visit_file(&mut includes, &parsed);
    scan.includes = includes.out;
    if !runs {
        return Some((scan, Vec::new()));
    }
    let mut paths = PathScan {
        source: text,
        line_starts: &line_starts,
        depth: 0,
        out: Vec::new(),
    };
    syn::visit::Visit::visit_file(&mut paths, &parsed);
    Some((scan, paths.out))
}

/// Descends into inline `mod name { .. }` bodies, carrying the block names: a
/// decl nested in one resolves against a directory per enclosing block.
fn collect_items(
    items: &[syn::Item],
    source: &str,
    line_starts: &[u32],
    chain: &mut Vec<String>,
    out: &mut FileScan,
) {
    for item in items {
        match item {
            syn::Item::Mod(mod_item) => match &mod_item.content {
                Some((_, inner)) => {
                    chain.push(mod_item.ident.to_string());
                    collect_items(inner, source, line_starts, chain, out);
                    chain.pop();
                }
                None => {
                    let span = syn_span(line_starts, mod_item.span());
                    let Some(text) = slice(source, span).filter(|text| text.contains("mod")) else {
                        continue;
                    };
                    out.decls.push(ModDecl {
                        item: span,
                        text,
                        name: mod_item.ident.to_string(),
                        chain: chain.clone(),
                        attr: path_attr(&mod_item.attrs, source, line_starts),
                        vis: vis_text(&mod_item.vis, source, line_starts),
                    });
                }
            },
            syn::Item::Use(use_item) => {
                let span = syn_span(line_starts, use_item.span());
                let Some(text) = slice(source, span) else {
                    continue;
                };
                let mut segments = BTreeSet::new();
                use_segments(&use_item.tree, &mut segments);
                out.uses.push(UseItem {
                    span,
                    text,
                    segments,
                });
            }
            _ => {}
        }
    }
}

/// `#[path = "x.rs"]` as (literal span, value). The span covers the quotes, so a
/// respell reproduces the literal whole.
fn path_attr(
    attrs: &[syn::Attribute],
    source: &str,
    line_starts: &[u32],
) -> Option<(Span, String)> {
    attrs.iter().find_map(|attr| {
        if !attr.path().is_ident("path") {
            return None;
        }
        let syn::Meta::NameValue(pair) = &attr.meta else {
            return None;
        };
        let syn::Expr::Lit(literal) = &pair.value else {
            return None;
        };
        let syn::Lit::Str(text) = &literal.lit else {
            return None;
        };
        let span = syn_span(line_starts, text.span());
        is_literal(source, span).then(|| (span, text.value()))
    })
}

/// Whether `span` really covers a string literal. `syn_span` bridges a
/// proc_macro2 CHAR column, so a non-ASCII byte earlier on the line shifts it.
fn is_literal(source: &str, span: Span) -> bool {
    slice(source, span).is_some_and(|text| text.starts_with('"') || text.starts_with('r'))
}

/// `pub`, `pub(crate)`, `pub(in path)` as written; `""` for a private decl.
fn vis_text(vis: &syn::Visibility, source: &str, line_starts: &[u32]) -> String {
    match vis {
        syn::Visibility::Inherited => String::new(),
        written => {
            let span = syn_span(line_starts, written.span());
            slice(source, span)
                .filter(|text| text.starts_with("pub"))
                .unwrap_or_else(|| "pub".to_string())
        }
    }
}

/// Every module segment a `use` tree names. A glob binds no segment of its own.
fn use_segments(tree: &syn::UseTree, out: &mut BTreeSet<String>) {
    match tree {
        syn::UseTree::Path(segment) => {
            out.insert(segment.ident.to_string());
            use_segments(&segment.tree, out);
        }
        syn::UseTree::Group(group) => {
            for member in &group.items {
                use_segments(member, out);
            }
        }
        syn::UseTree::Name(leaf) => {
            out.insert(leaf.ident.to_string());
        }
        syn::UseTree::Rename(leaf) => {
            out.insert(leaf.ident.to_string());
        }
        syn::UseTree::Glob(_) => {}
    }
}

/// `include!`-family invocations anywhere: item, expression, statement, type or
/// pattern position, which is what one `Visit` over the file reaches.
struct IncludeScan<'a> {
    source: &'a str,
    line_starts: &'a [u32],
    out: Vec<IncludeLit>,
}

impl<'ast> syn::visit::Visit<'ast> for IncludeScan<'_> {
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        let Some(name) = node.path.get_ident() else {
            return;
        };
        if !INCLUDE_MACROS.contains(&name.to_string().as_str()) {
            return;
        }
        let Ok(literal) = node.parse_body::<syn::LitStr>() else {
            return;
        };
        let span = syn_span(self.line_starts, literal.span());
        let Some(text) = slice(self.source, span).filter(|_| is_literal(self.source, span)) else {
            return;
        };
        self.out.push(IncludeLit {
            span,
            text,
            value: literal.value(),
        });
    }
}

fn slice(source: &str, span: Span) -> Option<String> {
    let start = span.start as usize;
    source
        .get(start..start + span.len as usize)
        .map(str::to_string)
}

/// One `::`-joined run of path segments as written, `idents` and `spans` lined
/// up. A `use` tree branch and an expression/type path both flatten to this.
struct SegRun {
    idents: Vec<String>,
    spans: Vec<Span>,
    /// A `use` may name a module with nothing after it; an expression never can,
    /// so a bare trailing segment there is a value or a type, not a module.
    from_use: bool,
    /// Written inside an inline `mod x { .. }`, which re-bases `self` and `super`.
    in_block: bool,
}

/// Every path a file writes, `use` trees included. `crate::a::f` in expression
/// position and `use crate::a::f;` reach `--relocate-mod` the same way.
struct PathScan<'a> {
    source: &'a str,
    line_starts: &'a [u32],
    depth: usize,
    out: Vec<SegRun>,
}

impl PathScan<'_> {
    /// Drops a run whose spans do not slice back to their own idents:
    /// `syn_span` bridges a CHAR column, so a non-ASCII byte shifts the line.
    fn push(&mut self, idents: Vec<String>, spans: Vec<Span>, from_use: bool) {
        if idents.is_empty() || idents.len() != spans.len() {
            return;
        }
        let honest = idents
            .iter()
            .zip(&spans)
            .all(|(ident, span)| slice(self.source, *span).as_deref() == Some(ident.as_str()));
        if !honest {
            return;
        }
        self.out.push(SegRun {
            idents,
            spans,
            from_use,
            in_block: self.depth > 0,
        });
    }
}

impl<'ast> syn::visit::Visit<'ast> for PathScan<'_> {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        match node.content.is_some() {
            true => {
                self.depth += 1;
                syn::visit::visit_item_mod(self, node);
                self.depth -= 1;
            }
            false => syn::visit::visit_item_mod(self, node),
        }
    }

    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        // `use ::krate::..` names an extern crate, never a module of this one.
        if node.leading_colon.is_some() {
            return;
        }
        let mut branches = Vec::new();
        use_runs(&node.tree, self.line_starts, &mut Vec::new(), &mut branches);
        for (idents, spans) in branches {
            self.push(idents, spans, true);
        }
    }

    fn visit_path(&mut self, node: &'ast syn::Path) {
        if node.leading_colon.is_none() {
            let mut idents = Vec::new();
            let mut spans = Vec::new();
            for segment in &node.segments {
                idents.push(segment.ident.to_string());
                spans.push(syn_span(self.line_starts, segment.ident.span()));
            }
            self.push(idents, spans, false);
        }
        syn::visit::visit_path(self, node);
    }
}

/// One flattened branch per bound name: a `Group` forks, a `Glob` ends the run
/// at the module it stars.
fn use_runs(
    tree: &syn::UseTree,
    line_starts: &[u32],
    prefix: &mut Vec<(String, Span)>,
    out: &mut Vec<(Vec<String>, Vec<Span>)>,
) {
    let mut emit = |prefix: &Vec<(String, Span)>, leaf: Option<(String, Span)>| {
        let mut idents: Vec<String> = prefix.iter().map(|(ident, _)| ident.clone()).collect();
        let mut spans: Vec<Span> = prefix.iter().map(|(_, span)| *span).collect();
        if let Some((ident, span)) = leaf {
            idents.push(ident);
            spans.push(span);
        }
        out.push((idents, spans));
    };
    match tree {
        syn::UseTree::Path(segment) => {
            prefix.push((
                segment.ident.to_string(),
                syn_span(line_starts, segment.ident.span()),
            ));
            use_runs(&segment.tree, line_starts, prefix, out);
            prefix.pop();
        }
        syn::UseTree::Group(group) => {
            for member in &group.items {
                use_runs(member, line_starts, prefix, out);
            }
        }
        syn::UseTree::Name(leaf) => emit(
            prefix,
            Some((
                leaf.ident.to_string(),
                syn_span(line_starts, leaf.ident.span()),
            )),
        ),
        syn::UseTree::Rename(leaf) => emit(
            prefix,
            Some((
                leaf.ident.to_string(),
                syn_span(line_starts, leaf.ident.span()),
            )),
        ),
        syn::UseTree::Glob(_) => emit(prefix, None),
    }
}

// ── rustc's module-file law ─────────────────────────────────────────────────

/// The directory a file's child `mod` decls resolve against: a crate root and a
/// `mod.rs` own theirs, every other file owns one named after itself.
fn module_dir(rel: &str, roots: &BTreeSet<String>) -> String {
    if is_mod_rs(rel, roots) {
        dirname(rel).to_string()
    } else {
        join_rel(dirname(rel), &stem(rel))
    }
}

fn is_mod_rs(rel: &str, roots: &BTreeSet<String>) -> bool {
    stem(rel) == "mod" || roots.contains(rel)
}

/// The directory the decl itself resolves against: the declaring file's module
/// directory, one level deeper per enclosing inline `mod` block.
fn decl_base(rel: &str, chain: &[String], roots: &BTreeSet<String>) -> String {
    chain
        .iter()
        .fold(module_dir(rel, roots), |dir, block| join_rel(&dir, block))
}

/// The directory a `#[path]` on the decl resolves against. Outside an inline
/// block that is the declaring FILE's directory, not its module directory.
fn attr_base(rel: &str, chain: &[String], roots: &BTreeSet<String>) -> String {
    match chain.is_empty() {
        true => dirname(rel).to_string(),
        false => decl_base(rel, chain, roots),
    }
}

/// Where rustc looks for `mod name;` declared against `base`, in probe order.
fn natural_paths(base: &str, name: &str) -> [String; 2] {
    [
        join_rel(base, &format!("{name}.rs")),
        join_rel(base, &format!("{name}/mod.rs")),
    ]
}

/// The corpus file one decl names, pre-move.
fn resolve_decl(
    cx: &MoveCx,
    roots: &BTreeSet<String>,
    rel: &str,
    decl: &ModDecl,
) -> Option<String> {
    match &decl.attr {
        Some((_, value)) => {
            let target = join_rel(&attr_base(rel, &decl.chain, roots), value);
            cx.contains(&target).then_some(target)
        }
        None => natural_paths(&decl_base(rel, &decl.chain, roots), &decl.name)
            .into_iter()
            .find(|candidate| cx.contains(candidate)),
    }
}

fn decl_ref(rel: &str, decl: &ModDecl, target: &str) -> ImportRef {
    match &decl.attr {
        Some((span, _)) => ImportRef {
            importer: rel.to_string(),
            literal: *span,
            text: slice_of(&decl.text, decl.item, *span),
            target: target.to_string(),
            kind: "path_attr",
        },
        None => ImportRef {
            importer: rel.to_string(),
            literal: decl.item,
            text: decl.text.clone(),
            target: target.to_string(),
            kind: "mod_decl",
        },
    }
}

/// `inner`'s bytes, read out of the item text `outer` spans.
fn slice_of(text: &str, outer: Span, inner: Span) -> String {
    let start = inner.start.saturating_sub(outer.start) as usize;
    text.get(start..start + inner.len as usize)
        .map(str::to_string)
        .unwrap_or_default()
}

/// The decl `import_refs` recorded at `reference.literal`, re-derived off the
/// same walk of the same (still unwritten) file, so the two calls share no state.
fn decl_at(cx: &MoveCx, reference: &ImportRef) -> Option<ModDecl> {
    let text = cx.text(&reference.importer)?;
    let scan = scan_file(&text)?;
    scan.decls.into_iter().find(|decl| match &decl.attr {
        Some((span, _)) => span.start == reference.literal.start,
        None => decl.item.start == reference.literal.start,
    })
}

// ── the respells ────────────────────────────────────────────────────────────

/// A `mod` decl whose file leaves the place rustc looks for it grows a
/// `#[path]`; one that already carries a `#[path]` keeps it, re-aimed.
fn mod_respell(cx: &MoveCx, reference: &ImportRef) -> Option<String> {
    let roots = crate_roots(cx);
    let decl = decl_at(cx, reference)?;
    let importer = cx.after(&reference.importer);
    let target = cx.after(&reference.target);
    let aimed = relative_between(&attr_base(importer, &decl.chain, roots), target);
    match reference.kind {
        "path_attr" => Some(format!("\"{aimed}\"")),
        _ => match natural_paths(&decl_base(importer, &decl.chain, roots), &decl.name)
            .contains(&target.to_string())
        {
            true => None,
            false => Some(format!("#[path = \"{aimed}\"] {}", decl.text)),
        },
    }
}

/// An `include!` argument resolves against the including file's own directory,
/// so a move of either end re-aims it.
fn include_respell(cx: &MoveCx, reference: &ImportRef) -> Option<String> {
    let from_dir = dirname(cx.after(&reference.importer));
    let aimed = relative_between(from_dir, cx.after(&reference.target));
    Some(format!("\"{aimed}\""))
}

/// The replacement for one manifest target, plus the receipt line it reports
/// itself with, or None when it names nothing this run moved.
fn manifest_respell(cx: &MoveCx, reference: &ImportRef) -> Option<(String, Option<String>)> {
    let package_dir = dirname(&reference.importer);
    let quote = quote_of(&reference.text);
    let written = toml_bare(&reference.text);
    let old = join_rel(package_dir, written);
    let new = cx.destination(&old)?;
    let aimed = relative_between(package_dir, new);
    let receipt = format!(
        "manifest {}: {} {written} -> {aimed}",
        reference.importer, reference.target
    );
    Some((format!("{quote}{aimed}{quote}"), Some(receipt)))
}

// ── --relocate-mod: the module tree follows the file ────────────────────────

/// One module whose declaring parent changes when the batch lands: the default
/// arm holds the tree still and writes `#[path]`, this one moves the tree.
struct Relocation {
    name: String,
    /// Module path from the crate root, before and after the batch.
    old_path: Vec<String>,
    new_path: Vec<String>,
    /// Pre-move rels. Every edit stages ahead of the Moves (`0_move.rs`), so a
    /// parent this same batch creates is edited at the path it still wears.
    old_parent: String,
    new_parent: String,
    /// The decl to lift, grown to whole lines, and the bytes it spans.
    decl: Span,
    decl_text: String,
    vis: String,
    /// The `#[path = ".."] ` the new parent needs when the destination file's
    /// name is not the module's; empty when rustc's own probe finds it.
    aim: String,
}

/// The ref `import_refs` publishes and the bytes `respell` answers it with. A
/// zero-length span is an insertion, which soopy plans.
struct RelocateEdit {
    importer: String,
    span: Span,
    text: String,
    target: String,
    kind: &'static str,
    replacement: String,
    receipt: Option<String>,
}

#[derive(Default)]
struct RelocatePlan {
    /// Moved files whose decl this strategy owns, so the `#[path]` arm drops them.
    relocated: BTreeSet<String>,
    /// Keyed by (file, offset), which is exactly the key the core claims.
    edits: BTreeMap<(String, u32), RelocateEdit>,
    /// Reasons this batch cannot be planned, answered by `Rehome::plan_errors`.
    errors: Vec<String>,
}

/// The plan, built once per root per process. A run carries ONE batch, so the
/// key `crate_roots` already caches by is the key this caches by too.
fn relocate_plan(cx: &MoveCx) -> &'static RelocatePlan {
    static CACHE: OnceLock<Mutex<BTreeMap<PathBuf, &'static RelocatePlan>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut held = match cache.lock() {
        Ok(held) => held,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(existing) = held.get(cx.root()) {
        return existing;
    }
    let leaked: &'static RelocatePlan = Box::leak(Box::new(build_relocate_plan(cx)));
    held.insert(cx.root().to_path_buf(), leaked);
    leaked
}

fn build_relocate_plan(cx: &MoveCx) -> RelocatePlan {
    let mut plan = RelocatePlan::default();
    if !cx.relocate_mod() {
        return plan;
    }
    let roots = crate_roots(cx);
    let scanned = relocate_scan(cx);

    let mut moves: BTreeMap<String, Relocation> = BTreeMap::new();
    for (rel, text, scan, _) in &scanned {
        for decl in &scan.decls {
            // A `#[path]` decl and one inside an inline block both spell a module
            // tree the file layout does not, and the arithmetic here reads layout.
            if decl.attr.is_some() || !decl.chain.is_empty() {
                continue;
            }
            let Some(target) = resolve_decl(cx, roots, rel, decl) else {
                continue;
            };
            match plan_relocation(cx, roots, rel, text, decl, &target) {
                Ok(Some(relocation)) => {
                    moves.insert(target, relocation);
                }
                Ok(None) => {}
                Err(reason) => plan.errors.push(reason),
            }
        }
    }
    // A batch this arm cannot plan whole is planned not at all: `plan_errors`
    // stops the run before any stage is built.
    if moves.is_empty() || !plan.errors.is_empty() {
        return plan;
    }
    plan.relocated = moves.keys().cloned().collect();

    // Every path that named a relocated module, and whether anything reaches it
    // from outside the module it lands in: that is the whole visibility question.
    let mut outside: BTreeSet<String> = BTreeSet::new();
    for (rel, text, _, runs) in &scanned {
        let moving = cx.destination(rel).is_some();
        let Some((_, here)) = module_path(rel, roots) else {
            continue;
        };
        for run in runs {
            let Some((target, span, written, replacement)) =
                run_edit(&moves, moving, &here, run, text)
            else {
                continue;
            };
            let relocation = &moves[&target];
            let landing = &relocation.new_path[..relocation.new_path.len() - 1];
            if !here.starts_with(landing) {
                outside.insert(target.clone());
            }
            plan.edits.insert(
                (rel.clone(), span.start),
                RelocateEdit {
                    importer: rel.clone(),
                    span,
                    text: written,
                    target,
                    kind: "mod_path",
                    replacement,
                    receipt: None,
                },
            );
        }
    }

    for (target, relocation) in &moves {
        plan.edits.insert(
            (relocation.old_parent.clone(), relocation.decl.start),
            RelocateEdit {
                importer: relocation.old_parent.clone(),
                span: relocation.decl,
                text: relocation.decl_text.clone(),
                target: target.clone(),
                kind: "mod_relocate_out",
                replacement: String::new(),
                receipt: None,
            },
        );
    }
    insert_decls(cx, &moves, &outside, &mut plan);
    plan
}

/// The `mod` lines the new parents gain. Two modules landing at one offset merge
/// into ONE insertion: the core gives an offset a single claimant.
fn insert_decls(
    cx: &MoveCx,
    moves: &BTreeMap<String, Relocation>,
    outside: &BTreeSet<String>,
    plan: &mut RelocatePlan,
) {
    let mut by_parent: BTreeMap<&String, Vec<(&String, &Relocation)>> = BTreeMap::new();
    for (target, relocation) in moves {
        by_parent
            .entry(&relocation.new_parent)
            .or_default()
            .push((target, relocation));
    }
    for (parent, group) in by_parent {
        let Some(text) = cx.text(parent) else {
            continue;
        };
        let Some(scan) = scan_file(&text) else {
            continue;
        };
        let mut lines: BTreeMap<u32, Vec<String>> = BTreeMap::new();
        let mut owners: BTreeMap<u32, (String, String)> = BTreeMap::new();
        for (target, relocation) in group {
            let vis = match (relocation.vis.is_empty(), outside.contains(target)) {
                (false, _) => format!("{} ", relocation.vis),
                (true, true) => "pub ".to_string(),
                (true, false) => String::new(),
            };
            let offset = insertion_offset(&text, &scan, &relocation.name);
            lines
                .entry(offset)
                .or_default()
                .push(format!("{}{vis}mod {};\n", relocation.aim, relocation.name));
            owners.entry(offset).or_insert_with(|| {
                (
                    target.clone(),
                    format!(
                        "relocate mod {}: {} -> {parent}",
                        relocation.name, relocation.old_parent
                    ),
                )
            });
        }
        for (offset, mut written) in lines {
            written.sort();
            let mut body: String = written.concat();
            if offset as usize == text.len() && !text.is_empty() && !text.ends_with('\n') {
                body.insert(0, '\n');
            }
            let (target, receipt) = owners.remove(&offset).unwrap_or_default();
            plan.edits.insert(
                (parent.clone(), offset),
                RelocateEdit {
                    importer: parent.clone(),
                    span: Span {
                        start: offset,
                        len: 0,
                    },
                    text: String::new(),
                    target,
                    kind: "mod_relocate_in",
                    replacement: body,
                    receipt: Some(receipt),
                },
            );
        }
    }
}

/// Every owned file, read and parsed once with the path walk on. The read and
/// parse fan out; the result stays in path order.
fn relocate_scan(cx: &MoveCx) -> Vec<(String, String, FileScan, Vec<SegRun>)> {
    let corpus = cx.files_of(&RustSource);
    let scans: Vec<Option<(String, FileScan, Vec<SegRun>)>> = extract_pool().install(|| {
        corpus
            .par_iter()
            .map(|rel| {
                let text = String::from_utf8(cx.read(rel)?).ok()?;
                let (scan, runs) = scan_with(&text, true)?;
                Some((text, scan, runs))
            })
            .collect()
    });
    corpus
        .into_iter()
        .zip(scans)
        .filter_map(|(rel, scan)| {
            scan.map(|(text, scan, runs)| (rel.to_string(), text, scan, runs))
        })
        .collect()
}

/// The relocation one decl asks for, or None when its parent module is unchanged
/// and the default `#[path]` arm still answers. Err when there is nowhere to
/// write the lifted decl, which stops the whole run.
fn plan_relocation(
    cx: &MoveCx,
    roots: &BTreeSet<String>,
    parent_rel: &str,
    parent_text: &str,
    decl: &ModDecl,
    target: &str,
) -> Result<Option<Relocation>, String> {
    let Some(new_target) = cx.destination(target) else {
        return Ok(None);
    };
    let Some((_, old_path)) = module_path(target, roots) else {
        return Ok(None);
    };
    let Some((root, new_path)) = module_path(new_target, roots) else {
        return Err(no_parent_module(target, new_target, &[]));
    };
    if old_path.is_empty() || new_path.is_empty() {
        return Ok(None);
    }
    if old_path[..old_path.len() - 1] == new_path[..new_path.len() - 1] {
        return Ok(None);
    }
    let candidates = parent_files(&root, &new_path[..new_path.len() - 1]);
    let Some((edit_at, lands_at)) = candidates
        .iter()
        .find_map(|candidate| editable(cx, candidate).map(|pre| (pre, candidate.clone())))
    else {
        return Err(no_parent_module(target, new_target, &candidates));
    };
    let aim = match natural_paths(&module_dir(&lands_at, roots), &decl.name)
        .contains(&new_target.to_string())
    {
        true => String::new(),
        false => format!(
            "#[path = \"{}\"] ",
            relative_between(dirname(&lands_at), new_target)
        ),
    };
    let Some((span, text)) = whole_lines(parent_text, decl.item) else {
        return Ok(None);
    };
    Ok(Some(Relocation {
        name: decl.name.clone(),
        old_path,
        new_path,
        old_parent: parent_rel.to_string(),
        new_parent: edit_at,
        decl: span,
        decl_text: text,
        vis: decl.vis.clone(),
        aim,
    }))
}

/// The decl goes into the module owning the destination directory; with no file
/// for that module there is nowhere to write it.
fn no_parent_module(target: &str, new_target: &str, candidates: &[String]) -> String {
    let named = match candidates.is_empty() {
        true => "the destination sits under no crate root".to_string(),
        false => format!("expected {}", candidates.join(" or ")),
    };
    format!(
        "--relocate-mod: {target} -> {new_target} has no parent module file ({named}); \
         create one in the same batch or drop --relocate-mod"
    )
}

/// The replacement one written path run asks for: the target it names, the bytes
/// it covers, and the path it becomes.
fn run_edit(
    moves: &BTreeMap<String, Relocation>,
    moving: bool,
    here: &[String],
    run: &SegRun,
    source: &str,
) -> Option<(String, Span, String, String)> {
    let (steps, eaten, absolute) = qualifier_of(&run.idents);
    // `super`, `self` and a bare name read against the file's OWN module, which
    // an inline block re-bases and a moving file re-parents. `crate` does not.
    if !absolute && (moving || run.in_block) {
        return None;
    }
    let base: Vec<String> = match absolute {
        true => Vec::new(),
        false => here.get(..here.len().checked_sub(steps)?)?.to_vec(),
    };
    for (target, relocation) in moves {
        let wanted = relocation.old_path.len();
        // The run spells nothing of the module path when the base already covers
        // it: the file sits inside the module that moved, which is its own batch.
        if base.len() >= wanted {
            continue;
        }
        let cut = eaten + (wanted - base.len());
        if cut > run.idents.len() || (!run.from_use && cut == run.idents.len()) {
            continue;
        }
        if base[..] != relocation.old_path[..base.len()]
            || run.idents[eaten..cut] != relocation.old_path[base.len()..]
        {
            continue;
        }
        let start = run.spans[0].start;
        let last = run.spans[cut - 1];
        let span = Span {
            start,
            len: last.start + last.len - start,
        };
        let written = slice(source, span)?;
        // The same qualifier re-spells the new path when it can still reach it;
        // otherwise the only spelling every module shares is `crate`.
        let (head, tail) = match relocation.new_path.starts_with(&base) {
            true => (
                run.idents[..eaten].join("::"),
                relocation.new_path[base.len()..].join("::"),
            ),
            false => ("crate".to_string(), relocation.new_path.join("::")),
        };
        let replacement = match head.is_empty() {
            true => tail,
            false => format!("{head}::{tail}"),
        };
        return Some((target.clone(), span, written, replacement));
    }
    None
}

/// The leading `crate` / `self` / `super`*: how many steps it climbs, how many
/// idents that eats, and whether it reads from the crate root wherever written.
fn qualifier_of(idents: &[String]) -> (usize, usize, bool) {
    match idents.first().map(String::as_str) {
        Some("crate") => (0, 1, true),
        Some("self") => (0, 1, false),
        Some("super") => {
            let steps = idents
                .iter()
                .take_while(|ident| ident.as_str() == "super")
                .count();
            (steps, steps, false)
        }
        _ => (0, 0, false),
    }
}

/// A file's crate root and its module path from that root, by file layout alone.
/// A `#[path]` decl breaks that reading, so only natural decls reach here.
fn module_path(rel: &str, roots: &BTreeSet<String>) -> Option<(String, Vec<String>)> {
    let root = owning_root(rel, roots)?;
    if rel == root {
        return Some((root, Vec::new()));
    }
    let base = dirname(&root);
    let tail = match base.is_empty() {
        true => rel,
        false => rel.strip_prefix(&format!("{base}/"))?,
    };
    let mut parts: Vec<String> = tail.split('/').map(str::to_string).collect();
    let name = parts.pop()?;
    let name = name.strip_suffix(".rs")?;
    if name != "mod" {
        parts.push(name.to_string());
    }
    Some((root, parts))
}

/// The crate root `rel` answers to: the one whose directory is its deepest
/// ancestor, and itself when it is a root.
fn owning_root(rel: &str, roots: &BTreeSet<String>) -> Option<String> {
    roots
        .iter()
        .filter(|root| rel == root.as_str() || under(rel, dirname(root)))
        .max_by_key(|root| (rel == root.as_str(), dirname(root).len()))
        .cloned()
}

/// The two files rustc accepts for the module path `path` under `root`'s crate,
/// or the crate root itself when `path` is the root module.
fn parent_files(root: &str, path: &[String]) -> Vec<String> {
    if path.is_empty() {
        return vec![root.to_string()];
    }
    let base = dirname(root);
    let joined = path.join("/");
    vec![
        join_rel(base, &format!("{joined}.rs")),
        join_rel(base, &format!("{joined}/mod.rs")),
    ]
}

/// The pre-move rel to edit for a file that exists once the batch lands: itself
/// when it stays, its source when this same batch moves it there.
fn editable(cx: &MoveCx, rel: &str) -> Option<String> {
    if let Some((old, _)) = cx.moved().iter().find(|(_, new)| new.as_str() == rel) {
        return Some(old.clone());
    }
    (cx.contains(rel) && cx.destination(rel).is_none()).then(|| rel.to_string())
}

/// `span` grown to the whole lines it sits on, so lifting an item leaves no
/// blank remainder. A line carrying other code keeps its own bytes.
fn whole_lines(text: &str, span: Span) -> Option<(Span, String)> {
    let start = span.start as usize;
    let end = start + span.len as usize;
    let opens = line_start(text, start);
    let from = match text.get(opens..start)?.trim().is_empty() {
        true => opens,
        false => start,
    };
    let closes = line_after(text, end);
    let to = match text.get(end..closes)?.trim().is_empty() {
        true => closes,
        false => end,
    };
    Some((
        Span {
            start: from as u32,
            len: (to - from) as u32,
        },
        text.get(from..to)?.to_string(),
    ))
}

/// Where a new `mod name;` goes: sorted among the file-level `mod` items, after
/// the last of them, or above the first item when the file declares none.
fn insertion_offset(text: &str, scan: &FileScan, name: &str) -> u32 {
    let siblings: Vec<&ModDecl> = scan
        .decls
        .iter()
        .filter(|decl| decl.chain.is_empty())
        .collect();
    for decl in &siblings {
        if decl.name.as_str() > name {
            return line_start(text, decl.item.start as usize) as u32;
        }
    }
    if let Some(last) = siblings.last() {
        return line_after(text, (last.item.start + last.item.len) as usize) as u32;
    }
    match scan.first_item {
        Some(at) => line_start(text, at as usize) as u32,
        None => text.len() as u32,
    }
}

fn line_start(text: &str, at: usize) -> usize {
    text.get(..at)
        .and_then(|head| head.rfind('\n'))
        .map(|found| found + 1)
        .unwrap_or(0)
}

/// The offset just past the newline ending the line `at` sits on.
fn line_after(text: &str, at: usize) -> usize {
    text.get(at..)
        .and_then(|tail| tail.find('\n'))
        .map(|found| at + found + 1)
        .unwrap_or(text.len())
}

// ── Cargo.toml targets ──────────────────────────────────────────────────────

/// One string leaf naming a `.rs` file: where it sits, what it spells, and the
/// `bin[0].path` display a receipt line prints.
struct ManifestLeaf {
    span: Span,
    literal: String,
    field: String,
}

fn is_manifest(rel: &str) -> bool {
    rel == "Cargo.toml" || rel.ends_with("/Cargo.toml")
}

/// Whether the root-relative `path` sits at or under the directory `dir`.
fn under(path: &str, dir: &str) -> bool {
    dir.is_empty() || path.starts_with(&format!("{dir}/"))
}

/// Every `path` under a target table and `[package] build`, in document order.
/// Spans include the quotes, so every byte outside a rewritten literal survives.
fn manifest_leaves(text: &str) -> Vec<ManifestLeaf> {
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_toml_ng::LANGUAGE.into())
        .is_err()
    {
        return Vec::new();
    }
    let Some(tree) = parser.parse(text, None) else {
        return Vec::new();
    };
    let source = text.as_bytes();
    let mut out = Vec::new();
    // A `[[bin]]` header repeats, so its element index joins the display;
    // without it two elements print one address.
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let root = tree.root_node();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        let header = match child.kind() {
            "table" | "table_array_element" => table_key(child, source),
            _ => continue,
        };
        let Some(header) = header else { continue };
        let display = match child.kind() {
            "table_array_element" => {
                let index = seen.entry(header.clone()).or_insert(0);
                let display = format!("{header}[{index}]");
                *index += 1;
                display
            }
            _ => header.clone(),
        };
        let Some(key) = target_key(&header) else {
            continue;
        };
        collect_pairs(child, source, key, &display, &mut out);
    }
    out
}

/// The key a table's `path`-shaped value hides behind, or None when the table
/// names no `.rs` file at all.
fn target_key(header: &str) -> Option<&'static str> {
    if TARGET_TABLES.contains(&header) {
        return Some("path");
    }
    (header == "package").then_some("build")
}

fn collect_pairs(
    table: tree_sitter::Node<'_>,
    source: &[u8],
    key: &str,
    display: &str,
    out: &mut Vec<ManifestLeaf>,
) {
    let mut cursor = table.walk();
    for pair in table.named_children(&mut cursor) {
        if pair.kind() != "pair" {
            continue;
        }
        let (Some(name), Some(value)) =
            (node_text(pair.named_child(0), source), pair.named_child(1))
        else {
            continue;
        };
        if name != key || value.kind() != "string" {
            continue;
        }
        let start = value.start_byte() as u32;
        out.push(ManifestLeaf {
            span: Span {
                start,
                len: value.end_byte() as u32 - start,
            },
            literal: String::from_utf8_lossy(&source[value.start_byte()..value.end_byte()])
                .to_string(),
            field: format!("{display}.{key}"),
        });
    }
}

/// A `[table]` / `[[table]]` header as its dotted key text.
fn table_key(table: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    let mut cursor = table.walk();
    let key = table
        .named_children(&mut cursor)
        .find(|child| matches!(child.kind(), "bare_key" | "dotted_key" | "quoted_key"))?;
    node_text(Some(key), source)
}

fn node_text(node: Option<tree_sitter::Node<'_>>, source: &[u8]) -> Option<String> {
    let node = node?;
    Some(String::from_utf8_lossy(&source[node.start_byte()..node.end_byte()]).to_string())
}

fn toml_bare(literal: &str) -> &str {
    let bytes = literal.as_bytes();
    let quoted =
        bytes.len() >= 2 && matches!(bytes[0], b'"' | b'\'') && bytes[bytes.len() - 1] == bytes[0];
    match quoted {
        true => &literal[1..literal.len() - 1],
        false => literal,
    }
}

fn quote_of(literal: &str) -> char {
    match literal.as_bytes().first() {
        Some(b'\'') => '\'',
        _ => '"',
    }
}

// ── the corpus view ─────────────────────────────────────────────────────────

/// Crate roots are mod-rs files and a `[[bin]] path` can put one anywhere, so
/// the manifests are read. ONE read per root per process, `ts_rehome::resolver`'s law.
fn crate_roots(cx: &MoveCx) -> &'static BTreeSet<String> {
    static CACHE: OnceLock<Mutex<BTreeMap<PathBuf, &'static BTreeSet<String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut held = match cache.lock() {
        Ok(held) => held,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(existing) = held.get(cx.root()) {
        return existing;
    }
    let mut roots: BTreeSet<String> = cx
        .files()
        .iter()
        .filter(|rel| auto_crate_root(rel))
        .cloned()
        .collect();
    for manifest in cx.files().iter().filter(|rel| is_manifest(rel)) {
        let Some(text) = cx.text(manifest) else {
            continue;
        };
        let package_dir = dirname(manifest);
        for leaf in manifest_leaves(&text) {
            roots.insert(join_rel(package_dir, toml_bare(&leaf.literal)));
        }
    }
    let leaked: &'static BTreeSet<String> = Box::leak(Box::new(roots));
    held.insert(cx.root().to_path_buf(), leaked);
    leaked
}

/// Cargo's target auto-discovery, as path shapes: the two library/binary roots,
/// the `src/bin` binaries, the integration/bench/example roots, the build script.
fn auto_crate_root(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    let (Some(last), Some(parent)) = (parts.last(), parts.iter().nth_back(1)) else {
        return parts.last() == Some(&"build.rs");
    };
    match *parent {
        "src" => matches!(*last, "lib.rs" | "main.rs"),
        "bin" | "tests" | "benches" | "examples" => last.ends_with(".rs"),
        _ => *last == "build.rs",
    }
}

/// The names a batch can be reached by: every moved file's stem, plus the
/// directory name of a moved `mod.rs`, which is the module name a decl spells.
fn moved_names(cx: &MoveCx, rehome: &dyn Rehome) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for old in cx.moved().keys() {
        if !crate::move_cx::owned_by(old, rehome) {
            continue;
        }
        let own = stem(old);
        if own == "mod" {
            names.insert(stem(dirname(old)));
        }
        names.insert(own);
    }
    names
}

/// Whether a file can name the batch at all. A superset filter: it never drops a
/// file that references a moved one, and a short module name admits most files.
fn carries_name(bytes: &[u8], names: &BTreeSet<String>) -> bool {
    names
        .iter()
        .any(|name| memchr::memmem::find(bytes, name.as_bytes()).is_some())
}

fn stem(rel: &str) -> String {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    name.rsplit_once('.')
        .map(|(head, _)| head)
        .unwrap_or(name)
        .to_string()
}
