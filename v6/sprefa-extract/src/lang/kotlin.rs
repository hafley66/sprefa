//! The Kotlin extractor arm: tree-sitter-kotlin front-end for type/call/df,
//! ast-grep for cst. Mirrors GoSource (the "floor as the only tier" shape -
//! kotlin has no syn/oxc analog either): cst via ast-grep's kotlin grammar +
//! one tree-sitter-kotlin parse feeding the type/call/df projections.
//!
//! The grammar crate is `tree-sitter-kotlin-sg` (the ast-grep fork), NOT
//! `tree-sitter-kotlin`: it is the exact crate v5's kotlin front-end carries
//! (root Cargo.toml: `tree-sitter-kotlin-sg = "0.4"`, so the v6 parse is
//! byte-identical to the oracle's), it is already in this workspace's lock as
//! an ast-grep-language transitive (0.4.1, one copy), and it exports
//! `LANGUAGE: LanguageFn` the way tree-sitter-go 0.23 does, which tree-sitter
//! 0.25's `Language::new` wraps. Zero new dup risk (it deps only
//! `tree-sitter-language` + `cc`, no tree-sitter core).
//!
//! Span bridge: NONE needed (same as go.rs; unlike rust.rs's syn line/col ->
//! byte table). tree-sitter nodes give raw byte offsets directly
//! (`start_byte`/`end_byte`), so `Span { start: node.start_byte(), len:
//! node.end_byte() - node.start_byte() }` is the whole story.
//!
//! Commit A (skeleton): KotlinSource wires cst via ast-grep + a
//! tree-sitter-kotlin parse; type/call/df projections are stubbed empty.
//! Commit B ports `walk_kotlin_entities` + `kotlin_fn_type` (TypeF nodes +
//! arrow-type sigs); commit C ports `kt_walk_call_defs` + `kt_walk_call_sites`
//! (CallF); commit D ports `kotlin_dataflow_from` (DfF nodes + Direct edges,
//! incl. the `lam_sym` closure naming).
//!
//! Deferred follow-ups (the same set the other langs parked): the docs facet
//! (`walk_kotlin_docs`); the df enrichment aux (args/fields/lits/param_pos/
//! loops/nests); the type_edge candidates (`kotlin_decl_edges`) +
//! `Resolve<TypeF>`/`Resolve<CallF>` arms - v5 kotlin DOES emit type_edge, and
//! both resolve arms land with the traits/codegen arc, not this port. The
//! const facet is NOT ported: v5 kotlin emits no const entities and no
//! const_value rows (`extract` leaves `consts` at Default), so v6 matches by
//! emitting none either.

use std::collections::BTreeSet;

use crate::family::{
    CallF, CallKind, CallSite, CstF, DfEdgeKind, DfF, DfNodeKind, SigSlot, TypeEntityKind, TypeF,
    TypeSig,
};
use crate::rows::{Edge, FamilyBundle, Node};
use crate::seams::{Parser, Project};
use crate::shape::{NodeRef, Span, Strings};
use crate::source::{ExtractOutput, FamilyMask, Source};
use super::astgrep::{AstGrepParser, CstProjector};

// ── the tree-sitter-kotlin parse (one parse feeds type/call/df) ─────────────

/// Parse Kotlin source via tree-sitter-kotlin-sg. Port of v5's inline parse in
/// `KotlinTypes::extract` (src/graph/typegraph/kotlin.rs:13). tree-sitter
/// 0.25's `Language::new` wraps the `LanguageFn` tree-sitter-kotlin-sg 0.4
/// exports as `LANGUAGE`; the versions unify with what ast-grep-language
/// already transitively pulls (one copy, 0.4.1).
fn kt_parse(content: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    let lang = tree_sitter::Language::new(tree_sitter_kotlin_sg::LANGUAGE);
    parser.set_language(&lang).ok()?;
    parser.parse(content, None)
}

/// UTF-8 text of a tree-sitter node. Port of v5's inline `utf8_text` calls.
fn kt_text<'a>(node: tree_sitter::Node, src: &'a [u8]) -> &'a str {
    node.utf8_text(src).unwrap_or("")
}

/// The byte span of a tree-sitter node `[start_byte, end_byte)`.
fn node_span(node: tree_sitter::Node) -> Span {
    Span {
        start: node.start_byte() as u32,
        len: (node.end_byte() - node.start_byte()) as u32,
    }
}

/// The first direct child of `node` with `kind`. Port of v5 `kt_first_child`.
fn kt_first_child<'a>(node: tree_sitter::Node<'a>, kind: &str) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    let kids: Vec<tree_sitter::Node<'a>> = node.children(&mut cursor).collect();
    kids.into_iter().find(|c| c.kind() == kind)
}

