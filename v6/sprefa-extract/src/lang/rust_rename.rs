//! `impl Rename for RustSource`: every question `extract rename` asks a language,
//! answered for Rust over `syn`, the parse `lang/rust.rs` already carries. Spans
//! are identifier spans bridged by `build_line_starts` (`rust.rs:57`) and
//! `syn_span` (`rust.rs:81`); no new crate.
//! @comment-ok: module header, the seam list every lang file opens with
//!
//! rustc's module-file law (crate roots, `mod.rs` owning its directory, a module
//! path read off the layout) is restated here rather than shared: the same law
//! sits in `rust_rehome.rs:685-1300` behind a `MoveCx`, and a rename carries a
//! `RenameCx`.
//!
//! Three seats are reported and never rewritten, because rewriting one guesses:
//! a glob `use m::*` that puts the symbol in a scope which then writes the bare
//! name, an identifier token inside a macro or attribute body, and any span whose
//! bytes do not read back as the old name (`syn_span` bridges a proc_macro2 CHAR
//! column, so a non-ASCII byte earlier on the line shifts it).
//!
//! Two limits this arm states rather than hides. A `use` written inside a block
//! counts against the enclosing MODULE scope, which can only drop a seat, never
//! invent one. A method seat is matched by name: the receiver's type is not
//! inferred, so `x.old()` on an unrelated type is respelled too, and only a
//! request whose anchor declaration IS a method reaches that code at all.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use syn::spanned::Spanned;

use super::rust::{build_line_starts, syn_span, RustSource};
use crate::move_cx::{dirname, join_rel, stem};
use crate::rename_cx::{RenameCx, RenameRequest};
use crate::types::{RefRole, Rename, RenameStop, Respell, Span, SymbolRef, SymbolSeat};

impl Rename for RustSource {
    fn symbol_refs(
        &self,
        cx: &RenameCx,
        request: &RenameRequest,
    ) -> Result<Vec<SymbolRef>, RenameStop> {
        let corpus = Corpus::open(cx, &request.old);
        let anchor = corpus
            .scans
            .get(&request.anchor)
            .ok_or_else(|| not_found(request))?;
        // An item at the anchor's own module root is the one a `use` can reach;
        // an item nested in a `mod` block or a function body needs `--at`.
        let at_root: Vec<&Decl> = anchor
            .decls
            .iter()
            .filter(|decl| decl.chain.is_empty() && decl.block.is_none())
            .collect();
        let declaration = match (request.at, at_root.as_slice(), anchor.decls.as_slice()) {
            (_, _, []) => return Err(not_found(request)),
            (None, [one], _) => *one,
            (None, [], [one]) => one,
            (None, [], many) => {
                return Err(ambiguous(
                    request,
                    many.iter().map(|decl| decl.span).collect(),
                ))
            }
            (None, many, _) => {
                return Err(ambiguous(
                    request,
                    many.iter().map(|decl| decl.span).collect(),
                ))
            }
            (Some(_), _, many) => select_by_at(many, request.at)
                .ok_or_else(|| ambiguous(request, many.iter().map(|decl| decl.span).collect()))?,
        };
        let home = corpus.home(&request.anchor);
        let nameable = corpus.nameable(module_of(home, &declaration.chain));

        let mut refs = vec![SymbolRef {
            file: request.anchor.clone(),
            span: declaration.span,
            role: RefRole::Definition,
            text: request.old.clone(),
        }];
        let mut seats: Vec<SymbolSeat> = Vec::new();
        for (rel, scan) in &corpus.scans {
            let anchored = (rel == &request.anchor).then_some(declaration);
            corpus.harvest(
                rel, scan, &nameable, anchored, request, &mut refs, &mut seats,
            );
        }
        if let Some(stop) = corpus.inexact(&refs) {
            return Err(stop);
        }
        if !seats.is_empty() {
            seats.sort_by(|left, right| {
                left.file
                    .cmp(&right.file)
                    .then(left.span.start.cmp(&right.span.start))
            });
            seats.dedup_by(|left, right| left.file == right.file && left.span == right.span);
            return Err(RenameStop::Dynamic(seats));
        }
        Ok(settle(refs))
    }

