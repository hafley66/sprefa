//! Kotlin extractor arm (tree-sitter-kotlin front-end): TypeLang impl,
//! dataflow, type edges, call defs/sites, entities/docs/fn_type. Pure code
//! motion out of the former single typegraph.rs; zero behavior change.

use std::collections::BTreeSet;

use super::*;

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
        let mut docs = Vec::new();
        walk_kotlin_docs(root, src, file, &mut docs);
        TypeFacts { entities, edges: kotlin_edges_from(root, src), docs, ..Default::default() }
    }
    // One tree-sitter parse feeds defs + sites, same shape as the Rust pass.
    fn extract_calls(&self, file: &str, content: &str) -> CallFacts {
        let mut parser = tree_sitter::Parser::new();
        let lang = tree_sitter::Language::new(tree_sitter_kotlin_sg::LANGUAGE);
        if parser.set_language(&lang).is_err() {
            return CallFacts::default();
        }
        let Some(tree) = parser.parse(content, None) else {
            return CallFacts::default();
        };
        let src = content.as_bytes();
        let root = tree.root_node();
        let mut defs = Vec::new();
        kt_walk_call_defs(root, src, file, None, "", &mut defs);
        let mut sites = Vec::new();
        kt_walk_call_sites(root, src, file, &mut sites);
        CallFacts { defs, sites }
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
    // tree-sitter rows are 0-based; the df contract is 1-based (syn and the TS
    // line_at both emit 1-based), so a (file, line) join against call_site —
    // the call_node bridge every interprocedural hop rides — is a single
    // equality across languages. Nodes and loop spans bump together, so the
    // nest containment below stays internally consistent. `bump_node_lines_1based`
    // bumps each node's line AND rebuilds its id to match (the coordinate
    // de-intern reconstructs the id from the columns, so the id's line must equal
    // the stored line), remapping every id-referencing fact; `compute_nests`
    // below then reads the rebuilt ids.
    for l in &mut out.loops { l.start += 1; l.end += 1; }
    bump_node_lines_1based(&mut out);
    out.nests = compute_nests(&out.nodes, &out.loops);
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
    let mut scope: std::collections::HashMap<String, NodeIdx> = std::collections::HashMap::new();
    if let Some(params) = kt_first_child(fn_node, "function_value_parameters") {
        let mut cur = params.walk();
        for (pos, p) in params.children(&mut cur).filter(|n| n.kind() == "parameter").enumerate() {
            if let Some(idn) = kt_first_child(p, "simple_identifier") {
                let ppos = idn.start_position();
                let v = idn.utf8_text(src).unwrap_or("").to_string();
                let id = push_node(out, file, ppos.row as u32, ppos.column as u32, "param", &v, &fn_sym);
                out.param_pos.push((id.clone(), pos as u32));
                scope.insert(v, id);
            }
        }
    }
    if let Some(body) = kt_first_child(fn_node, "function_body") {
        // The body's tail value is the implicit return (block tail, or the
        // expression of `fun f() = expr`): flow it into the fn's `ret` node.
        // Explicit `return EXPR` is handled in the jump_expression arm.
        if let Some(tail) = flow_kt(body, src, file, &fn_sym, &mut scope, out) {
            let bpos = body.start_position();
            let ret = push_node(out, file, bpos.row as u32, bpos.column as u32, "ret", "", &fn_sym);
            out.edges.push(DfEdge { from: tail, to: ret });
        }
    }
}

