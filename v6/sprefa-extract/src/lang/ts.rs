//! The oxc Parser + the TypeF projection (TS/JS type entities + their arrow-type
//! signatures). oxc is the arena AST front-end v5 carried
//! (`src/graph/typegraph/ts`). `Program<'a>` borrows the `Allocator` AND the
//! source text, so the dispatch owns the arena and the parse ties `content` to
//! `'a` (the GAT seam from commit 2a).
//!
//! Commit 2b ports v5 `ts_entities_from`: the type declarations (class /
//! interface / alias / enum / function / method) become span-addressed
//! `Node<TypeF>`. Commit 2c ports the arrow-type half of v5
//! `ts_fn_signature_edges`: each callable's param + return type references
//! become `TypeSig` rows in the bundle's aux (the D-arrow-type payload: a
//! function IS a type, `[...A] => B`). The target stays a bare name (phase-1
//! honest; `Resolve<TypeF>` binds it to a declaration span at commit 4). The
//! name-resolved type EDGES (field / impl / uses / ...) still land with
//! `Resolve<TypeF>`; phase 1 stays pure-content.

use std::collections::BTreeSet;

use oxc_allocator::Allocator;
use oxc_ast::ast as ts;
use oxc_ast::ast::Program;
use oxc_ast_visit::Visit as OxcVisit;
use oxc_span::{GetSpan, SourceType};

use crate::family::{CallF, CallKind, CallSite, ConstKind, ConstValue, CstF, DfEdgeKind, DfF, DfNodeKind, SigSlot, TypeEntityKind, TypeF};
use crate::rows::{Edge, FamilyBundle, Node};
use crate::seams::{ParseError, Parser, Project};
use crate::shape::{NameId, NodeRef, Span, Strings};
use crate::source::{ExtractOutput, FamilyMask, Source};
use super::astgrep::{AstGrepParser, CstProjector};

/// `oxc_span::Span` (start + end) -> our byte `Span` (start + len). One
/// coordinate; the engine derives line/col from the file bytes when needed.
fn to_span(s: oxc_span::Span) -> Span {
    Span { start: s.start, len: s.end - s.start }
}

/// The Parser: oxc arena parse for TS/TSX/JS/JSX. `Arena = Allocator`;
/// `Parsed<'a> = Program<'a>` (borrows the arena + source text).
pub struct OxcParser;

impl Parser for OxcParser {
    type Arena = Allocator;
    type Parsed<'a> = Program<'a>;

    fn name(&self) -> &'static str {
        "oxc"
    }

    fn matches(&self, path: &str) -> bool {
        source_type_for(path).is_some()
    }

    fn make_arena(&self) -> Allocator {
        Allocator::default()
    }

    fn parse<'a>(
        &self,
        arena: &'a Allocator,
        path: &str,
        content: &'a [u8],
    ) -> Result<Program<'a>, ParseError> {
        let source_type = source_type_for(path)
            .ok_or_else(|| ParseError::NoGrammar(path.to_string()))?;
        let src =
            std::str::from_utf8(content).map_err(|err| ParseError::Utf8(err.to_string()))?;
        let ret = oxc_parser::Parser::new(arena, src, source_type).parse();
        if ret.panicked {
            return Err(ParseError::Parse(format!("oxc panicked on {path}")));
        }
        Ok(ret.program)
    }
}

/// TS/JS source type by extension. `.tsx` -> TSX; `.ts`/`.mts`/`.cts` -> TS;
/// `.js`/`.jsx`/`.mjs`/`.cjs` -> JSX-enabled JS. (v5 `source_type_for`.)
fn source_type_for(path: &str) -> Option<SourceType> {
    if path.ends_with(".tsx") {
        Some(SourceType::tsx())
    } else if path.ends_with(".ts") || path.ends_with(".mts") || path.ends_with(".cts") {
        Some(SourceType::ts())
    } else {
        match path.rsplit('.').next() {
            Some("js" | "jsx" | "mjs" | "cjs") => Some(SourceType::jsx()),
            _ => None,
        }
    }
}

/// The TypeF projector: walks the oxc `Program`, minting one entity node per
/// type/function declaration. Port of v5 `ts_entities_from` (entity half); the
/// arrow types and edge graph are dropped here (edges land with Resolve<TypeF>).
pub struct TypeProjector;

impl Project<TypeF> for TypeProjector {
    type Parsed<'a> = Program<'a>;

    fn project(&self, program: &Program<'_>, strings: &mut Strings, sink: &mut FamilyBundle<TypeF>) {
        for stmt in &program.body {
            use ts::Statement as S;
            match stmt {
                S::ExportNamedDeclaration(export) => {
                    if let Some(decl) = &export.declaration {
                        decl_entity(decl, strings, sink);
                    }
                }
                S::ExportDefaultDeclaration(export) => match &export.declaration {
                    ts::ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                        class_entity(class, strings, sink)
                    }
                    ts::ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
                        push_entity(sink, strings, interface.span, interface.id.name.to_string(), TypeEntityKind::Interface);
                    }
                    ts::ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                        fn_entity(func, strings, sink)
                    }
                    _ => {}
                },
                S::ClassDeclaration(class) => class_entity(class, strings, sink),
                S::TSInterfaceDeclaration(interface) => push_entity(
                    sink,
                    strings,
                    interface.span,
                    interface.id.name.to_string(),
                    TypeEntityKind::Interface,
                ),
                S::TSTypeAliasDeclaration(alias) => push_entity(
                    sink,
                    strings,
                    alias.span,
                    alias.id.name.to_string(),
                    TypeEntityKind::Alias,
                ),
                S::TSEnumDeclaration(enum_decl) => push_entity(
                    sink,
                    strings,
                    enum_decl.span,
                    enum_decl.id.name.to_string(),
                    TypeEntityKind::Enum,
                ),
                S::FunctionDeclaration(func) => fn_entity(func, strings, sink),
                S::VariableDeclaration(var) => var_fn_entity(var, strings, sink),
                _ => {}
            }
        }
    }
}

fn decl_entity(decl: &ts::Declaration, strings: &mut Strings, sink: &mut FamilyBundle<TypeF>) {
    match decl {
        ts::Declaration::ClassDeclaration(class) => class_entity(class, strings, sink),
        ts::Declaration::TSInterfaceDeclaration(interface) => push_entity(
            sink,
            strings,
            interface.span,
            interface.id.name.to_string(),
            TypeEntityKind::Interface,
        ),
        ts::Declaration::TSTypeAliasDeclaration(alias) => push_entity(
            sink,
            strings,
            alias.span,
            alias.id.name.to_string(),
            TypeEntityKind::Alias,
        ),
        ts::Declaration::TSEnumDeclaration(enum_decl) => push_entity(
            sink,
            strings,
            enum_decl.span,
            enum_decl.id.name.to_string(),
            TypeEntityKind::Enum,
        ),
        ts::Declaration::FunctionDeclaration(func) => fn_entity(func, strings, sink),
        ts::Declaration::VariableDeclaration(var) => var_fn_entity(var, strings, sink),
        _ => {}
    }
}