    fn respell_symbol(
        &self,
        _cx: &RenameCx,
        request: &RenameRequest,
        reference: &SymbolRef,
    ) -> Option<Respell> {
        Some(Respell {
            file: reference.file.clone(),
            span: reference.span,
            text: request.new.clone(),
            receipt: None,
        })
    }
}

fn not_found(request: &RenameRequest) -> RenameStop {
    RenameStop::NotFound {
        anchor: request.anchor.clone(),
        old: request.old.clone(),
    }
}

fn ambiguous(request: &RenameRequest, sites: Vec<Span>) -> RenameStop {
    RenameStop::Ambiguous {
        anchor: request.anchor.clone(),
        old: request.old.clone(),
        sites,
    }
}

/// `--at` picks the declaration the offset lands in, else the nearest one
/// opening at or before it, the law `ts_rename.rs:111` states.
fn select_by_at(candidates: &[Decl], at: Option<u32>) -> Option<&Decl> {
    let at = at?;
    let inside: Vec<&Decl> = candidates
        .iter()
        .filter(|decl| decl.span.start <= at && at < decl.span.end())
        .collect();
    match inside.as_slice() {
        [one] => Some(one),
        [] => candidates
            .iter()
            .filter(|decl| decl.span.start <= at)
            .max_by_key(|decl| decl.span.start),
        _ => None,
    }
}

/// One seat per `(file, offset)`, in plan order: a `use` clause's trailing
/// segment and the binding walk can name the same token.
fn settle(mut refs: Vec<SymbolRef>) -> Vec<SymbolRef> {
    refs.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then(left.span.start.cmp(&right.span.start))
    });
    refs.dedup_by(|left, right| left.file == right.file && left.span.start == right.span.start);
    refs
}

// ── the corpus view ─────────────────────────────────────────────────────────

/// One module: the crate-root file it answers to, and its path from that root.
type ModuleId = (String, Vec<String>);

fn module_of(home: &ModuleId, chain: &[String]) -> ModuleId {
    let mut path = home.1.clone();
    path.extend(chain.iter().cloned());
    (home.0.clone(), path)
}

/// Every Rust file that spells the old name, scanned once, plus the two layout
/// tables the module law reads.
struct Corpus {
    scans: BTreeMap<String, FileScan>,
    /// rel -> the module that file IS.
    homes: BTreeMap<String, ModuleId>,
    /// A crate's identifier as a `use` writes it -> that crate's root file.
    crates: BTreeMap<String, String>,
}

impl Corpus {
    fn open(cx: &RenameCx, old: &str) -> Self {
        let roots = crate_roots(cx);
        let crates = crate_idents(cx);
        let mut scans = BTreeMap::new();
        let mut homes = BTreeMap::new();
        for rel in cx.files_of(&RustSource) {
            let Some(text) = cx.text(rel) else {
                continue;
            };
            // A file that never spells the name seats it nowhere, and a re-export
            // chain writes the name at every hop, so the filter drops no seat.
            if !text.contains(old) {
                continue;
            }
            let Ok(parsed) = syn::parse_file(&text) else {
                continue;
            };
            let line_starts = build_line_starts(&text);
            let mut scan = Scan {
                old,
                source: &text,
                line_starts: &line_starts,
                chain: Vec::new(),
                blocks: Vec::new(),
                role: RefRole::TypeRef,
                out: FileScan::default(),
            };
            syn::visit::Visit::visit_file(&mut scan, &parsed);
            homes.insert(rel.to_string(), module_path(rel, &roots));
            scans.insert(rel.to_string(), scan.out);
        }
        Corpus {
            scans,
            homes,
            crates,
        }
    }

    /// The module a file is. A file under no crate root answers to itself, so its
    /// own paths still resolve against each other.
    fn home(&self, rel: &str) -> &ModuleId {
        static ORPHAN: ModuleId = (String::new(), Vec::new());
        self.homes.get(rel).unwrap_or(&ORPHAN)
    }

