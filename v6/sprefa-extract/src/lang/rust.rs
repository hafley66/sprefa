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
//! Commit 4d-i lands the type EDGES: unresolved candidates ride `TypeFAux` out
//! of the one parse (port of v5 `edges_from`: field/variant/generic/impl — v5
//! rust emits NO param/returns and NO uses), and `Resolve<TypeF>` binds them
//! (the 4b-iii discipline, mirrored from the ts arm). Commit 4d-ii lands
//! `Resolve<CallF>` (the 4c-ii ts arm mirrored: NameResolve primary,
//! ScipOverride on scip disagreement; the rust-analyzer `local `-symbol
//! adaptation documented on the arm) + the scip ratchet. Deferred follow-ups:
//! the docs facet (`rust_docs_from`); df loop/nesting aux. Df argument slots,
//! parameter positions, field names and literal texts are emitted.

use std::collections::BTreeSet;

use syn::spanned::Spanned;
use syn::{
    AngleBracketedGenericArguments, Fields, GenericArgument, GenericParam, Path, PathArguments,
    ReturnType, Type, TypeParamBound, WherePredicate,
};

use super::astgrep::{AstGrepParser, CstProjector};
use crate::family::{
    CallEdgeKind, CallF, CallKind, CallSite, ConstKind, ConstValue, CstF, DfArg, DfEdgeKind, DfF,
    DfField, DfLit, DfNodeKind, DfParam, DocFact, DocTag, MethodOwner, ProjectEdge, SigSlot,
    Specifier, SpecifierKind, TypeEdgeCandidate, TypeEdgeKind, TypeEntityKind, TypeF, TypeSig,
};
use crate::project::ResolveDrop;
use crate::rows::{Edge, FamilyBundle, Node};
use crate::scip::{byte_range, definition_of, join_documents, site_occurrence};
use crate::seams::{
    containing_def_site, corpus_defs, covering_def, def_named, own_blob, DefIndex, Parser, Project,
    Resolve,
};
use crate::shape::{ContentId, FamilyTag, NodeRef, Span, Strings, ZERO_CONTENT_ID};
use crate::source::{ExtractOutput, FamilyMask, ProjectCx, Source};
use crate::trace;
use crate::types::LangKind;
use crate::types::ScipIndex;
use crate::types::{CfgScope, DefSite, MacroSite, MacroSiteSource, PathIndex, TestOnlyCall, UnresolvedReason};

// ── span bridge: proc_macro2 line/col -> v6 byte Span ───────────────────────

/// Byte offset of the start of each 1-based line: line N starts at `out[N-1]`.
/// Mirrors v5_normalize's `line_starts`; built once per file in `extract`.
pub(crate) fn build_line_starts(src: &str) -> Vec<u32> {
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
pub(crate) fn syn_span(line_starts: &[u32], span: proc_macro2::Span) -> Span {
    let start = span.start();
    let end = span.end();
    let start_byte = line_col_to_byte(line_starts, start.line as u32, start.column as u32);
    let end_byte = line_col_to_byte(line_starts, end.line as u32, end.column as u32);
    Span {
        start: start_byte,
        len: end_byte.saturating_sub(start_byte),
    }
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
    doc_facts(parsed, line_starts, strings, sink);
    // The candidates walk runs AFTER every entity is in the bundle so an
    // impl-owned candidate finds its in-file self-type entity regardless of
    // item order (v5's text-keyed pass has no order sensitivity; spans do).
    edge_candidates(parsed, line_starts, strings, sink);
}

/// Port of v5 `rust_docs_from`. The walked set is v5's: struct, enum, union,
/// trait, fn and impl methods. A documented const or alias mints no row.
fn doc_facts(
    parsed: &syn::File,
    line_starts: &[u32],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    doc_facts_in_items(&parsed.items, line_starts, strings, sink);
}

fn doc_facts_in_items(
    items: &[syn::Item],
    line_starts: &[u32],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    for item in items {
        match item {
            syn::Item::Struct(s) => {
                push_doc(sink, strings, line_starts, s.ident.span(), &s.attrs, None)
            }
            syn::Item::Enum(en) => {
                push_doc(sink, strings, line_starts, en.ident.span(), &en.attrs, None)
            }
            syn::Item::Union(u) => {
                push_doc(sink, strings, line_starts, u.ident.span(), &u.attrs, None)
            }
            syn::Item::Trait(t) => {
                push_doc(sink, strings, line_starts, t.ident.span(), &t.attrs, None)
            }
            syn::Item::Fn(f) => push_doc(
                sink,
                strings,
                line_starts,
                f.sig.ident.span(),
                &f.attrs,
                None,
            ),
            syn::Item::Impl(i) => {
                let owner = primary_type(&i.self_ty);
                for ii in &i.items {
                    if let syn::ImplItem::Fn(m) = ii {
                        push_doc(
                            sink,
                            strings,
                            line_starts,
                            m.sig.ident.span(),
                            &m.attrs,
                            owner.as_deref(),
                        );
                    }
                }
            }
            syn::Item::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    doc_facts_in_items(inner, line_starts, strings, sink);
                }
            }
            _ => {}
        }
    }
}

fn push_doc(
    sink: &mut FamilyBundle<TypeF>,
    strings: &mut Strings,
    line_starts: &[u32],
    span: proc_macro2::Span,
    attrs: &[syn::Attribute],
    parent: Option<&str>,
) {
    let lines = doc_lines(attrs);
    if lines.is_empty() {
        return;
    }
    let text = lines.join("\n");
    let tags = doc_sections(&text, strings);
    sink.aux.docs.push(DocFact {
        owner: syn_span(line_starts, span),
        parent: parent.map(|name| strings.intern(name)),
        text: strings.intern(&text),
        tags,
    });
}

/// Each `#[doc = "..."]` value, the single leading space syn keeps from `/// x`
/// stripped. Port of v5 `rust_doc_lines`.
fn doc_lines(attrs: &[syn::Attribute]) -> Vec<String> {
    let mut lines = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        if let syn::Meta::NameValue(nv) = &attr.meta {
            if let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) = &nv.value
            {
                let value = s.value();
                lines.push(value.strip_prefix(' ').unwrap_or(&value).to_string());
            }
        }
    }
    lines
}

