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
use oxc_span::SourceType;

use crate::family::{SigSlot, TypeEntityKind, TypeF};
use crate::rows::{FamilyBundle, Node};
use crate::seams::{ParseError, Parser, Project};
use crate::shape::{Span, Strings};

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
