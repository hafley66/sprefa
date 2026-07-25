//! The Go extractor arm: tree-sitter-go front-end for type/call/df, ast-grep for
//! cst. Mirrors RustSource/TsSource (same shape, different front-end): cst via
//! ast-grep's go grammar + one tree-sitter-go parse feeding the type/call/df
//! projections.
//!
//! Span bridge: NONE needed (unlike rust.rs's syn line/col -> byte table).
//! tree-sitter nodes give raw byte offsets directly (`start_byte`/`end_byte`), so
//! `Span { start: node.start_byte(), len: node.end_byte() - node.start_byte() }`
//! is the whole story. This is simpler than the rust port.
//!
//! Commit A (skeleton): GoSource wires cst via ast-grep + a tree-sitter-go parse;
//! type/call/df projections are stubbed empty. Commit B ports `walk_go_entities`
//! (TypeF nodes + arrow-type sigs); commit C ports `go_walk_call_defs` +
//! `go_walk_call_sites` (CallF); commit D ports `go_dataflow_from` (DfF nodes +
//! Direct edges).
//!
//! Deferred to `Resolve<TypeF>` (commit 4): type EDGES (field/impl/generic from
//! `go_edges_from`). Deferred follow-ups: the docs facet (`walk_go_docs`); the df
//! enrichment aux (args/fields/lits/param_pos/loops/nests). The const facet is
//! NOT ported: v5 go emits no const entities and no const_value rows
//! (`walk_go_entities` skips `const_declaration`; `extract` leaves `consts`
//! empty), so v6 matches by emitting none either.

use std::collections::BTreeSet;

use crate::family::{
    CallF, CallKind, CallSite, CstF, DfEdgeKind, DfF, DfNodeKind, SigSlot, TypeEntityKind, TypeF,
    TypeSig,
};
use crate::rows::{Edge, FamilyBundle, Node};
use crate::seams::{Parser, Project};
use crate::shape::{Span, Strings};
use crate::source::{ExtractOutput, FamilyMask, Source};
use super::astgrep::{AstGrepParser, CstProjector};

// ── the tree-sitter-go parse (one parse feeds type/call/df) ──────────────────

/// Parse Go source via tree-sitter-go. Port of v5 `go_parse`
/// (src/graph/typegraph/go.rs:41). tree-sitter 0.25's `Language::new` wraps the
/// `LanguageFn` tree-sitter-go 0.23 exports as `LANGUAGE`; the versions unify
/// with what ast-grep-language already transitively pulls.
fn go_parse(content: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    let lang = tree_sitter::Language::new(tree_sitter_go::LANGUAGE);
    parser.set_language(&lang).ok()?;
    parser.parse(content, None)
}

/// UTF-8 text of a tree-sitter node. Port of v5 `go_text`.
fn go_text<'a>(node: tree_sitter::Node, src: &'a [u8]) -> &'a str {
    node.utf8_text(src).unwrap_or("")
}

// ════════════════════════════════════════════════════════════════════════════
// TypeF: entity nodes + arrow-type sigs. Commit B.
//
// Ports v5 `walk_go_entities` (the entity half) + `go_fn_type` (the arrow-type
// payload). The name-resolved type EDGES (field/impl/generic from `go_edges_from`)
// land with `Resolve<TypeF>` (commit 4); phase 1 stays pure-content span nodes.
// No const facet (v5 go emits none: walk_go_entities skips const_declaration and
// extract leaves consts empty, so v6 matches by emitting none).
//
// v6 drops v5's `parent`/`sym`/`mint_sym`/`go_owner_kinds`: a node is span+kind+
// name; the parent linkage is span-containment at the seam. The method receiver
// (`go_receiver_type`) is kept ONLY as the gate v5 uses to emit-or-skip a method
// (a malformed receiver skips the entity); the resolved owner name is dropped.
// ════════════════════════════════════════════════════════════════════════════

/// Project the TypeF family: one entity node per type/function/method declaration
/// + an arrow-type sig per callable param/return type reference. Port of v5
/// `walk_go_entities` + `go_fn_type`.
fn project_types(
    root: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    walk_go_entities(root, src, strings, sink);
}

