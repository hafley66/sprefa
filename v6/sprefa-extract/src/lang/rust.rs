//! The Rust extractor arm: syn front-end for type/call/df/const, ast-grep for cst.
//! Mirrors TsSource (same shape, different front-end): cst via ast-grep's rust
//! grammar + one `syn::parse_file` feeding the type/call/df/const projections.
//!
//! Commit A (skeleton): RustSource wires cst via ast-grep + a syn parse.
//! Commit B (this): TypeF entity nodes + arrow-type sigs + the const facet.
//! Commits C/D port `rust_call_defs_from`/`rust_call_sites_from` and
//! `rust_dataflow_from` from v5 (`src/graph/typegraph/rust/mod.rs`).
//!
//! Span bridge: syn's proc_macro2 spans are line/col; v6 `Span` is byte offsets,
//! so one `line_starts` table + `line_col_to_byte` converts (the rust-specific
//! bit oxc gives for free). v5's `rust_line` used `span.start().line`; the
//! parity oracle (v5_normalize) reconstructs the byte as `line_starts[line-1] +
//! col`, which is exactly `line_col_to_byte`.
//!
//! Deferred to `Resolve<TypeF>` (commit 4): type EDGES (field/impl/variant/uses/
//! generic). Deferred follow-ups: the docs facet (`rust_docs_from`); the df
//! enrichment aux (args/fields/lits/param_pos/loops/nests).

use syn::spanned::Spanned;
use syn::{
    AngleBracketedGenericArguments, GenericArgument, Path, PathArguments, ReturnType, Type,
    TypeParamBound,
};

use crate::family::{CallF, CallKind, CallSite, ConstKind, ConstValue, CstF, SigSlot, TypeEntityKind, TypeF, TypeSig};
use crate::rows::{FamilyBundle, Node};
use crate::seams::{Parser, Project};
use crate::shape::{Span, Strings};
use crate::source::{ExtractOutput, FamilyMask, Source};
use super::astgrep::{AstGrepParser, CstProjector};

// ── span bridge: proc_macro2 line/col -> v6 byte Span ───────────────────────

/// Byte offset of the start of each 1-based line: line N starts at `out[N-1]`.
/// Mirrors v5_normalize's `line_starts`; built once per file in `extract`.
fn build_line_starts(src: &str) -> Vec<u32> {
    let mut out = vec![0u32];
    for (byte_off, byte) in src.bytes().enumerate() {
        if byte == b'\n' {
            out.push((byte_off + 1) as u32);
        }
    }
    out
}

/// Convert a syn (1-based line, 0-based column) coordinate to a byte offset.
/// `column` is proc_macro2's char column; for ASCII source it equals the byte
/// column (v5's `rust_line`/`ts_push` make the same char-as-byte approximation,
/// and the parity oracle reconstructs bytes the same way).
fn line_col_to_byte(line_starts: &[u32], line: u32, col: u32) -> u32 {
    line_starts
        .get((line as usize).saturating_sub(1))
        .copied()
        .unwrap_or(0)
        .saturating_add(col)
}

/// A proc_macro2 span -> v6 byte Span. Used for entity/def spans where a real
/// length is kept (joins + future resolution); df nodes use start-only anchors.
fn syn_span(line_starts: &[u32], span: proc_macro2::Span) -> Span {
    let start = span.start();
    let end = span.end();
    let start_byte = line_col_to_byte(line_starts, start.line as u32, start.column as u32);
    let end_byte = line_col_to_byte(line_starts, end.line as u32, end.column as u32);
    Span { start: start_byte, len: end_byte.saturating_sub(start_byte) }
}

// ════════════════════════════════════════════════════════════════════════════
// TypeF: entity nodes + arrow-type sigs + the const facet. Commit B.
//
// Ports v5 `rust_entities_from` (the entity half) + `rust_fn_type` (the arrow-
// type payload) + `rust_const_values_from` (Const entities + ConstValue rows).
// The name-resolved type EDGES (field/impl/variant/uses/generic) land with
// `Resolve<TypeF>` (commit 4); phase 1 stays pure-content span nodes.
//
// v5 stores `parent`/`sym`/`mint_sym`; v6 drops them (a node is span+kind+name;
// the parent linkage is span-containment at the seam). v5 maps Union -> Struct
// (EntityKind has no union); v6 has no union kind either, so the same mapping.
// ════════════════════════════════════════════════════════════════════════════

