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
use syn::spanned::Spanned;

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
    pub name: String,       // bare callable name, for callee resolution (not written)
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

/// Intra-procedural dataflow facts (the lift-to-node model). Each value-bearing
/// position in a fn body becomes a `DfNode` whose id is `file:line:col` of its
/// span start (unique per program point); local value flow becomes `DfEdge`. The
/// engine stores these as `df_node` / `df_edge`, and a rule
/// `df_reaches(a,b) <- closure(df_edge)` walks the lifted graph transitively on
/// the SAME SCC engine the call/type/module graphs already use. Approximate by
/// design: no SSA, no borrow/alias unification, no interprocedural arg/return
/// stitching yet — a deliberate first slice proving the engine-side work is done.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DataflowFacts {
    pub nodes: Vec<DfNode>,
    pub edges: Vec<DfEdge>,
    pub loops: Vec<LoopFact>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoopFact {
    pub file: String,
    pub start: u32,        // start line of the loop header
    pub end: u32,          // close line of the loop body (span end)
    pub var: String,       // loop variable name, "" when none (while/loop)
    pub collection: String, // textual form of the iterated collection, "" when none
    pub fn_sym: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DfNode {
    pub id: String,
    pub kind: String,   // param | let_bind | var_read | var_write | lit | call_res | borrow | binop | unop | loop | if | match | block | closure | try | expr
    pub var: String,    // variable name when the node is var-related, else ""
    pub fn_sym: String, // enclosing def sym (file::function::name), joins call_def
    pub file: String,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DfEdge {
    pub from: String,
    pub to: String,
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
    /// Intra-procedural dataflow lift (see `DataflowFacts`). Default empty so the
    /// lazy `DATAFLOW_RELS` wiring is live end to end with zero rows; each
    /// front-end overrides as its extractor lands.
    fn extract_dataflow(&self, _file: &str, _content: &str) -> DataflowFacts { DataflowFacts::default() }
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
    fn extract_dataflow(&self, file: &str, content: &str) -> DataflowFacts {
        let mut parser = tree_sitter::Parser::new();
        let lang = tree_sitter::Language::new(tree_sitter_kotlin_sg::LANGUAGE);
        if parser.set_language(&lang).is_err() { return DataflowFacts::default(); }
        let Some(tree) = parser.parse(content, None) else { return DataflowFacts::default(); };
        kotlin_dataflow_from(tree.root_node(), content.as_bytes(), file)
    }
}

// --- Kotlin intra-procedural dataflow lift (tree-sitter). Same two-rule model
// as the Rust syn lift: value-bearing children flow into their parent, and a
// `val/var x = rhs` binds rhs -> x_slot with later reads flowing slot -> read.
// Node id is `file:row:col` from the tree-sitter start position (0-based). A
// `simple_identifier`'s role is decided by its parent: under variable_declaration
// it's a binding target, under parameter it's a param, under call_expression it's
// the callee (skipped), otherwise it's a var_read. Conservative on unsupported
// constructs: may miss flows, never invents.

fn kt_first_child<'a>(node: tree_sitter::Node<'a>, kind: &str) -> Option<tree_sitter::Node<'a>> {
    let mut cur = node.walk();
    let kids: Vec<tree_sitter::Node<'a>> = node.children(&mut cur).collect();
    kids.into_iter().find(|c| c.kind() == kind)
}

fn kotlin_dataflow_from(root: tree_sitter::Node, src: &[u8], file: &str) -> DataflowFacts {
    let mut out = DataflowFacts::default();
    kt_walk_fns(root, src, file, &mut out);
    out
}

fn kt_walk_fns(node: tree_sitter::Node, src: &[u8], file: &str, out: &mut DataflowFacts) {
    let mut cur = node.walk();
    for c in node.children(&mut cur) {
        if c.kind() == "function_declaration" {
            kt_flow_fn(c, src, file, out);
        }
        kt_walk_fns(c, src, file, out);
    }
}

fn kt_flow_fn(fn_node: tree_sitter::Node, src: &[u8], file: &str, out: &mut DataflowFacts) {
    let name = kt_first_child(fn_node, "simple_identifier")
        .map(|n| n.utf8_text(src).unwrap_or("").to_string())
        .unwrap_or_default();
    let fn_sym = mint_sym(file, EntityKind::Function, &name, None);
    let mut scope: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if let Some(params) = kt_first_child(fn_node, "function_value_parameters") {
        let mut cur = params.walk();
        for p in params.children(&mut cur).filter(|n| n.kind() == "parameter") {
            if let Some(idn) = kt_first_child(p, "simple_identifier") {
                let pos = idn.start_position();
                let v = idn.utf8_text(src).unwrap_or("").to_string();
                let id = push_node(out, file, pos.row as u32, pos.column as u32, "param", &v, &fn_sym);
                scope.insert(v, id);
            }
        }
    }
    if let Some(body) = kt_first_child(fn_node, "function_body") {
        flow_kt(body, src, file, &fn_sym, &mut scope, out);
    }
}