/// Walk every type/function/method declaration, minting one entity node per decl
/// + one arrow-type sig per callable param/return type ref. Port of v5
/// `walk_go_entities`. The entity span is anchored at the spec/decl node's start
/// byte so `line_of(span.start)` equals v5's reported `entity.line` (the spec/
/// decl start row, 1-based).
fn walk_go_entities(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "type_declaration" => {
                let mut spec_cursor = child.walk();
                for spec in child.children(&mut spec_cursor) {
                    let (name_node, kind) = match spec.kind() {
                        "type_spec" => {
                            let k = match spec.child_by_field_name("type").map(|t| t.kind()) {
                                Some("struct_type") => TypeEntityKind::Struct,
                                Some("interface_type") => TypeEntityKind::Interface,
                                _ => TypeEntityKind::Alias,
                            };
                            (spec.child_by_field_name("name"), k)
                        }
                        "type_alias" => (spec.child_by_field_name("name"), TypeEntityKind::Alias),
                        _ => continue,
                    };
                    let Some(name_node) = name_node else { continue };
                    let span = node_span(spec);
                    let name = go_text(name_node, src).to_string();
                    push_entity(sink, strings, span, &name, kind);
                }
            }
            "function_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let span = node_span(child);
                    let name = go_text(name_node, src).to_string();
                    push_entity(sink, strings, span, &name, TypeEntityKind::Function);
                    fn_sigs(sink, strings, span, child, src);
                }
            }
            "method_declaration" => {
                // Gate on a resolvable receiver, matching v5: a malformed receiver
                // skips the entity (so v6 emits-or-skips exactly as v5 does). The
                // owner name itself is dropped (no parent sym in v6).
                if let (Some(name_node), Some(())) = (
                    child.child_by_field_name("name"),
                    go_receiver_type(child, src).map(|_| ()),
                ) {
                    let span = node_span(child);
                    let name = go_text(name_node, src).to_string();
                    push_entity(sink, strings, span, &name, TypeEntityKind::Method);
                    fn_sigs(sink, strings, span, child, src);
                }
            }
            _ => {}
        }
        walk_go_entities(child, src, strings, sink);
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

/// The byte span of a tree-sitter node `[start_byte, end_byte)`.
fn node_span(node: tree_sitter::Node) -> Span {
    Span {
        start: node.start_byte() as u32,
        len: (node.end_byte() - node.start_byte()) as u32,
    }
}

// ── arrow-type signatures (port of v5 `go_fn_type`) ──────────────────────────

