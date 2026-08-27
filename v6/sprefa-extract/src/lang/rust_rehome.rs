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
//!
//! OPT-IN STRATEGY, `cx.relocate_mod()` (v5 `src/rspath.rs` +
//! `lib.rs:1676 rust_mod_surgery`; v1 `crates/rs/src/lib.rs:270`). When the flag
//! is off, every answer below is byte-identical to the pre-flag impl. When it
//! is on, a moved file-level module whose stem survives and whose decl sits at
//! the top level of an unmoved parent: the `mod` decl is CUT from the old
//! parent, `pub mod a;` (or `mod a;`, when every referencing file ends up
//! inside the new directory) lands sorted among the new parent's own `mod`
//! items through a composed whole-file rewrite, and the direct-child spellings
//! `crate::a::...` / `super::a::...` grow the intermediate segment
//! (`util::`). Bare relative paths (`a::f()` inside the old parent) name the
//! child by shorthand and are NOT rewritten; an inline-block or renamed-file
//! move falls back to the `#[path]` default. The trait has no error channel,
//! so the missing-parent case exits through a plan-time panic carrying
//! `--relocate-mod`: planning completes before any soopy stage runs, so the
//! failure never lands a partial edit.

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

        let mut refs = Vec::new();
        // `(file, span)` names ONE replacement, and the slot scan can reach the
        // same byte twice (an expression nested in a type path), so the
        // relocate refs claim their spans once.
        let mut seen: BTreeSet<(String, u32)> = BTreeSet::new();
        for (rel, decl, target) in &resolved {
            if cx.destination(target).is_none() && cx.destination(rel).is_none() {
                continue;
            }
            refs.push(decl_ref(rel, decl, target));
        }
        for (rel, scan) in corpus.iter().zip(&scans) {
            let Some(scan) = scan else { continue };
            let moving = cx.destination(rel).is_some();
            for include in &scan.includes {
                let target = join_rel(dirname(rel), &include.value);
                if !cx.contains(&target) || (!moving && cx.destination(&target).is_none()) {
                    continue;
                }
                seen.insert((rel.to_string(), include.span.start));
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
            for slot in scan.slots.iter().filter(|_| cx.relocate_mod()) {
                let Some(target) = moved_modules.get(&slot.module) else {
                    continue;
                };
                let dir = dirname(cx.after(target));
                // A file that ends up inside the new directory keeps its
                // spelling; one outside needs the intermediate segment.
                if dir.is_empty() || module_dir(dirname(cx.after(rel)), roots) == dir {
                    continue;
                }
                if !seen.insert((rel.to_string(), slot.span.start)) {
                    continue;
                }
                refs.push(ImportRef {
                    importer: rel.to_string(),
                    literal: slot.span,
                    text: String::new(),
                    target: cx.after(target).to_string(),
                    kind: "relocate_slot",
                });
            }
        }
        if cx.relocate_mod() {
            relocate_plan(cx, &roots, &resolved, &refs);
            // One composed-rewrite ref per touched file: it claims the whole
            // file, so nothing else on that file survives.
            let changed: Vec<String> = view_relocations(cx, |plan| {
                plan.rewrites
                    .iter()
                    .filter(|(file, text)| cx.text(file).as_deref().is_some_and(|c| c != *text))
                    .map(|(file, _)| file.clone())
                    .collect()
            });
            for file in changed {
                refs.push(ImportRef {
                    importer: file,
                    literal: Span { start: 0, len: 0 },
                    text: String::new(),
                    target: String::new(),
                    kind: "relocate_insert",
                });
            }
            suppress_in_rewritten(cx, &mut refs);
        }
        tracing::debug!(corpus = corpus.len(), refs = refs.len(), "move rust refs");
        refs
    }

    fn respell(&self, cx: &MoveCx, reference: &ImportRef) -> Option<Respell> {
        match reference.kind {
            // The composed rewrite is planned as one whole-file edit; the cut
            // and the landing line can share the file the bounded spans of
            // other arms would collide with.
            "relocate_insert" => return insert_respell(cx, reference),
            "relocate_slot" => return slot_respell(reference),
            "mod_decl" | "path_attr" if cx.relocate_mod() => {
                if let Some(cut) = removal_respell(cx, reference) {
                    return Some(cut);
                }
                if relocated_files(cx).contains(&reference.importer) {
                    return None;
                }
            }
            _ => {}
        }
        let (text, receipt) = match reference.kind {
            "mod_decl" | "path_attr" => (mod_respell(cx, reference)?, None),
            "include" => (include_respell(cx, reference)?, None),
            // The module tree names a module, never its path on disk, and this
            // arm keeps that tree by writing `#[path]`, never by re-parenting.
            "use_path" => return None,
            "manifest_target" => manifest_respell(cx, reference)?,
            _ => return None,
        };
        (text != reference.text).then(|| Respell {
            file: reference.importer.clone(),
            span: reference.literal,
            text,
            receipt,
        })
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
    slots: Vec<PathSlot>,
}

