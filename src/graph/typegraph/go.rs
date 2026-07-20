//! Go extractor arm (tree-sitter-go front-end): TypeLang impl, type
//! edges, entities/docs, call defs/sites, dataflow. Pure code motion out
//! of the former single typegraph.rs; zero behavior change.

use std::collections::BTreeSet;

use super::*;

impl TypeLang for GoTypes {
    fn name(&self) -> &'static str { "go" }
    fn matches(&self, path: &str) -> bool { path.ends_with(".go") }
    // One tree-sitter parse feeds the entity, edge, and doc walks.
    fn extract(&self, file: &str, content: &str) -> TypeFacts {
        let Some(tree) = go_parse(content) else { return TypeFacts::default(); };
        let src = content.as_bytes();
        let root = tree.root_node();
        let owners = go_owner_kinds(root, src);
        let mut entities = Vec::new();
        walk_go_entities(root, src, file, &owners, &mut entities);
        let mut docs = Vec::new();
        walk_go_docs(root, src, file, &mut docs);
        TypeFacts { entities, edges: go_edges_from(root, src), docs, ..Default::default() }
    }
    fn extract_calls(&self, file: &str, content: &str) -> CallFacts {
        let Some(tree) = go_parse(content) else { return CallFacts::default(); };
        let src = content.as_bytes();
        let root = tree.root_node();
        let mut defs = Vec::new();
        go_walk_call_defs(root, src, file, "", &mut defs);
        let mut sites = Vec::new();
        go_walk_call_sites(root, src, file, &mut sites);
        CallFacts { defs, sites }
    }
    fn extract_dataflow(&self, file: &str, content: &str) -> DataflowFacts {
        let Some(tree) = go_parse(content) else { return DataflowFacts::default(); };
        go_dataflow_from(tree.root_node(), content.as_bytes(), file)
    }
}


fn go_parse(content: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    let lang = tree_sitter::Language::new(tree_sitter_go::LANGUAGE);
    parser.set_language(&lang).ok()?;
    parser.parse(content, None)
}

fn go_text<'a>(node: tree_sitter::Node, src: &'a [u8]) -> &'a str {
    node.utf8_text(src).unwrap_or("")
}

fn is_noise_go(name: &str) -> bool {
    matches!(
        name,
        "int" | "int8" | "int16" | "int32" | "int64"
            | "uint" | "uint8" | "uint16" | "uint32" | "uint64" | "uintptr"
            | "float32" | "float64" | "complex64" | "complex128"
            | "bool" | "string" | "byte" | "rune" | "error" | "any" | "comparable"
    )
}

/// Collect the named type references anywhere under `node`. A `qualified_type`
/// (`pkg.Type`) is one ref, kept as `pkg.Type` (the package qualifier stays —
/// unlike Kotlin's fully-dotted package path this is just the two segments
/// tree-sitter-go exposes) and NOT recursed into further (its own `name` field
/// is a `type_identifier` that would otherwise double-count). A bare
/// `type_identifier` is a ref unless it names a declared type parameter or a
/// predeclared/builtin type.
fn go_type_refs(node: tree_sitter::Node, src: &[u8], params: &BTreeSet<String>) -> Vec<String> {
    let mut out = Vec::new();
    collect_go_refs(node, src, params, &mut out);
    out.sort();
    out.dedup();
    out
}

fn collect_go_refs(node: tree_sitter::Node, src: &[u8], params: &BTreeSet<String>, out: &mut Vec<String>) {
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

/// The textual name of a composite literal's element type, for the `new`
/// dataflow node's `var`: a bare/qualified named type keeps its name; an
/// anonymous array/slice/map/struct literal type has no name (`""`).
fn go_type_name_text(node: tree_sitter::Node, src: &[u8]) -> String {
    match node.kind() {
        "type_identifier" => go_text(node, src).to_string(),
        "qualified_type" => node.child_by_field_name("name").map(|n| go_text(n, src).to_string()).unwrap_or_default(),
        "generic_type" => node.child_by_field_name("type").map(|t| go_type_name_text(t, src)).unwrap_or_default(),
        _ => String::new(),
    }
}

/// A method's receiver base type name, `*`/generic-args stripped
/// (`(r *Repo[T])` -> `"Repo"`). None for a malformed/absent receiver.
fn go_receiver_type(method: tree_sitter::Node, src: &[u8]) -> Option<String> {
    let recv_list = method.child_by_field_name("receiver")?;
    let mut cursor = recv_list.walk();
    let param = recv_list.children(&mut cursor).find(|n| n.kind() == "parameter_declaration")?;
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

/// First-pass file-local owner-kind lookup (mirrors Rust's `rust_owner_kinds`):
/// for each package-level `type X struct{}`/`interface{}` declared in THIS
/// file, record its real `EntityKind` so a same-file method's receiver mints
/// the correctly-kinded parent sym. A method whose receiver type is declared
/// in a DIFFERENT file (common — Go methods are routinely split across files
/// in one package) defaults to `Struct`; the engine's cross-file owner-name
/// resolution (same as Rust) still finds the real declaring sym, kind-agnostic.
fn go_owner_kinds(root: tree_sitter::Node, src: &[u8]) -> std::collections::HashMap<String, EntityKind> {
    let mut out = std::collections::HashMap::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() != "type_declaration" { continue; }
        let mut c2 = child.walk();
        for spec in child.children(&mut c2) {
            if spec.kind() != "type_spec" { continue; }
            let Some(name) = spec.child_by_field_name("name") else { continue };
            let kind = match spec.child_by_field_name("type").map(|t| t.kind()) {
                Some("interface_type") => EntityKind::Interface,
                Some("struct_type") => EntityKind::Struct,
                _ => EntityKind::Alias,
            };
            out.insert(go_text(name, src).to_string(), kind);
        }
    }
    out
}

// --- Go entity pass: struct/interface/alias type declarations, functions, and
// methods (parent = receiver base type, real-kinded via `go_owner_kinds`). ---

fn walk_go_entities(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    owners: &std::collections::HashMap<String, EntityKind>,
    out: &mut Vec<TypeEntity>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "type_declaration" => {
                let mut c2 = child.walk();
                for spec in child.children(&mut c2) {
                    let (name_node, kind) = match spec.kind() {
                        "type_spec" => {
                            let k = match spec.child_by_field_name("type").map(|t| t.kind()) {
                                Some("struct_type") => EntityKind::Struct,
                                Some("interface_type") => EntityKind::Interface,
                                _ => EntityKind::Alias,
                            };
                            (spec.child_by_field_name("name"), k)
                        }
                        "type_alias" => (spec.child_by_field_name("name"), EntityKind::Alias),
                        _ => continue,
                    };
                    let Some(name_node) = name_node else { continue };
                    let name = go_text(name_node, src).to_string();
                    out.push(TypeEntity {
                        sym: mint_sym(file, kind, &name, None),
                        name,
                        kind,
                        parent: None,
                        file: file.to_string(),
                        line: (spec.start_position().row + 1) as u32,
                        ty: None,
                    });
                }
            }
            "function_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = go_text(name_node, src).to_string();
                    out.push(TypeEntity {
                        sym: mint_sym(file, EntityKind::Function, &name, None),
                        name,
                        kind: EntityKind::Function,
                        parent: None,
                        file: file.to_string(),
                        line: (child.start_position().row + 1) as u32,
                        ty: Some(go_fn_type(child, src)),
                    });
                }
            }
            "method_declaration" => {
                if let (Some(name_node), Some(owner_name)) =
                    (child.child_by_field_name("name"), go_receiver_type(child, src))
                {
                    let name = go_text(name_node, src).to_string();
                    let owner_kind = owners.get(&owner_name).copied().unwrap_or(EntityKind::Struct);
                    out.push(TypeEntity {
                        sym: mint_sym(file, EntityKind::Method, &name, Some(&owner_name)),
                        name,
                        kind: EntityKind::Method,
                        parent: Some(mint_sym(file, owner_kind, &owner_name, None)),
                        file: file.to_string(),
                        line: (child.start_position().row + 1) as u32,
                        ty: Some(go_fn_type(child, src)),
                    });
                }
            }
            _ => {}
        }
        walk_go_entities(child, src, file, owners, out);
    }
}