/// The arrow-type sigs of one callable: param type-refs (positional, receiver
/// skipped, grouped params `a, b int` expanded to one slot per name) + return
/// type-refs (all unioned into slot pos=0, matching v5's flat `ret` list). Port
/// of v5 `go_fn_type`. `owner` is the callable entity's span (the sig join key).
fn fn_sigs(
    sink: &mut FamilyBundle<TypeF>,
    strings: &mut Strings,
    owner: Span,
    node: tree_sitter::Node,
    src: &[u8],
) {
    let tparams = go_type_param_names(node, src);
    let mut pos: u32 = 0;
    if let Some(plist) = node.child_by_field_name("parameters") {
        let mut cursor = plist.walk();
        for param in plist.children(&mut cursor) {
            if !matches!(param.kind(), "parameter_declaration" | "variadic_parameter_declaration") {
                continue;
            }
            let Some(ty) = param.child_by_field_name("type") else { continue };
            // A grouped parameter (`a, b int`) is ONE grammar node but TWO slots;
            // each declared name gets its own slot sharing the group's type.
            let mut name_cursor = param.walk();
            let count = param
                .children(&mut name_cursor)
                .filter(|n| n.kind() == "identifier")
                .count()
                .max(1);
            let refs = go_type_refs(ty, src, &tparams);
            for _ in 0..count {
                for name in &refs {
                    push_sig(sink, strings, owner, SigSlot::Param, pos, name);
                }
                pos += 1;
            }
        }
    }
    // Go's multi-value return unions every result type ref into one flat list at
    // pos 0 (v5 stores one flat `ret` list regardless of slot count).
    if let Some(result) = node.child_by_field_name("result") {
        if result.kind() == "parameter_list" {
            let mut cursor = result.walk();
            for param in result.children(&mut cursor) {
                if matches!(param.kind(), "parameter_declaration" | "variadic_parameter_declaration") {
                    if let Some(ty) = param.child_by_field_name("type") {
                        for name in go_type_refs(ty, src, &tparams) {
                            push_sig(sink, strings, owner, SigSlot::Ret, 0, &name);
                        }
                    }
                }
            }
        } else {
            for name in go_type_refs(result, src, &tparams) {
                push_sig(sink, strings, owner, SigSlot::Ret, 0, &name);
            }
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

/// The callable's declared type-parameter names (the exclusion set: a generic
/// `[T]` referencing itself is not a sig). Port of v5 `go_fn_type`'s tparams.
fn go_type_param_names(node: tree_sitter::Node, src: &[u8]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if let Some(tp_list) = node.child_by_field_name("type_parameters") {
        let mut cursor = tp_list.walk();
        for tp in tp_list
            .children(&mut cursor)
            .filter(|n| n.kind() == "type_parameter_declaration")
        {
            let mut inner = tp.walk();
            for child in tp.children(&mut inner).filter(|n| n.kind() == "identifier") {
                names.insert(go_text(child, src).to_string());
            }
        }
    }
    names
}

/// Collect the named type references anywhere under `node`, de-duplicated and
/// sorted. A `qualified_type` (`pkg.Type`) is one ref kept as `pkg.Type` (NOT
/// recursed into; its inner `type_identifier` would double-count). A bare
/// `type_identifier` is a ref unless it names a type parameter or a predeclared/
/// builtin type. Port of v5 `go_type_refs`/`collect_go_refs`.
fn go_type_refs(node: tree_sitter::Node, src: &[u8], params: &BTreeSet<String>) -> Vec<String> {
    let mut out = Vec::new();
    collect_go_refs(node, src, params, &mut out);
    out.sort();
    out.dedup();
    out
}

fn collect_go_refs(
    node: tree_sitter::Node,
    src: &[u8],
    params: &BTreeSet<String>,
    out: &mut Vec<String>,
) {
    match node.kind() {
        "type_identifier" => {
            let name = go_text(node, src).to_string();
            if !params.contains(&name) && !is_noise_go(&name) {
                out.push(name);
            }
        }
        "qualified_type" => {
            let pkg = node.child_by_field_name("package").map(|n| go_text(n, src)).unwrap_or("");
            let name = node.child_by_field_name("name").map(|n| go_text(n, src)).unwrap_or("");
            if !pkg.is_empty() && !name.is_empty() {
                out.push(format!("{pkg}.{name}"));
            }
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_go_refs(child, src, params, out);
            }
        }
    }
}

/// Predeclared/builtin type filter: a reference to `int`/`string`/`error`/...
/// carries no resolvable declaration. Port of v5 `is_noise_go`.
fn is_noise_go(name: &str) -> bool {
    matches!(
        name,
        "int" | "int8" | "int16" | "int32" | "int64"
            | "uint" | "uint8" | "uint16" | "uint32" | "uint64" | "uintptr"
            | "float32" | "float64" | "complex64" | "complex128"
            | "bool" | "string" | "byte" | "rune" | "error" | "any" | "comparable"
    )
}

/// A method's receiver base type name, `*`/generic-args stripped
/// (`(r *Repo[T])` -> `"Repo"`). None for a malformed/absent receiver. Port of v5
/// `go_receiver_type`; used here only as the emit-or-skip gate for a method entity.
fn go_receiver_type(method: tree_sitter::Node, src: &[u8]) -> Option<String> {
    let recv_list = method.child_by_field_name("receiver")?;
    let mut cursor = recv_list.walk();
    let param = recv_list
        .children(&mut cursor)
        .find(|n| n.kind() == "parameter_declaration")?;
    let mut ty = param.child_by_field_name("type")?;
    loop {
        match ty.kind() {
            "pointer_type" => ty = ty.named_child(0)?,
            "generic_type" => ty = ty.child_by_field_name("type")?,
            _ => break,
        }
    }
    match ty.kind() {
        "type_identifier" => Some(go_text(ty, src).to_string()),
        "qualified_type" => ty.child_by_field_name("name").map(|n| go_text(n, src).to_string()),
        _ => None,
    }
}

// ════════════════════════════════════════════════════════════════════════════
// CallF: callable definitions (nodes) + call sites (aux). Commit C.
//
// Ports v5 `go_walk_call_defs` (defs, incl. func_literal lambdas) +
// `go_walk_call_sites` (sites). v5's `mint_sym`/`lambda_sym`/`end` line are
// dropped: a def is span + kind + name (the name is the bare identifier for
// callee resolution, NOT a qualified sym). The def span COVERS its body (decl
// start -> block end) so the seam's span-containment can bind a site's caller;
// the parity line reads `line_of(span.start)` = the decl start line (v5's
// `def.line`). Lambda defs (func_literal inside a fn/method body) keep kind=
// Lambda, name=None (v5's empty name). A package-level func_literal
// (`var f = func(){}`) is skipped: v5's lift only walks fn/method bodies, so
// there is no enclosing scope to join (enclosing == "").
// ════════════════════════════════════════════════════════════════════════════

/// Project the CallF family: one def node per callable (Free / Method / Lambda)
/// + one site per call expression. Port of v5 `go_walk_call_defs` +
/// `go_walk_call_sites`.
fn project_call(
    root: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    go_walk_call_defs(root, src, strings, sink, false);
    go_walk_call_sites(root, src, strings, sink);
}

/// The def span covers the whole callable body `[child.start, body.end)` for
/// span-containment resolution. Port of v5 `end_of(child)` (the body end line).
fn def_span(child: tree_sitter::Node) -> Span {
    let start = child.start_byte();
    let end = child.child_by_field_name("body").unwrap_or(child).end_byte();
    Span { start: start as u32, len: (end - start) as u32 }
}

/// Walk every callable declaration, minting one def node per Free function /
/// Method / Lambda. Port of v5 `go_walk_call_defs`. `in_fn` is v5's
/// `!enclosing.is_empty()`: a func_literal only mints a Lambda def when inside a
/// fn/method body (a package-level func literal has no enclosing scope to join).
fn go_walk_call_defs(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
    in_fn: bool,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            // @callable go function
            "function_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let span = def_span(child);
                    let name = go_text(name_node, src).to_string();
                    sink.nodes.push(Node::new(span, CallKind::Free).with_name(strings.intern(&name)));
                    go_walk_call_defs(child, src, strings, sink, true);
                    continue;
                }
            }
            // @callable go method
            "method_declaration" => {
                if let (Some(name_node), Some(_)) =
                    (child.child_by_field_name("name"), go_receiver_type(child, src))
                {
                    let span = def_span(child);
                    let name = go_text(name_node, src).to_string();
                    sink.nodes
                        .push(Node::new(span, CallKind::Method).with_name(strings.intern(&name)));
                    go_walk_call_defs(child, src, strings, sink, true);
                    continue;
                }
            }
            // `func(...) {...}` inside a fn/method body: a Lambda. A package-level
            // `var f = func(){}` (in_fn == false) is skipped, matching v5
            // (enclosing == "" -> no Lambda def).
            // @callable go lambda
            "func_literal" if in_fn => {
                let span = def_span(child);
                sink.nodes.push(Node::new(span, CallKind::Lambda));
                go_walk_call_defs(child, src, strings, sink, true);
                continue;
            }
            _ => {}
        }
        go_walk_call_defs(child, src, strings, sink, in_fn);
    }
}

