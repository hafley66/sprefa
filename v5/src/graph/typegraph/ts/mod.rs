//! TS/TSX/JS extractor arm (oxc front-end): TypeLang impl, type edges,
//! entities/consts/docs, call defs/sites. Pure code motion out of the
//! former single typegraph.rs; zero behavior change.

pub mod flow;
pub mod text;

use std::collections::BTreeSet;

use oxc_ast::ast as ts_ast;
use oxc_ast_visit::Visit as OxcVisit;

use super::*;
use flow::{span_off, ts_dataflow_from};
pub use text::{ts_comments, ts_template_parts, ts_unresolved_refs, TemplatePart, UnresolvedRef};

impl TypeLang for TsTypes {
    fn name(&self) -> &'static str {
        "ts"
    }
    // Plain JS rides the same oxc front-end as TS: `.js`/`.jsx`/`.mjs`/`.cjs`
    // parse fine as JSX-enabled JavaScript, so type_entity/call_*/df_*/
    // doc_comment all populate for JS too (type_link/type_sig stay thin, a
    // JS file carries no type annotations to resolve). Nothing else in the
    // `type_langs()` registry claims these extensions.
    fn matches(&self, path: &str) -> bool {
        path.ends_with(".ts")
            || path.ends_with(".tsx")
            || path.ends_with(".js")
            || path.ends_with(".jsx")
            || path.ends_with(".mjs")
            || path.ends_with(".cjs")
            || path.ends_with(".mts")
            || path.ends_with(".cts")
    }
    // One oxc parse feeds both walks.
    fn extract(&self, file: &str, content: &str) -> TypeFacts {
        let alloc = oxc_allocator::Allocator::default();
        let st = source_type_for(file);
        let ret = oxc_parser::Parser::new(&alloc, content, st).parse();
        if ret.panicked {
            return TypeFacts::default();
        }
        let mut entities = ts_entities_from(&ret.program, file, content);
        let (const_entities, consts, const_spread_skips, const_mutable_skips) =
            ts_const_facts_from(&ret.program, file, content);
        entities.extend(const_entities);
        TypeFacts {
            entities,
            edges: ts_edges_from(&ret.program),
            docs: ts_docs_from(&ret.program, file, content),
            consts,
            const_spread_skips,
            const_mutable_skips,
        }
    }
    // One oxc parse feeds defs + sites, same shape as the Rust pass. `line_at`
    // recovers 1-based lines from oxc's byte-offset spans.
    fn extract_calls(&self, file: &str, content: &str) -> CallFacts {
        let alloc = oxc_allocator::Allocator::default();
        let st = source_type_for(file);
        let ret = oxc_parser::Parser::new(&alloc, content, st).parse();
        if ret.panicked {
            return CallFacts::default();
        }
        let starts = line_index(content);
        let mut defs = ts_call_defs_from(&ret.program, file, &starts);
        // Unbound arrow / function-expression lambdas. The df lift is the one
        // place that already mints their `::closure::<byte_off>` sym, and it
        // mints a `closure` value node ONLY for an inline (unbound) function
        // value — a const-bound arrow is lifted under its binding name with no
        // closure node — so this set is exactly the unbound lambdas, disjoint
        // from ts_var_call_defs. Reusing it makes call_def.sym == df fn_sym hold
        // by construction. (Same already-parsed program, one extra walk.)
        let df = ts_dataflow_from(&ret.program, file, content);
        ts_push_lambda_defs(&df, file, &mut defs);
        // Nested named function declarations (below top level), file-level mint.
        let mut nested = TsNestedFnDefs {
            file,
            starts: &starts,
            depth: 0,
            out: Vec::new(),
        };
        nested.visit_program(&ret.program);
        defs.extend(nested.out);
        let mut sites = TsCallSites {
            file,
            starts: &starts,
            sites: Vec::new(),
        };
        sites.visit_program(&ret.program);
        CallFacts {
            defs,
            sites: sites.sites,
        }
    }
    // One oxc parse feeds the node + edge lift. Byte-offset spans (oxc's native
    // shape) become node ids `file:<byte_off>`; `line_at` recovers the 1-based
    // line for the `line` column.
    fn extract_dataflow(&self, file: &str, content: &str) -> DataflowFacts {
        let alloc = oxc_allocator::Allocator::default();
        let st = source_type_for(file);
        let ret = oxc_parser::Parser::new(&alloc, content, st).parse();
        if ret.panicked {
            return DataflowFacts::default();
        }
        ts_dataflow_from(&ret.program, file, content)
    }
}

