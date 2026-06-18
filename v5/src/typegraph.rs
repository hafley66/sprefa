//! Diet type graph extractors. Intentionally syntax-only: parse a file (`syn`
//! for Rust, tree-sitter for Kotlin, oxc for TypeScript/TSX), walk item/type
//! shapes, and emit deterministic edges the engine stores as
//! `type_edge(from, to, kind)`. All languages share one kind vocabulary —
//! field | variant | impl | generic — so closure queries written for one
//! work on the others. The TS extractor additionally treats functions as
//! edge owners: param | returns | uses (input types, the output type, and
//! types referenced in the body), so a function node reaches the types it
//! consumes, produces, and mentions.

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

/// What a declared symbol is. sem's entity_type, shared across languages so the
/// deck can style a function differently from a data type. The `tag` is the
/// short slug used in a symbol id and in the `type_entity.kind` column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityKind {
    Struct,
    Enum,
    Trait,
    Class,
    Interface,
    Alias,
    Function,
    Method,
    Const,
}

impl EntityKind {
    pub fn tag(self) -> &'static str {
        match self {
            EntityKind::Struct => "struct",
            EntityKind::Enum => "enum",
            EntityKind::Trait => "trait",
            EntityKind::Class => "class",
            EntityKind::Interface => "interface",
            EntityKind::Alias => "alias",
            EntityKind::Function => "function",
            EntityKind::Method => "method",
            EntityKind::Const => "const",
        }
    }
    /// Functions and methods carry an arrow type; everything else is a data type.
    pub fn is_callable(self) -> bool {
        matches!(self, EntityKind::Function | EntityKind::Method)
    }
}

/// One slot in a function's arrow type. Resolution later binds `Named` to a
/// definition symbol (`Resolved`) or leaves it `Unresolved` (stdlib/extern).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeRef {
    Named(String),
    Resolved(String),
    Unresolved(String),
}

impl TypeRef {
    pub fn name(&self) -> &str {
        match self {
            TypeRef::Named(s) | TypeRef::Resolved(s) | TypeRef::Unresolved(s) => s,
        }
    }
}

/// A function *is* a type: `[...A] => B`. `params` is the ordered input refs,
/// `ret` the output ref. A param slot with several refs (e.g. a union) keeps
/// them all; an empty slot means a non-type (keyword/primitive) parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeExpr {
    pub params: Vec<Vec<TypeRef>>,
    pub ret: Vec<TypeRef>,
}

/// A declared type-or-function entity: sem's SemanticEntity trimmed to what the
/// type graph needs (identity, kind, location, parent, and -- for callables --
/// the arrow type). No content/hashes; those live in the spine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeEntity {
    pub sym: String,
    pub name: String,
    pub kind: EntityKind,
    pub parent: Option<String>,
    pub file: String,
    pub line: u32,
    pub ty: Option<TypeExpr>,
}

/// One language's extraction of a file: declared entities + the flat edge graph.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypeFacts {
    pub entities: Vec<TypeEntity>,
    pub edges: Vec<TypeEdge>,
}

/// Phase D call-graph extraction: callable definitions + the raw call sites a
/// file contains. Caller resolution (which def encloses a site) is a second
/// pass in the engine, not the extractor's job; extractors emit sites with the
/// callee text as it appears (bare or qualified), and the engine resolves to a
/// def sym when unique, the same path `type_link` uses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CallFacts {
    pub defs: Vec<CallDef>,
    pub sites: Vec<CallSite>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallDef {
    pub sym: String,        // file::function::name (free) or file::method::Parent.name
    pub kind: CallKind,
    pub file: String,
    pub line: u32,
    pub end: u32,           // body span end (1-based line), for callsite containment
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallSite {
    pub caller_sym: Option<String>,   // filled by the engine's span-containment pass
    pub callee: String,               // bare/qualified text; resolved to a def sym when unique
    pub file: String,
    pub line: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallKind {
    Free,
    Method,
    Closure,
}

impl CallKind {
    pub fn tag(self) -> &'static str {
        match self {
            CallKind::Free => "function",
            CallKind::Method => "method",
            CallKind::Closure => "closure",
        }
    }
}

/// sem-style symbol id: `file::kind::name`, scoped by an optional parent for
/// methods (`file::method::Class.name`). Stable, index-free, human-readable.
pub fn mint_sym(file: &str, kind: EntityKind, name: &str, parent: Option<&str>) -> String {
    match parent {
        Some(p) => format!("{file}::{}::{p}.{name}", kind.tag()),
        None => format!("{file}::{}::{name}", kind.tag()),
    }
}

/// The common interface: a language front-end that recognizes paths and turns a
/// file's source into `TypeFacts`. The per-language specifics (syn, tree-sitter,
/// oxc) live behind this; the engine asks the registry, never the extension.
pub trait TypeLang: Sync {
    fn name(&self) -> &'static str;
    fn matches(&self, path: &str) -> bool;
    fn extract(&self, file: &str, content: &str) -> TypeFacts;
    /// Phase D call-graph extraction. The default returns empty `CallFacts` so
    /// the lazy-indexer wiring (`CALL_RELS`) is live end to end with zero rows;
    /// each front-end overrides this as its extractor lands. One parse can feed
    /// both `extract` and `extract_calls`, but that join is a follow-up.
    fn extract_calls(&self, _file: &str, _content: &str) -> CallFacts { CallFacts::default() }
}

/// Registry order matters: `.kts` matches before `.ts` would, so KotlinTypes
/// must precede TsTypes. The engine picks the first `matches` hit.
pub fn type_langs() -> &'static [&'static dyn TypeLang] {
    &[&RustTypes, &KotlinTypes, &TsTypes]
}