/// Walk every `call_expression`, minting one call site per call. The callee is
/// the trailing name: a bare `identifier`, or a `selector_expression`'s field
/// (`recv.M` -> "M"). A type conversion `T(x)` reads as an ordinary call (the
// syntactic tier can't tell a conversion from a call). Port of v5
/// `go_walk_call_sites` + `go_callee`. The site span is the CALLEE node's start
/// (line_of(span.start) = v5's reported site line).
fn go_walk_call_sites(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "call_expression" {
            if let Some(func) = child.child_by_field_name("function") {
                let callee = match func.kind() {
                    "identifier" => Some(go_text(func, src).to_string()),
                    "selector_expression" => {
                        func.child_by_field_name("field").map(|field| go_text(field, src).to_string())
                    }
                    _ => None,
                };
                if let Some(callee) = callee {
                    sink.aux.sites.push(CallSite {
                        span: node_span(func),
                        callee: strings.intern(&callee),
                        callee_path: None,
                    });
                }
            }
        }
        go_walk_call_sites(child, src, strings, sink);
    }
}

// ════════════════════════════════════════════════════════════════════════════
// DfF: intra-procedural value flow (nodes + Direct edges). Commit D.
//
// Ports v5 `go_dataflow_from` (src/graph/typegraph/go.rs:576). Every value-bearing
// position in a callable's body becomes a NODE; local value flow becomes a Direct
// EDGE. The two are the dataflow graph the engine's `df_reaches` closure walks.
//
// BYTE PARITY: v5 mints each node at `(node.start_position().row, .column)` and
// the oracle reconstructs the byte as `line_starts[row] + col`, which equals
// tree-sitter's `node.start_byte()`. So v6 mints each node at `node.start_byte()`
// directly (no line/col bridge, unlike the syn front-end in rust.rs). The
// (kind, var, byte) triples and the (from_byte, to_byte) edge pairs match v5
// exactly.
//
// What is DROPPED vs v5 (each deliberate, matching the TS/Rust DfF ports):
//  - `fn_sym` ON NODES: the enclosing callable is not stored on every df node;
//    it is threaded through the walk (v5's own mechanism) purely so the
//    `closure` VALUE node carries v5's exact `lam_sym` name
//    (`{file}::function::{fn}::closure::{row}_{col}`, tree-sitter's 0-based
//    row/col; methods root at `{file}::method::{Recv}.{m}`). No sym store:
//    the name derives from the walk's containment path + the literal's start.
//  - the enrichment aux: `args`, `fields`, `lits`, `param_pos`, `loops`,
//    `nests`. The EDGES already carry every value flow.
// ════════════════════════════════════════════════════════════════════════════

/// Transient scope: a variable name -> its binding node (param or `let`).
type Scope = std::collections::HashMap<String, NodeRef>;

use crate::shape::NodeRef;

/// Project the DfF family: each callable's body lifted to its value-flow graph.
/// Port of v5 `go_dataflow_from` (the driver half). Unlike v5, no post-pass bumps
/// (v6 stores bytes directly, not 0-based rows), and `loops`/`nests` aux is dropped.
/// `file` roots each fn_sym (the closure value node's name derives from it).
fn project_df(
    root: tree_sitter::Node,
    src: &[u8],
    file: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    go_walk_fns(root, src, file, strings, sink);
}

/// Walk every function/method declaration, lifting each body. Port of v5
/// `go_walk_fns` (incl. its syms: `{file}::function::{name}` /
/// `{file}::method::{Recv}.{name}`). The receiver is NOT seeded as a param (it
/// lives in the `receiver` field, not `parameters`); a read of the receiver in
/// the body has no binding edge, matching v5.
fn go_walk_fns(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let fn_sym = format!("{file}::function::{}", go_text(name_node, src));
                    go_flow_fn(child, src, &fn_sym, strings, sink);
                }
            }
            "method_declaration" => {
                if let (Some(name_node), Some(owner)) =
                    (child.child_by_field_name("name"), go_receiver_type(child, src))
                {
                    let fn_sym = format!("{file}::method::{owner}.{}", go_text(name_node, src));
                    go_flow_fn(child, src, &fn_sym, strings, sink);
                }
            }
            _ => {}
        }
        go_walk_fns(child, src, file, strings, sink);
    }
}