pub fn ts_edges(content: &str, tsx: bool) -> Vec<TypeEdge> {
    let alloc = oxc_allocator::Allocator::default();
    let st = if tsx {
        oxc_span::SourceType::tsx()
    } else {
        oxc_span::SourceType::ts()
    };
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
                ts_ast::ExportDefaultDeclarationKind::ClassDeclaration(c) => {
                    ts_class_edges(c, &mut out)
                }
                ts_ast::ExportDefaultDeclarationKind::TSInterfaceDeclaration(i) => {
                    ts_interface_edges(i, &mut out)
                }
                ts_ast::ExportDefaultDeclarationKind::FunctionDeclaration(f) => {
                    ts_function_edges(f, &mut out)
                }
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
    let mut c = TsRefs {
        params,
        out: Vec::new(),
    };
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
fn ts_var_fn_edges(
    v: &ts_ast::VariableDeclaration,
    out: &mut BTreeSet<(String, String, &'static str)>,
) {
    for d in &v.declarations {
        let ts_ast::BindingPattern::BindingIdentifier(name) = &d.id else {
            continue;
        };
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
        let mut v = TsRefs {
            params: &tp,
            out: Vec::new(),
        };
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

fn ts_enum_edges(
    e: &ts_ast::TSEnumDeclaration,
    out: &mut BTreeSet<(String, String, &'static str)>,
) {
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
    let st = if tsx {
        oxc_span::SourceType::tsx()
    } else {
        oxc_span::SourceType::ts()
    };
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
                ts_ast::ExportDefaultDeclarationKind::ClassDeclaration(c) => {
                    ts_class_entity(c, file, &starts, &mut out)
                }
                ts_ast::ExportDefaultDeclarationKind::TSInterfaceDeclaration(i) => push_entity(
                    &mut out,
                    file,
                    &starts,
                    &i.id.name,
                    i.span.start,
                    EntityKind::Interface,
                    None,
                    None,
                ),
                ts_ast::ExportDefaultDeclarationKind::FunctionDeclaration(f) => {
                    ts_fn_entity(f, file, &starts, &mut out)
                }
                _ => {}
            },
            S::ClassDeclaration(c) => ts_class_entity(c, file, &starts, &mut out),
            S::TSInterfaceDeclaration(i) => push_entity(
                &mut out,
                file,
                &starts,
                &i.id.name,
                i.span.start,
                EntityKind::Interface,
                None,
                None,
            ),
            S::TSTypeAliasDeclaration(a) => push_entity(
                &mut out,
                file,
                &starts,
                &a.id.name,
                a.span.start,
                EntityKind::Alias,
                None,
                None,
            ),
            S::TSEnumDeclaration(e) => push_entity(
                &mut out,
                file,
                &starts,
                &e.id.name,
                e.span.start,
                EntityKind::Enum,
                None,
                None,
            ),
            S::FunctionDeclaration(f) => ts_fn_entity(f, file, &starts, &mut out),
            S::VariableDeclaration(v) => ts_var_fn_entity(v, file, &starts, &mut out),
            _ => {}
        }
    }
    out
}

fn ts_decl_entity(
    d: &ts_ast::Declaration,
    file: &str,
    starts: &[usize],
    out: &mut Vec<TypeEntity>,
) {
    match d {
        ts_ast::Declaration::ClassDeclaration(c) => ts_class_entity(c, file, starts, out),
        ts_ast::Declaration::TSInterfaceDeclaration(i) => push_entity(
            out,
            file,
            starts,
            &i.id.name,
            i.span.start,
            EntityKind::Interface,
            None,
            None,
        ),
        ts_ast::Declaration::TSTypeAliasDeclaration(a) => push_entity(
            out,
            file,
            starts,
            &a.id.name,
            a.span.start,
            EntityKind::Alias,
            None,
            None,
        ),
        ts_ast::Declaration::TSEnumDeclaration(e) => push_entity(
            out,
            file,
            starts,
            &e.id.name,
            e.span.start,
            EntityKind::Enum,
            None,
            None,
        ),
        ts_ast::Declaration::FunctionDeclaration(f) => ts_fn_entity(f, file, starts, out),
        ts_ast::Declaration::VariableDeclaration(v) => ts_var_fn_entity(v, file, starts, out),
        _ => {}
    }
}

fn ts_class_entity(c: &ts_ast::Class, file: &str, starts: &[usize], out: &mut Vec<TypeEntity>) {
    let Some(id) = &c.id else { return };
    let owner = id.name.to_string();
    push_entity(
        out,
        file,
        starts,
        &id.name,
        c.span.start,
        EntityKind::Class,
        None,
        None,
    );
    for el in &c.body.body {
        if let ts_ast::ClassElement::MethodDefinition(m) = el {
            // normal method name `foo()`; skip computed/private/constructor keys
            if m.kind == ts_ast::MethodDefinitionKind::Constructor {
                continue;
            }
            if let ts_ast::PropertyKey::StaticIdentifier(k) = &m.key {
                let ty = ts_fn_type(
                    &m.value.type_parameters,
                    &m.value.params,
                    &m.value.return_type,
                );
                // a TS method's owner is always the enclosing class.
                push_entity(
                    out,
                    file,
                    starts,
                    &k.name,
                    m.span.start,
                    EntityKind::Method,
                    Some((&owner, EntityKind::Class)),
                    Some(ty),
                );
            }
        }
    }
}

fn ts_fn_entity(f: &ts_ast::Function, file: &str, starts: &[usize], out: &mut Vec<TypeEntity>) {
    let Some(id) = &f.id else { return };
    let ty = ts_fn_type(&f.type_parameters, &f.params, &f.return_type);
    push_entity(
        out,
        file,
        starts,
        &id.name,
        f.span.start,
        EntityKind::Function,
        None,
        Some(ty),
    );
}

fn ts_var_fn_entity(
    v: &ts_ast::VariableDeclaration,
    file: &str,
    starts: &[usize],
    out: &mut Vec<TypeEntity>,
) {
    for d in &v.declarations {
        let ts_ast::BindingPattern::BindingIdentifier(name) = &d.id else {
            continue;
        };
        let ty = match &d.init {
            Some(ts_ast::Expression::ArrowFunctionExpression(a)) => {
                ts_fn_type(&a.type_parameters, &a.params, &a.return_type)
            }
            Some(ts_ast::Expression::FunctionExpression(f)) => {
                ts_fn_type(&f.type_parameters, &f.params, &f.return_type)
            }
            _ => continue,
        };
        push_entity(
            out,
            file,
            starts,
            &name.name,
            d.span.start,
            EntityKind::Function,
            None,
            Some(ty),
        );
    }
}

/// Strip the type-level wrappers that are transparent to a const's runtime
/// value — `as const`, `satisfies T`, parens — same transparency `ts_flow_expr`
/// already gives these forms, so the initializer underneath is reached the
/// same way whether we're lifting dataflow or folding a constant.
fn ts_unwrap_const<'a, 'b>(e: &'b ts_ast::Expression<'a>) -> &'b ts_ast::Expression<'a> {
    match e {
        ts_ast::Expression::TSAsExpression(t) => ts_unwrap_const(&t.expression),
        ts_ast::Expression::TSSatisfiesExpression(t) => ts_unwrap_const(&t.expression),
        ts_ast::Expression::ParenthesizedExpression(p) => ts_unwrap_const(&p.expression),
        _ => e,
    }
}

/// Whether an expression (after unwrapping `as const`/`satisfies`/parens)
/// carries a string value somewhere — a plain string literal, a template, or
/// an object literal with at least one string-bearing property (recursively).
/// Gates entity-minting: a const whose value has no string anywhere gains
/// neither a `type_entity` row nor any `const_value` rows (the "don't mint an
/// entity for every const in the corpus" rule).
fn ts_expr_string_bearing(e: &ts_ast::Expression) -> bool {
    match ts_unwrap_const(e) {
        ts_ast::Expression::StringLiteral(_) | ts_ast::Expression::TemplateLiteral(_) => true,
        ts_ast::Expression::ObjectExpression(o) => o.properties.iter().any(|p| match p {
            ts_ast::ObjectPropertyKind::ObjectProperty(prop) => ts_expr_string_bearing(&prop.value),
            // A spread's value is opaque without evaluating its source; it
            // can't make the object string-bearing on its own (the caller
            // counts it separately when walking for real).
            ts_ast::ObjectPropertyKind::SpreadProperty(_) => false,
        }),
        _ => false,
    }
}

/// Recursively collect `ConstValueFact` rows from a const initializer.
/// `prefix` is the dotted field path built so far ("" at the top, "home",
/// "nested.a", ...). A computed object key (`[expr]: v`) is skipped — there is
/// no static name to hang the field on. A spread property is counted (never
/// followed: its value lives in another symbol this walk hasn't resolved).
fn ts_collect_const_values(
    e: &ts_ast::Expression,
    sym: &str,
    prefix: &str,
    file: &str,
    starts: &[usize],
    content: &str,
    out: &mut Vec<ConstValueFact>,
    spread_skips: &mut usize,
) {
    use oxc_span::GetSpan;
    match ts_unwrap_const(e) {
        ts_ast::Expression::StringLiteral(s) => {
            out.push(ConstValueFact {
                sym: sym.to_string(),
                field: prefix.to_string(),
                text: s.value.to_string(),
                kind: "lit",
                file: file.to_string(),
                line: line_at(starts, s.span.start as usize),
            });
        }
        ts_ast::Expression::TemplateLiteral(t) => {
            let span = t.span();
            let text = content
                .get(span.start as usize..span.end as usize)
                .unwrap_or_default()
                .to_string();
            out.push(ConstValueFact {
                sym: sym.to_string(),
                field: prefix.to_string(),
                text,
                kind: "template",
                file: file.to_string(),
                line: line_at(starts, span.start as usize),
            });
        }
        ts_ast::Expression::ObjectExpression(o) => {
            for p in &o.properties {
                match p {
                    ts_ast::ObjectPropertyKind::ObjectProperty(prop) => {
                        let key = match &prop.key {
                            ts_ast::PropertyKey::StaticIdentifier(i) => i.name.to_string(),
                            ts_ast::PropertyKey::StringLiteral(s) => s.value.to_string(),
                            _ => continue, // computed key: no static field name
                        };
                        let field = if prefix.is_empty() {
                            key
                        } else {
                            format!("{prefix}.{key}")
                        };
                        ts_collect_const_values(
                            &prop.value,
                            sym,
                            &field,
                            file,
                            starts,
                            content,
                            out,
                            spread_skips,
                        );
                    }
                    ts_ast::ObjectPropertyKind::SpreadProperty(_) => {
                        *spread_skips += 1;
                    }
                }
            }
        }
        _ => {}
    }
}

/// A `const`/`let`/`var` binding, entity + value pass. `scope` is the name of
/// the nearest enclosing function/closure for a binding found INSIDE a
/// function body (`None` at true module level) — folded into `mint_sym`'s
/// `parent` slot so a lookup table declared inside two different functions in
/// the same file mints two distinct syms rather than colliding. Module-level
/// callers (`ts_const_facts_from`'s own top-level loop) pass `None`;
/// `TsNestedConstWalker` (below) passes the enclosing scope name for anything
/// found inside a function/arrow body — this is what gives `const_value`
/// parity with the retired `const_string_member`'s "generically discovered,
/// not scope-restricted" coverage (a lookup table inside a function body
/// counts too). Arrow/function-expression consts are `ts_var_fn_entity`'s job
/// (a Function entity, untouched here); this walk only looks at bindings that
/// carry a string value: `const name = "..."`. SOUNDNESS RULE: only `const`
/// (or a `let`/`var` marked `as const`) is honest to fold — a plain `let`/`var`
/// string initializer can change under your feet, so it is counted loudly
/// (`const_mutable_skips`) and never emitted.
#[allow(clippy::too_many_arguments)]
fn ts_var_const_facts(
    v: &ts_ast::VariableDeclaration,
    file: &str,
    starts: &[usize],
    content: &str,
    scope: Option<&str>,
    entities: &mut Vec<TypeEntity>,
    consts: &mut Vec<ConstValueFact>,
    spread_skips: &mut usize,
    mutable_skips: &mut usize,
) {
    for d in &v.declarations {
        let ts_ast::BindingPattern::BindingIdentifier(name) = &d.id else {
            continue;
        };
        let Some(init) = &d.init else { continue };
        // Arrow/function-expression consts are ts_var_fn_entity's Function
        // entities; leave those exactly as they are.
        if matches!(
            init,
            ts_ast::Expression::ArrowFunctionExpression(_)
                | ts_ast::Expression::FunctionExpression(_)
        ) {
            continue;
        }
        if !ts_expr_string_bearing(init) {
            continue;
        }
        let as_const = matches!(init, ts_ast::Expression::TSAsExpression(t) if t.type_annotation.is_const_type_reference());
        if !v.kind.is_const() && !as_const {
            *mutable_skips += 1;
            continue;
        }
        let sym = mint_sym(file, EntityKind::Const, &name.name, scope);
        entities.push(TypeEntity {
            sym: sym.clone(),
            name: name.name.to_string(),
            kind: EntityKind::Const,
            parent: None,
            file: file.to_string(),
            line: line_at(starts, d.span.start as usize),
            ty: None,
        });
        ts_collect_const_values(init, &sym, "", file, starts, content, consts, spread_skips);
    }
}

/// String enum members (`enum Routes { Home = '/home' }`): `sym` is the
/// ENUM's own entity sym (already minted by `ts_entities_from`'s
/// `TSEnumDeclaration` arm) — a member is a field of its enum, not a second
/// entity. Only a plain string initializer qualifies; a computed/numeric
/// member yields no row.
fn ts_enum_const_values(
    e: &ts_ast::TSEnumDeclaration,
    file: &str,
    starts: &[usize],
    out: &mut Vec<ConstValueFact>,
) {
    let owner_sym = mint_sym(file, EntityKind::Enum, &e.id.name, None);
    for m in &e.body.members {
        let name = match &m.id {
            ts_ast::TSEnumMemberName::Identifier(id) => id.name.to_string(),
            ts_ast::TSEnumMemberName::String(s) => s.value.to_string(),
            _ => continue,
        };
        let Some(init) = &m.initializer else { continue };
        if let ts_ast::Expression::StringLiteral(s) = ts_unwrap_const(init) {
            out.push(ConstValueFact {
                sym: owner_sym.clone(),
                field: name,
                text: s.value.to_string(),
                kind: "lit",
                file: file.to_string(),
                line: line_at(starts, m.span.start as usize),
            });
        }
    }
}

/// Top-level driver for the const-value pass (item 3 of the string-values
/// arc): walks `program.body` once for top-level `const`/`let`/`var`
/// declarations (bare or `export`-wrapped) and `enum` declarations, returning
/// the const entities to fold into `ts_entities_from`'s output plus the
/// `const_value` rows and the two loud-skip counters. A SEPARATE statement
/// walk from `ts_entities_from`/`ts_edges_from`/`ts_docs_from` (same "one
/// file, several cheap syntax walks" shape those already use) rather than
/// retrofitting those recursive helpers, which are reused by call-graph/
/// dataflow passes with a narrower `Vec<TypeEntity>`-only signature.
///
/// After the top-level loop, `TsNestedConstWalker` descends into every
/// function/arrow body in the file for the SAME string-bearing-const shape,
/// scoped by the nearest enclosing function/closure name — this is the
/// evidence-diff fix from the `const_string_member` retirement (plans/
/// 2026-07-10-string-values-const-value.md follow-up): `const_string_member`
/// was "generically discovered" (every `const` declarator in the file, no
/// scope restriction), so a lookup table declared inside a function body
/// counted there but was invisible to `const_value`'s module-level-only walk.
/// Enum declarations stay top-level-only (no known corpus case of a
/// function-local enum feeding a route table; `const_string_member` never
/// covered enums either).
fn ts_const_facts_from(
    program: &ts_ast::Program,
    file: &str,
    content: &str,
) -> (Vec<TypeEntity>, Vec<ConstValueFact>, usize, usize) {
    let starts = line_index(content);
    let mut entities = Vec::new();
    let mut consts = Vec::new();
    let mut spread_skips = 0usize;
    let mut mutable_skips = 0usize;
    for stmt in &program.body {
        use ts_ast::Statement as S;
        let var_decl: Option<&ts_ast::VariableDeclaration> = match stmt {
            S::VariableDeclaration(v) => Some(v),
            S::ExportNamedDeclaration(exp) => match &exp.declaration {
                Some(ts_ast::Declaration::VariableDeclaration(v)) => Some(v),
                _ => None,
            },
            _ => None,
        };
        if let Some(v) = var_decl {
            ts_var_const_facts(
                v,
                file,
                &starts,
                content,
                None,
                &mut entities,
                &mut consts,
                &mut spread_skips,
                &mut mutable_skips,
            );
        }
        let enum_decl: Option<&ts_ast::TSEnumDeclaration> = match stmt {
            S::TSEnumDeclaration(en) => Some(en),
            S::ExportNamedDeclaration(exp) => match &exp.declaration {
                Some(ts_ast::Declaration::TSEnumDeclaration(en)) => Some(en),
                _ => None,
            },
            _ => None,
        };
        if let Some(en) = enum_decl {
            ts_enum_const_values(en, file, &starts, &mut consts);
        }
    }
    let mut nested = TsNestedConstWalker {
        file,
        content,
        starts: &starts,
        scope: Vec::new(),
        entities: &mut entities,
        consts: &mut consts,
        spread_skips: &mut spread_skips,
        mutable_skips: &mut mutable_skips,
    };
    nested.visit_program(program);
    (entities, consts, spread_skips, mutable_skips)
}

/// Descends into every function/arrow body for string-bearing `const`
/// declarations found there — see `ts_const_facts_from`'s doc comment. Only
/// fires INSIDE a function scope (`scope` non-empty); top-level statements
/// are the existing loop's job, so `visit_variable_declaration` is a no-op at
/// depth 0 (avoids double-emitting a module-level const). `visit_function`/
/// `visit_arrow_function_expression` push a scope name — the function's own
/// name when named, else a byte-offset-derived `closure_<span-start>` tag
/// (stable across ticks for unchanged content, matching the `df_lit`/
/// `template_parts` `node` id convention) — so two same-named local consts in
/// two different functions in the same file mint distinct syms.
struct TsNestedConstWalker<'s> {
    file: &'s str,
    content: &'s str,
    starts: &'s [usize],
    scope: Vec<String>,
    entities: &'s mut Vec<TypeEntity>,
    consts: &'s mut Vec<ConstValueFact>,
    spread_skips: &'s mut usize,
    mutable_skips: &'s mut usize,
}

impl<'a, 's> OxcVisit<'a> for TsNestedConstWalker<'s> {
    fn visit_function(&mut self, it: &ts_ast::Function<'a>, flags: oxc_syntax::scope::ScopeFlags) {
        let name = it
            .id
            .as_ref()
            .map(|id| id.name.to_string())
            .unwrap_or_else(|| format!("closure_{}", it.span.start));
        self.scope.push(name);
        oxc_ast_visit::walk::walk_function(self, it, flags);
        self.scope.pop();
    }

    fn visit_arrow_function_expression(&mut self, it: &ts_ast::ArrowFunctionExpression<'a>) {
        self.scope.push(format!("closure_{}", it.span.start));
        oxc_ast_visit::walk::walk_arrow_function_expression(self, it);
        self.scope.pop();
    }

    fn visit_variable_declaration(&mut self, it: &ts_ast::VariableDeclaration<'a>) {
        if let Some(scope) = self.scope.last() {
            let scope = scope.clone();
            ts_var_const_facts(
                it,
                self.file,
                self.starts,
                self.content,
                Some(&scope),
                self.entities,
                self.consts,
                self.spread_skips,
                self.mutable_skips,
            );
        }
        oxc_ast_visit::walk::walk_variable_declaration(self, it);
    }
}

/// Doc-comment pass (oxc): oxc keeps comments out of the AST, so each `/** */`
/// block in the source is associated with the entity it documents by byte
/// position — the nearest anchor at or after the block's end, with only
/// whitespace between (so an `export`/`default` prefix, which sits before the
/// anchored statement start, is fine; a decorator or another statement is not,
/// and the block is dropped). Syms match `ts_entities_from` exactly so
/// `doc_comment` joins `type_entity`.
fn ts_docs_from(program: &ts_ast::Program, file: &str, content: &str) -> Vec<DocFact> {
    let anchors = ts_doc_anchors(program, file);
    if anchors.is_empty() {
        return Vec::new();
    }
    let starts = line_index(content);
    let mut out = Vec::new();
    for (cstart, cend) in ts_block_comments(content) {
        let raw = &content[cstart..cend];
        if !raw.trim_start().starts_with("/**") {
            continue;
        }
        let Some((sym, at)) = anchors
            .iter()
            .filter(|(_, s)| (*s as usize) >= cend)
            .min_by_key(|(_, s)| *s)
        else {
            continue;
        };
        if !content[cend..*at as usize].trim().is_empty() {
            continue;
        }
        let text = clean_block_comment(raw);
        out.push(DocFact {
            sym: sym.clone(),
            line: line_at(&starts, *at as usize),
            tags: parse_jsdoc_tags(&text),
            text,
        });
    }
    out
}

/// `(sym, byte)` for every entity `ts_entities_from` emits. Top-level decls
/// anchor at the STATEMENT start; class methods at the method span start.
fn ts_doc_anchors(program: &ts_ast::Program, file: &str) -> Vec<(String, u32)> {
    use oxc_span::GetSpan;
    let mut out = Vec::new();
    for stmt in &program.body {
        use ts_ast::Statement as S;
        let at = stmt.span().start;
        match stmt {
            S::ExportNamedDeclaration(e) => {
                if let Some(d) = &e.declaration {
                    ts_decl_anchor(d, file, at, &mut out);
                }
            }
            S::ExportDefaultDeclaration(e) => match &e.declaration {
                ts_ast::ExportDefaultDeclarationKind::ClassDeclaration(c) => {
                    ts_class_anchor(c, file, at, &mut out)
                }
                ts_ast::ExportDefaultDeclarationKind::TSInterfaceDeclaration(i) => {
                    out.push((mint_sym(file, EntityKind::Interface, &i.id.name, None), at))
                }
                ts_ast::ExportDefaultDeclarationKind::FunctionDeclaration(f) => {
                    if let Some(id) = &f.id {
                        out.push((mint_sym(file, EntityKind::Function, &id.name, None), at));
                    }
                }
                _ => {}
            },
            S::ClassDeclaration(c) => ts_class_anchor(c, file, at, &mut out),
            S::TSInterfaceDeclaration(i) => {
                out.push((mint_sym(file, EntityKind::Interface, &i.id.name, None), at))
            }
            S::TSTypeAliasDeclaration(a) => {
                out.push((mint_sym(file, EntityKind::Alias, &a.id.name, None), at))
            }
            S::TSEnumDeclaration(en) => {
                out.push((mint_sym(file, EntityKind::Enum, &en.id.name, None), at))
            }
            S::FunctionDeclaration(f) => {
                if let Some(id) = &f.id {
                    out.push((mint_sym(file, EntityKind::Function, &id.name, None), at));
                }
            }
            S::VariableDeclaration(v) => ts_var_anchor(v, file, at, &mut out),
            _ => {}
        }
    }
    out
}

fn ts_decl_anchor(d: &ts_ast::Declaration, file: &str, at: u32, out: &mut Vec<(String, u32)>) {
    match d {
        ts_ast::Declaration::ClassDeclaration(c) => ts_class_anchor(c, file, at, out),
        ts_ast::Declaration::TSInterfaceDeclaration(i) => {
            out.push((mint_sym(file, EntityKind::Interface, &i.id.name, None), at))
        }
        ts_ast::Declaration::TSTypeAliasDeclaration(a) => {
            out.push((mint_sym(file, EntityKind::Alias, &a.id.name, None), at))
        }
        ts_ast::Declaration::TSEnumDeclaration(en) => {
            out.push((mint_sym(file, EntityKind::Enum, &en.id.name, None), at))
        }
        ts_ast::Declaration::FunctionDeclaration(f) => {
            if let Some(id) = &f.id {
                out.push((mint_sym(file, EntityKind::Function, &id.name, None), at));
            }
        }
        ts_ast::Declaration::VariableDeclaration(v) => ts_var_anchor(v, file, at, out),
        _ => {}
    }
}

fn ts_class_anchor(c: &ts_ast::Class, file: &str, at: u32, out: &mut Vec<(String, u32)>) {
    let Some(id) = &c.id else { return };
    let owner = id.name.to_string();
    out.push((mint_sym(file, EntityKind::Class, &id.name, None), at));
    for el in &c.body.body {
        if let ts_ast::ClassElement::MethodDefinition(m) = el {
            if m.kind == ts_ast::MethodDefinitionKind::Constructor {
                continue;
            }
            if let ts_ast::PropertyKey::StaticIdentifier(k) = &m.key {
                out.push((
                    mint_sym(file, EntityKind::Method, &k.name, Some(&owner)),
                    m.span.start,
                ));
            }
        }
    }
}

fn ts_var_anchor(
    v: &ts_ast::VariableDeclaration,
    file: &str,
    at: u32,
    out: &mut Vec<(String, u32)>,
) {
    for d in &v.declarations {
        let ts_ast::BindingPattern::BindingIdentifier(name) = &d.id else {
            continue;
        };
        if matches!(
            &d.init,
            Some(ts_ast::Expression::ArrowFunctionExpression(_))
                | Some(ts_ast::Expression::FunctionExpression(_))
        ) {
            out.push((mint_sym(file, EntityKind::Function, &name.name, None), at));
        }
    }
}

/// Byte ranges of every `/* ... */` block comment, including delimiters. A naive
/// scan: good enough for doc association (non-`/**` blocks are filtered by the
/// caller, and `/*` inside a string is rare and harmless here).
fn ts_block_comments(content: &str) -> Vec<(usize, usize)> {
    let b = content.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < b.len() {
        if b[i] == b'/' && b[i + 1] == b'*' {
            match content[i + 2..].find("*/") {
                Some(rel) => {
                    let end = i + 2 + rel + 2;
                    out.push((i, end));
                    i = end;
                    continue;
                }
                None => break,
            }
        }
        i += 1;
    }
    out
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

// `parent` is `(owner_name, owner_kind)`: the method sym embeds the owner NAME
// (`Owner.name`), while the stored `parent` field is the owner's OWN entity sym
// minted with the owner's REAL kind — so `type_entity.parent` joins
// `type_entity.sym` with no normalization.
fn push_entity(
    out: &mut Vec<TypeEntity>,
    file: &str,
    starts: &[usize],
    name: &str,
    span_start: u32,
    kind: EntityKind,
    parent: Option<(&str, EntityKind)>,
    ty: Option<TypeExpr>,
) {
    out.push(TypeEntity {
        sym: mint_sym(file, kind, name, parent.map(|(p, _)| p)),
        name: name.to_string(),
        kind,
        parent: parent.map(|(p, pk)| mint_sym(file, pk, p, None)),
        file: file.to_string(),
        line: line_at(starts, span_start as usize),
        ty,
    });
}

// --- TypeScript call-graph pass (oxc): function declarations, exported/const
// arrow + function-expression bindings, and class methods become CallDefs (Free
// for standalone callables, Method for class members keyed to the class); every
// `CallExpression` becomes a CallSite whose callee is the called name as written
// (identifier, or the trailing property of a member expression). `end` is the
// body span end converted to a 1-based line; caller resolution is the engine's
// span-containment pass, same as Rust. ---

fn ts_call_defs_from(program: &ts_ast::Program, file: &str, starts: &[usize]) -> Vec<CallDef> {
    let mut out = Vec::new();
    for stmt in &program.body {
        use ts_ast::Statement as S;
        match stmt {
            S::FunctionDeclaration(f) => ts_fn_call_def(f, file, starts, &mut out),
            S::ExportNamedDeclaration(e) => {
                if let Some(d) = &e.declaration {
                    ts_decl_call_def(d, file, starts, &mut out);
                }
            }
            S::ExportDefaultDeclaration(e) => match &e.declaration {
                ts_ast::ExportDefaultDeclarationKind::ClassDeclaration(c) => {
                    ts_class_call_defs(c, file, starts, &mut out)
                }
                ts_ast::ExportDefaultDeclarationKind::FunctionDeclaration(f) => {
                    ts_fn_call_def(f, file, starts, &mut out)
                }
                _ => {}
            },
            S::ClassDeclaration(c) => ts_class_call_defs(c, file, starts, &mut out),
            S::VariableDeclaration(v) => ts_var_call_defs(v, file, starts, &mut out),
            _ => {}
        }
    }
    out
}

fn ts_decl_call_def(d: &ts_ast::Declaration, file: &str, starts: &[usize], out: &mut Vec<CallDef>) {
    match d {
        ts_ast::Declaration::FunctionDeclaration(f) => ts_fn_call_def(f, file, starts, out),
        ts_ast::Declaration::ClassDeclaration(c) => ts_class_call_defs(c, file, starts, out),
        ts_ast::Declaration::VariableDeclaration(v) => ts_var_call_defs(v, file, starts, out),
        _ => {}
    }
}

// @callable ts function
fn ts_fn_call_def(f: &ts_ast::Function, file: &str, starts: &[usize], out: &mut Vec<CallDef>) {
    let Some(id) = &f.id else { return };
    let Some(body) = f.body.as_deref() else {
        return;
    };
    let name = id.name.to_string();
    out.push(CallDef {
        sym: mint_sym(file, EntityKind::Function, &name, None),
        name,
        kind: CallKind::Free,
        file: file.to_string(),
        line: line_at(starts, id.span.start as usize),
        end: line_at(starts, body.span.end as usize),
    });
}

// @callable ts method
fn ts_class_call_defs(c: &ts_ast::Class, file: &str, starts: &[usize], out: &mut Vec<CallDef>) {
    let Some(id) = &c.id else { return };
    let owner = id.name.to_string();
    for el in &c.body.body {
        let ts_ast::ClassElement::MethodDefinition(m) = el else {
            continue;
        };
        // computed/private keys have no static name to resolve.
        let ts_ast::PropertyKey::StaticIdentifier(k) = &m.key else {
            continue;
        };
        let Some(body) = m.value.body.as_deref() else {
            continue;
        };
        let is_ctor = m.kind == ts_ast::MethodDefinitionKind::Constructor;
        // The constructor's sym stays `mint_sym(Method, "constructor", owner)` —
        // IDENTICAL to what `ts_flow_class` mints for its df body, so df<->call
        // joins line up. Its call_name uses the CLASS name, so a `new Widget(x)`
        // call site (callee "Widget") resolves to this ctor row. Every other
        // method resolves by its own name (getters/setters share one sym).
        let name = if is_ctor {
            owner.clone()
        } else {
            k.name.to_string()
        };
        out.push(CallDef {
            sym: mint_sym(file, EntityKind::Method, &k.name, Some(&owner)),
            name,
            kind: CallKind::Method,
            file: file.to_string(),
            line: line_at(starts, m.span.start as usize),
            end: line_at(starts, body.span.end as usize),
        });
    }
}

/// Map the df lift's `closure` value nodes to Lambda call_defs. A closure node's
/// `var` is the lam_sym — identical to the lifted body's `fn_sym` — so the
/// call_def sym joins df exactly. `end` is the deepest body line (this lambda's
/// own nodes plus any nested closures), the extent callsite containment needs;
/// it falls back to the closure's own line for an empty body.
// @callable ts lambda
fn ts_push_lambda_defs(df: &DataflowFacts, file: &str, out: &mut Vec<CallDef>) {
    for node in &df.nodes {
        if node.kind != "closure" {
            continue;
        }
        let lam_sym = &node.var;
        let nested = format!("{lam_sym}::closure::");
        let end = df
            .nodes
            .iter()
            .filter(|n| &n.fn_sym == lam_sym || n.fn_sym.starts_with(&nested))
            .map(|n| n.line)
            .max()
            .unwrap_or(node.line)
            .max(node.line);
        out.push(CallDef {
            sym: lam_sym.clone(),
            name: String::new(),
            kind: CallKind::Lambda,
            file: file.to_string(),
            line: node.line,
            end,
        });
    }
}

/// Nested (below top level) named function DECLARATIONS — `function inner(){}`
/// inside another callable's body. Top-level declarations are `ts_fn_call_def`'s
/// job, so this walker only emits at `depth > 0`; the `FunctionType` guard skips
/// function expressions and method values (already Methods). File-level mint (df
/// does not lift a nested named-fn body, so there is no owner-scoped df sym to
/// match), mirroring the Rust nested-fn convention.
// @callable ts function
struct TsNestedFnDefs<'p> {
    file: &'p str,
    starts: &'p [usize],
    depth: u32,
    out: Vec<CallDef>,
}

impl<'a, 'p> OxcVisit<'a> for TsNestedFnDefs<'p> {
    fn visit_function(&mut self, f: &ts_ast::Function<'a>, flags: oxc_syntax::scope::ScopeFlags) {
        if self.depth > 0 && f.r#type == ts_ast::FunctionType::FunctionDeclaration {
            if let (Some(id), Some(body)) = (&f.id, f.body.as_deref()) {
                let name = id.name.to_string();
                self.out.push(CallDef {
                    sym: mint_sym(self.file, EntityKind::Function, &name, None),
                    name,
                    kind: CallKind::Free,
                    file: self.file.to_string(),
                    line: line_at(self.starts, id.span.start as usize),
                    end: line_at(self.starts, body.span.end as usize),
                });
            }
        }
        self.depth += 1;
        oxc_ast_visit::walk::walk_function(self, f, flags);
        self.depth -= 1;
    }
    fn visit_arrow_function_expression(&mut self, a: &ts_ast::ArrowFunctionExpression<'a>) {
        self.depth += 1;
        oxc_ast_visit::walk::walk_arrow_function_expression(self, a);
        self.depth -= 1;
    }
}