/// Returns the node id carrying the value of this subtree, or None when the
/// subtree is not value-bearing (statements, wrappers, bindings handled inline).
fn flow_kt(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) -> Option<String> {
    let pos = node.start_position();
    match node.kind() {
        // a name in expression position is a read; role decided by parent.
        "simple_identifier" => {
            let parent_kind = node.parent().map(|p| p.kind());
            match parent_kind.as_deref() {
                Some("variable_declaration") | Some("parameter") | Some("call_expression") => None,
                _ => {
                    let v = node.utf8_text(src).unwrap_or("").to_string();
                    let id = push_node(out, file, pos.row as u32, pos.column as u32, "var_read", &v, fn_sym);
                    if let Some(b) = scope.get(&v) {
                        out.edges.push(DfEdge { from: b.clone(), to: id.clone() });
                    }
                    Some(id)
                }
            }
        }
        // f(args): every argument value flows into the call result. The callee
        // ident (parent == call_expression) is skipped above. Descend through
        // call_suffix -> value_arguments -> value_argument so multi-arg calls
        // taint the result from every arg, not just the last.
        "call_expression" => {
            let mut child_ids = Vec::new();
            if let Some(suffix) = kt_first_child(node, "call_suffix") {
                if let Some(vargs) = kt_first_child(suffix, "value_arguments") {
                    let mut cur = vargs.walk();
                    for va in vargs.children(&mut cur).filter(|n| n.kind() == "value_argument") {
                        if let Some(id) = flow_kt(va, src, file, fn_sym, scope, out) {
                            child_ids.push(id);
                        }
                    }
                }
            }
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "call_res", "", fn_sym);
            for cid in child_ids {
                out.edges.push(DfEdge { from: cid, to: id.clone() });
            }
            Some(id)
        }
        // val/var x = rhs: mint the binding slot, flow rhs -> slot, register.
        "property_declaration" => {
            let mut bind: Option<(String, String)> = None;
            let mut rhs_id: Option<String> = None;
            let mut cur = node.walk();
            for c in node.children(&mut cur) {
                match c.kind() {
                    "variable_declaration" => {
                        if let Some(si) = kt_first_child(c, "simple_identifier") {
                            let sp = si.start_position();
                            let v = si.utf8_text(src).unwrap_or("").to_string();
                            let id = push_node(out, file, sp.row as u32, sp.column as u32, "let_bind", &v, fn_sym);
                            bind = Some((v, id));
                        }
                    }
                    "=" | "binding_pattern_kind" | "val" | "var" => {}
                    _ => {
                        if let Some(id) = flow_kt(c, src, file, fn_sym, scope, out) {
                            rhs_id = Some(id);
                        }
                    }
                }
            }
            if let (Some((v, bid)), Some(rhs)) = (bind, rhs_id) {
                out.edges.push(DfEdge { from: rhs, to: bid.clone() });
                scope.insert(v, bid);
            }
            None
        }
        // wrappers / statements: flow the last value-bearing child through.
        "value_argument" | "statements" | "function_body" | "source_file" => {
            let mut last = None;
            let mut cur = node.walk();
            for c in node.children(&mut cur) {
                if let Some(id) = flow_kt(c, src, file, fn_sym, scope, out) {
                    last = Some(id);
                }
            }
            last
        }
        // return EXPR: the returned value is the expression's node.
        "jump_expression" => {
            let mut inner = None;
            let mut cur = node.walk();
            for c in node.children(&mut cur) {
                if c.kind() != "return" {
                    if let Some(id) = flow_kt(c, src, file, fn_sym, scope, out) {
                        inner = Some(id);
                    }
                }
            }
            inner
        }
        // a OP b: both operands taint the result. This is the taint-vs-dataflow
        // distinction in one arm — exact dataflow would say `a + 1` is not `a`,
        // taint propagates `a` through the operation into the result. Kotlin
        // splits operators across additive/multiplicative/infix expression kinds
        // (no named fields), so take the first and last named children as the
        // two operands and skip the anonymous operator token between them.
        "additive_expression" | "multiplicative_expression" | "infix_expression" => {
            let mut cur = node.walk();
            let kids: Vec<tree_sitter::Node> = node.named_children(&mut cur).collect();
            let l = kids.first().and_then(|n| flow_kt(*n, src, file, fn_sym, scope, out));
            let r = kids.last().and_then(|n| flow_kt(*n, src, file, fn_sym, scope, out));
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "binop", "", fn_sym);
            if let Some(lid) = l { out.edges.push(DfEdge { from: lid, to: id.clone() }); }
            if let Some(rid) = r { out.edges.push(DfEdge { from: rid, to: id.clone() }); }
            Some(id)
        }
        "string_literal" | "integer_literal" | "real_literal" | "boolean_literal" | "character_literal" | "long_literal" => {
            Some(push_node(out, file, pos.row as u32, pos.column as u32, "lit", "", fn_sym))
        }
        // `for (x in coll) body`: record the span + loop var so loop_over can flag
        // loop-invariant calls inside the body. The body is then walked by the
        // conservative recursion below (Kotlin has no named fields on for_statement).
        "for_statement" => {
            let lvar = {
                let mut cur = node.walk();
                let kids: Vec<tree_sitter::Node> = node.named_children(&mut cur).collect();
                kids.iter().find(|c| c.kind() == "simple_identifier")
                    .map(|n| n.utf8_text(src).unwrap_or("").to_string())
                    .unwrap_or_default()
            };
            let end = node.end_position();
            out.loops.push(LoopFact {
                file: file.into(), start: pos.row as u32, end: end.row as u32,
                var: lvar, collection: String::new(), fn_sym: fn_sym.into(),
            });
            kt_recurse_children(node, src, file, fn_sym, scope, out)
        }
        "while_statement" | "do_while_statement" => {
            let end = node.end_position();
            out.loops.push(LoopFact {
                file: file.into(), start: pos.row as u32, end: end.row as u32,
                var: String::new(), collection: String::new(), fn_sym: fn_sym.into(),
            });
            kt_recurse_children(node, src, file, fn_sym, scope, out)
        }
        // anything else (when-arms, lambda bodies, etc.): recurse conservatively,
        // surface the last value if any. May miss, never invents.
        _ => kt_recurse_children(node, src, file, fn_sym, scope, out),
    }
}