/// Seed `param` nodes from the (non-receiver) parameter list, then walk the body.
/// A grouped parameter (`a, b int`) mints one param node PER declared name,
/// matching `go_fn_type`'s slot count; an unnamed parameter still advances the
/// position counter so later named params keep the right index. Port of v5
/// `go_flow_fn` (the `param_pos` aux is dropped).
fn go_flow_fn(
    fn_node: tree_sitter::Node,
    src: &[u8],
    fn_sym: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    let mut scope = Scope::new();
    if let Some(params) = fn_node.child_by_field_name("parameters") {
        let mut cursor = params.walk();
        for param in params.children(&mut cursor) {
            if !matches!(param.kind(), "parameter_declaration" | "variadic_parameter_declaration") {
                continue;
            }
            let mut name_cursor = param.walk();
            let names: Vec<tree_sitter::Node> = param
                .children(&mut name_cursor)
                .filter(|n| n.kind() == "identifier")
                .collect();
            if names.is_empty() {
                continue;
            }
            for name_node in names {
                let name = go_text(name_node, src).to_string();
                let node = df_push(sink, strings, name_node.start_byte() as u32, DfNodeKind::Param, Some(&name));
                scope.insert(name, node);
            }
        }
    }
    if let Some(body) = fn_node.child_by_field_name("body") {
        flow_go(body, src, fn_sym, strings, &mut scope, sink);
    }
}

