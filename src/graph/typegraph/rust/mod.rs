//! Rust extractor arm (syn front-end): TypeLang impl, type edges,
//! entities/consts/docs, call defs/sites, dataflow. Pure code motion out
//! of the former single typegraph.rs; zero behavior change.

use std::collections::BTreeSet;

use syn::spanned::Spanned;
use syn::{
    AngleBracketedGenericArguments, Fields, GenericArgument, GenericParam, Generics, Item, Path,
    PathArguments, ReturnType, Type, TypeParamBound, WherePredicate,
};

use super::*;

impl TypeLang for RustTypes {
    fn name(&self) -> &'static str {
        "rust"
    }
    fn matches(&self, path: &str) -> bool {
        path.ends_with(".rs")
    }
    fn supports_analysis_bundle(&self) -> bool {
        true
    }
    // One syn parse feeds both the entity pass and the edge pass.
    fn extract(&self, file: &str, content: &str) -> TypeFacts {
        let Ok(parsed) = syn::parse_file(content) else {
            return TypeFacts::default();
        };
        let mut entities = rust_entities_from(&parsed, file);
        let (const_entities, consts) = rust_const_values_from(&parsed, file);
        entities.extend(const_entities);
        TypeFacts {
            entities,
            edges: edges_from(&parsed),
            docs: rust_docs_from(&parsed, file),
            consts,
            ..Default::default()
        }
    }
    // One syn parse feeds defs + sites; a follow-up folds this into `extract`
    // so a file parses once per tick instead of twice.
    fn extract_calls(&self, file: &str, content: &str) -> CallFacts {
        let Ok(parsed) = syn::parse_file(content) else {
            return CallFacts::default();
        };
        CallFacts {
            defs: rust_call_defs_from(&parsed, file),
            sites: rust_call_sites_from(&parsed, file),
        }
    }
    // One syn parse feeds the node + edge lift. Same `parse_file` cost as
    // extract/extract_calls; folding all three into one parse is a follow-up.
    fn extract_dataflow(&self, file: &str, content: &str) -> DataflowFacts {
        let Ok(parsed) = syn::parse_file(content) else {
            return DataflowFacts::default();
        };
        rust_dataflow_from(&parsed, file)
    }

    fn extract_bundle(&self, file: &str, content: &str, mask: AnalysisMask) -> AnalysisBundle {
        let Ok(parsed) = syn::parse_file(content) else {
            return AnalysisBundle::default();
        };
        let types = mask.types.then(|| {
            let mut entities = rust_entities_from(&parsed, file);
            let (const_entities, consts) = rust_const_values_from(&parsed, file);
            entities.extend(const_entities);
            TypeFacts {
                entities,
                edges: edges_from(&parsed),
                docs: rust_docs_from(&parsed, file),
                consts,
                ..Default::default()
            }
        });
        let calls = mask.calls.then(|| CallFacts {
            defs: rust_call_defs_from(&parsed, file),
            sites: rust_call_sites_from(&parsed, file),
        });
        let dataflow = mask.dataflow.then(|| rust_dataflow_from(&parsed, file));
        AnalysisBundle {
            types,
            calls,
            dataflow,
        }
    }
}

pub fn edges(content: &str) -> Vec<TypeEdge> {
    let Ok(file) = syn::parse_file(content) else {
        return Vec::new();
    };
    edges_from(&file)
}

