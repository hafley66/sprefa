//! Rust type-edge candidate collection: the unresolved TypeF candidates
//! (field/variant/generic/impl/uses) that `Resolve<TypeF>` binds. Port of v5
//! `edges_from`.

use syn::{Fields, GenericParam, Path, Type, TypeParamBound, WherePredicate};

use crate::family::{TypeEdgeCandidate, TypeEdgeKind, TypeF};
use crate::rows::FamilyBundle;
use crate::shape::{Span, Strings};

use super::rust::syn_span;
use super::rust_type_refs::{collect_path_args, path_name, primary_type, type_refs};

// ── type-edge candidates (4d-i; the Resolve<TypeF> input) ───────────────────
//
// A candidate carries an owner SPAN, so an impl on a self type declared
// OUTSIDE this file has no owner to point at and is skipped.

/// Collect one file's unresolved type-edge candidates. Port of v5 `edges_from`
/// + `item_edges`.
pub(crate) fn edge_candidates(
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
                // STAY text (the 4b-iii ruling).
                push_candidate(
                    sink,
                    strings,
                    owner,
                    &format!("{}::{}", e.ident, variant.ident),
                    TypeEdgeKind::Variant,
                );
                field_candidates(owner, &variant.fields, strings, sink);
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
        // The right-hand side is walked as a field type is: head plus every
        // generic argument, the `type_refs` recursion.
        syn::Item::Type(a) => {
            let owner = syn_span(line_starts, a.ident.span());
            generic_candidates(owner, &a.generics, strings, sink);
            for to in type_refs(&a.ty) {
                push_candidate(sink, strings, owner, &to, TypeEdgeKind::Uses);
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
                arg_candidates(owner, path, strings, sink);
            }
            // The self type's own HEAD names the owner; only its arguments are
            // references.
            if let Type::Path(self_path) = strip_type(&i.self_ty) {
                arg_candidates(owner, &self_path.path, strings, sink);
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
        arg_candidates(owner, &t.path, strings, sink);
    }
}

/// One candidate per named reference under a path's GENERIC ARGUMENTS, the
/// `collect_path_args` recursion a field type already gets through `type_refs`.
fn arg_candidates(
    owner: Span,
    path: &Path,
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let mut args = Vec::new();
    collect_path_args(path, &mut args);
    args.sort();
    args.dedup();
    for to in args {
        push_candidate(sink, strings, owner, &to, TypeEdgeKind::Generic);
    }
}

/// A type with its wrappers peeled: `&mut Foo<T>` and `(Foo<T>)` are `Foo<T>`.
pub(crate) fn strip_type(ty: &Type) -> &Type {
    match ty {
        Type::Group(t) => strip_type(&t.elem),
        Type::Paren(t) => strip_type(&t.elem),
        Type::Ptr(t) => strip_type(&t.elem),
        Type::Reference(t) => strip_type(&t.elem),
        other => other,
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