/// Returns the node carrying the value of this subtree, or None when the subtree
/// is a pure statement/binder handled inline (bindings, control-flow headers).
/// Unhandled node kinds fall through to `go_recurse_children`, conservative.
/// Port of v5 `flow_go`; byte-exact (each node minted at `node.start_byte()`).
fn flow_go(
    node: tree_sitter::Node,
    src: &[u8],
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> Option<NodeRef> {
    let start_byte = node.start_byte() as u32;
    match node.kind() {
        "identifier" => {
            let name = go_text(node, src).to_string();
            let read = df_push(sink, strings, start_byte, DfNodeKind::VarRead, Some(&name));
            if let Some(binding) = scope.get(&name) {
                df_edge(sink, *binding, read);
            }
            Some(read)
        }
        "interpreted_string_literal" | "raw_string_literal" | "int_literal" | "float_literal"
        | "imaginary_literal" | "rune_literal" | "true" | "false" | "nil" | "iota" => {
            Some(df_push(sink, strings, start_byte, DfNodeKind::Lit, None))
        }
        // f(args): every argument flows into the call result; a selector callee
        // `recv.M(args)` flows the receiver in too. Go has no syntactic ctor
        // marker (capitalization means EXPORTED), so every call is `call_res`.
        "call_expression" => {
            let func = node.child_by_field_name("function");
            let mut receiver: Option<NodeRef> = None;
            if let Some(func) = func {
                if func.kind() == "selector_expression" {
                    if let Some(operand) = func.child_by_field_name("operand") {
                        receiver = flow_go(operand, src, fn_sym, strings, scope, sink);
                    }
                }
            }
            let mut arg_ids = Vec::new();
            if let Some(args) = node.child_by_field_name("arguments") {
                let mut cursor = args.walk();
                for arg in args.children(&mut cursor) {
                    if let Some(id) = flow_go(arg, src, fn_sym, strings, scope, sink) {
                        arg_ids.push(id);
                    }
                }
            }
            let call_res = df_push(sink, strings, start_byte, DfNodeKind::CallRes, None);
            if let Some(recv) = receiver {
                df_edge(sink, recv, call_res);
            }
            for arg_id in arg_ids {
                df_edge(sink, arg_id, call_res);
            }
            Some(call_res)
        }
        // `base.Field` outside a call: a member read. As a call's callee (parent
        // is the enclosing call_expression) the call arm above owns it instead.
        "selector_expression" => {
            if node.parent().map(|p| p.kind()) == Some("call_expression") {
                return None;
            }
            let operand = node
                .child_by_field_name("operand")
                .and_then(|o| flow_go(o, src, fn_sym, strings, scope, sink));
            let name = node
                .child_by_field_name("field")
                .map(|f| go_text(f, src).to_string())
                .unwrap_or_default();
            let member = df_push(sink, strings, start_byte, DfNodeKind::Member, Some(&name));
            if let Some(operand) = operand {
                df_edge(sink, operand, member);
            }
            Some(member)
        }
        // `T{...}` / `[]T{...}` / `map[K]V{...}`: an instantiation. Each element
        // flows into the `new` node (the keyed/positional field labels are dropped
        // aux). The key subtree of a `keyed_element` is a LABEL, never walked.
        "composite_literal" => {
            let type_name = node
                .child_by_field_name("type")
                .map(|t| go_type_name_text(t, src))
                .unwrap_or_default();
            let new_node = df_push(sink, strings, start_byte, DfNodeKind::New, Some(&type_name));
            if let Some(body) = node.child_by_field_name("body") {
                go_flow_literal_fields(body, src, fn_sym, strings, scope, sink, new_node);
            }
            Some(new_node)
        }
        // A `literal_value` reached directly (not via `composite_literal`): a
        // nested element literal whose type is implied by the enclosing composite.
        "literal_value" => {
            let new_node = df_push(sink, strings, start_byte, DfNodeKind::New, None);
            go_flow_literal_fields(node, src, fn_sym, strings, scope, sink, new_node);
            Some(new_node)
        }
        "binary_expression" => {
            let left = node
                .child_by_field_name("left")
                .and_then(|n| flow_go(n, src, fn_sym, strings, scope, sink));
            let right = node
                .child_by_field_name("right")
                .and_then(|n| flow_go(n, src, fn_sym, strings, scope, sink));
            let binop = df_push(sink, strings, start_byte, DfNodeKind::Binop, None);
            if let Some(left) = left {
                df_edge(sink, left, binop);
            }
            if let Some(right) = right {
                df_edge(sink, right, binop);
            }
            Some(binop)
        }
        "unary_expression" => {
            let inner = node
                .child_by_field_name("operand")
                .and_then(|n| flow_go(n, src, fn_sym, strings, scope, sink));
            let unop = df_push(sink, strings, start_byte, DfNodeKind::Unop, None);
            if let Some(inner) = inner {
                df_edge(sink, inner, unop);
            }
            Some(unop)
        }
        // `x := rhs` (possibly multi-value): bind each declared name to a fresh
        // `let_bind` node. A matching-arity rhs pairs positionally; a mismatched
        // arity taints every target from the first rhs value conservatively.
        "short_var_declaration" => {
            let rhs_ids = node
                .child_by_field_name("right")
                .map(|right| go_flow_expr_list(right, src, fn_sym, strings, scope, sink))
                .unwrap_or_default();
            if let Some(left) = node.child_by_field_name("left") {
                let mut cursor = left.walk();
                let names: Vec<tree_sitter::Node> = left
                    .children(&mut cursor)
                    .filter(|n| n.kind() == "identifier")
                    .collect();
                go_bind(&names, &rhs_ids, DfNodeKind::LetBind, src, strings, scope, sink);
            }
            None
        }
        "var_declaration" | "const_declaration" => {
            let mut cursor = node.walk();
            for spec in node
                .children(&mut cursor)
                .filter(|n| matches!(n.kind(), "var_spec" | "const_spec"))
            {
                go_flow_spec(spec, src, fn_sym, strings, scope, sink);
            }
            None
        }
        // `lhs = rhs` (incl. compound `+=`/etc): rebind so later reads see the
        // new value. Non-identifier targets (`x.Field = v`) still flow for side-
        // effect visibility without a scope rebind.
        "assignment_statement" => {
            let rhs_ids = node
                .child_by_field_name("right")
                .map(|right| go_flow_expr_list(right, src, fn_sym, strings, scope, sink))
                .unwrap_or_default();
            if let Some(left) = node.child_by_field_name("left") {
                let mut cursor = left.walk();
                let targets: Vec<tree_sitter::Node> = left.children(&mut cursor).collect();
                let names: Vec<tree_sitter::Node> = targets
                    .iter()
                    .filter(|n| n.kind() == "identifier")
                    .copied()
                    .collect();
                go_bind(&names, &rhs_ids, DfNodeKind::VarWrite, src, strings, scope, sink);
                for target in targets
                    .iter()
                    .filter(|n| n.kind() != "identifier" && n.kind() != ",")
                {
                    flow_go(*target, src, fn_sym, strings, scope, sink);
                }
            }
            None
        }
        // `return a, b`: one `ret` node PER returned value, each fed by its own
        // expression. A naked `return` mints one empty `ret` node so the fn has a
        // visible graph endpoint. The ret node sits at the EXPRESSION's byte (v5
        // uses the expression's position), not the return statement's.
        "return_statement" => {
            let mut cursor = node.walk();
            let list = node
                .children(&mut cursor)
                .find(|n| n.kind() == "expression_list");
            let mut minted = false;
            if let Some(list) = list {
                let mut list_cursor = list.walk();
                for expr in list.children(&mut list_cursor) {
                    if let Some(value) = flow_go(expr, src, fn_sym, strings, scope, sink) {
                        let ret = df_push(sink, strings, expr.start_byte() as u32, DfNodeKind::Ret, None);
                        df_edge(sink, value, ret);
                        minted = true;
                    }
                }
            }
            if !minted {
                df_push(sink, strings, start_byte, DfNodeKind::Ret, None);
            }
            None
        }
        "if_statement" => {
            if let Some(init) = node.child_by_field_name("initializer") {
                flow_go(init, src, fn_sym, strings, scope, sink);
            }
            if let Some(cond) = node.child_by_field_name("condition") {
                flow_go(cond, src, fn_sym, strings, scope, sink);
            }
            if let Some(cons) = node.child_by_field_name("consequence") {
                flow_go(cons, src, fn_sym, strings, scope, sink);
            }
            if let Some(alt) = node.child_by_field_name("alternative") {
                flow_go(alt, src, fn_sym, strings, scope, sink);
            }
            Some(df_push(sink, strings, start_byte, DfNodeKind::If, None))
        }
        // `for range/clause/cond { body }`: walk the header (binding the range
        // variable when present), then walk the body. The loop FACT (span/var) is
        // dropped aux. A for_statement's non-`body` child is at most ONE of {bare
        // condition, `for_clause`, `range_clause`} per the grammar.
        "for_statement" => {
            let mut loop_var = String::new();
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "range_clause" => {
                        if let Some(right) = child.child_by_field_name("right") {
                            flow_go(right, src, fn_sym, strings, scope, sink);
                        }
                        if let Some(left) = child.child_by_field_name("left") {
                            let mut left_cursor = left.walk();
                            let names: Vec<tree_sitter::Node> = left
                                .children(&mut left_cursor)
                                .filter(|n| n.kind() == "identifier")
                                .collect();
                            for name_node in &names {
                                let name = go_text(*name_node, src).to_string();
                                if name == "_" {
                                    continue;
                                }
                                let bind = df_push(
                                    sink,
                                    strings,
                                    name_node.start_byte() as u32,
                                    DfNodeKind::LetBind,
                                    Some(&name),
                                );
                                scope.insert(name.clone(), bind);
                                if loop_var.is_empty() {
                                    loop_var = name;
                                }
                            }
                        }
                    }
                    "for_clause" => {
                        if let Some(init) = child.child_by_field_name("initializer") {
                            flow_go(init, src, fn_sym, strings, scope, sink);
                        }
                        if let Some(cond) = child.child_by_field_name("condition") {
                            flow_go(cond, src, fn_sym, strings, scope, sink);
                        }
                        if let Some(update) = child.child_by_field_name("update") {
                            flow_go(update, src, fn_sym, strings, scope, sink);
                        }
                    }
                    "block" | "for" => {}
                    _ => {
                        flow_go(child, src, fn_sym, strings, scope, sink);
                    }
                }
            }
            if let Some(body) = node.child_by_field_name("body") {
                flow_go(body, src, fn_sym, strings, scope, sink);
            }
            Some(df_push(sink, strings, start_byte, DfNodeKind::Loop, Some(&loop_var)))
        }
        // `func(...) {...}`: lift as its OWN fn scope under v5's `lam_sym`
        // (`{fn_sym}::closure::{row}_{col}`, tree-sitter's 0-based row/col of the
        // literal's start; chains when nested), same shape as Rust
        // closures/Kotlin lambda literals. The enclosing `scope` is shared, so a
        // captured outer variable's read still resolves. The `closure` VALUE node
        // stays in the enclosing fn and carries that exact sym as its name.
        "func_literal" => {
            let pos = node.start_position();
            let lam_sym = format!("{fn_sym}::closure::{}_{}", pos.row, pos.column);
            if let Some(params) = node.child_by_field_name("parameters") {
                let mut cursor = params.walk();
                for param in params.children(&mut cursor) {
                    if !matches!(param.kind(), "parameter_declaration" | "variadic_parameter_declaration") {
                        continue;
                    }
                    let mut name_cursor = param.walk();
                    let names: Vec<tree_sitter::Node> = param
                        .children(&mut name_cursor)
                        .filter(|n| n.kind() == "identifier")
                        .collect();
                    if names.is_empty() {
                        continue;
                    }
                    for name_node in names {
                        let name = go_text(name_node, src).to_string();
                        let node_ref = df_push(
                            sink,
                            strings,
                            name_node.start_byte() as u32,
                            DfNodeKind::Param,
                            Some(&name),
                        );
                        scope.insert(name, node_ref);
                    }
                }
            }
            if let Some(body) = node.child_by_field_name("body") {
                flow_go(body, src, &lam_sym, strings, scope, sink);
            }
            Some(df_push(sink, strings, start_byte, DfNodeKind::Closure, Some(&lam_sym)))
        }
        // everything else (blocks/statement lists, expression statements,
        // parenthesized/index/slice/type-assertion/conversion expressions,
        // go/defer/send/select/switch/labeled statements, ...): recurse
        // conservatively, surfacing the last value-bearing child.
        _ => go_recurse_children(node, src, fn_sym, strings, scope, sink),
    }
}

