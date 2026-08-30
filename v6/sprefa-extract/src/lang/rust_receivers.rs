//! Receiver typing for the rust call arm, the go #554/#562 twin: a method
//! site `x.m()` binds through `x`'s declared type T when an `impl` block in
//! the corpus defines `m` for T. The corpus-wide (T, m) -> def table is
//! built by the module plane's second parse (`rust_modules.rs`, which calls
//! `impl_facts` below); the per-site receiver outcome is phase 1
//! (`CallFAux.receivers`), one row per method-call site: `Named(T)` when the
//! compiler could see the type in scope, `Inferred` when it could not.
//!
//! Type sources, in the order the compiler uses them: a param annotation,
//! `let x: T`, `self` (the enclosing impl's self type), a struct field type,
//! and ONE hop `let x = f()` through `f`'s declared return type
//! (`-> Result<T, _>`/`-> Option<T>` take T). Everything else stays
//! `Inferred`; resolution never guesses and never invents an edge.

use crate::shape::{Span, Strings};
use crate::types::{FamilyBundle, CallF, ReceiverBinding, ReceiverOutcome};

use super::rust::{def_span, syn_span};

use syn::spanned::Spanned as _;

fn spanned<T: syn::spanned::Spanned>(t: &T) -> &T {
    t
}

/// One impl block's contribution to the corpus (T, m) table: the self type's
/// name (generics stripped) and every fn inside it with its def span (the
/// same span math phase 1 mints def nodes with, so the table's spans ARE
/// edge destinations).
#[derive(Clone, Debug)]
pub(crate) struct ImplEntry {
    pub(crate) self_type: String,
    /// `Some` for `impl Trait for T`: the trait's principal name, which the
    /// inherent-before-trait tiebreak reads.
    pub(crate) trait_name: Option<String>,
    pub(crate) methods: Vec<(String, Span)>,
}

/// Every impl block in a parsed file, inline `mod x { .. }` bodies included.
pub(crate) fn impl_facts(parsed: &syn::File, line_starts: &[u32]) -> Vec<ImplEntry> {
    let mut out = Vec::new();
    impls_in_items(&parsed.items, line_starts, &mut out);
    out
}

fn impls_in_items(items: &[syn::Item], line_starts: &[u32], out: &mut Vec<ImplEntry>) {
    for item in items {
        match item {
            syn::Item::Impl(imp) => {
                if let Some(self_type) = principal_ty(&imp.self_ty) {
                    let trait_name = imp.trait_.as_ref().and_then(|(_, path, _)| {
                        path.segments.last().map(|segment| segment.ident.to_string())
                    });
                    let methods = imp
                        .items
                        .iter()
                        .filter_map(|item| match item {
                            syn::ImplItem::Fn(f) => Some((
                                f.sig.ident.to_string(),
                                def_span(
                                    line_starts,
                                    spanned(&f.sig.ident).span(),
                                    spanned(&f.block).span(),
                                ),
                            )),
                            _ => None,
                        })
                        .collect();
                    out.push(ImplEntry { self_type, trait_name, methods });
                }
            }
            syn::Item::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    impls_in_items(inner, line_starts, out);
                }
            }
            _ => {}
        }
    }
}

/// A type's principal name: the last path segment, generics stripped,
/// `&`/`*`/parens peeled. `Result`/`Option` unwrap one level to their first
/// type argument (`-> Result<T, _>` takes T).
fn principal_ty(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Reference(r) => principal_ty(&r.elem),
        syn::Type::Ptr(p) => principal_ty(&p.elem),
        syn::Type::Paren(p) => principal_ty(&p.elem),
        syn::Type::Group(g) => principal_ty(&g.elem),
        syn::Type::Path(p) => {
            let segment = p.path.segments.last()?;
            let ident = segment.ident.to_string();
            if matches!(ident.as_str(), "Result" | "Option") {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        return principal_ty(inner);
                    }
                }
            }
            Some(ident)
        }
        // `dyn Trait` / `impl Trait`: the receiver's type IS the trait, the
        // trait-dispatch leg's input.
        syn::Type::TraitObject(t) => single_bound_trait(&t.bounds),
        syn::Type::ImplTrait(t) => single_bound_trait(&t.bounds),
        _ => None,
    }
}