fn class_entity(class: &ts::Class, strings: &mut Strings, sink: &mut FamilyBundle<TypeF>) {
    let Some(id) = &class.id else { return };
    push_entity(sink, strings, class.span, id.name.to_string(), TypeEntityKind::Class);
    for element in &class.body.body {
        if let ts::ClassElement::MethodDefinition(method) = element {
            // Skip constructors and computed/private keys; a normal method's
            // owner is its enclosing class (port of v5 ts_class_entity).
            if method.kind == ts::MethodDefinitionKind::Constructor {
                continue;
            }
            if let ts::PropertyKey::StaticIdentifier(key) = &method.key {
                push_entity(sink, strings, method.span, key.name.to_string(), TypeEntityKind::Method);
                // A method IS a type: carry its arrow signature (param/ret refs).
                fn_sigs(
                    method.span,
                    &method.value.type_parameters,
                    &method.value.params,
                    &method.value.return_type,
                    strings,
                    sink,
                );
            }
        }
    }
}

fn fn_entity(func: &ts::Function, strings: &mut Strings, sink: &mut FamilyBundle<TypeF>) {
    let Some(id) = &func.id else { return };
    push_entity(sink, strings, func.span, id.name.to_string(), TypeEntityKind::Function);
    fn_sigs(
        func.span,
        &func.type_parameters,
        &func.params,
        &func.return_type,
        strings,
        sink,
    );
}

/// `const foo = (...) => ...` / `const foo = function (...) {...}` at the top
/// level: the binding name owns a Function entity. Plain value consts (no
/// function initializer) carry no type shape and are skipped (v5: the
/// "don't mint an entity for every const" rule).
fn var_fn_entity(var: &ts::VariableDeclaration, strings: &mut Strings, sink: &mut FamilyBundle<TypeF>) {
    for declarator in &var.declarations {
        let ts::BindingPattern::BindingIdentifier(name) = &declarator.id else {
            continue;
        };
        match &declarator.init {
            Some(ts::Expression::ArrowFunctionExpression(arrow)) => {
                push_entity(sink, strings, declarator.span, name.name.to_string(), TypeEntityKind::Function);
                fn_sigs(
                    declarator.span,
                    &arrow.type_parameters,
                    &arrow.params,
                    &arrow.return_type,
                    strings,
                    sink,
                );
            }
            Some(ts::Expression::FunctionExpression(func)) => {
                push_entity(sink, strings, declarator.span, name.name.to_string(), TypeEntityKind::Function);
                fn_sigs(
                    declarator.span,
                    &func.type_parameters,
                    &func.params,
                    &func.return_type,
                    strings,
                    sink,
                );
            }
            _ => {}
        }
    }
}

// ── arrow-type signatures (the D-arrow-type payload) ────────────────────────
//
// Port of v5 `ts_fn_signature_edges`'s param/returns half. Collects every
// `TSTypeReference` name under a signature's type annotations (excluding the
// callable's own type-parameter names), minting one `TypeSig` per name per
// slot. Keyword types (number/string) are distinct AST variants, never
// references, so they emit nothing; a union slot (A | B) emits one per arm.

/// The signature slots of one callable: param type-refs (with their positional
/// index) + the return type-refs. `owner` is the callable node's span; the sigs
/// join back to that node at the wire and the resolution seam.
fn fn_sigs(
    owner: oxc_span::Span,
    type_parameters: &Option<oxc_allocator::Box<ts::TSTypeParameterDeclaration>>,
    params: &ts::FormalParameters,
    return_type: &Option<oxc_allocator::Box<ts::TSTypeAnnotation>>,
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let exclude = type_param_names(type_parameters);
    for (pos, param) in params.items.iter().enumerate() {
        if let Some(ann) = &param.type_annotation {
            for name in refs_in_type(&ann.type_annotation, &exclude) {
                push_sig(sink, strings, owner, SigSlot::Param, pos as u32, &name);
            }
        }
    }
    if let Some(rt) = return_type {
        for name in refs_in_type(&rt.type_annotation, &exclude) {
            push_sig(sink, strings, owner, SigSlot::Ret, 0, &name);
        }
    }
}

fn push_sig(
    sink: &mut FamilyBundle<TypeF>,
    strings: &mut Strings,
    owner: oxc_span::Span,
    slot: SigSlot,
    pos: u32,
    name: &str,
) {
    sink.aux.sigs.push(crate::family::TypeSig {
        owner: Span { start: owner.start, len: owner.end - owner.start },
        slot,
        pos,
        ty: strings.intern(name),
    });
}

/// The callable's declared type-parameter names (the exclusion set: a generic
/// `<T>` referencing itself is not a sig). Port of v5 `ts_param_edges`'s name
/// collection (constraint refs as "generic" edges are deferred to commit 4).
fn type_param_names(
    tp: &Option<oxc_allocator::Box<ts::TSTypeParameterDeclaration>>,
) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if let Some(tp) = tp {
        for param in &tp.params {
            names.insert(param.name.name.to_string());
        }
    }
    names
}

/// Every `TSTypeReference` name under a type subtree, excluding the callable's
/// own type-parameter names. Port of v5 `ts_refs_in_type`.
fn refs_in_type(ty: &ts::TSType, exclude: &BTreeSet<String>) -> Vec<String> {
    let mut collector = TypeRefCollector { exclude, out: Vec::new() };
    collector.visit_ts_type(ty);
    collector.out
}

struct TypeRefCollector<'p> {
    exclude: &'p BTreeSet<String>,
    out: Vec<String>,
}

impl<'a, 'p> OxcVisit<'a> for TypeRefCollector<'p> {
    fn visit_ts_type_reference(&mut self, reference: &ts::TSTypeReference<'a>) {
        if let Some(name) = ts_type_name(&reference.type_name) {
            if !self.exclude.contains(&name) {
                self.out.push(name);
            }
        }
        oxc_ast_visit::walk::walk_ts_type_reference(self, reference);
    }
}

/// A `TSTypeName` to a dotted string (`Db`, `React.Node`), or None for `this`.
/// Port of v5 `ts_type_name`.
fn ts_type_name(name: &ts::TSTypeName) -> Option<String> {
    match name {
        ts::TSTypeName::IdentifierReference(id) => Some(id.name.to_string()),
        ts::TSTypeName::QualifiedName(qualified) => {
            ts_type_name(&qualified.left).map(|left| format!("{left}.{}", qualified.right.name))
        }
        ts::TSTypeName::ThisExpression(_) => None,
    }
}

// ════════════════════════════════════════════════════════════════════════════
// CallF: callable definitions (nodes) + call sites (aux). Commit 3a.
//
// Ports v5 `ts_call_defs_from` (defs) + `TsCallSites` (sites) + `TsNestedFnDefs`
// (nested named-fn defs). v5's `mint_sym`/`line_at`/`starts` are deleted: a def
// is span + kind + name (the name is the bare identifier for callee resolution,
// NOT a qualified sym). The def span MATCHES the TypeF entity span (func.span /
// method.span / declarator.span) so the two facets join on the same coordinate.
// Lambda defs (anonymous callables from the df lift) land with DfF.
// ════════════════════════════════════════════════════════════════════════════

/// The CallF projector: emits one def node per callable (Free / Method) and one
/// site per call expression. Sites are unresolved in phase 1 (the callee as
/// written); `Resolve<CallF>` binds them at commit 4.
pub struct CallProjector;

impl Project<CallF> for CallProjector {
    type Parsed<'a> = Program<'a>;

