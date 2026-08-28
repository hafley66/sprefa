//! `impl Rename for TsSource`: every question `extract rename` asks a language,
//! answered for the TS family over the anchor file. Spans come off
//! `oxc_semantic`'s scope plane, which is the only TS seat in this crate that is
//! identifier-exact (`plans/2026-08-27-extract-rename.PLAN.md:113`).
//! @comment-ok: module header, the seam list every lang file opens with

use oxc_ast::ast as ts;
use oxc_ast::ast::Program;
use oxc_semantic::SemanticBuilder;
use oxc_span::GetSpan;
use oxc_syntax::reference::Reference;
use oxc_syntax::symbol::SymbolId;

use crate::lang::ts::{OxcParser, TsSource};
use crate::rename_cx::{RenameCx, RenameRequest};
use crate::seams::Parser;
use crate::types::{RefRole, Rename, RenameStop, Respell, Span, SymbolRef};

impl Rename for TsSource {
    fn symbol_refs(
        &self,
        cx: &RenameCx,
        request: &RenameRequest,
    ) -> Result<Vec<SymbolRef>, RenameStop> {
        let text = cx.text(&request.anchor).ok_or_else(|| not_found(request))?;
        let parser = OxcParser;
        let arena = parser.make_arena();
        let program = parser
            .parse(&arena, &request.anchor, text.as_bytes())
            .map_err(|_| not_found(request))?;
        let semantic = SemanticBuilder::new().build(&program).semantic;
        let scoping = semantic.scoping();
        let root = scoping.root_scope_id();

        let bound: Vec<SymbolId> = scoping
            .iter_bindings_in(root)
            .filter(|symbol| scoping.symbol_name(*symbol) == request.old)
            .collect();
        let symbol = match bound.as_slice() {
            [] => return Err(not_found(request)),
            [one] => *one,
            many => {
                let sites = many
                    .iter()
                    .map(|other| to_span(scoping.symbol_span(*other)))
                    .collect();
                return Err(ambiguous(request, sites));
            }
        };
        // A TS merged declaration (`interface Foo` + `const Foo`) is ONE symbol
        // wearing several binding identifiers, so it is ambiguous too.
        let redeclarations = scoping.symbol_redeclarations(symbol);
        if !redeclarations.is_empty() {
            let mut sites = vec![to_span(scoping.symbol_span(symbol))];
            sites.extend(redeclarations.iter().map(|other| to_span(other.span)));
            return Err(ambiguous(request, sites));
        }

        let mut refs = vec![SymbolRef {
            file: request.anchor.clone(),
            span: to_span(scoping.symbol_span(symbol)),
            role: RefRole::Definition,
            text: request.old.clone(),
        }];
        for reference in scoping.get_resolved_references(symbol) {
            refs.push(SymbolRef {
                file: request.anchor.clone(),
                span: to_span(semantic.nodes().kind(reference.node_id()).span()),
                role: role_of(reference),
                text: request.old.clone(),
            });
        }
        refs.sort_by_key(|reference| reference.span.start);
        Ok(refs)
    }

    fn respell_symbol(
        &self,
        cx: &RenameCx,
        request: &RenameRequest,
        reference: &SymbolRef,
    ) -> Option<Respell> {
        // Arc 1 reaches one file, so the exported anchor's importers are still
        // unrepaired; the definition seat carries that warning once.
        let receipt = match reference.role == RefRole::Definition && exported(cx, request) {
            true => Some(format!(
                "public: {} is exported; importers are arc 3",
                request.old
            )),
            false => None,
        };
        Some(Respell {
            file: reference.file.clone(),
            span: reference.span,
            text: request.new.clone(),
            receipt,
        })
    }
}

fn not_found(request: &RenameRequest) -> RenameStop {
    RenameStop::NotFound {
        anchor: request.anchor.clone(),
        old: request.old.clone(),
    }
}

fn ambiguous(request: &RenameRequest, sites: Vec<Span>) -> RenameStop {
    RenameStop::Ambiguous {
        anchor: request.anchor.clone(),
        old: request.old.clone(),
        sites,
    }
}

/// `ReferenceFlags` onto `RefRole`. Write wins over Read: a compound assignment
/// carries both, and the write is the stronger statement about the seat.
fn role_of(reference: &Reference) -> RefRole {
    if reference.is_write() {
        return RefRole::Write;
    }
    if reference.is_type() {
        return RefRole::TypeRef;
    }
    RefRole::Read
}

/// Whether the anchor's module surface carries `request.old`: an `export`ed
/// declaration binding it, or an `export { old }` clause naming it.
fn exported(cx: &RenameCx, request: &RenameRequest) -> bool {
    let Some(text) = cx.text(&request.anchor) else {
        return false;
    };
    let parser = OxcParser;
    let arena = parser.make_arena();
    let Ok(program) = parser.parse(&arena, &request.anchor, text.as_bytes()) else {
        return false;
    };
    program_exports(&program, &request.old)
}

fn program_exports(program: &Program<'_>, name: &str) -> bool {
    program.body.iter().any(|statement| match statement {
        ts::Statement::ExportNamedDeclaration(export) => {
            export
                .declaration
                .as_ref()
                .is_some_and(|declaration| declares(declaration, name))
                || export
                    .specifiers
                    .iter()
                    .any(|specifier| specifier.local.name() == name)
        }
        ts::Statement::ExportDefaultDeclaration(export) => match &export.declaration {
            ts::ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                class.id.as_ref().is_some_and(|id| id.name == name)
            }
            ts::ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                func.id.as_ref().is_some_and(|id| id.name == name)
            }
            _ => false,
        },
        _ => false,
    })
}

/// Whether one exported declaration binds `name` at its top level. A destructured
/// binding pattern is not reached here; it is an arc-2 `Inexact` seat.
fn declares(declaration: &ts::Declaration<'_>, name: &str) -> bool {
    match declaration {
        ts::Declaration::VariableDeclaration(var) => var.declarations.iter().any(|declarator| {
            matches!(&declarator.id, ts::BindingPattern::BindingIdentifier(id) if id.name == name)
        }),
        ts::Declaration::FunctionDeclaration(func) => {
            func.id.as_ref().is_some_and(|id| id.name == name)
        }
        ts::Declaration::ClassDeclaration(class) => {
            class.id.as_ref().is_some_and(|id| id.name == name)
        }
        ts::Declaration::TSTypeAliasDeclaration(alias) => alias.id.name == name,
        ts::Declaration::TSInterfaceDeclaration(interface) => interface.id.name == name,
        ts::Declaration::TSEnumDeclaration(enumeration) => enumeration.id.name == name,
        _ => false,
    }
}

fn to_span(span: oxc_span::Span) -> Span {
    Span {
        start: span.start,
        len: span.end - span.start,
    }
}
