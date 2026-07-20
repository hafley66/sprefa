//! TS/TSX/JS dataflow lift + JSX walkers (oxc front-end). Pure code
//! motion out of the former single typegraph.rs; zero behavior change.

use oxc_ast::ast as ts_ast;

use super::super::*;

fn ts_push(out: &mut DataflowFacts, file: &str, starts: &[usize], byte_off: u32, kind: &str, var: &str, fn_sym: &str) -> String {
    // kind suffix disambiguates a parent from its first child where spans share
    // a start position (see push_node); (line,col) alone is not unique for
    // `a + 1`. The id is `file:line:col:kind` (uniform with push_node) so the
    // coordinate text reconstructs from (file, line, col, kind) at display time
    // — never interned into `_strings`. col is the 0-based BYTE column within
    // the line (`line_col`); (line,col) is a bijection with byte_off given the
    // file, so this keeps every id distinct exactly as `byte_off:kind` did.
    let (line, col) = line_col(starts, byte_off as usize);
    let id = format!("{file}:{line}:{col}:{kind}");
    out.nodes.push(DfNode {
        id: id.clone(),
        kind: kind.into(),
        var: var.into(),
        fn_sym: fn_sym.into(),
        file: file.into(),
        line,
        col,
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

/// Seed a fn's param nodes into the scope. A bare identifier binds as itself.
/// An object-destructuring param (`{title, count: n}` — the React props shape)
/// mints one param node PER property: var carries the PROPERTY name (what a
/// caller's df_field prop row matches by name), while the scope binds the
/// LOCAL name (they differ under `key: renamed`). Every piece shares the
/// slot's positional index, so the positional arg->param hop fans the incoming
/// object into each piece — the conservative read of destructuring.
fn ts_seed_params(
    params: &ts_ast::FormalParameters,
    file: &str,
    starts: &[usize],
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) {
    for (pos, p) in params.items.iter().enumerate() {
        match &p.pattern {
            ts_ast::BindingPattern::BindingIdentifier(b) => {
                let id = ts_push(out, file, starts, p.span.start, "param", &b.name, fn_sym);
                out.param_pos.push((id.clone(), pos as u32));
                scope.insert(b.name.to_string(), id);
            }
            ts_ast::BindingPattern::ObjectPattern(op) => {
                for prop in &op.properties {
                    if let ts_ast::BindingPattern::BindingIdentifier(b) = &prop.value {
                        let key = match &prop.key {
                            ts_ast::PropertyKey::StaticIdentifier(i) => i.name.to_string(),
                            ts_ast::PropertyKey::StringLiteral(s) => s.value.to_string(),
                            _ => b.name.to_string(),
                        };
                        let id = ts_push(out, file, starts, b.span.start, "param", &key, fn_sym);
                        out.param_pos.push((id.clone(), pos as u32));
                        scope.insert(b.name.to_string(), id);
                    }
                }
                if let Some(rest) = &op.rest {
                    if let ts_ast::BindingPattern::BindingIdentifier(b) = &rest.argument {
                        let id = ts_push(out, file, starts, b.span.start, "param", &b.name, fn_sym);
                        out.param_pos.push((id.clone(), pos as u32));
                        scope.insert(b.name.to_string(), id);
                    }
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn ts_dataflow_from(program: &ts_ast::Program, file: &str, content: &str) -> DataflowFacts {
    let starts = line_index(content);
    let mut out = DataflowFacts::default();
    for stmt in &program.body {
        ts_flow_stmt(stmt, file, &starts, &mut out);
    }
    out.nests = compute_nests(&out.nodes, &out.loops);
    // Resolve the pending template/concat spans into raw source-slice text —
    // the one place that already holds `content`, so no function between here
    // and the per-node lift needs it threaded through.
    for (id, start, end, kind) in out.lit_spans.drain(..) {
        let text = content.get(start as usize..end as usize).unwrap_or_default().to_string();
        out.lits.push((id, text, kind));
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
                ts_seed_params(&f.params, file, starts, &fn_sym, &mut scope, out);
                ts_flow_body(body, file, starts, &fn_sym, &mut scope, out);
            }
        }
        S::ExportNamedDeclaration(e) => {
            if let Some(d) = &e.declaration {
                ts_flow_decl(d, file, starts, out);
            }
        }
        S::ClassDeclaration(c) => ts_flow_class(c, file, starts, out),
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
                ts_seed_params(&f.params, file, starts, &fn_sym, &mut scope, out);
                ts_flow_body(body, file, starts, &fn_sym, &mut scope, out);
            }
        }
        D::ClassDeclaration(c) => ts_flow_class(c, file, starts, out),
        _ => {}
    }
}

/// `class Owner { ... }`: each method body (instance, static, getter, setter,
/// constructor) flows like a free function's, scoped under the same
/// `Owner.method` sym `ts_class_call_defs`/`ts_class_entity` already mint for
/// the method — so an interprocedural hop lands on the node this walk wrote.
/// Field initializers are not covered: a `PropertyDefinition`'s init
/// expression has no natural enclosing fn scope to attach nodes to (see
/// docs/df-coverage.md).
fn ts_flow_class(c: &ts_ast::Class, file: &str, starts: &[usize], out: &mut DataflowFacts) {
    let Some(id) = &c.id else { return };
    let owner = id.name.to_string();
    for el in &c.body.body {
        let ts_ast::ClassElement::MethodDefinition(m) = el else { continue };
        let ts_ast::PropertyKey::StaticIdentifier(k) = &m.key else { continue };
        let Some(body) = m.value.body.as_deref() else { continue };
        let method_name = k.name.to_string();
        let fn_sym = mint_sym(file, EntityKind::Method, &method_name, Some(&owner));
        let mut scope: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        ts_seed_params(&m.value.params, file, starts, &fn_sym, &mut scope, out);
        ts_flow_body(body, file, starts, &fn_sym, &mut scope, out);
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

/// Lift a function value (arrow or function expression) as its own fn scope:
/// seed param nodes, then walk the body. For an expression-body arrow
/// (`(x) => expr`, `expression == true`) oxc wraps the expr as a single
/// ExpressionStatement — that is the implicit return, so it flows into a `ret`
/// node. Block bodies handle returns via the ReturnStatement arm.
fn ts_lift_fn(
    params: &ts_ast::FormalParameters,
    body: &ts_ast::FunctionBody,
    expression: bool,
    fn_sym: &str,
    file: &str,
    starts: &[usize],
    out: &mut DataflowFacts,
) {
    let mut scope: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    ts_seed_params(params, file, starts, fn_sym, &mut scope, out);
    if expression {
        if let Some(ts_ast::Statement::ExpressionStatement(es)) = body.statements.first() {
            let v = ts_flow_expr(&es.expression, file, starts, fn_sym, &mut scope, out);
            let ret = ts_push(out, file, starts, es.span.start, "ret", "", fn_sym);
            out.edges.push(DfEdge { from: v, to: ret });
        }
    } else {
        for stmt in &body.statements {
            ts_flow_body_stmt(stmt, file, starts, fn_sym, &mut scope, out);
        }
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
                // A const-bound arrow / function expression is a function
                // definition, not a value: lift it as its own fn scope (params +
                // body + ret) keyed by the binding name, so its params and
                // returns join the interprocedural graph like a top-level fn.
                if let ts_ast::BindingPattern::BindingIdentifier(bn) = &d.id {
                    match &d.init {
                        Some(ts_ast::Expression::ArrowFunctionExpression(a)) => {
                            let sym = mint_sym(file, EntityKind::Function, &bn.name, None);
                            ts_lift_fn(&a.params, &a.body, a.expression, &sym, file, starts, out);
                            continue;
                        }
                        Some(ts_ast::Expression::FunctionExpression(f)) => {
                            if let Some(body) = f.body.as_deref() {
                                let sym = mint_sym(file, EntityKind::Function, &bn.name, None);
                                ts_lift_fn(&f.params, body, false, &sym, file, starts, out);
                            }
                            continue;
                        }
                        _ => {}
                    }
                }
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
        // `return EXPR`: the returned value flows into the fn's `ret` node — the
        // sink the interprocedural backward hop reads. (Arrow expression-body
        // returns, `(x) => expr`, are not yet lifted; explicit return only.)
        S::ReturnStatement(r) => {
            let id = ts_push(out, file, starts, r.span.start, "ret", "", fn_sym);
            if let Some(arg) = &r.argument {
                let v = ts_flow_expr(arg, file, starts, fn_sym, scope, out);
                out.edges.push(DfEdge { from: v, to: id });
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
/// `f(args)` / `recv.m(args)`: each argument flows into the call result, with
/// `df_arg` recording its 0-based slot for the positional interprocedural hop.
/// A member callee flows its receiver in at slot -1; a bare callee is the
/// target, not a value in, so it is skipped. Shared by the plain-call arm and
/// the optional-chained-call (`recv?.m()`) arm.
fn ts_flow_call(
    c: &ts_ast::CallExpression,
    off: u32,
    file: &str,
    starts: &[usize],
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) -> String {
    use ts_ast::Expression as E;
    let recv = match &c.callee {
        E::StaticMemberExpression(m) => Some(ts_flow_expr(&m.object, file, starts, fn_sym, scope, out)),
        E::ComputedMemberExpression(m) => Some(ts_flow_expr(&m.object, file, starts, fn_sym, scope, out)),
        _ => None,
    };
    let mut child_ids = Vec::new();
    for arg in &c.arguments {
        if let Some(id) = arg.as_expression() {
            child_ids.push(ts_flow_expr(id, file, starts, fn_sym, scope, out));
        }
    }
    let id = ts_push(out, file, starts, off, "call_res", "", fn_sym);
    if let Some(r) = recv {
        out.edges.push(DfEdge { from: r.clone(), to: id.clone() });
        out.args.push((id.clone(), -1, r));
    }
    for (pos, cid) in child_ids.into_iter().enumerate() {
        out.edges.push(DfEdge { from: cid.clone(), to: id.clone() });
        out.args.push((id.clone(), pos as i64, cid));
    }
    id
}

/// `recv.prop` / `recv?.prop` / `recv[expr]`: the receiver flows into a
/// `member` node whose var is the accessed name (empty for a computed access),
/// so a `df_field` write of the same field name matches the read. Shared by
/// the static/computed member arms and the optional-chained member arm.
fn ts_flow_member(
    object: &ts_ast::Expression,
    prop: &str,
    off: u32,
    file: &str,
    starts: &[usize],
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) -> String {
    let obj = ts_flow_expr(object, file, starts, fn_sym, scope, out);
    let id = ts_push(out, file, starts, off, "member", prop, fn_sym);
    out.edges.push(DfEdge { from: obj, to: id.clone() });
    id
}

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
        // A string literal carries its cooked value into `df_lit` — the only
        // literal kind that does (numbers/bools/regex stay textless `lit`
        // nodes, same as before; bounded rows, and strings are the use case).
        E::StringLiteral(s) => {
            let id = ts_push(out, file, starts, off, "lit", "", fn_sym);
            out.lits.push((id.clone(), s.value.to_string(), "lit"));
            id
        }
        E::NumericLiteral(_)
        | E::BooleanLiteral(_)
        | E::NullLiteral(_)
        | E::BigIntLiteral(_)
        | E::RegExpLiteral(_) => ts_push(out, file, starts, off, "lit", "", fn_sym),
        // f(args): each argument flows into the call result, with `df_arg`
        // recording its 0-based slot for the positional interprocedural hop.
        // A member callee `recv.m(a)` flows the receiver in at slot -1; a bare
        // callee is the target, not a value in, so it is skipped.
        E::CallExpression(c) => ts_flow_call(c, off, file, starts, fn_sym, scope, out),
        // `new Foo(args)`: an instantiation — a `new` node carrying the class
        // name, args recorded positionally like a call.
        E::NewExpression(n) => {
            let ty = match &n.callee {
                E::Identifier(i) => i.name.to_string(),
                E::StaticMemberExpression(m) => m.property.name.to_string(),
                _ => String::new(),
            };
            let mut child_ids = Vec::new();
            for arg in &n.arguments {
                if let Some(a) = arg.as_expression() {
                    child_ids.push(ts_flow_expr(a, file, starts, fn_sym, scope, out));
                }
            }
            let id = ts_push(out, file, starts, off, "new", &ty, fn_sym);
            for (pos, cid) in child_ids.into_iter().enumerate() {
                out.edges.push(DfEdge { from: cid.clone(), to: id.clone() });
                out.args.push((id.clone(), pos as i64, cid));
            }
            id
        }
        // `{ a: x, ...rest }`: the JS instantiation. Each property value flows
        // into an anonymous `new` node and `df_field` records the property
        // name; a spread flows in under the pseudo-field ".." (mirroring
        // Rust's functional-update base).
        E::ObjectExpression(o) => {
            let mut filled: Vec<(String, String)> = Vec::new();
            for prop in &o.properties {
                match prop {
                    ts_ast::ObjectPropertyKind::ObjectProperty(p) => {
                        let v = ts_flow_expr(&p.value, file, starts, fn_sym, scope, out);
                        let name = match &p.key {
                            ts_ast::PropertyKey::StaticIdentifier(i) => i.name.to_string(),
                            ts_ast::PropertyKey::StringLiteral(s) => s.value.to_string(),
                            _ => String::new(),
                        };
                        filled.push((name, v));
                    }
                    ts_ast::ObjectPropertyKind::SpreadProperty(sp) => {
                        let v = ts_flow_expr(&sp.argument, file, starts, fn_sym, scope, out);
                        filled.push(("..".into(), v));
                    }
                }
            }
            let id = ts_push(out, file, starts, off, "new", "", fn_sym);
            for (name, v) in filled {
                out.edges.push(DfEdge { from: v.clone(), to: id.clone() });
                if !name.is_empty() {
                    out.fields.push((id.clone(), name, v));
                }
            }
            id
        }
        // `<Card title={t} {...rest}>{kids}</Card>`: JSX is a call in costume —
        // jsx(Card, {title: t, ...rest, children: kids}) — so an element lifts
        // exactly like an instantiation: a `new` node carrying the component/
        // tag name, each attribute a df_field row (spread under ".."), children
        // under the "children" pseudo-prop React actually passes.
        E::JSXElement(el) => ts_flow_jsx_element(el, file, starts, fn_sym, scope, out),
        E::JSXFragment(fr) => ts_flow_jsx_fragment(fr, file, starts, fn_sym, scope, out),
        // recv.prop / recv[prop]: the receiver flows through into a `member`
        // node; a static property records its name so a `df_field` write can
        // be matched against the read of the same field. oxc flattens
        // MemberExpression into StaticMemberExpression / ComputedMemberExpression.
        E::StaticMemberExpression(m) => ts_flow_member(&m.object, &m.property.name, off, file, starts, fn_sym, scope, out),
        E::ComputedMemberExpression(m) => ts_flow_member(&m.object, "", off, file, starts, fn_sym, scope, out),
        // `a + b`: its own `concat` kind (not `binop`) so a query for string
        // construction can match `kind IN (template, concat)` explicitly, the
        // same shape a TemplateLiteral mints. `+` also qualifies for numeric
        // addition — the kind is a syntactic label (any-operand `+` is real
        // value flow either way), not a type judgment; `df_lit`'s row for it
        // carries the written source (holes intact, like a template), which a
        // downstream string-flow query is free to treat as advisory.
        E::BinaryExpression(b) if b.operator == ts_ast::BinaryOperator::Addition => {
            let l = ts_flow_expr(&b.left, file, starts, fn_sym, scope, out);
            let r = ts_flow_expr(&b.right, file, starts, fn_sym, scope, out);
            let id = ts_push(out, file, starts, off, "concat", "", fn_sym);
            out.edges.push(DfEdge { from: l, to: id.clone() });
            out.edges.push(DfEdge { from: r, to: id.clone() });
            out.lit_spans.push((id.clone(), b.span.start, b.span.end, "concat"));
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
        // An INLINE lambda (`xs.map((x) => x + 1)`, a function-expression
        // argument): lift it as its own fn scope — params + body + ret under a
        // synthetic `<enclosing>::closure::<off>` sym — and mint the `closure`
        // VALUE node here, carrying that sym in `var`. The value node is what
        // df_arg records; the sym is the join key a higher-order hop (see
        // std/flow.dl flow_lambda) uses to feed the lifted params and read the
        // lifted ret. (Fresh inner scope: captures were already a hole for
        // inline lambdas — the old catch-all didn't walk the body at all.)
        E::ArrowFunctionExpression(a) => {
            let lam_sym = lambda_sym(fn_sym, &off.to_string());
            ts_lift_fn(&a.params, &a.body, a.expression, &lam_sym, file, starts, out);
            ts_push(out, file, starts, off, "closure", &lam_sym, fn_sym)
        }
        E::FunctionExpression(f) => match f.body.as_deref() {
            Some(body) => {
                let lam_sym = lambda_sym(fn_sym, &off.to_string());
                ts_lift_fn(&f.params, body, false, &lam_sym, file, starts, out);
                ts_push(out, file, starts, off, "closure", &lam_sym, fn_sym)
            }
            None => ts_push(out, file, starts, off, "expr", "", fn_sym),
        },
        // `(value)`: parens are preserved in the oxc AST (preserve_parens); the
        // value is exactly the inner expression, so pass it through with no
        // node of our own. Without this a parenthesized prop value
        // (`prop={(cond ? a : b)}`) dead-ends at an unlinked `expr` node.
        E::ParenthesizedExpression(p) => ts_flow_expr(&p.expression, file, starts, fn_sym, scope, out),
        // `x as T`, `x satisfies T`, `x!`, `await x`: type-level / effect
        // wrappers that are transparent to the runtime value — flow the inner
        // expression straight through.
        E::TSAsExpression(t) => ts_flow_expr(&t.expression, file, starts, fn_sym, scope, out),
        E::TSSatisfiesExpression(t) => ts_flow_expr(&t.expression, file, starts, fn_sym, scope, out),
        E::TSNonNullExpression(t) => ts_flow_expr(&t.expression, file, starts, fn_sym, scope, out),
        E::AwaitExpression(a) => ts_flow_expr(&a.argument, file, starts, fn_sym, scope, out),
        E::TSTypeAssertion(t) => ts_flow_expr(&t.expression, file, starts, fn_sym, scope, out),
        E::TSInstantiationExpression(t) => ts_flow_expr(&t.expression, file, starts, fn_sym, scope, out),
        // `obj?.title`, `handlers?.save()`: optional chaining wraps a member or
        // call. It is transparent to the value — flow the underlying access the
        // same way its unwrapped form would. `title={obj?.title}` is a routine
        // prop shape the catch-all otherwise dropped.
        E::ChainExpression(ch) => {
            use ts_ast::ChainElement as CE;
            use ts_ast::MemberExpression as ME;
            match &ch.expression {
                CE::CallExpression(c) => ts_flow_call(c, off, file, starts, fn_sym, scope, out),
                other => match other.member_expression() {
                    Some(ME::StaticMemberExpression(m)) => ts_flow_member(&m.object, &m.property.name, off, file, starts, fn_sym, scope, out),
                    Some(ME::ComputedMemberExpression(m)) => ts_flow_member(&m.object, "", off, file, starts, fn_sym, scope, out),
                    Some(ME::PrivateFieldExpression(m)) => ts_flow_member(&m.object, "", off, file, starts, fn_sym, scope, out),
                    None => ts_push(out, file, starts, off, "expr", "", fn_sym),
                },
            }
        }
        // `x = y` as a value: the expression evaluates to the assigned value.
        E::AssignmentExpression(a) => ts_flow_expr(&a.right, file, starts, fn_sym, scope, out),
        // `[a, b, ...rest]`: a list value. Each element flows into an array
        // `new` node (spread under ".."), so `items={[first, second]}` carries
        // both elements. Holes in a sparse array carry nothing.
        E::ArrayExpression(arr) => {
            let mut child_ids: Vec<(String, String)> = Vec::new();
            for el in &arr.elements {
                match el {
                    ts_ast::ArrayExpressionElement::SpreadElement(sp) => {
                        let v = ts_flow_expr(&sp.argument, file, starts, fn_sym, scope, out);
                        child_ids.push(("..".into(), v));
                    }
                    ts_ast::ArrayExpressionElement::Elision(_) => {}
                    _ => {
                        if let Some(e) = el.as_expression() {
                            let v = ts_flow_expr(e, file, starts, fn_sym, scope, out);
                            child_ids.push((String::new(), v));
                        }
                    }
                }
            }
            let id = ts_push(out, file, starts, off, "new", "", fn_sym);
            for (name, v) in child_ids {
                out.edges.push(DfEdge { from: v.clone(), to: id.clone() });
                if !name.is_empty() {
                    out.fields.push((id.clone(), name, v));
                }
            }
            id
        }
        // `test ? consequent : alternate`: the value is EITHER branch, so both
        // flow into a `cond` node; `test` is a guard (walked for its own nested
        // facts — a call in the test still records — but never edged in as a
        // value). This is the common JSX prop shape `prop={ok ? a : b}`.
        E::ConditionalExpression(c) => {
            let _test = ts_flow_expr(&c.test, file, starts, fn_sym, scope, out);
            let cons = ts_flow_expr(&c.consequent, file, starts, fn_sym, scope, out);
            let alt = ts_flow_expr(&c.alternate, file, starts, fn_sym, scope, out);
            let id = ts_push(out, file, starts, off, "cond", "", fn_sym);
            out.edges.push(DfEdge { from: cons, to: id.clone() });
            out.edges.push(DfEdge { from: alt, to: id.clone() });
            id
        }
        // `left && right`, `left || right`, `left ?? right`: short-circuit
        // logic. For `&&` the value is `right` (left is a truthiness guard); for
        // `||` / `??` the value is EITHER operand. `cond && <Foo/>` and
        // `value ?? fallback` are both routine prop shapes. Walk the guard for
        // its nested facts even when it isn't edged in.
        E::LogicalExpression(b) => {
            use ts_ast::LogicalOperator as Op;
            let l = ts_flow_expr(&b.left, file, starts, fn_sym, scope, out);
            let r = ts_flow_expr(&b.right, file, starts, fn_sym, scope, out);
            let id = ts_push(out, file, starts, off, "logic", "", fn_sym);
            if matches!(b.operator, Op::Or | Op::Coalesce) {
                out.edges.push(DfEdge { from: l, to: id.clone() });
            }
            out.edges.push(DfEdge { from: r, to: id.clone() });
            id
        }
        // `(a, b, c)`: the value is the LAST expression; earlier ones are
        // evaluated for effect (walked, not edged in).
        E::SequenceExpression(s) => {
            let mut last = ts_push(out, file, starts, off, "expr", "", fn_sym);
            for sub in &s.expressions {
                last = ts_flow_expr(sub, file, starts, fn_sym, scope, out);
            }
            last
        }
        // `` `hello ${name}, you have ${count}` ``: a string built from its
        // interpolations — each `${...}` value flows into a `template` node,
        // the same shape as a concatenation. `title={`Hi ${secret}`}` then
        // carries `secret` into the prop.
        E::TemplateLiteral(t) => {
            let id = ts_push(out, file, starts, off, "template", "", fn_sym);
            for sub in &t.expressions {
                let v = ts_flow_expr(sub, file, starts, fn_sym, scope, out);
                out.edges.push(DfEdge { from: v, to: id.clone() });
            }
            out.lit_spans.push((id.clone(), t.span.start, t.span.end, "template"));
            id
        }
        // `` styled.div`color: ${c}` ``, `` sql`... ${id}` ``: a call in tagged
        // costume — tag(quasis, ...exprs). The tag can transform, but the
        // conservative value carries each interpolation through, matching the
        // plain-template treatment. The tag itself is walked for its own facts.
        E::TaggedTemplateExpression(t) => {
            let _tag = ts_flow_expr(&t.tag, file, starts, fn_sym, scope, out);
            let id = ts_push(out, file, starts, off, "template", "", fn_sym);
            for sub in &t.quasi.expressions {
                let v = ts_flow_expr(sub, file, starts, fn_sym, scope, out);
                out.edges.push(DfEdge { from: v, to: id.clone() });
            }
            // `t.quasi` is the TemplateLiteral portion (the tag itself is not
            // part of the string source); its span excludes the tag prefix.
            out.lit_spans.push((id.clone(), t.quasi.span.start, t.quasi.span.end, "template"));
            id
        }
        // template strings, control flow, remaining variants: mint a node,
        // don't chase. Conservative — may miss, never invents.
        _ => ts_push(out, file, starts, off, "expr", "", fn_sym),
    }
}

/// Byte offset of an expression's span start. oxc nodes expose their span via
/// the matched inner struct; the Expression enum carries a `.span()` through
/// the GetSpan impl, which we reach via this thin shim.
pub(crate) fn span_off(e: &ts_ast::Expression) -> u32 {
    use oxc_span::GetSpan;
    e.span().start
}

/// The element's name as written: `<div/>` -> "div" (host element),
/// `<Card/>` -> "Card" (component), `<Foo.Bar/>` -> "Bar" (trailing property,
/// matching the callee-name convention), `<ns:tag/>` -> the tag part.
fn ts_jsx_name(n: &ts_ast::JSXElementName) -> String {
    use ts_ast::JSXElementName as N;
    match n {
        N::Identifier(i) => i.name.to_string(),
        N::IdentifierReference(r) => r.name.to_string(),
        N::MemberExpression(m) => m.property.name.to_string(),
        N::NamespacedName(ns) => ns.name.name.to_string(),
        N::ThisExpression(_) => String::new(),
    }
}

/// A JSX element is `jsx(Name, {props..., children})`: lift it as a `new`
/// node carrying the component/tag name, each attribute as a df_field row
/// (a bare boolean prop `<Foo flag/>` fills with a lit — it IS `true` — and
/// a spread `{...rest}` lands under ".." like an object spread), and each
/// non-text child under the "children" pseudo-prop React actually passes.
fn ts_flow_jsx_element(
    el: &ts_ast::JSXElement,
    file: &str,
    starts: &[usize],
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) -> String {
    let comp = ts_jsx_name(&el.opening_element.name);
    let mut filled: Vec<(String, String)> = Vec::new();
    for attr in &el.opening_element.attributes {
        match attr {
            ts_ast::JSXAttributeItem::Attribute(a) => {
                let name = match &a.name {
                    ts_ast::JSXAttributeName::Identifier(i) => i.name.to_string(),
                    ts_ast::JSXAttributeName::NamespacedName(ns) => ns.name.name.to_string(),
                };
                let v = match &a.value {
                    None => ts_push(out, file, starts, a.span.start, "lit", "", fn_sym),
                    Some(ts_ast::JSXAttributeValue::StringLiteral(s)) => {
                        ts_push(out, file, starts, s.span.start, "lit", "", fn_sym)
                    }
                    Some(ts_ast::JSXAttributeValue::ExpressionContainer(c)) => {
                        match c.expression.as_expression() {
                            Some(e) => ts_flow_expr(e, file, starts, fn_sym, scope, out),
                            None => continue, // empty container `{}` carries no value
                        }
                    }
                    Some(ts_ast::JSXAttributeValue::Element(child)) => {
                        ts_flow_jsx_element(child, file, starts, fn_sym, scope, out)
                    }
                    Some(ts_ast::JSXAttributeValue::Fragment(fr)) => {
                        ts_flow_jsx_fragment(fr, file, starts, fn_sym, scope, out)
                    }
                };
                filled.push((name, v));
            }
            ts_ast::JSXAttributeItem::SpreadAttribute(sp) => {
                let v = ts_flow_expr(&sp.argument, file, starts, fn_sym, scope, out);
                filled.push(("..".into(), v));
            }
        }
    }
    ts_flow_jsx_children(&el.children, file, starts, fn_sym, scope, out, &mut filled);
    let id = ts_push(out, file, starts, el.span.start, "new", &comp, fn_sym);
    for (name, v) in filled {
        out.edges.push(DfEdge { from: v.clone(), to: id.clone() });
        out.fields.push((id.clone(), name, v));
    }
    id
}

/// `<>...</>`: an anonymous element — children only.
fn ts_flow_jsx_fragment(
    fr: &ts_ast::JSXFragment,
    file: &str,
    starts: &[usize],
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) -> String {
    let mut filled: Vec<(String, String)> = Vec::new();
    ts_flow_jsx_children(&fr.children, file, starts, fn_sym, scope, out, &mut filled);
    let id = ts_push(out, file, starts, fr.span.start, "new", "", fn_sym);
    for (name, v) in filled {
        out.edges.push(DfEdge { from: v.clone(), to: id.clone() });
        out.fields.push((id.clone(), name, v));
    }
    id
}

/// Non-text children flow into the parent element under the "children"
/// pseudo-prop (that is the prop React passes); a spread child under "..".
fn ts_flow_jsx_children(
    children: &[ts_ast::JSXChild],
    file: &str,
    starts: &[usize],
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
    filled: &mut Vec<(String, String)>,
) {
    for ch in children {
        match ch {
            ts_ast::JSXChild::Element(el) => {
                filled.push(("children".into(), ts_flow_jsx_element(el, file, starts, fn_sym, scope, out)));
            }
            ts_ast::JSXChild::Fragment(fr) => {
                filled.push(("children".into(), ts_flow_jsx_fragment(fr, file, starts, fn_sym, scope, out)));
            }
            ts_ast::JSXChild::ExpressionContainer(c) => {
                if let Some(e) = c.expression.as_expression() {
                    filled.push(("children".into(), ts_flow_expr(e, file, starts, fn_sym, scope, out)));
                }
            }
            ts_ast::JSXChild::Spread(sp) => {
                filled.push(("..".into(), ts_flow_expr(&sp.expression, file, starts, fn_sym, scope, out)));
            }
            ts_ast::JSXChild::Text(_) => {}
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tsx_lift_jsx_elements_props_children() {
        let src = "function go(t: number) {\n    \
                       const el = <Card title={t} flag {...rest}><Item/></Card>;\n    \
                       const frag = <>{t}</>;\n\
                   }\n";
        let df = TsTypes.extract_dataflow("f.tsx", src);

        // the element is a `new` node carrying the component name.
        let card = dnode(&df, "new", "Card").id.clone();
        // title={t}: the var_read flows in under the prop name.
        let t_reads: Vec<&DfNode> = df.nodes.iter().filter(|n| n.kind == "var_read" && n.var == "t").collect();
        assert!(t_reads.iter().any(|t| has_field(&df, &card, "title", &t.id)), "{:?}", df.fields);
        // bare boolean prop fills with a lit; spread lands under "..".
        assert!(df.fields.iter().any(|(i, f, _)| i == &card && f == "flag"), "{:?}", df.fields);
        assert!(df.fields.iter().any(|(i, f, _)| i == &card && f == ".."), "{:?}", df.fields);
        // the child element fills the "children" pseudo-prop.
        let item = dnode(&df, "new", "Item").id.clone();
        assert!(has_field(&df, &card, "children", &item), "{:?}", df.fields);
        // a fragment is an anonymous element whose children flow in.
        let frag = df.nodes.iter().find(|n| n.kind == "new" && n.var.is_empty()).expect("fragment new node");
        assert!(df.fields.iter().any(|(i, f, _)| i == &frag.id && f == "children"), "{:?}", df.fields);
    }

    #[test]
    fn ts_destructured_params_bind_by_prop_name() {
        let src = "function card({title, count: n}: any, plain: number) {\n    \
                       const a = title;\n    \
                       const b = n;\n    \
                       const c = plain;\n\
                   }\n";
        let df = TsTypes.extract_dataflow("f.ts", src);

        // one param node per property, var = the PROPERTY name (what a JSX
        // prop row matches), even when the local binding is renamed.
        let title = dnode(&df, "param", "title").id.clone();
        let count = dnode(&df, "param", "count").id.clone();
        let plain = dnode(&df, "param", "plain").id.clone();
        // scope binds the LOCAL names: reads of title/n/plain edge from them.
        let read_of = |v: &str| df.nodes.iter().find(|n| n.kind == "var_read" && n.var == v).unwrap().id.clone();
        assert!(df.edges.iter().any(|e| e.from == title && e.to == read_of("title")), "{:?}", df.edges);
        assert!(df.edges.iter().any(|e| e.from == count && e.to == read_of("n")), "{:?}", df.edges);
        assert!(df.edges.iter().any(|e| e.from == plain && e.to == read_of("plain")), "{:?}", df.edges);
        // both destructured pieces share slot 0; plain is slot 1.
        let pos_of = |id: &str| df.param_pos.iter().find(|(i, _)| i == id).map(|(_, p)| *p);
        assert_eq!(pos_of(&title), Some(0));
        assert_eq!(pos_of(&count), Some(0));
        assert_eq!(pos_of(&plain), Some(1));
    }

    #[test]
    fn ts_lift_new_object_literal_and_members() {
        let src = "function go(h: number): void {\n    \
                       const w = new Widget(h);\n    \
                       const c = { host: h, port: 1 };\n    \
                       const x = c.host;\n    \
                       const n = x.toFixed(2);\n\
                   }\n";
        let df = TsTypes.extract_dataflow("f.ts", src);

        // `new Widget(h)`: a `new` node with the class name and a slot-0 arg.
        let widget = dnode(&df, "new", "Widget").id.clone();
        let h_reads: Vec<&DfNode> = df.nodes.iter().filter(|n| n.kind == "var_read" && n.var == "h").collect();
        assert!(h_reads.iter().any(|h| has_arg(&df, &widget, 0, &h.id)), "{:?}", df.args);
        // object literal: anonymous `new` with named property fills.
        let obj = df.nodes.iter().find(|n| n.kind == "new" && n.var.is_empty()).expect("object literal new node");
        assert!(h_reads.iter().any(|h| has_field(&df, &obj.id, "host", &h.id)), "{:?}", df.fields);
        assert!(df.fields.iter().any(|(i, f, _)| i == &obj.id && f == "port"), "{:?}", df.fields);
        // `.host` member read carries the property name.
        let member = dnode(&df, "member", "host");
        assert!(df.edges.iter().any(|e| e.to == member.id), "member has a base edge");
        // method receiver at slot -1: x.toFixed(2).
        assert!(
            df.args.iter().any(|(_, p, a)| *p == -1
                && df.nodes.iter().any(|n| &n.id == a && n.kind == "var_read" && n.var == "x")),
            "{:?}", df.args
        );
    }

    #[test]
    fn ts_inline_arrow_lifts_as_own_scope() {
        let src = "function go(xs: number[]): void {\n    const out = xs.map((x) => x + 1);\n}\n";
        let df = TsTypes.extract_dataflow("f.ts", src);
        assert_lambda_lifted(&df, 0, "x");
        // a function expression lifts too.
        let src2 = "function go(xs: number[]): void {\n    const out = xs.map(function (x) { return x + 1; });\n}\n";
        let df2 = TsTypes.extract_dataflow("f.ts", src2);
        assert_lambda_lifted(&df2, 0, "x");
    }

    #[test]
    fn ts_df_lit_carries_cooked_string_and_template_holes() {
        let src = "function build(name: string) {\n    \
                       const a = 'plain';\n    \
                       const b = `hi ${name}`;\n\
                   }\n";
        let df = TsTypes.extract_dataflow("f.ts", src);
        assert!(df.lits.iter().any(|(_, text, kind)| text == "plain" && *kind == "lit"), "{:?}", df.lits);
        assert!(
            df.lits.iter().any(|(_, text, kind)| text == "`hi ${name}`" && *kind == "template"),
            "{:?}", df.lits
        );
        // no leftover pending spans after resolution.
        assert!(df.lit_spans.is_empty());
    }

    #[test]
    fn ts_concat_binop_mints_own_kind_and_edges_both_operands() {
        let src = "function url(base: string) {\n    \
                       const full = base + '/x';\n\
                   }\n";
        let df = TsTypes.extract_dataflow("f.ts", src);
        let concat = df.nodes.iter().find(|n| n.kind == "concat").expect("concat node");
        // both operands flow into it: the base var_read and the string lit.
        let base_read = df.nodes.iter().find(|n| n.kind == "var_read" && n.var == "base").expect("base read");
        let lit = df.nodes.iter().find(|n| n.kind == "lit").expect("lit node");
        assert!(df.edges.iter().any(|e| e.from == base_read.id && e.to == concat.id), "{:?}", df.edges);
        assert!(df.edges.iter().any(|e| e.from == lit.id && e.to == concat.id), "{:?}", df.edges);
        // the concat's df_lit row carries the written source, holes intact
        // (here: no interpolation holes, just the plain `+` text).
        assert!(
            df.lits.iter().any(|(id, text, kind)| id == &concat.id && text == "base + '/x'" && *kind == "concat"),
            "{:?}", df.lits
        );
        // a non-`+` binary op stays the old "binop" kind, untouched.
        let other_src = "function cmp(a: number, b: number) { const c = a - b; }\n";
        let other = TsTypes.extract_dataflow("f.ts", other_src);
        assert!(other.nodes.iter().any(|n| n.kind == "binop"), "{:?}", other.nodes);
        assert!(!other.nodes.iter().any(|n| n.kind == "concat"), "{:?}", other.nodes);
    }
}