    fn project(&self, program: &Program<'_>, strings: &mut Strings, sink: &mut FamilyBundle<CallF>) {
        // Top-level defs (depth 0): free functions, class methods, var-bound fns.
        call_defs(program, strings, sink);
        // One walk for the rest: nested named-fn defs (depth > 0) + every call
        // site. The walker owns its output (no &mut strings/sink inside the
        // visitor), drained here so the two &mut params never alias through self.
        let mut walker = CallWalker { depth: 0, nested_defs: Vec::new(), sites: Vec::new() };
        walker.visit_program(program);
        for (span, name) in walker.nested_defs {
            push_def(sink, strings, span, name, CallKind::Free);
        }
        for site in walker.sites {
            sink.aux.sites.push(CallSite {
                span: to_span(site.span),
                callee: strings.intern(&site.callee),
                callee_path: None,
            });
        }
    }
}

fn call_defs(program: &Program<'_>, strings: &mut Strings, sink: &mut FamilyBundle<CallF>) {
    for stmt in &program.body {
        use ts::Statement as S;
        match stmt {
            S::ExportNamedDeclaration(export) => {
                if let Some(decl) = &export.declaration {
                    call_decl_def(decl, strings, sink);
                }
            }
            S::ExportDefaultDeclaration(export) => match &export.declaration {
                ts::ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                    class_call_defs(class, strings, sink)
                }
                ts::ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                    fn_call_def(func, strings, sink)
                }
                _ => {}
            },
            S::ClassDeclaration(class) => class_call_defs(class, strings, sink),
            S::FunctionDeclaration(func) => fn_call_def(func, strings, sink),
            S::VariableDeclaration(var) => var_call_defs(var, strings, sink),
            _ => {}
        }
    }
}

fn call_decl_def(decl: &ts::Declaration, strings: &mut Strings, sink: &mut FamilyBundle<CallF>) {
    match decl {
        ts::Declaration::ClassDeclaration(class) => class_call_defs(class, strings, sink),
        ts::Declaration::FunctionDeclaration(func) => fn_call_def(func, strings, sink),
        ts::Declaration::VariableDeclaration(var) => var_call_defs(var, strings, sink),
        _ => {}
    }
}

/// A named `function foo() {}`. Overloads (no body) are skipped: only the impl
/// carries a def. Port of v5 `ts_fn_call_def`.
fn fn_call_def(func: &ts::Function, strings: &mut Strings, sink: &mut FamilyBundle<CallF>) {
    let Some(id) = &func.id else { return };
    if func.body.is_none() {
        return;
    }
    push_def(sink, strings, func.span, id.name.to_string(), CallKind::Free);
}

/// One def per class method. The CONSTRUCTOR's call-name is the CLASS name so a
/// `new Foo()` site resolves to it (v5 `ts_class_call_defs`); its kind is Method.
/// Abstract methods (no body) are skipped. Port of v5 `ts_class_call_defs`.
fn class_call_defs(class: &ts::Class, strings: &mut Strings, sink: &mut FamilyBundle<CallF>) {
    let Some(id) = &class.id else { return };
    let owner = id.name.to_string();
    for element in &class.body.body {
        let ts::ClassElement::MethodDefinition(method) = element else { continue };
        let ts::PropertyKey::StaticIdentifier(key) = &method.key else { continue };
        if method.value.body.is_none() {
            continue;
        }
        let is_ctor = method.kind == ts::MethodDefinitionKind::Constructor;
        let name = if is_ctor { owner.clone() } else { key.name.to_string() };
        push_def(sink, strings, method.span, name, CallKind::Method);
    }
}

/// `const foo = (...) => ...` / `const foo = function () {}`: a Free def owned
/// by the binding name. Bodiless function expressions are skipped. Port of v5
/// `ts_var_call_defs`.
fn var_call_defs(var: &ts::VariableDeclaration, strings: &mut Strings, sink: &mut FamilyBundle<CallF>) {
    for declarator in &var.declarations {
        let ts::BindingPattern::BindingIdentifier(name) = &declarator.id else { continue };
        let has_body = match &declarator.init {
            Some(ts::Expression::ArrowFunctionExpression(_)) => true,
            Some(ts::Expression::FunctionExpression(func)) => func.body.is_some(),
            _ => false,
        };
        if !has_body {
            continue;
        }
        push_def(sink, strings, declarator.span, name.name.to_string(), CallKind::Free);
    }
}

fn push_def(
    sink: &mut FamilyBundle<CallF>,
    strings: &mut Strings,
    span: oxc_span::Span,
    name: impl AsRef<str>,
    kind: CallKind,
) {
    sink.nodes.push(Node::new(to_span(span), kind).with_name(strings.intern(name.as_ref())));
}

/// One collected call site before it is interned into the aux.
struct CollectedSite {
    span: oxc_span::Span,
    callee: String,
}

/// Walks the whole program for (a) nested named function DECLARATIONS at
/// `depth > 0` (top-level ones are `call_defs`' job) and (b) every call site
/// (`foo()`, `new Foo()`, `<Card/>`). Collects into owned vecs; the projector
/// drains + interns after the walk. Port of v5 `TsNestedFnDefs` + `TsCallSites`.
struct CallWalker {
    depth: u32,
    nested_defs: Vec<(oxc_span::Span, String)>,
    sites: Vec<CollectedSite>,
}

impl<'a> OxcVisit<'a> for CallWalker {
    fn visit_function(&mut self, func: &ts::Function<'a>, flags: oxc_syntax::scope::ScopeFlags) {
        // Only named DECLARATIONS below the top level (function expressions and
        // method values are already Methods; top-level decls are call_defs').
        if self.depth > 0 && func.r#type == ts::FunctionType::FunctionDeclaration {
            if let Some(id) = &func.id {
                self.nested_defs.push((func.span, id.name.to_string()));
            }
        }
        self.depth += 1;
        oxc_ast_visit::walk::walk_function(self, func, flags);
        self.depth -= 1;
    }

    fn visit_arrow_function_expression(&mut self, arrow: &ts::ArrowFunctionExpression<'a>) {
        // Arrows have no own declaration name, but they raise the depth so their
        // nested named decls land as Free defs.
        self.depth += 1;
        oxc_ast_visit::walk::walk_arrow_function_expression(self, arrow);
        self.depth -= 1;
    }

    fn visit_call_expression(&mut self, call: &ts::CallExpression<'a>) {
        if let Some(callee) = callee_name(&call.callee) {
            self.sites.push(CollectedSite { span: call.callee.span(), callee });
        }
        oxc_ast_visit::walk::walk_call_expression(self, call);
    }

    fn visit_new_expression(&mut self, new_expr: &ts::NewExpression<'a>) {
        // `new Foo(x)`: the callee is the constructed name; resolves to the
        // class's ctor call_def (whose name is the class name).
        if let Some(callee) = callee_name(&new_expr.callee) {
            self.sites.push(CollectedSite { span: new_expr.span, callee });
        }
        oxc_ast_visit::walk::walk_new_expression(self, new_expr);
    }

    fn visit_jsx_element(&mut self, element: &ts::JSXElement<'a>) {
        // `<Card/>` is a call (jsx(Card, props)); host elements (`<div/>`,
        // lowercase Identifier) have no def to resolve to and are skipped.
        use ts::JSXElementName as N;
        let callee = match &element.opening_element.name {
            N::IdentifierReference(reference) => Some(reference.name.to_string()),
            N::MemberExpression(member) => Some(member.property.name.to_string()),
            _ => None,
        };
        if let Some(callee) = callee {
            self.sites.push(CollectedSite { span: element.opening_element.span, callee });
        }
        oxc_ast_visit::walk::walk_jsx_element(self, element);
    }
}

