//! `impl Rename for TsSource`: every question `extract rename` asks a language,
//! answered for the TS family over the anchor file. Spans come off
//! `oxc_semantic`'s scope plane, which is the only TS seat in this crate that is
//! identifier-exact (`plans/2026-08-27-extract-rename.PLAN.md:113`).
//! @comment-ok: module header, the seam list every lang file opens with

//! The importer graph comes off `TsRehome::import_refs` (`ts_rehome.rs:33`),
//! which already resolves every specifier through `oxc_resolver`.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use oxc_ast::ast as ts;
use oxc_ast::ast::Program;
use oxc_ast_visit::Visit;
use oxc_semantic::{Scoping, Semantic, SemanticBuilder};
use oxc_span::GetSpan;
use oxc_syntax::reference::Reference;
use oxc_syntax::symbol::SymbolId;

use crate::lang::ts::{OxcParser, TsSource};
use crate::move_cx::MoveCx;
use crate::rename_cx::{RenameCx, RenameRequest};
use crate::seams::Parser;
use crate::types::{
    ImportRefKind, RefRole, Rehome, Rename, RenameStop, Respell, Span, SymbolRef, SymbolSeat,
};

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

        let mut candidates: Vec<SymbolId> = scoping
            .scope_descendants_from_root()
            .flat_map(|scope| scoping.iter_bindings_in(scope))
            .filter(|symbol| scoping.symbol_name(*symbol) == request.old)
            .collect();
        candidates.sort_by_key(|symbol| scoping.symbol_span(*symbol).start);
        let symbol = match candidates.as_slice() {
            [] => return Err(not_found(request)),
            [one] => *one,
            many => select_by_at(scoping, many, request.at).ok_or_else(|| {
                ambiguous(
                    request,
                    many.iter()
                        .map(|s| to_span(scoping.symbol_span(*s)))
                        .collect(),
                )
            })?,
        };
        // A TS merged declaration (`interface Foo` + `const Foo`) is ONE symbol
        // wearing several binding identifiers, so it is ambiguous too.
        let redeclarations = scoping.symbol_redeclarations(symbol);
        if !redeclarations.is_empty() {
            let mut sites = vec![to_span(scoping.symbol_span(symbol))];
            sites.extend(redeclarations.iter().map(|other| to_span(other.span)));
            return Err(ambiguous(request, sites));
        }

        let seats = dynamic_seats(&program, &request.anchor, &request.old);
        if !seats.is_empty() {
            return Err(RenameStop::Dynamic(seats));
        }

        let mut refs = binding_refs(&semantic, symbol, &request.anchor, &request.old);
        // A symbol no importer can name is file-local, so the run opens one file.
        if exports_bare(&program, &request.old) {
            refs.extend(importer_refs(cx, request));
        }
        Ok(settle(refs))
    }

    fn respell_symbol(
        &self,
        _cx: &RenameCx,
        request: &RenameRequest,
        reference: &SymbolRef,
    ) -> Option<Respell> {
        Some(Respell {
            file: reference.file.clone(),
            span: reference.span,
            text: request.new.clone(),
            receipt: None,
        })
    }
}

fn not_found(request: &RenameRequest) -> RenameStop {
    RenameStop::NotFound {
        anchor: request.anchor.clone(),
        old: request.old.clone(),
    }
}

/// `--at` picks the declaration the byte offset lands in. When the offset sits
/// between declarations, the nearest declaration opening at or before it wins,
/// so an offset anywhere inside a declaration body still selects it. `None`
/// means the caller reports every candidate as ambiguous.
fn select_by_at(
    scoping: &Scoping,
    candidates: &[SymbolId],
    at: Option<u32>,
) -> Option<SymbolId> {
    let at = at?;
    let inside: Vec<SymbolId> = candidates
        .iter()
        .copied()
        .filter(|symbol| {
            let span = scoping.symbol_span(*symbol);
            span.start <= at && at < span.end
        })
        .collect();
    match inside.as_slice() {
        [one] => Some(*one),
        [] => candidates
            .iter()
            .copied()
            .filter(|symbol| scoping.symbol_span(*symbol).start <= at)
            .max_by_key(|symbol| scoping.symbol_span(*symbol).start),
        _ => None,
    }
}