/// Walk all children of a node conservatively, surfacing the last value-bearing
/// child's id. Factored out of the flow_kt default arm so loop arms reuse it.
fn kt_recurse_children(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) -> Option<String> {
    let mut last = None;
    let mut cur = node.walk();
    for c in node.children(&mut cur) {
        if let Some(id) = flow_kt(c, src, file, fn_sym, scope, out) {
            last = Some(id);
        }
    }
    last
}

// --- TypeScript/JavaScript intra-procedural dataflow lift (oxc). Same two-rule
// model: value-bearing children flow into their parent, and `const/let/var x =
// rhs` binds rhs -> x_slot with later reads flowing slot -> read. Node id is
// `file:<byte_off>` (oxc's native byte-offset span); `line_at` recovers the
// 1-based line for the `line` column. Conservative on unsupported constructs.

fn ts_push(out: &mut DataflowFacts, file: &str, starts: &[usize], byte_off: u32, kind: &str, var: &str, fn_sym: &str) -> String {
    // kind suffix disambiguates a parent from its first child where spans share
    // a start byte (see push_node); byte_off alone is not unique for `a + 1`.
    let id = format!("{file}:{byte_off}:{kind}");
    let line = line_at(starts, byte_off as usize);
    out.nodes.push(DfNode {
        id: id.clone(),
        kind: kind.into(),
        var: var.into(),
        fn_sym: fn_sym.into(),
        file: file.into(),
        line,
    });
    id
}

/// Extract the binding identifier name from a pattern (handles the common
/// `const x = ...` single-ident case; destructuring falls through to None).
fn ts_binding_name(p: &ts_ast::BindingPattern) -> Option<String> {
    match p {
        ts_ast::BindingPattern::BindingIdentifier(b) => Some(b.name.to_string()),
        _ => None,
    }
}

fn ts_dataflow_from(program: &ts_ast::Program, file: &str, content: &str) -> DataflowFacts {
    let starts = line_index(content);
    let mut out = DataflowFacts::default();
    for stmt in &program.body {
        ts_flow_stmt(stmt, file, &starts, &mut out);
    }
    out
}

fn ts_flow_stmt(stmt: &ts_ast::Statement, file: &str, starts: &[usize], out: &mut DataflowFacts) {
    use ts_ast::Statement as S;
    match stmt {
        S::FunctionDeclaration(f) => {
            if let Some(body) = f.body.as_deref() {
                let name = f.id.as_ref().map(|i| i.name.to_string()).unwrap_or_default();
                let fn_sym = mint_sym(file, EntityKind::Function, &name, None);
                let mut scope: std::collections::HashMap<String, String> = std::collections::HashMap::new();
                for p in &f.params.items {
                    if let Some(name) = ts_binding_name(&p.pattern) {
                        let off = p.span.start;
                        let id = ts_push(out, file, starts, off, "param", &name, &fn_sym);
                        scope.insert(name, id);
                    }
                }
                ts_flow_body(body, file, starts, &fn_sym, &mut scope, out);
            }
        }
        S::ExportNamedDeclaration(e) => {
            if let Some(d) = &e.declaration {
                ts_flow_decl(d, file, starts, out);
            }
        }
        S::VariableDeclaration(_) | S::ExpressionStatement(_) | S::ReturnStatement(_) => {
            let mut scope: std::collections::HashMap<String, String> = std::collections::HashMap::new();
            let fn_sym = mint_sym(file, EntityKind::Function, "<top>", None);
            ts_flow_body_stmt(stmt, file, starts, &fn_sym, &mut scope, out);
        }
        _ => {}
    }
}

