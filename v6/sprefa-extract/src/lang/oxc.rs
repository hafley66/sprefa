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

use crate::family::{CallF, CallKind, CallSite, SigSlot, TypeEntityKind, TypeF};
use crate::rows::{FamilyBundle, Node};
use crate::seams::{ParseError, Parser, Project};
use crate::shape::{Span, Strings};

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