/// One binding's own identifier plus every reference the scope plane resolves to
/// it, all inside `file`.
fn binding_refs(
    semantic: &Semantic<'_>,
    symbol: SymbolId,
    file: &str,
    name: &str,
) -> Vec<SymbolRef> {
    let scoping = semantic.scoping();
    let mut refs = vec![SymbolRef {
        file: file.to_string(),
        span: to_span(scoping.symbol_span(symbol)),
        role: RefRole::Definition,
        text: name.to_string(),
    }];
    for reference in scoping.get_resolved_references(symbol) {
        refs.push(SymbolRef {
            file: file.to_string(),
            span: to_span(semantic.nodes().kind(reference.node_id()).span()),
            role: role_of(reference),
            text: name.to_string(),
        });
    }
    refs
}

/// One seat per `(file, offset)`, in plan order. A bare `import {OLD}` writes
/// the imported name and the local binding with ONE token, so both walks meet.
fn settle(mut refs: Vec<SymbolRef>) -> Vec<SymbolRef> {
    refs.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then(left.span.start.cmp(&right.span.start))
    });
    refs.dedup_by(|left, right| left.file == right.file && left.span.start == right.span.start);
    refs
}

// ── the importer walk ───────────────────────────────────────────────────────

/// Target module -> importer -> the source-literal offsets reaching it. Keying
/// on the offset re-uses `oxc_resolver`'s answer instead of resolving twice.
type ImportGraph = BTreeMap<String, BTreeMap<String, BTreeSet<u32>>>;

/// What one importer answers about a module it imports the symbol from.
struct ImporterSeats {
    refs: Vec<SymbolRef>,
    /// The names this importer re-exports the symbol under, for the next hop.
    exports: Vec<String>,
}

/// Every seat outside the anchor, breadth first. A queue entry is a module and
/// the name it exports the symbol under; an aliasing relay ends that branch.
fn importer_refs(cx: &RenameCx, request: &RenameRequest) -> Vec<SymbolRef> {
    let graph = import_graph(cx);
    let mut refs = Vec::new();
    let mut queue: VecDeque<(String, String)> = VecDeque::new();
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    seen.insert((request.anchor.clone(), request.old.clone()));
    queue.push_back((request.anchor.clone(), request.old.clone()));
    while let Some((module, name)) = queue.pop_front() {
        let Some(importers) = graph.get(&module) else {
            continue;
        };
        for (importer, sources) in importers {
            let seats = importer_seats(cx, importer, sources, &name);
            refs.extend(seats.refs);
            for exported in seats.exports {
                if seen.insert((importer.clone(), exported.clone())) {
                    queue.push_back((importer.clone(), exported));
                }
            }
        }
    }
    refs
}

/// One importer's seats for `name`, over the import and re-export clauses whose
/// module specifier sits at one of `sources`.
fn importer_seats(
    cx: &RenameCx,
    rel: &str,
    sources: &BTreeSet<u32>,
    name: &str,
) -> ImporterSeats {
    let mut out = ImporterSeats {
        refs: Vec::new(),
        exports: Vec::new(),
    };
    let Some(text) = cx.text(rel) else {
        return out;
    };
    let parser = OxcParser;
    let arena = parser.make_arena();
    let Ok(program) = parser.parse(&arena, rel, text.as_bytes()) else {
        return out;
    };
    let semantic = SemanticBuilder::new().build(&program).semantic;
    for statement in &program.body {
        match statement {
            ts::Statement::ImportDeclaration(import) => {
                if !sources.contains(&import.source.span.start) {
                    continue;
                }
                let Some(specifiers) = &import.specifiers else {
                    continue;
                };
                for specifier in specifiers {
                    let ts::ImportDeclarationSpecifier::ImportSpecifier(named) = specifier else {
                        continue;
                    };
                    import_seat(&semantic, rel, named, name, &mut out);
                }
            }
            ts::Statement::ExportNamedDeclaration(export) => {
                let Some(source) = &export.source else {
                    continue;
                };
                if !sources.contains(&source.span.start) {
                    continue;
                }
                for specifier in &export.specifiers {
                    relay_seat(rel, specifier, name, &mut out);
                }
            }
            ts::Statement::ExportAllDeclaration(export) => {
                if !sources.contains(&export.source.span.start) {
                    continue;
                }
                // `export * as ns from` binds a namespace object: the symbol is
                // a property at that seat, never an identifier the plane binds.
                if export.exported.is_none() {
                    out.exports.push(name.to_string());
                }
            }
            _ => {}
        }
    }
    out
}