fn edges_from(file: &syn::File) -> Vec<TypeEdge> {
    let mut out: BTreeSet<(String, String, &'static str)> = BTreeSet::new();
    for item in &file.items {
        item_edges(item, &mut out);
    }
    out.into_iter()
        .map(|(from, to, kind)| TypeEdge { from, to, kind })
        .collect()
}

fn item_edges(item: &Item, out: &mut BTreeSet<(String, String, &'static str)>) {
    match item {
        Item::Struct(s) => {
            let owner = s.ident.to_string();
            generic_edges(&owner, &s.generics, out);
            field_edges(&owner, &s.fields, out);
        }
        Item::Enum(e) => {
            let owner = e.ident.to_string();
            generic_edges(&owner, &e.generics, out);
            for v in &e.variants {
                let variant = format!("{owner}::{}", v.ident);
                push(out, &owner, &variant, "variant");
                field_edges(&variant, &v.fields, out);
            }
        }
        Item::Union(u) => {
            let owner = u.ident.to_string();
            generic_edges(&owner, &u.generics, out);
            field_edges(&owner, &Fields::Named(u.fields.clone()), out);
        }
        Item::Trait(t) => {
            let owner = t.ident.to_string();
            generic_edges(&owner, &t.generics, out);
            for bound in &t.supertraits {
                bound_edge(&owner, bound, "generic", out);
            }
        }
        Item::Impl(i) => {
            let Some(owner) = primary_type(&i.self_ty) else {
                return;
            };
            generic_edges(&owner, &i.generics, out);
            if let Some((_, path, _)) = &i.trait_ {
                if let Some(to) = path_name(path) {
                    push(out, &owner, &to, "impl");
                }
            }
        }
        _ => {}
    }
}

fn field_edges(from: &str, fields: &Fields, out: &mut BTreeSet<(String, String, &'static str)>) {
    for field in fields.iter() {
        for to in type_refs(&field.ty) {
            push(out, from, &to, "field");
        }
    }
}

fn generic_edges(
    from: &str,
    generics: &Generics,
    out: &mut BTreeSet<(String, String, &'static str)>,
) {
    for param in &generics.params {
        if let GenericParam::Type(t) = param {
            for bound in &t.bounds {
                bound_edge(from, bound, "generic", out);
            }
        }
    }
    if let Some(where_clause) = &generics.where_clause {
        for pred in &where_clause.predicates {
            if let WherePredicate::Type(t) = pred {
                for bound in &t.bounds {
                    bound_edge(from, bound, "generic", out);
                }
            }
        }
    }
}

fn bound_edge(
    from: &str,
    bound: &TypeParamBound,
    kind: &'static str,
    out: &mut BTreeSet<(String, String, &'static str)>,
) {
    if let TypeParamBound::Trait(t) = bound {
        if let Some(to) = path_name(&t.path) {
            push(out, from, &to, kind);
        }
    }
}

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

// ── Kotlin ──────────────────────────────────────────────────────────────────
//
// The tree-sitter-kotlin grammar folds `interface` into `class_declaration`
// (and `enum class` is a modifier + `enum_class_body`), so decl flavor comes
// from keyword-level token inspection, not the node kind. Edge mapping mirrors
// Rust: an interface's supertypes are "generic" (trait supertraits), a
// class/object's supertypes are "impl", val/var constructor params and body
// properties are "field", enum entries are "variant". Declared type-parameter
// names are excluded from refs (`Repo<T>(x: T)` has no edge to T), unlike the
// syn extractor which leaks them — tree-sitter hands us the param list cheap.

#[cfg(test)]
fn rust_entities(file: &str, content: &str) -> Vec<TypeEntity> {
    let Ok(parsed) = syn::parse_file(content) else {
        return Vec::new();
    };
    rust_entities_from(&parsed, file)
}

fn rust_entities_from(parsed: &syn::File, file: &str) -> Vec<TypeEntity> {
    let owner_kinds = rust_owner_kinds(parsed);
    let mut out = Vec::new();
    for item in &parsed.items {
        rust_item_entity(item, file, &owner_kinds, &mut out);
    }
    out
}

/// Top-level `const X: &str = "...";` string values (item 3's Rust slice,
/// ledgered as "if cheap" — a plain `syn::Lit::Str` initializer on a
/// module-level `const` is). Non-goals: consts inside `impl`/`mod`/fn bodies,
/// non-string consts (no entity, no row — same "don't mint for every const"
/// rule the TS lift follows), and no `as const` equivalent (Rust has none).
fn rust_const_values_from(
    parsed: &syn::File,
    file: &str,
) -> (Vec<TypeEntity>, Vec<ConstValueFact>) {
    let mut entities = Vec::new();
    let mut consts = Vec::new();
    for item in &parsed.items {
        let Item::Const(c) = item else { continue };
        let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(s),
            ..
        }) = &*c.expr
        else {
            continue;
        };
        let name = c.ident.to_string();
        let sym = mint_sym(file, EntityKind::Const, &name, None);
        let line = rust_line(c.ident.span());
        entities.push(TypeEntity {
            sym: sym.clone(),
            name,
            kind: EntityKind::Const,
            parent: None,
            file: file.to_string(),
            line,
            ty: None,
        });
        consts.push(ConstValueFact {
            sym,
            field: String::new(),
            text: s.value(),
            kind: "lit",
            file: file.to_string(),
            line,
        });
    }
    (entities, consts)
}

/// Map each top-level type declaration's name to its real `EntityKind`, so an
/// `impl` block's methods can key their `parent` to the owner's OWN entity sym
/// (`file::struct::Foo`, not a hardcoded `file::class::Foo`). An `impl` for a
/// type declared in another file has no local owner row, so no parent sym could
/// join regardless of kind; the default guess is harmless there.
fn rust_owner_kinds(parsed: &syn::File) -> std::collections::HashMap<String, EntityKind> {
    let mut m = std::collections::HashMap::new();
    for item in &parsed.items {
        match item {
            Item::Struct(s) => {
                m.insert(s.ident.to_string(), EntityKind::Struct);
            }
            Item::Enum(en) => {
                m.insert(en.ident.to_string(), EntityKind::Enum);
            }
            Item::Union(u) => {
                m.insert(u.ident.to_string(), EntityKind::Struct);
            }
            Item::Trait(t) => {
                m.insert(t.ident.to_string(), EntityKind::Trait);
            }
            _ => {}
        }
    }
    m
}

fn rust_line(span: proc_macro2::Span) -> u32 {
    span.start().line as u32
}

fn rust_item_entity(
    item: &Item,
    file: &str,
    owner_kinds: &std::collections::HashMap<String, EntityKind>,
    out: &mut Vec<TypeEntity>,
) {
    // `parent` is the bare owner name (e.g. "Engine"); the method sym uses it
    // as `Owner.name`, while the stored parent field is the owner's own sym
    // minted with the owner's REAL kind (looked up in `owner_kinds`) so it
    // equality-joins `type_entity.sym`.
    let mut e = |name: String,
                 line: u32,
                 kind: EntityKind,
                 parent: Option<String>,
                 ty: Option<TypeExpr>| {
        let parent_sym = parent.as_deref().map(|p| {
            let pk = owner_kinds.get(p).copied().unwrap_or(EntityKind::Struct);
            mint_sym(file, pk, p, None)
        });
        out.push(TypeEntity {
            sym: mint_sym(file, kind, &name, parent.as_deref()),
            name,
            kind,
            parent: parent_sym,
            file: file.to_string(),
            line,
            ty,
        });
    };
    match item {
        Item::Struct(s) => e(
            s.ident.to_string(),
            rust_line(s.ident.span()),
            EntityKind::Struct,
            None,
            None,
        ),
        Item::Enum(en) => e(
            en.ident.to_string(),
            rust_line(en.ident.span()),
            EntityKind::Enum,
            None,
            None,
        ),
        Item::Union(u) => e(
            u.ident.to_string(),
            rust_line(u.ident.span()),
            EntityKind::Struct,
            None,
            None,
        ),
        Item::Trait(t) => {
            e(
                t.ident.to_string(),
                rust_line(t.ident.span()),
                EntityKind::Trait,
                None,
                None,
            );
            let owner = Some(t.ident.to_string());
            for ti in &t.items {
                // Only default methods (a body inside the trait block) get an
                // entity row here; a bare signature (no body) has no code to
                // hang a `type_entity` on and is left to the impl side.
                if let syn::TraitItem::Fn(m) = ti {
                    if m.default.is_some() {
                        e(
                            m.sig.ident.to_string(),
                            rust_line(m.sig.ident.span()),
                            EntityKind::Method,
                            owner.clone(),
                            Some(rust_fn_type(&m.sig)),
                        );
                    }
                }
            }
        }
        Item::Fn(f) => e(
            f.sig.ident.to_string(),
            rust_line(f.sig.ident.span()),
            EntityKind::Function,
            None,
            Some(rust_fn_type(&f.sig)),
        ),
        Item::Impl(i) => {
            let owner = primary_type(&i.self_ty);
            for ii in &i.items {
                if let syn::ImplItem::Fn(m) = ii {
                    e(
                        m.sig.ident.to_string(),
                        rust_line(m.sig.ident.span()),
                        EntityKind::Method,
                        owner.clone(),
                        Some(rust_fn_type(&m.sig)),
                    );
                }
            }
        }
        _ => {}
    }
}

/// Doc-comment pass (syn): every `#[doc]` attribute on an item — the desugared
/// form of `///` / `//!` / `/** */` — becomes a `DocFact` keyed by the same sym
/// `rust_item_entity` mints. Reading attrs (not scanning lines above the decl)
/// is what makes a `#[derive(..)]` between the doc and the `struct` keyword a
/// non-issue. Tags are the rustdoc `# Section` headings.
fn rust_docs_from(parsed: &syn::File, file: &str) -> Vec<DocFact> {
    let mut out = Vec::new();
    for item in &parsed.items {
        rust_item_docs(item, file, &mut out);
    }
    out
}

fn rust_item_docs(item: &Item, file: &str, out: &mut Vec<DocFact>) {
    match item {
        Item::Struct(s) => push_doc(
            out,
            file,
            &s.attrs,
            &s.ident.to_string(),
            rust_line(s.ident.span()),
            EntityKind::Struct,
            None,
        ),
        Item::Enum(en) => push_doc(
            out,
            file,
            &en.attrs,
            &en.ident.to_string(),
            rust_line(en.ident.span()),
            EntityKind::Enum,
            None,
        ),
        Item::Union(u) => push_doc(
            out,
            file,
            &u.attrs,
            &u.ident.to_string(),
            rust_line(u.ident.span()),
            EntityKind::Struct,
            None,
        ),
        Item::Trait(t) => push_doc(
            out,
            file,
            &t.attrs,
            &t.ident.to_string(),
            rust_line(t.ident.span()),
            EntityKind::Trait,
            None,
        ),
        Item::Fn(f) => push_doc(
            out,
            file,
            &f.attrs,
            &f.sig.ident.to_string(),
            rust_line(f.sig.ident.span()),
            EntityKind::Function,
            None,
        ),
        Item::Impl(i) => {
            let owner = primary_type(&i.self_ty);
            for ii in &i.items {
                if let syn::ImplItem::Fn(m) = ii {
                    push_doc(
                        out,
                        file,
                        &m.attrs,
                        &m.sig.ident.to_string(),
                        rust_line(m.sig.ident.span()),
                        EntityKind::Method,
                        owner.as_deref(),
                    );
                }
            }
        }
        _ => {}
    }
}

fn push_doc(
    out: &mut Vec<DocFact>,
    file: &str,
    attrs: &[syn::Attribute],
    name: &str,
    line: u32,
    kind: EntityKind,
    parent: Option<&str>,
) {
    let lines = rust_doc_lines(attrs);
    if lines.is_empty() {
        return;
    }
    let text = lines.join("\n");
    out.push(DocFact {
        sym: mint_sym(file, kind, name, parent),
        line,
        tags: parse_rust_sections(&text),
        text,
    });
}

/// Collect the string values of an item's `#[doc = "..."]` attributes, one per
/// `///` line, dropping the single leading space syn keeps from `/// foo`.
fn rust_doc_lines(attrs: &[syn::Attribute]) -> Vec<String> {
    let mut lines = Vec::new();
    for a in attrs {
        if !a.path().is_ident("doc") {
            continue;
        }
        if let syn::Meta::NameValue(nv) = &a.meta {
            if let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) = &nv.value
            {
                let v = s.value();
                lines.push(v.strip_prefix(' ').unwrap_or(&v).to_string());
            }
        }
    }
    lines
}

fn rust_fn_type(sig: &syn::Signature) -> TypeExpr {
    let named = |refs: Vec<String>| refs.into_iter().map(TypeRef::Named).collect::<Vec<_>>();
    // `self` receivers are not value params; drop them so positions line up
    // with the written argument list.
    let params = sig
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            syn::FnArg::Typed(pt) => Some(named(type_refs(&pt.ty))),
            syn::FnArg::Receiver(_) => None,
        })
        .collect();
    let ret = match &sig.output {
        ReturnType::Type(_, ty) => named(type_refs(ty)),
        ReturnType::Default => Vec::new(),
    };
    TypeExpr { params, ret }
}