/// Build the arrow `[...A] => B` for a `function_declaration`/`method_declaration`.
/// The receiver (methods only) is never read here, so params stay aligned with
/// the written argument list — same convention as Rust dropping `self`. A
/// grouped parameter (`a, b int`) is ONE grammar node but TWO positional
/// params, so each declared name gets its own slot sharing the group's type.
/// Go's multi-value return has no per-slot structure in `type_sig` (which
/// stores one flat `ret` list at position 0 regardless of language, see
/// `refresh_type_rels`): every result type's refs are unioned into that one
/// list rather than kept per-slot. A caller wanting per-return precision reads
/// `df_arg`/the dataflow `ret` nodes, not `type_sig`, for a multi-return fn.
fn go_fn_type(node: tree_sitter::Node, src: &[u8]) -> TypeExpr {
    let named = |refs: Vec<String>| refs.into_iter().map(TypeRef::Named).collect::<Vec<_>>();
    let mut tparams: BTreeSet<String> = BTreeSet::new();
    if let Some(tp_list) = node.child_by_field_name("type_parameters") {
        let mut cursor = tp_list.walk();
        for tp in tp_list.children(&mut cursor).filter(|n| n.kind() == "type_parameter_declaration") {
            let mut cc = tp.walk();
            for n in tp.children(&mut cc) {
                if n.kind() == "identifier" {
                    tparams.insert(go_text(n, src).to_string());
                }
            }
        }
    }
    let mut params = Vec::new();
    if let Some(plist) = node.child_by_field_name("parameters") {
        let mut cursor = plist.walk();
        for p in plist.children(&mut cursor) {
            if !matches!(p.kind(), "parameter_declaration" | "variadic_parameter_declaration") { continue; }
            let Some(ty) = p.child_by_field_name("type") else { continue };
            let mut nc = p.walk();
            let count = p.children(&mut nc).filter(|n| n.kind() == "identifier").count().max(1);
            for _ in 0..count {
                params.push(named(go_type_refs(ty, src, &tparams)));
            }
        }
    }
    let mut ret = Vec::new();
    if let Some(result) = node.child_by_field_name("result") {
        if result.kind() == "parameter_list" {
            let mut cursor = result.walk();
            for p in result.children(&mut cursor)
                .filter(|n| matches!(n.kind(), "parameter_declaration" | "variadic_parameter_declaration"))
            {
                if let Some(ty) = p.child_by_field_name("type") {
                    ret.extend(named(go_type_refs(ty, src, &tparams)));
                }
            }
        } else {
            ret.extend(named(go_type_refs(result, src, &tparams)));
        }
    }
    TypeExpr { params, ret }
}

// --- Go type-graph edges: struct fields (named -> `field`, embedded ->
// `impl`), interface embeds (`impl`), declared type-parameter constraints
// (`generic`). Method signatures are NOT edge sources (entity-level
// `type_sig` covers callables; type_edge is shape-only, matching Kotlin/TS). ---

fn go_edges_from(root: tree_sitter::Node, src: &[u8]) -> Vec<TypeEdge> {
    let mut out: BTreeSet<(String, String, &'static str)> = BTreeSet::new();
    walk_go_types(root, src, &mut out);
    out.into_iter().map(|(from, to, kind)| TypeEdge { from, to, kind }).collect()
}

fn walk_go_types(node: tree_sitter::Node, src: &[u8], out: &mut BTreeSet<(String, String, &'static str)>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "type_declaration" {
            let mut c2 = child.walk();
            for spec in child.children(&mut c2) {
                if spec.kind() == "type_spec" {
                    go_type_spec_edges(spec, src, out);
                }
            }
        }
        walk_go_types(child, src, out);
    }
}