/// One `import { NAME }` / `import { NAME as local }` clause. The aliased form
/// moves the imported seat alone: `local` is a name this file owns.
fn import_seat(
    semantic: &Semantic<'_>,
    rel: &str,
    named: &ts::ImportSpecifier<'_>,
    name: &str,
    out: &mut ImporterSeats,
) {
    if plain_name(&named.imported) != Some(name) {
        return;
    }
    out.refs.push(SymbolRef {
        file: rel.to_string(),
        span: to_span(named.imported.span()),
        role: RefRole::Import,
        text: name.to_string(),
    });
    if !one_token(named.imported.span(), named.local.span) {
        return;
    }
    let local = named.local.name.as_str();
    if let Some(symbol) = binding_at(semantic.scoping(), named.local.span.start, local) {
        out.refs.extend(binding_refs(semantic, symbol, rel, name));
    }
    if exports_bare(semantic.nodes().program(), local) {
        out.exports.push(local.to_string());
    }
}

/// One `export { NAME } from "./m"` clause. The bare form carries the rename
/// onward; `export { NAME as other } from` pins `other` and ends the branch.
fn relay_seat(rel: &str, specifier: &ts::ExportSpecifier<'_>, name: &str, out: &mut ImporterSeats) {
    if plain_name(&specifier.local) != Some(name) {
        return;
    }
    out.refs.push(SymbolRef {
        file: rel.to_string(),
        span: to_span(specifier.local.span()),
        role: RefRole::Export,
        text: name.to_string(),
    });
    if one_token(specifier.local.span(), specifier.exported.span()) {
        out.exports.push(name.to_string());
    }
}

/// The corpus import graph, off `TsRehome::import_refs`. ONE per root per
/// process, the law `ts_rehome.rs:435` sets for the resolver behind it.
fn import_graph(cx: &RenameCx) -> &'static ImportGraph {
    static CACHE: OnceLock<Mutex<BTreeMap<PathBuf, &'static ImportGraph>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    static EMPTY: OnceLock<ImportGraph> = OnceLock::new();
    let empty = || EMPTY.get_or_init(ImportGraph::new);
    let Ok(mut held) = cache.lock() else {
        return empty();
    };
    if let Some(existing) = held.get(cx.root()) {
        return existing;
    }
    let leaked: &'static ImportGraph = Box::leak(Box::new(build_import_graph(cx)));
    held.insert(cx.root().to_path_buf(), leaked);
    leaked
}

/// `import_refs` reports the specifiers a MOVE would re-aim, so the batch maps
/// every TS file to itself: every file a target, no specifier filtered by name.
fn build_import_graph(cx: &RenameCx) -> ImportGraph {
    let Ok(move_cx) = MoveCx::open(cx.root()) else {
        return ImportGraph::new();
    };
    let batch: BTreeMap<String, String> = move_cx
        .files_of(&TsSource)
        .into_iter()
        .map(|rel| (rel.to_string(), rel.to_string()))
        .collect();
    let move_cx = move_cx.with_batch(batch, false);
    let mut graph = ImportGraph::new();
    for reference in TsSource.import_refs(&move_cx) {
        if reference.kind != ImportRefKind::Import {
            continue;
        }
        graph
            .entry(reference.target)
            .or_default()
            .entry(reference.importer)
            .or_default()
            .insert(reference.literal.start);
    }
    graph
}

