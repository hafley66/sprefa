//! Diet type graph extractors. Intentionally syntax-only: parse a file (`syn`
//! for Rust, tree-sitter for Kotlin), walk item/type shapes, and emit
//! deterministic edges the engine stores as `type_edge(from, to, kind)`.
//! Both languages share one kind vocabulary — field | variant | impl |
//! generic — so closure queries written for one work on the other.

use std::collections::BTreeSet;

use syn::{
    AngleBracketedGenericArguments, Fields, GenericArgument, GenericParam, Generics, Item, Path,
    PathArguments, ReturnType, Type, TypeParamBound, WherePredicate,
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TypeEdge {
    pub from: String,
    pub to: String,
    pub kind: &'static str,
}

pub fn edges(content: &str) -> Vec<TypeEdge> {
    let Ok(file) = syn::parse_file(content) else {
        return Vec::new();
    };
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

fn push(
    out: &mut BTreeSet<(String, String, &'static str)>,
    from: &str,
    to: &str,
    kind: &'static str,
) {
    if from == to || is_noise_type(to) {
        return;
    }
    out.insert((from.to_string(), to.to_string(), kind));
}

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

pub fn kotlin_edges(content: &str) -> Vec<TypeEdge> {
    let mut parser = tree_sitter::Parser::new();
    let lang = tree_sitter::Language::new(tree_sitter_kotlin_sg::LANGUAGE);
    if parser.set_language(&lang).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(content, None) else {
        return Vec::new();
    };
    let src = content.as_bytes();
    let mut out: BTreeSet<(String, String, &'static str)> = BTreeSet::new();
    walk_kotlin(tree.root_node(), src, &mut out);
    out.into_iter()
        .map(|(from, to, kind)| TypeEdge { from, to, kind })
        .collect()
}

fn walk_kotlin(node: tree_sitter::Node, src: &[u8], out: &mut BTreeSet<(String, String, &'static str)>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(child.kind(), "class_declaration" | "object_declaration") {
            kotlin_decl_edges(child, src, out);
        }
        walk_kotlin(child, src, out);
    }
}

fn kotlin_decl_edges(decl: tree_sitter::Node, src: &[u8], out: &mut BTreeSet<(String, String, &'static str)>) {
    let text = |n: tree_sitter::Node| n.utf8_text(src).unwrap_or("").to_string();
    let mut cursor = decl.walk();
    let children: Vec<tree_sitter::Node> = decl.children(&mut cursor).collect();

    let Some(owner) = children.iter().find(|n| n.kind() == "type_identifier").map(|n| text(*n)) else {
        return;
    };
    // keyword-level split: `interface` is an anonymous token under the same
    // class_declaration node kind as `class`
    let is_interface = children.iter().any(|n| n.kind() == "interface");
    let super_kind: &'static str = if is_interface { "generic" } else { "impl" };

    // declared type-parameter names; their bounds are "generic" edges and the
    // names themselves are not type refs
    let mut params: BTreeSet<String> = BTreeSet::new();
    for n in &children {
        if n.kind() != "type_parameters" { continue; }
        let mut c = n.walk();
        for tp in n.children(&mut c).filter(|n| n.kind() == "type_parameter") {
            let mut cc = tp.walk();
            let kids: Vec<tree_sitter::Node> = tp.children(&mut cc).collect();
            if let Some(name) = kids.iter().find(|n| n.kind() == "type_identifier") {
                params.insert(text(*name));
            }
            for bound in kids.iter().filter(|n| n.kind() != "type_identifier") {
                for to in kotlin_type_refs(*bound, src, &params) {
                    push(out, &owner, &to, "generic");
                }
            }
        }
    }

    for n in &children {
        match n.kind() {
            "delegation_specifier" => {
                // constructor_invocation = superclass call, bare user_type =
                // interface; both are supertypes, kind set by the owner flavor
                for to in kotlin_type_refs(*n, src, &params) {
                    push(out, &owner, &to, super_kind);
                }
            }
            "primary_constructor" => {
                let mut c = n.walk();
                for param in n.children(&mut c).filter(|n| n.kind() == "class_parameter") {
                    let mut cc = param.walk();
                    let kids: Vec<tree_sitter::Node> = param.children(&mut cc).collect();
                    // val/var (binding_pattern_kind) makes it a field; a bare
                    // constructor arg is not part of the type's shape
                    if !kids.iter().any(|n| n.kind() == "binding_pattern_kind") { continue; }
                    for kid in kids.iter().filter(|n| n.kind() != "simple_identifier") {
                        for to in kotlin_type_refs(*kid, src, &params) {
                            push(out, &owner, &to, "field");
                        }
                    }
                }
            }
            "class_body" => {
                let mut c = n.walk();
                for prop in n.children(&mut c).filter(|n| n.kind() == "property_declaration") {
                    let mut cc = prop.walk();
                    for vd in prop.children(&mut cc).filter(|n| n.kind() == "variable_declaration") {
                        for to in kotlin_type_refs(vd, src, &params) {
                            push(out, &owner, &to, "field");
                        }
                    }
                }
            }
            "enum_class_body" => {
                let mut c = n.walk();
                for entry in n.children(&mut c).filter(|n| n.kind() == "enum_entry") {
                    let mut cc = entry.walk();
                    let name = entry.children(&mut cc).find(|n| n.kind() == "simple_identifier");
                    if let Some(name) = name {
                        let variant = format!("{owner}::{}", text(name));
                        push(out, &owner, &variant, "variant");
                    }
                }
            }
            _ => {}
        }
    }
}

/// Collect type names referenced anywhere under `node`: each `user_type`'s own
/// dotted path is one ref, its `type_arguments` recurse into more refs.
fn kotlin_type_refs(node: tree_sitter::Node, src: &[u8], params: &BTreeSet<String>) -> Vec<String> {
    let mut out = Vec::new();
    collect_kotlin_refs(node, src, params, &mut out);
    out.sort();
    out.dedup();
    out
}

fn collect_kotlin_refs(node: tree_sitter::Node, src: &[u8], params: &BTreeSet<String>, out: &mut Vec<String>) {
    if node.kind() == "user_type" {
        let mut cursor = node.walk();
        let segs: Vec<String> = node.children(&mut cursor)
            .filter(|n| n.kind() == "type_identifier")
            .map(|n| n.utf8_text(src).unwrap_or("").to_string())
            .collect();
        let name = segs.join(".");
        if !name.is_empty() && !params.contains(&name) && !is_noise_kotlin(&name) {
            out.push(name);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor).filter(|n| n.kind() != "type_identifier") {
            collect_kotlin_refs(child, src, params, out);
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_kotlin_refs(child, src, params, out);
    }
}

fn is_noise_kotlin(name: &str) -> bool {
    matches!(
        name,
        "Int" | "Long" | "Short" | "Byte" | "Float" | "Double" | "Boolean" | "Char"
            | "String" | "Unit" | "Any" | "Nothing"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has(got: &[TypeEdge], from: &str, to: &str, kind: &'static str) -> bool {
        got.contains(&TypeEdge { from: from.into(), to: to.into(), kind })
    }

    #[test]
    fn kotlin_fields_supers_variants_and_generics() {
        let src = r#"
package com.app
interface Pricing
abstract class Repo<T : Entity>(val store: Store, var meta: Meta?, ctor: Wire) : Base(1), Pricing {
    val cache: Cache<Item> = Cache()
}
object Single : Pricing
enum class Color(val rgb: Int) { RED, GREEN }
"#;
        let got = kotlin_edges(src);
        assert!(has(&got, "Repo", "Store", "field"), "{got:?}");
        assert!(has(&got, "Repo", "Meta", "field"), "{got:?}");
        assert!(has(&got, "Repo", "Cache", "field"), "{got:?}");
        assert!(has(&got, "Repo", "Item", "field"), "{got:?}");
        assert!(has(&got, "Repo", "Base", "impl"), "{got:?}");
        assert!(has(&got, "Repo", "Pricing", "impl"), "{got:?}");
        assert!(has(&got, "Repo", "Entity", "generic"), "{got:?}");
        assert!(has(&got, "Single", "Pricing", "impl"), "{got:?}");
        assert!(has(&got, "Color", "Color::RED", "variant"), "{got:?}");
        assert!(has(&got, "Color", "Color::GREEN", "variant"), "{got:?}");
        // bare ctor arg is not a field; type params and builtins are not refs
        assert!(!got.iter().any(|e| e.to == "Wire"), "{got:?}");
        assert!(!got.iter().any(|e| e.to == "T"), "{got:?}");
        assert!(!got.iter().any(|e| e.to == "Int"), "{got:?}");
    }

    #[test]
    fn kotlin_interface_supertypes_are_generic_kind() {
        let src = "interface Tiered : Pricing\nclass Flat : Pricing\n";
        let got = kotlin_edges(src);
        assert!(has(&got, "Tiered", "Pricing", "generic"), "{got:?}");
        assert!(has(&got, "Flat", "Pricing", "impl"), "{got:?}");
    }

    #[test]
    fn kotlin_nested_and_qualified_types() {
        let src = r#"
class Outer {
    class Inner(val link: com.lib.Remote)
}
"#;
        let got = kotlin_edges(src);
        assert!(has(&got, "Inner", "com.lib.Remote", "field"), "{got:?}");
    }

    #[test]
    fn extracts_fields_variants_impls_and_generics() {
        let src = r#"
            trait Identity {}
            trait Store {}
            struct Id;
            struct Meta<T>(T);
            struct User<T: Identity> { id: Id, meta: Option<Meta<T>> }
            enum Event { Created(User<Id>), Deleted { id: Id } }
            impl<T: Identity> Store for User<T> {}
        "#;
        let got = edges(src);
        assert!(got.contains(&TypeEdge {
            from: "User".into(),
            to: "Id".into(),
            kind: "field"
        }));
        assert!(got.contains(&TypeEdge {
            from: "User".into(),
            to: "Identity".into(),
            kind: "generic"
        }));
        assert!(got.contains(&TypeEdge {
            from: "Event".into(),
            to: "Event::Created".into(),
            kind: "variant"
        }));
        assert!(got.contains(&TypeEdge {
            from: "Event::Created".into(),
            to: "User".into(),
            kind: "field"
        }));
        assert!(got.contains(&TypeEdge {
            from: "User".into(),
            to: "Store".into(),
            kind: "impl"
        }));
    }
}