/// The one trait a `dyn`/`impl` bound names; `A + B` multi-bounds bind none.
fn single_bound_trait(bounds: &syn::punctuated::Punctuated<syn::TypeParamBound, syn::token::Plus>) -> Option<String> {
    if bounds.len() != 1 {
        return None;
    }
    match bounds.first()? {
        syn::TypeParamBound::Trait(tb) => {
            Some(tb.path.segments.last()?.ident.to_string())
        }
        _ => None,
    }
}

/// A fn signature's declared output type, `principal_ty` applied.
fn output_ty(sig: &syn::Signature) -> Option<String> {
    match &sig.output {
        syn::ReturnType::Type(_, ty) => principal_ty(ty),
        syn::ReturnType::Default => None,
    }
}

/// Generic param name -> every trait bound on it, from the param list and
/// the where clause (`fn f<T: Iter>(t: T)` / `where T: Display`).
fn trait_bounds_of_generics(generics: &syn::Generics) -> std::collections::HashMap<String, Vec<String>> {
    let mut out: std::collections::HashMap<String, Vec<String>> = Default::default();
    let mut push_bound = |name: String, bound: &syn::TypeParamBound| {
        if let syn::TypeParamBound::Trait(tb) = bound {
            if let Some(segment) = tb.path.segments.last() {
                out.entry(name).or_default().push(segment.ident.to_string());
            }
        }
    };
    for param in &generics.params {
        if let syn::GenericParam::Type(tp) = param {
            let name = tp.ident.to_string();
            for bound in &tp.bounds {
                push_bound(name.clone(), bound);
            }
        }
    }
    if let Some(where_clause) = &generics.where_clause {
        for predicate in &where_clause.predicates {
            if let syn::WherePredicate::Type(pred) = predicate {
                if let Some(name) = principal_ty(&pred.bounded_ty) {
                    for bound in &pred.bounds {
                        push_bound(name.clone(), bound);
                    }
                }
            }
        }
    }
    out
}

/// One name's binding in a scope frame. `Unknown` covers an untyped `let`
/// whose initializer is not a one-hop call.
#[derive(Clone, Debug, PartialEq, Eq)]
enum TypeBinding {
    Named(String),
    Unknown,
}

/// The receiver walk: one visit per file, `ReceiverBinding` per method-call
/// site appended to `sink.aux.receivers`.
struct ReceiverWalk<'a> {
    line_starts: &'a [u32],
    strings: &'a mut Strings,
    /// Same-file fn name -> declared return type (the one-hop table).
    rets: std::collections::HashMap<String, String>,
    /// (struct, field) -> type, same-file structs only.
    fields: std::collections::HashMap<(String, String), String>,
    /// Enclosing impl self types, outermost first.
    impl_stack: Vec<String>,
    scopes: Vec<std::collections::HashMap<String, TypeBinding>>,
    out: Vec<ReceiverBinding>,
}

impl<'ast, 'a> syn::visit::Visit<'ast> for ReceiverWalk<'a> {
    fn visit_item_impl(&mut self, imp: &'ast syn::ItemImpl) {
        if let Some(self_type) = principal_ty(&imp.self_ty) {
            self.impl_stack.push(self_type);
            syn::visit::visit_item_impl(self, imp);
            self.impl_stack.pop();
        } else {
            syn::visit::visit_item_impl(self, imp);
        }
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        self.scopes.push(Default::default());
        self.seed_params(&f.sig);
        syn::visit::visit_item_fn(self, f);
        self.scopes.pop();
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        self.scopes.push(Default::default());
        self.seed_params(&f.sig);
        syn::visit::visit_impl_item_fn(self, f);
        self.scopes.pop();
    }