fn ts_var_call_defs(
    v: &ts_ast::VariableDeclaration,
    file: &str,
    starts: &[usize],
    out: &mut Vec<CallDef>,
) {
    for d in &v.declarations {
        let ts_ast::BindingPattern::BindingIdentifier(name) = &d.id else {
            continue;
        };
        let body_end = match &d.init {
            Some(ts_ast::Expression::ArrowFunctionExpression(a)) => a.body.span.end,
            Some(ts_ast::Expression::FunctionExpression(f)) => match f.body.as_deref() {
                Some(b) => b.span.end,
                None => continue,
            },
            _ => continue,
        };
        let nm = name.name.to_string();
        out.push(CallDef {
            sym: mint_sym(file, EntityKind::Function, &nm, None),
            name: nm,
            kind: CallKind::Free,
            file: file.to_string(),
            line: line_at(starts, d.span.start as usize),
            end: line_at(starts, body_end as usize),
        });
    }
}

/// Collect every `CallExpression` anywhere in the program (including method and
/// nested bodies); the engine's containment pass attaches each to its caller.
struct TsCallSites<'p> {
    file: &'p str,
    starts: &'p [usize],
    sites: Vec<CallSite>,
}

impl<'a, 'p> OxcVisit<'a> for TsCallSites<'p> {
    fn visit_call_expression(&mut self, c: &ts_ast::CallExpression<'a>) {
        if let Some(callee) = ts_callee_name(&c.callee) {
            self.sites.push(CallSite {
                caller_sym: None,
                callee,
                callee_path: None,
                file: self.file.to_string(),
                line: line_at(self.starts, span_off(&c.callee) as usize),
            });
        }
        oxc_ast_visit::walk::walk_call_expression(self, c);
    }
    // `new Widget(x)` is a call behind the parens: the callee is the constructed
    // type name, so it resolves (via `call_name`) to the class's ctor call_def
    // (whose name is the class name; see `ts_class_call_defs`). Same trailing-
    // segment convention as an ordinary call — `new a.b.C()` -> "C".
    fn visit_new_expression(&mut self, n: &ts_ast::NewExpression<'a>) {
        if let Some(callee) = ts_callee_name(&n.callee) {
            self.sites.push(CallSite {
                caller_sym: None,
                callee,
                callee_path: None,
                file: self.file.to_string(),
                line: line_at(self.starts, n.span.start as usize),
            });
        }
        oxc_ast_visit::walk::walk_new_expression(self, n);
    }
    // `<Card .../>` is a call — jsx(Card, props) — so a component usage is a
    // call site and call_edge resolves caller -> Card like any other callee.
    // Host elements (`<div/>`, lowercase = JSXElementName::Identifier) are
    // skipped at the source: there is no def to resolve to.
    fn visit_jsx_element(&mut self, el: &ts_ast::JSXElement<'a>) {
        use ts_ast::JSXElementName as N;
        let callee = match &el.opening_element.name {
            N::IdentifierReference(r) => Some(r.name.to_string()),
            N::MemberExpression(m) => Some(m.property.name.to_string()),
            _ => None,
        };
        if let Some(callee) = callee {
            self.sites.push(CallSite {
                caller_sym: None,
                callee,
                callee_path: None,
                file: self.file.to_string(),
                line: line_at(self.starts, el.opening_element.span.start as usize),
            });
        }
        oxc_ast_visit::walk::walk_jsx_element(self, el);
    }
}