fn ts_flow_decl(d: &ts_ast::Declaration, file: &str, starts: &[usize], out: &mut DataflowFacts) {
    use ts_ast::Declaration as D;
    match d {
        D::FunctionDeclaration(f) => {
            if let Some(body) = f.body.as_deref() {
                let name = f.id.as_ref().map(|i| i.name.to_string()).unwrap_or_default();
                let fn_sym = mint_sym(file, EntityKind::Function, &name, None);
                let mut scope: std::collections::HashMap<String, String> = std::collections::HashMap::new();
                for p in &f.params.items {
                    if let Some(name) = ts_binding_name(&p.pattern) {
                        let off = p.span.start;
                        let id = ts_push(out, file, starts, off, "param", &name, &fn_sym);
                        scope.insert(name, id);
                    }
                }
                ts_flow_body(body, file, starts, &fn_sym, &mut scope, out);
            }
        }
        _ => {}
    }
}

fn ts_flow_body(
    body: &ts_ast::FunctionBody,
    file: &str,
    starts: &[usize],
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) {
    for stmt in &body.statements {
        ts_flow_body_stmt(stmt, file, starts, fn_sym, scope, out);
    }
}

fn ts_flow_body_stmt(
    stmt: &ts_ast::Statement,
    file: &str,
    starts: &[usize],
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) {
    use ts_ast::Statement as S;
    match stmt {
        S::VariableDeclaration(v) => {
            for d in &v.declarations {
                let rhs_id = d.init.as_ref().map(|init| ts_flow_expr(init, file, starts, fn_sym, scope, out));
                if let Some(name) = ts_binding_name(&d.id) {
                    let off = d.span.start;
                    let bind = ts_push(out, file, starts, off, "let_bind", &name, fn_sym);
                    if let Some(rhs) = rhs_id {
                        out.edges.push(DfEdge { from: rhs, to: bind.clone() });
                    }
                    scope.insert(name, bind);
                }
            }
        }
        S::ExpressionStatement(e) => {
            let _ = ts_flow_expr(&e.expression, file, starts, fn_sym, scope, out);
        }
        S::ReturnStatement(r) => {
            if let Some(arg) = &r.argument {
                let _ = ts_flow_expr(arg, file, starts, fn_sym, scope, out);
            }
        }
        // `{ stmts }`: walk each inner statement so flow continues through blocks.
        S::BlockStatement(b) => {
            for s in &b.body {
                ts_flow_body_stmt(s, file, starts, fn_sym, scope, out);
            }
        }
        // `if (test) consequent else alternate`: taint is the union of branches.
        S::IfStatement(i) => {
            let _ = ts_flow_expr(&i.test, file, starts, fn_sym, scope, out);
            ts_flow_body_stmt(&i.consequent, file, starts, fn_sym, scope, out);
            if let Some(alt) = &i.alternate {
                ts_flow_body_stmt(alt, file, starts, fn_sym, scope, out);
            }
        }
        // C-style `for (init; test; update) body`: record the span, flow each.
        S::ForStatement(f) => {
            if let Some(ts_ast::ForStatementInit::VariableDeclaration(v)) = &f.init {
                for d in &v.declarations {
                    let rhs_id = d.init.as_ref().map(|init| ts_flow_expr(init, file, starts, fn_sym, scope, out));
                    if let Some(name) = ts_binding_name(&d.id) {
                        let bind = ts_push(out, file, starts, d.span.start, "let_bind", &name, fn_sym);
                        if let Some(rhs) = rhs_id { out.edges.push(DfEdge { from: rhs, to: bind.clone() }); }
                        scope.insert(name, bind);
                    }
                }
            }
            if let Some(test) = &f.test { let _ = ts_flow_expr(test, file, starts, fn_sym, scope, out); }
            if let Some(upd) = &f.update { let _ = ts_flow_expr(upd, file, starts, fn_sym, scope, out); }
            ts_loop_fact(out, file, starts, f.span.start, f.span.end, "", fn_sym);
            ts_flow_body_stmt(&f.body, file, starts, fn_sym, scope, out);
        }
        // `for (x of/in coll) body`: bind x, flow coll, record span, walk body.
        S::ForOfStatement(f) => ts_for_in_of(&f.left, &f.right, &f.body, f.span.start, f.span.end, file, starts, fn_sym, scope, out),
        S::ForInStatement(f) => ts_for_in_of(&f.left, &f.right, &f.body, f.span.start, f.span.end, file, starts, fn_sym, scope, out),
        S::WhileStatement(w) => {
            let _ = ts_flow_expr(&w.test, file, starts, fn_sym, scope, out);
            ts_loop_fact(out, file, starts, w.span.start, w.span.end, "", fn_sym);
            ts_flow_body_stmt(&w.body, file, starts, fn_sym, scope, out);
        }
        S::DoWhileStatement(d) => {
            let _ = ts_flow_expr(&d.test, file, starts, fn_sym, scope, out);
            ts_loop_fact(out, file, starts, d.span.start, d.span.end, "", fn_sym);
            ts_flow_body_stmt(&d.body, file, starts, fn_sym, scope, out);
        }
        _ => {}
    }
}