/// Rustdoc `# Heading` sections, each a `section` tag whose `arg` is the heading
/// and whose text is the body. Port of v5 `parse_rust_sections`.
fn doc_sections(text: &str, strings: &mut Strings) -> Vec<DocTag> {
    let mut out: Vec<(String, Vec<&str>)> = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.trim_start().strip_prefix("# ") {
            out.push((rest.trim().to_string(), Vec::new()));
        } else if let Some((_, body)) = out.last_mut() {
            body.push(line);
        }
    }
    out.into_iter()
        .map(|(heading, body)| DocTag {
            tag: strings.intern("section"),
            arg: Some(strings.intern(&heading)),
            text: strings.intern(body.join("\n").trim()),
        })
        .collect()
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
        syn::Item::Struct(s) => push_entity(
            sink,
            strings,
            line_starts,
            s.ident.span(),
            &s.ident.to_string(),
            TypeEntityKind::Struct,
        ),
        syn::Item::Enum(en) => push_entity(
            sink,
            strings,
            line_starts,
            en.ident.span(),
            &en.ident.to_string(),
            TypeEntityKind::Enum,
        ),
        // v5 maps Union to EntityKind::Struct (no union brand); v6 does the same.
        syn::Item::Union(u) => push_entity(
            sink,
            strings,
            line_starts,
            u.ident.span(),
            &u.ident.to_string(),
            TypeEntityKind::Struct,
        ),
        syn::Item::Trait(t) => {
            push_entity(
                sink,
                strings,
                line_starts,
                t.ident.span(),
                &t.ident.to_string(),
                TRAIT,
            );
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
        syn::Item::Mod(m) => {
            if let Some((_, inner)) = &m.content {
                for nested in inner {
                    item_entity(nested, line_starts, strings, sink);
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
    sink.nodes
        .push(Node::new(span, kind).with_name(strings.intern(name)));
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
    let mut pos = 0u32;
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
    sink.aux.sigs.push(TypeSig {
        owner,
        slot,
        pos,
        ty: strings.intern(name),
    });
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

/// Item-level `const X: &str = "...";` string values, inline `mod` bodies
/// included. Non-goals: consts inside `impl` or fn bodies, non-string consts.
fn const_values(
    parsed: &syn::File,
    line_starts: &[u32],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    const_values_in_items(&parsed.items, line_starts, strings, sink);
}

fn const_values_in_items(
    items: &[syn::Item],
    line_starts: &[u32],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    for item in items {
        if let syn::Item::Mod(m) = item {
            if let Some((_, inner)) = &m.content {
                const_values_in_items(inner, line_starts, strings, sink);
            }
            continue;
        }
        let syn::Item::Const(c) = item else { continue };
        let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(s),
            ..
        }) = &*c.expr
        else {
            continue;
        };
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

// ── type-edge candidates (4d-i; the Resolve<TypeF> input) ───────────────────
//
// Port of v5 `edges_from` (src/graph/typegraph/rust/mod.rs:88-183), collected
// during the ONE syn parse into TypeFAux.candidates — the 4b-iii ruling (the
// CallFAux.specifiers pattern: unresolved rows; owner span + to-name as
// written + kind; resolve binds purely, phase 2 stays zero-AST). v5 rust emits
// field/variant/generic/impl ONLY — NO param/returns (v5's rust edges_from
// never walks a fn signature, so the ts arm's Function-only sig filter has no
// rust analogue; per-lang toward v5 per the v5-is-correct ruling) and NO uses
// (ts-only). TWO v5 rows are unrepresentable in the candidate shape (the
// owner is a Span; v5's `from` is free text) and are SKIPPED with this comment
// as the loud marker — NEITHER is exercised by any fixture or oracle row, so
// the asserted oracle diff stays green; the honest fix is a candidate-shape
// evolution (an adjudicated increment, not a silent skip):
//  - enum-variant FIELD edges: v5's from is the synthetic `Owner::Variant`
//    text and no entity exists for the owner span to point at.
//  - impl-owned edges (generic bounds + the trait `impl` edge) on a self-type
//    declared OUTSIDE this file: no in-file entity carries the owner name.
//    An impl on an IN-FILE self-type IS minted (owner = that entity's span;
//    v5's from-text is exactly the entity name).

/// Collect one file's unresolved type-edge candidates. Port of v5 `edges_from`
/// + `item_edges`.
fn edge_candidates(
    parsed: &syn::File,
    line_starts: &[u32],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    for item in &parsed.items {
        item_edge_candidates(item, line_starts, strings, sink);
    }
}

fn item_edge_candidates(
    item: &syn::Item,
    line_starts: &[u32],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    match item {
        syn::Item::Struct(s) => {
            let owner = syn_span(line_starts, s.ident.span());
            generic_candidates(owner, &s.generics, strings, sink);
            field_candidates(owner, &s.fields, strings, sink);
        }
        syn::Item::Enum(e) => {
            let owner = syn_span(line_starts, e.ident.span());
            generic_candidates(owner, &e.generics, strings, sink);
            for variant in &e.variants {
                // The `to` is v5's synthetic `Owner::Member` text — text dsts
                // STAY text (the 4b-iii ruling). The variant's own field edges
                // are unrepresentable (see the section comment).
                push_candidate(
                    sink,
                    strings,
                    owner,
                    &format!("{}::{}", e.ident, variant.ident),
                    TypeEdgeKind::Variant,
                );
            }
        }
        // v5 maps Union to Struct for entities and walks its fields the same way.
        syn::Item::Union(u) => {
            let owner = syn_span(line_starts, u.ident.span());
            generic_candidates(owner, &u.generics, strings, sink);
            field_candidates(owner, &Fields::Named(u.fields.clone()), strings, sink);
        }
        syn::Item::Trait(t) => {
            let owner = syn_span(line_starts, t.ident.span());
            generic_candidates(owner, &t.generics, strings, sink);
            for bound in &t.supertraits {
                bound_candidate(owner, bound, strings, sink);
            }
        }
        syn::Item::Impl(i) => {
            // Port of v5: the whole impl is skipped when the self-type has no
            // primary name. The owner is the IN-FILE entity of that name; an
            // external self-type is unrepresentable (see the section comment).
            let Some(owner_name) = primary_type(&i.self_ty) else {
                return;
            };
            let Some(owner) = entity_span_named(sink, strings, &owner_name) else {
                return;
            };
            generic_candidates(owner, &i.generics, strings, sink);
            if let Some((_, path, _)) = &i.trait_ {
                if let Some(to) = path_name(path) {
                    push_candidate(sink, strings, owner, &to, TypeEdgeKind::Impl);
                }
            }
        }
        syn::Item::Mod(m) => {
            if let Some((_, inner)) = &m.content {
                for nested in inner {
                    item_edge_candidates(nested, line_starts, strings, sink);
                }
            }
        }
        _ => {}
    }
}

/// The span of the TypeF entity interned as `name` in this bundle (the owner
/// leg of an impl-owned candidate: v5's from-text is the self-type name, which
/// the in-file entity carries verbatim).
fn entity_span_named(sink: &FamilyBundle<TypeF>, strings: &Strings, name: &str) -> Option<Span> {
    sink.nodes
        .iter()
        .find(|node| node.name.map_or(false, |id| strings.lookup(id) == name))
        .map(|node| node.span)
}

/// One field candidate per named type reference under each field's type. Port
/// of v5 `field_edges` (`type_refs` is the shared port above).
fn field_candidates(
    owner: Span,
    fields: &Fields,
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    for field in fields.iter() {
        for to in type_refs(&field.ty) {
            push_candidate(sink, strings, owner, &to, TypeEdgeKind::Field);
        }
    }
}

/// Generic-bound + where-clause candidates. Port of v5 `generic_edges`.
fn generic_candidates(
    owner: Span,
    generics: &syn::Generics,
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    for param in &generics.params {
        if let GenericParam::Type(t) = param {
            for bound in &t.bounds {
                bound_candidate(owner, bound, strings, sink);
            }
        }
    }
    if let Some(where_clause) = &generics.where_clause {
        for pred in &where_clause.predicates {
            if let WherePredicate::Type(t) = pred {
                for bound in &t.bounds {
                    bound_candidate(owner, bound, strings, sink);
                }
            }
        }
    }
}

/// One generic candidate per trait bound. Port of v5 `bound_edge` (the kind is
/// always Generic here — v5 rust binds bounds under no other edge kind).
fn bound_candidate(
    owner: Span,
    bound: &TypeParamBound,
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    if let TypeParamBound::Trait(t) = bound {
        if let Some(to) = path_name(&t.path) {
            push_candidate(sink, strings, owner, &to, TypeEdgeKind::Generic);
        }
    }
}

fn push_candidate(
    sink: &mut FamilyBundle<TypeF>,
    strings: &mut Strings,
    owner: Span,
    to: &str,
    kind: TypeEdgeKind,
) {
    sink.aux.candidates.push(TypeEdgeCandidate {
        owner,
        to: strings.intern(to),
        kind,
    });
}

// ════════════════════════════════════════════════════════════════════════════
// Resolve<TypeF> for RustSource (commit 4d-i). The 4b-iii discipline, mirrored
// from the ts arm: the candidate row IS the parity target; text dsts STAY
// text — a candidate whose `to` names no corpus node (v5's synthetic
// `Owner::Member` variant text, externals) emits a ZERO dst leg. The
// genuinely-resolved span->blob legs are a v6-only ADDITIVE layer (reported,
// never asserted). Same-file blob leg: the TypeF node named `to` in THIS
// bundle gives the span, the DefIndex span-join gives the blob. Corpus
// fallback: a UNIQUE site only.
// ════════════════════════════════════════════════════════════════════════════

impl RustSource {
    /// The deduped, deterministically-ordered candidate list (v5's BTreeSet
    /// shaping): the aux candidates, deduped on (owner, to, kind). `resolve`
    /// emits its edges in EXACTLY this order, one per candidate — the parity
    /// golden zips the two (the zip discipline: edge i resolves candidate i).
    pub fn type_edge_candidates(output: &ExtractOutput) -> Vec<TypeEdgeCandidate> {
        let mut set: BTreeSet<TypeEdgeCandidate> = BTreeSet::new();
        if let Some(types) = &output.types {
            for candidate in &types.aux.candidates {
                set.insert(candidate.clone());
            }
        }
        set.into_iter().collect()
    }
}

/// The dst leg of one candidate: same-file TypeF entity first (its span joined
/// through the `DefIndex` for the blob), else a unique corpus site, else None
/// (text stays text — the zero leg). Name-only resolution, per the 4a ADDENDUM
/// site-key discipline. Mirror of the ts arm's `resolve_type_dst` (the post-4d
/// dedup sweep owns unifying the per-lang copies).
fn resolve_type_dst(
    types: &FamilyBundle<TypeF>,
    strings: &Strings,
    index: Option<&DefIndex>,
    modules: Option<&crate::lang::rust_modules::RustModuleIndex>,
    own_path: Option<&str>,
    name: &str,
) -> Option<(ContentId, Span)> {
    let same_file = types
        .nodes
        .iter()
        .find(|node| node.name.map_or(false, |id| strings.lookup(id) == name));
    if let (Some(node), Some(index)) = (same_file, index) {
        if let Some(found) = corpus_defs(index, name)
            .iter()
            .find(|site| site.span == node.span)
            .map(|site| (site.blob.clone(), site.span))
        {
            return Some(found);
        }
    }
    if let Some(found) = import_bound_target(modules, own_path, name) {
        return Some(found);
    }
    let sites = index.map(|index| corpus_defs(index, name)).unwrap_or(&[]);
    match sites {
        [only] => Some((only.blob.clone(), only.span)),
        _ => None,
    }
}

/// A bare name with no same-file def: the `use` binding named `name` in
/// `own_path`, resolved through the module plane.
fn import_bound_target(
    modules: Option<&crate::lang::rust_modules::RustModuleIndex>,
    own_path: Option<&str>,
    name: &str,
) -> Option<(ContentId, Span)> {
    modules?.target(own_path?, name)
}

impl Resolve<TypeF> for RustSource {
    fn resolve(&self, output: &ExtractOutput, cx: &ProjectCx) -> Vec<ProjectEdge<TypeF>> {
        let Some(types) = &output.types else {
            return Vec::new();
        };
        let index = cx.indexes.def_index.get();
        let modules = cx.indexes.rust_modules.get();
        let own_path = index
            .and_then(|index| own_blob(output, index))
            .zip(cx.indexes.paths.get())
            .and_then(|(blob, paths)| paths.get(&blob).map(str::to_string));
        let mut edges = Vec::new();
        for candidate in RustSource::type_edge_candidates(output) {
            // src: the TypeF entity at the owner span, exists by construction.
            let Some(src_ix) = types
                .nodes
                .iter()
                .position(|node| node.span == candidate.owner)
            else {
                continue;
            };
            let (dst_blob, dst_span) = resolve_type_dst(
                types,
                &output.strings,
                index,
                modules,
                own_path.as_deref(),
                output.strings.lookup(candidate.to),
            )
            .unwrap_or((ZERO_CONTENT_ID, Span::empty()));
            edges.push(ProjectEdge::new(
                NodeRef(src_ix as u32),
                dst_blob,
                dst_span,
                candidate.kind,
            ));
        }
        edges
    }
}

// ════════════════════════════════════════════════════════════════════════════
// Resolve<CallF> for RustSource (commit 4d-ii). The 4c-ii ts arm mirrored, two
// legs per the user rulings (scip-override ALLOWED; the v5-shaped name-match
// stays primary):
//   NameResolve — callee name -> unique def. Same-file WINS via the span-join
//     (def_named in THIS CallF bundle -> its span -> the DefIndex gives the
//     blob); cross-file a UNIQUE corpus blob (CallF facet preferred);
//     ambiguous/absent -> NO ROW (the 4b-iii discipline).
//   ScipOverride — scip's occurrence resolution for the site disagrees with
//     the name-match outcome: scip's corpus target WINS the edge, the
//     name-match is displaced. Needs the corpus scip index
//     (cx.indexes.scip_index) AND the rev-correct reader (cx.reader); either
//     absent -> pure name-match. scip-EXTERNAL never displaces and never
//     mints.
// RUST-ANALYZER ADAPTATION (the honest per-indexer difference, mirrored by
// the ratchet and logged in the ledger): a `local ` symbol at a call site is
// a LOCAL BINDING (`let func = |x| ..; func(..)`) — df-owned, not a call-
// graph def (rust-analyzer names no closure symbol; scip's answer is the
// binding, and the 4c containing_def_site join would misroute it to the
// ENCLOSING fn, minting a false self-edge). Local-symbol sites are treated
// as scip-external: NO v6 edge. Method resolution stays NAME-ONLY per the 4a
// ADDENDUM (receiver typing out of scope). `callee_path` rides phase 1 as
// collected (rust fills it); the resolution key stays the trailing segment —
// no path-qualified matching is invented (unexercised by the fixtures and
// unratchetable where scip already arbitrates).
// The arm learns its own blob by the DefIndex span-join (`own_blob`) and its
// scip document by content hash (`join_documents`). Per-site edges, no dedup.
// A site outside every CallF def (module level) emits no row.
// ════════════════════════════════════════════════════════════════════════════

/// The SAME-FILE def named `callee`, extracted so `rust_modules.rs` can run it
/// before an import-binding leg: a local def shadows an import.
fn same_file_call_match(
    output: &ExtractOutput,
    index: &DefIndex,
    own: Option<&ContentId>,
    callee: &str,
) -> Option<(ContentId, Span)> {
    let call = output.call.as_ref()?;
    let r = def_named(call, &output.strings, callee)?;
    let span = call.node(r).span;
    // The span join must land on THIS file's DefSite: a byte-identical
    // (name, span) def can exist in two files.
    let blob = own?;
    corpus_defs(index, callee)
        .iter()
        .find(|site| site.span == span && &site.blob == blob)
        .map(|site| (site.blob.clone(), site.span))
}

impl RustSource {
    /// The name-match target of one callee (the NameResolve leg). Pub so the
    /// scip ratchet re-runs it to classify overrides — same discipline as
    /// `type_edge_candidates` in 4d-i. Mirror of `TsSource::call_name_match`
    /// (the post-4d dedup sweep owns unifying the per-lang copies).
    pub fn call_name_match(
        output: &ExtractOutput,
        index: &DefIndex,
        callee: &str,
    ) -> Option<(ContentId, Span)> {
        let own = own_file_blob(output, index);
        Self::call_name_match_in(output, index, own.as_ref(), callee)
    }

    /// `call_name_match` with the file's own blob already in hand: the blob is
    /// a per-FILE fact, and finding it costs a corpus-index join per call.
    pub fn call_name_match_in(
        output: &ExtractOutput,
        index: &DefIndex,
        own: Option<&ContentId>,
        callee: &str,
    ) -> Option<(ContentId, Span)> {
        if let Some(found) = same_file_call_match(output, index, own, callee) {
            return Some(found);
        }
        let sites = corpus_defs(index, callee);
        let mut blobs: Vec<ContentId> = Vec::new();
        for site in sites {
            if !blobs.contains(&site.blob) {
                blobs.push(site.blob.clone());
            }
        }
        let [blob] = blobs.as_slice() else {
            return None;
        };
        let site = sites
            .iter()
            .find(|s| s.family == FamilyTag::Call)
            .unwrap_or(&sites[0]);
        Some((blob.clone(), site.span))
    }

    /// The name-match target of a callee written `a::b::f` for MODULES `a::b`:
    /// only defs whose file spells a module path ending in `a::b` are candidates.
    pub fn call_name_match_in_module(
        index: &DefIndex,
        paths: &PathIndex,
        from: &str,
        qualifier: &[&str],
        callee: &str,
    ) -> Option<(ContentId, Span)> {
        let want = module_target(from, qualifier)?;
        let sites: Vec<&DefSite> = corpus_defs(index, callee)
            .iter()
            .filter(|site| {
                paths
                    .get(&site.blob)
                    .is_some_and(|path| want.covers(&module_segments(path)))
            })
            .collect();
        let mut blobs: Vec<&ContentId> = Vec::new();
        for site in &sites {
            if !blobs.contains(&&site.blob) {
                blobs.push(&site.blob);
            }
        }
        let [blob] = blobs.as_slice() else {
            return None;
        };
        let site = sites
            .iter()
            .find(|s| s.family == FamilyTag::Call)
            .unwrap_or(&sites[0]);
        Some(((*blob).clone(), site.span))
    }
}

/// A `callee_path`'s leading segments when every one is MODULE-shaped, else
/// None: receiver typing is out of scope, so `Widget::build` keeps the name leg.
fn module_qualifier(callee_path: &str) -> Option<Vec<&str>> {
    let mut segments: Vec<&str> = callee_path.split("::").collect();
    segments.pop()?;
    if segments.is_empty() {
        return None;
    }
    segments
        .iter()
        .all(|segment| {
            !segment
                .chars()
                .next()
                .is_some_and(|first| first.is_uppercase())
        })
        .then_some(segments)
}

/// The module path a file spells: minus `.rs`, `src` dropped, `mod`/`lib`/`main`
/// collapsing to the directory, `-` read as `_` (`crates/ide-db` is `ide_db`).
pub(crate) fn module_segments(path: &str) -> Vec<String> {
    let stem = path.strip_suffix(".rs").unwrap_or(path);
    let mut segments: Vec<String> = stem
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != "." && *segment != "src")
        .map(|segment| segment.replace('-', "_"))
        .collect();
    if matches!(
        segments.last().map(String::as_str),
        Some("mod" | "lib" | "main")
    ) {
        segments.pop();
    }
    segments
}

/// What a resolved qualifier demands of a candidate file: a module-path suffix,
/// and under `crate::` the caller's own crate directory as a path prefix.
pub(crate) struct ModuleTarget {
    pub(crate) suffix: Vec<String>,
    pub(crate) crate_root: Option<String>,
}

impl ModuleTarget {
    pub(crate) fn covers(&self, candidate: &[String]) -> bool {
        if let Some(root) = &self.crate_root {
            if !candidate.starts_with(&module_segments(root)) {
                return false;
            }
        }
        candidate.ends_with(&self.suffix)
    }
}

/// `qualifier` read from `from`'s position: `crate` restarts at the crate root,
/// `self` extends the caller's module, `super` pops one, else absolute suffix.
pub(crate) fn module_target(from: &str, qualifier: &[&str]) -> Option<ModuleTarget> {
    let own = module_segments(from);
    let normalize = |rest: &[&str]| -> Vec<String> {
        rest.iter()
            .map(|segment| segment.replace('-', "_"))
            .collect()
    };
    match qualifier[0] {
        "crate" => Some(ModuleTarget {
            suffix: normalize(&qualifier[1..]),
            crate_root: crate_root_of(from),
        }),
        "self" | "super" => {
            let mut base = own;
            let mut rest = qualifier;
            while let Some(head) = rest.first() {
                match *head {
                    "self" => {}
                    "super" => {
                        base.pop()?;
                    }
                    _ => break,
                }
                rest = &rest[1..];
            }
            base.extend(normalize(rest));
            Some(ModuleTarget {
                suffix: base,
                crate_root: None,
            })
        }
        _ => Some(ModuleTarget {
            suffix: normalize(qualifier),
            crate_root: None,
        }),
    }
}

/// The crate directory holding `path`: the prefix ending at the segment before
/// the first `src`. None where the file sits outside a Cargo layout.
pub(crate) fn crate_root_of(path: &str) -> Option<String> {
    let (root, _) = path.split_once("/src/")?;
    Some(root.to_string())
}

/// One corpus `DefSite` examined while learning a file's own blob. The term
/// that was quadratic while the join ran once per call site.
static OWN_BLOB_PROBES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn own_blob_probes() -> u64 {
    OWN_BLOB_PROBES.load(std::sync::atomic::Ordering::Relaxed)
}

fn probe<T>(value: T) -> T {
    OWN_BLOB_PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    value
}

/// The corpus blob covering every named CallF def of `output`. One def is not
/// a file identity: two files can hold an identical def at the same offset.
fn own_file_blob(output: &ExtractOutput, index: &DefIndex) -> Option<ContentId> {
    let call = output.call.as_ref()?;
    let own: Vec<(&str, Span)> = call
        .nodes
        .iter()
        .filter_map(|node| Some((output.strings.lookup(node.name?), node.span)))
        .collect();
    // Seeded on the RAREST name: a corpus-wide name like `new` puts every file
    // in the candidate set, and each candidate costs a full cover check.
    let (seed_name, seed_span) = *own
        .iter()
        .min_by_key(|(name, _)| corpus_defs(index, name).len())?;
    let seeds = || {
        corpus_defs(index, seed_name)
            .iter()
            .filter(|site| probe(site.span == seed_span))
    };
    let mut hits = seeds();
    let first = hits.next()?;
    if hits.next().is_none() {
        return Some(first.blob.clone());
    }
    // Two files carry this (name, span): only the whole named-def set tells
    // them apart.
    let covers = |blob: &ContentId| {
        own.iter().all(|(name, span)| {
            corpus_defs(index, name)
                .iter()
                .any(|site| probe(&site.blob == blob && site.span == *span))
        })
    };
    seeds()
        .find(|site| covers(&site.blob))
        .map(|site| site.blob.clone())
}

/// The scip-resolved corpus target of one call site: the site's occurrence
/// (the shared `site_occurrence` convention) -> its symbol's definition
/// occurrence -> the containing DefSite. None = scip has no corpus CALL
/// target: an external library symbol, an unresolved reference, no occurrence
/// at the site, the target document outside the corpus — OR a `local `
/// symbol, the rust-analyzer adaptation documented on the arm above (a local
/// binding is df-owned; the enclosing fn is NOT the callee).
fn scip_call_target<'a>(
    index: &ScipIndex,
    joined: &[Option<(ContentId, Vec<u8>)>],
    doc_ix: usize,
    site: &CallSite,
    callee: &str,
    def_index: &'a DefIndex,
) -> Option<(ContentId, Span, &'a str)> {
    let doc = &index.documents[doc_ix];
    let (_, content) = joined[doc_ix].as_ref()?;
    let occ = site_occurrence(doc, content, site.span, callee)?;
    if occ.symbol.starts_with("local ") {
        return None;
    }
    let (def_doc_ix, def_occ) = definition_of(index, doc_ix, &occ.symbol)?;
    let def_doc = &index.documents[def_doc_ix];
    let (def_blob, def_content) = joined[def_doc_ix].as_ref()?;
    let ident = byte_range(def_content, def_occ.range, def_doc.position_encoding)?;
    let (name, def_site) = containing_def_site(def_index, def_blob.clone(), ident)?;
    Some((def_blob.clone(), def_site.span, name))
}

impl Resolve<CallF> for RustSource {
    fn resolve(&self, output: &ExtractOutput, cx: &ProjectCx) -> Vec<ProjectEdge<CallF>> {
        let Some(call) = &output.call else {
            return Vec::new();
        };
        let Some(def_index) = cx.indexes.def_index.get() else {
            return Vec::new();
        };
        // The scip leg: the corpus index + the rev-correct reader + this
        // file's own document (found by content hash). Any missing piece ->
        // pure name-match (v5-shaped).
        let scip = cx
            .indexes
            .scip_index
            .get()
            .zip(cx.reader)
            .and_then(|(index, reader)| {
                let joined = cx
                    .indexes
                    .joined_documents
                    .get_or_init(|| join_documents(index, reader));
                let blob = own_blob(output, def_index)?;
                let doc_ix = joined
                    .iter()
                    .position(|j| j.as_ref().map_or(false, |(b, _)| *b == blob))?;
                Some((index, joined, doc_ix))
            });
        // Per FILE, never per site: the join is over the whole corpus index.
        let own = own_file_blob(output, def_index);
        let paths = cx.indexes.paths.get();
        let own_path = own
            .as_ref()
            .zip(paths)
            .and_then(|(blob, paths)| paths.get(blob));
        let modules = cx.indexes.rust_modules.get();
        // Sorted once per file: the mirror lookup runs per closure-caller site,
        // and a per-site scan of the def table is the shape kink 1 was.
        let named = named_def_spans(call);
        let mut edges = Vec::new();
        for site in &call.aux.sites {
            // The caller is the innermost covering CallF def (the 4a
            // caller-binding discipline); a module-level site has no caller
            // node and emits no row.
            let Some(caller) = covering_def(call, site.span) else {
                continue;
            };
            let callee = output.strings.lookup(site.callee);
            let qualifier = site
                .callee_path
                .map(|id| output.strings.lookup(id))
                .and_then(module_qualifier);
            let name_t: Option<(ContentId, Span, CallEdgeKind)> = match (qualifier, own_path, paths) {
                (Some(qualifier), Some(from), Some(paths)) => {
                    RustSource::call_name_match_in_module(def_index, paths, from, &qualifier, callee)
                        .map(|(blob, span)| (blob, span, CallEdgeKind::NameResolve))
                }
                _ => same_file_call_match(output, def_index, own.as_ref(), callee)
                    .map(|(blob, span)| (blob, span, CallEdgeKind::NameResolve))
                    .or_else(|| {
                        import_bound_target(modules, own_path, callee)
                            .map(|(blob, span)| (blob, span, CallEdgeKind::ImportResolve))
                    })
                    .or_else(|| {
                        RustSource::call_name_match_in(output, def_index, own.as_ref(), callee)
                            .map(|(blob, span)| (blob, span, CallEdgeKind::NameResolve))
                    }),
            };
            let scip_t = scip.as_ref().and_then(|(index, joined, doc_ix)| {
                scip_call_target(index, joined, *doc_ix, site, callee, def_index)
            });
            // Agreement is judged at (blob, name): the name-match binds the
            // call FACET while scip can name the type facet — one definition,
            // two facet coordinates (the ORACLE entry's "the models differ by
            // construction").
            let ((dst_blob, dst_span), kind) = match (name_t, scip_t) {
                (Some((blob, span, _)), Some(s)) if blob == s.0 && callee == s.2 => {
                    ((blob, span), CallEdgeKind::NameResolve)
                }
                (_, Some(s)) => ((s.0, s.1), CallEdgeKind::ScipOverride),
                (Some((blob, span, kind)), None) => ((blob, span), kind),
                (None, None) => continue,
            };
            // Nothing names `closure@<n>` as a callee, so a walk over named
            // defs stops at one. The closure row stays; it names the frame.
            if call.node(caller).name.is_none() {
                if let Some(enclosing) = enclosing_named_def(&named, site.span) {
                    edges.push(
                        ProjectEdge::new(
                            enclosing,
                            dst_blob.clone(),
                            dst_span,
                            CallEdgeKind::NameResolve,
                        )
                        .with_call_site(site.span),
                    );
                }
            }
            edges
                .push(ProjectEdge::new(caller, dst_blob, dst_span, kind).with_call_site(site.span));
        }
        edges
    }
}

/// One `unresolved` row per site the `Resolve<CallF>` pass dropped. The reason
/// reads the corpus def count for the callee's name: none, or more than one.
pub fn call_drops(
    output: &ExtractOutput,
    cx: &ProjectCx,
    edges: &[ProjectEdge<CallF>],
) -> Vec<ResolveDrop> {
    let (Some(call), Some(def_index)) = (&output.call, cx.indexes.def_index.get()) else {
        return Vec::new();
    };
    let bound: BTreeSet<(u32, u32)> = edges
        .iter()
        .filter_map(|edge| edge.call_site.map(|span| (span.start, span.end())))
        .collect();
    call.aux
        .sites
        .iter()
        .filter(|site| !bound.contains(&(site.span.start, site.span.end())))
        .map(|site| {
            let callee = output.strings.lookup(site.callee);
            let reason = if corpus_defs(def_index, callee).is_empty() {
                UnresolvedReason::NoCorpusDef
            } else {
                UnresolvedReason::Ambiguous
            };
            let detail = site.callee_path.map_or_else(
                || callee.to_string(),
                |id| output.strings.lookup(id).to_string(),
            );
            ResolveDrop {
                span: site.span,
                reason,
                detail,
            }
        })
        .collect()
}

/// Every NAMED CallF def as (span, ref), sorted by (start, end) for the
/// `enclosing_named_def` binary search.
fn named_def_spans(defs: &FamilyBundle<CallF>) -> Vec<(Span, NodeRef)> {
    let mut sorted: Vec<(Span, NodeRef)> = defs
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.name.is_some())
        .map(|(ix, node)| (node.span, NodeRef(ix as u32)))
        .collect();
    sorted.sort_by_key(|(span, _)| (span.start, span.end()));
    sorted
}

/// The innermost NAMED def covering `site`. `covering_def` takes the innermost
/// def of any kind, which is the closure wherever one is in the way.
fn enclosing_named_def(sorted: &[(Span, NodeRef)], site: Span) -> Option<NodeRef> {
    let cut = sorted.partition_point(|(span, _)| span.start <= site.start);
    let mut best: Option<(Span, NodeRef)> = None;
    for &(span, r) in &sorted[..cut] {
        if site.end() <= span.end()
            && best.map_or(true, |(b, _)| span.end() - span.start < b.end() - b.start)
        {
            best = Some((span, r));
        }
    }
    best.map(|(_, r)| r)
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
    Span {
        start: start_byte,
        len: end_byte.saturating_sub(start_byte),
    }
}

/// Descends inline `mod name { .. }`: the SITE half walks the whole file, so a
/// callable declared in one needs a def or the file reports uses without them.
fn call_defs_in_items(
    items: &[syn::Item],
    line_starts: &[u32],
    defs: &mut RustCallDefs,
    owners: &mut Vec<CollectedOwner>,
    scopes: &mut Vec<(Span, String)>,
    under_cfg: Option<&str>,
) {
    for item in items {
        // An item inherits its enclosing module's predicate: `#[cfg(test)] mod
        // tests` guards every def beneath it, however deeply nested, and the
        // outermost predicate is the one that decides.
        let own = cfg_test_predicate(item_attrs(item));
        let active: Option<&str> = under_cfg.or(own.as_deref());
        let note = |span: Span, scopes: &mut Vec<(Span, String)>| {
            if let Some(predicate) = active {
                scopes.push((span, predicate.to_string()));
            }
        };
        match item {
            syn::Item::Fn(f) => {
                let span = def_span(line_starts, f.sig.ident.span(), f.block.span());
                defs.push(span, Some(f.sig.ident.to_string()), CallKind::Free);
                note(span, scopes);
                syn::visit::visit_block(defs, &f.block);
            }
            syn::Item::Impl(i) => {
                let self_type = primary_type(&i.self_ty);
                let trait_name = i.trait_.as_ref().map(|(_, path, _)| path_string(path));
                for ii in &i.items {
                    if let syn::ImplItem::Fn(m) = ii {
                        let span = def_span(line_starts, m.sig.ident.span(), m.block.span());
                        defs.push(span, Some(m.sig.ident.to_string()), CallKind::Method);
                        note(span, scopes);
                        owners.push(CollectedOwner {
                            span,
                            self_type: self_type.clone(),
                            trait_name: trait_name.clone(),
                        });
                        syn::visit::visit_block(defs, &m.block);
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
                        note(span, scopes);
                        owners.push(CollectedOwner {
                            span,
                            self_type: None,
                            trait_name: Some(t.ident.to_string()),
                        });
                        if let Some(block) = &m.default {
                            syn::visit::visit_block(defs, block);
                        }
                    }
                }
            }
            syn::Item::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    call_defs_in_items(inner, line_starts, defs, owners, scopes, active);
                }
            }
            syn::Item::Const(c) => {
                let span = def_span(line_starts, c.ident.span(), c.expr.span());
                if initializer_defs(
                    span,
                    &c.ident,
                    &c.expr,
                    line_starts,
                    defs,
                    owners,
                    scopes,
                    active,
                ) {
                    note(span, scopes);
                }
            }
            syn::Item::Static(s) => {
                let span = def_span(line_starts, s.ident.span(), s.expr.span());
                if initializer_defs(
                    span,
                    &s.ident,
                    &s.expr,
                    line_starts,
                    defs,
                    owners,
                    scopes,
                    active,
                ) {
                    note(span, scopes);
                }
            }
            _ => {}
        }
    }
}