    /// The `::`-prefix of a path, as a module. `None` when it climbs above a
    /// crate root, which names nothing.
    fn resolve(&self, home: &ModuleId, chain: &[String], prefix: &[String]) -> Option<ModuleId> {
        let mut here: Vec<String> = home.1.iter().chain(chain).cloned().collect();
        let mut rest = prefix;
        let mut root = home.0.clone();
        match rest.first().map(String::as_str) {
            Some("crate") => {
                here.clear();
                rest = &rest[1..];
            }
            Some("self") => {
                rest = &rest[1..];
            }
            Some("super") => {
                while rest.first().map(String::as_str) == Some("super") {
                    here.pop()?;
                    rest = &rest[1..];
                }
            }
            Some(name) if self.crates.contains_key(name) => {
                here.clear();
                root = self.crates.get(name)?.clone();
                rest = &rest[1..];
            }
            _ => {}
        }
        here.extend(rest.iter().cloned());
        Some((root, here))
    }

    /// Every module the symbol can be named from: the declaring one, plus a hop
    /// per public re-export under the SAME name, to a fixpoint.
    fn nameable(&self, anchor: ModuleId) -> BTreeSet<ModuleId> {
        let mut set = BTreeSet::new();
        set.insert(anchor);
        loop {
            let mut grew = false;
            for (rel, scan) in &self.scans {
                let home = self.home(rel);
                for leaf in &scan.uses {
                    if !leaf.exported || !matches!(leaf.kind, LeafKind::Name) {
                        continue;
                    }
                    let Some(from) = self.resolve(home, &leaf.chain, &leaf.prefix) else {
                        continue;
                    };
                    if set.contains(&from) {
                        grew |= set.insert(module_of(home, &leaf.chain));
                    }
                }
            }
            if !grew {
                return set;
            }
        }
    }

    /// One file's seats. `anchored` carries the declaration when this file is the
    /// anchor, so its own scope binds the name and its own ident is not a shadow.
    #[allow(clippy::too_many_arguments)]
    fn harvest(
        &self,
        rel: &str,
        scan: &FileScan,
        nameable: &BTreeSet<ModuleId>,
        anchored: Option<&Decl>,
        request: &RenameRequest,
        refs: &mut Vec<SymbolRef>,
        seats: &mut Vec<SymbolSeat>,
    ) {
        let home = self.home(rel);
        let mut ours: BTreeSet<&[String]> = BTreeSet::new();
        let mut shadowed: BTreeSet<&[String]> = BTreeSet::new();
        let mut shadow_blocks: Vec<Span> = Vec::new();
        let mut globs: BTreeMap<&[String], Vec<Span>> = BTreeMap::new();

        for decl in &scan.decls {
            match (anchored, decl.block) {
                (Some(picked), _) if picked.span == decl.span => {
                    ours.insert(&picked.chain);
                }
                (_, Some(block)) => shadow_blocks.push(block),
                (_, None) => {
                    shadowed.insert(&decl.chain);
                }
            }
        }
        let inside_shadow_block = |span: Span| {
            shadow_blocks
                .iter()
                .any(|block| block.start <= span.start && span.end() <= block.end())
        };
        for leaf in &scan.uses {
            let reaches = self
                .resolve(home, &leaf.chain, &leaf.prefix)
                .is_some_and(|module| nameable.contains(&module));
            match leaf.kind {
                LeafKind::Name if reaches => {
                    ours.insert(&leaf.chain);
                    refs.push(seat(rel, leaf.span, RefRole::Import, &request.old));
                }
                LeafKind::Name => {
                    shadowed.insert(&leaf.chain);
                }
                LeafKind::Alias if reaches => {
                    refs.push(seat(rel, leaf.span, RefRole::Import, &request.old))
                }
                LeafKind::Shadow => {
                    shadowed.insert(&leaf.chain);
                }
                LeafKind::Glob if reaches => globs.entry(&leaf.chain).or_default().push(leaf.item),
                _ => {}
            }
        }

        for path in &scan.paths {
            if !path.prefix.is_empty() {
                if self
                    .resolve(home, &path.chain, &path.prefix)
                    .is_some_and(|module| nameable.contains(&module))
                {
                    refs.push(seat(rel, path.span, path.role, &request.old));
                }
                continue;
            }
            if shadowed.contains(path.chain.as_slice()) || inside_shadow_block(path.span) {
                continue;
            }
            if ours.contains(path.chain.as_slice()) {
                refs.push(seat(rel, path.span, path.role, &request.old));
                continue;
            }
            // A glob whose scope never writes the bare name survives the rename
            // untouched, so only a scope that writes it stops.
            for span in globs.get(path.chain.as_slice()).into_iter().flatten() {
                seats.push(SymbolSeat {
                    file: rel.to_string(),
                    span: *span,
                    form: "glob import",
                });
            }
        }

        if ours.is_empty() {
            return;
        }
        for span in &scan.opaque {
            seats.push(SymbolSeat {
                file: rel.to_string(),
                span: *span,
                form: "macro body",
            });
        }
        if anchored.is_some_and(|decl| decl.method) {
            for span in &scan.methods {
                refs.push(seat(rel, *span, RefRole::Read, &request.old));
            }
        }
    }

