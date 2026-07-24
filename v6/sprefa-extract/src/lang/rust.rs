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

use syn::{
    AngleBracketedGenericArguments, GenericArgument, Path, PathArguments, ReturnType, Type,
    TypeParamBound,
};

use crate::family::{ConstKind, ConstValue, CstF, SigSlot, TypeEntityKind, TypeF, TypeSig};
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
                        // commit C: CallF defs + sites.
                        let _ = &parsed;
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