/// Defs under a `const`/`static` initializer, plus the item as a def when the
/// initializer holds a call no inner def covers. Returns whether it minted one.
#[allow(clippy::too_many_arguments)]
fn initializer_defs(
    item_span: Span,
    ident: &syn::Ident,
    expr: &syn::Expr,
    line_starts: &[u32],
    defs: &mut RustCallDefs,
    owners: &mut Vec<CollectedOwner>,
    scopes: &mut Vec<(Span, String)>,
    under_cfg: Option<&str>,
) -> bool {
    let mark = defs.out.len();
    match expr {
        // The block form is the derive-macro shape: its statement items carry
        // impl blocks and trait impls, which only `call_defs_in_items` reads.
        syn::Expr::Block(block) => {
            let items: Vec<syn::Item> = block
                .block
                .stmts
                .iter()
                .filter_map(|stmt| match stmt {
                    syn::Stmt::Item(item) => Some(item.clone()),
                    _ => None,
                })
                .collect();
            call_defs_in_items(&items, line_starts, defs, owners, scopes, under_cfg);
            for stmt in &block.block.stmts {
                if !matches!(stmt, syn::Stmt::Item(_)) {
                    syn::visit::Visit::visit_stmt(defs, stmt);
                }
            }
        }
        // The METHOD, never `syn::visit::visit_expr`: the free fn dispatches
        // past the override, so a top-level `f()` or `|| ..` is not seen.
        _ => syn::visit::Visit::visit_expr(defs, expr),
    }
    let covered: Vec<Span> = defs.out[mark..].iter().map(|def| def.span).collect();
    let mut sites = CallCollector {
        line_starts,
        sites: Vec::new(),
        under_cfg: None,
    };
    syn::visit::Visit::visit_expr(&mut sites, expr);
    let uncovered = sites.sites.iter().any(|site| {
        !covered
            .iter()
            .any(|span| span.start <= site.span.start && site.span.end() <= span.end())
    });
    if uncovered {
        defs.push(item_span, Some(ident.to_string()), CONST_INIT);
    }
    uncovered
}