// ════════════════════════════════════════════════════════════════════════════
// TypeF: entity nodes + arrow-type sigs. Commit B.
//
// Ports v5 `walk_kotlin_entities` (src/graph/typegraph/kotlin.rs:741) +
// `kotlin_fn_type` (kotlin.rs:847, the arrow-type payload). Entities:
// class_declaration / object_declaration / companion_object with a direct
// type_identifier child -> Class/Interface/Enum (keyword scan of the decl's own
// children: an `interface` keyword -> Interface, an `enum` keyword -> Enum,
// else Class); EVERY function_declaration (top-level, member, or local - v5
// never mints a Method entity for kotlin) -> Function + arrow sigs.
//
// v6 drops v5's `sym`/`parent`/`file`/`line`: a node is span+kind+name; the
// entity span is anchored at the decl node's start byte so
// `line_of(span.start)` equals v5's reported `entity.line` (the decl start
// row, 1-based). The type-edge candidates (`kotlin_decl_edges`: field/impl/
// variant/generic rows v5 kotlin DOES emit) are DEFERRED to the traits/codegen
// arc with `Resolve<TypeF>` - this port ships phase 1 only. No const facet
// (v5 kotlin leaves `consts` at Default; v6 matches by emitting none).
// ════════════════════════════════════════════════════════════════════════════

/// Project the TypeF family: one entity node per class/object/fun declaration
/// + an arrow-type sig per callable param/return type reference. Port of v5
/// `walk_kotlin_entities` + `kotlin_fn_type`.
fn project_types(
    root: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    walk_kotlin_entities(root, src, strings, sink);
}

/// Walk every declaration, minting one entity node per class/object/fun decl.
/// Port of v5 `walk_kotlin_entities`. Recurses everywhere: member funs, local
/// funs, nested classes, and companion objects all mint (v5's walk has no
/// depth gate). `companion_object` is a distinct grammar node from
/// `object_declaration`; both mint a `class` entity the same way.
fn walk_kotlin_entities(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "class_declaration" | "object_declaration" | "companion_object" => {
                let mut c = child.walk();
                let kids: Vec<tree_sitter::Node> = child.children(&mut c).collect();
                if let Some(id) = kids.iter().find(|n| n.kind() == "type_identifier") {
                    let name = kt_text(*id, src).to_string();
                    let kind = if kids.iter().any(|n| n.kind() == "interface") {
                        TypeEntityKind::Interface
                    } else if kids.iter().any(|n| n.kind() == "enum") {
                        TypeEntityKind::Enum
                    } else {
                        TypeEntityKind::Class
                    };
                    push_entity(sink, strings, node_span(child), &name, kind);
                }
            }
            "function_declaration" => {
                if let Some(id) = kt_first_child(child, "simple_identifier") {
                    let name = kt_text(id, src).to_string();
                    let span = node_span(child);
                    push_entity(sink, strings, span, &name, TypeEntityKind::Function);
                    fn_sigs(sink, strings, span, child, src);
                }
            }
            _ => {}
        }
        walk_kotlin_entities(child, src, strings, sink);
    }
}

fn push_entity(
    sink: &mut FamilyBundle<TypeF>,
    strings: &mut Strings,
    span: Span,
    name: &str,
    kind: TypeEntityKind,
) {
    sink.nodes.push(Node::new(span, kind).with_name(strings.intern(name)));
}

// ── arrow-type signatures (port of v5 `kotlin_fn_type`) ─────────────────────