/// One mid-path position a `--relocate-mod` respell may fill: the segment
/// directly after a crate- or super-rooted head (`crate::|a`, here), wherever
/// the path sits — a `use` tree, an expression, or a type.
struct PathSlot {
    module: String,
    /// Zero-length at the segment's first byte; the respell splices
    /// `<intermediate>::` in front of it.
    span: Span,
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
    let mut includes = IncludeScan {
        source: text,
        line_starts: &line_starts,
        out: Vec::new(),
    };
    syn::visit::Visit::visit_file(&mut includes, &parsed);
    scan.includes = includes.out;
    let mut segments = SegmentScan {
        line_starts: &line_starts,
        out: &mut scan.slots,
    };
    syn::visit::Visit::visit_file(&mut segments, &parsed);
    Some(scan)
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
                collect_use_slots(&use_item.tree, &[], line_starts, &mut out.slots);
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

/// The crate- or super-rooted direct-child segments of one `use` tree. A glob
/// or leaf binds no mid-path position.
fn collect_use_slots(
    tree: &syn::UseTree,
    prefix: &[String],
    line_starts: &[u32],
    out: &mut Vec<PathSlot>,
) {
    match tree {
        syn::UseTree::Path(segment) => {
            if matches!(prefix, [head] if head == "crate" || head == "super") {
                // Zero-length at the ident's first byte: the respell splices
                // ahead of it.
                let start = syn_span(line_starts, segment.ident.span());
                out.push(PathSlot {
                    module: segment.ident.to_string(),
                    span: Span {
                        start: start.start,
                        len: 0,
                    },
                });
            }
            let mut deeper = prefix.to_vec();
            deeper.push(segment.ident.to_string());
            collect_use_slots(&segment.tree, &deeper, line_starts, out);
        }
        syn::UseTree::Group(group) => {
            for member in &group.items {
                collect_use_slots(member, prefix, line_starts, out);
            }
        }
        _ => {}
    }
}

/// The same positions in expression and type paths: a bare visit reaches both,
/// and macro token streams expand to nothing a visitor can see.
struct SegmentScan<'a> {
    line_starts: &'a [u32],
    out: &'a mut Vec<PathSlot>,
}

impl SegmentScan<'_> {
    fn consider(&mut self, path: &syn::Path) {
        if path.segments.len() < 2 {
            return;
        }
        let head = path.segments[0].ident.to_string();
        if head != "crate" && head != "super" {
            return;
        }
        let recorded = syn_span(self.line_starts, path.segments[1].ident.span());
        self.out.push(PathSlot {
            module: path.segments[1].ident.to_string(),
            span: Span {
                start: recorded.start,
                len: 0,
            },
        });
    }
}