/// Record a loop fact from byte-offset span endpoints. `var` is the loop
/// variable name when known (for-of/for-in), else "".
fn ts_loop_fact(out: &mut DataflowFacts, file: &str, starts: &[usize], start_off: u32, end_off: u32, var: &str, fn_sym: &str) {
    out.loops.push(LoopFact {
        file: file.into(),
        start: line_at(starts, start_off as usize),
        end: line_at(starts, end_off as usize),
        var: var.into(),
        collection: String::new(),
        fn_sym: fn_sym.into(),
    });
}

/// Shared handling for `for (x of/in coll) body`: bind the loop variable, flow
/// the collection, record the span, then walk the body.
fn ts_for_in_of(
    left: &ts_ast::ForStatementLeft,
    right: &ts_ast::Expression,
    body: &ts_ast::Statement,
    start_off: u32,
    end_off: u32,
    file: &str,
    starts: &[usize],
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) {
    let coll = ts_flow_expr(right, file, starts, fn_sym, scope, out);
    let var = match left {
        ts_ast::ForStatementLeft::VariableDeclaration(v) => {
            v.declarations.first().and_then(|d| {
                let name = ts_binding_name(&d.id)?;
                let bind = ts_push(out, file, starts, d.span.start, "let_bind", &name, fn_sym);
                out.edges.push(DfEdge { from: coll.clone(), to: bind.clone() });
                scope.insert(name.clone(), bind);
                Some(name)
            }).unwrap_or_default()
        }
        _ => String::new(),
    };
    ts_loop_fact(out, file, starts, start_off, end_off, &var, fn_sym);
    ts_flow_body_stmt(body, file, starts, fn_sym, scope, out);
}

/// Post-order value flow for one TS expression. Returns the node id carrying
/// its value, or a generic node when the variant isn't chased (conservative).
fn ts_flow_expr(
    e: &ts_ast::Expression,
    file: &str,
    starts: &[usize],
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) -> String {
    use ts_ast::Expression as E;
    let off = span_off(e);
    match e {
        // a read of a variable: flow from its binding slot.
        E::Identifier(id) => {
            let name = id.name.to_string();
            let node = ts_push(out, file, starts, off, "var_read", &name, fn_sym);
            if let Some(b) = scope.get(&name) {
                out.edges.push(DfEdge { from: b.clone(), to: node.clone() });
            }
            node
        }
        E::StringLiteral(_)
        | E::NumericLiteral(_)
        | E::BooleanLiteral(_)
        | E::NullLiteral(_)
        | E::BigIntLiteral(_)
        | E::RegExpLiteral(_) => ts_push(out, file, starts, off, "lit", "", fn_sym),
        // f(args): each argument flows into the call result. The callee is the
        // target, not a value in, so it is skipped.
        E::CallExpression(c) => {
            let mut child_ids = Vec::new();
            for arg in &c.arguments {
                if let Some(id) = arg.as_expression() {
                    child_ids.push(ts_flow_expr(id, file, starts, fn_sym, scope, out));
                }
            }
            let id = ts_push(out, file, starts, off, "call_res", "", fn_sym);
            for cid in child_ids {
                out.edges.push(DfEdge { from: cid, to: id.clone() });
            }
            id
        }
        // recv.prop / recv[prop]: the receiver flows through; the property is
        // a name, not a value in (skipped). oxc flattens MemberExpression into
        // StaticMemberExpression / ComputedMemberExpression on Expression.
        E::StaticMemberExpression(m) => {
            let obj = ts_flow_expr(&m.object, file, starts, fn_sym, scope, out);
            let id = ts_push(out, file, starts, off, "member", "", fn_sym);
            out.edges.push(DfEdge { from: obj, to: id.clone() });
            id
        }
        E::ComputedMemberExpression(m) => {
            let obj = ts_flow_expr(&m.object, file, starts, fn_sym, scope, out);
            let id = ts_push(out, file, starts, off, "member", "", fn_sym);
            out.edges.push(DfEdge { from: obj, to: id.clone() });
            id
        }
        E::BinaryExpression(b) => {
            let l = ts_flow_expr(&b.left, file, starts, fn_sym, scope, out);
            let r = ts_flow_expr(&b.right, file, starts, fn_sym, scope, out);
            let id = ts_push(out, file, starts, off, "binop", "", fn_sym);
            out.edges.push(DfEdge { from: l, to: id.clone() });
            out.edges.push(DfEdge { from: r, to: id.clone() });
            id
        }
        // arrow/function values, template strings, control flow: mint a node,
        // don't chase. Conservative — may miss, never invents.
        _ => ts_push(out, file, starts, off, "expr", "", fn_sym),
    }
}