/// Flow every element of an `expression_list`, in source order, returning one
/// `Option<NodeRef>` per element. Port of v5 `go_flow_expr_list`.
fn go_flow_expr_list(
    list: tree_sitter::Node,
    src: &[u8],
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> Vec<Option<NodeRef>> {
    let mut cursor = list.walk();
    list.children(&mut cursor)
        .map(|e| flow_go(e, src, fn_sym, strings, scope, sink))
        .collect()
}

/// Bind each name in `names` to a fresh node of `kind` ("let_bind" for a
/// declaration, "var_write" for a plain assignment), wiring the matching rhs
/// value when arity lines up (else every target derives from the first rhs value,
/// conservative). `_` binds nothing. Port of v5 `go_bind`.
fn go_bind(
    names: &[tree_sitter::Node],
    rhs_ids: &[Option<NodeRef>],
    kind: DfNodeKind,
    src: &[u8],
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    for (i, name_node) in names.iter().enumerate() {
        let name = go_text(*name_node, src).to_string();
        if name == "_" {
            continue;
        }
        let bind = df_push(sink, strings, name_node.start_byte() as u32, kind, Some(&name));
        let rhs = if rhs_ids.len() == names.len() {
            rhs_ids.get(i).cloned().flatten()
        } else {
            rhs_ids.first().cloned().flatten()
        };
        if let Some(rhs) = rhs {
            df_edge(sink, rhs, bind);
        }
        scope.insert(name, bind);
    }
}

/// A `var_spec`/`const_spec`: bind each declared identifier to a `let_bind` node
/// fed by its initializer. Port of v5 `go_flow_spec`.
fn go_flow_spec(
    spec: tree_sitter::Node,
    src: &[u8],
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    let mut cursor = spec.walk();
    let names: Vec<tree_sitter::Node> = spec
        .children(&mut cursor)
        .filter(|n| n.kind() == "identifier")
        .collect();
    let rhs_ids = spec
        .child_by_field_name("value")
        .map(|value| go_flow_expr_list(value, src, fn_sym, strings, scope, sink))
        .unwrap_or_default();
    go_bind(&names, &rhs_ids, DfNodeKind::LetBind, src, strings, scope, sink);
}

/// A composite literal's body (`literal_value`): each `keyed_element`'s value
/// (and each bare `literal_element`'s value) flows into `owner`. The field labels
/// are dropped aux; the EDGES carry the flow. Port of v5
/// `go_flow_literal_fields`.
fn go_flow_literal_fields(
    lit: tree_sitter::Node,
    src: &[u8],
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
    owner: NodeRef,
) {
    let mut cursor = lit.walk();
    for child in lit.children(&mut cursor) {
        let value_wrap = match child.kind() {
            "keyed_element" => child.child_by_field_name("value"),
            "literal_element" => Some(child),
            _ => continue,
        };
        let Some(value_wrap) = value_wrap else { continue };
        let Some(inner) = value_wrap.named_child(0) else { continue };
        if let Some(value) = flow_go(inner, src, fn_sym, strings, scope, sink) {
            df_edge(sink, value, owner);
        }
    }
}

/// The textual name of a composite literal's element type, for the `new` node's
/// name: a bare/qualified named type keeps its name; an anonymous array/slice/
/// map/struct literal type has no name (`""`). Port of v5 `go_type_name_text`.
fn go_type_name_text(node: tree_sitter::Node, src: &[u8]) -> String {
    match node.kind() {
        "type_identifier" => go_text(node, src).to_string(),
        "qualified_type" => node
            .child_by_field_name("name")
            .map(|n| go_text(n, src).to_string())
            .unwrap_or_default(),
        "generic_type" => node
            .child_by_field_name("type")
            .map(|t| go_type_name_text(t, src))
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// Walk all children conservatively, surfacing the last value-bearing child's
/// node. The generic fallback `flow_go` reaches for every node kind it doesn't
/// special-case. Port of v5 `go_recurse_children`.
fn go_recurse_children(
    node: tree_sitter::Node,
    src: &[u8],
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> Option<NodeRef> {
    let mut cursor = node.walk();
    let mut last = None;
    for child in node.children(&mut cursor) {
        if let Some(id) = flow_go(child, src, fn_sym, strings, scope, sink) {
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
// GoSource: the Go Source (cst via ast-grep + type/call/df via tree-sitter-go).
//
// The two-parser, masked shape (mirrors RustSource/TsSource). cst runs through
// ast-grep (one dep = the CST floor for every lang); type/call/df run through
// ONE tree-sitter-go parse (three masked projections over the same tree). ONE
// shared `Strings` across all four families.
// ════════════════════════════════════════════════════════════════════════════

/// The Go `Source`. `matches` = the path ends in `.go`. cst via ast-grep's go
/// grammar; type/call/df via one tree-sitter-go parse.
#[derive(Default)]
pub struct GoSource;

impl Source for GoSource {
    fn name(&self) -> &'static str {
        "go"
    }

    fn matches(&self, path: &str) -> bool {
        path.ends_with(".go")
    }

    fn extract(&self, path: &str, content: &[u8], mask: FamilyMask) -> ExtractOutput {
        let mut strings = Strings::new();

        // cst via ast-grep (masked). ast-grep's SupportLang has a go grammar, so
        // a .go parses losslessly. Owns its () arena; dropped at block end. A
        // failed ast-grep parse leaves cst None (no panic).
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

        // type/call/df via ONE tree-sitter-go parse (masked). Byte spans come
        // straight off the tree-sitter nodes (no line/col bridge, unlike syn). A
        // failed parse leaves all three None (partial output: cst above may be
        // Some).
        let mut types = None;
        let mut call = None;
        let mut df = None;
        if mask.types || mask.call || mask.df {
            if let Ok(src) = std::str::from_utf8(content) {
                if let Some(tree) = go_parse(src) {
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