/// The arrow-type sigs of one `fun`: param type-refs (one slot per `parameter`
/// child of `function_value_parameters`, positional) + return type-refs (all
/// unioned into slot pos=0). Declared type-parameter names and Kotlin builtins
/// are excluded from refs. Port of v5 `kotlin_fn_type`, incl. its overwrite
/// semantics: the LAST type-node child fills `ret` (an extension receiver's
/// user_type reads as `ret` until the real return type overwrites it - a
/// builtin receiver like `String` noise-filters to empty; V5-IS-CORRECT ports
/// the quirk). `owner` is the callable entity's span (the sig join key).
fn fn_sigs(
    sink: &mut FamilyBundle<TypeF>,
    strings: &mut Strings,
    owner: Span,
    node: tree_sitter::Node,
    src: &[u8],
) {
    let mut cursor = node.walk();
    let children: Vec<tree_sitter::Node> = node.children(&mut cursor).collect();

    // Declared type-parameter names: excluded from refs, like the decl pass.
    let mut tparams: BTreeSet<String> = BTreeSet::new();
    for n in &children {
        if n.kind() != "type_parameters" {
            continue;
        }
        let mut c = n.walk();
        for tp in n.children(&mut c).filter(|n| n.kind() == "type_parameter") {
            if let Some(name) = kt_first_child(tp, "type_identifier") {
                tparams.insert(kt_text(name, src).to_string());
            }
        }
    }

    let mut pos: u32 = 0;
    let mut ret_refs: Vec<String> = Vec::new();
    for n in &children {
        match n.kind() {
            "function_value_parameters" => {
                let mut c = n.walk();
                for p in n.children(&mut c).filter(|n| n.kind() == "parameter") {
                    // The parameter's name is a simple_identifier (not collected:
                    // collect_kotlin_refs only reads user_type); its type recurses.
                    for name in kotlin_type_refs(p, src, &tparams) {
                        push_sig(sink, strings, owner, SigSlot::Param, pos, &name);
                    }
                    pos += 1;
                }
            }
            // The return type is a type-node sibling after the parameter list.
            k if is_kotlin_type_node(k) => ret_refs = kotlin_type_refs(*n, src, &tparams),
            _ => {}
        }
    }
    for name in ret_refs {
        push_sig(sink, strings, owner, SigSlot::Ret, 0, &name);
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

fn is_kotlin_type_node(kind: &str) -> bool {
    matches!(
        kind,
        "user_type" | "nullable_type" | "function_type" | "parenthesized_type"
    )
}

/// Collect the type names referenced anywhere under `node`, de-duplicated and
/// sorted: each `user_type`'s own dotted path is one ref, its `type_arguments`
/// recurse into more refs. Declared type-parameter names and Kotlin builtins
/// are not refs. Port of v5 `kotlin_type_refs`/`collect_kotlin_refs`.
fn kotlin_type_refs(node: tree_sitter::Node, src: &[u8], params: &BTreeSet<String>) -> Vec<String> {
    let mut out = Vec::new();
    collect_kotlin_refs(node, src, params, &mut out);
    out.sort();
    out.dedup();
    out
}

fn collect_kotlin_refs(
    node: tree_sitter::Node,
    src: &[u8],
    params: &BTreeSet<String>,
    out: &mut Vec<String>,
) {
    if node.kind() == "user_type" {
        let mut cursor = node.walk();
        let segs: Vec<&str> = node
            .children(&mut cursor)
            .filter(|n| n.kind() == "type_identifier")
            .map(|n| kt_text(n, src))
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

/// Kotlin builtin type filter: a reference to `Int`/`String`/`Unit`/... carries
/// no resolvable declaration. Port of v5 `is_noise_kotlin`.
fn is_noise_kotlin(name: &str) -> bool {
    matches!(
        name,
        "Int" | "Long" | "Short" | "Byte" | "Float" | "Double" | "Boolean" | "Char"
            | "String" | "Unit" | "Any" | "Nothing"
    )
}

// ════════════════════════════════════════════════════════════════════════════
// CallF: callable definitions (nodes) + call sites (aux). Commit C.
//
// Ports v5 `kt_walk_call_defs` (defs, incl. ctors + lambda literals) +
// `kt_walk_call_sites`/`kt_callee` (sites). v5's `sym`/`end` line are dropped:
// a def is span + kind + name (the name is the bare identifier for callee
// resolution, NOT a qualified sym). The def span COVERS its body (decl start
// -> function_body end) so the seam's span-containment can bind a site's
// caller; the parity line reads `line_of(span.start)` = the decl start line
// (v5's `def.line`). Kind rules (v5-exact): a fun inside a class/object body
// is a Method (a nested LOCAL fun is Free - descending into a fn body resets
// the owner); primary/secondary constructors are Method rows named after the
// CLASS (so a `Widget(x)` call site name-matches here); a lambda literal
// inside a fn body is a nameless Lambda (a property-init lambda has no
// enclosing fn scope and is skipped). A fn with no name (an anonymous
// `fun(x) {}` expression) still mints a def with an empty name, like v5.
// ════════════════════════════════════════════════════════════════════════════

/// Project the CallF family: one def node per callable (Free / Method /
/// Lambda) + one site per call expression. Port of v5 `kt_walk_call_defs` +
/// `kt_walk_call_sites`.
fn project_call(
    root: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    kt_walk_call_defs(root, src, strings, sink, None, false);
    kt_walk_call_sites(root, src, strings, sink);
}

/// Walk every callable declaration, minting one def node per Free function /
/// Method / Lambda. Port of v5 `kt_walk_call_defs`. `parent` is the enclosing
/// class/object name (v5's `parent`); `in_fn` is v5's `!enclosing.is_empty()`:
/// a lambda literal only mints a Lambda def when inside a fn/lambda body (a
/// property-init lambda has no enclosing scope to join). v6 drops the sym
/// strings themselves - only the gate survives, the def's span is its only
/// coordinate.
fn kt_walk_call_defs(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
    parent: Option<&str>,
    in_fn: bool,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "class_declaration" | "object_declaration" => {
                let owner = kt_first_child(child, "type_identifier").map(|n| kt_text(n, src));
                // A class body is not a fn scope: reset `in_fn` (v5 resets
                // `enclosing` to "") so a bare property-init lambda is skipped;
                // a member fun opens its own scope below.
                kt_walk_call_defs(child, src, strings, sink, owner, false);
            }
            // @callable kotlin function / @callable kotlin method
            "function_declaration" => {
                let name = kt_first_child(child, "simple_identifier")
                    .map(|n| kt_text(n, src).to_string())
                    .unwrap_or_default();
                let kind = match parent {
                    Some(_) => CallKind::Method,
                    None => CallKind::Free,
                };
                // The def span covers the whole callable body [decl.start,
                // body.end) for span-containment resolution; abstract/interface
                // funs have no body, so fall back to the decl end (v5's exact
                // fallback). line_of(span.start) == v5's def.line.
                let span = def_span(child);
                sink.nodes.push(Node::new(span, kind).with_name(strings.intern(&name)));
                // v5 threads the fn's df_sym as `enclosing` and resets the
                // owner: a nested local fun is Free, not a method.
                kt_walk_call_defs(child, src, strings, sink, None, true);
            }
            // Primary/secondary constructors: Method rows named after the
            // class, so a `Widget(x)` call site resolves here via the bare-name
            // convention. Only minted inside a class/object body (parent Some).
            // @callable kotlin method
            "primary_constructor" | "secondary_constructor" => {
                if let Some(owner) = parent {
                    sink.nodes.push(
                        Node::new(node_span(child), CallKind::Method)
                            .with_name(strings.intern(owner)),
                    );
                }
                kt_walk_call_defs(child, src, strings, sink, parent, in_fn);
            }
            // `{ it + 1 }` inside a fn body: a nameless Lambda def (v5 keys it
            // by the same `lambda_sym` the df lift mints; v6 keeps only the
            // span - the df closure VALUE node carries that name instead).
            // @callable kotlin lambda
            "lambda_literal" if in_fn => {
                sink.nodes.push(Node::new(node_span(child), CallKind::Lambda));
                kt_walk_call_defs(child, src, strings, sink, parent, true);
            }
            _ => kt_walk_call_defs(child, src, strings, sink, parent, in_fn),
        }
    }
}

/// The def span covers the whole callable `[child.start, body.end)` for
/// span-containment resolution. Port of v5's `end` computation (the
/// function_body end, or the decl end for a bodyless fun).
fn def_span(child: tree_sitter::Node) -> Span {
    let start = child.start_byte();
    let end = kt_first_child(child, "function_body").unwrap_or(child).end_byte();
    Span { start: start as u32, len: (end - start) as u32 }
}

/// Walk every `call_expression`, minting one call site per call. Port of v5
/// `kt_walk_call_sites`. The site span is the LEAD callee node's span
/// (line_of(span.start) = v5's reported site line - for `recv.m()` the
/// navigation_expression's start, NOT the suffix's).
fn kt_walk_call_sites(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "call_expression" {
            if let Some((callee, lead)) = kt_callee(child, src) {
                sink.aux.sites.push(CallSite {
                    span: node_span(lead),
                    callee: strings.intern(&callee),
                    callee_path: None,
                });
            }
        }
        kt_walk_call_sites(child, src, strings, sink);
    }
}

/// (callee name, lead node) for a `call_expression`, or None when the callee
/// is not a plain/navigation name (e.g. an invoked lambda value). Port of v5
/// `kt_callee`: the lead is the call's first child that is not the
/// `call_suffix`; a bare `simple_identifier` is the callee, or the trailing
/// `simple_identifier` of a `navigation_expression` (`recv.qux()` -> "qux").
fn kt_callee<'a>(call: tree_sitter::Node<'a>, src: &[u8]) -> Option<(String, tree_sitter::Node<'a>)> {
    let mut cursor = call.walk();
    let lead = call.children(&mut cursor).find(|c| c.kind() != "call_suffix")?;
    match lead.kind() {
        "simple_identifier" => Some((kt_text(lead, src).to_string(), lead)),
        "navigation_expression" => {
            let nav = kt_first_child(lead, "navigation_suffix")?;
            let id = kt_first_child(nav, "simple_identifier")?;
            Some((kt_text(id, src).to_string(), lead))
        }
        _ => None,
    }
}