fn go_type_spec_edges(spec: tree_sitter::Node, src: &[u8], out: &mut BTreeSet<(String, String, &'static str)>) {
    let Some(name_node) = spec.child_by_field_name("name") else { return };
    let owner = go_text(name_node, src).to_string();

    let mut params: BTreeSet<String> = BTreeSet::new();
    if let Some(tp_list) = spec.child_by_field_name("type_parameters") {
        let mut cursor = tp_list.walk();
        for tp in tp_list.children(&mut cursor).filter(|n| n.kind() == "type_parameter_declaration") {
            let mut cc = tp.walk();
            let kids: Vec<tree_sitter::Node> = tp.children(&mut cc).collect();
            for n in kids.iter().filter(|n| n.kind() == "identifier") {
                params.insert(go_text(*n, src).to_string());
            }
            if let Some(constraint) = tp.child_by_field_name("type") {
                for to in go_type_refs(constraint, src, &params) {
                    out.insert((owner.clone(), to, "generic"));
                }
            }
        }
    }

    let Some(ty) = spec.child_by_field_name("type") else { return };
    match ty.kind() {
        "struct_type" => {
            let mut c = ty.walk();
            let Some(list) = ty.children(&mut c).find(|n| n.kind() == "field_declaration_list") else { return };
            let mut c2 = list.walk();
            for field in list.children(&mut c2).filter(|n| n.kind() == "field_declaration") {
                let Some(ftype) = field.child_by_field_name("type") else { continue };
                let kind: &'static str = if field.child_by_field_name("name").is_some() { "field" } else { "impl" };
                for to in go_type_refs(ftype, src, &params) {
                    out.insert((owner.clone(), to, kind));
                }
            }
        }
        "interface_type" => {
            let mut c = ty.walk();
            for elem in ty.children(&mut c).filter(|n| n.kind() == "type_elem") {
                for to in go_type_refs(elem, src, &params) {
                    out.insert((owner.clone(), to, "impl"));
                }
            }
            // `method_elem` (interface method signatures) intentionally
            // skipped: no type_sig-equivalent exists for an interface's own
            // method specs at the type_edge level.
        }
        _ => {}
    }
}

// --- Go call-graph pass: `function_declaration`/`method_declaration` become
// CallDefs (method keyed to its receiver's base type, matching the entity
// pass); every `call_expression` becomes a CallSite whose callee is the bare
// name (a selector callee's trailing field name, matching the Rust/Kotlin
// trailing-segment convention). ---

fn go_walk_call_defs(node: tree_sitter::Node, src: &[u8], file: &str, enclosing: &str, out: &mut Vec<CallDef>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let end_of = |c: tree_sitter::Node| c.child_by_field_name("body").unwrap_or(c).end_position().row as u32 + 1;
        match child.kind() {
            // @callable go function
            "function_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = go_text(name_node, src).to_string();
                    let sym = mint_sym(file, EntityKind::Function, &name, None);
                    out.push(CallDef {
                        sym: sym.clone(), name, kind: CallKind::Free, file: file.to_string(),
                        line: child.start_position().row as u32 + 1, end: end_of(child),
                    });
                    go_walk_call_defs(child, src, file, &sym, out);
                    continue;
                }
            }
            // @callable go method
            "method_declaration" => {
                if let (Some(name_node), Some(owner)) =
                    (child.child_by_field_name("name"), go_receiver_type(child, src))
                {
                    let name = go_text(name_node, src).to_string();
                    let sym = mint_sym(file, EntityKind::Method, &name, Some(&owner));
                    out.push(CallDef {
                        sym: sym.clone(), name, kind: CallKind::Method, file: file.to_string(),
                        line: child.start_position().row as u32 + 1, end: end_of(child),
                    });
                    go_walk_call_defs(child, src, file, &sym, out);
                    continue;
                }
            }
            // `func(...) {...}` inside a fn/method body: a Lambda whose sym is the
            // SAME `lambda_sym(enclosing, "<row>_<col>")` `go_dataflow_from` mints
            // (0-based tree-sitter coords), so df<->call joins line up. A
            // package-level `var f = func(){}` (enclosing == "") is skipped — the
            // df lift only walks fn/method bodies, so there is no scope to join.
            // @callable go lambda
            "func_literal" if !enclosing.is_empty() => {
                let pos = child.start_position();
                let sym = lambda_sym(enclosing, &format!("{}_{}", pos.row, pos.column));
                out.push(CallDef {
                    sym: sym.clone(), name: String::new(), kind: CallKind::Lambda, file: file.to_string(),
                    line: pos.row as u32 + 1, end: end_of(child),
                });
                go_walk_call_defs(child, src, file, &sym, out);
                continue;
            }
            _ => {}
        }
        go_walk_call_defs(child, src, file, enclosing, out);
    }
}

fn go_walk_call_sites(node: tree_sitter::Node, src: &[u8], file: &str, out: &mut Vec<CallSite>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "call_expression" {
            if let Some((callee, line)) = go_callee(child, src) {
                out.push(CallSite { caller_sym: None, callee, callee_path: None, file: file.to_string(), line });
            }
        }
        go_walk_call_sites(child, src, file, out);
    }
}

/// (callee name, 1-based call line) for a `call_expression`, or None when the
/// callee is not a plain/selector name (a type conversion `T(x)` where `T` is
/// a `type_identifier` callee is NOT skipped here — it reads as an ordinary
/// call, honest: the syntactic tier can't tell a conversion from a call).
fn go_callee(call: tree_sitter::Node, src: &[u8]) -> Option<(String, u32)> {
    let func = call.child_by_field_name("function")?;
    let line = func.start_position().row as u32 + 1;
    match func.kind() {
        "identifier" => Some((go_text(func, src).to_string(), line)),
        "selector_expression" => {
            let field = func.child_by_field_name("field")?;
            Some((go_text(field, src).to_string(), line))
        }
        _ => None,
    }
}

// --- Go doc-comment pass: the contiguous run of `//` line comments (or a
// single leading `/* */` block) immediately above a decl, godoc convention.
// Tags: only "Deprecated:" — plain godoc has no `@`-style annotations. ---