// --- Rust call-graph pass (syn): free functions and impl methods become
// CallDefs (sym + body span for callsite containment), and every call
// expression becomes a CallSite whose caller is left blank for the engine's
// span-containment pass to fill. Closures are collected as anonymous defs in a
// follow-up; the visitor still walks into them so calls inside a closure body
// attribute to the enclosing named def. ---

fn rust_call_defs_from(parsed: &syn::File, file: &str) -> Vec<CallDef> {
    let mut v = RustCallDefs {
        file,
        stack: Vec::new(),
        out: Vec::new(),
    };
    for item in &parsed.items {
        match item {
            // Top-level free fn: Free callable, then walk the body for nested
            // named fns and closures under this fn's sym.
            // @callable rust function
            Item::Fn(f) => {
                let name = f.sig.ident.to_string();
                let sym = mint_sym(file, EntityKind::Function, &name, None);
                let end = f.block.span().end().line as u32;
                v.emit(
                    sym.clone(),
                    name,
                    CallKind::Free,
                    rust_line(f.sig.ident.span()),
                    end,
                );
                v.walk_body(&sym, &f.block);
            }
            // Impl method: Method keyed to the impl's primary type (existing
            // identity — kept EXACTLY, `owner.as_deref()`), then walk the body.
            // @callable rust method
            Item::Impl(i) => {
                let owner = primary_type(&i.self_ty);
                for ii in &i.items {
                    if let syn::ImplItem::Fn(m) = ii {
                        let name = m.sig.ident.to_string();
                        let sym = mint_sym(file, EntityKind::Method, &name, owner.as_deref());
                        let end = m.block.span().end().line as u32;
                        v.emit(
                            sym.clone(),
                            name,
                            CallKind::Method,
                            rust_line(m.sig.ident.span()),
                            end,
                        );
                        v.walk_body(&sym, &m.block);
                    }
                }
            }
            // Trait item fns: a signature-only declaration OR a default body,
            // both Method-owned by the trait — so a call resolving through the
            // trait has a target row. A default body is walked for closures.
            // @callable rust method
            Item::Trait(t) => {
                let owner = t.ident.to_string();
                for ti in &t.items {
                    if let syn::TraitItem::Fn(m) = ti {
                        let name = m.sig.ident.to_string();
                        let sym = mint_sym(file, EntityKind::Method, &name, Some(&owner));
                        let end = match &m.default {
                            Some(block) => block.span().end().line as u32,
                            None => m.sig.span().end().line as u32,
                        };
                        v.emit(
                            sym.clone(),
                            name,
                            CallKind::Method,
                            rust_line(m.sig.ident.span()),
                            end,
                        );
                        if let Some(block) = &m.default {
                            v.walk_body(&sym, block);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    v.out
}

/// Walks fn/impl/trait bodies to collect the callables the top-level pass misses:
/// nested named fns (Free, file-level mint) and closures (Lambda). `stack`'s top
/// is the enclosing callable sym; a closure/nested-fn pushes its own sym before
/// its body is walked, so a closure-in-a-closure or closure-in-a-nested-fn chains
/// exactly like the dataflow lift. Const/static-initializer bodies and inline-mod
/// items are NOT descended — the dataflow lift (`rust_dataflow_from`) walks only
/// `Item::Fn`/`Item::Impl` too, so a lambda there would have no df scope to join;
/// documented in docs/callable-coverage.md as a shared Rust gap.
struct RustCallDefs<'a> {
    file: &'a str,
    stack: Vec<String>,
    out: Vec<CallDef>,
}

impl<'a> RustCallDefs<'a> {
    fn emit(&mut self, sym: String, name: String, kind: CallKind, line: u32, end: u32) {
        self.out.push(CallDef {
            sym,
            name,
            kind,
            file: self.file.to_string(),
            line,
            end,
        });
    }
    fn cur(&self) -> &str {
        self.stack.last().map(String::as_str).unwrap_or("")
    }
    fn walk_body(&mut self, fn_sym: &str, block: &syn::Block) {
        self.stack.push(fn_sym.to_string());
        syn::visit::visit_block(self, block);
        self.stack.pop();
    }
}

impl<'ast, 'a> syn::visit::Visit<'ast> for RustCallDefs<'a> {
    // A nested named fn (`fn helper() {}` inside a body). Reached only for nested
    // items — the top-level driver visits bodies, never the ItemFn nodes it
    // already emitted. File-level mint (df does not lift nested-fn bodies, so
    // there is no owner-scoped df sym to match).
    // @callable rust function
    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        let name = f.sig.ident.to_string();
        let sym = mint_sym(self.file, EntityKind::Function, &name, None);
        let end = f.block.span().end().line as u32;
        self.emit(
            sym.clone(),
            name,
            CallKind::Free,
            rust_line(f.sig.ident.span()),
            end,
        );
        self.walk_body(&sym, &f.block);
    }
    // A closure (`|x| ...`). Sym is `lambda_sym(enclosing, "<line>_<col>")` — the
    // SAME string `rust_dataflow_from`'s closure arm mints from the closure expr's
    // span start, so the lifted body's df nodes (fn_sym = this sym) and the
    // closure value node (var = this sym) join `call_def.sym` exactly.
    // @callable rust lambda
    fn visit_expr_closure(&mut self, c: &'ast syn::ExprClosure) {
        let start = c.span().start();
        let (line, col) = (start.line as u32, start.column as u32);
        let sym = lambda_sym(self.cur(), &format!("{line}_{col}"));
        let end = c.body.span().end().line as u32;
        self.emit(sym.clone(), String::new(), CallKind::Lambda, line, end);
        // Walk only the body (params hold no callables) under the closure sym.
        self.stack.push(sym);
        syn::visit::visit_expr(self, &c.body);
        self.stack.pop();
    }
}

/// The trailing identifier of a callee expression's source text: the last run
/// of alnum/underscore. `helper` -> "helper", `Vec::new` -> "new",
/// `self.foo.bar` -> "bar". Used to key the bare-name resolver the same way
/// `type_link` resolves a type reference.
fn rust_call_sites_from(parsed: &syn::File, file: &str) -> Vec<CallSite> {
    let mut v = CallCollector {
        file,
        sites: Vec::new(),
    };
    syn::visit::visit_file(&mut v, parsed);
    v.sites
}

struct CallCollector<'a> {
    file: &'a str,
    sites: Vec<CallSite>,
}

impl<'a> syn::visit::Visit<'a> for CallCollector<'a> {
    fn visit_expr(&mut self, e: &'a syn::Expr) {
        match e {
            // `f(args)` / `Foo(args)`: callee is the path's trailing segment;
            // callee_path carries the full qualified path when >1 segment.
            syn::Expr::Call(c) => {
                let func = peel_parens(&c.func);
                if let syn::Expr::Path(p) = func {
                    if let Some(seg) = p.path.segments.last() {
                        let path_str = path_string(&p.path);
                        self.sites.push(CallSite {
                            caller_sym: None,
                            callee: seg.ident.to_string(),
                            callee_path: (p.path.segments.len() > 1).then_some(path_str),
                            file: self.file.to_string(),
                            line: c.func.span().start().line as u32,
                        });
                    }
                }
                syn::visit::visit_expr(self, e);
            }
            // `recv.m(args)`: callee is the method ident.
            syn::Expr::MethodCall(m) => {
                self.sites.push(CallSite {
                    caller_sym: None,
                    callee: m.method.to_string(),
                    callee_path: None,
                    file: self.file.to_string(),
                    line: m.method.span().start().line as u32,
                });
                syn::visit::visit_expr(self, e);
            }
            // `Foo { x: 1 }`: struct literal constructor. callee is the type
            // path's trailing segment; callee_path carries the full path.
            syn::Expr::Struct(s) => {
                if let Some(seg) = s.path.segments.last() {
                    let path_str = path_string(&s.path);
                    self.sites.push(CallSite {
                        caller_sym: None,
                        callee: seg.ident.to_string(),
                        callee_path: (s.path.segments.len() > 1).then_some(path_str),
                        file: self.file.to_string(),
                        line: s.path.span().start().line as u32,
                    });
                }
                syn::visit::visit_expr(self, e);
            }
            _ => syn::visit::visit_expr(self, e),
        }
    }
}

/// Render a syn::Path as `a::b::c`.
fn path_string(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

/// Strip nested `Expr::Paren` to find the inner expression.
fn peel_parens(e: &syn::Expr) -> &syn::Expr {
    let mut cur = e;
    while let syn::Expr::Paren(p) = cur {
        cur = &p.expr;
    }
    cur
}

// --- Rust intra-procedural dataflow lift (syn). The lift is two rules:
//   (1) post-order, every value-bearing child expression flows into its
//       value-bearing parent (args into a call, operands into a binop, the
//       referent into a borrow);
//   (2) storage — `let x = rhs` binds rhs -> x_slot, and a later read of x
//       flows slot -> read. Params seed the scope as pre-bound slots.
// Macros, control-flow arms, and anything syn doesn't expose as a child Expr
// get a node but no chased edges (conservative: may miss flows, never invents
// them). `df_reaches <- closure(df_edge)` does the rest on the shared SCC engine.

fn rust_dataflow_from(parsed: &syn::File, file: &str) -> DataflowFacts {
    let mut out = DataflowFacts::default();
    for item in &parsed.items {
        match item {
            Item::Fn(f) => {
                let sym = mint_sym(file, EntityKind::Function, &f.sig.ident.to_string(), None);
                flow_fn_body(&sym, &f.sig, &f.block, file, &mut out);
            }
            Item::Impl(i) => {
                let owner = primary_type(&i.self_ty);
                for ii in &i.items {
                    if let syn::ImplItem::Fn(m) = ii {
                        let sym = match &owner {
                            Some(o) => mint_sym(
                                file,
                                EntityKind::Method,
                                &m.sig.ident.to_string(),
                                Some(o),
                            ),
                            None => {
                                mint_sym(file, EntityKind::Function, &m.sig.ident.to_string(), None)
                            }
                        };
                        flow_fn_body(&sym, &m.sig, &m.block, file, &mut out);
                    }
                }
            }
            _ => {}
        }
    }
    out.nests = compute_nests(&out.nodes, &out.loops);
    out
}

/// Seed the scope with param nodes, then walk the body. The scope maps a var
/// name to the id of its binding node (param or `let`); a read looks it up and
/// emits slot -> read. Flat function-level scope (no block shadowing) is the
/// v0 approximation.
fn flow_fn_body(
    fn_sym: &str,
    sig: &syn::Signature,
    block: &syn::Block,
    file: &str,
    out: &mut DataflowFacts,
) {
    let mut scope: std::collections::HashMap<String, NodeIdx> = std::collections::HashMap::new();
    // Position counts only typed params (the receiver `self` is skipped), so the
    // index aligns with `type_sig`, which also drops self.
    let mut pos: u32 = 0;
    for arg in &sig.inputs {
        if let syn::FnArg::Typed(pt) = arg {
            if let syn::Pat::Ident(pi) = &*pt.pat {
                let (l, c) = (
                    pi.ident.span().start().line as u32,
                    pi.ident.span().start().column as u32,
                );
                let id = push_node(out, file, l, c, "param", &pi.ident.to_string(), fn_sym);
                out.param_pos.push((id.clone(), pos));
                scope.insert(pi.ident.to_string(), id);
            }
            pos += 1;
        }
    }
    // The block's tail expression (last stmt, no semicolon) is the fn's implicit
    // return value: mint a `ret` node and flow the tail into it. `return EXPR`
    // anywhere in the body is handled in `flow_expr` (Expr::Return). The `ret`
    // node is the interprocedural sink the backward flow hop reads — a callee's
    // returned value reaches the caller's `call_res`.
    //
    // `loop_breaks` is the stack of live enclosing `loop` frames — each entry is
    // (label, collected break-value tail ids) — threaded through every recursive
    // flow_block/flow_expr call so `Expr::Break` can find the loop it targets and
    // `Expr::Loop` can drain the tails it collected. Starts empty at the fn body.
    let mut loop_breaks: Vec<(Option<String>, Vec<NodeIdx>)> = Vec::new();
    if let Some((tail, l, c)) = flow_block(block, file, fn_sym, &mut scope, out, &mut loop_breaks) {
        let ret = push_node(out, file, l, c, "ret", "", fn_sym);
        out.edges.push(DfEdge {
            from: tail,
            to: ret,
        });
    }
}

/// Walk a block. Returns the (id, line, col) of the block's tail value — the last
/// statement when it is a no-semicolon expression — so a caller (a fn body) can
/// treat it as an implicit return. Nested-block callers ignore the result.
fn flow_block(
    b: &syn::Block,
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, NodeIdx>,
    out: &mut DataflowFacts,
    loop_breaks: &mut Vec<(Option<String>, Vec<NodeIdx>)>,
) -> Option<(NodeIdx, u32, u32)> {
    let mut tail = None;
    let n = b.stmts.len();
    for (idx, stmt) in b.stmts.iter().enumerate() {
        match stmt {
            syn::Stmt::Local(loc) => {
                if let Some(init) = loc.init.as_ref() {
                    let rhs = flow_expr(&init.expr, file, fn_sym, scope, out, loop_breaks);
                    // bind every ident in the pattern (handles `let (a, b) = pair`),
                    // each tainted by the rhs conservatively.
                    for (_, bid) in bind_pat(&loc.pat, file, fn_sym, scope, out) {
                        out.edges.push(DfEdge {
                            from: rhs.clone(),
                            to: bid,
                        });
                    }
                }
            }
            syn::Stmt::Expr(e, semi) => {
                let start = e.span().start();
                let id = flow_expr(e, file, fn_sym, scope, out, loop_breaks);
                if idx + 1 == n && semi.is_none() {
                    tail = Some((id, start.line as u32, start.column as u32));
                }
            }
            syn::Stmt::Item(_) => {}
            syn::Stmt::Macro(_) => {}
        }
    }
    tail
}

/// A call expression whose callee is a bare path with a capitalized last
/// segment is a tuple-struct or enum-variant constructor (`Foo(x)`,
/// `Some(x)`, `mod::Variant(x)`) under Rust naming convention — functions are
/// snake_case. Returns the constructed type/variant name, or None for an
/// ordinary call. `Foo::new(x)` stays a call: `new` is lowercase.
fn ctor_name(e: &syn::Expr) -> Option<String> {
    if let syn::Expr::Path(p) = e {
        let last = p.path.segments.last()?.ident.to_string();
        if last.chars().next().is_some_and(|c| c.is_uppercase()) {
            return Some(last);
        }
    }
    None
}

/// A call whose callee is a collection constructor (`Vec::new`, `HashMap::new`,
/// `String::new`, ...) marks its enclosing fn as allocating — the cost signal
/// for the loop-invariant-call flag. Conservative: catches the common shapes,
/// may miss ad-hoc allocators behind wrappers or macros.
fn is_allocator_call(e: &syn::Expr) -> bool {
    if let syn::Expr::Path(p) = e {
        let segs: Vec<String> = p
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect();
        let full = segs.join("::");
        if full.ends_with("::new") {
            return segs.iter().any(|s| {
                matches!(
                    s.as_str(),
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
        if matches!(
            full.as_str(),
            "Vec::with_capacity" | "HashMap::with_capacity" | "String::with_capacity"
        ) {
            return true;
        }
    }
    false
}

/// A method that builds a fresh owned value per call: `.collect()`, `.to_vec()`,
/// `.to_string()`, `.to_owned()`, `.clone()`. Conservative — `.clone()` of a
/// cheap-Copy type does not allocate, but the false positive is benign for a
/// suspect-list filter.
fn is_allocator_method(ident: &syn::Ident) -> bool {
    matches!(
        ident.to_string().as_str(),
        "collect" | "to_vec" | "to_string" | "to_owned" | "clone" | "format"
    )
}

/// Post-order value flow for one expression. Returns the node id for `e` and
/// emits every internal edge as a side effect. `loop_breaks` is the live stack
/// of enclosing `loop` frames (see `flow_fn_body`) — `Expr::Loop` pushes/pops
/// its own frame, `Expr::Break` records its value's tail into the frame it
/// targets, every other arm just forwards the stack unchanged.
fn flow_expr(
    e: &syn::Expr,
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, NodeIdx>,
    out: &mut DataflowFacts,
    loop_breaks: &mut Vec<(Option<String>, Vec<NodeIdx>)>,
) -> NodeIdx {
    let start = e.span().start();
    let (line, col) = (start.line as u32, start.column as u32);
    match e {
        // a read of a variable: flow from its binding slot to this read.
        syn::Expr::Path(p) => {
            let name = p
                .path
                .segments
                .last()
                .map(|s| s.ident.to_string())
                .unwrap_or_default();
            let id = push_node(out, file, line, col, "var_read", &name, fn_sym);
            if let Some(b) = scope.get(&name) {
                out.edges.push(DfEdge {
                    from: b.clone(),
                    to: id.clone(),
                });
            }
            id
        }
        syn::Expr::Lit(lit_expr) => {
            let id = push_node(out, file, line, col, "lit", "", fn_sym);
            if let syn::Lit::Str(s) = &lit_expr.lit {
                out.lits.push((id.clone(), s.value(), "lit"));
            }
            id
        }
        // f(args): each argument flows into the call result, and `df_arg`
        // records its 0-based slot so the interprocedural hop can join it
        // against `df_param`/`type_sig` by position. A capitalized last path
        // segment is a tuple-struct / enum-variant constructor (`Foo(x)`,
        // `Some(x)`) — those become `new` nodes carrying the type name, since
        // they build a value rather than resolve through the call graph.
        syn::Expr::Call(c) => {
            if is_allocator_call(&c.func) {
                out.allocators.insert(fn_sym.to_string());
            }
            let ctor = ctor_name(&c.func);
            let mut children = Vec::new();
            for arg in &c.args {
                children.push(flow_expr(arg, file, fn_sym, scope, out, loop_breaks));
            }
            let (kind, var) = match &ctor {
                Some(n) => ("new", n.as_str()),
                None => ("call_res", ""),
            };
            let id = push_node(out, file, line, col, kind, var, fn_sym);
            for (pos, child) in children.into_iter().enumerate() {
                out.edges.push(DfEdge {
                    from: child.clone(),
                    to: id.clone(),
                });
                out.args.push((id.clone(), pos as i64, child));
            }
            id
        }
        // recv.m(args): receiver + args flow into the result; method name
        // skipped. The receiver is `df_arg` slot -1 (mirroring the skipped
        // `self` in `df_param`), args count 0.. so they align with the
        // callee's typed params.
        syn::Expr::MethodCall(m) => {
            if is_allocator_method(&m.method) {
                out.allocators.insert(fn_sym.to_string());
            }
            let recv = flow_expr(&m.receiver, file, fn_sym, scope, out, loop_breaks);
            let mut children = Vec::new();
            for arg in &m.args {
                children.push(flow_expr(arg, file, fn_sym, scope, out, loop_breaks));
            }
            // The node sits at the METHOD ident, not the receiver expression's
            // start — the same line the call-site extractor records, so the
            // (file, line) call_node join holds for a multiline builder chain.
            let msp = m.method.span().start();
            let id = push_node(
                out,
                file,
                msp.line as u32,
                msp.column as u32,
                "call_res",
                "",
                fn_sym,
            );
            out.edges.push(DfEdge {
                from: recv.clone(),
                to: id.clone(),
            });
            out.args.push((id.clone(), -1, recv));
            for (pos, child) in children.into_iter().enumerate() {
                out.edges.push(DfEdge {
                    from: child.clone(),
                    to: id.clone(),
                });
                out.args.push((id.clone(), pos as i64, child));
            }
            id
        }
        // `Foo { a: x, ..base }`: an instantiation. Each field value flows into
        // the `new` node and `df_field` records which field it fills — the
        // field-sensitive half the blanket edge can't express. A functional-
        // update base flows in under the pseudo-field "..".
        syn::Expr::Struct(s) => {
            let ty = s
                .path
                .segments
                .last()
                .map(|sg| sg.ident.to_string())
                .unwrap_or_default();
            let mut filled: Vec<(String, NodeIdx)> = Vec::new();
            for f in &s.fields {
                let v = flow_expr(&f.expr, file, fn_sym, scope, out, loop_breaks);
                let name = match &f.member {
                    syn::Member::Named(i) => i.to_string(),
                    syn::Member::Unnamed(i) => i.index.to_string(),
                };
                filled.push((name, v));
            }
            let base = s
                .rest
                .as_ref()
                .map(|r| flow_expr(r, file, fn_sym, scope, out, loop_breaks));
            let id = push_node(out, file, line, col, "new", &ty, fn_sym);
            for (name, v) in filled {
                out.edges.push(DfEdge {
                    from: v.clone(),
                    to: id.clone(),
                });
                out.fields.push((id.clone(), name, v));
            }
            if let Some(b) = base {
                out.edges.push(DfEdge {
                    from: b.clone(),
                    to: id.clone(),
                });
                out.fields.push((id.clone(), "..".into(), b));
            }
            id
        }
        // `base.f` / `tuple.0`: a field read. The base flows into a `member`
        // node whose var is the field name, so a query can match a `df_field`
        // write against the read of the same field (field-sensitive flow).
        syn::Expr::Field(f) => {
            let base = flow_expr(&f.base, file, fn_sym, scope, out, loop_breaks);
            let name = match &f.member {
                syn::Member::Named(i) => i.to_string(),
                syn::Member::Unnamed(i) => i.index.to_string(),
            };
            let id = push_node(out, file, line, col, "member", &name, fn_sym);
            out.edges.push(DfEdge {
                from: base,
                to: id.clone(),
            });
            id
        }
        syn::Expr::Paren(p) => flow_expr(&p.expr, file, fn_sym, scope, out, loop_breaks),
        syn::Expr::Reference(r) => {
            let inner = flow_expr(&r.expr, file, fn_sym, scope, out, loop_breaks);
            let id = push_node(out, file, line, col, "borrow", "", fn_sym);
            out.edges.push(DfEdge {
                from: inner,
                to: id.clone(),
            });
            id
        }
        syn::Expr::Binary(b) => {
            let l = flow_expr(&b.left, file, fn_sym, scope, out, loop_breaks);
            let r = flow_expr(&b.right, file, fn_sym, scope, out, loop_breaks);
            let id = push_node(out, file, line, col, "binop", "", fn_sym);
            out.edges.push(DfEdge {
                from: l,
                to: id.clone(),
            });
            out.edges.push(DfEdge {
                from: r,
                to: id.clone(),
            });
            id
        }
        syn::Expr::Unary(u) => {
            let inner = flow_expr(&u.expr, file, fn_sym, scope, out, loop_breaks);
            let id = push_node(out, file, line, col, "unop", "", fn_sym);
            out.edges.push(DfEdge {
                from: inner,
                to: id.clone(),
            });
            id
        }
        // transparent pass-through: the ? operator does not alter value flow.
        syn::Expr::Try(t) => flow_expr(&t.expr, file, fn_sym, scope, out, loop_breaks),
        // `return EXPR`: the returned value flows into the fn's `ret` node — the
        // sink the interprocedural backward hop reads.
        syn::Expr::Return(r) => {
            let id = push_node(out, file, line, col, "ret", "", fn_sym);
            if let Some(inner) = &r.expr {
                let v = flow_expr(inner, file, fn_sym, scope, out, loop_breaks);
                out.edges.push(DfEdge {
                    from: v,
                    to: id.clone(),
                });
            }
            id
        }
        // `break EXPR;` / `break 'label EXPR;`: Rust's only value-yielding break
        // (`while`/`for` breaks never carry a value, so this is the loop-only
        // counterpart of the if/match/block tail routing above). The value's
        // tail id is recorded into the `loop_breaks` frame it targets — the
        // innermost live `loop` frame for an unlabeled break, or the frame whose
        // label matches for a labeled one — and `Expr::Loop` drains its frame's
        // collected tails into edges on its own node when it finishes walking
        // its body, exactly mirroring how `then_tail`/`arm_tails` feed the `if`/
        // `match` node. Only `Expr::Loop` ever pushes a frame (`while`/`for`
        // can't be break-value targets in valid Rust), so a label that resolves
        // to a `while`/`for` loop — never legal for a value-carrying break —
        // finds no frame: the value still gets its own node (never silently
        // dropped), it just has nowhere to route to.
        syn::Expr::Break(brk) => {
            let id = push_node(out, file, line, col, "break", "", fn_sym);
            if let Some(value_expr) = &brk.expr {
                let value_id = flow_expr(value_expr, file, fn_sym, scope, out, loop_breaks);
                out.edges.push(DfEdge {
                    from: value_id,
                    to: id.clone(),
                });
                let target_label = brk.label.as_ref().map(|lt| lt.ident.to_string());
                let frame = match &target_label {
                    Some(label) => loop_breaks
                        .iter_mut()
                        .rev()
                        .find(|(frame_label, _)| frame_label.as_deref() == Some(label.as_str())),
                    None => loop_breaks.last_mut(),
                };
                if let Some((_, tails)) = frame {
                    tails.push(id.clone());
                }
            }
            id
        }
        // `for pat in coll { body }`: bind the loop variable from the collection,
        // record the loop span so loop_over can flag loop-invariant calls inside
        // it, then walk the body. Each element taints the loop var conservatively.
        syn::Expr::ForLoop(f) => {
            let coll = flow_expr(&f.expr, file, fn_sym, scope, out, loop_breaks);
            let binds = bind_pat(&f.pat, file, fn_sym, scope, out);
            // the whole collection taints each bound element conservatively
            // (a tuple element derives from the iterator's yield value).
            for (_, bid) in &binds {
                out.edges.push(DfEdge {
                    from: coll.clone(),
                    to: bid.clone(),
                });
            }
            let lvar = binds.first().map(|(n, _)| n.clone()).unwrap_or_default();
            let end = f.body.span().end().line as u32;
            out.loops.push(LoopFact {
                file: file.into(),
                start: line,
                end,
                var: lvar.clone(),
                collection: String::new(),
                fn_sym: fn_sym.into(),
            });
            // No `loop_breaks` frame here: a `for` loop cannot yield a value
            // through `break` in Rust, so there is nothing to route. Its body
            // still shares the same live stack — a `loop` nested inside pushes
            // and pops its own frame regardless of what encloses it.
            flow_block(&f.body, file, fn_sym, scope, out, loop_breaks);
            push_node(out, file, line, col, "loop", &lvar, fn_sym)
        }
        // `while cond { body }`: `while let` is ExprWhile with cond = Expr::Let.
        // No collection, but the span is still recorded so calls in the body can
        // be flagged. Same no-frame reasoning as `for` above: `while` can't
        // yield a break value either.
        syn::Expr::While(w) => {
            let _ = flow_expr(&w.cond, file, fn_sym, scope, out, loop_breaks);
            if let syn::Expr::Let(l) = &*w.cond {
                let _ = bind_pat(&l.pat, file, fn_sym, scope, out);
            }
            let end = w.body.span().end().line as u32;
            out.loops.push(LoopFact {
                file: file.into(),
                start: line,
                end,
                var: String::new(),
                collection: String::new(),
                fn_sym: fn_sym.into(),
            });
            flow_block(&w.body, file, fn_sym, scope, out, loop_breaks);
            push_node(out, file, line, col, "loop", "", fn_sym)
        }
        // `loop { body }`: Rust's only value-yielding loop construct — a
        // `break EXPR` anywhere inside (including nested `if`/`match`/inner
        // loops) supplies the value of the whole `loop` expression, the same
        // way a block's tail supplies a block's value. Push a fresh
        // `loop_breaks` frame (carrying this loop's label, if any) before
        // walking the body so nested `Expr::Break` calls have somewhere to
        // record their tail; pop it after and edge every collected tail into
        // this loop's own node, mirroring the if/match/block tail routing.
        syn::Expr::Loop(l) => {
            let end = l.body.span().end().line as u32;
            out.loops.push(LoopFact {
                file: file.into(),
                start: line,
                end,
                var: String::new(),
                collection: String::new(),
                fn_sym: fn_sym.into(),
            });
            let label = l.label.as_ref().map(|lbl| lbl.name.ident.to_string());
            loop_breaks.push((label, Vec::new()));
            flow_block(&l.body, file, fn_sym, scope, out, loop_breaks);
            let (_, break_tails) = loop_breaks
                .pop()
                .expect("Expr::Loop popping the frame it just pushed");
            let id = push_node(out, file, line, col, "loop", "", fn_sym);
            for tail in break_tails {
                out.edges.push(DfEdge {
                    from: tail,
                    to: id.clone(),
                });
            }
            id
        }
        // `if cond { then } else { els }`: flow each branch; taint is the union.
        // Branch TAILS flow into the `if` node itself, so a value-position if
        // (`let x = if c { a() } else { b() }`) carries a()/b() through to the
        // binding instead of dead-ending at the branch (the arch_df starvation).
        syn::Expr::If(i) => {
            let _ = flow_expr(&i.cond, file, fn_sym, scope, out, loop_breaks);
            let then_tail = flow_block(&i.then_branch, file, fn_sym, scope, out, loop_breaks);
            let else_tail = i
                .else_branch
                .as_ref()
                .map(|(_, els)| flow_expr(els, file, fn_sym, scope, out, loop_breaks));
            let id = push_node(out, file, line, col, "if", "", fn_sym);
            if let Some((t, _, _)) = then_tail {
                out.edges.push(DfEdge {
                    from: t,
                    to: id.clone(),
                });
            }
            if let Some(e) = else_tail {
                out.edges.push(DfEdge {
                    from: e,
                    to: id.clone(),
                });
            }
            id
        }
        // `match scrut { arms }`: scrut + each arm body; guards too. Arm-bound
        // patterns (`Stmt::Expr(e) => ...`) derive from the scrutinee, so each is
        // tainted by it — this is what makes match-bound vars track as loop-carried
        // when the scrutinee is the loop variable.
        syn::Expr::Match(m) => {
            let scrut = flow_expr(&m.expr, file, fn_sym, scope, out, loop_breaks);
            let mut arm_tails = Vec::new();
            for arm in &m.arms {
                for (_, bid) in bind_pat(&arm.pat, file, fn_sym, scope, out) {
                    out.edges.push(DfEdge {
                        from: scrut.clone(),
                        to: bid,
                    });
                }
                if let Some((_, g)) = &arm.guard {
                    let _ = flow_expr(g, file, fn_sym, scope, out, loop_breaks);
                }
                arm_tails.push(flow_expr(&arm.body, file, fn_sym, scope, out, loop_breaks));
            }
            // Arm tails flow into the `match` node: a value-position match
            // carries every arm's value to the consumer (same as `if` above).
            let id = push_node(out, file, line, col, "match", "", fn_sym);
            for t in arm_tails {
                out.edges.push(DfEdge {
                    from: t,
                    to: id.clone(),
                });
            }
            id
        }
        // `{ stmts }` as an expression: reuse the block walker; the tail
        // statement's value flows through the block node.
        syn::Expr::Block(b) => {
            let tail = flow_block(&b.block, file, fn_sym, scope, out, loop_breaks);
            let id = push_node(out, file, line, col, "block", "", fn_sym);
            if let Some((t, _, _)) = tail {
                out.edges.push(DfEdge {
                    from: t,
                    to: id.clone(),
                });
            }
            id
        }
        // `|params| body`: lift the lambda as its OWN fn scope — kind "param"
        // nodes with df_param slots, body walked under the lambda sym, the body
        // result flowing into a "ret" node — so a higher-order hop (see
        // std/flow.dl flow_lambda) can feed its params and read its result. The
        // `closure` VALUE node stays in the enclosing fn (it is the argument a
        // df_arg row records) and carries the lambda sym in `var`, the join key
        // between the value and its lifted scope. The enclosing scope is shared,
        // so captures still resolve (a read of an outer var links to its slot).
        syn::Expr::Closure(c) => {
            let lam_sym = lambda_sym(fn_sym, &format!("{line}_{col}"));
            let mut pos: u32 = 0;
            for inp in &c.inputs {
                // `|x|` is Pat::Ident; `|x: T|` wraps it in Pat::Type. Either
                // way the single-ident case gets a positional param node;
                // destructuring patterns bind without a slot (conservative).
                let ident_pat = match inp {
                    syn::Pat::Type(pt) => pt.pat.as_ref(),
                    other => other,
                };
                if let syn::Pat::Ident(pi) = ident_pat {
                    let sp = pi.ident.span().start();
                    let id = push_node(
                        out,
                        file,
                        sp.line as u32,
                        sp.column as u32,
                        "param",
                        &pi.ident.to_string(),
                        &lam_sym,
                    );
                    out.param_pos.push((id.clone(), pos));
                    scope.insert(pi.ident.to_string(), id);
                } else {
                    let _ = bind_pat(inp, file, &lam_sym, scope, out);
                }
                pos += 1;
            }
            // A `break` cannot cross a closure boundary in Rust (it would be a
            // compile error), so the closure body gets its own fresh, empty
            // `loop_breaks` stack rather than inheriting the enclosing fn's —
            // a `loop` written inside the closure pushes/pops onto this one.
            let mut closure_loop_breaks: Vec<(Option<String>, Vec<NodeIdx>)> = Vec::new();
            let body_val = match c.body.as_ref() {
                syn::Expr::Block(b) => flow_block(
                    &b.block,
                    file,
                    &lam_sym,
                    scope,
                    out,
                    &mut closure_loop_breaks,
                ),
                other => {
                    let sp = other.span().start();
                    Some((
                        flow_expr(other, file, &lam_sym, scope, out, &mut closure_loop_breaks),
                        sp.line as u32,
                        sp.column as u32,
                    ))
                }
            };
            if let Some((v, l, cl)) = body_val {
                let ret = push_node(out, file, l, cl, "ret", "", &lam_sym);
                out.edges.push(DfEdge { from: v, to: ret });
            }
            push_node(out, file, line, col, "closure", &lam_sym, fn_sym)
        }
        // `lhs = rhs`: flow rhs, rebind a write slot so later reads see the new
        // value (taint-correct for reassignment). Compound assignment (`+=`) and
        // macros fall through to the conservative default below.
        syn::Expr::Assign(a) => assign_flow(
            &a.left,
            &a.right,
            file,
            line,
            col,
            fn_sym,
            scope,
            out,
            loop_breaks,
        ),
        // macros (format!/println!), verbatim, and remaining variants: syn exposes
        // these as token streams or non-Expr children, so mint a node but don't
        // chase. Conservative — may miss flows into macro args, never invents.
        _ => push_node(out, file, line, col, "expr", "", fn_sym),
    }
}

/// Bind every identifier in a pattern into scope, returning `(name, bind_id)`
/// for each. Handles the common single-ident case plus tuple / tuple-struct /
/// struct / reference / paren destructuring — so `for (r, cs) in iter` and
/// `let (a, b) = pair` bind both elements, not just the outer pattern. Wildcards
/// and literals in patterns bind nothing.
fn bind_pat(
    pat: &syn::Pat,
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, NodeIdx>,
    out: &mut DataflowFacts,
) -> Vec<(String, NodeIdx)> {
    let mut acc = Vec::new();
    bind_pat_rec(pat, file, fn_sym, scope, out, &mut acc);
    acc
}

fn bind_pat_rec(
    pat: &syn::Pat,
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, NodeIdx>,
    out: &mut DataflowFacts,
    acc: &mut Vec<(String, NodeIdx)>,
) {
    match pat {
        syn::Pat::Ident(pi) => {
            let (l, c) = (
                pi.ident.span().start().line as u32,
                pi.ident.span().start().column as u32,
            );
            let bind = push_node(out, file, l, c, "let_bind", &pi.ident.to_string(), fn_sym);
            scope.insert(pi.ident.to_string(), bind.clone());
            acc.push((pi.ident.to_string(), bind));
        }
        syn::Pat::Tuple(t) => {
            for e in &t.elems {
                bind_pat_rec(e, file, fn_sym, scope, out, acc);
            }
        }
        syn::Pat::TupleStruct(ts) => {
            for e in &ts.elems {
                bind_pat_rec(e, file, fn_sym, scope, out, acc);
            }
        }
        syn::Pat::Struct(s) => {
            for f in &s.fields {
                bind_pat_rec(&f.pat, file, fn_sym, scope, out, acc);
            }
        }
        syn::Pat::Reference(r) => bind_pat_rec(&r.pat, file, fn_sym, scope, out, acc),
        syn::Pat::Paren(p) => bind_pat_rec(&p.pat, file, fn_sym, scope, out, acc),
        syn::Pat::Slice(s) => {
            for e in &s.elems {
                bind_pat_rec(e, file, fn_sym, scope, out, acc);
            }
        }
        _ => {}
    }
}

/// `lhs = rhs`: flow the rhs; if the lhs is a bare path, mint a var_write slot,
/// edge rhs -> slot, and rebind in scope so later reads pick up the new reaching
/// def (taint-correct under reassignment).
fn assign_flow(
    lhs: &syn::Expr,
    rhs: &syn::Expr,
    file: &str,
    line: u32,
    col: u32,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, NodeIdx>,
    out: &mut DataflowFacts,
    loop_breaks: &mut Vec<(Option<String>, Vec<NodeIdx>)>,
) -> NodeIdx {
    let r = flow_expr(rhs, file, fn_sym, scope, out, loop_breaks);
    if let syn::Expr::Path(p) = lhs {
        if let Some(name) = p.path.segments.last().map(|s| s.ident.to_string()) {
            let id = push_node(out, file, line, col, "var_write", &name, fn_sym);
            out.edges.push(DfEdge {
                from: r.clone(),
                to: id.clone(),
            });
            scope.insert(name, id.clone());
            return id;
        }
    }
    r
}

#[cfg(test)]
mod tests;