impl<'ast> syn::visit::Visit<'ast> for SegmentScan<'_> {
    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        if node.qself.is_none() {
            self.consider(&node.path);
        }
        syn::visit::visit_expr_path(self, node);
    }

    fn visit_type_path(&mut self, node: &'ast syn::TypePath) {
        if node.qself.is_none() {
            self.consider(&node.path);
        }
        syn::visit::visit_type_path(self, node);
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

// ── the --relocate-mod strategy ─────────────────────────────────────────────

/// One composed rewrite per touched file plus, separately, the top-level
/// decls whose bytes a cut removes from a file that gets no rewrite. When one
/// file is both a departure and a destination the cut folds into the rewrite,
/// and every other ref into that file is suppressed in exchange. Process-local
/// because the roster impl carries no state of its own (`crate_roots` above is
/// the same precedent).
#[derive(Default)]
struct Relocations {
    cuts: BTreeMap<String, BTreeSet<u32>>,
    rewrites: BTreeMap<String, String>,
}

fn edit_relocations<R>(cx: &MoveCx, f: impl FnOnce(&mut Relocations) -> R) -> R {
    static PLANS: OnceLock<Mutex<BTreeMap<PathBuf, Relocations>>> = OnceLock::new();
    let plans = PLANS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut held = match plans.lock() {
        Ok(held) => held,
        Err(poisoned) => poisoned.into_inner(),
    };
    f(held.entry(cx.root().to_path_buf()).or_default())
}

fn view_relocations<R>(cx: &MoveCx, read: impl FnOnce(&Relocations) -> R) -> R {
    edit_relocations(cx, |plan| read(plan))
}

fn relocated_files(cx: &MoveCx) -> Vec<String> {
    view_relocations(cx, |plan| plan.rewrites.keys().cloned().collect())
}

/// Decides the whole strategy up front, at plan time: which decls leave, where
/// each lands, and the one whole-file rewrite per touched file that carries
/// both. The slot refs are already on `refs`, so a module referenced from
/// outside its new directory reads as public.
fn relocate_plan(
    cx: &MoveCx,
    roots: &BTreeSet<String>,
    resolved: &[(&str, &ModDecl, String)],
    refs: &[ImportRef],
) {
    let outward: BTreeSet<String> = refs
        .iter()
        .filter(|reference| reference.kind == "relocate_slot")
        .map(|reference| stem(&reference.target))
        .collect();

    // file -> (departure cut ranges, landing lines into its mod items)
    let mut work: BTreeMap<String, (Vec<(u32, u32)>, Vec<(String, bool)>)> = BTreeMap::new();
    for (rel, decl, target) in resolved {
        let Some(dest) = cx.destination(target) else {
            continue;
        };
        let Some(landing) = landing_of(cx, roots, rel, decl, dest) else {
            continue;
        };
        work.entry(rel.to_string())
            .or_default()
            .0
            .push((decl.item.start, extended_end(cx, rel, &decl.item)));
        let dir = dirname(dest);
        let Some(parent) = parent_decl_file(cx, dir) else {
            panic!(
                "--relocate-mod: no parent module file for {dest}; expected {} or {}",
                join_rel(dir, "mod.rs"),
                join_rel(
                    dirname(dir),
                    &format!("{}.rs", dir.rsplit('/').next().unwrap_or(dir))
                )
            );
        };
        let visible = outward.contains(&landing);
        work.entry(parent).or_default().1.push((landing, visible));
    }

    edit_relocations(cx, |plan| {
        for (file, (cuts, landings)) in work {
            let Some(source) = cx.text(&file) else {
                continue;
            };
            // A departure that also receives landings folds into the whole-file
            // rewrite; every other ref into the file gives way to it.
            match (!cuts.is_empty(), !landings.is_empty()) {
                (_, true) => {
                    let text = place_landings(&compose_cuts(source.clone(), &cuts), &landings);
                    if text != source {
                        plan.rewrites.insert(file, text);
                    }
                }
                (true, false) => {
                    let starts: BTreeSet<u32> = cuts.into_iter().map(|(start, _)| start).collect();
                    plan.cuts.insert(file, starts);
                }
                (false, false) => {}
            }
        }
    });
}

fn landing_of(
    cx: &MoveCx,
    roots: &BTreeSet<String>,
    rel: &str,
    decl: &ModDecl,
    dest: &str,
) -> Option<String> {
    let name = stem(dest);
    if name != decl.name || name == "mod" || !decl.chain.is_empty() || !basename_is_rust_file(dest)
    {
        // An inline block, a rename, or a `mod.rs` destination keeps the
        // `#[path]` default.
        return None;
    }
    if cx.destination(rel).is_some() {
        return None;
    }
    if natural_paths(&decl_base(rel, &decl.chain, roots), &decl.name).contains(&dest.to_string()) {
        return None;
    }
    Some(decl.name.clone())
}

fn basename_is_rust_file(path: &str) -> bool {
    matches!(path.rsplit('/').next(), Some(name) if name.ends_with(".rs") && name != "mod.rs")
}

/// Cut bytes with one trailing newline folded in, so the item's blank line
/// leaves with it.
fn extended_end(cx: &MoveCx, file: &str, span: &Span) -> u32 {
    let end = span.start + span.len;
    match std::fs::read(cx.abs(file))
        .ok()
        .and_then(|bytes| bytes.get(end as usize).copied())
    {
        Some(b'\n') => end + 1,
        _ => end,
    }
}

/// Removes the cut ranges from a copy of the source, deepest range last.
fn compose_cuts(mut source: String, cuts: &[(u32, u32)]) -> String {
    let mut sorted = cuts.to_vec();
    sorted.sort();
    for (start, end) in sorted.into_iter().rev() {
        source.replace_range(start as usize..end as usize, "");
    }
    source
}

/// `pub mod a;` / `mod a;`, by the visibility decision made at plan time.
fn landing_line(name: &str, visible: bool) -> String {
    format!("{}mod {name};", if visible { "pub " } else { "" })
}

/// Splices each landing line before the first surviving `mod` item that sorts
/// after it; names greater than everything on file go to the end, so the
/// landing lines stay sorted among the existing items.
fn place_landings(source: &str, landings: &[(String, bool)]) -> String {
    let Some(scan) = scan_file(source) else {
        return source.to_string();
    };
    let mut items: Vec<&ModDecl> = scan.decls.iter().filter(|d| d.chain.is_empty()).collect();
    items.sort_by_key(|item| item.item.start);
    let pending = &mut landings.to_vec();
    pending.sort_by(|left, right| left.0.cmp(&right.0));
    let mut out = String::with_capacity(source.len() + 64);
    let mut cursor = 0usize;
    for item in items {
        let taken = pending.partition_point(|(name, _)| *name < item.name);
        if taken > 0 {
            out.push_str(&source[cursor..item.item.start as usize]);
            for (name, visible) in &pending[..taken] {
                out.push_str(&landing_line(name, *visible));
                out.push('\n');
            }
            cursor = item.item.start as usize;
            pending.drain(..taken);
        }
    }
    out.push_str(&source[cursor..]);
    if !pending.is_empty() {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        for (name, visible) in pending {
            out.push_str(&landing_line(name, *visible));
            out.push('\n');
        }
    }
    out
}

/// The corpus file whose children resolve against `dir`: its own `mod.rs`,
/// else any crate-root shape rustc puts there (`lib.rs`, `main.rs`), else the
/// unique remaining owner — two owning files would not compile anyway.
fn parent_decl_file(cx: &MoveCx, dir: &str) -> Option<String> {
    let direct = join_rel(dir, "mod.rs");
    if cx.contains(&direct) {
        return Some(direct);
    }
    let roots = crate_roots(cx);
    let mut owners: Vec<String> = cx
        .files()
        .iter()
        .filter(|rel| module_dir(rel, roots) == dir)
        .map(|rel| rel.to_string())
        .collect();
    owners.sort_by_key(|rel| match basename(rel).as_str() {
        "lib.rs" => 0,
        "main.rs" => 1,
        _ => 2,
    });
    match owners.len() {
        1 => owners.pop(),
        _ => None,
    }
}

fn basename(rel: &str) -> String {
    rel.rsplit('/').next().unwrap_or(rel).to_string()
}

/// A file carrying a composed rewrite keeps every other ref out: no bounded
/// span can share a file with the whole-file Replace.
fn suppress_in_rewritten(cx: &MoveCx, refs: &mut Vec<ImportRef>) {
    let files = relocated_files(cx);
    if files.is_empty() {
        return;
    }
    refs.retain(|reference| {
        reference.kind == "relocate_insert" || !files.contains(&reference.importer)
    });
}

/// The composed rewrite consumes every other claim on the file, so it answers
/// once per surviving ref.
fn insert_respell(cx: &MoveCx, reference: &ImportRef) -> Option<Respell> {
    view_relocations(cx, |plan| {
        let written = plan.rewrites.get(&reference.importer)?;
        let current = cx.text(&reference.importer)?;
        (written != &current).then(|| Respell {
            file: reference.importer.clone(),
            span: Span {
                start: 0,
                len: current.len() as u32,
            },
            text: written.clone(),
            receipt: None,
        })
    })
}

/// `<intermediate>::` spliced at the recorded segment start.
fn slot_respell(reference: &ImportRef) -> Option<Respell> {
    let segment = dirname(&reference.target).rsplit('/').next()?;
    Some(Respell {
        file: reference.importer.clone(),
        span: reference.literal,
        text: format!("{segment}::"),
        receipt: None,
    })
}

/// A cut-listed decl leaves through one Replace spanning it plus its newline.
/// Returns None when this decl was planned otherwise or left alone, so the
/// caller falls through to the `#[path]` default.
fn removal_respell(cx: &MoveCx, reference: &ImportRef) -> Option<Respell> {
    view_relocations(cx, |plan| {
        let starts = plan.cuts.get(&reference.importer)?;
        let decl = decl_at(cx, reference)?;
        if !starts.contains(&decl.item.start) {
            return None;
        }
        let end = extended_end(cx, &reference.importer, &decl.item);
        Some(Respell {
            file: reference.importer.clone(),
            span: Span {
                start: decl.item.start,
                len: end - decl.item.start,
            },
            text: String::new(),
            receipt: None,
        })
    })
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