/// The cleaned doc block directly above `node`, or None. Walks BACKWARD over
/// `prev_sibling`s while each one is a `comment` node whose last line ends
/// exactly one row before the block collected so far starts (no blank-line
/// gap) — so a multi-line `// foo\n// bar` godoc block joins into one text,
/// and a comment separated by a blank line (not a doc comment, by convention)
/// is left alone.
fn go_leading_doc(node: tree_sitter::Node, src: &[u8]) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut expected_row = node.start_position().row;
    let mut cur = node.prev_sibling()?;
    loop {
        if cur.kind() != "comment" || cur.end_position().row + 1 != expected_row {
            break;
        }
        let raw = go_text(cur, src);
        if raw.trim_start().starts_with("/*") {
            lines.insert(0, clean_block_comment(raw));
            break;
        }
        let body = raw.trim_start().strip_prefix("//").unwrap_or(raw).trim_start().to_string();
        lines.insert(0, body);
        expected_row = cur.start_position().row;
        let Some(prev) = cur.prev_sibling() else { break };
        cur = prev;
    }
    if lines.is_empty() { None } else { Some(lines.join("\n")) }
}

/// godoc's one structured convention: a paragraph (blank-line-separated block)
/// starting `Deprecated:` marks the decl deprecated. No `@`-tags exist in
/// plain godoc, so this is the only tag this extractor ever emits.
fn parse_go_doc_tags(text: &str) -> Vec<DocTag> {
    let mut out = Vec::new();
    for para in text.split("\n\n") {
        if let Some(rest) = para.trim_start().strip_prefix("Deprecated:") {
            out.push(DocTag { tag: "deprecated".to_string(), arg: String::new(), text: rest.trim().to_string() });
        }
    }
    out
}

fn push_go_doc(out: &mut Vec<DocFact>, file: &str, name: &str, kind: EntityKind, line: u32, text: String) {
    out.push(DocFact { sym: mint_sym(file, kind, name, None), line, tags: parse_go_doc_tags(&text), text });
}

fn walk_go_docs(node: tree_sitter::Node, src: &[u8], file: &str, out: &mut Vec<DocFact>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "type_declaration" => {
                let mut c2 = child.walk();
                for spec in child.children(&mut c2) {
                    let (name_node, kind) = match spec.kind() {
                        "type_spec" => {
                            let k = match spec.child_by_field_name("type").map(|t| t.kind()) {
                                Some("struct_type") => EntityKind::Struct,
                                Some("interface_type") => EntityKind::Interface,
                                _ => EntityKind::Alias,
                            };
                            (spec.child_by_field_name("name"), k)
                        }
                        "type_alias" => (spec.child_by_field_name("name"), EntityKind::Alias),
                        _ => continue,
                    };
                    let Some(name_node) = name_node else { continue };
                    // Try the spec itself first (a grouped `type ( ... )` decl
                    // has its doc comment directly above the spec); a lone
                    // `type X struct{}` decl's comment sits above the whole
                    // `type_declaration` instead, so fall back to the parent.
                    let text = go_leading_doc(spec, src).or_else(|| go_leading_doc(child, src));
                    if let Some(text) = text {
                        push_go_doc(out, file, &go_text(name_node, src).to_string(), kind,
                                    spec.start_position().row as u32 + 1, text);
                    }
                }
            }
            "function_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    if let Some(text) = go_leading_doc(child, src) {
                        push_go_doc(out, file, &go_text(name_node, src).to_string(), EntityKind::Function,
                                    child.start_position().row as u32 + 1, text);
                    }
                }
            }
            "method_declaration" => {
                if let (Some(name_node), Some(owner)) =
                    (child.child_by_field_name("name"), go_receiver_type(child, src))
                {
                    if let Some(text) = go_leading_doc(child, src) {
                        let sym = mint_sym(file, EntityKind::Method, go_text(name_node, src), Some(&owner));
                        out.push(DocFact {
                            sym,
                            line: child.start_position().row as u32 + 1,
                            tags: parse_go_doc_tags(&text),
                            text,
                        });
                    }
                }
            }
            _ => {}
        }
        walk_go_docs(child, src, file, out);
    }
}

// --- Go intra-procedural dataflow lift (tree-sitter, fields). Same lift-to-
// node model as Rust/Kotlin: value-bearing subtrees mint a `DfNode`, local
// value flow becomes `DfEdge`. Unlike Rust/Kotlin, Go has no implicit tail
// return (every return is a `return_statement`), so there is no fn-level
// "wrap the tail in a ret node" step — each `return_statement` mints its own
// `ret` node(s) directly, one per returned value (multi-value return). ---

fn go_dataflow_from(root: tree_sitter::Node, src: &[u8], file: &str) -> DataflowFacts {
    let mut out = DataflowFacts::default();
    go_walk_fns(root, src, file, &mut out);
    // tree-sitter rows are 0-based; the df contract is 1-based (see Kotlin's
    // identical bump), so bump reported node lines and loop spans. Node ids
    // keep the raw 0-based row (opaque; only uniqueness matters).
    // tree-sitter rows are 0-based -> 1-based; `bump_node_lines_1based` also
    // rebuilds each node id so it reconstructs from the stored columns (the
    // coordinate de-intern contract). Loops bump first; nests recompute after.
    for l in &mut out.loops { l.start += 1; l.end += 1; }
    bump_node_lines_1based(&mut out);
    out.nests = compute_nests(&out.nodes, &out.loops);
    out
}

fn go_walk_fns(node: tree_sitter::Node, src: &[u8], file: &str, out: &mut DataflowFacts) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let fn_sym = mint_sym(file, EntityKind::Function, go_text(name_node, src), None);
                    go_flow_fn(child, src, file, &fn_sym, out);
                }
            }
            "method_declaration" => {
                if let (Some(name_node), Some(owner)) =
                    (child.child_by_field_name("name"), go_receiver_type(child, src))
                {
                    let fn_sym = mint_sym(file, EntityKind::Method, go_text(name_node, src), Some(&owner));
                    go_flow_fn(child, src, file, &fn_sym, out);
                }
            }
            _ => {}
        }
        go_walk_fns(child, src, file, out);
    }
}