/// Project the TypeF family: one entity node per type/function declaration, an
/// arrow-type sig per callable param/return type reference, and the const facet
/// (Const entities + ConstValue rows). Port of v5 `rust_entities_from` +
/// `rust_const_values_from`.
fn project_types(
    parsed: &syn::File,
    line_starts: &[u32],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    for item in &parsed.items {
        item_entity(item, line_starts, strings, sink);
    }
    const_values(parsed, line_starts, strings, sink);
}

/// One declared entity per item, mirroring v5 `rust_item_entity`. A callable
/// (function/method) additionally carries its arrow-type sigs.
fn item_entity(
    item: &syn::Item,
    line_starts: &[u32],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    match item {
        syn::Item::Struct(s) => push_entity(sink, strings, line_starts, s.ident.span(), &s.ident.to_string(), TypeEntityKind::Struct),
        syn::Item::Enum(en) => push_entity(sink, strings, line_starts, en.ident.span(), &en.ident.to_string(), TypeEntityKind::Enum),
        // v5 maps Union to EntityKind::Struct (no union brand); v6 does the same.
        syn::Item::Union(u) => push_entity(sink, strings, line_starts, u.ident.span(), &u.ident.to_string(), TypeEntityKind::Struct),
        syn::Item::Trait(t) => {
            push_entity(sink, strings, line_starts, t.ident.span(), &t.ident.to_string(), TypeEntityKind::Trait);
            // Only default methods (a body inside the trait block) get an entity
            // row; a bare signature has no code to hang a node on. Port of v5.
            for ti in &t.items {
                if let syn::TraitItem::Fn(m) = ti {
                    if m.default.is_some() {
                        let name = m.sig.ident.to_string();
                        let span = syn_span(line_starts, m.sig.ident.span());
                        push_entity_raw(sink, strings, span, &name, TypeEntityKind::Method);
                        fn_sigs(sink, strings, span, &m.sig);
                    }
                }
            }
        }
        syn::Item::Fn(f) => {
            let name = f.sig.ident.to_string();
            let span = syn_span(line_starts, f.sig.ident.span());
            push_entity_raw(sink, strings, span, &name, TypeEntityKind::Function);
            fn_sigs(sink, strings, span, &f.sig);
        }
        syn::Item::Impl(i) => {
            for ii in &i.items {
                if let syn::ImplItem::Fn(m) = ii {
                    let name = m.sig.ident.to_string();
                    let span = syn_span(line_starts, m.sig.ident.span());
                    push_entity_raw(sink, strings, span, &name, TypeEntityKind::Method);
                    fn_sigs(sink, strings, span, &m.sig);
                }
            }
        }
        _ => {}
    }
}

fn push_entity(
    sink: &mut FamilyBundle<TypeF>,
    strings: &mut Strings,
    line_starts: &[u32],
    span: proc_macro2::Span,
    name: &str,
    kind: TypeEntityKind,
) {
    push_entity_raw(sink, strings, syn_span(line_starts, span), name, kind);
}

fn push_entity_raw(
    sink: &mut FamilyBundle<TypeF>,
    strings: &mut Strings,
    span: Span,
    name: &str,
    kind: TypeEntityKind,
) {
    sink.nodes.push(Node::new(span, kind).with_name(strings.intern(name)));
}

/// The arrow-type sigs of one callable: param type-refs (positional, receiver
/// skipped) + return type-refs. Port of v5 `rust_fn_type` (the sig half; the
/// `TypeExpr` is flattened to `TypeSig` rows here). Each named type reference
/// under a signature annotation becomes one sig; keyword types (`String` is NOT
/// a keyword, it's a path -> "String") are distinct path variants.
fn fn_sigs(
    sink: &mut FamilyBundle<TypeF>,
    strings: &mut Strings,
    owner: Span,
    sig: &syn::Signature,
) {
    let mut pos: u32 = 0;
    for arg in &sig.inputs {
        if let syn::FnArg::Typed(pt) = arg {
            for name in type_refs(&pt.ty) {
                push_sig(sink, strings, owner, SigSlot::Param, pos, &name);
            }
            pos += 1;
        }
        // FnArg::Receiver (`self`) is skipped so positions align with the written
        // argument list (port of v5 `rust_fn_type`).
    }
    if let ReturnType::Type(_, ty) = &sig.output {
        for name in type_refs(ty) {
            push_sig(sink, strings, owner, SigSlot::Ret, 0, &name);
        }
    }
}