/// One method's declaration before it is interned into the aux. `self_type` is
/// `None` for a trait's own items; `trait_name` is `None` for an inherent impl.
struct CollectedOwner {
    span: Span,
    self_type: Option<String>,
    trait_name: Option<String>,
}

/// Every syn item form that can carry attributes, so a cfg predicate on any of
/// them is seen. The forms with no attributes yield an empty slice rather than
/// being skipped, which keeps the match total and the default safe.
fn item_attrs(item: &syn::Item) -> &[syn::Attribute] {
    match item {
        syn::Item::Fn(i) => &i.attrs,
        syn::Item::Impl(i) => &i.attrs,
        syn::Item::Trait(i) => &i.attrs,
        syn::Item::Mod(i) => &i.attrs,
        syn::Item::Struct(i) => &i.attrs,
        syn::Item::Enum(i) => &i.attrs,
        syn::Item::Const(i) => &i.attrs,
        syn::Item::Static(i) => &i.attrs,
        syn::Item::Type(i) => &i.attrs,
        syn::Item::Use(i) => &i.attrs,
        syn::Item::Macro(i) => &i.attrs,
        syn::Item::TraitAlias(i) => &i.attrs,
        syn::Item::Union(i) => &i.attrs,
        syn::Item::ExternCrate(i) => &i.attrs,
        syn::Item::ForeignMod(i) => &i.attrs,
        _ => &[],
    }
}

/// The `cfg` predicate as written, when it names `test` anywhere inside it.
/// `#[cfg(test)]`, `#[cfg(any(test, feature = "x"))]` and `#[cfg(all(test,
/// unix))]` all qualify; `#[cfg(feature = "testing")]` does not, because the
/// token is matched as a whole word and not as a substring.
fn cfg_test_predicate(attrs: &[syn::Attribute]) -> Option<String> {
    for attr in attrs {
        if !attr.path().is_ident("cfg") {
            continue;
        }
        let syn::Meta::List(list) = &attr.meta else {
            continue;
        };
        let text = list.tokens.to_string();
        let names_test = text
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|word| word == "test");
        if names_test {
            return Some(text);
        }
    }
    None
}