// ════════════════════════════════════════════════════════════════════════════
// DfF: intra-procedural value flow (nodes + Direct edges). Commit D.
//
// Ports v5 `kotlin_dataflow_from` (src/graph/typegraph/kotlin.rs:73): every
// value-bearing position in a callable's body becomes a NODE; local value flow
// becomes a Direct EDGE. The two-rule model (same as the go/rust lifts):
// value-bearing children flow into their parent, and a `val/var x = rhs`
// binds rhs -> x's slot with later reads flowing slot -> read.
//
// BYTE PARITY: v5 mints each node at `(node.start_position().row, .column)`,
// then bumps rows 1-based (`bump_node_lines_1based`); the oracle reconstructs
// the byte as `line_starts[row_0based] + col`, which equals tree-sitter's
// `node.start_byte()`. So v6 mints each node at the byte DIRECTLY (no line/col
// bridge, no post-pass) and the (kind, var, byte) triples + (from_byte,
// to_byte) edge pairs match v5 exactly. The lambda-tail `ret` sits at the
// lambda's END byte (v5's `node.end_position()`).
//
// What is DROPPED vs v5 (each deliberate, matching the TS/Rust/Go DfF ports):
//  - `fn_sym` ON NODES: the enclosing callable is not stored on every df node;
//    it is threaded through the walk (v5's own mechanism) purely so the
//    `closure` VALUE node carries v5's exact `lam_sym` name
//    (`{file}::function::{fn}::closure::{row}_{col}`, tree-sitter's 0-based
//    row/col of the lambda literal's start; nesting chains - EVERY fun, even a
//    method, roots at `{file}::function::{name}` per v5's kt_flow_fn). No sym
//    store: the name derives from the walk's containment path + span data.
//  - the enrichment aux: `args` (incl. the receiver slot -1 and named-arg
//    source positions), `fields` (named-arg labels), `param_pos`, `loops`,
//    `nests`. The EDGES already carry every value flow.
//  - `for`/`while`/`do-while` mint NO Loop node in v5 kotlin (the loop FACT is
//    aux, dropped); they fall to the conservative recursion, and the for-loop
//    variable is NEVER scope-bound (v5 exact).
// ════════════════════════════════════════════════════════════════════════════