fn push_sig(
    sink: &mut FamilyBundle<TypeF>,
    strings: &mut Strings,
    owner: Span,
    slot: SigSlot,
    pos: u32,
    name: &str,
) {
    sink.aux.sigs.push(TypeSig { owner, slot, pos, ty: strings.intern(name) });
}

// ── type-reference collection (the arrow-type payload) ──────────────────────
//
// Port of v5 `type_refs`/`collect_type_refs`/`collect_bound_ref`/
// `collect_path_args`/`path_name`/`is_noise_type`. Collects the trailing path
// name of every named type reference under a signature annotation, filtering
// primitive names (`u32`, `str`, ...). One name per reference; a union slot
// stays one name (Rust has no inline union type syntax).

/// Every named type reference under `ty`, de-duplicated and sorted (port of v5
/// `type_refs`). Sorting makes the emitted sig order deterministic regardless of
/// syn traversal order.
fn type_refs(ty: &Type) -> Vec<String> {
    let mut out = Vec::new();
    collect_type_refs(ty, &mut out);
    out.sort();
    out.dedup();
    out
}

fn collect_type_refs(ty: &Type, out: &mut Vec<String>) {
    match ty {
        Type::Array(t) => collect_type_refs(&t.elem, out),
        Type::BareFn(t) => {
            for input in &t.inputs {
                collect_type_refs(&input.ty, out);
            }
            if let ReturnType::Type(_, ty) = &t.output {
                collect_type_refs(ty, out);
            }
        }
        Type::Group(t) => collect_type_refs(&t.elem, out),
        Type::ImplTrait(t) => {
            for bound in &t.bounds {
                collect_bound_ref(bound, out);
            }
        }
        Type::Paren(t) => collect_type_refs(&t.elem, out),
        Type::Path(t) => {
            if let Some(qself) = &t.qself {
                collect_type_refs(&qself.ty, out);
            }
            if let Some(name) = path_name(&t.path) {
                out.push(name);
            }
            collect_path_args(&t.path, out);
        }
        Type::Ptr(t) => collect_type_refs(&t.elem, out),
        Type::Reference(t) => collect_type_refs(&t.elem, out),
        Type::Slice(t) => collect_type_refs(&t.elem, out),
        Type::TraitObject(t) => {
            for bound in &t.bounds {
                collect_bound_ref(bound, out);
            }
        }
        Type::Tuple(t) => {
            for elem in &t.elems {
                collect_type_refs(elem, out);
            }
        }
        _ => {}
    }
}

fn collect_bound_ref(bound: &TypeParamBound, out: &mut Vec<String>) {
    if let TypeParamBound::Trait(t) = bound {
        if let Some(name) = path_name(&t.path) {
            out.push(name);
        }
        collect_path_args(&t.path, out);
    }
}

fn collect_path_args(path: &Path, out: &mut Vec<String>) {
    for seg in &path.segments {
        match &seg.arguments {
            PathArguments::AngleBracketed(AngleBracketedGenericArguments { args, .. }) => {
                for arg in args {
                    match arg {
                        GenericArgument::Type(t) => collect_type_refs(t, out),
                        GenericArgument::AssocType(t) => collect_type_refs(&t.ty, out),
                        GenericArgument::Constraint(c) => {
                            for bound in &c.bounds {
                                collect_bound_ref(bound, out);
                            }
                        }
                        _ => {}
                    }
                }
            }
            PathArguments::Parenthesized(p) => {
                for input in &p.inputs {
                    collect_type_refs(input, out);
                }
                if let ReturnType::Type(_, ty) = &p.output {
                    collect_type_refs(ty, out);
                }
            }
            PathArguments::None => {}
        }
    }
}