/// Seed `param` nodes from the (non-receiver) parameter list, then walk the
/// body. A grouped parameter (`a, b int`) mints one param node PER declared
/// name, matching `go_fn_type`'s slot count; an unnamed parameter still
/// advances the position counter so later named params keep the right index.
fn go_flow_fn(fn_node: tree_sitter::Node, src: &[u8], file: &str, fn_sym: &str, out: &mut DataflowFacts) {
    let mut scope: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut pos: u32 = 0;
    if let Some(params) = fn_node.child_by_field_name("parameters") {
        let mut cursor = params.walk();
        for p in params.children(&mut cursor) {
            if !matches!(p.kind(), "parameter_declaration" | "variadic_parameter_declaration") { continue; }
            let mut nc = p.walk();
            let names: Vec<tree_sitter::Node> = p.children(&mut nc).filter(|n| n.kind() == "identifier").collect();
            if names.is_empty() { pos += 1; continue; }
            for name_node in names {
                let sp = name_node.start_position();
                let v = go_text(name_node, src).to_string();
                let id = push_node(out, file, sp.row as u32, sp.column as u32, "param", &v, fn_sym);
                out.param_pos.push((id.clone(), pos));
                scope.insert(v, id);
                pos += 1;
            }
        }
    }
    if let Some(body) = fn_node.child_by_field_name("body") {
        flow_go(body, src, file, fn_sym, &mut scope, out);
    }
}