/// The binding whose own identifier opens at `start`.
fn binding_at(scoping: &Scoping, start: u32, name: &str) -> Option<SymbolId> {
    scoping
        .scope_descendants_from_root()
        .flat_map(|scope| scoping.iter_bindings_in(scope))
        .find(|symbol| {
            scoping.symbol_span(*symbol).start == start && scoping.symbol_name(*symbol) == name
        })
}

/// Whether two module-clause halves are the one written token, which is what
/// `{NAME}` writes and `{NAME as other}` does not.
fn one_token(left: oxc_span::Span, right: oxc_span::Span) -> bool {
    left.start == right.start && left.end == right.end
}

/// The identifier token a module export name writes. A string-literal name
/// (`export { "a b" as x }`) is data, so it writes none.
fn plain_name<'a>(name: &ts::ModuleExportName<'a>) -> Option<&'a str> {
    match name {
        ts::ModuleExportName::IdentifierName(id) => Some(id.name.as_str()),
        ts::ModuleExportName::IdentifierReference(id) => Some(id.name.as_str()),
        ts::ModuleExportName::StringLiteral(_) => None,
    }
}

// ── the runtime seats ───────────────────────────────────────────────────────

/// One runtime-only seat: the bytes, and how they reach the symbol. A seat is
/// any member access spelling `old` that the scope plane never binds, so a
/// rename that skipped it could silently miss the real call site.
type DynamicSeat = (oxc_span::Span, &'static str);

/// Every seat in the anchor, earliest first. Importers are outside this scan: a
/// property named `old` on any object anywhere would stop every run.
fn dynamic_seats(program: &Program<'_>, file: &str, old: &str) -> Vec<SymbolSeat> {
    let mut scan = DynamicScan {
        old,
        seats: Vec::new(),
    };
    scan.visit_program(program);
    let mut seats = scan.seats;
    seats.sort_by_key(|(span, _)| span.start);
    seats
        .into_iter()
        .map(|(span, form)| SymbolSeat {
            file: file.to_string(),
            span: to_span(span),
            form,
        })
        .collect()
}

struct DynamicScan<'a> {
    old: &'a str,
    seats: Vec<DynamicSeat>,
}

impl<'a> Visit<'a> for DynamicScan<'a> {
    fn visit_computed_member_expression(&mut self, expression: &ts::ComputedMemberExpression<'a>) {
        if let ts::Expression::StringLiteral(literal) = &expression.expression {
            if literal.value.as_str() == self.old {
                self.seats
                    .push((expression.expression.span(), "computed member"));
            }
        }
        self.visit_expression(&expression.object);
    }

    fn visit_static_member_expression(&mut self, expression: &ts::StaticMemberExpression<'a>) {
        if expression.property.name.as_str() == self.old {
            self.seats
                .push((expression.property.span(), "member access"));
        }
        self.visit_expression(&expression.object);
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

/// Whether the module surface carries `name` under the SAME token that binds it.
/// An aliased clause pins the public name, so no importer of it needs repairing.
fn exports_bare(program: &Program<'_>, name: &str) -> bool {
    program.body.iter().any(|statement| match statement {
        ts::Statement::ExportNamedDeclaration(export) if export.source.is_none() => {
            export
                .declaration
                .as_ref()
                .is_some_and(|declaration| declares(declaration, name))
                || export.specifiers.iter().any(|specifier| {
                    plain_name(&specifier.local) == Some(name)
                        && one_token(specifier.local.span(), specifier.exported.span())
                })
        }
        _ => false,
    })
}

/// Whether one exported declaration binds `name` at its top level.
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