pub struct RustTypes;
pub struct KotlinTypes;
pub struct TsTypes;

impl TypeLang for RustTypes {
    fn name(&self) -> &'static str { "rust" }
    fn matches(&self, path: &str) -> bool { path.ends_with(".rs") }
    // One syn parse feeds both the entity pass and the edge pass.
    fn extract(&self, file: &str, content: &str) -> TypeFacts {
        let Ok(parsed) = syn::parse_file(content) else {
            return TypeFacts::default();
        };
        TypeFacts {
            entities: rust_entities_from(&parsed, file),
            edges: edges_from(&parsed),
        }
    }
}

impl TypeLang for KotlinTypes {
    fn name(&self) -> &'static str { "kotlin" }
    fn matches(&self, path: &str) -> bool { path.ends_with(".kt") || path.ends_with(".kts") }
    // One tree-sitter parse feeds both walks.
    fn extract(&self, file: &str, content: &str) -> TypeFacts {
        let mut parser = tree_sitter::Parser::new();
        let lang = tree_sitter::Language::new(tree_sitter_kotlin_sg::LANGUAGE);
        if parser.set_language(&lang).is_err() {
            return TypeFacts::default();
        }
        let Some(tree) = parser.parse(content, None) else {
            return TypeFacts::default();
        };
        let src = content.as_bytes();
        let root = tree.root_node();
        let mut entities = Vec::new();
        walk_kotlin_entities(root, src, file, &mut entities);
        TypeFacts { entities, edges: kotlin_edges_from(root, src) }
    }
}

impl TypeLang for TsTypes {
    fn name(&self) -> &'static str { "ts" }
    fn matches(&self, path: &str) -> bool { path.ends_with(".ts") || path.ends_with(".tsx") }
    // One oxc parse feeds both walks.
    fn extract(&self, file: &str, content: &str) -> TypeFacts {
        let tsx = path_is_tsx(file);
        let alloc = oxc_allocator::Allocator::default();
        let st = if tsx { oxc_span::SourceType::tsx() } else { oxc_span::SourceType::ts() };
        let ret = oxc_parser::Parser::new(&alloc, content, st).parse();
        if ret.panicked {
            return TypeFacts::default();
        }
        TypeFacts {
            entities: ts_entities_from(&ret.program, file, content),
            edges: ts_edges_from(&ret.program),
        }
    }
}

fn path_is_tsx(file: &str) -> bool {
    file.ends_with(".tsx")
}