/// The trailing path name (`a::b::c` -> `a::b::c`), or None for a primitive /
/// `Self`. Port of v5 `path_name`.
fn path_name(path: &Path) -> Option<String> {
    let parts: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    if parts.is_empty() {
        return None;
    }
    let name = parts.join("::");
    if is_noise_type(&name) {
        None
    } else {
        Some(name)
    }
}

/// Primitive + `Self` filter: a reference to `u32`/`str`/`Self` carries no
/// resolvable declaration. Port of v5 `is_noise_type`.
fn is_noise_type(name: &str) -> bool {
    matches!(
        name,
        "Self"
            | "bool"
            | "char"
            | "str"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "f32"
            | "f64"
    )
}

// ── const facet: Const entities + ConstValue rows ───────────────────────────

/// Top-level `const X: &str = "...";` string values. Port of v5
/// `rust_const_values_from`. Non-goals (kept identical to v5): consts inside
/// `impl`/`mod`/fn bodies, non-string consts (no entity, no row).
fn const_values(
    parsed: &syn::File,
    line_starts: &[u32],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    for item in &parsed.items {
        let syn::Item::Const(c) = item else { continue };
        let syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. }) = &*c.expr else { continue };
        let span = syn_span(line_starts, c.ident.span());
        let name = c.ident.to_string();
        push_entity_raw(sink, strings, span, &name, TypeEntityKind::Const);
        sink.aux.consts.push(ConstValue {
            owner: span,
            field: None,
            text: strings.intern(&s.value()),
            kind: ConstKind::Lit,
        });
    }
}

// ════════════════════════════════════════════════════════════════════════════
// CallF: callable definitions (nodes) + call sites (aux). Commit C.
//
// Ports v5 `rust_call_defs_from` (defs, incl. the nested-fn/closure walker) +
// `rust_call_sites_from` (sites). v5's `mint_sym`/`lambda_sym`/`end` line are
// deleted: a def is span + kind + name. The def span COVERS its body (ident
// start -> block end) so the seam's span-containment can bind a site's caller;
// the parity line reads `line_of(span.start)` = the ident line (v5's `def.line`).
// Lambda defs (closures) keep kind=Lambda, name=None (v5's empty name).
// ════════════════════════════════════════════════════════════════════════════

/// A proc_macro2 span pair -> v6 byte Span covering `[start.start, end.end)`.
/// The def span covers the whole callable body for span-containment resolution.
fn def_span(line_starts: &[u32], start: proc_macro2::Span, end: proc_macro2::Span) -> Span {
    let start_lc = start.start();
    let end_lc = end.end();
    let start_byte = line_col_to_byte(line_starts, start_lc.line as u32, start_lc.column as u32);
    let end_byte = line_col_to_byte(line_starts, end_lc.line as u32, end_lc.column as u32);
    Span { start: start_byte, len: end_byte.saturating_sub(start_byte) }
}