/// Transient scope: a variable name -> its binding node (param or `let`).
/// v5 shares ONE scope through the whole fn walk (lambdas included): a capture
/// resolves, a lambda param shadows, and an `it` binding leaks past the lambda
/// body - all ported verbatim.
type Scope = std::collections::HashMap<String, NodeRef>;

/// Project the DfF family: each callable's body lifted to its value-flow
/// graph. Port of v5 `kotlin_dataflow_from` (the driver half). Unlike v5, no
/// post-pass bumps (v6 stores bytes directly, not 0-based rows), and
/// `loops`/`nests` aux is dropped. `file` roots each fn_sym (the closure value
/// node's name derives from it).
fn project_df(
    root: tree_sitter::Node,
    src: &[u8],
    file: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    kt_walk_fns(root, src, file, strings, sink);
}

/// Walk every function_declaration, lifting each body. Port of v5
/// `kt_walk_fns`. Only function_declarations lift (ctors, property
/// initializers, and class bodies do not); nested funs are found by the
/// recursion.
fn kt_walk_fns(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "function_declaration" {
            kt_flow_fn(child, src, file, strings, sink);
        }
        kt_walk_fns(child, src, file, strings, sink);
    }
}

/// Seed `param` nodes from the parameter list, then walk the body. Port of v5
/// `kt_flow_fn` (the `param_pos` aux is dropped). EVERY fun - method or not -
/// roots its fn_sym at `{file}::function::{name}` (v5's exact mint, so a
/// lambda inside a method still joins under the bare fun sym). The body's tail
/// value is the implicit return: it flows into a `ret` node minted at the
/// BODY's start byte (v5's `body.start_position()` - covers both the block
/// form and `fun f() = expr`). An explicit `return EXPR` mints its own ret in
/// the jump_expression arm.
fn kt_flow_fn(
    fn_node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    let name = kt_first_child(fn_node, "simple_identifier")
        .map(|n| kt_text(n, src))
        .unwrap_or("");
    let fn_sym = format!("{file}::function::{name}");
    let mut scope = Scope::new();
    if let Some(params) = kt_first_child(fn_node, "function_value_parameters") {
        let mut cursor = params.walk();
        for p in params.children(&mut cursor).filter(|n| n.kind() == "parameter") {
            if let Some(idn) = kt_first_child(p, "simple_identifier") {
                let v = kt_text(idn, src).to_string();
                let id = df_push(sink, strings, idn.start_byte() as u32, DfNodeKind::Param, Some(&v));
                scope.insert(v, id);
            }
        }
    }
    if let Some(body) = kt_first_child(fn_node, "function_body") {
        if let Some(tail) = flow_kt(body, src, &fn_sym, strings, &mut scope, sink) {
            let ret = df_push(sink, strings, body.start_byte() as u32, DfNodeKind::Ret, None);
            df_edge(sink, tail, ret);
        }
    }
}