/// The called name as written: a bare identifier, or the trailing property of a
/// static member expression (`a.b.c()` -> "c"). Computed/other callee shapes
/// resolve to nothing. Port of v5 `ts_callee_name`.
fn callee_name(expr: &ts::Expression) -> Option<String> {
    use ts::Expression as E;
    match expr {
        E::Identifier(id) => Some(id.name.to_string()),
        E::StaticMemberExpression(member) => Some(member.property.name.to_string()),
        _ => None,
    }
}

// ════════════════════════════════════════════════════════════════════════════
// DfF: intra-procedural value flow (nodes + edges). Commit 3b.
//
// Ports v5 `ts_dataflow_from` (ts/flow.rs). Every value-bearing position in a
// callable's body becomes a NODE; local value flow becomes an EDGE (Direct).
// The two are the dataflow graph the engine's `df_reaches` closure walks.
//
// What is DROPPED vs v5 (each is a deliberate, documented deferral):
//  - `fn_sym` / `mint_sym` / `lambda_sym`: the enclosing callable is NOT stored
//    on a df node. It is derived at the seam by span-containment over the CallF
//    defs (the same pattern as the CallF site caller). The transient scope
//    HashMap (var name -> NodeRef) for intra-procedural resolution is kept.
//  - `line_at` / `line_index` / `line_col`: a node is a byte Span, never a line.
//  - the enrichment aux: `args` (positional slots), `fields` (object/array
//    field names), `lits` (literal texts), `param_pos`, `loops`, `nests`. The
//    EDGES already carry every value flow; the aux only labels slots/names/texts
//    for the later interprocedural (arg->param) + string-flow queries.
//  - JSX element/fragment flow (tsx-specific; the catch-all covers it for now).
// ════════════════════════════════════════════════════════════════════════════

/// The DfF projector: lifts each callable's body to its value-flow graph.
pub struct DfProjector;

impl Project<DfF> for DfProjector {
    type Parsed<'a> = Program<'a>;

    fn project(&self, program: &Program<'_>, strings: &mut Strings, sink: &mut FamilyBundle<DfF>) {
        for stmt in &program.body {
            df_flow_stmt(stmt, strings, sink);
        }
    }
}

type Scope = std::collections::HashMap<String, NodeRef>;

fn df_flow_stmt(stmt: &ts::Statement, strings: &mut Strings, sink: &mut FamilyBundle<DfF>) {
    use ts::Statement as S;
    match stmt {
        S::FunctionDeclaration(func) => {
            if let Some(body) = func.body.as_deref() {
                let mut scope = Scope::new();
                df_seed_params(&func.params, strings, &mut scope, sink);
                df_flow_body(body, strings, &mut scope, sink);
            }
        }
        S::ExportNamedDeclaration(export) => {
            if let Some(decl) = &export.declaration {
                df_flow_decl(decl, strings, sink);
            }
        }
        S::ExportDefaultDeclaration(export) => match &export.declaration {
            ts::ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                if let Some(body) = func.body.as_deref() {
                    let mut scope = Scope::new();
                    df_seed_params(&func.params, strings, &mut scope, sink);
                    df_flow_body(body, strings, &mut scope, sink);
                }
            }
            ts::ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                df_flow_class(class, strings, sink);
            }
            _ => {}
        },
        S::ClassDeclaration(class) => df_flow_class(class, strings, sink),
        // Top-level var/expr/return statements have no enclosing callable; walk
        // them under a fresh empty scope (their nodes' caller is "none" / top).
        S::VariableDeclaration(_) | S::ExpressionStatement(_) | S::ReturnStatement(_) => {
            let mut scope = Scope::new();
            df_flow_body_stmt(stmt, strings, &mut scope, sink);
        }
        _ => {}
    }
}

fn df_flow_decl(decl: &ts::Declaration, strings: &mut Strings, sink: &mut FamilyBundle<DfF>) {
    use ts::Declaration as D;
    match decl {
        D::FunctionDeclaration(func) => {
            if let Some(body) = func.body.as_deref() {
                let mut scope = Scope::new();
                df_seed_params(&func.params, strings, &mut scope, sink);
                df_flow_body(body, strings, &mut scope, sink);
            }
        }
        D::ClassDeclaration(class) => df_flow_class(class, strings, sink),
        _ => {}
    }
}

/// Each method body flows like a free function's. Field initializers are not
/// covered (no natural enclosing callable scope). Port of v5 `ts_flow_class`.
fn df_flow_class(class: &ts::Class, strings: &mut Strings, sink: &mut FamilyBundle<DfF>) {
    for element in &class.body.body {
        let ts::ClassElement::MethodDefinition(method) = element else { continue };
        let Some(body) = method.value.body.as_deref() else { continue };
        let mut scope = Scope::new();
        df_seed_params(&method.value.params, strings, &mut scope, sink);
        df_flow_body(body, strings, &mut scope, sink);
    }
}

fn df_flow_body(
    body: &ts::FunctionBody,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    for stmt in &body.statements {
        df_flow_body_stmt(stmt, strings, scope, sink);
    }
}

/// Lift a function value (arrow or function expression) as its own scope: seed
/// params, then walk the body. An expression-body arrow (`(x) => expr`) wraps
/// the expr as an implicit return into a `ret` node. Port of v5 `ts_lift_fn`.
fn df_lift_fn(
    params: &ts::FormalParameters,
    body: &ts::FunctionBody,
    expression: bool,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    let mut scope = Scope::new();
    df_seed_params(params, strings, &mut scope, sink);
    if expression {
        if let Some(ts::Statement::ExpressionStatement(expr_stmt)) = body.statements.first() {
            let value = df_flow_expr(&expr_stmt.expression, strings, &mut scope, sink);
            let ret = df_push(sink, strings, expr_stmt.span, DfNodeKind::Ret, None);
            df_edge(sink, value, ret);
        }
    } else {
        for stmt in &body.statements {
            df_flow_body_stmt(stmt, strings, &mut scope, sink);
        }
    }
}