/// Returns the node id carrying the value of this subtree, or None when the
/// subtree is a pure statement/binder handled inline (bindings, control-flow
/// headers). Unhandled node kinds fall through to `go_recurse_children`,
/// conservative — may miss a flow, never invents one.
fn flow_go(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) -> Option<String> {
    let pos = node.start_position();
    match node.kind() {
        "identifier" => {
            let v = go_text(node, src).to_string();
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "var_read", &v, fn_sym);
            if let Some(b) = scope.get(&v) {
                out.edges.push(DfEdge { from: b.clone(), to: id.clone() });
            }
            Some(id)
        }
        "interpreted_string_literal" | "raw_string_literal" | "int_literal" | "float_literal"
        | "imaginary_literal" | "rune_literal" | "true" | "false" | "nil" | "iota" => {
            Some(push_node(out, file, pos.row as u32, pos.column as u32, "lit", "", fn_sym))
        }
        // f(args): every argument flows into the call result; `df_arg` records
        // its 0-based slot. A selector callee `recv.M(args)` flows the
        // receiver in at slot -1 (mirroring the skipped receiver in
        // `df_param`), the bare method name carried on the node text-side by
        // `call_node`'s (file, line) join, not here. Go has no syntactic ctor
        // marker (capitalization means EXPORTED, not "constructor"), so every
        // call is `call_res`; instantiation rides `composite_literal` below.
        "call_expression" => {
            let func = node.child_by_field_name("function");
            let mut recv: Option<String> = None;
            if let Some(func) = func {
                if func.kind() == "selector_expression" {
                    if let Some(operand) = func.child_by_field_name("operand") {
                        recv = flow_go(operand, src, file, fn_sym, scope, out);
                    }
                }
            }
            let mut arg_ids = Vec::new();
            if let Some(args) = node.child_by_field_name("arguments") {
                let mut cursor = args.walk();
                for a in args.children(&mut cursor) {
                    if let Some(id) = flow_go(a, src, file, fn_sym, scope, out) {
                        arg_ids.push(id);
                    }
                }
            }
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "call_res", "", fn_sym);
            if let Some(r) = recv {
                out.edges.push(DfEdge { from: r.clone(), to: id.clone() });
                out.args.push((id.clone(), -1, r));
            }
            for (p, vid) in arg_ids.into_iter().enumerate() {
                out.edges.push(DfEdge { from: vid.clone(), to: id.clone() });
                out.args.push((id.clone(), p as i64, vid));
            }
            Some(id)
        }
        // `base.Field` outside a call: a member read. As a call's callee
        // (parent is the enclosing call_expression) the call arm above owns
        // it instead — receiver at slot -1, bare name on the call node.
        "selector_expression" => {
            if node.parent().map(|p| p.kind()) == Some("call_expression") {
                return None;
            }
            let operand = node.child_by_field_name("operand")
                .and_then(|o| flow_go(o, src, file, fn_sym, scope, out));
            let name = node.child_by_field_name("field").map(|n| go_text(n, src).to_string()).unwrap_or_default();
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "member", &name, fn_sym);
            if let Some(o) = operand {
                out.edges.push(DfEdge { from: o, to: id.clone() });
            }
            Some(id)
        }
        // `T{...}` / `[]T{...}` / `map[K]V{...}`: an instantiation. Each
        // element flows into the `new` node and `df_field` records which
        // field it fills (a keyed struct field's name, else the 0-based
        // positional index as a string — array/slice/map literals have no
        // field name). The key subtree of a `keyed_element` is a LABEL, never
        // walked as a read (mirrors Kotlin's named-argument convention) —
        // even though a map literal's key COULD be a real expression, the
        // syntactic tier can't tell a struct field label from a map key
        // without type info, so it is read as text only, conservative.
        "composite_literal" => {
            let type_name = node.child_by_field_name("type").map(|t| go_type_name_text(t, src)).unwrap_or_default();
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "new", &type_name, fn_sym);
            if let Some(body) = node.child_by_field_name("body") {
                go_flow_literal_fields(body, src, file, fn_sym, scope, out, &id);
            }
            Some(id)
        }
        // A `literal_value` reached directly (not via `composite_literal`):
        // a nested element literal whose type is implied by the enclosing
        // composite (`[]Foo{ {A: 1} }`'s inner `{A: 1}`). Anonymous `new`.
        "literal_value" => {
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "new", "", fn_sym);
            go_flow_literal_fields(node, src, file, fn_sym, scope, out, &id);
            Some(id)
        }
        "binary_expression" => {
            let l = node.child_by_field_name("left").and_then(|n| flow_go(n, src, file, fn_sym, scope, out));
            let r = node.child_by_field_name("right").and_then(|n| flow_go(n, src, file, fn_sym, scope, out));
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "binop", "", fn_sym);
            if let Some(l) = l { out.edges.push(DfEdge { from: l, to: id.clone() }); }
            if let Some(r) = r { out.edges.push(DfEdge { from: r, to: id.clone() }); }
            Some(id)
        }
        "unary_expression" => {
            let inner = node.child_by_field_name("operand").and_then(|n| flow_go(n, src, file, fn_sym, scope, out));
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "unop", "", fn_sym);
            if let Some(inner) = inner { out.edges.push(DfEdge { from: inner, to: id.clone() }); }
            Some(id)
        }
        // `x := rhs` (possibly multi-value): bind each declared name to a
        // fresh `let_bind` node. A matching-arity rhs pairs positionally; a
        // mismatched arity (`a, b := f()` — one call, two targets) taints
        // every target from that one rhs value conservatively.
        "short_var_declaration" => {
            let rhs_ids = node.child_by_field_name("right")
                .map(|right| go_flow_expr_list(right, src, file, fn_sym, scope, out))
                .unwrap_or_default();
            if let Some(left) = node.child_by_field_name("left") {
                let mut cursor = left.walk();
                let names: Vec<tree_sitter::Node> = left.children(&mut cursor).filter(|n| n.kind() == "identifier").collect();
                go_bind(&names, &rhs_ids, "let_bind", src, file, fn_sym, scope, out);
            }
            None
        }
        "var_declaration" | "const_declaration" => {
            let mut cursor = node.walk();
            for spec in node.children(&mut cursor).filter(|n| matches!(n.kind(), "var_spec" | "const_spec")) {
                go_flow_spec(spec, src, file, fn_sym, scope, out);
            }
            None
        }
        // `lhs = rhs` (incl. compound `+=`/etc, treated the same — a write
        // either way): rebind so later reads see the new value. Non-identifier
        // targets (`x.Field = v`, `arr[i] = v`) still flow for side-effect
        // visibility without a scope rebind.
        "assignment_statement" => {
            let rhs_ids = node.child_by_field_name("right")
                .map(|right| go_flow_expr_list(right, src, file, fn_sym, scope, out))
                .unwrap_or_default();
            if let Some(left) = node.child_by_field_name("left") {
                let mut cursor = left.walk();
                let targets: Vec<tree_sitter::Node> = left.children(&mut cursor).collect();
                let names: Vec<tree_sitter::Node> = targets.iter().filter(|n| n.kind() == "identifier").copied().collect();
                go_bind(&names, &rhs_ids, "var_write", src, file, fn_sym, scope, out);
                for t in targets.iter().filter(|n| n.kind() != "identifier" && n.kind() != ",") {
                    flow_go(*t, src, file, fn_sym, scope, out);
                }
            }
            None
        }
        // `return a, b`: one `ret` node PER returned value (multi-value
        // return), each fed by its own expression — the sink the
        // interprocedural backward hop reads. A naked `return` still mints
        // one empty `ret` node so the fn has a visible graph endpoint.
        "return_statement" => {
            let mut cursor = node.walk();
            let list = node.children(&mut cursor).find(|n| n.kind() == "expression_list");
            let mut minted = false;
            if let Some(list) = list {
                let mut c2 = list.walk();
                for e in list.children(&mut c2) {
                    if let Some(vid) = flow_go(e, src, file, fn_sym, scope, out) {
                        let rp = e.start_position();
                        let ret = push_node(out, file, rp.row as u32, rp.column as u32, "ret", "", fn_sym);
                        out.edges.push(DfEdge { from: vid, to: ret });
                        minted = true;
                    }
                }
            }
            if !minted {
                push_node(out, file, pos.row as u32, pos.column as u32, "ret", "", fn_sym);
            }
            None
        }
        "if_statement" => {
            if let Some(init) = node.child_by_field_name("initializer") {
                flow_go(init, src, file, fn_sym, scope, out);
            }
            if let Some(cond) = node.child_by_field_name("condition") {
                flow_go(cond, src, file, fn_sym, scope, out);
            }
            if let Some(cons) = node.child_by_field_name("consequence") {
                flow_go(cons, src, file, fn_sym, scope, out);
            }
            if let Some(alt) = node.child_by_field_name("alternative") {
                flow_go(alt, src, file, fn_sym, scope, out);
            }
            Some(push_node(out, file, pos.row as u32, pos.column as u32, "if", "", fn_sym))
        }
        // `for range/clause/cond { body }`: record the loop span (+ the range
        // variable, when present) so `loop_over`/`nest` see loop-invariant
        // calls inside it, then walk the body. A for_statement's non-`body`,
        // non-`for`-keyword child is at most ONE of {bare condition
        // expression, `for_clause`, `range_clause`} per the grammar.
        "for_statement" => {
            let mut lvar = String::new();
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "range_clause" => {
                        if let Some(right) = child.child_by_field_name("right") {
                            flow_go(right, src, file, fn_sym, scope, out);
                        }
                        if let Some(left) = child.child_by_field_name("left") {
                            let mut lc = left.walk();
                            let names: Vec<tree_sitter::Node> =
                                left.children(&mut lc).filter(|n| n.kind() == "identifier").collect();
                            for name_node in &names {
                                let v = go_text(*name_node, src).to_string();
                                if v == "_" { continue; }
                                let sp = name_node.start_position();
                                let id = push_node(out, file, sp.row as u32, sp.column as u32, "let_bind", &v, fn_sym);
                                scope.insert(v.clone(), id);
                                if lvar.is_empty() { lvar = v; }
                            }
                        }
                    }
                    "for_clause" => {
                        if let Some(init) = child.child_by_field_name("initializer") {
                            flow_go(init, src, file, fn_sym, scope, out);
                        }
                        if let Some(cond) = child.child_by_field_name("condition") {
                            flow_go(cond, src, file, fn_sym, scope, out);
                        }
                        if let Some(upd) = child.child_by_field_name("update") {
                            flow_go(upd, src, file, fn_sym, scope, out);
                        }
                    }
                    "block" | "for" => {}
                    _ => { flow_go(child, src, file, fn_sym, scope, out); }
                }
            }
            let end = node.end_position();
            out.loops.push(LoopFact {
                file: file.into(), start: pos.row as u32, end: end.row as u32,
                var: lvar.clone(), collection: String::new(), fn_sym: fn_sym.into(),
            });
            if let Some(body) = node.child_by_field_name("body") {
                flow_go(body, src, file, fn_sym, scope, out);
            }
            Some(push_node(out, file, pos.row as u32, pos.column as u32, "loop", &lvar, fn_sym))
        }
        // `func(...) {...}`: lift as its OWN fn scope, same shape as Rust
        // closures/Kotlin lambda literals — `param` nodes with `df_param`
        // slots, body walked under the lifted sym. The enclosing `scope` is
        // shared, so a captured outer variable's read still resolves. The
        // `closure` VALUE node stays in the enclosing fn (it's the argument a
        // `df_arg` row records when the literal is passed straight to a call,
        // e.g. `go func(){ ... }()`/`defer func(){ ... }()`).
        "func_literal" => {
            let lam_sym = lambda_sym(fn_sym, &format!("{}_{}", pos.row, pos.column));
            let mut lpos: u32 = 0;
            if let Some(params) = node.child_by_field_name("parameters") {
                let mut cursor = params.walk();
                for p in params.children(&mut cursor) {
                    if !matches!(p.kind(), "parameter_declaration" | "variadic_parameter_declaration") { continue; }
                    let mut nc = p.walk();
                    let names: Vec<tree_sitter::Node> = p.children(&mut nc).filter(|n| n.kind() == "identifier").collect();
                    if names.is_empty() { lpos += 1; continue; }
                    for name_node in names {
                        let sp = name_node.start_position();
                        let v = go_text(name_node, src).to_string();
                        let id = push_node(out, file, sp.row as u32, sp.column as u32, "param", &v, &lam_sym);
                        out.param_pos.push((id.clone(), lpos));
                        scope.insert(v, id);
                        lpos += 1;
                    }
                }
            }
            if let Some(body) = node.child_by_field_name("body") {
                flow_go(body, src, file, &lam_sym, scope, out);
            }
            Some(push_node(out, file, pos.row as u32, pos.column as u32, "closure", &lam_sym, fn_sym))
        }
        // everything else (blocks/statement lists, expression statements,
        // parenthesized/index/slice/type-assertion/conversion expressions,
        // go/defer/send/select/switch/labeled statements, ...): recurse
        // conservatively, surfacing the last value-bearing child.
        _ => go_recurse_children(node, src, file, fn_sym, scope, out),
    }
}