/// Project the CallF family: one def node per callable (Free / Method / Lambda)
/// + one site per call expression. Port of v5 `rust_call_defs_from` +
/// `rust_call_sites_from`.
fn project_call(
    parsed: &syn::File,
    line_starts: &[u32],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    // Defs: the top-level driver emits Free (Item::Fn) + Method (impl/trait)
    // defs and walks each body; the visitor collects nested named fns (Free) and
    // closures (Lambda) reached inside a body. Port of v5's driver + RustCallDefs.
    let mut defs = RustCallDefs { line_starts, out: Vec::new() };
    for item in &parsed.items {
        match item {
            syn::Item::Fn(f) => {
                let span = def_span(line_starts, f.sig.ident.span(), f.block.span());
                defs.push(span, Some(f.sig.ident.to_string()), CallKind::Free);
                syn::visit::visit_block(&mut defs, &f.block);
            }
            syn::Item::Impl(i) => {
                for ii in &i.items {
                    if let syn::ImplItem::Fn(m) = ii {
                        let span = def_span(line_starts, m.sig.ident.span(), m.block.span());
                        defs.push(span, Some(m.sig.ident.to_string()), CallKind::Method);
                        syn::visit::visit_block(&mut defs, &m.block);
                    }
                }
            }
            // A trait fn: a signature-only declaration OR a default body, both
            // Method-owned by the trait, so a call through the trait has a target.
            syn::Item::Trait(t) => {
                for ti in &t.items {
                    if let syn::TraitItem::Fn(m) = ti {
                        let name = m.sig.ident.to_string();
                        let span = match &m.default {
                            Some(block) => def_span(line_starts, m.sig.ident.span(), block.span()),
                            None => def_span(line_starts, m.sig.ident.span(), m.sig.span()),
                        };
                        defs.push(span, Some(name), CallKind::Method);
                        if let Some(block) = &m.default {
                            syn::visit::visit_block(&mut defs, block);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    for def in defs.out {
        let mut node = Node::new(def.span, def.kind);
        if let Some(name) = def.name {
            node = node.with_name(strings.intern(&name));
        }
        sink.nodes.push(node);
    }

    // Sites: one walk over the whole file for every call/method-call/struct-literal
    // expression. The callee is the trailing name as written (unresolved in phase
    // 1). Port of v5's CallCollector.
    let mut collector = CallCollector { line_starts, sites: Vec::new() };
    syn::visit::visit_file(&mut collector, parsed);
    for site in collector.sites {
        sink.aux.sites.push(CallSite {
            span: site.span,
            callee: strings.intern(&site.callee),
            callee_path: site.callee_path.map(|path| strings.intern(&path)),
        });
    }
}

/// One collected def before it is interned into the bundle.
struct CollectedDef {
    span: Span,
    name: Option<String>,
    kind: CallKind,
}

/// Walks callable bodies for the callables the top-level driver misses: nested
/// named fns (Free) and closures (Lambda). Port of v5 `RustCallDefs` (the sym
/// stack is dropped: v6 needs no enclosing sym for a lambda def).
struct RustCallDefs<'a> {
    line_starts: &'a [u32],
    out: Vec<CollectedDef>,
}

impl<'a> RustCallDefs<'a> {
    fn push(&mut self, span: Span, name: Option<String>, kind: CallKind) {
        self.out.push(CollectedDef { span, name, kind });
    }
}

impl<'ast, 'a> syn::visit::Visit<'ast> for RustCallDefs<'a> {
    // A nested named fn (`fn helper() {}` inside a body). File-level identity
    // (df does not lift nested-fn bodies, so no owner-scoped sym to match). Port
    // of v5 visit_item_fn.
    fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
        let span = def_span(self.line_starts, function.sig.ident.span(), function.block.span());
        self.push(span, Some(function.sig.ident.to_string()), CallKind::Free);
        syn::visit::visit_item_fn(self, function);
    }
    // A closure (`|x| ...`). The def span covers the closure body so a call inside
    // it binds to this lambda by containment. Port of v5 visit_expr_closure.
    fn visit_expr_closure(&mut self, closure: &'ast syn::ExprClosure) {
        let span = def_span(self.line_starts, closure.span(), closure.body.span());
        self.push(span, None, CallKind::Lambda);
        syn::visit::visit_expr_closure(self, closure);
    }
}

/// One collected call site before it is interned into the aux.
struct CollectedSite {
    span: Span,
    callee: String,
    callee_path: Option<String>,
}

/// Walks the whole file for call expressions (`f(x)`, `recv.m(x)`, `Foo { .. }`).
/// Port of v5 `CallCollector`.
struct CallCollector<'a> {
    line_starts: &'a [u32],
    sites: Vec<CollectedSite>,
}

impl<'ast, 'a> syn::visit::Visit<'ast> for CallCollector<'a> {
    fn visit_expr(&mut self, expr: &'ast syn::Expr) {
        match expr {
            // `f(args)` / `Foo(args)`: callee is the path's trailing segment.
            syn::Expr::Call(call) => {
                let function = peel_parens(&call.func);
                if let syn::Expr::Path(path) = function {
                    if let Some(segment) = path.path.segments.last() {
                        let path_str = path_string(&path.path);
                        self.sites.push(CollectedSite {
                            span: syn_span(self.line_starts, call.func.span()),
                            callee: segment.ident.to_string(),
                            callee_path: (path.path.segments.len() > 1).then_some(path_str),
                        });
                    }
                }
                syn::visit::visit_expr(self, expr);
            }
            // `recv.m(args)`: callee is the method ident.
            syn::Expr::MethodCall(call) => {
                self.sites.push(CollectedSite {
                    span: syn_span(self.line_starts, call.method.span()),
                    callee: call.method.to_string(),
                    callee_path: None,
                });
                syn::visit::visit_expr(self, expr);
            }
            // `Foo { x: 1 }`: struct literal constructor; callee is the type path's
            // trailing segment.
            syn::Expr::Struct(struct_expr) => {
                if let Some(segment) = struct_expr.path.segments.last() {
                    let path_str = path_string(&struct_expr.path);
                    self.sites.push(CollectedSite {
                        span: syn_span(self.line_starts, struct_expr.path.span()),
                        callee: segment.ident.to_string(),
                        callee_path: (struct_expr.path.segments.len() > 1).then_some(path_str),
                    });
                }
                syn::visit::visit_expr(self, expr);
            }
            _ => syn::visit::visit_expr(self, expr),
        }
    }
}

/// Render a syn::Path as `a::b::c`. Port of v5 `path_string`.
fn path_string(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

/// Strip nested `Expr::Paren` to find the inner expression. Port of v5
/// `peel_parens`.
fn peel_parens(expr: &syn::Expr) -> &syn::Expr {
    let mut current = expr;
    while let syn::Expr::Paren(paren) = current {
        current = &paren.expr;
    }
    current
}

// ════════════════════════════════════════════════════════════════════════════
// RustSource: the Rust Source (cst via ast-grep + type/call/df via syn).
//
// The two-parser, masked shape (mirrors TsSource). cst runs through ast-grep
// (one dep = the CST floor for every lang); type/call/df run through ONE syn
// parse (three masked projections over the same tree). ONE shared `Strings`
// across all four families.
// ════════════════════════════════════════════════════════════════════════════

/// The Rust `Source`. `matches` = the path ends in `.rs`. cst via ast-grep's rust
/// grammar; type/call/df/const via one `syn::parse_file`.
#[derive(Default)]
pub struct RustSource;

impl Source for RustSource {
    fn name(&self) -> &'static str {
        "rust"
    }

    fn matches(&self, path: &str) -> bool {
        path.ends_with(".rs")
    }

    fn extract(&self, path: &str, content: &[u8], mask: FamilyMask) -> ExtractOutput {
        let mut strings = Strings::new();

        // cst via ast-grep (masked). ast-grep's SupportLang has a rust grammar, so
        // a .rs parses losslessly. Owns its () arena; dropped at block end. A failed
        // ast-grep parse leaves cst None (no panic).
        let cst = if mask.cst {
            let arena = AstGrepParser.make_arena();
            AstGrepParser.parse(&arena, path, content).ok().map(|parsed| {
                let mut bundle = FamilyBundle::<CstF>::default();
                CstProjector.project(&parsed, &mut strings, &mut bundle);
                bundle
            })
        } else {
            None
        };

        // type/call/df via ONE syn parse (masked). Owns no arena (syn::File is
        // owned); the line_starts table bridges proc_macro2 line/col to byte
        // spans once, shared across the masked projections. A failed parse leaves
        // all three None (partial output: cst above may still be Some).
        let mut types = None;
        let mut call = None;
        let mut df = None;
        if mask.types || mask.call || mask.df {
            if let Ok(src) = std::str::from_utf8(content) {
                if let Ok(parsed) = syn::parse_file(src) {
                    let line_starts = build_line_starts(src);
                    if mask.types {
                        let mut bundle = FamilyBundle::<TypeF>::default();
                        project_types(&parsed, &line_starts, &mut strings, &mut bundle);
                        types = Some(bundle);
                    }
                    if mask.call {
                        let mut bundle = FamilyBundle::<CallF>::default();
                        project_call(&parsed, &line_starts, &mut strings, &mut bundle);
                        call = Some(bundle);
                    }
                    if mask.df {
                        // commit D: DfF nodes + Direct edges.
                        let _ = &parsed;
                    }
                }
            }
        }

        ExtractOutput { strings, cst, types, call, df }
    }
}