fn df_flow_body_stmt(
    stmt: &ts::Statement,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    use ts::Statement as S;
    match stmt {
        S::VariableDeclaration(var) => {
            for declarator in &var.declarations {
                // A const-bound arrow / function expression is a callable, not a
                // value: lift its body as its own scope (its nodes' caller is
                // derived by containment at the seam).
                if let ts::BindingPattern::BindingIdentifier(_) = &declarator.id {
                    match &declarator.init {
                        Some(ts::Expression::ArrowFunctionExpression(arrow)) => {
                            df_lift_fn(&arrow.params, &arrow.body, arrow.expression, strings, sink);
                            continue;
                        }
                        Some(ts::Expression::FunctionExpression(func)) => {
                            if let Some(body) = func.body.as_deref() {
                                df_lift_fn(&func.params, body, false, strings, sink);
                            }
                            continue;
                        }
                        _ => {}
                    }
                }
                let rhs = declarator
                    .init
                    .as_ref()
                    .map(|init| df_flow_expr(init, strings, scope, sink));
                if let Some(name) = binding_name(&declarator.id) {
                    let bind = df_push(sink, strings, declarator.span, DfNodeKind::LetBind, Some(&name));
                    if let Some(rhs) = rhs {
                        df_edge(sink, rhs, bind);
                    }
                    scope.insert(name, bind);
                }
            }
        }
        S::ExpressionStatement(expr_stmt) => {
            let _ = df_flow_expr(&expr_stmt.expression, strings, scope, sink);
        }
        // `return EXPR`: the returned value flows into the fn's `ret` node (the
        // sink the interprocedural backward hop reads).
        S::ReturnStatement(ret_stmt) => {
            let ret = df_push(sink, strings, ret_stmt.span, DfNodeKind::Ret, None);
            if let Some(arg) = &ret_stmt.argument {
                let value = df_flow_expr(arg, strings, scope, sink);
                df_edge(sink, value, ret);
            }
        }
        S::BlockStatement(block) => {
            for inner in &block.body {
                df_flow_body_stmt(inner, strings, scope, sink);
            }
        }
        S::IfStatement(if_stmt) => {
            let _ = df_flow_expr(&if_stmt.test, strings, scope, sink);
            df_flow_body_stmt(&if_stmt.consequent, strings, scope, sink);
            if let Some(alternate) = &if_stmt.alternate {
                df_flow_body_stmt(alternate, strings, scope, sink);
            }
        }
        S::ForStatement(for_stmt) => {
            if let Some(ts::ForStatementInit::VariableDeclaration(var)) = &for_stmt.init {
                for declarator in &var.declarations {
                    let rhs = declarator
                        .init
                        .as_ref()
                        .map(|init| df_flow_expr(init, strings, scope, sink));
                    if let Some(name) = binding_name(&declarator.id) {
                        let bind = df_push(sink, strings, declarator.span, DfNodeKind::LetBind, Some(&name));
                        if let Some(rhs) = rhs {
                            df_edge(sink, rhs, bind);
                        }
                        scope.insert(name, bind);
                    }
                }
            }
            if let Some(test) = &for_stmt.test {
                let _ = df_flow_expr(test, strings, scope, sink);
            }
            if let Some(update) = &for_stmt.update {
                let _ = df_flow_expr(update, strings, scope, sink);
            }
            df_flow_body_stmt(&for_stmt.body, strings, scope, sink);
        }
        S::ForOfStatement(for_stmt) => df_for_in_of(
            &for_stmt.left,
            &for_stmt.right,
            &for_stmt.body,
            strings,
            scope,
            sink,
        ),
        S::ForInStatement(for_stmt) => df_for_in_of(
            &for_stmt.left,
            &for_stmt.right,
            &for_stmt.body,
            strings,
            scope,
            sink,
        ),
        S::WhileStatement(while_stmt) => {
            let _ = df_flow_expr(&while_stmt.test, strings, scope, sink);
            df_flow_body_stmt(&while_stmt.body, strings, scope, sink);
        }
        S::DoWhileStatement(do_stmt) => {
            let _ = df_flow_expr(&do_stmt.test, strings, scope, sink);
            df_flow_body_stmt(&do_stmt.body, strings, scope, sink);
        }
        _ => {}
    }
}

/// Shared handling for `for (x of/in coll) body`: bind the loop variable, flow
/// the collection into it, then walk the body. (The loop FACT is deferred aux.)
fn df_for_in_of(
    left: &ts::ForStatementLeft,
    right: &ts::Expression,
    body: &ts::Statement,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    let collection = df_flow_expr(right, strings, scope, sink);
    if let ts::ForStatementLeft::VariableDeclaration(var) = left {
        if let Some(declarator) = var.declarations.first() {
            if let Some(name) = binding_name(&declarator.id) {
                let bind = df_push(sink, strings, declarator.span, DfNodeKind::LetBind, Some(&name));
                df_edge(sink, collection, bind);
                scope.insert(name, bind);
            }
        }
    }
    df_flow_body_stmt(body, strings, scope, sink);
}