/// Project the CallF family: one def node per callable (Free / Method / Lambda)
/// + one site per call expression. Port of v5 `rust_call_{defs,sites}_from`.
fn project_call(
    parsed: &syn::File,
    line_starts: &[u32],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let mut defs = RustCallDefs {
        line_starts,
        out: Vec::new(),
    };
    let mut owners = Vec::new();
    let mut scopes: Vec<(Span, String)> = Vec::new();
    call_defs_in_items(
        &parsed.items,
        line_starts,
        &mut defs,
        &mut owners,
        &mut scopes,
        None,
    );
    for def in defs.out {
        let mut node = Node::new(def.span, def.kind);
        if let Some(name) = def.name {
            node = node.with_name(strings.intern(&name));
        }
        sink.nodes.push(node);
    }
    for (span, predicate) in scopes {
        sink.aux.cfg_scopes.push(CfgScope {
            span,
            cfg: strings.intern(&predicate),
        });
    }
    for owner in owners {
        sink.aux.method_owners.push(MethodOwner {
            span: owner.span,
            self_type: owner.self_type.map(|name| strings.intern(&name)),
            trait_name: owner.trait_name.map(|name| strings.intern(&name)),
        });
    }

    // Sites: one walk over the whole file for every call/method-call/struct-literal
    // expression. The callee is the trailing name as written (unresolved in phase
    // 1). Port of v5's CallCollector.
    let mut collector = CallCollector {
        line_starts,
        sites: Vec::new(),
        under_cfg: None,
    };
    syn::visit::visit_file(&mut collector, parsed);
    for (callee, predicate) in test_only_calls(&collector.sites) {
        sink.aux.test_only_calls.push(TestOnlyCall {
            callee: strings.intern(&callee),
            cfg: strings.intern(&predicate),
        });
    }
    for site in collector.sites {
        sink.aux.sites.push(CallSite {
            span: site.span,
            callee: strings.intern(&site.callee),
            callee_path: site.callee_path.map(|path| strings.intern(&path)),
        });
    }

    module_specifiers(&parsed.items, line_starts, strings, sink);
}

// ── module specifiers (CallFAux.specifiers) ─────────────────────────────────
// @comment-ok: the kind/name/module contract, pinned row-for-row by
// tests/24_rust_specifiers.rs. `Default` and `SideEffect` are unreachable here.
//
// | rust source                 | kind      | name  | module     |
// |-----------------------------|-----------|-------|------------|
// | `use a::b;`                 | Named     | b     | a::b       |
// | `use a::b as c;`            | Named     | c     | a::b       |
// | `use a::{b, c};`            | Named x2  | b, c  | a::b, a::c |
// | `use a::b::{self};`         | Named     | b     | a::b       |
// | `use a::b::self;`           | Named     | b     | a::b       |
// | `use a::*;`                 | Namespace | a     | a          |
// | `pub use a::b;`             | Reexport  | b     | a::b       |
// | `pub use a::*;`             | Reexport  | a     | a          |
// | `mod foo;`                  | Named     | foo   | foo        |
// | `#[path = "x.rs"] mod foo;` | Named     | foo   | x.rs       |
// | `mod foo { ... }`           | NO ROW, items inside it still walked      |
// | `extern crate a;`           | NO ROW                                    |

/// `span` is the leaf's own tokens for a `use`, the whole item for a `mod`,
/// where a `#[path]` attribute is part of that item.
struct ModuleLeaf {
    span: Span,
    name: String,
    kind: SpecifierKind,
    module: String,
}

/// Rides the one syn parse `project_call` already holds. v5 read the same facts
/// with regexes over comment-stripped text (`src/graph/modgraph/rust.rs:5-37`).
fn module_specifiers(
    items: &[syn::Item],
    line_starts: &[u32],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let mut leaves = Vec::new();
    collect_module_leaves(items, line_starts, &mut leaves);
    sink.aux
        .specifiers
        .extend(leaves.into_iter().map(|leaf| Specifier {
            span: leaf.span,
            name: strings.intern(&leaf.name),
            kind: leaf.kind,
            module: Some(strings.intern(&leaf.module)),
            imported: None,
        }));
}

/// Descends into inline `mod name { .. }` bodies: a `use` inside one is a use.
/// A `mod` decl is Named at any visibility (`src/graph/modgraph/rust.rs:65,92`).
fn collect_module_leaves(items: &[syn::Item], line_starts: &[u32], out: &mut Vec<ModuleLeaf>) {
    for item in items {
        match item {
            syn::Item::Use(use_item) => {
                // v5's `rust_use_is_reexport` line check (`modgraph/rust.rs:20-30`),
                // read off the parsed visibility so `pub(in ..)` needs no regex.
                let reexport = !matches!(use_item.vis, syn::Visibility::Inherited);
                let mut prefix = Vec::new();
                use_tree_leaves(&use_item.tree, reexport, line_starts, &mut prefix, out);
            }
            syn::Item::Mod(mod_item) => match &mod_item.content {
                Some((_, inner)) => collect_module_leaves(inner, line_starts, out),
                None => {
                    let name = mod_item.ident.to_string();
                    let module = mod_path_attr(&mod_item.attrs).unwrap_or_else(|| name.clone());
                    out.push(ModuleLeaf {
                        span: syn_span(line_starts, mod_item.span()),
                        name,
                        kind: SpecifierKind::Named,
                        module,
                    });
                }
            },
            _ => {}
        }
    }
}

/// One leaf per bound name, `prefix` carrying the module segments above this
/// node. A glob binds no single local name (`src/graph/modgraph/rust.rs:120-126`).
fn use_tree_leaves(
    tree: &syn::UseTree,
    reexport: bool,
    line_starts: &[u32],
    prefix: &mut Vec<String>,
    out: &mut Vec<ModuleLeaf>,
) {
    match tree {
        syn::UseTree::Path(segment) => {
            prefix.push(segment.ident.to_string());
            use_tree_leaves(&segment.tree, reexport, line_starts, prefix, out);
            prefix.pop();
        }
        syn::UseTree::Group(group) => {
            for member in &group.items {
                use_tree_leaves(member, reexport, line_starts, prefix, out);
            }
        }
        syn::UseTree::Name(leaf) => {
            let span = syn_span(line_starts, leaf.ident.span());
            push_use_leaf(&leaf.ident.to_string(), None, span, reexport, prefix, out);
        }
        syn::UseTree::Rename(leaf) => {
            let span = syn_span(line_starts, leaf.span());
            let alias = Some(leaf.rename.to_string());
            push_use_leaf(&leaf.ident.to_string(), alias, span, reexport, prefix, out);
        }
        syn::UseTree::Glob(glob) => {
            let Some(last) = prefix.last() else { return };
            out.push(ModuleLeaf {
                span: syn_span(line_starts, glob.star_token.span()),
                name: last.clone(),
                kind: if reexport {
                    SpecifierKind::Reexport
                } else {
                    SpecifierKind::Namespace
                },
                module: prefix.join("::"),
            });
        }
    }
}

/// A `self` leaf names the module the prefix already spells, so its local name
/// is the last segment (`src/graph/modgraph/rust.rs:121-123`).
fn push_use_leaf(
    segment: &str,
    alias: Option<String>,
    span: Span,
    reexport: bool,
    prefix: &[String],
    out: &mut Vec<ModuleLeaf>,
) {
    let (name, module) = if segment == "self" {
        let Some(last) = prefix.last() else { return };
        (alias.unwrap_or_else(|| last.clone()), prefix.join("::"))
    } else {
        let mut segments = prefix.to_vec();
        segments.push(segment.to_string());
        (
            alias.unwrap_or_else(|| segment.to_string()),
            segments.join("::"),
        )
    };
    out.push(ModuleLeaf {
        span,
        name,
        kind: if reexport {
            SpecifierKind::Reexport
        } else {
            SpecifierKind::Named
        },
        module,
    });
}

