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

        let mut refs = Vec::new();
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