/// Byte offset of an expression's span start. oxc nodes expose their span via
/// the matched inner struct; the Expression enum carries a `.span()` through
/// the GetSpan impl, which we reach via this thin shim.
fn span_off(e: &ts_ast::Expression) -> u32 {
    use oxc_span::GetSpan;
    e.span().start
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
    // One oxc parse feeds the node + edge lift. Byte-offset spans (oxc's native
    // shape) become node ids `file:<byte_off>`; `line_at` recovers the 1-based
    // line for the `line` column.
    fn extract_dataflow(&self, file: &str, content: &str) -> DataflowFacts {
        let tsx = path_is_tsx(file);
        let alloc = oxc_allocator::Allocator::default();
        let st = if tsx { oxc_span::SourceType::tsx() } else { oxc_span::SourceType::ts() };
        let ret = oxc_parser::Parser::new(&alloc, content, st).parse();
        if ret.panicked {
            return DataflowFacts::default();
        }
        ts_dataflow_from(&ret.program, file, content)
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

// --- Rust call-graph pass (syn): free functions and impl methods become
// CallDefs (sym + body span for callsite containment), and every call
// expression becomes a CallSite whose caller is left blank for the engine's
// span-containment pass to fill. Closures are collected as anonymous defs in a
// follow-up; the visitor still walks into them so calls inside a closure body
// attribute to the enclosing named def. ---

fn rust_call_defs_from(parsed: &syn::File, file: &str) -> Vec<CallDef> {
    let mut out = Vec::new();
    let push = |out: &mut Vec<CallDef>, sym: String, name: String, kind: CallKind, line: u32, end: u32| {
        out.push(CallDef {
            sym, name, kind, file: file.to_string(), line, end,
        });
    };
    for item in &parsed.items {
        match item {
            Item::Fn(f) => {
                let name = f.sig.ident.to_string();
                let line = rust_line(f.sig.ident.span());
                let end = f.block.span().end().line as u32;
                push(&mut out, mint_sym(file, EntityKind::Function, &name, None), name, CallKind::Free, line, end);
            }
            Item::Impl(i) => {
                let owner = primary_type(&i.self_ty);
                for ii in &i.items {
                    if let syn::ImplItem::Fn(m) = ii {
                        let name = m.sig.ident.to_string();
                        let line = rust_line(m.sig.ident.span());
                        let end = m.block.span().end().line as u32;
                        push(&mut out,
                            mint_sym(file, EntityKind::Method, &name, owner.as_deref()),
                            name, CallKind::Method, line, end);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// The trailing identifier of a callee expression's source text: the last run
/// of alnum/underscore. `helper` -> "helper", `Vec::new` -> "new",
/// `self.foo.bar` -> "bar". Used to key the bare-name resolver the same way
/// `type_link` resolves a type reference.
fn rust_call_sites_from(parsed: &syn::File, file: &str) -> Vec<CallSite> {
    let mut v = CallCollector { file, sites: Vec::new() };
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
            // `f(args)` / `Foo(args)`: callee is the path's trailing segment.
            syn::Expr::Call(c) => {
                if let syn::Expr::Path(p) = &*c.func {
                    if let Some(seg) = p.path.segments.last() {
                        self.sites.push(CallSite {
                            caller_sym: None,
                            callee: seg.ident.to_string(),
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
                    file: self.file.to_string(),
                    line: m.method.span().start().line as u32,
                });
                syn::visit::visit_expr(self, e);
            }
            _ => syn::visit::visit_expr(self, e),
        }
    }
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
                            Some(o) => mint_sym(file, EntityKind::Method, &m.sig.ident.to_string(), Some(o)),
                            None => mint_sym(file, EntityKind::Function, &m.sig.ident.to_string(), None),
                        };
                        flow_fn_body(&sym, &m.sig, &m.block, file, &mut out);
                    }
                }
            }
            _ => {}
        }
    }
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
    let mut scope: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for arg in &sig.inputs {
        if let syn::FnArg::Typed(pt) = arg {
            if let syn::Pat::Ident(pi) = &*pt.pat {
                let (l, c) = (pi.ident.span().start().line as u32, pi.ident.span().start().column as u32);
                let id = push_node(out, file, l, c, "param", &pi.ident.to_string(), fn_sym);
                scope.insert(pi.ident.to_string(), id);
            }
        }
    }
    flow_block(block, file, fn_sym, &mut scope, out);
}

fn flow_block(
    b: &syn::Block,
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) {
    for stmt in &b.stmts {
        match stmt {
            syn::Stmt::Local(loc) => {
                if let Some(init) = loc.init.as_ref() {
                    let rhs = flow_expr(&init.expr, file, fn_sym, scope, out);
                    if let syn::Pat::Ident(pi) = &loc.pat {
                        let (l, c) = (pi.ident.span().start().line as u32, pi.ident.span().start().column as u32);
                        let bind = push_node(out, file, l, c, "let_bind", &pi.ident.to_string(), fn_sym);
                        out.edges.push(DfEdge { from: rhs, to: bind.clone() });
                        scope.insert(pi.ident.to_string(), bind);
                    }
                }
            }
            syn::Stmt::Expr(e, _) => {
                let _ = flow_expr(e, file, fn_sym, scope, out);
            }
            syn::Stmt::Item(_) => {}
            syn::Stmt::Macro(_) => {}
        }
    }
}

/// Mint a node and return its id. Free helper (not a closure) so the recursive
/// `flow_expr` calls can borrow `out` without holding a second `&mut` alive. The
/// id is `file:line:col:kind`: a parent expression and its first child share a
/// start position (e.g. `a + 1` starts where `a` starts), so the kind suffix
/// disambiguates them — every lifted node is a distinct (position, kind) pair.
fn push_node(
    out: &mut DataflowFacts,
    file: &str,
    line: u32,
    col: u32,
    kind: &str,
    var: &str,
    fn_sym: &str,
) -> String {
    let id = format!("{file}:{line}:{col}:{kind}");
    out.nodes.push(DfNode {
        id: id.clone(),
        kind: kind.into(),
        var: var.into(),
        fn_sym: fn_sym.into(),
        file: file.into(),
        line,
    });
    id
}

/// Post-order value flow for one expression. Returns the node id for `e` and
/// emits every internal edge as a side effect.
fn flow_expr(
    e: &syn::Expr,
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) -> String {
    let start = e.span().start();
    let (line, col) = (start.line as u32, start.column as u32);
    match e {
        // a read of a variable: flow from its binding slot to this read.
        syn::Expr::Path(p) => {
            let name = p.path.segments.last().map(|s| s.ident.to_string()).unwrap_or_default();
            let id = push_node(out, file, line, col, "var_read", &name, fn_sym);
            if let Some(b) = scope.get(&name) {
                out.edges.push(DfEdge { from: b.clone(), to: id.clone() });
            }
            id
        }
        syn::Expr::Lit(_) => push_node(out, file, line, col, "lit", "", fn_sym),
        // f(args): each argument flows into the call result. The callee path is
        // the target, not a value in, so it gets no flow edge.
        syn::Expr::Call(c) => {
            let mut children = Vec::new();
            for arg in &c.args {
                children.push(flow_expr(arg, file, fn_sym, scope, out));
            }
            let id = push_node(out, file, line, col, "call_res", "", fn_sym);
            for child in children {
                out.edges.push(DfEdge { from: child, to: id.clone() });
            }
            id
        }
        // recv.m(args): receiver + args flow into the result; method name skipped.
        syn::Expr::MethodCall(m) => {
            let mut children = vec![flow_expr(&m.receiver, file, fn_sym, scope, out)];
            for arg in &m.args {
                children.push(flow_expr(arg, file, fn_sym, scope, out));
            }
            let id = push_node(out, file, line, col, "call_res", "", fn_sym);
            for child in children {
                out.edges.push(DfEdge { from: child, to: id.clone() });
            }
            id
        }
        syn::Expr::Paren(p) => flow_expr(&p.expr, file, fn_sym, scope, out),
        syn::Expr::Reference(r) => {
            let inner = flow_expr(&r.expr, file, fn_sym, scope, out);
            let id = push_node(out, file, line, col, "borrow", "", fn_sym);
            out.edges.push(DfEdge { from: inner, to: id.clone() });
            id
        }
        syn::Expr::Binary(b) => {
            let l = flow_expr(&b.left, file, fn_sym, scope, out);
            let r = flow_expr(&b.right, file, fn_sym, scope, out);
            let id = push_node(out, file, line, col, "binop", "", fn_sym);
            out.edges.push(DfEdge { from: l, to: id.clone() });
            out.edges.push(DfEdge { from: r, to: id.clone() });
            id
        }
        syn::Expr::Unary(u) => {
            let inner = flow_expr(&u.expr, file, fn_sym, scope, out);
            let id = push_node(out, file, line, col, "unop", "", fn_sym);
            out.edges.push(DfEdge { from: inner, to: id.clone() });
            id
        }
        // transparent pass-through: the ? operator does not alter value flow.
        syn::Expr::Try(t) => flow_expr(&t.expr, file, fn_sym, scope, out),
        // `for pat in coll { body }`: bind the loop variable from the collection,
        // record the loop span so loop_over can flag loop-invariant calls inside
        // it, then walk the body. Each element taints the loop var conservatively.
        syn::Expr::ForLoop(f) => {
            let coll = flow_expr(&f.expr, file, fn_sym, scope, out);
            let lvar = bind_pat(&f.pat, file, fn_sym, scope, out);
            if let Some(v) = &lvar {
                if let Some(b) = scope.get(v) {
                    out.edges.push(DfEdge { from: coll.clone(), to: b.clone() });
                }
            }
            let end = f.body.span().end().line as u32;
            out.loops.push(LoopFact {
                file: file.into(), start: line, end,
                var: lvar.clone().unwrap_or_default(),
                collection: String::new(),
                fn_sym: fn_sym.into(),
            });
            flow_block(&f.body, file, fn_sym, scope, out);
            push_node(out, file, line, col, "loop", &lvar.unwrap_or_default(), fn_sym)
        }
        // `while cond { body }`: `while let` is ExprWhile with cond = Expr::Let.
        // No collection, but the span is still recorded so calls in the body can
        // be flagged.
        syn::Expr::While(w) => {
            let _ = flow_expr(&w.cond, file, fn_sym, scope, out);
            if let syn::Expr::Let(l) = &*w.cond { let _ = bind_pat(&l.pat, file, fn_sym, scope, out); }
            let end = w.body.span().end().line as u32;
            out.loops.push(LoopFact { file: file.into(), start: line, end, var: String::new(), collection: String::new(), fn_sym: fn_sym.into() });
            flow_block(&w.body, file, fn_sym, scope, out);
            push_node(out, file, line, col, "loop", "", fn_sym)
        }
        syn::Expr::Loop(l) => {
            let end = l.body.span().end().line as u32;
            out.loops.push(LoopFact { file: file.into(), start: line, end, var: String::new(), collection: String::new(), fn_sym: fn_sym.into() });
            flow_block(&l.body, file, fn_sym, scope, out);
            push_node(out, file, line, col, "loop", "", fn_sym)
        }
        // `if cond { then } else { els }`: flow each branch; taint is the union.
        syn::Expr::If(i) => {
            let _ = flow_expr(&i.cond, file, fn_sym, scope, out);
            flow_block(&i.then_branch, file, fn_sym, scope, out);
            if let Some((_, els)) = &i.else_branch {
                let _ = flow_expr(els, file, fn_sym, scope, out);
            }
            push_node(out, file, line, col, "if", "", fn_sym)
        }
        // `match scrut { arms }`: scrut + each arm body; guards too.
        syn::Expr::Match(m) => {
            let _ = flow_expr(&m.expr, file, fn_sym, scope, out);
            for arm in &m.arms {
                if let Some((_, g)) = &arm.guard { let _ = flow_expr(g, file, fn_sym, scope, out); }
                let _ = flow_expr(&arm.body, file, fn_sym, scope, out);
            }
            push_node(out, file, line, col, "match", "", fn_sym)
        }
        // `{ stmts }` as an expression: reuse the block walker.
        syn::Expr::Block(b) => {
            flow_block(&b.block, file, fn_sym, scope, out);
            push_node(out, file, line, col, "block", "", fn_sym)
        }
        // `|params| body`: bind params (in-scope for the body), walk the body.
        // Closures are everywhere in real Rust (`let t = |s| ...`); without this
        // the body was a total hole.
        syn::Expr::Closure(c) => {
            for inp in &c.inputs {
                let _ = bind_pat(inp, file, fn_sym, scope, out);
            }
            match c.body.as_ref() {
                syn::Expr::Block(b) => flow_block(&b.block, file, fn_sym, scope, out),
                other => { let _ = flow_expr(other, file, fn_sym, scope, out); }
            }
            push_node(out, file, line, col, "closure", "", fn_sym)
        }
        // `lhs = rhs`: flow rhs, rebind a write slot so later reads see the new
        // value (taint-correct for reassignment). Compound assignment (`+=`) and
        // macros fall through to the conservative default below.
        syn::Expr::Assign(a) => assign_flow(&a.left, &a.right, file, line, col, fn_sym, scope, out),
        // macros (format!/println!), verbatim, and remaining variants: syn exposes
        // these as token streams or non-Expr children, so mint a node but don't
        // chase. Conservative — may miss flows into macro args, never invents.
        _ => push_node(out, file, line, col, "expr", "", fn_sym),
    }
}

/// Bind a pattern into scope: mint a `let_bind`/`param` slot for the common
/// single-ident case and return the name. Destructuring falls through (no bind).
fn bind_pat(
    pat: &syn::Pat,
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) -> Option<String> {
    if let syn::Pat::Ident(pi) = pat {
        let (l, c) = (pi.ident.span().start().line as u32, pi.ident.span().start().column as u32);
        let bind = push_node(out, file, l, c, "let_bind", &pi.ident.to_string(), fn_sym);
        scope.insert(pi.ident.to_string(), bind);
        Some(pi.ident.to_string())
    } else {
        None
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
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) -> String {
    let r = flow_expr(rhs, file, fn_sym, scope, out);
    if let syn::Expr::Path(p) = lhs {
        if let Some(name) = p.path.segments.last().map(|s| s.ident.to_string()) {
            let id = push_node(out, file, line, col, "var_write", &name, fn_sym);
            out.edges.push(DfEdge { from: r.clone(), to: id.clone() });
            scope.insert(name, id.clone());
            return id;
        }
    }
    r
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