/// `#[path = "x.rs"]`: the literal a resolver must resolve, so it becomes the
/// module text as written (`src/graph/modgraph/rust.rs:50-64`).
pub(crate) fn mod_path_attr(attrs: &[syn::Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| {
        if !attr.path().is_ident("path") {
            return None;
        }
        match &attr.meta {
            syn::Meta::NameValue(pair) => match &pair.value {
                syn::Expr::Lit(literal) => match &literal.lit {
                    syn::Lit::Str(text) => Some(text.value()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    })
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
        let span = def_span(
            self.line_starts,
            function.sig.ident.span(),
            function.block.span(),
        );
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

/// One collected call site before it is interned into the aux. `cfg` is the
/// enclosing cfg predicate naming `test`, at any item depth above the call.
struct CollectedSite {
    span: Span,
    callee: String,
    callee_path: Option<String>,
    cfg: Option<String>,
}

/// The callees this file names ONLY from cfg-guarded sites. One unguarded site
/// keeps a callee out: the consumer subtracts the NAME, never the site.
fn test_only_calls(sites: &[CollectedSite]) -> Vec<(String, String)> {
    let shipped: std::collections::HashSet<&str> = sites
        .iter()
        .filter(|site| site.cfg.is_none())
        .map(|site| site.callee.as_str())
        .collect();
    let mut seen = std::collections::HashSet::new();
    sites
        .iter()
        .filter_map(|site| site.cfg.as_ref().map(|cfg| (&site.callee, cfg)))
        .filter(|(callee, _)| !shipped.contains(callee.as_str()))
        .filter(|(callee, _)| seen.insert(callee.as_str()))
        .map(|(callee, cfg)| (callee.clone(), cfg.clone()))
        .collect()
}

/// Walks the whole file for call expressions (`f(x)`, `recv.m(x)`, `Foo { .. }`).
/// Port of v5 `CallCollector`.
struct CallCollector<'a> {
    line_starts: &'a [u32],
    sites: Vec<CollectedSite>,
    /// The cfg predicate the walk currently sits under, restored on the way out.
    under_cfg: Option<String>,
}

impl<'ast, 'a> syn::visit::Visit<'ast> for CallCollector<'a> {
    // Every item form reaches this, including one declared inside a fn body, so
    // a predicate on any ancestor covers the calls beneath it.
    fn visit_item(&mut self, item: &'ast syn::Item) {
        let outer = self.under_cfg.take();
        let own = cfg_test_predicate(item_attrs(item));
        self.under_cfg = outer.clone().or(own);
        syn::visit::visit_item(self, item);
        self.under_cfg = outer;
    }

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
                            cfg: self.under_cfg.clone(),
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
                    cfg: self.under_cfg.clone(),
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
                        cfg: self.under_cfg.clone(),
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
// DfF: intra-procedural value flow (nodes + Direct edges). Commit D.
//
// Ports v5 `rust_dataflow_from` (src/graph/typegraph/rust/mod.rs:746-1332). Every
// value-bearing position in a callable's body becomes a NODE; local value flow
// becomes a Direct EDGE. The two are the dataflow graph the engine's `df_reaches`
// closure walks.
//
// What is DROPPED vs v5 (each deliberate, matching the TS DfF port):
//  - `fn_sym` ON NODES: the enclosing callable is not stored on every df node;
//    it is threaded through the walk (v5's own mechanism) purely so the
//    `closure` VALUE node carries v5's exact `lam_sym` name
//    (`{file}::function::{fn}::closure::{line}_{col}`, syn's 1-based line /
//    0-based col; methods root at `{file}::method::{Owner}.{m}`). No sym store:
//    the name derives from the walk's containment path + the closure's span.
//  - `line`/`col`: a node is a byte Span (start via line_col_to_byte), never a
//    line/col pair.
//  - the enrichment aux: `args`, `fields`, `lits`, `param_pos`. The EDGES
//    already carry every value flow.
// ════════════════════════════════════════════════════════════════════════════

/// Transient scope: a variable name -> its binding node (param or `let`).
type Scope = std::collections::HashMap<String, NodeRef>;

/// Live enclosing `loop` frames for break-value routing: each entry is
/// (label, collected break-value tails). Threaded through the recursive walk so
/// `Expr::Break` finds its target loop and `Expr::Loop` drains the tails.
type LoopBreaks = Vec<(Option<String>, Vec<NodeRef>)>;

/// Project the DfF family: each callable's body lifted to its value-flow graph.
/// Port of v5 `rust_dataflow_from` (the driver half). `file` roots each fn_sym:
/// `{file}::function::{name}` for free fns, `{file}::method::{Owner}.{name}` for
/// impl methods (v5 `mint_sym`; the closure value node's name derives from it).
fn project_df(
    parsed: &syn::File,
    file: &str,
    src: &str,
    line_starts: &[u32],
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    df_items(&parsed.items, "", file, line_starts, strings, sink);
    for (index, start, end) in std::mem::take(&mut sink.aux.loop_collection_spans) {
        sink.aux.loops[index].collection =
            src.get(start as usize..end as usize).map(str::to_string);
    }
}

/// `mod_path` is the enclosing inline-`mod` chain (`""` at the file root,
/// `inner::deeper::` two mods down), so sibling mods mint distinct fn syms.
fn df_items(
    items: &[syn::Item],
    mod_path: &str,
    file: &str,
    line_starts: &[u32],
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    for item in items {
        match item {
            syn::Item::Fn(f) => {
                let fn_sym = format!("{file}::function::{mod_path}{}", f.sig.ident);
                let mut scope = Scope::new();
                let mut loop_breaks = LoopBreaks::new();
                flow_fn_body(
                    &f.sig,
                    &f.block,
                    &fn_sym,
                    line_starts,
                    strings,
                    &mut scope,
                    sink,
                    &mut loop_breaks,
                );
                claim_allocator_hits(
                    sink,
                    0,
                    def_span(line_starts, f.sig.ident.span(), f.block.span()),
                );
            }
            syn::Item::Impl(i) => {
                let owner = primary_type(&i.self_ty);
                for ii in &i.items {
                    if let syn::ImplItem::Fn(m) = ii {
                        let fn_sym = match &owner {
                            Some(o) => format!("{file}::method::{mod_path}{o}.{}", m.sig.ident),
                            None => format!("{file}::function::{mod_path}{}", m.sig.ident),
                        };
                        let mut scope = Scope::new();
                        let mut loop_breaks = LoopBreaks::new();
                        flow_fn_body(
                            &m.sig,
                            &m.block,
                            &fn_sym,
                            line_starts,
                            strings,
                            &mut scope,
                            sink,
                            &mut loop_breaks,
                        );
                        claim_allocator_hits(
                            sink,
                            0,
                            def_span(line_starts, m.sig.ident.span(), m.block.span()),
                        );
                    }
                }
            }
            syn::Item::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    let nested = format!("{mod_path}{}::", m.ident);
                    df_items(inner, &nested, file, line_starts, strings, sink);
                }
            }
            _ => {}
        }
    }
    sink.aux.nests = crate::types::compute_nests(&sink.nodes, &sink.aux.loops);
}

/// v5 keys `allocators` on `fn_sym`, which a closure rebinds to its `lam_sym`
/// (rust/mod.rs:1149,1176): the INNERMOST callable claims the hit and truncates.
fn claim_allocator_hits(sink: &mut FamilyBundle<DfF>, mark: usize, owner: Span) {
    if sink.aux.allocator_hits.len() > mark {
        sink.aux.allocator_hits.truncate(mark);
        sink.aux.allocates.push(crate::types::DfAllocates { owner });
    }
}

/// Port of v5 `is_allocator_call` (rust/mod.rs:1056): a collection constructor
/// callee marks its enclosing callable as allocating.
fn is_allocator_call(expr: &syn::Expr) -> bool {
    let syn::Expr::Path(path) = expr else {
        return false;
    };
    let segments: Vec<String> = path
        .path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect();
    let full = segments.join("::");
    if full.ends_with("::new") {
        return segments.iter().any(|segment| {
            matches!(
                segment.as_str(),
                "Vec"
                    | "HashMap"
                    | "BTreeMap"
                    | "HashSet"
                    | "BTreeSet"
                    | "VecDeque"
                    | "String"
                    | "LinkedList"
            )
        });
    }
    matches!(
        full.as_str(),
        "Vec::with_capacity" | "HashMap::with_capacity" | "String::with_capacity"
    )
}

/// A collecting method call marks its enclosing callable as allocating. Port of
/// v5 `is_allocator_method` (src/graph/typegraph/rust/mod.rs:1094).
fn is_allocator_method(ident: &syn::Ident) -> bool {
    matches!(
        ident.to_string().as_str(),
        "collect" | "to_vec" | "to_string" | "to_owned" | "clone" | "format"
    )
}

/// One loop row, with the iterated expression's byte range parked on
/// `loop_collection_spans` for `project_df` to slice once the source is in hand.
fn df_loop_row(
    sink: &mut FamilyBundle<DfF>,
    line_starts: &[u32],
    loop_span: proc_macro2::Span,
    var: Option<String>,
    collection: Option<proc_macro2::Span>,
) {
    let index = sink.aux.loops.len();
    sink.aux.loops.push(crate::types::DfLoop {
        span: syn_span(line_starts, loop_span),
        var,
        collection: None,
    });
    if let Some(collection) = collection {
        let span = syn_span(line_starts, collection);
        sink.aux
            .loop_collection_spans
            .push((index, span.start, span.end()));
    }
}

/// The impl self-type's primary name (`&Foo` / `Foo<T>` / `(Foo)` -> `Foo`),
/// None for noise/primitive/unnamable types. Port of v5 `primary_type` — the
/// method sym's owner. (`path_name` / `is_noise_type` are the existing ports
/// above, shared with the type-edge walk.)
fn primary_type(ty: &Type) -> Option<String> {
    match ty {
        Type::Group(t) => primary_type(&t.elem),
        Type::Paren(t) => primary_type(&t.elem),
        Type::Path(t) => path_name(&t.path),
        Type::Ptr(t) => primary_type(&t.elem),
        Type::Reference(t) => primary_type(&t.elem),
        _ => None,
    }
}

/// Seed the scope with param nodes, then walk the body. The block's tail
/// expression (last stmt, no semicolon) is the fn's implicit return: mint a
/// `ret` node and flow the tail into it. Port of v5 `flow_fn_body`. `fn_sym`
/// is the callable's v5 sym (only a closure node's name derives from it).
#[allow(clippy::too_many_arguments)]
fn flow_fn_body(
    sig: &syn::Signature,
    block: &syn::Block,
    fn_sym: &str,
    line_starts: &[u32],
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
    loop_breaks: &mut LoopBreaks,
) {
    // Position counts only typed params (the receiver `self` is skipped).
    let mut pos = 0u32;
    for arg in &sig.inputs {
        if let syn::FnArg::Typed(pt) = arg {
            if let syn::Pat::Ident(pi) = &*pt.pat {
                let node = df_push(
                    sink,
                    strings,
                    line_starts,
                    pi.ident.span(),
                    DfNodeKind::Param,
                    Some(&pi.ident.to_string()),
                );
                sink.aux.params.push(DfParam { node, pos });
                scope.insert(pi.ident.to_string(), node);
            }
            pos += 1;
        }
    }
    if let Some((tail, tail_span)) = flow_block(
        block,
        fn_sym,
        line_starts,
        strings,
        scope,
        sink,
        loop_breaks,
    ) {
        let ret = df_push(sink, strings, line_starts, tail_span, DfNodeKind::Ret, None);
        df_edge(sink, tail, ret);
    }
}

/// Walk a block. Returns the (node, span) of the tail value (a last statement
/// with no semicolon) so a caller can treat it as an implicit return.
#[allow(clippy::too_many_arguments)]
fn flow_block(
    block: &syn::Block,
    fn_sym: &str,
    line_starts: &[u32],
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
    loop_breaks: &mut LoopBreaks,
) -> Option<(NodeRef, proc_macro2::Span)> {
    let mut tail = None;
    let statement_count = block.stmts.len();
    for (idx, stmt) in block.stmts.iter().enumerate() {
        match stmt {
            syn::Stmt::Local(local) => {
                if let Some(init) = local.init.as_ref() {
                    let rhs = flow_expr(
                        &init.expr,
                        fn_sym,
                        line_starts,
                        strings,
                        scope,
                        sink,
                        loop_breaks,
                    );
                    // Bind every ident in the pattern (`let (a, b) = pair`), each
                    // tainted by the rhs conservatively.
                    for (_, binding) in bind_pat(&local.pat, line_starts, strings, scope, sink) {
                        df_edge(sink, rhs, binding);
                    }
                }
            }
            syn::Stmt::Expr(expr, semi) => {
                // A bare `;` is Expr::Verbatim(<empty>); its span is (0,0), so
                // skip it. Non-empty Verbatim keeps its node in the `_ =>` arm.
                if matches!(expr, syn::Expr::Verbatim(tokens) if tokens.is_empty()) {
                    continue;
                }
                let stmt_span = expr.span();
                let node = flow_expr(expr, fn_sym, line_starts, strings, scope, sink, loop_breaks);
                if idx + 1 == statement_count && semi.is_none() {
                    tail = Some((node, stmt_span));
                }
            }
            syn::Stmt::Item(_) | syn::Stmt::Macro(_) => {}
        }
    }
    tail
}

/// A call whose callee is a bare path with a capitalized last segment is a
/// tuple-struct or enum-variant constructor (`Foo(x)`, `Some(x)`). Returns the
/// constructed type/variant name, or None for an ordinary call. Port of v5
/// `ctor_name`.
fn ctor_name(expr: &syn::Expr) -> Option<String> {
    if let syn::Expr::Path(path) = expr {
        let last = path.path.segments.last()?.ident.to_string();
        if last.chars().next().is_some_and(char::is_uppercase) {
            return Some(last);
        }
    }
    None
}

/// Post-order value flow for one expression. Returns the node carrying its value
/// and emits every internal edge as a side effect. `loop_breaks` is the live
/// stack of enclosing `loop` frames for break-value routing. Port of v5
/// `flow_expr`. `fn_sym` is the enclosing callable's v5 sym (a closure node's
/// name = `{fn_sym}::closure::{line}_{col}`).
#[allow(clippy::too_many_arguments)]
fn flow_expr(
    expr: &syn::Expr,
    fn_sym: &str,
    line_starts: &[u32],
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
    loop_breaks: &mut LoopBreaks,
) -> NodeRef {
    let node_span = expr.span();
    let start = node_span.start();
    let (line, col) = (start.line as u32, start.column as u32);
    match expr {
        // A read of a variable: flow from its binding slot to this read.
        syn::Expr::Path(path) => {
            let name = path
                .path
                .segments
                .last()
                .map(|segment| segment.ident.to_string())
                .unwrap_or_default();
            let node = df_push(
                sink,
                strings,
                line_starts,
                node_span,
                DfNodeKind::VarRead,
                Some(&name),
            );
            if let Some(binding) = scope.get(&name) {
                df_edge(sink, *binding, node);
            }
            node
        }
        syn::Expr::Lit(lit_expr) => {
            let node = df_push(sink, strings, line_starts, node_span, DfNodeKind::Lit, None);
            // A string literal carries its cooked value into `df_lit` (v5
            // `rust` mints `lit` rows for `syn::Lit::Str` only).
            if let syn::Lit::Str(string) = &lit_expr.lit {
                sink.aux.lits.push(DfLit {
                    node,
                    kind: "lit",
                    text: string.value(),
                });
            }
            node
        }
        // f(args): each argument flows into the call result. A capitalized last
        // path segment is a tuple-struct / enum-variant constructor -> a `new`
        // node carrying the type name.
        syn::Expr::Call(call) => {
            if is_allocator_call(&call.func) {
                sink.aux
                    .allocator_hits
                    .push(syn_span(line_starts, node_span));
            }
            let constructor = ctor_name(&call.func);
            let mut children = Vec::new();
            for arg in &call.args {
                children.push(flow_expr(
                    arg,
                    fn_sym,
                    line_starts,
                    strings,
                    scope,
                    sink,
                    loop_breaks,
                ));
            }
            let kind = if constructor.is_some() {
                DfNodeKind::New
            } else {
                DfNodeKind::CallRes
            };
            let node = df_push(
                sink,
                strings,
                line_starts,
                node_span,
                kind,
                constructor.as_deref(),
            );
            for (pos, child) in children.into_iter().enumerate() {
                df_edge(sink, child, node);
                sink.aux.args.push(DfArg {
                    call: node,
                    pos: pos as i64,
                    arg: child,
                });
            }
            node
        }
        // recv.m(args): receiver + args flow into the result. The node sits at
        // the METHOD ident (the same line the call-site extractor records).
        syn::Expr::MethodCall(call) => {
            if is_allocator_method(&call.method) {
                sink.aux
                    .allocator_hits
                    .push(syn_span(line_starts, node_span));
            }
            let receiver = flow_expr(
                &call.receiver,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            let mut children = Vec::new();
            for arg in &call.args {
                children.push(flow_expr(
                    arg,
                    fn_sym,
                    line_starts,
                    strings,
                    scope,
                    sink,
                    loop_breaks,
                ));
            }
            let node = df_push(
                sink,
                strings,
                line_starts,
                call.method.span(),
                DfNodeKind::CallRes,
                None,
            );
            df_edge(sink, receiver, node);
            sink.aux.args.push(DfArg {
                call: node,
                pos: -1,
                arg: receiver,
            });
            for (pos, child) in children.into_iter().enumerate() {
                df_edge(sink, child, node);
                sink.aux.args.push(DfArg {
                    call: node,
                    pos: pos as i64,
                    arg: child,
                });
            }
            node
        }
        // `Foo { a: x, ..base }`: an instantiation; each field value flows into
        // the `new` node and records a `df_field` row (the functional-update
        // base under "..").
        syn::Expr::Struct(struct_expr) => {
            let type_name = struct_expr
                .path
                .segments
                .last()
                .map(|segment| segment.ident.to_string())
                .unwrap_or_default();
            let mut filled: Vec<(String, NodeRef)> = Vec::new();
            for field in &struct_expr.fields {
                let value = flow_expr(
                    &field.expr,
                    fn_sym,
                    line_starts,
                    strings,
                    scope,
                    sink,
                    loop_breaks,
                );
                let name = match &field.member {
                    syn::Member::Named(ident) => ident.to_string(),
                    syn::Member::Unnamed(index) => index.index.to_string(),
                };
                filled.push((name, value));
            }
            let base = struct_expr.rest.as_ref().map(|rest| {
                flow_expr(rest, fn_sym, line_starts, strings, scope, sink, loop_breaks)
            });
            let node = df_push(
                sink,
                strings,
                line_starts,
                node_span,
                DfNodeKind::New,
                Some(type_name.as_str()),
            );
            for (name, value) in filled {
                df_edge(sink, value, node);
                sink.aux.fields.push(DfField {
                    owner: node,
                    name,
                    value,
                });
            }
            if let Some(base) = base {
                df_edge(sink, base, node);
                sink.aux.fields.push(DfField {
                    owner: node,
                    name: "..".to_string(),
                    value: base,
                });
            }
            node
        }
        // `base.f` / `tuple.0`: a field read. The base flows into a `member` node
        // whose name is the field name (field-sensitive flow).
        syn::Expr::Field(field) => {
            let base = flow_expr(
                &field.base,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            let name = match &field.member {
                syn::Member::Named(ident) => ident.to_string(),
                syn::Member::Unnamed(index) => index.index.to_string(),
            };
            let node = df_push(
                sink,
                strings,
                line_starts,
                node_span,
                DfNodeKind::Member,
                Some(&name),
            );
            df_edge(sink, base, node);
            node
        }
        syn::Expr::Paren(paren) => flow_expr(
            &paren.expr,
            fn_sym,
            line_starts,
            strings,
            scope,
            sink,
            loop_breaks,
        ),
        syn::Expr::Reference(reference) => {
            let inner = flow_expr(
                &reference.expr,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            let node = df_push(sink, strings, line_starts, node_span, BORROW, None);
            df_edge(sink, inner, node);
            node
        }
        syn::Expr::Binary(binary) => {
            let left = flow_expr(
                &binary.left,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            let right = flow_expr(
                &binary.right,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            let node = df_push(
                sink,
                strings,
                line_starts,
                node_span,
                DfNodeKind::Binop,
                None,
            );
            df_edge(sink, left, node);
            df_edge(sink, right, node);
            node
        }
        syn::Expr::Unary(unary) => {
            let inner = flow_expr(
                &unary.expr,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            let node = df_push(
                sink,
                strings,
                line_starts,
                node_span,
                DfNodeKind::Unop,
                None,
            );
            df_edge(sink, inner, node);
            node
        }
        // Transparent pass-through: the `?` operator does not alter value flow.
        syn::Expr::Try(try_expr) => flow_expr(
            &try_expr.expr,
            fn_sym,
            line_starts,
            strings,
            scope,
            sink,
            loop_breaks,
        ),
        // `return EXPR`: the returned value flows into the fn's `ret` node.
        syn::Expr::Return(return_expr) => {
            let node = df_push(sink, strings, line_starts, node_span, DfNodeKind::Ret, None);
            if let Some(inner) = &return_expr.expr {
                let value = flow_expr(
                    inner,
                    fn_sym,
                    line_starts,
                    strings,
                    scope,
                    sink,
                    loop_breaks,
                );
                df_edge(sink, value, node);
            }
            node
        }
        // `break EXPR;`: the value's tail is recorded into the `loop_breaks`
        // frame it targets; `Expr::Loop` drains its frame's tails into edges on
        // its own node.
        syn::Expr::Break(break_expr) => {
            let node = df_push(sink, strings, line_starts, node_span, BREAK, None);
            if let Some(value_expr) = &break_expr.expr {
                let value = flow_expr(
                    value_expr,
                    fn_sym,
                    line_starts,
                    strings,
                    scope,
                    sink,
                    loop_breaks,
                );
                df_edge(sink, value, node);
                let target_label = break_expr
                    .label
                    .as_ref()
                    .map(|lifetime| lifetime.ident.to_string());
                let frame = match &target_label {
                    Some(label) => loop_breaks
                        .iter_mut()
                        .rev()
                        .find(|(frame_label, _)| frame_label.as_deref() == Some(label.as_str())),
                    None => loop_breaks.last_mut(),
                };
                if let Some((_, tails)) = frame {
                    tails.push(node);
                }
            }
            node
        }
        // `for pat in coll { body }`: bind the loop variable from the collection
        // (each element tainted conservatively), then walk the body.
        syn::Expr::ForLoop(for_loop) => {
            let collection = flow_expr(
                &for_loop.expr,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            let binds = bind_pat(&for_loop.pat, line_starts, strings, scope, sink);
            for (_, binding) in &binds {
                df_edge(sink, collection, *binding);
            }
            let loop_var = binds
                .first()
                .map(|(name, _)| name.clone())
                .unwrap_or_default();
            df_loop_row(
                sink,
                line_starts,
                node_span,
                Some(loop_var.clone()).filter(|name| !name.is_empty()),
                Some(for_loop.expr.span()),
            );
            flow_block(
                &for_loop.body,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            df_push(
                sink,
                strings,
                line_starts,
                node_span,
                DfNodeKind::Loop,
                Some(&loop_var),
            )
        }
        // `while cond { body }`: no collection; walk cond + body.
        syn::Expr::While(while_expr) => {
            let _ = flow_expr(
                &while_expr.cond,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            if let syn::Expr::Let(let_expr) = &*while_expr.cond {
                let _ = bind_pat(&let_expr.pat, line_starts, strings, scope, sink);
            }
            df_loop_row(sink, line_starts, node_span, None, None);
            flow_block(
                &while_expr.body,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            df_push(
                sink,
                strings,
                line_starts,
                node_span,
                DfNodeKind::Loop,
                None,
            )
        }
        // `loop { body }`: Rust's value-yielding loop. Push a fresh `loop_breaks`
        // frame before walking the body, pop it after, and edge every collected
        // break tail into this loop's node.
        syn::Expr::Loop(loop_expr) => {
            df_loop_row(sink, line_starts, node_span, None, None);
            loop_breaks.push((
                loop_expr
                    .label
                    .as_ref()
                    .map(|label| label.name.ident.to_string()),
                Vec::new(),
            ));
            flow_block(
                &loop_expr.body,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            let (_, break_tails) = loop_breaks
                .pop()
                .expect("Expr::Loop popping the frame it pushed");
            let node = df_push(
                sink,
                strings,
                line_starts,
                node_span,
                DfNodeKind::Loop,
                None,
            );
            for tail in break_tails {
                df_edge(sink, tail, node);
            }
            node
        }
        // `if cond { then } else { els }`: branch TAILS flow into the `if` node,
        // so a value-position if carries both branches through to the binding.
        syn::Expr::If(if_expr) => {
            let _ = flow_expr(
                &if_expr.cond,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            let then_tail = flow_block(
                &if_expr.then_branch,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            let else_tail = if_expr.else_branch.as_ref().map(|(_, els)| {
                flow_expr(els, fn_sym, line_starts, strings, scope, sink, loop_breaks)
            });
            let node = df_push(sink, strings, line_starts, node_span, DfNodeKind::If, None);
            if let Some((tail, _)) = then_tail {
                df_edge(sink, tail, node);
            }
            if let Some(else_tail) = else_tail {
                df_edge(sink, else_tail, node);
            }
            node
        }
        // `match scrut { arms }`: scrut + each arm body; arm-bound patterns derive
        // from the scrutinee. Arm tails flow into the `match` node.
        syn::Expr::Match(match_expr) => {
            let scrut = flow_expr(
                &match_expr.expr,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            let mut arm_tails = Vec::new();
            for arm in &match_expr.arms {
                for (_, binding) in bind_pat(&arm.pat, line_starts, strings, scope, sink) {
                    df_edge(sink, scrut, binding);
                }
                if let Some((_, guard)) = &arm.guard {
                    let _ = flow_expr(
                        guard,
                        fn_sym,
                        line_starts,
                        strings,
                        scope,
                        sink,
                        loop_breaks,
                    );
                }
                arm_tails.push(flow_expr(
                    &arm.body,
                    fn_sym,
                    line_starts,
                    strings,
                    scope,
                    sink,
                    loop_breaks,
                ));
            }
            let node = df_push(sink, strings, line_starts, node_span, MATCH, None);
            for tail in arm_tails {
                df_edge(sink, tail, node);
            }
            node
        }
        // `{ stmts }` as an expression: the tail statement's value flows through.
        syn::Expr::Block(block_expr) => {
            let tail = flow_block(
                &block_expr.block,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            let node = df_push(sink, strings, line_starts, node_span, BLOCK, None);
            if let Some((tail, _)) = tail {
                df_edge(sink, tail, node);
            }
            node
        }
        // `|params| body`: lift the lambda as its own scope (seed params, walk the
        // body under v5's `lam_sym`, mint a ret for the body value), then mint the
        // `closure` VALUE node in the enclosing fn carrying that exact sym as its
        // name (`{fn_sym}::closure::{line}_{col}`, syn's 1-based line / 0-based
        // col; chains when nested). The enclosing scope is shared so captures
        // resolve.
        syn::Expr::Closure(closure) => {
            let allocator_mark = sink.aux.allocator_hits.len();
            let lam_sym = format!("{fn_sym}::closure::{line}_{col}");
            for (pos, input) in closure.inputs.iter().enumerate() {
                let ident_pat = match input {
                    syn::Pat::Type(pat_type) => pat_type.pat.as_ref(),
                    other => other,
                };
                if let syn::Pat::Ident(ident) = ident_pat {
                    let node = df_push(
                        sink,
                        strings,
                        line_starts,
                        ident.ident.span(),
                        DfNodeKind::Param,
                        Some(&ident.ident.to_string()),
                    );
                    sink.aux.params.push(DfParam {
                        node,
                        pos: pos as u32,
                    });
                    scope.insert(ident.ident.to_string(), node);
                } else {
                    let _ = bind_pat(input, line_starts, strings, scope, sink);
                }
            }
            // A `break` cannot cross a closure boundary, so the body gets a fresh
            // loop_breaks stack.
            let mut closure_loop_breaks = LoopBreaks::new();
            let body_val = match closure.body.as_ref() {
                syn::Expr::Block(block) => flow_block(
                    &block.block,
                    &lam_sym,
                    line_starts,
                    strings,
                    scope,
                    sink,
                    &mut closure_loop_breaks,
                ),
                other => {
                    let other_span = other.span();
                    let value = flow_expr(
                        other,
                        &lam_sym,
                        line_starts,
                        strings,
                        scope,
                        sink,
                        &mut closure_loop_breaks,
                    );
                    Some((value, other_span))
                }
            };
            if let Some((value, ret_span)) = body_val {
                let ret = df_push(sink, strings, line_starts, ret_span, DfNodeKind::Ret, None);
                df_edge(sink, value, ret);
            }
            claim_allocator_hits(sink, allocator_mark, syn_span(line_starts, node_span));
            df_push(
                sink,
                strings,
                line_starts,
                node_span,
                DfNodeKind::Closure,
                Some(&lam_sym),
            )
        }
        // `lhs = rhs`: flow rhs; rebind a write slot so later reads see the new
        // value (taint-correct for reassignment).
        syn::Expr::Assign(assign) => {
            let rhs = flow_expr(
                &assign.right,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            if let syn::Expr::Path(path) = assign.left.as_ref() {
                if let Some(name) = path
                    .path
                    .segments
                    .last()
                    .map(|segment| segment.ident.to_string())
                {
                    let node = df_push(
                        sink,
                        strings,
                        line_starts,
                        node_span,
                        DfNodeKind::VarWrite,
                        Some(&name),
                    );
                    df_edge(sink, rhs, node);
                    scope.insert(name, node);
                    return node;
                }
            }
            rhs
        }
        // Macros (format!/println!), verbatim, and remaining variants: mint a
        // node but don't chase. Conservative: may miss flows, never invents.
        _ => df_push(
            sink,
            strings,
            line_starts,
            node_span,
            DfNodeKind::Expr,
            None,
        ),
    }
}

/// Bind every identifier in a pattern into scope, returning `(name, binding)`
/// for each. Handles single-ident + tuple / tuple-struct / struct / reference /
/// paren / slice destructuring. Port of v5 `bind_pat`/`bind_pat_rec`.
fn bind_pat(
    pattern: &syn::Pat,
    line_starts: &[u32],
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> Vec<(String, NodeRef)> {
    let mut acc = Vec::new();
    bind_pat_rec(pattern, line_starts, strings, scope, sink, &mut acc);
    acc
}

#[allow(clippy::too_many_arguments)]
fn bind_pat_rec(
    pattern: &syn::Pat,
    line_starts: &[u32],
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
    acc: &mut Vec<(String, NodeRef)>,
) {
    match pattern {
        syn::Pat::Ident(ident) => {
            let binding = df_push(
                sink,
                strings,
                line_starts,
                ident.ident.span(),
                DfNodeKind::LetBind,
                Some(&ident.ident.to_string()),
            );
            scope.insert(ident.ident.to_string(), binding);
            acc.push((ident.ident.to_string(), binding));
        }
        syn::Pat::Tuple(tuple) => {
            for elem in &tuple.elems {
                bind_pat_rec(elem, line_starts, strings, scope, sink, acc);
            }
        }
        syn::Pat::TupleStruct(tuple_struct) => {
            for elem in &tuple_struct.elems {
                bind_pat_rec(elem, line_starts, strings, scope, sink, acc);
            }
        }
        syn::Pat::Struct(struct_pat) => {
            for field in &struct_pat.fields {
                bind_pat_rec(&field.pat, line_starts, strings, scope, sink, acc);
            }
        }
        syn::Pat::Reference(reference) => {
            bind_pat_rec(&reference.pat, line_starts, strings, scope, sink, acc)
        }
        syn::Pat::Paren(paren) => bind_pat_rec(&paren.pat, line_starts, strings, scope, sink, acc),
        syn::Pat::Slice(slice) => {
            for elem in &slice.elems {
                bind_pat_rec(elem, line_starts, strings, scope, sink, acc);
            }
        }
        _ => {}
    }
}

/// Push one df node at its FULL syntactic extent: `FlatFact::Edge` carries
/// endpoint spans only, so a start-only anchor merges distinct value nodes.
fn df_push(
    sink: &mut FamilyBundle<DfF>,
    strings: &mut Strings,
    line_starts: &[u32],
    node_span: proc_macro2::Span,
    kind: DfNodeKind,
    name: Option<&str>,
) -> NodeRef {
    let node_ref = NodeRef(sink.nodes.len() as u32);
    let mut node = Node::new(syn_span(line_starts, node_span), kind);
    if let Some(name) = name.filter(|candidate| !candidate.is_empty()) {
        node = node.with_name(strings.intern(name));
    }
    sink.nodes.push(node);
    node_ref
}

/// One Direct value edge: `dst` receives the value of `src`.
fn df_edge(sink: &mut FamilyBundle<DfF>, src: NodeRef, dst: NodeRef) {
    sink.edges.push(Edge::new(src, dst, DfEdgeKind::Direct));
}

// ════════════════════════════════════════════════════════════════════════════
// RustSource: the Rust Source (cst via ast-grep + type/call/df via syn).
//
// The two-parser, masked shape (mirrors TsSource). cst runs through ast-grep
// (one dep = the CST floor for every lang); type/call/df run through ONE syn
// parse (three masked projections over the same tree). ONE shared `Strings`
// across all four families.
// ════════════════════════════════════════════════════════════════════════════

/// Re-runs `project_call` over `rust_mbe::expand_file`'s spliced text, folding
/// in only the defs/sites born inside a macro expansion, span-mapped back.
fn splice_macro_expansions(src: &str, strings: &mut Strings, bundle: &mut FamilyBundle<CallF>) {
    let Some(expanded) = super::rust_mbe::expand_file(src) else {
        return;
    };
    let Ok(expanded_parsed) = syn::parse_file(&expanded.text) else {
        return;
    };
    let expanded_line_starts = build_line_starts(&expanded.text);
    let mut expanded_bundle = FamilyBundle::<CallF>::default();
    project_call(
        &expanded_parsed,
        &expanded_line_starts,
        strings,
        &mut expanded_bundle,
    );

    for mut node in expanded_bundle.nodes {
        let range = node.span.start..node.span.start + node.span.len;
        if !expanded.is_macro_span(range.clone()) {
            continue;
        }
        if let Some(mapped) = expanded.map_span(range) {
            node.span = mapped;
            bundle.nodes.push(node);
        }
    }
    for mut site in expanded_bundle.aux.sites {
        let range = site.span.start..site.span.start + site.span.len;
        if !expanded.is_macro_span(range.clone()) {
            continue;
        }
        if let Some(mapped) = expanded.map_span(range) {
            site.span = mapped;
            bundle.aux.sites.push(site);
        }
    }
    for (span, name) in expanded.macro_sites() {
        bundle.aux.macro_sites.push(MacroSite {
            span,
            macro_name: strings.intern(name),
            source: MacroSiteSource::Mbe,
        });
    }
}

/// The Rust `Source`. `matches` = the path ends in `.rs`. cst via ast-grep's rust
/// grammar; type/call/df/const via one `syn::parse_file`.
#[derive(Default)]
pub struct RustSource;

/// Kinds only Rust constructs: the core enums do not carry them (tests/6_kind_vocab.rs).
pub const TRAIT: TypeEntityKind = TypeEntityKind::Ext(LangKind {
    lang: "rust",
    tag: "trait",
});
pub const BORROW: DfNodeKind = DfNodeKind::Ext(LangKind {
    lang: "rust",
    tag: "borrow",
});
pub const BREAK: DfNodeKind = DfNodeKind::Ext(LangKind {
    lang: "rust",
    tag: "break",
});
pub const MATCH: DfNodeKind = DfNodeKind::Ext(LangKind {
    lang: "rust",
    tag: "match",
});
pub const BLOCK: DfNodeKind = DfNodeKind::Ext(LangKind {
    lang: "rust",
    tag: "block",
});
/// A `const`/`static` item that owns calls in its initializer. Not `Free`: it
/// is a caller and never a callee, and no other language has the shape.
pub const CONST_INIT: CallKind = CallKind::Ext(LangKind {
    lang: "rust",
    tag: "const_init",
});

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
            let parsed = {
                let span = trace::parse_span("rust", "astgrep");
                let _entered = span.enter();
                AstGrepParser.parse(&arena, path, content).ok()
            };
            parsed.map(|parsed| {
                let span = trace::family_span("rust", "cst");
                let _entered = span.enter();
                let mut bundle = FamilyBundle::<CstF>::default();
                CstProjector.project(&parsed, &mut strings, &mut bundle);
                trace::record_bundle(&span, &bundle, 0);
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
                let parsed = {
                    let span = trace::parse_span("rust", "syn");
                    let _entered = span.enter();
                    syn::parse_file(src)
                };
                if let Ok(parsed) = parsed {
                    let line_starts = build_line_starts(src);
                    if mask.types {
                        let span = trace::family_span("rust", "type");
                        let _entered = span.enter();
                        let mut bundle = FamilyBundle::<TypeF>::default();
                        project_types(&parsed, &line_starts, &mut strings, &mut bundle);
                        trace::record_bundle(&span, &bundle, 0);
                        types = Some(bundle);
                    }
                    if mask.call {
                        let span = trace::family_span("rust", "call");
                        let _entered = span.enter();
                        let mut bundle = FamilyBundle::<CallF>::default();
                        project_call(&parsed, &line_starts, &mut strings, &mut bundle);
                        splice_macro_expansions(src, &mut strings, &mut bundle);
                        trace::record_bundle(&span, &bundle, bundle.aux.sites.len());
                        call = Some(bundle);
                    }
                    if mask.df {
                        let span = trace::family_span("rust", "df");
                        let _entered = span.enter();
                        let mut bundle = FamilyBundle::<DfF>::default();
                        project_df(&parsed, path, src, &line_starts, &mut strings, &mut bundle);
                        trace::record_bundle(&span, &bundle, 0);
                        df = Some(bundle);
                    }
                }
            }
        }

        ExtractOutput {
            strings,
            cst,
            types,
            call,
            df,
            data: None,
        }
    }
}