    /// The first span in a touched file whose bytes are not the old name. One
    /// such span means every span in that file is suspect, so the run stops.
    fn inexact(&self, refs: &[SymbolRef]) -> Option<RenameStop> {
        let touched: BTreeSet<&str> = refs
            .iter()
            .map(|reference| reference.file.as_str())
            .collect();
        touched
            .into_iter()
            .find_map(|rel| Some((rel, *self.scans.get(rel)?.inexact.first()?)))
            .map(|(rel, span)| RenameStop::Inexact {
                file: rel.to_string(),
                span,
                why: "a syn char column does not read back as the identifier",
            })
    }
}

fn seat(rel: &str, span: Span, role: RefRole, old: &str) -> SymbolRef {
    SymbolRef {
        file: rel.to_string(),
        span,
        role,
        text: old.to_string(),
    }
}

// ── the syn scan ────────────────────────────────────────────────────────────

/// One file's old-name seats, off ONE `syn::parse_file`.
#[derive(Default)]
struct FileScan {
    decls: Vec<Decl>,
    uses: Vec<UseLeaf>,
    paths: Vec<PathSeat>,
    /// `x.old()` receivers, kept for a request whose anchor IS a method.
    methods: Vec<Span>,
    /// The name written as an identifier token inside a macro or attribute body.
    opaque: Vec<Span>,
    inexact: Vec<Span>,
}

/// One declaration of the name: the item ident's own span, and the inline
/// `mod x { .. }` blocks enclosing it, outermost first.
struct Decl {
    chain: Vec<String>,
    span: Span,
    /// Declared in an `impl` or `trait` block, so call sites spell it as a method.
    method: bool,
    /// The innermost block a function-body item is declared in: it shadows the
    /// name inside that block only. None = declared at module scope.
    block: Option<Span>,
}

/// One `use` clause naming the symbol, or a glob that could reach it.
struct UseLeaf {
    chain: Vec<String>,
    /// The `::`-segments before the named one; a glob's is the whole starred path.
    prefix: Vec<String>,
    /// The named identifier's own span; a glob names none.
    span: Span,
    kind: LeafKind,
    /// The whole `use` item, which is what a glob stop reports.
    item: Span,
    exported: bool,
}

enum LeafKind {
    /// `use P::OLD;` names the symbol and binds `OLD` here.
    Name,
    /// `use P::OLD as local;` names the symbol and binds `local`.
    Alias,
    /// `use P::other as OLD;` binds the name to something else.
    Shadow,
    /// `use P::*;`
    Glob,
}

/// One path segment spelling the name, and the segments before it.
struct PathSeat {
    chain: Vec<String>,
    prefix: Vec<String>,
    span: Span,
    role: RefRole,
}

struct Scan<'a> {
    old: &'a str,
    source: &'a str,
    line_starts: &'a [u32],
    chain: Vec<String>,
    blocks: Vec<Span>,
    role: RefRole,
    out: FileScan,
}