    fn visit_local(&mut self, local: &'ast syn::Local) {
        let bound = match &local.pat {
            syn::Pat::Ident(pat) => Some((
                pat.ident.to_string(),
                self.one_hop_init(local.init.as_ref().map(|i| i.expr.as_ref())).unwrap_or(TypeBinding::Unknown),
            )),
            syn::Pat::Type(pat) => match &*pat.pat {
                syn::Pat::Ident(inner) => {
                    let binding = principal_ty(&pat.ty)
                        .map(TypeBinding::Named)
                        .or_else(|| self.one_hop_init(local.init.as_ref().map(|i| i.expr.as_ref())))
                        .unwrap_or(TypeBinding::Unknown);
                    Some((inner.ident.to_string(), binding))
                }
                _ => None,
            },
            // `let (a, b) = ..` / `let Some(x) = .. else`: no name this walk
            // can type, and an initializer that still holds call sites.
            _ => None,
        };
        // The initializer is read in the OUTER scope: `let a = a.tick()` types
        // its receiver through the binding it shadows, never through itself.
        syn::visit::visit_local(self, local);
        if let Some((name, binding)) = bound {
            self.insert(name, binding);
        }
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        let outcome = self.receiver_outcome(&call.receiver);
        let span = syn_span(self.line_starts, call.method.span());
        self.out.push(ReceiverBinding { call_site: span, outcome });
        syn::visit::visit_expr_method_call(self, call);
    }
}

impl<'a> ReceiverWalk<'a> {
    fn insert(&mut self, name: String, binding: TypeBinding) {
        let Some(frame) = self.scopes.last_mut() else {
            return;
        };
        if let Some(existing) = frame.get(&name) {
            if *existing != binding {
                frame.insert(name, TypeBinding::Unknown);
                return;
            }
        }
        frame.insert(name, binding);
    }

    fn lookup(&self, name: &str) -> Option<&TypeBinding> {
        self.scopes
            .iter()
            .rev()
            .find_map(|frame| frame.get(name))
    }

    /// `let x = f(..)`: the callee's same-file declared return type, one hop.
    fn one_hop_init(&self, init: Option<&syn::Expr>) -> Option<TypeBinding> {
        let mut current = init?;
        loop {
            match current {
                syn::Expr::Paren(p) => current = &p.expr,
                syn::Expr::Reference(r) => current = &r.expr,
                syn::Expr::Try(t) => current = &t.expr,
                _ => break,
            }
        }
        let syn::Expr::Call(c) = current else {
            return None;
        };
        let syn::Expr::Path(p) = c.func.as_ref() else {
            return None;
        };
        let name = p.path.segments.last()?.ident.to_string();
        self.rets.get(&name).cloned().map(TypeBinding::Named)
    }

    fn receiver_outcome(&mut self, expr: &syn::Expr) -> ReceiverOutcome {
        let mut current = expr;
        loop {
            match current {
                syn::Expr::Reference(r) => current = &r.expr,
                syn::Expr::Paren(p) => current = &p.expr,
                syn::Expr::Group(g) => current = &g.expr,
                syn::Expr::Unary(u) if matches!(u.op, syn::UnOp::Deref(_)) => current = &u.expr,
                syn::Expr::Path(p) if p.path.segments.len() == 1 => {
                    let ident = p.path.segments[0].ident.to_string();
                    if ident == "self" {
                        return match self.impl_stack.last() {
                            Some(ty) => ReceiverOutcome::Named(self.strings.intern(ty)),
                            None => ReceiverOutcome::Inferred,
                        };
                    }
                    let bound = self
                        .lookup(&ident)
                        .and_then(|b| match b {
                            TypeBinding::Named(ty) => Some(ty.clone()),
                            _ => None,
                        })
                        .and_then(|ty| self.resolve_self(&ty));
                    return match bound {
                        Some(ty) => ReceiverOutcome::Named(self.strings.intern(&ty)),
                        None => ReceiverOutcome::Inferred,
                    };
                }
                syn::Expr::Field(f) => {
                    let (base, member) = (&f.base, &f.member);
                    let Some(base_ty) = self.base_type(base) else {
                        return ReceiverOutcome::Inferred;
                    };
                    let syn::Member::Named(ident) = member else {
                        return ReceiverOutcome::Inferred;
                    };
                    let field_ty = self
                        .fields
                        .get(&(base_ty, ident.to_string()))
                        .cloned()
                        .and_then(|ty| self.resolve_self(&ty));
                    return match field_ty {
                        Some(ty) => ReceiverOutcome::Named(self.strings.intern(&ty)),
                        None => ReceiverOutcome::Inferred,
                    };
                }
                _ => return ReceiverOutcome::Inferred,
            }
        }
    }