/// Flow every element of an `expression_list`, in source order, returning one
/// `Option<String>` per element (mismatched-arity binds use this alongside a
/// binding target list of a different length).
fn go_flow_expr_list(
    list: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) -> Vec<Option<String>> {
    let mut cursor = list.walk();
    list.children(&mut cursor).map(|e| flow_go(e, src, file, fn_sym, scope, out)).collect()
}

/// Bind each name in `names` to a fresh node of `kind` ("let_bind" for a
/// declaration, "var_write" for a plain assignment), wiring the matching rhs
/// value when arity lines up (else every target derives from the first rhs
/// value, conservative). `_` binds nothing.
fn go_bind(
    names: &[tree_sitter::Node],
    rhs_ids: &[Option<String>],
    kind: &str,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) {
    for (i, name_node) in names.iter().enumerate() {
        let v = go_text(*name_node, src).to_string();
        if v == "_" { continue; }
        let sp = name_node.start_position();
        let id = push_node(out, file, sp.row as u32, sp.column as u32, kind, &v, fn_sym);
        let rhs = if rhs_ids.len() == names.len() { rhs_ids.get(i).cloned().flatten() } else { rhs_ids.first().cloned().flatten() };
        if let Some(rhs) = rhs {
            out.edges.push(DfEdge { from: rhs, to: id.clone() });
        }
        scope.insert(v, id);
    }
}

fn go_flow_spec(
    spec: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) {
    let mut cursor = spec.walk();
    let names: Vec<tree_sitter::Node> = spec.children(&mut cursor).filter(|n| n.kind() == "identifier").collect();
    let rhs_ids = spec.child_by_field_name("value")
        .map(|value| go_flow_expr_list(value, src, file, fn_sym, scope, out))
        .unwrap_or_default();
    go_bind(&names, &rhs_ids, "let_bind", src, file, fn_sym, scope, out);
}

/// A composite literal's body (`literal_value`): each `keyed_element`'s value
/// (and each bare `literal_element`'s value, keyed by its 0-based position)
/// flows into `owner_id` and records a `df_field` row.
fn go_flow_literal_fields(
    lit: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
    owner_id: &str,
) {
    let mut cursor = lit.walk();
    let mut pos_idx: usize = 0;
    for child in lit.children(&mut cursor) {
        let (key_text, value_wrap) = match child.kind() {
            "keyed_element" => {
                let key_text = child.child_by_field_name("key")
                    .and_then(|k| k.named_child(0))
                    .filter(|inner| inner.kind() == "identifier")
                    .map(|inner| go_text(inner, src).to_string());
                (key_text, child.child_by_field_name("value"))
            }
            "literal_element" => (None, Some(child)),
            _ => continue,
        };
        let Some(value_wrap) = value_wrap else { continue };
        let Some(inner) = value_wrap.named_child(0) else { continue };
        if let Some(vid) = flow_go(inner, src, file, fn_sym, scope, out) {
            out.edges.push(DfEdge { from: vid.clone(), to: owner_id.to_string() });
            let field = key_text.unwrap_or_else(|| pos_idx.to_string());
            out.fields.push((owner_id.to_string(), field, vid));
        }
        pos_idx += 1;
    }
}