impl Scan<'_> {
    /// A span that reads back as the old name, else a recorded `inexact`.
    fn exact(&mut self, span: proc_macro2::Span) -> Option<Span> {
        let span = syn_span(self.line_starts, span);
        let start = span.start as usize;
        match self.source.get(start..start + span.len as usize) {
            Some(text) if text == self.old => Some(span),
            _ => {
                self.out.inexact.push(span);
                None
            }
        }
    }

    fn declare(&mut self, ident: &proc_macro2::Ident, method: bool) {
        if ident != self.old {
            return;
        }
        let Some(span) = self.exact(ident.span()) else {
            return;
        };
        self.out.decls.push(Decl {
            chain: self.chain.clone(),
            span,
            method,
            block: self.blocks.last().copied(),
        });
    }

    /// Identifier tokens in a stream no rewrite may enter. The span is reported,
    /// never replaced, so it is not held to the `exact` check.
    fn tokens(&mut self, stream: proc_macro2::TokenStream) {
        for tree in stream {
            match tree {
                proc_macro2::TokenTree::Ident(ident) if ident == self.old => {
                    let span = syn_span(self.line_starts, ident.span());
                    self.out.opaque.push(span);
                }
                proc_macro2::TokenTree::Group(group) => self.tokens(group.stream()),
                _ => {}
            }
        }
    }
}

impl<'ast> syn::visit::Visit<'ast> for Scan<'_> {
    fn visit_item(&mut self, node: &'ast syn::Item) {
        if let Some(ident) = item_ident(node) {
            self.declare(ident, false);
        }
        syn::visit::visit_item(self, node);
    }

    fn visit_block(&mut self, node: &'ast syn::Block) {
        self.blocks
            .push(syn_span(self.line_starts, node.brace_token.span.join()));
        syn::visit::visit_block(self, node);
        self.blocks.pop();
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        self.declare(&node.ident, false);
        match node.content.is_some() {
            true => {
                self.chain.push(node.ident.to_string());
                syn::visit::visit_item_mod(self, node);
                self.chain.pop();
            }
            false => syn::visit::visit_item_mod(self, node),
        }
    }

    fn visit_impl_item(&mut self, node: &'ast syn::ImplItem) {
        match node {
            syn::ImplItem::Fn(item) => self.declare(&item.sig.ident, true),
            syn::ImplItem::Const(item) => self.declare(&item.ident, false),
            syn::ImplItem::Type(item) => self.declare(&item.ident, false),
            _ => {}
        }
        syn::visit::visit_impl_item(self, node);
    }

    fn visit_trait_item(&mut self, node: &'ast syn::TraitItem) {
        match node {
            syn::TraitItem::Fn(item) => self.declare(&item.sig.ident, true),
            syn::TraitItem::Const(item) => self.declare(&item.ident, false),
            syn::TraitItem::Type(item) => self.declare(&item.ident, false),
            _ => {}
        }
        syn::visit::visit_trait_item(self, node);
    }

    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        // `use ::krate::..` names a crate by an absolute spelling this walk does
        // not carry; a relative reading of it would resolve to another module.
        if node.leading_colon.is_some() {
            return;
        }
        let exported = !matches!(node.vis, syn::Visibility::Inherited);
        let item = syn_span(self.line_starts, node.span());
        let mut branches = Vec::new();
        use_branches(&node.tree, &mut Vec::new(), &mut branches);
        for branch in branches {
            let named = match branch.kind {
                LeafKind::Glob => branch.idents.len(),
                _ => branch.idents.len().saturating_sub(1),
            };
            for (index, ident) in branch.idents.iter().enumerate().take(named) {
                if ident != self.old {
                    continue;
                }
                let Some(span) = self.exact(branch.spans[index]) else {
                    continue;
                };
                self.out.paths.push(PathSeat {
                    chain: self.chain.clone(),
                    prefix: branch.idents[..index].to_vec(),
                    span,
                    role: RefRole::Import,
                });
            }
            let names_it = branch.idents.get(named).map(String::as_str) == Some(self.old);
            if names_it {
                let Some(span) = self.exact(branch.spans[named]) else {
                    continue;
                };
                self.out.uses.push(UseLeaf {
                    chain: self.chain.clone(),
                    prefix: branch.idents[..named].to_vec(),
                    span,
                    kind: branch.kind,
                    item,
                    exported,
                });
                continue;
            }
            let kind = match branch.binds.as_deref() == Some(self.old) {
                true => LeafKind::Shadow,
                false => match branch.kind {
                    LeafKind::Glob => LeafKind::Glob,
                    _ => continue,
                },
            };
            self.out.uses.push(UseLeaf {
                chain: self.chain.clone(),
                prefix: branch.idents.clone(),
                span: Span::empty(),
                kind,
                item,
                exported,
            });
        }
    }

    fn visit_path(&mut self, node: &'ast syn::Path) {
        if node.leading_colon.is_none() {
            let idents: Vec<String> = node
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect();
            for (index, segment) in node.segments.iter().enumerate() {
                if segment.ident != self.old {
                    continue;
                }
                let Some(span) = self.exact(segment.ident.span()) else {
                    continue;
                };
                self.out.paths.push(PathSeat {
                    chain: self.chain.clone(),
                    prefix: idents[..index].to_vec(),
                    span,
                    role: self.role,
                });
            }
        }
        syn::visit::visit_path(self, node);
    }

    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        let held = std::mem::replace(&mut self.role, RefRole::Read);
        syn::visit::visit_expr_path(self, node);
        self.role = held;
    }

    fn visit_type_path(&mut self, node: &'ast syn::TypePath) {
        let held = std::mem::replace(&mut self.role, RefRole::TypeRef);
        syn::visit::visit_type_path(self, node);
        self.role = held;
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if node.method == self.old {
            if let Some(span) = self.exact(node.method.span()) {
                self.out.methods.push(span);
            }
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    /// A macro body is tokens, not a scope the plane binds; the walk stops at the
    /// invocation and reports what it spells.
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        self.tokens(node.tokens.clone());
    }

    /// An attribute's arguments are tokens too, and its own path is a lint or
    /// derive name, never this symbol.
    fn visit_attribute(&mut self, node: &'ast syn::Attribute) {
        if let syn::Meta::List(list) = &node.meta {
            self.tokens(list.tokens.clone());
        }
    }
}

