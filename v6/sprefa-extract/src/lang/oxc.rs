//! The oxc Parser + the TypeF projection (TS/JS type entities). oxc is the
//! arena AST front-end v5 carried (`src/graph/typegraph/ts`). `Program<'a>`
//! borrows the `Allocator` AND the source text, so the dispatch owns the arena
//! and the parse ties `content` to `'a` (the GAT seam from commit 2a).
//!
//! Commit 2b ports v5 `ts_entities_from`: the type declarations (class /
//! interface / alias / enum / function / method) become span-addressed
//! `Node<TypeF>`. The type EDGES (field / impl / uses / ...) are name-resolved
//! relationships and land with `Resolve<TypeF>` (commit 4, scip-typescript);
//! phase 1 stays pure-content span nodes.

use oxc_allocator::Allocator;
use oxc_ast::ast as ts;
use oxc_ast::ast::Program;
use oxc_span::SourceType;

use crate::family::{TypeEntityKind, TypeF};
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
            }
        }
    }
}

fn fn_entity(func: &ts::Function, strings: &mut Strings, sink: &mut FamilyBundle<TypeF>) {
    let Some(id) = &func.id else { return };
    push_entity(sink, strings, func.span, id.name.to_string(), TypeEntityKind::Function);
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
        let is_function = match &declarator.init {
            Some(
                ts::Expression::ArrowFunctionExpression(_) | ts::Expression::FunctionExpression(_),
            ) => true,
            _ => false,
        };
        if !is_function {
            continue;
        }
        push_entity(sink, strings, declarator.span, name.name.to_string(), TypeEntityKind::Function);
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