/// `f(args)` / `recv.m(args)`: each argument flows into the call result; a
/// member callee flows its receiver in too. (The positional `args` slots are
/// deferred aux; the edges already carry the flow.) Port of v5 `ts_flow_call`.
fn df_flow_call(
    call: &ts::CallExpression,
    span: oxc_span::Span,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> NodeRef {
    use ts::Expression as E;
    let receiver = match &call.callee {
        E::StaticMemberExpression(member) => {
            Some(df_flow_expr(&member.object, strings, scope, sink))
        }
        E::ComputedMemberExpression(member) => {
            Some(df_flow_expr(&member.object, strings, scope, sink))
        }
        _ => None,
    };
    let mut arg_ids = Vec::new();
    for arg in &call.arguments {
        if let Some(expr) = arg.as_expression() {
            arg_ids.push(df_flow_expr(expr, strings, scope, sink));
        }
    }
    let call_res = df_push(sink, strings, span, DfNodeKind::CallRes, None);
    if let Some(recv) = receiver {
        df_edge(sink, recv, call_res);
    }
    for arg_id in arg_ids {
        df_edge(sink, arg_id, call_res);
    }
    call_res
}

/// `recv.prop` / `recv[prop]`: the receiver flows into a `member` node whose
/// name is the accessed property (empty for a computed access). Port of v5
/// `ts_flow_member`.
fn df_flow_member(
    object: &ts::Expression,
    property: Option<&str>,
    span: oxc_span::Span,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> NodeRef {
    let object_id = df_flow_expr(object, strings, scope, sink);
    let member = df_push(sink, strings, span, DfNodeKind::Member, property);
    df_edge(sink, object_id, member);
    member
}

/// Post-order value flow for one TS expression. Returns the node carrying its
/// value, or a generic `expr` node when the variant isn't chased (conservative:
/// may miss, never invents). Port of v5 `ts_flow_expr`.
fn df_flow_expr(
    expr: &ts::Expression,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> NodeRef {
    use ts::Expression as E;
    let span = expr.span();
    match expr {
        // A read of a variable: flow from its binding slot.
        E::Identifier(id) => {
            let name = id.name.to_string();
            let node = df_push(sink, strings, span, DfNodeKind::VarRead, Some(&name));
            if let Some(binding) = scope.get(&name) {
                df_edge(sink, *binding, node);
            }
            node
        }
        // Literals: a `lit` node. (The string text is deferred `lits` aux.)
        E::StringLiteral(_)
        | E::NumericLiteral(_)
        | E::BooleanLiteral(_)
        | E::NullLiteral(_)
        | E::BigIntLiteral(_)
        | E::RegExpLiteral(_) => df_push(sink, strings, span, DfNodeKind::Lit, None),
        E::CallExpression(call) => df_flow_call(call, span, strings, scope, sink),
        // `new Foo(args)`: a `new` node carrying the class name; each arg flows in.
        E::NewExpression(new_expr) => {
            let type_name = match &new_expr.callee {
                E::Identifier(id) => Some(id.name.to_string()),
                E::StaticMemberExpression(member) => Some(member.property.name.to_string()),
                _ => None,
            };
            let mut arg_ids = Vec::new();
            for arg in &new_expr.arguments {
                if let Some(expr) = arg.as_expression() {
                    arg_ids.push(df_flow_expr(expr, strings, scope, sink));
                }
            }
            let new_node = df_push(sink, strings, span, DfNodeKind::New, type_name.as_deref());
            for arg_id in arg_ids {
                df_edge(sink, arg_id, new_node);
            }
            new_node
        }
        // `{ a: x, ...rest }` / `[a, b, ...rest]`: a composite `new` node; each
        // element flows in. (Field names are deferred `fields` aux.)
        E::ObjectExpression(object) => {
            let mut value_ids = Vec::new();
            for property in &object.properties {
                match property {
                    ts::ObjectPropertyKind::ObjectProperty(prop) => {
                        value_ids.push(df_flow_expr(&prop.value, strings, scope, sink));
                    }
                    ts::ObjectPropertyKind::SpreadProperty(spread) => {
                        value_ids.push(df_flow_expr(&spread.argument, strings, scope, sink));
                    }
                }
            }
            let new_node = df_push(sink, strings, span, DfNodeKind::New, None);
            for value_id in value_ids {
                df_edge(sink, value_id, new_node);
            }
            new_node
        }
        E::ArrayExpression(array) => {
            let mut element_ids = Vec::new();
            for element in &array.elements {
                match element {
                    ts::ArrayExpressionElement::SpreadElement(spread) => {
                        element_ids.push(df_flow_expr(&spread.argument, strings, scope, sink));
                    }
                    ts::ArrayExpressionElement::Elision(_) => {}
                    _ => {
                        if let Some(expr) = element.as_expression() {
                            element_ids.push(df_flow_expr(expr, strings, scope, sink));
                        }
                    }
                }
            }
            let new_node = df_push(sink, strings, span, DfNodeKind::New, None);
            for element_id in element_ids {
                df_edge(sink, element_id, new_node);
            }
            new_node
        }
        // recv.prop / recv[prop]: receiver flows into a `member` node.
        E::StaticMemberExpression(member) => {
            df_flow_member(&member.object, Some(member.property.name.as_str()), span, strings, scope, sink)
        }
        E::ComputedMemberExpression(member) => {
            df_flow_member(&member.object, None, span, strings, scope, sink)
        }
        // `a + b` is its own `concat` kind (so a string-construction query matches
        // `kind IN (template, concat)`); any other binary op is `binop`.
        E::BinaryExpression(binary) => {
            let left = df_flow_expr(&binary.left, strings, scope, sink);
            let right = df_flow_expr(&binary.right, strings, scope, sink);
            let kind = if binary.operator == ts::BinaryOperator::Addition {
                DfNodeKind::Concat
            } else {
                DfNodeKind::Binop
            };
            let node = df_push(sink, strings, span, kind, None);
            df_edge(sink, left, node);
            df_edge(sink, right, node);
            node
        }
        // An INLINE lambda: lift its body as its own scope, then mint the
        // `closure` VALUE node here. (The join sym is deferred; the node marks
        // the closure value for now.)
        E::ArrowFunctionExpression(arrow) => {
            df_lift_fn(&arrow.params, &arrow.body, arrow.expression, strings, sink);
            df_push(sink, strings, span, DfNodeKind::Closure, None)
        }
        E::FunctionExpression(func) => match func.body.as_deref() {
            Some(body) => {
                df_lift_fn(&func.params, body, false, strings, sink);
                df_push(sink, strings, span, DfNodeKind::Closure, None)
            }
            None => df_push(sink, strings, span, DfNodeKind::Expr, None),
        },
        // Transparent wrappers: flow the inner expression straight through.
        E::ParenthesizedExpression(paren) => df_flow_expr(&paren.expression, strings, scope, sink),
        E::TSAsExpression(inner) => df_flow_expr(&inner.expression, strings, scope, sink),
        E::TSSatisfiesExpression(inner) => df_flow_expr(&inner.expression, strings, scope, sink),
        E::TSNonNullExpression(inner) => df_flow_expr(&inner.expression, strings, scope, sink),
        E::AwaitExpression(inner) => df_flow_expr(&inner.argument, strings, scope, sink),
        E::TSTypeAssertion(inner) => df_flow_expr(&inner.expression, strings, scope, sink),
        E::TSInstantiationExpression(inner) => df_flow_expr(&inner.expression, strings, scope, sink),
        E::ChainExpression(chain) => {
            use ts::ChainElement as Chain;
            use ts::MemberExpression as Member;
            match &chain.expression {
                Chain::CallExpression(call) => df_flow_call(call, span, strings, scope, sink),
                other => match other.member_expression() {
                    Some(Member::StaticMemberExpression(member)) => df_flow_member(
                        &member.object,
                        Some(member.property.name.as_str()),
                        span,
                        strings,
                        scope,
                        sink,
                    ),
                    Some(Member::ComputedMemberExpression(member)) => {
                        df_flow_member(&member.object, None, span, strings, scope, sink)
                    }
                    Some(Member::PrivateFieldExpression(member)) => {
                        df_flow_member(&member.object, None, span, strings, scope, sink)
                    }
                    None => df_push(sink, strings, span, DfNodeKind::Expr, None),
                },
            }
        }
        // `x = y` as a value evaluates to the assigned value.
        E::AssignmentExpression(assignment) => {
            df_flow_expr(&assignment.right, strings, scope, sink)
        }
        // `test ? cons : alt`: the value is EITHER branch (both flow in); the
        // test is a guard (walked, not edged).
        E::ConditionalExpression(cond) => {
            let _test = df_flow_expr(&cond.test, strings, scope, sink);
            let consequent = df_flow_expr(&cond.consequent, strings, scope, sink);
            let alternate = df_flow_expr(&cond.alternate, strings, scope, sink);
            let node = df_push(sink, strings, span, DfNodeKind::Cond, None);
            df_edge(sink, consequent, node);
            df_edge(sink, alternate, node);
            node
        }
        // `&&` / `||` / `??`: for `||` / `??` the value is EITHER operand; for
        // `&&` the value is the right (left is a guard).
        E::LogicalExpression(logic) => {
            use ts::LogicalOperator as Op;
            let left = df_flow_expr(&logic.left, strings, scope, sink);
            let right = df_flow_expr(&logic.right, strings, scope, sink);
            let node = df_push(sink, strings, span, DfNodeKind::Logic, None);
            if matches!(logic.operator, Op::Or | Op::Coalesce) {
                df_edge(sink, left, node);
            }
            df_edge(sink, right, node);
            node
        }
        // `(a, b, c)`: the value is the LAST expression; earlier ones are effect.
        E::SequenceExpression(sequence) => {
            let mut last = df_push(sink, strings, span, DfNodeKind::Expr, None);
            for sub in &sequence.expressions {
                last = df_flow_expr(sub, strings, scope, sink);
            }
            last
        }
        // `` `hello ${name}` ``: each interpolation flows into a `template` node.
        E::TemplateLiteral(template) => {
            let node = df_push(sink, strings, span, DfNodeKind::Template, None);
            for sub in &template.expressions {
                let value = df_flow_expr(sub, strings, scope, sink);
                df_edge(sink, value, node);
            }
            node
        }
        E::TaggedTemplateExpression(tagged) => {
            let _tag = df_flow_expr(&tagged.tag, strings, scope, sink);
            let node = df_push(sink, strings, span, DfNodeKind::Template, None);
            for sub in &tagged.quasi.expressions {
                let value = df_flow_expr(sub, strings, scope, sink);
                df_edge(sink, value, node);
            }
            node
        }
        // JSX elements/fragments + remaining variants: mint a node, don't chase.
        _ => df_push(sink, strings, span, DfNodeKind::Expr, None),
    }
}

/// The binding identifier name from a pattern (the common `const x = ...` single-
/// ident case; destructuring falls through to None). Port of v5 `ts_binding_name`.
fn binding_name(pattern: &ts::BindingPattern) -> Option<String> {
    match pattern {
        ts::BindingPattern::BindingIdentifier(binding) => Some(binding.name.to_string()),
        _ => None,
    }
}

/// Seed a callable's param nodes into the scope. A bare identifier binds as
/// itself; an object-destructuring param mints one param node PER property
/// (whose name is the property key). Port of v5 `ts_seed_params` (the positional
/// `param_pos` aux is deferred).
fn df_seed_params(
    params: &ts::FormalParameters,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    for param in &params.items {
        match &param.pattern {
            ts::BindingPattern::BindingIdentifier(binding) => {
                let name = binding.name.to_string();
                let node = df_push(sink, strings, param.span, DfNodeKind::Param, Some(&name));
                scope.insert(name, node);
            }
            ts::BindingPattern::ObjectPattern(object) => {
                for property in &object.properties {
                    if let ts::BindingPattern::BindingIdentifier(binding) = &property.value {
                        let key = match &property.key {
                            ts::PropertyKey::StaticIdentifier(ident) => ident.name.to_string(),
                            ts::PropertyKey::StringLiteral(string) => string.value.to_string(),
                            _ => binding.name.to_string(),
                        };
                        let node = df_push(sink, strings, binding.span, DfNodeKind::Param, Some(&key));
                        scope.insert(binding.name.to_string(), node);
                    }
                }
                if let Some(rest) = &object.rest {
                    if let ts::BindingPattern::BindingIdentifier(binding) = &rest.argument {
                        let name = binding.name.to_string();
                        let node = df_push(sink, strings, binding.span, DfNodeKind::Param, Some(&name));
                        scope.insert(name, node);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Push one df node, returning its `NodeRef` (the dense index edges reference).
fn df_push(
    sink: &mut FamilyBundle<DfF>,
    strings: &mut Strings,
    node_span: oxc_span::Span,
    kind: DfNodeKind,
    var: Option<&str>,
) -> NodeRef {
    let node_ref = NodeRef(sink.nodes.len() as u32);
    let mut node = Node::new(to_span(node_span), kind);
    if let Some(name) = var.filter(|name| !name.is_empty()) {
        node = node.with_name(strings.intern(name));
    }
    sink.nodes.push(node);
    node_ref
}

/// One Direct value edge: `dst` receives the value of `src`.
fn df_edge(sink: &mut FamilyBundle<DfF>, src: NodeRef, dst: NodeRef) {
    sink.edges.push(Edge::new(src, dst, DfEdgeKind::Direct));
}

fn push_entity(
    sink: &mut FamilyBundle<TypeF>,
    strings: &mut Strings,
    node_span: oxc_span::Span,
    name: impl AsRef<str>,
    kind: TypeEntityKind,
) {
    sink.nodes.push(
        Node::new(
            Span {
                start: node_span.start,
                len: node_span.end - node_span.start,
            },
            kind,
        )
        .with_name(strings.intern(name.as_ref())),
    );
}

// ════════════════════════════════════════════════════════════════════════════
// TypeF const facet: port of v5 ts_const_facts_from.
//
// A string-bearing `const`/`as const` binding -> a Const TypeEntity (the
// declaration) + one ConstValue per resolved string (lit cooked value / template
// raw source slice; object literals fan into dotted field paths; string-enum
// members key off the enum entity). Non-string consts emit nothing (in both v5
// and v6). This is v5's model, restored: the D-arrow-type "consts stay df"
// reading had dropped the string-const entity + values v5 kept. The df let_bind
// node is SEPARATE (the const as a value POSITION) and unaffected — a string
// const is a declaration (here), a value (df), and carries its text (here), all
// at once. Runs from TsSource::extract: it needs the source bytes for template
// slices, which Project::project does not receive. v6 drops v5's scope/sym
// machinery — spans disambiguate two same-named consts in two fns.
// ════════════════════════════════════════════════════════════════════════════

/// Strip the type wrappers transparent to a const's runtime value (`as const`,
/// `satisfies T`, parens). Port of v5 `ts_unwrap_const`.
fn unwrap_const<'a>(e: &'a ts::Expression<'a>) -> &'a ts::Expression<'a> {
    match e {
        ts::Expression::TSAsExpression(t) => unwrap_const(&t.expression),
        ts::Expression::TSSatisfiesExpression(t) => unwrap_const(&t.expression),
        ts::Expression::ParenthesizedExpression(p) => unwrap_const(&p.expression),
        other => other,
    }
}

/// Whether an initializer is `... as const` — the only `let`/`var` form honest to
/// fold besides a true `const`. Port of v5's `as_const` check.
fn init_is_as_const(e: &ts::Expression) -> bool {
    matches!(e, ts::Expression::TSAsExpression(t) if t.type_annotation.is_const_type_reference())
}

/// Whether an initializer carries a string somewhere (a literal, a template, or
/// an object with a string-bearing property). Gates entity-minting: a const with
/// no string anywhere emits no entity and no values. Port of v5
/// `ts_expr_string_bearing` (spread never makes an object string-bearing).
fn expr_string_bearing(e: &ts::Expression) -> bool {
    match unwrap_const(e) {
        ts::Expression::StringLiteral(_) | ts::Expression::TemplateLiteral(_) => true,
        ts::Expression::ObjectExpression(o) => o.properties.iter().any(|p| match p {
            ts::ObjectPropertyKind::ObjectProperty(prop) => expr_string_bearing(&prop.value),
            ts::ObjectPropertyKind::SpreadProperty(_) => false,
        }),
        _ => false,
    }
}

/// The (VariableDeclaration | export-wrapped VariableDeclaration) in a statement.
fn var_decl_of<'a>(stmt: &'a ts::Statement<'a>) -> Option<&'a ts::VariableDeclaration<'a>> {
    use ts::Statement as S;
    match stmt {
        S::VariableDeclaration(v) => Some(v),
        S::ExportNamedDeclaration(exp) => match &exp.declaration {
            Some(ts::Declaration::VariableDeclaration(v)) => Some(v),
            _ => None,
        },
        _ => None,
    }
}

/// The (TSEnumDeclaration | export-wrapped) in a statement.
fn enum_decl_of<'a>(stmt: &'a ts::Statement<'a>) -> Option<&'a ts::TSEnumDeclaration<'a>> {
    use ts::Statement as S;
    match stmt {
        S::TSEnumDeclaration(en) => Some(en),
        S::ExportNamedDeclaration(exp) => match &exp.declaration {
            Some(ts::Declaration::TSEnumDeclaration(en)) => Some(en),
            _ => None,
        },
        _ => None,
    }
}

/// Walks for string-bearing consts: top-level (depth 0, via `collect_const_facts`'
/// loop) + nested inside fn/arrow bodies (depth > 0, via the visitor). Owns the
/// collected Const entities + ConstValue rows; drains into the TypeF bundle. The
/// `&mut Strings` is the shared per-file interner. Port of v5
/// `TsNestedConstWalker` (scope names dropped: spans disambiguate).
struct ConstWalker<'s> {
    content: &'s str,
    depth: u32,
    strings: &'s mut Strings,
    entities: Vec<Node<TypeF>>,
    values: Vec<ConstValue>,
}

impl<'s> ConstWalker<'s> {
    /// Recursively collect ConstValue rows from one initializer. `owner` is the
    /// owning Const entity's span; `prefix` the dotted field path so far (None at
    /// the top). Port of v5 `ts_collect_const_values` (spread skips dropped).
    fn collect_values(&mut self, e: &ts::Expression, owner: Span, prefix: Option<NameId>) {
        use oxc_span::GetSpan;
        match unwrap_const(e) {
            ts::Expression::StringLiteral(s) => {
                self.values.push(ConstValue {
                    owner,
                    field: prefix,
                    text: self.strings.intern(&s.value),
                    kind: ConstKind::Lit,
                });
            }
            ts::Expression::TemplateLiteral(t) => {
                let span = t.span();
                let text = self.content.get(span.start as usize..span.end as usize).unwrap_or_default();
                self.values.push(ConstValue {
                    owner,
                    field: prefix,
                    text: self.strings.intern(text),
                    kind: ConstKind::Template,
                });
            }
            ts::Expression::ObjectExpression(o) => {
                for property in &o.properties {
                    if let ts::ObjectPropertyKind::ObjectProperty(prop) = property {
                        let key = match &prop.key {
                            ts::PropertyKey::StaticIdentifier(i) => Some(i.name.to_string()),
                            ts::PropertyKey::StringLiteral(s) => Some(s.value.to_string()),
                            _ => None, // computed key: no static field name
                        };
                        if let Some(key) = key {
                            let field = match prefix {
                                None => Some(self.strings.intern(&key)),
                                Some(parent) => Some(
                                    self.strings.intern(&format!("{}.{key}", self.strings.lookup(parent))),
                                ),
                            };
                            self.collect_values(&prop.value, owner, field);
                        }
                    }
                    // spread: counted in v5; dropped here (facts-only).
                }
            }
            _ => {}
        }
    }

    /// One `const`/`as const` binding: a Const entity (if string-bearing + honest)
    /// + its values. Arrow/fn-expr inits are TypeProjector's Function entities,
    /// left alone. Port of v5 `ts_var_const_facts` (mutable-skip counter dropped).
    fn var_facts(&mut self, v: &ts::VariableDeclaration) {
        for declarator in &v.declarations {
            let ts::BindingPattern::BindingIdentifier(name) = &declarator.id else { continue };
            let Some(init) = &declarator.init else { continue };
            if matches!(
                init,
                ts::Expression::ArrowFunctionExpression(_) | ts::Expression::FunctionExpression(_)
            ) {
                continue;
            }
            if !expr_string_bearing(init) {
                continue;
            }
            if !v.kind.is_const() && !init_is_as_const(init) {
                continue; // mutable binding: v5 counts const_mutable_skips; v6 drops (facts-only)
            }
            let owner = to_span(declarator.span);
            self.entities
                .push(Node::new(owner, TypeEntityKind::Const).with_name(self.strings.intern(&name.name)));
            self.collect_values(init, owner, None);
        }
    }

    /// String-enum members (`enum R { Home = '/home' }`): one ConstValue per
    /// string-initialized member, keyed off the ENUM entity's span (the enum is
    /// already a TypeF node from TypeProjector; no per-member entity). Port of v5
    /// `ts_enum_const_values`.
    fn enum_facts(&mut self, en: &ts::TSEnumDeclaration) {
        let owner = to_span(en.span);
        for member in &en.body.members {
            let field = match &member.id {
                ts::TSEnumMemberName::Identifier(id) => Some(id.name.to_string()),
                ts::TSEnumMemberName::String(s) => Some(s.value.to_string()),
                _ => None,
            };
            if let (Some(field), Some(init)) = (field, &member.initializer) {
                if let ts::Expression::StringLiteral(s) = unwrap_const(init) {
                    self.values.push(ConstValue {
                        owner,
                        field: Some(self.strings.intern(&field)),
                        text: self.strings.intern(&s.value),
                        kind: ConstKind::Lit,
                    });
                }
            }
        }
    }
}

impl<'a, 's> OxcVisit<'a> for ConstWalker<'s> {
    fn visit_function(&mut self, it: &ts::Function<'a>, flags: oxc_syntax::scope::ScopeFlags) {
        self.depth += 1;
        oxc_ast_visit::walk::walk_function(self, it, flags);
        self.depth -= 1;
    }

    fn visit_arrow_function_expression(&mut self, it: &ts::ArrowFunctionExpression<'a>) {
        self.depth += 1;
        oxc_ast_visit::walk::walk_arrow_function_expression(self, it);
        self.depth -= 1;
    }

    fn visit_variable_declaration(&mut self, it: &ts::VariableDeclaration<'a>) {
        // depth 0 is the top-level loop's job; only act inside a fn/arrow body.
        if self.depth > 0 {
            self.var_facts(it);
        }
        oxc_ast_visit::walk::walk_variable_declaration(self, it);
    }
}

/// Top-level driver: top-level `const`/`enum` declarations, then descend into
/// fn/arrow bodies for nested consts. Appends Const entities + ConstValue rows
/// to the TypeF bundle. Needs `content` for template raw slices.
pub(crate) fn collect_const_facts(
    program: &Program,
    content: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let mut walker = ConstWalker { content, depth: 0, strings, entities: Vec::new(), values: Vec::new() };
    for stmt in &program.body {
        if let Some(v) = var_decl_of(stmt) {
            walker.var_facts(v);
        }
        if let Some(en) = enum_decl_of(stmt) {
            walker.enum_facts(en);
        }
    }
    walker.visit_program(program);
    sink.nodes.extend(walker.entities);
    sink.aux.consts.extend(walker.values);
}

// ════════════════════════════════════════════════════════════════════════════
// TsSource: the TS/JS Source (cst via ast-grep + type/call/df via oxc). Epic U.
//
// The two-parser, masked shape. cst runs through ast-grep (one dep = the CST
// floor for every lang); type/call/df run through ONE oxc parse (three masked
// projections over the same tree). ONE shared `Strings` across all four families.
// A .ts/.tsx/.js/... file with all families masked = 2 parses; the masked
// bundle's "one parse" is WITHIN a parser (one oxc parse feeds 3 projections).
// ════════════════════════════════════════════════════════════════════════════

/// The TS/JS `Source`. `matches` = oxc has a source type for the path
/// (.ts/.tsx/.mts/.cts/.js/.jsx/.mjs/.cjs).
#[derive(Default)]
pub struct TsSource;

impl Source for TsSource {
    fn name(&self) -> &'static str {
        "ts"
    }

    fn matches(&self, path: &str) -> bool {
        source_type_for(path).is_some()
    }

    fn extract(&self, path: &str, content: &[u8], mask: FamilyMask) -> ExtractOutput {
        let mut strings = Strings::new();

        // cst via ast-grep (masked). Owns its () arena; dropped at block end. A
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

        // type/call/df via ONE oxc parse (masked). Owns the Allocator; the Program
        // borrows it + content, both dropped at block end. A failed parse leaves
        // all three None (partial output: cst above may still be Some).
        let mut types = None;
        let mut call = None;
        let mut df = None;
        if mask.types || mask.call || mask.df {
            let arena = OxcParser.make_arena();
            if let Ok(parsed) = OxcParser.parse(&arena, path, content) {
                if mask.types {
                    let mut bundle = FamilyBundle::<TypeF>::default();
                    TypeProjector.project(&parsed, &mut strings, &mut bundle);
                    // const facet (port of v5 ts_const_facts_from): needs the
                    // source bytes for template slices. Appends Const entities +
                    // ConstValue rows to the same TypeF bundle.
                    if let Ok(src) = std::str::from_utf8(content) {
                        collect_const_facts(&parsed, src, &mut strings, &mut bundle);
                    }
                    types = Some(bundle);
                }
                if mask.call {
                    let mut bundle = FamilyBundle::<CallF>::default();
                    CallProjector.project(&parsed, &mut strings, &mut bundle);
                    call = Some(bundle);
                }
                if mask.df {
                    let mut bundle = FamilyBundle::<DfF>::default();
                    DfProjector.project(&parsed, &mut strings, &mut bundle);
                    df = Some(bundle);
                }
            }
        }

        ExtractOutput { strings, cst, types, call, df }
    }
}