/// Returns the node carrying the value of this subtree, or None when the
/// subtree is not value-bearing (statements, wrappers, bindings handled
/// inline). Conservative on unsupported constructs: may miss flows, never
/// invents. Port of v5 `flow_kt`; byte-exact (each node minted at
/// `node.start_byte()`, the lambda-tail ret at `node.end_byte()`).
fn flow_kt(
    node: tree_sitter::Node,
    src: &[u8],
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> Option<NodeRef> {
    let start_byte = node.start_byte() as u32;
    match node.kind() {
        // A name in expression position is a read; the role is decided by its
        // parent: under variable_declaration it is a binding target, under
        // parameter a param, under call_expression the callee - never a read.
        "simple_identifier" => {
            let parent_kind = node.parent().map(|p| p.kind());
            match parent_kind.as_deref() {
                Some("variable_declaration") | Some("parameter") | Some("call_expression") => None,
                _ => {
                    let v = kt_text(node, src).to_string();
                    let id = df_push(sink, strings, start_byte, DfNodeKind::VarRead, Some(&v));
                    if let Some(binding) = scope.get(&v) {
                        df_edge(sink, *binding, id);
                    }
                    Some(id)
                }
            }
        }
        // f(args): every argument value flows into the call result. A named
        // argument `f(x = v)` flows its VALUE (the name ident is a label, never
        // walked); a trailing lambda is the call's last positional argument
        // (the lambda_literal arm lifts it and returns its closure node). A
        // navigation callee `recv.m(a)` flows the receiver in too. A
        // capitalized callee is a constructor call (Kotlin classes are
        // UpperCamelCase), minted as a `new` node carrying the type name. The
        // args/fields aux (positions, named-arg labels) is dropped.
        "call_expression" => {
            let callee = node.child(0);
            let mut recv: Option<NodeRef> = None;
            let mut callee_name = String::new();
            match callee.map(|c| c.kind()) {
                Some("simple_identifier") => {
                    callee_name = kt_text(callee.unwrap(), src).to_string();
                }
                Some("navigation_expression") => {
                    let nav = callee.unwrap();
                    if let Some(obj) = nav.child(0) {
                        recv = flow_kt(obj, src, fn_sym, strings, scope, sink);
                    }
                    if let Some(idn) = kt_first_child(nav, "navigation_suffix")
                        .and_then(|s| kt_first_child(s, "simple_identifier"))
                    {
                        callee_name = kt_text(idn, src).to_string();
                    }
                }
                _ => {}
            }
            let mut arg_ids: Vec<NodeRef> = Vec::new();
            if let Some(suffix) = kt_first_child(node, "call_suffix") {
                if let Some(vargs) = kt_first_child(suffix, "value_arguments") {
                    let mut cursor = vargs.walk();
                    for va in vargs.children(&mut cursor).filter(|n| n.kind() == "value_argument") {
                        // Named form: value_argument = simple_identifier '=' expr.
                        let mut vc = va.walk();
                        let kids: Vec<tree_sitter::Node> = va.children(&mut vc).collect();
                        let eq_at = kids.iter().position(|k| k.kind() == "=");
                        let val_node = match eq_at {
                            Some(i) if i >= 1 && kids[i - 1].kind() == "simple_identifier" => {
                                kids.get(i + 1).copied()
                            }
                            _ => None,
                        };
                        let vid = match val_node {
                            Some(v) => flow_kt(v, src, fn_sym, strings, scope, sink),
                            None => flow_kt(va, src, fn_sym, strings, scope, sink),
                        };
                        if let Some(vid) = vid {
                            arg_ids.push(vid);
                        }
                    }
                }
                // A trailing lambda (`xs.map { it + 1 }`) is the call's last
                // positional argument; the lambda_literal arm lifts it and
                // returns its `closure` value node.
                if let Some(al) = kt_first_child(suffix, "annotated_lambda") {
                    if let Some(ll) = kt_first_child(al, "lambda_literal") {
                        if let Some(vid) = flow_kt(ll, src, fn_sym, strings, scope, sink) {
                            arg_ids.push(vid);
                        }
                    }
                }
            }
            let is_ctor = callee_name.chars().next().is_some_and(|c| c.is_uppercase());
            let (kind, name) = if is_ctor {
                (DfNodeKind::New, Some(callee_name.as_str()))
            } else {
                (DfNodeKind::CallRes, None)
            };
            let id = df_push(sink, strings, start_byte, kind, name);
            if let Some(r) = recv {
                df_edge(sink, r, id);
            }
            for arg_id in arg_ids {
                df_edge(sink, arg_id, id);
            }
            Some(id)
        }
        // `base.f` outside a call: a member read (the base flows into a
        // `member` node carrying the accessed name). As a call's callee (parent
        // == call_expression) the call arm owns it instead.
        "navigation_expression" => {
            if node.parent().map(|p| p.kind()) == Some("call_expression") {
                return None;
            }
            let obj = node
                .child(0)
                .and_then(|c| flow_kt(c, src, fn_sym, strings, scope, sink));
            let name = kt_first_child(node, "navigation_suffix")
                .and_then(|s| kt_first_child(s, "simple_identifier"))
                .map(|n| kt_text(n, src).to_string())
                .unwrap_or_default();
            let id = df_push(sink, strings, start_byte, DfNodeKind::Member, Some(&name));
            if let Some(o) = obj {
                df_edge(sink, o, id);
            }
            Some(id)
        }
        // val/var x = rhs: mint the binding slot, flow rhs -> slot, register.
        "property_declaration" => {
            let mut bind: Option<(String, NodeRef)> = None;
            let mut rhs_id: Option<NodeRef> = None;
            let mut cursor = node.walk();
            for c in node.children(&mut cursor) {
                match c.kind() {
                    "variable_declaration" => {
                        if let Some(si) = kt_first_child(c, "simple_identifier") {
                            let v = kt_text(si, src).to_string();
                            let id = df_push(
                                sink,
                                strings,
                                si.start_byte() as u32,
                                DfNodeKind::LetBind,
                                Some(&v),
                            );
                            bind = Some((v, id));
                        }
                    }
                    "=" | "binding_pattern_kind" | "val" | "var" => {}
                    _ => {
                        if let Some(id) = flow_kt(c, src, fn_sym, strings, scope, sink) {
                            rhs_id = Some(id);
                        }
                    }
                }
            }
            if let (Some((v, bid)), Some(rhs)) = (bind, rhs_id) {
                df_edge(sink, rhs, bid);
                scope.insert(v, bid);
            }
            None
        }
        // Wrappers / statements: flow the last value-bearing child through.
        "value_argument" | "statements" | "function_body" | "source_file" => {
            kt_recurse_children(node, src, fn_sym, strings, scope, sink)
        }
        // `{ x -> body }` / `{ it + 1 }`: lift the lambda as its OWN fn scope
        // under v5's `lam_sym` (`{fn_sym}::closure::{row}_{col}`, tree-sitter's
        // 0-based row/col of the literal's start; chains when nested), same
        // shape as Go func literals. Declared lambda params bind by name; with
        // no declared parameter list Kotlin's implicit `it` binds at slot 0.
        // The enclosing `scope` is shared, so a captured outer variable's read
        // still resolves (and the `it` binding leaks past the body - v5 exact).
        // The tail value flows into a `ret` node at the lambda's END byte. The
        // `closure` VALUE node stays in the enclosing fn and carries the exact
        // sym as its name.
        "lambda_literal" => {
            let pos = node.start_position();
            let lam_sym = format!("{fn_sym}::closure::{}_{}", pos.row, pos.column);
            let mut seeded = false;
            if let Some(lp) = kt_first_child(node, "lambda_parameters") {
                let mut cursor = lp.walk();
                for vd in lp.children(&mut cursor).filter(|n| n.kind() == "variable_declaration") {
                    if let Some(idn) = kt_first_child(vd, "simple_identifier") {
                        let v = kt_text(idn, src).to_string();
                        let id = df_push(
                            sink,
                            strings,
                            idn.start_byte() as u32,
                            DfNodeKind::Param,
                            Some(&v),
                        );
                        scope.insert(v, id);
                        seeded = true;
                    }
                }
            }
            if !seeded {
                let id = df_push(sink, strings, start_byte, DfNodeKind::Param, Some("it"));
                scope.insert("it".into(), id);
            }
            let tail = kt_first_child(node, "statements")
                .and_then(|s| flow_kt(s, src, &lam_sym, strings, scope, sink));
            if let Some(t) = tail {
                let ret = df_push(sink, strings, node.end_byte() as u32, DfNodeKind::Ret, None);
                df_edge(sink, t, ret);
            }
            Some(df_push(sink, strings, start_byte, DfNodeKind::Closure, Some(&lam_sym)))
        }
        // return EXPR: the returned value flows into the fn's `ret` node.
        "jump_expression" => {
            let mut inner = None;
            let mut cursor = node.walk();
            for c in node.children(&mut cursor) {
                if c.kind() != "return" {
                    if let Some(id) = flow_kt(c, src, fn_sym, strings, scope, sink) {
                        inner = Some(id);
                    }
                }
            }
            let id = df_push(sink, strings, start_byte, DfNodeKind::Ret, None);
            if let Some(v) = inner {
                df_edge(sink, v, id);
            }
            Some(id)
        }
        // a OP b: both operands taint the result (taint-vs-dataflow: `a + 1`
        // propagates `a` into the result). Kotlin splits operators across
        // additive/multiplicative/infix kinds (no named fields), so the first
        // and last NAMED children are the two operands; a single-named-child
        // form flows the same subtree twice, like v5.
        "additive_expression" | "multiplicative_expression" | "infix_expression" => {
            let mut cursor = node.walk();
            let kids: Vec<tree_sitter::Node> = node.named_children(&mut cursor).collect();
            let l = kids.first().and_then(|n| flow_kt(*n, src, fn_sym, strings, scope, sink));
            let r = kids.last().and_then(|n| flow_kt(*n, src, fn_sym, strings, scope, sink));
            let id = df_push(sink, strings, start_byte, DfNodeKind::Binop, None);
            if let Some(lid) = l {
                df_edge(sink, lid, id);
            }
            if let Some(rid) = r {
                df_edge(sink, rid, id);
            }
            Some(id)
        }
        "string_literal" | "integer_literal" | "real_literal" | "boolean_literal"
        | "character_literal" | "long_literal" => {
            Some(df_push(sink, strings, start_byte, DfNodeKind::Lit, None))
        }
        // Everything else (when-arms, if/for/while statements, elvis, index/
        // range expressions, ...): recurse conservatively, surfacing the last
        // value-bearing child. NOTE: v5 kotlin's for/while/do-while arms mint
        // no df node - the loop fact is dropped aux and the loop variable is
        // never scope-bound - so they belong to this same fallback.
        _ => kt_recurse_children(node, src, fn_sym, strings, scope, sink),
    }
}

/// Walk all children of a node conservatively, surfacing the last
/// value-bearing child's node. Port of v5 `kt_recurse_children`.
fn kt_recurse_children(
    node: tree_sitter::Node,
    src: &[u8],
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> Option<NodeRef> {
    let mut last = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(id) = flow_kt(child, src, fn_sym, strings, scope, sink) {
            last = Some(id);
        }
    }
    last
}

/// Push one df node, returning its `NodeRef` (the dense index edges reference).
/// The span is start-only (len 0): df node identity is `(span.start, kind)`,
/// byte-exact with v5's reconstructed `line_starts[row] + col`. Port of v5
/// `push_node` (minus fn_sym/file/aux).
fn df_push(
    sink: &mut FamilyBundle<DfF>,
    strings: &mut Strings,
    byte: u32,
    kind: DfNodeKind,
    name: Option<&str>,
) -> NodeRef {
    let node_ref = NodeRef(sink.nodes.len() as u32);
    let mut node = Node::new(Span { start: byte, len: 0 }, kind);
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
// KotlinSource: the Kotlin Source (cst via ast-grep + type/call/df via
// tree-sitter-kotlin).
//
// The two-parser, masked shape (mirrors GoSource/RustSource/TsSource). cst runs
// through ast-grep (one dep = the CST floor for every lang); type/call/df run
// through ONE tree-sitter-kotlin parse (three masked projections over the same
// tree). ONE shared `Strings` across all four families.
// ════════════════════════════════════════════════════════════════════════════

/// The Kotlin `Source`. `matches` = the path ends in `.kt` or `.kts` (v5
/// `KotlinTypes::matches`). cst via ast-grep's kotlin grammar; type/call/df via
/// one tree-sitter-kotlin parse.
#[derive(Default)]
pub struct KotlinSource;

impl Source for KotlinSource {
    fn name(&self) -> &'static str {
        "kotlin"
    }

    fn matches(&self, path: &str) -> bool {
        path.ends_with(".kt") || path.ends_with(".kts")
    }

    fn extract(&self, path: &str, content: &[u8], mask: FamilyMask) -> ExtractOutput {
        let mut strings = Strings::new();

        // cst via ast-grep (masked). ast-grep's SupportLang has a kotlin
        // grammar (the same tree-sitter-kotlin-sg crate), so a .kt parses
        // losslessly. Owns its () arena; dropped at block end. A failed
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

        // type/call/df via ONE tree-sitter-kotlin parse (masked). Byte spans
        // come straight off the tree-sitter nodes (no line/col bridge, unlike
        // syn). A failed parse leaves all three None (partial output: cst
        // above may be Some).
        let mut types = None;
        let mut call = None;
        let mut df = None;
        if mask.types || mask.call || mask.df {
            if let Ok(src) = std::str::from_utf8(content) {
                if let Some(tree) = kt_parse(src) {
                    let root = tree.root_node();
                    let src_bytes = src.as_bytes();
                    if mask.types {
                        let mut bundle = FamilyBundle::<TypeF>::default();
                        project_types(root, src_bytes, &mut strings, &mut bundle);
                        types = Some(bundle);
                    }
                    if mask.call {
                        let mut bundle = FamilyBundle::<CallF>::default();
                        project_call(root, src_bytes, &mut strings, &mut bundle);
                        call = Some(bundle);
                    }
                    if mask.df {
                        let mut bundle = FamilyBundle::<DfF>::default();
                        project_df(root, src_bytes, path, &mut strings, &mut bundle);
                        df = Some(bundle);
                    }
                }
            }
        }

        ExtractOutput { strings, cst, types, call, df }
    }
}