/// Walk all children conservatively, surfacing the last value-bearing child's
/// id. The generic fallback `flow_go` reaches for every node kind it doesn't
/// special-case.
fn go_recurse_children(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) -> Option<String> {
    let mut cursor = node.walk();
    let mut last = None;
    for child in node.children(&mut cursor) {
        if let Some(id) = flow_go(child, src, file, fn_sym, scope, out) {
            last = Some(id);
        }
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn go_fields_embeds_and_generic_constraints() {
        let src = "\
package app

type Pricing interface {
	Price() int
}

type Store struct{}

type Repo[T Entity] struct {
	Store
	*Pricing
	cache Cache
	items []Item
}

type Color int

const (
	Red Color = iota
)
";
        let got = go_edges_from(go_parse(src).unwrap().root_node(), src.as_bytes());
        assert!(has(&got, "Repo", "Store", "impl"), "{got:?}"); // embedded (no field name)
        assert!(has(&got, "Repo", "Pricing", "impl"), "{got:?}"); // embedded via pointer
        assert!(has(&got, "Repo", "Cache", "field"), "{got:?}");
        assert!(has(&got, "Repo", "Item", "field"), "{got:?}");
        assert!(has(&got, "Repo", "Entity", "generic"), "{got:?}");
        // declared type param T is not itself a ref, and builtin `int` is noise.
        assert!(!got.iter().any(|e| e.to == "T"), "{got:?}");
        assert!(!got.iter().any(|e| e.to == "int"), "{got:?}");
    }

    #[test]
    fn go_entities_cover_struct_interface_alias_function_method() {
        let src = "\
package app

type Store struct {
	Host string
}

type Pricing interface {
	Price() int
}

type ID = string

func Resolve(name string, count int) (Store, error) { return Store{}, nil }

func (s *Store) Name() string { return s.Host }
";
        let facts = GoTypes.extract("app/store.go", src);
        let by = |name: &str| facts.entities.iter().find(|e| e.name == name)
            .unwrap_or_else(|| panic!("missing {name}: {:?}", facts.entities));
        assert_eq!(by("Store").kind, EntityKind::Struct);
        assert_eq!(by("Pricing").kind, EntityKind::Interface);
        assert_eq!(by("ID").kind, EntityKind::Alias);
        let resolve = by("Resolve");
        assert_eq!(resolve.kind, EntityKind::Function);
        let ty = resolve.ty.as_ref().unwrap();
        assert_eq!(ty.params[0], vec![]); // `string` is a builtin, no ref
        assert_eq!(ty.params[1], vec![]); // `int` is a builtin, no ref
        // multi-value return: both result types union into one flat `ret` list.
        assert!(ty.ret.contains(&TypeRef::Named("Store".into())), "{ty:?}");
        assert!(!ty.ret.contains(&TypeRef::Named("error".into())), "error is builtin noise: {ty:?}");

        let name_method = facts.entities.iter().find(|e| e.name == "Name" && e.kind == EntityKind::Method)
            .unwrap_or_else(|| panic!("missing Name method: {:?}", facts.entities));
        // receiver `*Store` strips the pointer; parent joins Store's OWN sym.
        assert_eq!(name_method.parent.as_deref(), Some(mint_sym("app/store.go", EntityKind::Struct, "Store", None).as_str()));
        assert_eq!(name_method.sym, mint_sym("app/store.go", EntityKind::Method, "Name", Some("Store")));
    }

    #[test]
    fn go_dataflow_param_call_member_and_composite_literal() {
        let src = "\
package app

func build(host string) Widget {
	w := Widget{Host: host, Port: 1}
	n := w.Host
	Log(n)
	return w
}
";
        let df = GoTypes.extract_dataflow("f.go", src);
        // param seeded at slot 0.
        let host_param = dnode(&df, "param", "host");
        assert_eq!(df.param_pos.iter().find(|(i, _)| i == &host_param.id).map(|(_, p)| *p), Some(0));
        // composite literal: `new` node carrying the type name, keyed field flows.
        let widget = dnode(&df, "new", "Widget").id.clone();
        let host_read = df.nodes.iter().find(|n| n.kind == "var_read" && n.var == "host").unwrap().id.clone();
        assert!(has_field(&df, &widget, "Host", &host_read), "{:?}", df.fields);
        assert!(df.fields.iter().any(|(i, f, _)| i == &widget && f == "Port"), "{:?}", df.fields);
        // `.Host` outside a call is a member read carrying the field name.
        let member = dnode(&df, "member", "Host");
        assert!(df.edges.iter().any(|e| e.to == member.id), "{:?}", df.edges);
        // `Log(n)`: plain call stays call_res with a slot-0 arg.
        let n_read = df.nodes.iter().find(|n| n.kind == "var_read" && n.var == "n").unwrap().id.clone();
        let call = dnode(&df, "call_res", "").id.clone();
        assert!(has_arg(&df, &call, 0, &n_read), "{:?}", df.args);
        // `return w`: one ret node fed by the read of `w`.
        let ret = df.nodes.iter().find(|n| n.kind == "ret").expect("ret node");
        assert!(df.edges.iter().any(|e| e.to == ret.id), "{:?}", df.edges);
    }

    #[test]
    fn go_for_range_loop_span_and_nest() {
        let src = "\
package app

func sum(xs []int) int {
	total := 0
	for _, x := range xs {
		total = total + Compute(x)
	}
	return total
}
";
        let df = GoTypes.extract_dataflow("f.go", src);
        assert_eq!(df.loops.len(), 1, "{:?}", df.loops);
        assert_eq!(df.loops[0].var, "x");
        let call = df.nodes.iter().find(|n| n.kind == "call_res").expect("Compute call");
        assert!(df.nests.iter().any(|n| n.call_id == call.id), "{:?}", df.nests);
    }

    #[test]
    fn go_func_literal_lifts_as_own_scope() {
        let src = "\
package app

func process() {
	apply(func(x int) int { return x + 1 })
}
";
        let df = GoTypes.extract_dataflow("f.go", src);
        assert_lambda_lifted(&df, 0, "x");
    }

    #[test]
    fn go_doc_comment_block_and_deprecated_tag() {
        let src = r#"
package app

// Store holds pricing data.
//
// Deprecated: use Repo instead.
type Store struct{}

// Name returns the display name.
func (s *Store) Name() string { return "" }
"#;
        let facts = GoTypes.extract("app/store.go", src);
        let store_sym = mint_sym("app/store.go", EntityKind::Struct, "Store", None);
        let doc = facts.docs.iter().find(|d| d.sym == store_sym)
            .unwrap_or_else(|| panic!("missing Store doc: {:?}", facts.docs));
        assert!(doc.text.starts_with("Store holds pricing data."), "{:?}", doc.text);
        assert!(doc.tags.iter().any(|t| t.tag == "deprecated" && t.text == "use Repo instead."), "{:?}", doc.tags);
        let method_sym = mint_sym("app/store.go", EntityKind::Method, "Name", Some("Store"));
        assert!(facts.docs.iter().any(|d| d.sym == method_sym), "{:?}", facts.docs);
    }
    // ── Python ───────────────────────────────────────────────────────────────
}