fn item_ident(item: &syn::Item) -> Option<&proc_macro2::Ident> {
    match item {
        syn::Item::Struct(item) => Some(&item.ident),
        syn::Item::Enum(item) => Some(&item.ident),
        syn::Item::Union(item) => Some(&item.ident),
        syn::Item::Trait(item) => Some(&item.ident),
        syn::Item::TraitAlias(item) => Some(&item.ident),
        syn::Item::Type(item) => Some(&item.ident),
        syn::Item::Fn(item) => Some(&item.sig.ident),
        syn::Item::Const(item) => Some(&item.ident),
        syn::Item::Static(item) => Some(&item.ident),
        _ => None,
    }
}

/// One flattened `use` branch: the segments as written, and what the last one
/// does. A `Glob` branch ends at the module it stars and binds no name.
struct UseBranch {
    idents: Vec<String>,
    spans: Vec<proc_macro2::Span>,
    kind: LeafKind,
    /// The name this branch binds in the writing scope.
    binds: Option<String>,
}

fn use_branches(
    tree: &syn::UseTree,
    prefix: &mut Vec<(String, proc_macro2::Span)>,
    out: &mut Vec<UseBranch>,
) {
    let branch = |prefix: &Vec<(String, proc_macro2::Span)>,
                  leaf: Option<(String, proc_macro2::Span)>,
                  kind: LeafKind,
                  binds: Option<String>| {
        let mut idents: Vec<String> = prefix.iter().map(|(ident, _)| ident.clone()).collect();
        let mut spans: Vec<proc_macro2::Span> = prefix.iter().map(|(_, span)| *span).collect();
        if let Some((ident, span)) = leaf {
            idents.push(ident);
            spans.push(span);
        }
        UseBranch {
            idents,
            spans,
            kind,
            binds,
        }
    };
    match tree {
        syn::UseTree::Path(segment) => {
            prefix.push((segment.ident.to_string(), segment.ident.span()));
            use_branches(&segment.tree, prefix, out);
            prefix.pop();
        }
        syn::UseTree::Group(group) => {
            for member in &group.items {
                use_branches(member, prefix, out);
            }
        }
        syn::UseTree::Name(leaf) => {
            let name = leaf.ident.to_string();
            out.push(branch(
                prefix,
                Some((name.clone(), leaf.ident.span())),
                LeafKind::Name,
                Some(name),
            ));
        }
        syn::UseTree::Rename(leaf) => out.push(branch(
            prefix,
            Some((leaf.ident.to_string(), leaf.ident.span())),
            LeafKind::Alias,
            Some(leaf.rename.to_string()),
        )),
        syn::UseTree::Glob(_) => out.push(branch(prefix, None, LeafKind::Glob, None)),
    }
}