/// Returns the node id carrying the value of this subtree, or None when the
/// subtree is not value-bearing (statements, wrappers, bindings handled inline).
fn flow_kt(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, NodeIdx>,
    out: &mut DataflowFacts,
) -> Option<NodeIdx> {
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
        // f(args): every argument value flows into the call result, and
        // `df_arg` records its 0-based source position (named args keep their
        // source index — an approximation when Kotlin reorders them). A named
        // argument `f(x = v)` also lands in `df_field` under its name; the
        // name ident is a label, not a read, so it is never walked. A
        // navigation callee `recv.m(a)` flows the receiver in at slot -1; a
        // capitalized callee is a constructor call (Kotlin classes are
        // UpperCamelCase), minted as a `new` node carrying the type name.
        "call_expression" => {
            let callee = node.child(0);
            let mut recv: Option<NodeIdx> = None;
            let mut callee_name = String::new();
            match callee.map(|c| c.kind()) {
                Some("simple_identifier") => {
                    callee_name = callee.unwrap().utf8_text(src).unwrap_or("").to_string();
                }
                Some("navigation_expression") => {
                    let nav = callee.unwrap();
                    if let Some(obj) = nav.child(0) {
                        recv = flow_kt(obj, src, file, fn_sym, scope, out);
                    }
                    if let Some(idn) = kt_first_child(nav, "navigation_suffix")
                        .and_then(|s| kt_first_child(s, "simple_identifier"))
                    {
                        callee_name = idn.utf8_text(src).unwrap_or("").to_string();
                    }
                }
                _ => {}
            }
            // (source position, named-arg name if any, value node id)
            let mut arg_ids: Vec<(Option<String>, NodeIdx)> = Vec::new();
            if let Some(suffix) = kt_first_child(node, "call_suffix") {
                if let Some(vargs) = kt_first_child(suffix, "value_arguments") {
                    let mut cur = vargs.walk();
                    for va in vargs.children(&mut cur).filter(|n| n.kind() == "value_argument") {
                        // named form: value_argument = simple_identifier '=' expr
                        let mut kids = Vec::new();
                        let mut vc = va.walk();
                        for k in va.children(&mut vc) { kids.push(k); }
                        let eq_at = kids.iter().position(|k| k.kind() == "=");
                        let (name, val_node) = match eq_at {
                            Some(i) if i >= 1 && kids[i - 1].kind() == "simple_identifier" => {
                                (Some(kids[i - 1].utf8_text(src).unwrap_or("").to_string()),
                                 kids.get(i + 1).copied())
                            }
                            _ => (None, None),
                        };
                        let vid = match val_node {
                            Some(v) => flow_kt(v, src, file, fn_sym, scope, out),
                            None => flow_kt(va, src, file, fn_sym, scope, out),
                        };
                        if let Some(vid) = vid {
                            arg_ids.push((name, vid));
                        }
                    }
                }
                // A trailing lambda (`xs.map { it + 1 }`) is the call's last
                // positional argument; the lambda_literal arm lifts it and
                // returns its `closure` value node.
                if let Some(al) = kt_first_child(suffix, "annotated_lambda") {
                    if let Some(ll) = kt_first_child(al, "lambda_literal") {
                        if let Some(vid) = flow_kt(ll, src, file, fn_sym, scope, out) {
                            arg_ids.push((None, vid));
                        }
                    }
                }
            }
            let is_ctor = callee_name.chars().next().is_some_and(|c| c.is_uppercase());
            let (kind, var) = if is_ctor { ("new", callee_name.as_str()) } else { ("call_res", "") };
            let id = push_node(out, file, pos.row as u32, pos.column as u32, kind, var, fn_sym);
            if let Some(r) = recv {
                out.edges.push(DfEdge { from: r.clone(), to: id.clone() });
                out.args.push((id.clone(), -1, r));
            }
            for (p, (name, vid)) in arg_ids.into_iter().enumerate() {
                out.edges.push(DfEdge { from: vid.clone(), to: id.clone() });
                out.args.push((id.clone(), p as i64, vid.clone()));
                if let Some(n) = name {
                    out.fields.push((id.clone(), n, vid));
                }
            }
            Some(id)
        }
        // `base.f` outside a call: a member read. The base flows into a
        // `member` node whose var is the accessed name, so a `df_field` write
        // can be matched against the read of the same field. As a call's
        // callee (parent == call_expression) the call arm owns it instead —
        // receiver at slot -1, name on the call node.
        "navigation_expression" => {
            if node.parent().map(|p| p.kind()) == Some("call_expression") {
                return None;
            }
            let obj = node.child(0).and_then(|c| flow_kt(c, src, file, fn_sym, scope, out));
            let name = kt_first_child(node, "navigation_suffix")
                .and_then(|s| kt_first_child(s, "simple_identifier"))
                .map(|n| n.utf8_text(src).unwrap_or("").to_string())
                .unwrap_or_default();
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "member", &name, fn_sym);
            if let Some(o) = obj {
                out.edges.push(DfEdge { from: o, to: id.clone() });
            }
            Some(id)
        }
        // val/var x = rhs: mint the binding slot, flow rhs -> slot, register.
        "property_declaration" => {
            let mut bind: Option<(String, NodeIdx)> = None;
            let mut rhs_id: Option<NodeIdx> = None;
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
        // `{ x -> body }` / `{ it + 1 }`: lift the lambda as its OWN fn scope —
        // "param" nodes with df_param slots (the implicit `it` when no
        // parameter list is declared), body walked under the lambda sym, tail
        // value into a "ret" node — and mint the `closure` VALUE node in the
        // enclosing fn, carrying the lambda sym in `var` (the join key a
        // higher-order hop uses; see std/flow.dl flow_lambda). The enclosing
        // scope is shared, so captures still resolve.
        "lambda_literal" => {
            let lam_sym = lambda_sym(fn_sym, &format!("{}_{}", pos.row, pos.column));
            let mut seeded = false;
            if let Some(lp) = kt_first_child(node, "lambda_parameters") {
                let mut cur = lp.walk();
                for (i, vd) in lp.children(&mut cur).filter(|n| n.kind() == "variable_declaration").enumerate() {
                    if let Some(idn) = kt_first_child(vd, "simple_identifier") {
                        let ppos = idn.start_position();
                        let v = idn.utf8_text(src).unwrap_or("").to_string();
                        let id = push_node(out, file, ppos.row as u32, ppos.column as u32, "param", &v, &lam_sym);
                        out.param_pos.push((id.clone(), i as u32));
                        scope.insert(v, id);
                        seeded = true;
                    }
                }
            }
            if !seeded {
                // No declared parameter list: Kotlin's implicit `it`, slot 0.
                let id = push_node(out, file, pos.row as u32, pos.column as u32, "param", "it", &lam_sym);
                out.param_pos.push((id.clone(), 0));
                scope.insert("it".into(), id);
            }
            let tail = kt_first_child(node, "statements")
                .and_then(|s| flow_kt(s, src, file, &lam_sym, scope, out));
            if let Some(t) = tail {
                let end = node.end_position();
                let ret = push_node(out, file, end.row as u32, end.column as u32, "ret", "", &lam_sym);
                out.edges.push(DfEdge { from: t, to: ret });
            }
            Some(push_node(out, file, pos.row as u32, pos.column as u32, "closure", &lam_sym, fn_sym))
        }
        // return EXPR: the returned value flows into the fn's `ret` node — the
        // sink the interprocedural backward hop reads.
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
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "ret", "", fn_sym);
            if let Some(v) = inner { out.edges.push(DfEdge { from: v, to: id.clone() }); }
            Some(id)
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
    scope: &mut std::collections::HashMap<String, NodeIdx>,
    out: &mut DataflowFacts,
) -> Option<NodeIdx> {
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

// --- Kotlin call-graph pass (tree-sitter): `function_declaration` nodes become
// CallDefs (a fn inside a class/object/interface body is a Method keyed to the
// enclosing type, a top-level fun is Free), and every `call_expression` becomes
// a CallSite whose callee is the called name as written. Caller resolution is
// the engine's span-containment pass; mirror the Rust convention (bare callee
// name, body span end line for containment). ---

/// Walk for `function_declaration` defs, tracking the enclosing type name so a
/// member fn keys to its owner. Descending into a class/object body carries the
/// owner; descending into a fn body resets to None (a local fun is not a method
/// of the surrounding type).
fn kt_walk_call_defs(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    parent: Option<&str>,
    enclosing: &str,
    out: &mut Vec<CallDef>,
) {
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        match child.kind() {
            "class_declaration" | "object_declaration" => {
                let owner = kt_first_child(child, "type_identifier")
                    .map(|n| n.utf8_text(src).unwrap_or("").to_string());
                // A class body is not a fn scope: reset `enclosing` to "" (df
                // lifts only function_declaration bodies) so a bare property-init
                // lambda is skipped; a member fun opens its own Function/None scope.
                kt_walk_call_defs(child, src, file, owner.as_deref(), "", out);
            }
            // @callable kotlin function
            // @callable kotlin method
            "function_declaration" => {
                let name = kt_first_child(child, "simple_identifier")
                    .map(|n| n.utf8_text(src).unwrap_or("").to_string())
                    .unwrap_or_default();
                let (kind, ekind) = match parent {
                    Some(_) => (CallKind::Method, EntityKind::Method),
                    None => (CallKind::Free, EntityKind::Function),
                };
                // body span end (1-based) bounds the def for callsite containment;
                // abstract/interface fns have no body, so fall back to the decl end.
                let end = kt_first_child(child, "function_body")
                    .unwrap_or(child)
                    .end_position()
                    .row as u32
                    + 1;
                // `kt_flow_fn` lifts EVERY function_declaration as Function/None
                // (even a method), so a lambda inside this fn joins df under that
                // sym, not the method sym. A nested local fun is Free (parent None).
                let df_sym = mint_sym(file, EntityKind::Function, &name, None);
                out.push(CallDef {
                    sym: mint_sym(file, ekind, &name, parent),
                    name,
                    kind,
                    file: file.to_string(),
                    line: child.start_position().row as u32 + 1,
                    end,
                });
                kt_walk_call_defs(child, src, file, None, &df_sym, out);
            }
            // Primary/secondary constructors: Method rows keyed to the class. df
            // does not lift ctor bodies (no sym to match), so the sym is the
            // JVM ctor name `<init>` (secondaries get a `@<row>` discriminator so
            // several stay distinct rows). `name` is the class name, so a
            // `Widget(x)` call site resolves here via the bare-name resolver.
            // @callable kotlin method
            "primary_constructor" | "secondary_constructor" => {
                if let Some(owner) = parent {
                    let pos = child.start_position();
                    let seg = if child.kind() == "primary_constructor" {
                        "<init>".to_string()
                    } else {
                        format!("<init>@{}", pos.row)
                    };
                    out.push(CallDef {
                        sym: mint_sym(file, EntityKind::Method, &seg, Some(owner)),
                        name: owner.to_string(),
                        kind: CallKind::Method,
                        file: file.to_string(),
                        line: pos.row as u32 + 1,
                        end: child.end_position().row as u32 + 1,
                    });
                }
                kt_walk_call_defs(child, src, file, parent, enclosing, out);
            }
            // `{ it + 1 }` inside a fn body: Lambda with the SAME
            // `lambda_sym(enclosing, "<row>_<col>")` `kotlin_dataflow_from` mints.
            // @callable kotlin lambda
            "lambda_literal" if !enclosing.is_empty() => {
                let pos = child.start_position();
                let sym = lambda_sym(enclosing, &format!("{}_{}", pos.row, pos.column));
                out.push(CallDef {
                    sym: sym.clone(),
                    name: String::new(),
                    kind: CallKind::Lambda,
                    file: file.to_string(),
                    line: pos.row as u32 + 1,
                    end: child.end_position().row as u32 + 1,
                });
                kt_walk_call_defs(child, src, file, parent, &sym, out);
            }
            _ => kt_walk_call_defs(child, src, file, parent, enclosing, out),
        }
    }
}