    /// `Self` written as a type is the enclosing impl's self type; outside an
    /// impl it names nothing this walk can bind.
    fn resolve_self(&self, ty: &str) -> Option<String> {
        if ty == "Self" {
            self.impl_stack.last().cloned()
        } else {
            Some(ty.to_string())
        }
    }

    /// A receiver base expression's type, for field lookups: `self`,
    /// a scope-bound ident.
    fn base_type(&self, expr: &syn::Expr) -> Option<String> {
        match expr {
            syn::Expr::Path(p) if p.path.segments.len() == 1 => {
                let ident = p.path.segments[0].ident.to_string();
                if ident == "self" {
                    self.impl_stack.last().cloned()
                } else {
                    match self.lookup(&ident) {
                        Some(TypeBinding::Named(ty)) => self.resolve_self(ty),
                        _ => None,
                    }
                }
            }
            _ => None,
        }
    }

    fn seed_params(&mut self, sig: &syn::Signature) {
        let generic_bounds = trait_bounds_of_generics(&sig.generics);
        for input in &sig.inputs {
            let syn::FnArg::Typed(arg) = input else {
                continue;
            };
            let syn::Pat::Ident(pat) = &*arg.pat else {
                continue;
            };
            if let Some(ty) = principal_ty(&arg.ty) {
                // A param typed by the fn's OWN generic param resolves to the
                // param's single trait bound (class 6b); an unbound or
                // multi-bound param stays as written.
                let binding = generic_bounds.get(&ty).and_then(|bounds| match bounds.as_slice() {
                    [trait_name] => Some(trait_name.clone()),
                    _ => None,
                });
                self.insert(pat.ident.to_string(), TypeBinding::Named(binding.unwrap_or(ty)));
            }
        }
    }
}

/// Pre-pass: fn return types and struct field types, one walk.
fn tables(items: &[syn::Item], rets: &mut std::collections::HashMap<String, String>, fields: &mut std::collections::HashMap<(String, String), String>) {
    for item in items {
        match item {
            syn::Item::Fn(f) => {
                let name = f.sig.ident.to_string();
                if let Some(ty) = output_ty(&f.sig) {
                    rets.entry(name).or_insert(ty);
                }
            }
            syn::Item::Impl(imp) => {
                let Some(self_type) = principal_ty(&imp.self_ty) else {
                    continue;
                };
                for item in &imp.items {
                    if let syn::ImplItem::Fn(f) = item {
                        let name = f.sig.ident.to_string();
                        // `fn new() -> Self` returns the block's own type.
                        if let Some(ty) = output_ty(&f.sig) {
                            let ty = if ty == "Self" { self_type.clone() } else { ty };
                            rets.entry(name).or_insert(ty);
                        }
                    }
                }
            }
            syn::Item::Struct(s) => {
                let struct_name = s.ident.to_string();
                if let syn::Fields::Named(named) = &s.fields {
                    for field in &named.named {
                        if let (Some(field_name), Some(ty)) =
                            (&field.ident, principal_ty(&field.ty))
                        {
                            fields
                                .entry((struct_name.clone(), field_name.to_string()))
                                .or_insert(ty);
                        }
                    }
                }
            }
            syn::Item::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    tables(inner, rets, fields);
                }
            }
            _ => {}
        }
    }
}

/// Phase-1 entry: one `ReceiverBinding` per method-call site in the file.
pub(crate) fn collect_receivers(
    parsed: &syn::File,
    line_starts: &[u32],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let mut rets = Default::default();
    let mut fields = Default::default();
    tables(&parsed.items, &mut rets, &mut fields);
    let mut walk = ReceiverWalk {
        line_starts,
        strings,
        rets,
        fields,
        impl_stack: Vec::new(),
        scopes: Vec::new(),
        out: Vec::new(),
    };
    syn::visit::visit_file(&mut walk, parsed);
    sink.aux.receivers.append(&mut walk.out);
}