/// Map a byte offset to a 1-based line number. Built once per file by the oxc
/// entity pass (oxc spans are byte offsets, unlike syn's line/col).
fn line_index(content: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, b) in content.bytes().enumerate() {
        if b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

fn line_at(starts: &[usize], offset: usize) -> u32 {
    match starts.binary_search(&offset) {
        Ok(i) => (i + 1) as u32,
        Err(i) => i as u32, // i = count of starts <= offset
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
    kotlin_edges_from(tree.root_node(), content.as_bytes())
}

fn kotlin_edges_from(root: tree_sitter::Node, src: &[u8]) -> Vec<TypeEdge> {
    let mut out: BTreeSet<(String, String, &'static str)> = BTreeSet::new();
    walk_kotlin(root, src, &mut out);
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


// ── TypeScript / JavaScript (oxc) ───────────────────────────────────────────
//
// Same diet-extractor contract as the syn and tree-sitter passes: parse one
// file, walk declaration shapes, emit edges in the shared kind vocabulary.
// Mapping: an interface's `extends` are "generic" (trait supertraits), a
// class's `extends`/`implements` are "impl", property/parameter-property
// types are "field", enum members are `Owner::Name` "variant" rows, and a
// union type alias's referenced alternatives are "variant" (a sum type).
// Declared type-parameter names are excluded from refs, like Kotlin.
// Method signatures/bodies are skipped everywhere — shape only.
// Top-level + exported declarations only (namespaces wait on demand).

use oxc_ast::ast as ts_ast;
use oxc_ast_visit::Visit as OxcVisit;

pub fn ts_edges(content: &str, tsx: bool) -> Vec<TypeEdge> {
    let alloc = oxc_allocator::Allocator::default();
    let st = if tsx { oxc_span::SourceType::tsx() } else { oxc_span::SourceType::ts() };
    let ret = oxc_parser::Parser::new(&alloc, content, st).parse();
    if ret.panicked {
        return Vec::new();
    }
    ts_edges_from(&ret.program)
}

fn ts_edges_from(program: &ts_ast::Program) -> Vec<TypeEdge> {
    let mut out: BTreeSet<(String, String, &'static str)> = BTreeSet::new();
    for stmt in &program.body {
        use ts_ast::Statement as S;
        match stmt {
            S::ExportNamedDeclaration(e) => {
                if let Some(d) = &e.declaration {
                    ts_decl_edges(d, &mut out);
                }
            }
            S::ExportDefaultDeclaration(e) => match &e.declaration {
                ts_ast::ExportDefaultDeclarationKind::ClassDeclaration(c) => ts_class_edges(c, &mut out),
                ts_ast::ExportDefaultDeclarationKind::TSInterfaceDeclaration(i) => ts_interface_edges(i, &mut out),
                ts_ast::ExportDefaultDeclarationKind::FunctionDeclaration(f) => ts_function_edges(f, &mut out),
                _ => {}
            },
            S::ClassDeclaration(c) => ts_class_edges(c, &mut out),
            S::TSInterfaceDeclaration(i) => ts_interface_edges(i, &mut out),
            S::TSTypeAliasDeclaration(a) => ts_alias_edges(a, &mut out),
            S::TSEnumDeclaration(e) => ts_enum_edges(e, &mut out),
            S::FunctionDeclaration(f) => ts_function_edges(f, &mut out),
            S::VariableDeclaration(v) => ts_var_fn_edges(v, &mut out),
            _ => {}
        }
    }
    out.into_iter()
        .map(|(from, to, kind)| TypeEdge { from, to, kind })
        .collect()
}

/// Collect every `TSTypeReference` name under a type subtree, excluding the
/// declaration's own type-parameter names. Keyword types (string, number, ...)
/// are distinct AST variants, so primitives never show up.
struct TsRefs<'p> {
    params: &'p BTreeSet<String>,
    out: Vec<String>,
}

impl<'a, 'p> OxcVisit<'a> for TsRefs<'p> {
    fn visit_ts_type_reference(&mut self, it: &ts_ast::TSTypeReference<'a>) {
        if let Some(name) = ts_type_name(&it.type_name) {
            if !self.params.contains(&name) {
                self.out.push(name);
            }
        }
        oxc_ast_visit::walk::walk_ts_type_reference(self, it);
    }
}

fn ts_type_name(n: &ts_ast::TSTypeName) -> Option<String> {
    match n {
        ts_ast::TSTypeName::IdentifierReference(id) => Some(id.name.to_string()),
        ts_ast::TSTypeName::QualifiedName(q) => {
            ts_type_name(&q.left).map(|l| format!("{l}.{}", q.right.name))
        }
        ts_ast::TSTypeName::ThisExpression(_) => None,
    }
}

fn ts_refs_in_type(ty: &ts_ast::TSType, params: &BTreeSet<String>) -> Vec<String> {
    let mut c = TsRefs { params, out: Vec::new() };
    c.visit_ts_type(ty);
    c.out.sort();
    c.out.dedup();
    c.out
}

/// Declared type-parameter names + their constraint refs as "generic" edges.
fn ts_param_edges(
    owner: &str,
    tp: &Option<oxc_allocator::Box<ts_ast::TSTypeParameterDeclaration>>,
    out: &mut BTreeSet<(String, String, &'static str)>,
) -> BTreeSet<String> {
    let mut params = BTreeSet::new();
    let Some(tp) = tp else { return params };
    for p in &tp.params {
        params.insert(p.name.name.to_string());
    }
    for p in &tp.params {
        if let Some(c) = &p.constraint {
            for to in ts_refs_in_type(c, &params) {
                push(out, owner, &to, "generic");
            }
        }
    }
    params
}

fn ts_decl_edges(decl: &ts_ast::Declaration, out: &mut BTreeSet<(String, String, &'static str)>) {
    match decl {
        ts_ast::Declaration::ClassDeclaration(c) => ts_class_edges(c, out),
        ts_ast::Declaration::TSInterfaceDeclaration(i) => ts_interface_edges(i, out),
        ts_ast::Declaration::TSTypeAliasDeclaration(a) => ts_alias_edges(a, out),
        ts_ast::Declaration::TSEnumDeclaration(e) => ts_enum_edges(e, out),
        ts_ast::Declaration::FunctionDeclaration(f) => ts_function_edges(f, out),
        ts_ast::Declaration::VariableDeclaration(v) => ts_var_fn_edges(v, out),
        _ => {}
    }
}

/// A named `function foo(...)`. Anonymous functions have no owner, so skip.
fn ts_function_edges(f: &ts_ast::Function, out: &mut BTreeSet<(String, String, &'static str)>) {
    let Some(id) = &f.id else { return };
    ts_fn_signature_edges(
        &id.name,
        &f.type_parameters,
        &f.params,
        &f.return_type,
        f.body.as_deref(),
        out,
    );
}

/// `const foo = (...) => ...` / `const foo = function (...) {...}` at the top
/// level: the binding name owns the function's edges. Plain value consts (no
/// function initializer) carry no type shape and are skipped.
fn ts_var_fn_edges(v: &ts_ast::VariableDeclaration, out: &mut BTreeSet<(String, String, &'static str)>) {
    for d in &v.declarations {
        let ts_ast::BindingPattern::BindingIdentifier(name) = &d.id else { continue };
        match &d.init {
            Some(ts_ast::Expression::ArrowFunctionExpression(a)) => ts_fn_signature_edges(
                &name.name,
                &a.type_parameters,
                &a.params,
                &a.return_type,
                Some(&a.body),
                out,
            ),
            Some(ts_ast::Expression::FunctionExpression(f)) => ts_fn_signature_edges(
                &name.name,
                &f.type_parameters,
                &f.params,
                &f.return_type,
                f.body.as_deref(),
                out,
            ),
            _ => {}
        }
    }
}

/// The shared body of every function form: type-parameter bounds are "generic"
/// (and excluded from refs), parameter types are "param", the return type is
/// "returns", and every TSTypeReference inside the body is "uses".
fn ts_fn_signature_edges(
    owner: &str,
    type_parameters: &Option<oxc_allocator::Box<ts_ast::TSTypeParameterDeclaration>>,
    params: &ts_ast::FormalParameters,
    return_type: &Option<oxc_allocator::Box<ts_ast::TSTypeAnnotation>>,
    body: Option<&ts_ast::FunctionBody>,
    out: &mut BTreeSet<(String, String, &'static str)>,
) {
    let tp = ts_param_edges(owner, type_parameters, out);
    for p in &params.items {
        if let Some(ann) = &p.type_annotation {
            for to in ts_refs_in_type(&ann.type_annotation, &tp) {
                push(out, owner, &to, "param");
            }
        }
    }
    if let Some(rt) = return_type {
        for to in ts_refs_in_type(&rt.type_annotation, &tp) {
            push(out, owner, &to, "returns");
        }
    }
    if let Some(b) = body {
        let mut v = TsRefs { params: &tp, out: Vec::new() };
        v.visit_function_body(b);
        v.out.sort();
        v.out.dedup();
        for to in v.out {
            push(out, owner, &to, "uses");
        }
    }
}

fn ts_class_edges(class: &ts_ast::Class, out: &mut BTreeSet<(String, String, &'static str)>) {
    let Some(id) = &class.id else { return };
    let owner = id.name.to_string();
    let params = ts_param_edges(&owner, &class.type_parameters, out);

    if let Some(sup) = &class.super_class {
        if let ts_ast::Expression::Identifier(idr) = sup {
            push(out, &owner, idr.name.as_str(), "impl");
        }
    }
    if let Some(args) = &class.super_type_arguments {
        for ty in &args.params {
            for to in ts_refs_in_type(ty, &params) {
                push(out, &owner, &to, "impl");
            }
        }
    }
    for imp in &class.implements {
        if let Some(to) = ts_type_name(&imp.expression) {
            push(out, &owner, &to, "impl");
        }
        if let Some(args) = &imp.type_arguments {
            for ty in &args.params {
                for to in ts_refs_in_type(ty, &params) {
                    push(out, &owner, &to, "impl");
                }
            }
        }
    }
    for el in &class.body.body {
        match el {
            ts_ast::ClassElement::PropertyDefinition(p) => {
                if let Some(ann) = &p.type_annotation {
                    for to in ts_refs_in_type(&ann.type_annotation, &params) {
                        push(out, &owner, &to, "field");
                    }
                }
            }
            ts_ast::ClassElement::AccessorProperty(p) => {
                if let Some(ann) = &p.type_annotation {
                    for to in ts_refs_in_type(&ann.type_annotation, &params) {
                        push(out, &owner, &to, "field");
                    }
                }
            }
            // constructor parameter properties (`constructor(private db: Db)`)
            // declare fields; plain constructor args are not part of the shape
            ts_ast::ClassElement::MethodDefinition(m) => {
                if m.kind != ts_ast::MethodDefinitionKind::Constructor {
                    continue;
                }
                for fp in &m.value.params.items {
                    if fp.accessibility.is_none() && !fp.readonly {
                        continue;
                    }
                    if let Some(ann) = &fp.type_annotation {
                        for to in ts_refs_in_type(&ann.type_annotation, &params) {
                            push(out, &owner, &to, "field");
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn ts_interface_edges(
    i: &ts_ast::TSInterfaceDeclaration,
    out: &mut BTreeSet<(String, String, &'static str)>,
) {
    let owner = i.id.name.to_string();
    let params = ts_param_edges(&owner, &i.type_parameters, out);
    for ext in &i.extends {
        if let ts_ast::Expression::Identifier(idr) = &ext.expression {
            push(out, &owner, idr.name.as_str(), "generic");
        }
        if let Some(args) = &ext.type_arguments {
            for ty in &args.params {
                for to in ts_refs_in_type(ty, &params) {
                    push(out, &owner, &to, "generic");
                }
            }
        }
    }
    for member in &i.body.body {
        if let ts_ast::TSSignature::TSPropertySignature(p) = member {
            if let Some(ann) = &p.type_annotation {
                for to in ts_refs_in_type(&ann.type_annotation, &params) {
                    push(out, &owner, &to, "field");
                }
            }
        }
    }
}

fn ts_alias_edges(
    a: &ts_ast::TSTypeAliasDeclaration,
    out: &mut BTreeSet<(String, String, &'static str)>,
) {
    let owner = a.id.name.to_string();
    let params = ts_param_edges(&owner, &a.type_parameters, out);
    // a union alias is a sum type: alternatives that are plain refs are
    // "variant" edges (their type args stay "field"); anything else is shape
    if let ts_ast::TSType::TSUnionType(u) = &a.type_annotation {
        for member in &u.types {
            if let ts_ast::TSType::TSTypeReference(r) = member {
                if let Some(to) = ts_type_name(&r.type_name) {
                    if !params.contains(&to) {
                        push(out, &owner, &to, "variant");
                    }
                }
                if let Some(args) = &r.type_arguments {
                    for ty in &args.params {
                        for to in ts_refs_in_type(ty, &params) {
                            push(out, &owner, &to, "field");
                        }
                    }
                }
            } else {
                for to in ts_refs_in_type(member, &params) {
                    push(out, &owner, &to, "field");
                }
            }
        }
        return;
    }
    for to in ts_refs_in_type(&a.type_annotation, &params) {
        push(out, &owner, &to, "field");
    }
}

fn ts_enum_edges(e: &ts_ast::TSEnumDeclaration, out: &mut BTreeSet<(String, String, &'static str)>) {
    let owner = e.id.name.to_string();
    for m in &e.body.members {
        let name = match &m.id {
            ts_ast::TSEnumMemberName::Identifier(id) => id.name.to_string(),
            ts_ast::TSEnumMemberName::String(s) => s.value.to_string(),
            _ => continue,
        };
        push(out, &owner, &format!("{owner}::{name}"), "variant");
    }
}

// --- entity pass: declared symbols with kind, location, and (for callables)
// the arrow type. Parses a second time (independent of the edge pass) so the
// tested edge extraction stays untouched; one file, two cheap syntax walks. ---

#[cfg(test)]
fn ts_entities(file: &str, content: &str, tsx: bool) -> Vec<TypeEntity> {
    let alloc = oxc_allocator::Allocator::default();
    let st = if tsx { oxc_span::SourceType::tsx() } else { oxc_span::SourceType::ts() };
    let ret = oxc_parser::Parser::new(&alloc, content, st).parse();
    if ret.panicked {
        return Vec::new();
    }
    ts_entities_from(&ret.program, file, content)
}

fn ts_entities_from(program: &ts_ast::Program, file: &str, content: &str) -> Vec<TypeEntity> {
    let starts = line_index(content);
    let mut out = Vec::new();
    for stmt in &program.body {
        use ts_ast::Statement as S;
        match stmt {
            S::ExportNamedDeclaration(e) => {
                if let Some(d) = &e.declaration {
                    ts_decl_entity(d, file, &starts, &mut out);
                }
            }
            S::ExportDefaultDeclaration(e) => match &e.declaration {
                ts_ast::ExportDefaultDeclarationKind::ClassDeclaration(c) => ts_class_entity(c, file, &starts, &mut out),
                ts_ast::ExportDefaultDeclarationKind::TSInterfaceDeclaration(i) => {
                    push_entity(&mut out, file, &starts, &i.id.name, i.span.start, EntityKind::Interface, None, None)
                }
                ts_ast::ExportDefaultDeclarationKind::FunctionDeclaration(f) => ts_fn_entity(f, file, &starts, &mut out),
                _ => {}
            },
            S::ClassDeclaration(c) => ts_class_entity(c, file, &starts, &mut out),
            S::TSInterfaceDeclaration(i) => {
                push_entity(&mut out, file, &starts, &i.id.name, i.span.start, EntityKind::Interface, None, None)
            }
            S::TSTypeAliasDeclaration(a) => {
                push_entity(&mut out, file, &starts, &a.id.name, a.span.start, EntityKind::Alias, None, None)
            }
            S::TSEnumDeclaration(e) => {
                push_entity(&mut out, file, &starts, &e.id.name, e.span.start, EntityKind::Enum, None, None)
            }
            S::FunctionDeclaration(f) => ts_fn_entity(f, file, &starts, &mut out),
            S::VariableDeclaration(v) => ts_var_fn_entity(v, file, &starts, &mut out),
            _ => {}
        }
    }
    out
}

fn ts_decl_entity(d: &ts_ast::Declaration, file: &str, starts: &[usize], out: &mut Vec<TypeEntity>) {
    match d {
        ts_ast::Declaration::ClassDeclaration(c) => ts_class_entity(c, file, starts, out),
        ts_ast::Declaration::TSInterfaceDeclaration(i) => {
            push_entity(out, file, starts, &i.id.name, i.span.start, EntityKind::Interface, None, None)
        }
        ts_ast::Declaration::TSTypeAliasDeclaration(a) => {
            push_entity(out, file, starts, &a.id.name, a.span.start, EntityKind::Alias, None, None)
        }
        ts_ast::Declaration::TSEnumDeclaration(e) => {
            push_entity(out, file, starts, &e.id.name, e.span.start, EntityKind::Enum, None, None)
        }
        ts_ast::Declaration::FunctionDeclaration(f) => ts_fn_entity(f, file, starts, out),
        ts_ast::Declaration::VariableDeclaration(v) => ts_var_fn_entity(v, file, starts, out),
        _ => {}
    }
}

fn ts_class_entity(c: &ts_ast::Class, file: &str, starts: &[usize], out: &mut Vec<TypeEntity>) {
    let Some(id) = &c.id else { return };
    let owner = id.name.to_string();
    push_entity(out, file, starts, &id.name, c.span.start, EntityKind::Class, None, None);
    for el in &c.body.body {
        if let ts_ast::ClassElement::MethodDefinition(m) = el {
            // normal method name `foo()`; skip computed/private/constructor keys
            if m.kind == ts_ast::MethodDefinitionKind::Constructor {
                continue;
            }
            if let ts_ast::PropertyKey::StaticIdentifier(k) = &m.key {
                let ty = ts_fn_type(&m.value.type_parameters, &m.value.params, &m.value.return_type);
                push_entity(out, file, starts, &k.name, m.span.start, EntityKind::Method, Some(&owner), Some(ty));
            }
        }
    }
}

fn ts_fn_entity(f: &ts_ast::Function, file: &str, starts: &[usize], out: &mut Vec<TypeEntity>) {
    let Some(id) = &f.id else { return };
    let ty = ts_fn_type(&f.type_parameters, &f.params, &f.return_type);
    push_entity(out, file, starts, &id.name, f.span.start, EntityKind::Function, None, Some(ty));
}

fn ts_var_fn_entity(v: &ts_ast::VariableDeclaration, file: &str, starts: &[usize], out: &mut Vec<TypeEntity>) {
    for d in &v.declarations {
        let ts_ast::BindingPattern::BindingIdentifier(name) = &d.id else { continue };
        let ty = match &d.init {
            Some(ts_ast::Expression::ArrowFunctionExpression(a)) => {
                ts_fn_type(&a.type_parameters, &a.params, &a.return_type)
            }
            Some(ts_ast::Expression::FunctionExpression(f)) => {
                ts_fn_type(&f.type_parameters, &f.params, &f.return_type)
            }
            _ => continue,
        };
        push_entity(out, file, starts, &name.name, d.span.start, EntityKind::Function, None, Some(ty));
    }
}

/// Build the arrow `[...A] => B` for a function form. Each param slot collects
/// its referenced type names (declared type-param names excluded); the return
/// slot likewise. Keyword/primitive slots come back empty.
fn ts_fn_type(
    type_parameters: &Option<oxc_allocator::Box<ts_ast::TSTypeParameterDeclaration>>,
    params: &ts_ast::FormalParameters,
    return_type: &Option<oxc_allocator::Box<ts_ast::TSTypeAnnotation>>,
) -> TypeExpr {
    let mut tp = BTreeSet::new();
    if let Some(tps) = type_parameters {
        for p in &tps.params {
            tp.insert(p.name.name.to_string());
        }
    }
    let named = |refs: Vec<String>| refs.into_iter().map(TypeRef::Named).collect::<Vec<_>>();
    let params = params
        .items
        .iter()
        .map(|p| match &p.type_annotation {
            Some(ann) => named(ts_refs_in_type(&ann.type_annotation, &tp)),
            None => Vec::new(),
        })
        .collect();
    let ret = match return_type {
        Some(rt) => named(ts_refs_in_type(&rt.type_annotation, &tp)),
        None => Vec::new(),
    };
    TypeExpr { params, ret }
}

fn push_entity(
    out: &mut Vec<TypeEntity>,
    file: &str,
    starts: &[usize],
    name: &str,
    span_start: u32,
    kind: EntityKind,
    parent: Option<&str>,
    ty: Option<TypeExpr>,
) {
    out.push(TypeEntity {
        sym: mint_sym(file, kind, name, parent),
        name: name.to_string(),
        kind,
        parent: parent.map(|p| mint_sym(file, EntityKind::Class, p, None)),
        file: file.to_string(),
        line: line_at(starts, span_start as usize),
        ty,
    });
}

// --- Rust entity pass (syn): structs/enums/unions/traits as data types, free
// functions and impl methods as callables with arrow types. Lines come from
// proc-macro2 span-locations (the `Spanned` ident span). ---

#[cfg(test)]
fn rust_entities(file: &str, content: &str) -> Vec<TypeEntity> {
    let Ok(parsed) = syn::parse_file(content) else {
        return Vec::new();
    };
    rust_entities_from(&parsed, file)
}

fn rust_entities_from(parsed: &syn::File, file: &str) -> Vec<TypeEntity> {
    let mut out = Vec::new();
    for item in &parsed.items {
        rust_item_entity(item, file, &mut out);
    }
    out
}

fn rust_line(span: proc_macro2::Span) -> u32 {
    span.start().line as u32
}

fn rust_item_entity(item: &Item, file: &str, out: &mut Vec<TypeEntity>) {
    // `parent` is the bare owner name (e.g. "Engine"); the method sym uses it
    // as `Class.name` while the stored parent field is the minted class sym.
    let mut e = |name: String, line: u32, kind: EntityKind, parent: Option<String>, ty: Option<TypeExpr>| {
        out.push(TypeEntity {
            sym: mint_sym(file, kind, &name, parent.as_deref()),
            name,
            kind,
            parent: parent.map(|p| mint_sym(file, EntityKind::Class, &p, None)),
            file: file.to_string(),
            line,
            ty,
        });
    };
    match item {
        Item::Struct(s) => e(s.ident.to_string(), rust_line(s.ident.span()), EntityKind::Struct, None, None),
        Item::Enum(en) => e(en.ident.to_string(), rust_line(en.ident.span()), EntityKind::Enum, None, None),
        Item::Union(u) => e(u.ident.to_string(), rust_line(u.ident.span()), EntityKind::Struct, None, None),
        Item::Trait(t) => e(t.ident.to_string(), rust_line(t.ident.span()), EntityKind::Trait, None, None),
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

// --- Kotlin entity pass (tree-sitter): declared types plus functions, the
// latter carrying their arrow `[...A] => B` like Rust/TS. Line is the node's
// row. ---

fn walk_kotlin_entities(node: tree_sitter::Node, src: &[u8], file: &str, out: &mut Vec<TypeEntity>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(child.kind(), "class_declaration" | "object_declaration") {
            let mut c = child.walk();
            let kids: Vec<tree_sitter::Node> = child.children(&mut c).collect();
            if let Some(id) = kids.iter().find(|n| n.kind() == "type_identifier") {
                let name = id.utf8_text(src).unwrap_or("").to_string();
                let kind = if kids.iter().any(|n| n.kind() == "interface") {
                    EntityKind::Interface
                } else if kids.iter().any(|n| n.kind() == "enum") {
                    EntityKind::Enum
                } else {
                    EntityKind::Class
                };
                out.push(TypeEntity {
                    sym: mint_sym(file, kind, &name, None),
                    name,
                    kind,
                    parent: None,
                    file: file.to_string(),
                    line: (child.start_position().row + 1) as u32,
                    ty: None,
                });
            }
        } else if child.kind() == "function_declaration" {
            // top-level / member `fun name(...)`; the name is a simple_identifier
            let mut c = child.walk();
            let kids: Vec<tree_sitter::Node> = child.children(&mut c).collect();
            if let Some(id) = kids.iter().find(|n| n.kind() == "simple_identifier") {
                let name = id.utf8_text(src).unwrap_or("").to_string();
                out.push(TypeEntity {
                    sym: mint_sym(file, EntityKind::Function, &name, None),
                    name,
                    kind: EntityKind::Function,
                    parent: None,
                    file: file.to_string(),
                    line: (child.start_position().row + 1) as u32,
                    ty: Some(kotlin_fn_type(child, src)),
                });
            }
        }
        walk_kotlin_entities(child, src, file, out);
    }
}

/// Build the arrow `[...A] => B` for a `fun`: each `parameter` under
/// `function_value_parameters` becomes a slot of its referenced type names
/// (declared type-param names and Kotlin builtins excluded), and the return
/// type node after the parameter list fills `ret`. A function with no declared
/// return type leaves `ret` empty (Unit), matching the keyword-slot convention.
fn kotlin_fn_type(node: tree_sitter::Node, src: &[u8]) -> TypeExpr {
    let mut cursor = node.walk();
    let children: Vec<tree_sitter::Node> = node.children(&mut cursor).collect();

    // declared type-parameter names: excluded from refs, like the decl pass
    let mut tparams: BTreeSet<String> = BTreeSet::new();
    for n in &children {
        if n.kind() != "type_parameters" { continue; }
        let mut c = n.walk();
        for tp in n.children(&mut c).filter(|n| n.kind() == "type_parameter") {
            let mut cc = tp.walk();
            let kids: Vec<tree_sitter::Node> = tp.children(&mut cc).collect();
            if let Some(name) = kids.iter().find(|n| n.kind() == "type_identifier") {
                tparams.insert(name.utf8_text(src).unwrap_or("").to_string());
            }
        }
    }

    let named = |refs: Vec<String>| refs.into_iter().map(TypeRef::Named).collect::<Vec<_>>();
    let mut params = Vec::new();
    let mut ret = Vec::new();
    for n in &children {
        match n.kind() {
            "function_value_parameters" => {
                let mut c = n.walk();
                for p in n.children(&mut c).filter(|n| n.kind() == "parameter") {
                    // the parameter's name is a simple_identifier (not collected,
                    // collect_kotlin_refs only reads user_type); its type recurses
                    params.push(named(kotlin_type_refs(p, src, &tparams)));
                }
            }
            // the return type is a type-node sibling after the parameter list
            k if is_kotlin_type_node(k) => ret = named(kotlin_type_refs(*n, src, &tparams)),
            _ => {}
        }
    }
    TypeExpr { params, ret }
}

fn is_kotlin_type_node(kind: &str) -> bool {
    matches!(
        kind,
        "user_type" | "nullable_type" | "function_type" | "parenthesized_type"
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
    fn kotlin_function_entities_carry_arrow_types() {
        let src = "\
package com.app
fun resolve(model: Model, n: Int): NodeId { return n }
fun <T : Entity> wrap(item: T, sink: Sink<Report>) {}
";
        let es = KotlinTypes.extract("src/app.kt", src).entities;
        let by = |name: &str| es.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("missing {name}: {es:?}"));
        let resolve = by("resolve");
        assert_eq!(resolve.kind, EntityKind::Function);
        let ty = resolve.ty.as_ref().unwrap();
        assert_eq!(ty.params[0], vec![TypeRef::Named("Model".into())]);
        assert!(ty.params[1].is_empty(), "Int is a builtin, no ref: {ty:?}");
        assert_eq!(ty.ret, vec![TypeRef::Named("NodeId".into())]);
        // declared type-param T excluded; owner + nested generic arg both kept;
        // no return type -> empty ret
        let wrap = by("wrap").ty.as_ref().unwrap();
        assert!(wrap.params[0].is_empty(), "type-param T is not a ref: {wrap:?}");
        assert!(wrap.params[1].contains(&TypeRef::Named("Sink".into())), "owner: {wrap:?}");
        assert!(wrap.params[1].contains(&TypeRef::Named("Report".into())), "nested arg: {wrap:?}");
        assert!(wrap.ret.is_empty(), "no declared return: {wrap:?}");
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

    #[test]
    fn ts_fields_supers_variants_and_generics() {
        let src = r#"
interface Pricing {}
interface Entity { id: Id }
export interface Catalog<T extends Entity> extends Pricing {
    items: Map<Sku, T>
    name: string
}
export class Repo extends Base implements Pricing {
    cache: Cache<Item>
    constructor(private db: Db, wire: Wire) {}
}
export type Event = Created | Deleted<Reason> | "tombstone"
type Pair = [Left, Right]
enum Color { Red, Green = "g" }
"#;
        let got = ts_edges(src, false);
        assert!(has(&got, "Entity", "Id", "field"), "interface property: {got:?}");
        assert!(has(&got, "Catalog", "Pricing", "generic"), "interface extends: {got:?}");
        assert!(has(&got, "Catalog", "Entity", "generic"), "type-param bound: {got:?}");
        assert!(has(&got, "Catalog", "Sku", "field"), "generic arg in property: {got:?}");
        assert!(!got.iter().any(|e| e.to == "T"), "type-param name leaked: {got:?}");
        assert!(has(&got, "Repo", "Base", "impl"), "class extends: {got:?}");
        assert!(has(&got, "Repo", "Pricing", "impl"), "class implements: {got:?}");
        assert!(has(&got, "Repo", "Cache", "field"), "class property: {got:?}");
        assert!(has(&got, "Repo", "Item", "field"), "property generic arg: {got:?}");
        assert!(has(&got, "Repo", "Db", "field"), "ctor parameter property: {got:?}");
        assert!(!got.iter().any(|e| e.to == "Wire"), "plain ctor arg is not a field: {got:?}");
        assert!(has(&got, "Event", "Created", "variant"), "union alternative: {got:?}");
        assert!(has(&got, "Event", "Deleted", "variant"), "generic union alternative: {got:?}");
        assert!(has(&got, "Event", "Reason", "field"), "union alternative arg: {got:?}");
        assert!(has(&got, "Pair", "Left", "field"), "tuple alias member: {got:?}");
        assert!(has(&got, "Color", "Color::Red", "variant"), "enum member: {got:?}");
        assert!(has(&got, "Color", "Color::Green", "variant"), "initialized enum member: {got:?}");
        assert!(!got.iter().any(|e| e.to == "string"), "keyword type leaked: {got:?}");
    }

    #[test]
    fn tsx_parses_and_extracts() {
        let src = r#"
interface CardProps { item: Item; onPick: (s: Sku) => void }
export function Card({ item }: CardProps) { return <div>{item.name}</div> }
"#;
        let got = ts_edges(src, true);
        assert!(has(&got, "CardProps", "Item", "field"), "tsx interface prop: {got:?}");
        assert!(has(&got, "CardProps", "Sku", "field"), "function-type param ref: {got:?}");
    }

    #[test]
    fn ts_entities_kinds_lines_and_arrow_types() {
        let src = "\
export interface Entity { id: Id }
export type Event = A | B
export enum Color { Red }
export class Repo {
    find(q: Query): Entity { return q as Entity }
}
export function resolveIdent(model: Model, n: string): NodeId[] { return [] }
export const cone = (model: Model, mode: ConeMode): View => view()
";
        let es = ts_entities("src/core/model.ts", src, false);
        let by = |name: &str| es.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("missing {name}: {es:?}"));
        // kinds
        assert_eq!(by("Entity").kind, EntityKind::Interface);
        assert_eq!(by("Event").kind, EntityKind::Alias);
        assert_eq!(by("Color").kind, EntityKind::Enum);
        assert_eq!(by("Repo").kind, EntityKind::Class);
        assert_eq!(by("resolveIdent").kind, EntityKind::Function);
        assert_eq!(by("cone").kind, EntityKind::Function);
        // sem-style symbol + declaration line (1-based)
        assert_eq!(by("Entity").sym, "src/core/model.ts::interface::Entity");
        assert_eq!(by("Entity").line, 1);
        assert_eq!(by("resolveIdent").line, 7);
        // method: parented to the class, callable
        let find = by("find");
        assert_eq!(find.kind, EntityKind::Method);
        assert_eq!(find.parent.as_deref(), Some("src/core/model.ts::class::Repo"));
        assert_eq!(find.sym, "src/core/model.ts::method::Repo.find");
        // a function IS a type: [...A] => B
        let f = by("resolveIdent").ty.as_ref().unwrap();
        assert_eq!(f.params[0], vec![TypeRef::Named("Model".into())]);  // first param type
        assert!(f.params[1].is_empty(), "string is a keyword, no ref: {f:?}");
        assert_eq!(f.ret, vec![TypeRef::Named("NodeId".into())]);
        let a = by("cone").ty.as_ref().unwrap();
        assert_eq!(a.params[1], vec![TypeRef::Named("ConeMode".into())]);
        assert_eq!(a.ret, vec![TypeRef::Named("View".into())]);
    }

    #[test]
    fn rust_entities_kinds_and_arrow_types() {
        let src = "\
pub struct Engine { db: Db }
pub enum Mode { A, B }
pub trait Sink {}
pub fn run(e: Engine, n: usize) -> Report { todo!() }
impl Engine {
    pub fn tick(&self, db: Db) -> Result { todo!() }
}
";
        let es = rust_entities("src/engine.rs", src);
        let by = |name: &str| es.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("missing {name}: {es:?}"));
        assert_eq!(by("Engine").kind, EntityKind::Struct);
        assert_eq!(by("Mode").kind, EntityKind::Enum);
        assert_eq!(by("Sink").kind, EntityKind::Trait);
        assert_eq!(by("run").kind, EntityKind::Function);
        assert_eq!(by("Engine").line, 1);
        assert_eq!(by("run").line, 4);
        // free fn arrow type, receiver excluded on the method
        let run = by("run").ty.as_ref().unwrap();
        assert_eq!(run.params[0], vec![TypeRef::Named("Engine".into())]);
        assert!(run.params[1].is_empty(), "usize is primitive: {run:?}");
        assert_eq!(run.ret, vec![TypeRef::Named("Report".into())]);
        let tick = by("tick");
        assert_eq!(tick.kind, EntityKind::Method);
        assert_eq!(tick.parent.as_deref(), Some("src/engine.rs::class::Engine"));
        let tty = tick.ty.as_ref().unwrap();
        assert_eq!(tty.params, vec![vec![TypeRef::Named("Db".into())]], "self dropped: {tty:?}");
    }

    #[test]
    fn ts_function_param_return_and_body_edges() {
        let src = r#"
export function resolveIdent(model: Model, ident: string): NodeId[] {
    const seen: Visited = new Map()
    return model.lookup(ident) as NodeId[]
}
export const cone = <C extends Ctx>(model: Model, mode: ConeMode): View => {
    const acc: Accumulator = init()
    return acc.done()
}
function helper(raw: Raw) {}
"#;
        let got = ts_edges(src, false);
        // function declaration: params in, return out, body refs internal
        assert!(has(&got, "resolveIdent", "Model", "param"), "fn param type: {got:?}");
        assert!(has(&got, "resolveIdent", "NodeId", "returns"), "fn return type: {got:?}");
        assert!(has(&got, "resolveIdent", "Visited", "uses"), "body annotation: {got:?}");
        assert!(has(&got, "resolveIdent", "NodeId", "uses"), "body cast `as NodeId[]`: {got:?}");
        // arrow const: same three kinds, type-param bound is generic + excluded
        assert!(has(&got, "cone", "Model", "param"), "arrow param: {got:?}");
        assert!(has(&got, "cone", "ConeMode", "param"), "arrow param 2: {got:?}");
        assert!(has(&got, "cone", "View", "returns"), "arrow return: {got:?}");
        assert!(has(&got, "cone", "Accumulator", "uses"), "arrow body: {got:?}");
        assert!(has(&got, "cone", "Ctx", "generic"), "type-param bound: {got:?}");
        assert!(!got.iter().any(|e| e.from == "cone" && e.to == "C"), "type-param name leaked: {got:?}");
        // un-exported function still owns edges; keyword param type is no ref
        assert!(has(&got, "helper", "Raw", "param"), "non-exported fn: {got:?}");
        assert!(!got.iter().any(|e| e.to == "string"), "keyword param leaked: {got:?}");
    }
}