// ── rustc's module-file law ─────────────────────────────────────────────────

/// A file's crate root and its module path from it, by layout alone. A `#[path]`
/// decl breaks that reading, which drops seats rather than inventing them.
fn module_path(rel: &str, roots: &BTreeSet<String>) -> ModuleId {
    let Some(root) = owning_root(rel, roots) else {
        return (rel.to_string(), Vec::new());
    };
    if rel == root {
        return (root, Vec::new());
    }
    let base = dirname(&root);
    let tail = match base.is_empty() {
        true => Some(rel),
        false => rel.strip_prefix(&format!("{base}/")),
    };
    let Some(tail) = tail else {
        return (root, Vec::new());
    };
    let mut parts: Vec<String> = tail.split('/').map(str::to_string).collect();
    let leaf = parts.pop().unwrap_or_default();
    let name = leaf.strip_suffix(".rs").unwrap_or(&leaf);
    if name != "mod" {
        parts.push(name.to_string());
    }
    (root, parts)
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

fn under(path: &str, dir: &str) -> bool {
    dir.is_empty() || path.starts_with(&format!("{dir}/"))
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

fn crate_roots(cx: &RenameCx) -> BTreeSet<String> {
    let mut roots: BTreeSet<String> = cx
        .files()
        .iter()
        .filter(|rel| auto_crate_root(rel))
        .cloned()
        .collect();
    for (manifest, package) in manifests(cx) {
        let dir = dirname(&manifest);
        if let Some(path) = package.lib.as_ref().and_then(|lib| lib.path.clone()) {
            roots.insert(join_rel(dir, &path));
        }
    }
    roots
}

/// A crate's identifier as a `use` writes it -> that crate's library root, so a
/// path through the package name reaches the same modules `crate::` does.
fn crate_idents(cx: &RenameCx) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (manifest, package) in manifests(cx) {
        let dir = dirname(&manifest);
        let named = package
            .lib
            .as_ref()
            .and_then(|lib| lib.name.clone())
            .or_else(|| package.package.as_ref().map(|meta| meta.name.clone()));
        let Some(named) = named else {
            continue;
        };
        let root = match package.lib.as_ref().and_then(|lib| lib.path.clone()) {
            Some(path) => join_rel(dir, &path),
            None => join_rel(dir, "src/lib.rs"),
        };
        out.insert(named.replace('-', "_"), root);
    }
    out
}

fn manifests(cx: &RenameCx) -> Vec<(String, Manifest)> {
    cx.files()
        .iter()
        .filter(|rel| stem(rel) == "Cargo" && rel.ends_with(".toml"))
        .filter_map(|rel| {
            let text = cx.text(rel)?;
            let parsed: Manifest = basic_toml::from_str(&text).ok()?;
            Some((rel.clone(), parsed))
        })
        .collect()
}

/// The two manifest keys the module law reads: the crate's own name, and a
/// `[lib]` that renames or relocates its root.
#[derive(Deserialize)]
struct Manifest {
    package: Option<ManifestPackage>,
    lib: Option<ManifestLib>,
}

#[derive(Deserialize)]
struct ManifestPackage {
    name: String,
}

#[derive(Deserialize)]
struct ManifestLib {
    name: Option<String>,
    path: Option<String>,
}