/// The called name as written: a bare identifier, or the trailing property of a
/// member expression (`a.b.c()` -> "c"), matching the Rust trailing-segment
/// convention. Computed/other callee shapes resolve to nothing.
fn ts_callee_name(e: &ts_ast::Expression) -> Option<String> {
    use ts_ast::Expression as E;
    match e {
        E::Identifier(id) => Some(id.name.to_string()),
        E::StaticMemberExpression(m) => Some(m.property.name.to_string()),
        _ => None,
    }
}

// --- Rust entity pass (syn): structs/enums/unions/traits as data types, free
// functions and impl methods as callables with arrow types. Lines come from
// proc-macro2 span-locations (the `Spanned` ident span). ---

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(
            has(&got, "Entity", "Id", "field"),
            "interface property: {got:?}"
        );
        assert!(
            has(&got, "Catalog", "Pricing", "generic"),
            "interface extends: {got:?}"
        );
        assert!(
            has(&got, "Catalog", "Entity", "generic"),
            "type-param bound: {got:?}"
        );
        assert!(
            has(&got, "Catalog", "Sku", "field"),
            "generic arg in property: {got:?}"
        );
        assert!(
            !got.iter().any(|e| e.to == "T"),
            "type-param name leaked: {got:?}"
        );
        assert!(has(&got, "Repo", "Base", "impl"), "class extends: {got:?}");
        assert!(
            has(&got, "Repo", "Pricing", "impl"),
            "class implements: {got:?}"
        );
        assert!(
            has(&got, "Repo", "Cache", "field"),
            "class property: {got:?}"
        );
        assert!(
            has(&got, "Repo", "Item", "field"),
            "property generic arg: {got:?}"
        );
        assert!(
            has(&got, "Repo", "Db", "field"),
            "ctor parameter property: {got:?}"
        );
        assert!(
            !got.iter().any(|e| e.to == "Wire"),
            "plain ctor arg is not a field: {got:?}"
        );
        assert!(
            has(&got, "Event", "Created", "variant"),
            "union alternative: {got:?}"
        );
        assert!(
            has(&got, "Event", "Deleted", "variant"),
            "generic union alternative: {got:?}"
        );
        assert!(
            has(&got, "Event", "Reason", "field"),
            "union alternative arg: {got:?}"
        );
        assert!(
            has(&got, "Pair", "Left", "field"),
            "tuple alias member: {got:?}"
        );
        assert!(
            has(&got, "Color", "Color::Red", "variant"),
            "enum member: {got:?}"
        );
        assert!(
            has(&got, "Color", "Color::Green", "variant"),
            "initialized enum member: {got:?}"
        );
        assert!(
            !got.iter().any(|e| e.to == "string"),
            "keyword type leaked: {got:?}"
        );
    }

    #[test]
    fn tsx_parses_and_extracts() {
        let src = r#"
interface CardProps { item: Item; onPick: (s: Sku) => void }
export function Card({ item }: CardProps) { return <div>{item.name}</div> }
"#;
        let got = ts_edges(src, true);
        assert!(
            has(&got, "CardProps", "Item", "field"),
            "tsx interface prop: {got:?}"
        );
        assert!(
            has(&got, "CardProps", "Sku", "field"),
            "function-type param ref: {got:?}"
        );
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
        let by = |name: &str| {
            es.iter()
                .find(|e| e.name == name)
                .unwrap_or_else(|| panic!("missing {name}: {es:?}"))
        };
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
        assert_eq!(
            find.parent.as_deref(),
            Some("src/core/model.ts::class::Repo")
        );
        assert_eq!(find.sym, "src/core/model.ts::method::Repo.find");
        // a function IS a type: [...A] => B
        let f = by("resolveIdent").ty.as_ref().unwrap();
        assert_eq!(f.params[0], vec![TypeRef::Named("Model".into())]); // first param type
        assert!(f.params[1].is_empty(), "string is a keyword, no ref: {f:?}");
        assert_eq!(f.ret, vec![TypeRef::Named("NodeId".into())]);
        let a = by("cone").ty.as_ref().unwrap();
        assert_eq!(a.params[1], vec![TypeRef::Named("ConeMode".into())]);
        assert_eq!(a.ret, vec![TypeRef::Named("View".into())]);
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
        assert!(
            has(&got, "resolveIdent", "Model", "param"),
            "fn param type: {got:?}"
        );
        assert!(
            has(&got, "resolveIdent", "NodeId", "returns"),
            "fn return type: {got:?}"
        );
        assert!(
            has(&got, "resolveIdent", "Visited", "uses"),
            "body annotation: {got:?}"
        );
        assert!(
            has(&got, "resolveIdent", "NodeId", "uses"),
            "body cast `as NodeId[]`: {got:?}"
        );
        // arrow const: same three kinds, type-param bound is generic + excluded
        assert!(has(&got, "cone", "Model", "param"), "arrow param: {got:?}");
        assert!(
            has(&got, "cone", "ConeMode", "param"),
            "arrow param 2: {got:?}"
        );
        assert!(
            has(&got, "cone", "View", "returns"),
            "arrow return: {got:?}"
        );
        assert!(
            has(&got, "cone", "Accumulator", "uses"),
            "arrow body: {got:?}"
        );
        assert!(
            has(&got, "cone", "Ctx", "generic"),
            "type-param bound: {got:?}"
        );
        assert!(
            !got.iter().any(|e| e.from == "cone" && e.to == "C"),
            "type-param name leaked: {got:?}"
        );
        // un-exported function still owns edges; keyword param type is no ref
        assert!(
            has(&got, "helper", "Raw", "param"),
            "non-exported fn: {got:?}"
        );
        assert!(
            !got.iter().any(|e| e.to == "string"),
            "keyword param leaked: {got:?}"
        );
    }

    #[test]
    fn ts_string_literal_const_mints_entity_and_value() {
        let src = "const home = '/home';\n";
        let facts = TsTypes.extract("f.ts", src);
        let ent = facts
            .entities
            .iter()
            .find(|e| e.name == "home")
            .expect("const entity");
        assert_eq!(ent.kind, EntityKind::Const);
        let row = facts
            .consts
            .iter()
            .find(|c| c.sym == ent.sym)
            .expect("const_value row");
        assert_eq!(row.field, "");
        assert_eq!(row.text, "/home");
        assert_eq!(row.kind, "lit");
    }

    #[test]
    fn ts_object_literal_const_dotted_field_paths() {
        let src = "const routes = { home: '/home', nested: { a: '/a' } };\n";
        let facts = TsTypes.extract("f.ts", src);
        let ent = facts
            .entities
            .iter()
            .find(|e| e.name == "routes")
            .expect("const entity");
        let by_field = |field: &str| {
            facts
                .consts
                .iter()
                .find(|c| c.sym == ent.sym && c.field == field)
        };
        assert_eq!(by_field("home").expect("home row").text, "/home");
        assert_eq!(by_field("nested.a").expect("nested.a row").text, "/a");
    }

    #[test]
    fn ts_template_const_keeps_holes_and_no_entity_without_strings() {
        let src = "const greeting = `hi ${name}`;\nconst count = 3;\n";
        let facts = TsTypes.extract("f.ts", src);
        let ent = facts
            .entities
            .iter()
            .find(|e| e.name == "greeting")
            .expect("template const entity");
        let row = facts
            .consts
            .iter()
            .find(|c| c.sym == ent.sym)
            .expect("const_value row");
        assert_eq!(row.kind, "template");
        assert_eq!(row.text, "`hi ${name}`");
        // a numeric const gains neither an entity nor a const_value row.
        assert!(
            !facts.entities.iter().any(|e| e.name == "count"),
            "{:?}",
            facts.entities
        );
    }

    #[test]
    fn ts_string_enum_members_key_off_the_enum_sym() {
        let src = "enum Routes { Home = '/home', About = '/about' }\n";
        let facts = TsTypes.extract("f.ts", src);
        let enum_ent = facts
            .entities
            .iter()
            .find(|e| e.name == "Routes")
            .expect("enum entity");
        assert_eq!(enum_ent.kind, EntityKind::Enum);
        let home = facts
            .consts
            .iter()
            .find(|c| c.field == "Home")
            .expect("Home row");
        assert_eq!(home.sym, enum_ent.sym);
        assert_eq!(home.text, "/home");
        let about = facts
            .consts
            .iter()
            .find(|c| c.field == "About")
            .expect("About row");
        assert_eq!(about.sym, enum_ent.sym);
    }

    #[test]
    fn ts_let_var_string_init_excluded_but_as_const_included() {
        let src = "let mutablePath = '/mut';\nconst pinned = '/pin' as const;\n";
        let facts = TsTypes.extract("f.ts", src);
        assert!(
            !facts.entities.iter().any(|e| e.name == "mutablePath"),
            "{:?}",
            facts.entities
        );
        assert!(
            !facts.consts.iter().any(|c| c.text == "/mut"),
            "{:?}",
            facts.consts
        );
        assert_eq!(facts.const_mutable_skips, 1);
        let pinned = facts
            .entities
            .iter()
            .find(|e| e.name == "pinned")
            .expect("as-const entity");
        assert!(facts
            .consts
            .iter()
            .any(|c| c.sym == pinned.sym && c.text == "/pin"));
    }

    #[test]
    fn ts_object_spread_property_counted_not_followed() {
        let src = "const base = { a: '/a' };\nconst merged = { ...base, b: '/b' };\n";
        let facts = TsTypes.extract("f.ts", src);
        let merged = facts
            .entities
            .iter()
            .find(|e| e.name == "merged")
            .expect("merged entity");
        // "b" still lands; the spread contributes no field (nothing named ".." here).
        assert!(facts
            .consts
            .iter()
            .any(|c| c.sym == merged.sym && c.field == "b" && c.text == "/b"));
        assert_eq!(facts.const_spread_skips, 1);
    }

    #[test]
    fn ts_arrow_fn_const_unaffected_by_const_value_pass() {
        // arrow-fn consts stay Function entities (ts_var_fn_entity's job); the
        // const-value pass must not also mint a Const entity for them.
        let src = "const handler = (x: number) => x + 1;\n";
        let facts = TsTypes.extract("f.ts", src);
        let ents: Vec<&TypeEntity> = facts
            .entities
            .iter()
            .filter(|e| e.name == "handler")
            .collect();
        assert_eq!(ents.len(), 1, "{:?}", facts.entities);
        assert_eq!(ents[0].kind, EntityKind::Function);
        assert!(!facts.consts.iter().any(|c| c.sym == ents[0].sym));
    }

    // --- const_string_member retirement: evidence-diff gap fix ---
    // (plans/2026-07-10-string-values-const-value.md follow-up) —
    // const_string_member walked EVERY const declarator with no scope
    // restriction; const_value's original module-level-only loop missed a
    // lookup table declared inside a function body. TsNestedConstWalker
    // closes that gap; these two tests are the evidence.

    #[test]
    fn ts_const_inside_function_body_is_found_and_scoped() {
        let src = "\
function makeTable() {\n    \
    const INNER_TABLE = { x: '/inner/x' };\n    \
    return INNER_TABLE;\n\
}\n";
        let facts = TsTypes.extract("f.ts", src);
        let ent = facts
            .entities
            .iter()
            .find(|e| e.name == "INNER_TABLE")
            .expect("nested const entity");
        assert_eq!(ent.kind, EntityKind::Const);
        assert!(
            ent.sym.contains("makeTable"),
            "sym should carry the enclosing scope: {}",
            ent.sym
        );
        let row = facts
            .consts
            .iter()
            .find(|c| c.sym == ent.sym)
            .expect("const_value row");
        assert_eq!(row.field, "x");
        assert_eq!(row.text, "/inner/x");
    }

    #[test]
    fn ts_same_named_const_in_two_functions_does_not_collide() {
        let src = "\
function a() {\n    \
    const TABLE = { k: '/a' };\n    \
    return TABLE;\n\
}\n\
function b() {\n    \
    const TABLE = { k: '/b' };\n    \
    return TABLE;\n\
}\n";
        let facts = TsTypes.extract("f.ts", src);
        let ents: Vec<&TypeEntity> = facts
            .entities
            .iter()
            .filter(|e| e.name == "TABLE")
            .collect();
        assert_eq!(ents.len(), 2, "{:?}", facts.entities);
        assert_ne!(ents[0].sym, ents[1].sym);
        let text_for = |sym: &str| {
            facts
                .consts
                .iter()
                .find(|c| c.sym == sym && c.field == "k")
                .map(|c| c.text.as_str())
        };
        let texts: Vec<&str> = ents.iter().map(|e| text_for(&e.sym).unwrap()).collect();
        assert!(
            texts.contains(&"/a") && texts.contains(&"/b"),
            "{:?}",
            texts
        );
    }
}