/// Walk for `call_expression` sites. The callee is the call's leading child: a
/// bare `simple_identifier`, or the trailing `simple_identifier` of a
/// `navigation_expression` (`recv.qux()` -> "qux"), matching the Rust trailing-
/// segment convention.
fn kt_walk_call_sites(node: tree_sitter::Node, src: &[u8], file: &str, out: &mut Vec<CallSite>) {
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        if child.kind() == "call_expression" {
            if let Some((callee, line)) = kt_callee(child, src) {
                out.push(CallSite { caller_sym: None, callee, callee_path: None, file: file.to_string(), line });
            }
        }
        kt_walk_call_sites(child, src, file, out);
    }
}

/// (callee name, 1-based call line) for a `call_expression`, or None when the
/// callee is not a plain/navigation name (e.g. an invoked lambda value).
fn kt_callee(call: tree_sitter::Node, src: &[u8]) -> Option<(String, u32)> {
    let mut cur = call.walk();
    let lead = call.children(&mut cur).find(|c| c.kind() != "call_suffix")?;
    let line = lead.start_position().row as u32 + 1;
    match lead.kind() {
        "simple_identifier" => Some((lead.utf8_text(src).unwrap_or("").to_string(), line)),
        "navigation_expression" => {
            let nav = kt_first_child(lead, "navigation_suffix")?;
            let id = kt_first_child(nav, "simple_identifier")?;
            Some((id.utf8_text(src).unwrap_or("").to_string(), line))
        }
        _ => None,
    }
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

fn walk_kotlin_entities(node: tree_sitter::Node, src: &[u8], file: &str, out: &mut Vec<TypeEntity>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        // `companion_object` is a distinct grammar node from `object_declaration`
        // (a top-level/nested `object Name { ... }`); both mint a `type_entity`
        // the same way a plain class does.
        if matches!(child.kind(), "class_declaration" | "object_declaration" | "companion_object") {
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

/// Doc-comment pass (tree-sitter): the KDoc `/** */` that immediately precedes a
/// class/object/function declaration is its previous sibling (annotations and
/// modifiers are children of the decl, so they don't sit between). Same sym as
/// `walk_kotlin_entities`. Tags via the shared JSDoc/KDoc splitter.
fn walk_kotlin_docs(node: tree_sitter::Node, src: &[u8], file: &str, out: &mut Vec<DocFact>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let named = if matches!(child.kind(), "class_declaration" | "object_declaration") {
            let mut c = child.walk();
            let kids: Vec<tree_sitter::Node> = child.children(&mut c).collect();
            kids.iter().find(|n| n.kind() == "type_identifier").map(|id| {
                let kind = if kids.iter().any(|n| n.kind() == "interface") {
                    EntityKind::Interface
                } else if kids.iter().any(|n| n.kind() == "enum") {
                    EntityKind::Enum
                } else {
                    EntityKind::Class
                };
                (id.utf8_text(src).unwrap_or("").to_string(), kind)
            })
        } else if child.kind() == "function_declaration" {
            let mut c = child.walk();
            let kids: Vec<tree_sitter::Node> = child.children(&mut c).collect();
            kids.iter().find(|n| n.kind() == "simple_identifier")
                .map(|id| (id.utf8_text(src).unwrap_or("").to_string(), EntityKind::Function))
        } else {
            None
        };
        if let Some((name, kind)) = named {
            if let Some(text) = kotlin_leading_kdoc(child, src) {
                out.push(DocFact {
                    sym: mint_sym(file, kind, &name, None),
                    line: (child.start_position().row + 1) as u32,
                    tags: parse_jsdoc_tags(&text),
                    text,
                });
            }
        }
        walk_kotlin_docs(child, src, file, out);
    }
}

/// The cleaned KDoc block directly above `node`, or None. A KDoc is a
/// `*comment*` previous sibling whose text opens with `/**`.
fn kotlin_leading_kdoc(node: tree_sitter::Node, src: &[u8]) -> Option<String> {
    let prev = node.prev_sibling()?;
    if !prev.kind().contains("comment") { return None; }
    let raw = prev.utf8_text(src).ok()?;
    if !raw.trim_start().starts_with("/**") { return None; }
    Some(clean_block_comment(raw))
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

// ── Go (tree-sitter) ─────────────────────────────────────────────────────────
//
// Same diet-extractor contract as Rust/Kotlin/TS, but tree-sitter-go's grammar
// exposes named FIELDS on every node that matters (`name`, `type`, `parameters`,
// `receiver`, `result`, ...), so this front end reads structured fields instead
// of Kotlin's manual child-kind scanning — closer in spirit to Rust's syn
// AST, just via tree-sitter. Method receivers carry their type in the syntax
// (`func (r *Repo) Name()`), so method -> owner parenting is deterministic; one
// package per directory means module resolution never needs symbol-level
// disambiguation (see `GoResolver` in modgraph.rs). Kind vocabulary: struct
// field types (named) -> `field`; an EMBEDDED struct/interface type (no field
// name, or interface `type_elem`) -> `impl`; a declared type parameter's
// constraint -> `generic`. NON-GOALS (syntactic tier, honest): implicit
// interface-satisfaction edges (method-set computation is a heuristic, not
// attempted), cgo, build-tag-conditional files, cross-module resolution
// outside the workspace.


#[cfg(test)]
mod tests {
    use super::*;

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
    fn kotlin_lift_ctor_named_args_and_members() {
        let src = "class Cfg(val host: Int, val port: Int)\n\
                   fun go(h: Int) {\n    \
                       val c = Cfg(host = h, port = 1)\n    \
                       val x = c.host\n    \
                       val n = c.count()\n    \
                       val u = go2(x)\n\
                   }\n";
        let df = KotlinTypes.extract_dataflow("f.kt", src);

        // capitalized callee = ctor call = `new` node with the type name.
        let cfg = dnode(&df, "new", "Cfg").id.clone();
        // named args land in df_field AND keep their source slot in df_arg.
        let h_read = df.nodes.iter().find(|n| n.kind == "var_read" && n.var == "h").unwrap().id.clone();
        assert!(has_field(&df, &cfg, "host", &h_read), "{:?}", df.fields);
        assert!(df.fields.iter().any(|(i, f, _)| i == &cfg && f == "port"), "{:?}", df.fields);
        assert!(has_arg(&df, &cfg, 0, &h_read), "{:?}", df.args);
        // the named-arg label is NOT a var_read (it's a label, not a value).
        assert!(
            !df.nodes.iter().any(|n| n.kind == "var_read" && n.var == "host"),
            "named-arg label leaked as a read: {:?}", df.nodes
        );
        // `.host` outside a call is a member read carrying the name.
        let member = dnode(&df, "member", "host");
        assert!(df.edges.iter().any(|e| e.to == member.id), "member has a base edge");
        // navigation callee: c.count() flows the receiver in at slot -1.
        assert!(
            df.args.iter().any(|(_, p, a)| *p == -1
                && df.nodes.iter().any(|n| &n.id == a && n.kind == "var_read" && n.var == "c")),
            "{:?}", df.args
        );
        // lowercase callee stays a call with slot-0 arg.
        let go2 = df.nodes.iter().filter(|n| n.kind == "call_res").count();
        assert!(go2 >= 1, "go2(x) should stay call_res: {:?}", df.nodes);
    }

    #[test]
    fn kotlin_trailing_lambda_lifts_with_implicit_it() {
        // trailing lambda with no parameter list: implicit `it` at slot 0.
        let src = "fun go(xs: List<Int>) {\n    val out = xs.map { it + 1 }\n}\n";
        let df = KotlinTypes.extract_dataflow("f.kt", src);
        assert_lambda_lifted(&df, 0, "it");
        // declared parameter form binds by name; trailing lambda still slots
        // after the parenthesized args (fold's accumulator lambda at slot 1).
        let src2 = "fun go(xs: List<Int>) {\n    val out = xs.fold(0) { acc, x -> acc + x }\n}\n";
        let df2 = KotlinTypes.extract_dataflow("f.kt", src2);
        let clo = df2.nodes.iter().find(|n| n.kind == "closure").expect("closure node");
        assert!(
            df2.args.iter().any(|(_, p, a)| *p == 1 && a == &clo.id),
            "trailing lambda after one paren arg sits at slot 1: {:?}", df2.args
        );
        let lam_sym = clo.var.clone();
        let pos_of = |v: &str| df2.nodes.iter()
            .find(|n| n.kind == "param" && n.var == v && n.fn_sym == lam_sym)
            .and_then(|n| df2.param_pos.iter().find(|(i, _)| i == &n.id).map(|(_, p)| *p));
        assert_eq!(pos_of("acc"), Some(0), "{:?}", df2.nodes);
        assert_eq!(pos_of("x"), Some(1), "{:?}", df2.nodes);
    }
}
