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

use std::collections::{BTreeSet, HashMap};

use oxc_allocator::Allocator;
use oxc_ast::ast as ts;
use oxc_ast::ast::Program;
use oxc_ast_visit::Visit as OxcVisit;
use oxc_span::{GetSpan, SourceType};

use super::astgrep::{AstGrepParser, CstProjector};
use super::ts_resolve::{ResolvedImport, TsModuleIndex};
use crate::family::{
    CallEdgeKind, CallF, CallKind, CallSite, ConstKind, ConstValue, CstF, DfArg, DfEdgeKind, DfF,
    DfField, DfLit, DfNodeKind, DfParam, DocFact, DocTag, ProjectEdge, SigSlot, Specifier,
    SpecifierKind, TypeEdgeCandidate, TypeEdgeKind, TypeEntityKind, TypeF,
};
use crate::rows::{Edge, FamilyBundle, Node};
use crate::scip::{byte_range, definition_of, join_documents, site_occurrence};
use crate::seams::{
    corpus_defs, covering_def, def_named, own_blob, DefIndex, DefSite, ParseError, Parser, Project,
    Resolve,
};
use crate::shape::{ContentId, FamilyTag, NameId, NodeRef, Span, Strings, ZERO_CONTENT_ID};
use crate::source::{ExtractOutput, FamilyMask, ProjectCx, Source};
use crate::trace;
use crate::types::LangKind;
use crate::types::{KindIndex, ScipIndex};
use crate::types::{
    content_id_of, RefPosition, Reference, Unresolved, UnresolvedReason,
};

use super::ts_receivers;

/// `oxc_span::Span` (start + end) -> our byte `Span` (start + len). One
/// coordinate; the engine derives line/col from the file bytes when needed.
fn to_span(s: oxc_span::Span) -> Span {
    Span {
        start: s.start,
        len: s.end - s.start,
    }
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
        let source_type =
            source_type_for(path).ok_or_else(|| ParseError::NoGrammar(path.to_string()))?;
        let src = std::str::from_utf8(content).map_err(|err| ParseError::Utf8(err.to_string()))?;
        let ret = oxc_parser::Parser::new(arena, src, source_type).parse();
        if ret.panicked {
            return Err(ParseError::Parse(format!("oxc panicked on {path}")));
        }
        Ok(ret.program)
    }
}

/// TS/JS source type by extension. `.tsx` -> TSX; `.ts`/`.mts`/`.cts` -> TS;
/// `.js`/`.jsx`/`.mjs`/`.cjs` -> JSX-enabled JS. (v5 `source_type_for`.)
pub(crate) fn source_type_for(path: &str) -> Option<SourceType> {
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

/// `stmts` plus every statement nested in a `namespace`/`module`/`global`
/// block, source order: what each family's top-level loop iterates.
fn with_module_bodies<'s, 'a>(stmts: &'s [ts::Statement<'a>]) -> Vec<&'s ts::Statement<'a>> {
    let mut out = Vec::with_capacity(stmts.len());
    push_with_module_bodies(stmts, &mut out);
    out
}

fn push_with_module_bodies<'s, 'a>(
    stmts: &'s [ts::Statement<'a>],
    out: &mut Vec<&'s ts::Statement<'a>>,
) {
    for stmt in stmts {
        out.push(stmt);
        match module_decl_of(stmt) {
            Some(module) => push_with_module_bodies(module_block(module), out),
            None => {
                if let Some(global) = global_decl_of(stmt) {
                    push_with_module_bodies(&global.body.body, out);
                }
            }
        }
    }
}

/// `declare global {}` is its own oxc node, bare or `export`-wrapped.
fn global_decl_of<'s, 'a>(stmt: &'s ts::Statement<'a>) -> Option<&'s ts::TSGlobalDeclaration<'a>> {
    match stmt {
        ts::Statement::TSGlobalDeclaration(global) => Some(global),
        ts::Statement::ExportNamedDeclaration(export) => match &export.declaration {
            Some(ts::Declaration::TSGlobalDeclaration(global)) => Some(global),
            _ => None,
        },
        _ => None,
    }
}

/// The `TSModuleDeclaration` a statement declares, bare or `export`-wrapped.
fn module_decl_of<'s, 'a>(stmt: &'s ts::Statement<'a>) -> Option<&'s ts::TSModuleDeclaration<'a>> {
    match stmt {
        ts::Statement::TSModuleDeclaration(module) => Some(module),
        ts::Statement::ExportNamedDeclaration(export) => match &export.declaration {
            Some(ts::Declaration::TSModuleDeclaration(module)) => Some(module),
            _ => None,
        },
        _ => None,
    }
}

/// `namespace A.B {}` nests one `TSModuleDeclaration` per dotted segment and
/// only the innermost carries the block; `declare module "x";` carries none.
fn module_block<'s, 'a>(decl: &'s ts::TSModuleDeclaration<'a>) -> &'s [ts::Statement<'a>] {
    let mut current = decl;
    loop {
        match current.body.as_ref() {
            Some(ts::TSModuleDeclarationBody::TSModuleBlock(block)) => return &block.body,
            Some(ts::TSModuleDeclarationBody::TSModuleDeclaration(inner)) => current = inner,
            None => return &[],
        }
    }
}

/// The TypeF projector: walks the oxc `Program`, minting one entity node per
/// type/function declaration. Port of v5 `ts_entities_from` (entity half); the
/// arrow types and edge graph are dropped here (edges land with Resolve<TypeF>).
pub struct TypeProjector;

impl Project<TypeF> for TypeProjector {
    type Parsed<'a> = Program<'a>;

    fn project(
        &self,
        program: &Program<'_>,
        strings: &mut Strings,
        sink: &mut FamilyBundle<TypeF>,
    ) {
        for stmt in with_module_bodies(&program.body) {
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
                        push_entity(
                            sink,
                            strings,
                            interface.span,
                            interface.id.name.to_string(),
                            TypeEntityKind::Interface,
                        );
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
        // The second walk (mirroring v5's separate `ts_edges_from` pass): the
        // unresolved type-edge candidates (variant/field/impl/generic/uses).
        // param/returns candidates ride `fn_sigs` at the Function-entity call
        // sites above (v5 emits no method-signature type_edges).
        edge_candidates(program, strings, sink);
    }
}

// ── doc facet (port of v5 `ts_docs_from`) ────────────────────────────────────

/// Every `/** ... */` doc block bound to the entity it documents. Port of v5
/// `ts_docs_from`: nearest anchor at/after block end, whitespace between.
fn ts_doc_facts(
    program: &Program<'_>,
    content: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let anchors = ts_doc_anchors(program);
    if anchors.is_empty() {
        return;
    }
    for (cstart, cend) in ts_block_comments(content) {
        let raw = &content[cstart..cend];
        if !raw.trim_start().starts_with("/**") {
            continue;
        }
        let Some((at, owner, parent)) = anchors
            .iter()
            .filter(|(at, _, _)| (*at as usize) >= cend)
            .min_by_key(|(at, _, _)| *at)
        else {
            continue;
        };
        if !content[cend..*at as usize].trim().is_empty() {
            continue;
        }
        let text = clean_block_comment(raw);
        sink.aux.docs.push(DocFact {
            owner: *owner,
            parent: parent.as_deref().map(|name| strings.intern(name)),
            text: strings.intern(&text),
            tags: parse_jsdoc_tags(&text, strings),
        });
    }
}

/// `(anchor_byte, owner_span, parent)` per emitted entity; `anchor_byte` is the
/// statement start (before an export prefix, so the whitespace check passes).
fn ts_doc_anchors(program: &Program<'_>) -> Vec<(u32, Span, Option<String>)> {
    use oxc_span::GetSpan;
    let mut out = Vec::new();
    for stmt in &program.body {
        use ts::Statement as S;
        let at = stmt.span().start;
        match stmt {
            S::ExportNamedDeclaration(e) => {
                if let Some(d) = &e.declaration {
                    ts_decl_anchor(d, at, &mut out);
                }
            }
            S::ExportDefaultDeclaration(e) => match &e.declaration {
                ts::ExportDefaultDeclarationKind::ClassDeclaration(c) => {
                    ts_class_anchor(c, at, &mut out)
                }
                ts::ExportDefaultDeclarationKind::TSInterfaceDeclaration(i) => {
                    out.push((at, to_span(i.span), None))
                }
                ts::ExportDefaultDeclarationKind::FunctionDeclaration(f) => {
                    if f.id.is_some() {
                        out.push((at, to_span(f.span), None));
                    }
                }
                _ => {}
            },
            S::ClassDeclaration(c) => ts_class_anchor(c, at, &mut out),
            S::TSInterfaceDeclaration(i) => out.push((at, to_span(i.span), None)),
            S::TSTypeAliasDeclaration(a) => out.push((at, to_span(a.span), None)),
            S::TSEnumDeclaration(en) => out.push((at, to_span(en.span), None)),
            S::FunctionDeclaration(f) => {
                if f.id.is_some() {
                    out.push((at, to_span(f.span), None));
                }
            }
            S::VariableDeclaration(v) => ts_var_anchor(v, at, &mut out),
            _ => {}
        }
    }
    out
}

fn ts_decl_anchor(d: &ts::Declaration, at: u32, out: &mut Vec<(u32, Span, Option<String>)>) {
    match d {
        ts::Declaration::ClassDeclaration(c) => ts_class_anchor(c, at, out),
        ts::Declaration::TSInterfaceDeclaration(i) => out.push((at, to_span(i.span), None)),
        ts::Declaration::TSTypeAliasDeclaration(a) => out.push((at, to_span(a.span), None)),
        ts::Declaration::TSEnumDeclaration(en) => out.push((at, to_span(en.span), None)),
        ts::Declaration::FunctionDeclaration(f) => {
            if f.id.is_some() {
                out.push((at, to_span(f.span), None));
            }
        }
        ts::Declaration::VariableDeclaration(v) => ts_var_anchor(v, at, out),
        _ => {}
    }
}

fn ts_class_anchor(c: &ts::Class, at: u32, out: &mut Vec<(u32, Span, Option<String>)>) {
    let Some(id) = &c.id else { return };
    let owner = id.name.to_string();
    out.push((at, to_span(c.span), None));
    for el in &c.body.body {
        if let ts::ClassElement::MethodDefinition(m) = el {
            if m.kind == ts::MethodDefinitionKind::Constructor {
                continue;
            }
            if let ts::PropertyKey::StaticIdentifier(_) = &m.key {
                out.push((m.span.start, to_span(m.span), Some(owner.clone())));
            }
        }
    }
}

fn ts_var_anchor(v: &ts::VariableDeclaration, at: u32, out: &mut Vec<(u32, Span, Option<String>)>) {
    for d in &v.declarations {
        let ts::BindingPattern::BindingIdentifier(_) = &d.id else {
            continue;
        };
        if matches!(
            &d.init,
            Some(ts::Expression::ArrowFunctionExpression(_))
                | Some(ts::Expression::FunctionExpression(_))
        ) {
            out.push((at, to_span(d.span), None));
        }
    }
}

/// Byte ranges of every `/* ... */` block comment, including delimiters. Port
/// of v5 `ts_block_comments`.
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

/// Strip a `/** ... */` block down to its prose. Port of v5
/// `clean_block_comment`.
fn clean_block_comment(raw: &str) -> String {
    let inner = raw.trim();
    let inner = inner
        .strip_prefix("/**")
        .or_else(|| inner.strip_prefix("/*!"))
        .or_else(|| inner.strip_prefix("/*"))
        .unwrap_or(inner);
    let inner = inner.strip_suffix("*/").unwrap_or(inner);
    let mut lines: Vec<String> = inner
        .lines()
        .map(|l| {
            let t = l.trim_start();
            let t = t.strip_prefix('*').unwrap_or(t);
            t.strip_prefix(' ').unwrap_or(t).to_string()
        })
        .collect();
    while lines.first().is_some_and(|s| s.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|s| s.trim().is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

/// Split a JSDoc/KDoc block into `@tag` rows. Named tags carry a leading name
/// into `arg`; a leading `{type}` annotation is dropped. Port of v5.
fn parse_jsdoc_tags(text: &str, strings: &mut Strings) -> Vec<DocTag> {
    let mut out = Vec::new();
    for line in text.lines() {
        let l = line.trim_start();
        let Some(rest) = l.strip_prefix('@') else {
            continue;
        };
        let mut it = rest.splitn(2, char::is_whitespace);
        let tag = it.next().unwrap_or("").to_string();
        let mut body = it.next().unwrap_or("").trim_start();
        if body.starts_with('{') {
            if let Some(end) = body.find('}') {
                body = body[end + 1..].trim_start();
            }
        }
        let named = matches!(
            tag.as_str(),
            "param"
                | "arg"
                | "argument"
                | "property"
                | "prop"
                | "throws"
                | "exception"
                | "typeparam"
                | "tparam"
        );
        let (arg, desc) = if named {
            let mut bi = body.splitn(2, char::is_whitespace);
            (
                bi.next().unwrap_or("").to_string(),
                bi.next().unwrap_or("").trim().to_string(),
            )
        } else {
            (String::new(), body.trim().to_string())
        };
        out.push(DocTag {
            tag: strings.intern(&tag),
            arg: if arg.is_empty() {
                None
            } else {
                Some(strings.intern(&arg))
            },
            text: strings.intern(&desc),
        });
    }
    out
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
    push_entity(
        sink,
        strings,
        class.span,
        id.name.to_string(),
        TypeEntityKind::Class,
    );
    for element in &class.body.body {
        if let ts::ClassElement::MethodDefinition(method) = element {
            // Skip constructors and computed/private keys; a normal method's
            // owner is its enclosing class (port of v5 ts_class_entity).
            if method.kind == ts::MethodDefinitionKind::Constructor {
                continue;
            }
            if let ts::PropertyKey::StaticIdentifier(key) = &method.key {
                push_entity(
                    sink,
                    strings,
                    method.span,
                    key.name.to_string(),
                    TypeEntityKind::Method,
                );
                // A method IS a type: carry its arrow signature (param/ret refs).
                // v5 emits NO method-signature type_edges, so no candidates.
                fn_sigs(
                    method.span,
                    &method.value.type_parameters,
                    &method.value.params,
                    &method.value.return_type,
                    strings,
                    sink,
                    false,
                );
            }
        }
    }
}

fn fn_entity(func: &ts::Function, strings: &mut Strings, sink: &mut FamilyBundle<TypeF>) {    let Some(id) = &func.id else { return };
    push_entity(
        sink,
        strings,
        func.span,
        id.name.to_string(),
        TypeEntityKind::Function,
    );
    fn_sigs(
        func.span,
        &func.type_parameters,
        &func.params,
        &func.return_type,
        strings,
        sink,
        true,
    );
}

/// `const foo = (...) => ...` / `const foo = function (...) {...}` at the top
/// level: the binding name owns a Function entity. Plain value consts (no
/// function initializer) carry no type shape and are skipped (v5: the
/// "don't mint an entity for every const" rule).
fn var_fn_entity(
    var: &ts::VariableDeclaration,
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    for declarator in &var.declarations {
        let ts::BindingPattern::BindingIdentifier(name) = &declarator.id else {
            continue;
        };
        match &declarator.init {
            Some(ts::Expression::ArrowFunctionExpression(arrow)) => {
                push_entity(
                    sink,
                    strings,
                    declarator.span,
                    name.name.to_string(),
                    TypeEntityKind::Function,
                );
                fn_sigs(
                    declarator.span,
                    &arrow.type_parameters,
                    &arrow.params,
                    &arrow.return_type,
                    strings,
                    sink,
                    true,
                );
            }
            Some(ts::Expression::FunctionExpression(func)) => {
                push_entity(
                    sink,
                    strings,
                    declarator.span,
                    name.name.to_string(),
                    TypeEntityKind::Function,
                );
                fn_sigs(
                    declarator.span,
                    &func.type_parameters,
                    &func.params,
                    &func.return_type,
                    strings,
                    sink,
                    true,
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
/// join back to that node at the wire and the resolution seam. When
/// `record_candidates` (the Function-entity call sites — v5 emits no method-
/// signature type_edges), each ref ALSO lands as an unresolved param/returns
/// type-edge candidate (4b-iii); the sigs and the candidates then share ONE
/// refs walk, so they cannot drift.
fn fn_sigs(
    owner: oxc_span::Span,
    type_parameters: &Option<oxc_allocator::Box<ts::TSTypeParameterDeclaration>>,
    params: &ts::FormalParameters,
    return_type: &Option<oxc_allocator::Box<ts::TSTypeAnnotation>>,
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
    record_candidates: bool,
) {
    let exclude = type_param_names(type_parameters);
    for (pos, param) in params.items.iter().enumerate() {
        if let Some(ann) = &param.type_annotation {
            for name in refs_in_type(&ann.type_annotation, &exclude) {
                push_sig(sink, strings, owner, SigSlot::Param, pos as u32, &name);
                if record_candidates {
                    push_candidate(sink, strings, owner, &name, TypeEdgeKind::Param);
                }
            }
        }
    }
    if let Some(rt) = return_type {
        for name in refs_in_type(&rt.type_annotation, &exclude) {
            push_sig(sink, strings, owner, SigSlot::Ret, 0, &name);
            if record_candidates {
                push_candidate(sink, strings, owner, &name, TypeEdgeKind::Returns);
            }
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
        owner: Span {
            start: owner.start,
            len: owner.end - owner.start,
        },
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
    let mut collector = TypeRefCollector {
        exclude,
        out: Vec::new(),
    };
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

// ── type-edge candidates (TypeFAux.candidates; commit 4b-iii) ───────────────
//
// Port of v5 `ts_edges_from` (src/graph/typegraph/ts/mod.rs:103-421) onto the
// candidate row: the same top-level walk (v5 itself runs edges as a second
// pass over the program), the same kind vocabulary, the same `to` text —
// qualified where v5 qualifies (`A.B`), synthetic where v5 synthesizes
// (`Owner::Member` for enum variants). The owner is the entity's SPAN (the
// TypeF node join key) instead of v5's name string. param/returns candidates
// are NOT collected here: they ride `fn_sigs` at the Function-entity call
// sites (one refs walk feeds sigs + candidates; v5 emits no method-signature
// type_edges). Resolve input only — these rows flatten nowhere.

/// The top-level walk (port of v5 `ts_edges_from`'s statement match).
fn edge_candidates(program: &Program<'_>, strings: &mut Strings, sink: &mut FamilyBundle<TypeF>) {
    for stmt in with_module_bodies(&program.body) {
        use ts::Statement as S;
        match stmt {
            S::ExportNamedDeclaration(export) => {
                if let Some(decl) = &export.declaration {
                    decl_edge_candidates(decl, strings, sink);
                }
            }
            S::ExportDefaultDeclaration(export) => match &export.declaration {
                ts::ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                    class_edge_candidates(class, strings, sink)
                }
                ts::ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
                    interface_edge_candidates(interface, strings, sink)
                }
                ts::ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                    fn_edge_candidates(func, strings, sink)
                }
                _ => {}
            },
            S::ClassDeclaration(class) => class_edge_candidates(class, strings, sink),
            S::TSInterfaceDeclaration(interface) => {
                interface_edge_candidates(interface, strings, sink)
            }
            S::TSTypeAliasDeclaration(alias) => alias_edge_candidates(alias, strings, sink),
            S::TSEnumDeclaration(enum_decl) => enum_edge_candidates(enum_decl, strings, sink),
            S::FunctionDeclaration(func) => fn_edge_candidates(func, strings, sink),
            S::VariableDeclaration(var) => var_fn_edge_candidates(var, strings, sink),
            _ => {}
        }
    }
}

fn decl_edge_candidates(
    decl: &ts::Declaration,
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    match decl {
        ts::Declaration::ClassDeclaration(class) => class_edge_candidates(class, strings, sink),
        ts::Declaration::TSInterfaceDeclaration(interface) => {
            interface_edge_candidates(interface, strings, sink)
        }
        ts::Declaration::TSTypeAliasDeclaration(alias) => {
            alias_edge_candidates(alias, strings, sink)
        }
        ts::Declaration::TSEnumDeclaration(enum_decl) => {
            enum_edge_candidates(enum_decl, strings, sink)
        }
        ts::Declaration::FunctionDeclaration(func) => fn_edge_candidates(func, strings, sink),
        ts::Declaration::VariableDeclaration(var) => var_fn_edge_candidates(var, strings, sink),
        _ => {}
    }
}

fn push_candidate(
    sink: &mut FamilyBundle<TypeF>,
    strings: &mut Strings,
    owner: oxc_span::Span,
    to: &str,
    kind: TypeEdgeKind,
) {
    sink.aux.candidates.push(TypeEdgeCandidate {
        owner: to_span(owner),
        to: strings.intern(to),
        kind,
    });
}

/// Every ref under a type subtree becomes one candidate (dedup is resolve's
/// shaping, matching v5's BTreeSet).
fn refs_candidates(
    sink: &mut FamilyBundle<TypeF>,
    strings: &mut Strings,
    owner: oxc_span::Span,
    ty: &ts::TSType,
    exclude: &BTreeSet<String>,
    kind: TypeEdgeKind,
) {
    for name in refs_in_type(ty, exclude) {
        push_candidate(sink, strings, owner, &name, kind);
    }
}

/// Declared type-parameter names + their constraint refs as "generic" edges.
/// Port of v5 `ts_param_edges`; returns the exclusion set (the declared names).
fn param_constraint_candidates(
    owner: oxc_span::Span,
    type_parameters: &Option<oxc_allocator::Box<ts::TSTypeParameterDeclaration>>,
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) -> BTreeSet<String> {
    let params = type_param_names(type_parameters);
    if let Some(tp) = type_parameters {
        for param in &tp.params {
            if let Some(constraint) = &param.constraint {
                refs_candidates(
                    sink,
                    strings,
                    owner,
                    constraint,
                    &params,
                    TypeEdgeKind::Generic,
                );
            }
        }
    }
    params
}

/// The shared body of every function form's non-signature edges: every
/// TSTypeReference inside the body is "uses" (port of v5
/// `ts_fn_signature_edges`'s body half; the generic half rides
/// `param_constraint_candidates`, param/returns ride `fn_sigs`).
fn fn_body_uses(
    owner: oxc_span::Span,
    type_parameters: &Option<oxc_allocator::Box<ts::TSTypeParameterDeclaration>>,
    body: Option<&ts::FunctionBody>,
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let params = param_constraint_candidates(owner, type_parameters, strings, sink);
    if let Some(body) = body {
        let mut collector = TypeRefCollector {
            exclude: &params,
            out: Vec::new(),
        };
        collector.visit_function_body(body);
        for name in collector.out {
            push_candidate(sink, strings, owner, &name, TypeEdgeKind::Uses);
        }
    }
}

/// A named `function foo(...)`. Anonymous functions have no owner, so skip
/// (v5 `ts_function_edges`).
fn fn_edge_candidates(func: &ts::Function, strings: &mut Strings, sink: &mut FamilyBundle<TypeF>) {
    if func.id.is_none() {
        return;
    }
    fn_body_uses(
        func.span,
        &func.type_parameters,
        func.body.as_deref(),
        strings,
        sink,
    );
}

/// `const foo = (...) => ...` / `const foo = function (...) {...}` at the top
/// level: the binding name owns the function's edges (v5 `ts_var_fn_edges`).
/// Plain value consts carry no type shape and are skipped.
fn var_fn_edge_candidates(
    var: &ts::VariableDeclaration,
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    for declarator in &var.declarations {
        let ts::BindingPattern::BindingIdentifier(_) = &declarator.id else {
            continue;
        };
        match &declarator.init {
            Some(ts::Expression::ArrowFunctionExpression(arrow)) => {
                fn_body_uses(
                    declarator.span,
                    &arrow.type_parameters,
                    Some(&arrow.body),
                    strings,
                    sink,
                );
            }
            Some(ts::Expression::FunctionExpression(func)) => {
                fn_body_uses(
                    declarator.span,
                    &func.type_parameters,
                    func.body.as_deref(),
                    strings,
                    sink,
                );
            }
            _ => {}
        }
    }
}

/// Port of v5 `ts_class_edges`: heritage is "impl" (super class + super type
/// args + implements + implements type args), property / accessor / ctor
/// param-property type refs are "field", type-param constraint refs are
/// "generic". Method SIGNATURES mint no type_edges in v5 (nothing here).
fn class_edge_candidates(class: &ts::Class, strings: &mut Strings, sink: &mut FamilyBundle<TypeF>) {
    let Some(_) = &class.id else { return };
    let owner = class.span;
    let params = param_constraint_candidates(owner, &class.type_parameters, strings, sink);
    if let Some(sup) = &class.super_class {
        if let ts::Expression::Identifier(idr) = sup {
            push_candidate(sink, strings, owner, &idr.name, TypeEdgeKind::Impl);
        }
    }
    if let Some(args) = &class.super_type_arguments {
        for ty in &args.params {
            refs_candidates(sink, strings, owner, ty, &params, TypeEdgeKind::Impl);
        }
    }
    for imp in &class.implements {
        if let Some(to) = ts_type_name(&imp.expression) {
            push_candidate(sink, strings, owner, &to, TypeEdgeKind::Impl);
        }
        if let Some(args) = &imp.type_arguments {
            for ty in &args.params {
                refs_candidates(sink, strings, owner, ty, &params, TypeEdgeKind::Impl);
            }
        }
    }
    for element in &class.body.body {
        match element {
            ts::ClassElement::PropertyDefinition(prop) => {
                if let Some(ann) = &prop.type_annotation {
                    refs_candidates(
                        sink,
                        strings,
                        owner,
                        &ann.type_annotation,
                        &params,
                        TypeEdgeKind::Field,
                    );
                }
            }
            ts::ClassElement::AccessorProperty(prop) => {
                if let Some(ann) = &prop.type_annotation {
                    refs_candidates(
                        sink,
                        strings,
                        owner,
                        &ann.type_annotation,
                        &params,
                        TypeEdgeKind::Field,
                    );
                }
            }
            // Constructor parameter properties (`constructor(private db: Db)`)
            // declare fields; plain constructor args are not part of the shape.
            ts::ClassElement::MethodDefinition(method) => {
                if method.kind != ts::MethodDefinitionKind::Constructor {
                    continue;
                }
                for fp in &method.value.params.items {
                    if fp.accessibility.is_none() && !fp.readonly {
                        continue;
                    }
                    if let Some(ann) = &fp.type_annotation {
                        refs_candidates(
                            sink,
                            strings,
                            owner,
                            &ann.type_annotation,
                            &params,
                            TypeEdgeKind::Field,
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

/// Port of v5 `ts_interface_edges`: extends is "generic" (identifier + type
/// args), property signatures are "field", constraint refs are "generic".
fn interface_edge_candidates(
    interface: &ts::TSInterfaceDeclaration,
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let owner = interface.span;
    let params = param_constraint_candidates(owner, &interface.type_parameters, strings, sink);
    for ext in &interface.extends {
        if let ts::Expression::Identifier(idr) = &ext.expression {
            push_candidate(sink, strings, owner, &idr.name, TypeEdgeKind::Generic);
        }
        if let Some(args) = &ext.type_arguments {
            for ty in &args.params {
                refs_candidates(sink, strings, owner, ty, &params, TypeEdgeKind::Generic);
            }
        }
    }
    for member in &interface.body.body {
        if let ts::TSSignature::TSPropertySignature(prop) = member {
            if let Some(ann) = &prop.type_annotation {
                refs_candidates(
                    sink,
                    strings,
                    owner,
                    &ann.type_annotation,
                    &params,
                    TypeEdgeKind::Field,
                );
            }
        }
    }
}

/// Port of v5 `ts_alias_edges`: a union alias is a sum type — alternatives
/// that are plain refs are "variant" (their type args stay "field"); anything
/// else is shape ("field"). Non-union: all refs are "field".
fn alias_edge_candidates(
    alias: &ts::TSTypeAliasDeclaration,
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let owner = alias.span;
    let params = param_constraint_candidates(owner, &alias.type_parameters, strings, sink);
    if let ts::TSType::TSUnionType(union) = &alias.type_annotation {
        for member in &union.types {
            if let ts::TSType::TSTypeReference(reference) = member {
                if let Some(to) = ts_type_name(&reference.type_name) {
                    if !params.contains(&to) {
                        push_candidate(sink, strings, owner, &to, TypeEdgeKind::Variant);
                    }
                }
                if let Some(args) = &reference.type_arguments {
                    for ty in &args.params {
                        refs_candidates(sink, strings, owner, ty, &params, TypeEdgeKind::Field);
                    }
                }
            } else {
                refs_candidates(sink, strings, owner, member, &params, TypeEdgeKind::Field);
            }
        }
        return;
    }
    refs_candidates(
        sink,
        strings,
        owner,
        &alias.type_annotation,
        &params,
        TypeEdgeKind::Field,
    );
}

/// Port of v5 `ts_enum_edges`: every member is a "variant" whose `to` is the
/// synthetic `Owner::Member` text — exactly as v5 synthesizes it (Identifier
/// and String member names only; computed names are skipped).
fn enum_edge_candidates(
    enum_decl: &ts::TSEnumDeclaration,
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let owner = enum_decl.span;
    let owner_name = enum_decl.id.name.to_string();
    for member in &enum_decl.body.members {
        let name = match &member.id {
            ts::TSEnumMemberName::Identifier(id) => id.name.to_string(),
            ts::TSEnumMemberName::String(s) => s.value.to_string(),
            _ => continue,
        };
        push_candidate(
            sink,
            strings,
            owner,
            &format!("{owner_name}::{name}"),
            TypeEdgeKind::Variant,
        );
    }
}

// ════════════════════════════════════════════════════════════════════════════
// CallF: callable definitions (nodes) + call sites (aux). Commit 3a.
//
// Ports v5 `ts_call_defs_from` (defs) + `TsCallSites` (sites) + `TsNestedFnDefs`
// (nested named-fn defs) + `ts_push_lambda_defs` (lambda defs). v5's
// `mint_sym`/`line_at`/`starts` are deleted: a def is span + kind + name (the
// name is the bare identifier for callee resolution, NOT a qualified sym). The
// def span MATCHES the TypeF entity span (func.span / method.span /
// declarator.span) so the two facets join on the same coordinate. Lambda defs
// (anonymous callables) are kind=Lambda name=None over the df-covered scopes —
// the exact set v5 derives from the df closure nodes; the DfF `closure` node
// stays (it marks the closure VALUE). (3a deferred these to DfF; the TS port
// now emits them like the rust/go ports do — cross-lang consistency + oracle
// parity, user ruling 2026-07-24.)
// ════════════════════════════════════════════════════════════════════════════

/// The CallF projector: emits one def node per callable (Free / Method /
/// Lambda) and one site per call expression. Sites are unresolved in phase 1
/// (the callee as written); `Resolve<CallF>` binds them at commit 4.
pub struct CallProjector<'a> {
    /// The source text, needed for the `unresolved` detail slice (the computed
    /// expression's exact source at its span). Same shape as `DfProjector`.
    pub content: &'a str,
}

impl Project<CallF> for CallProjector<'_> {
    type Parsed<'a> = Program<'a>;

    fn project(
        &self,
        program: &Program<'_>,
        strings: &mut Strings,
        sink: &mut FamilyBundle<CallF>,
    ) {
        // Top-level defs (depth 0): free functions, class methods, var-bound fns.
        call_defs(program, strings, sink);
        // One walk for the rest: nested named-fn defs (depth > 0) + every call
        // site. The walker owns its output (no &mut strings/sink inside the
        // visitor), drained here so the two &mut params never alias through self.
        let mut walker = CallWalker {
            content: self.content,
            depth: 0,
            nested_defs: Vec::new(),
            sites: Vec::new(),
            value_refs: Vec::new(),
        };
        walker.visit_program(program);
        for (span, name) in walker.nested_defs {
            push_def(sink, strings, span, name, CallKind::Free);
        }
        for site in walker.sites {
            sink.aux.sites.push(CallSite {
                span: to_span(site.span),
                callee: strings.intern(&site.callee),
                callee_path: site.path.as_deref().map(|path| strings.intern(path)),
            });
        }
        // Lambda defs: one per inline arrow / fn-expr the DfF lift reaches
        // (v5 derives this exact set from the df closure nodes —
        // `ts_push_lambda_defs`). name=None (v5's empty name); span = the
        // arrow/fn-expr's own span (body-covering, like rust's `def_span`) so
        // a site inside the body binds to this lambda by containment.
        let mut lambdas = LambdaDefs { out: Vec::new() };
        for stmt in with_module_bodies(&program.body) {
            lambda_entry_stmt(&mut lambdas, stmt);
        }
        for span in lambdas.out {
            sink.nodes.push(Node::new(to_span(span), CallKind::Lambda));
        }
        // Conditional because v5 has no module-def facet: an unconditional row
        // is a v6-only line in the PORTED `call_def` set (tests/golden_parity.rs).
        if module_scope_owns_a_call(sink) {
            push_def(sink, strings, program.span, MODULE_DEF_NAME, MODULE);
        }
        // Module specifiers (4b-ii): import/export-from rows, as written, into
        // the CallF aux (the 4a ADDENDUM home). Same one parse, same top level.
        module_specifiers(program, strings, sink);
        // Runtime-computed edge markers: same one parse, same top level; the
        // walker needs the source text for the detail slice.
        let mut unresolved = UnresolvedWalker {
            content: self.content,
            out: Vec::new(),
        };
        unresolved.visit_program(program);
        for (span, reason, detail) in unresolved.out {
            sink.aux.unresolved.push(Unresolved {
                span: to_span(span),
                reason,
                detail: strings.intern(&detail),
            });
        }
        push_value_refs(sink, strings, walker.value_refs);
        // Receiver typing for member calls: one scope-threaded pass per
        // function body. The rows live in the per-blob facts store (never in
        // the wired aux, so the phase-1 wire stays byte-identical); the
        // resolve leg joins them by `CallSite.span`.
        ts_receivers::store_facts(
            content_id_of(self.content.as_bytes()),
            ts_receivers::collect(program),
        );
    }
}

/// The `position=value` rows. Restricted to names THIS file declares or
/// imports: any other bare argument names a local and answers no def.
fn push_value_refs(
    sink: &mut FamilyBundle<CallF>,
    strings: &mut Strings,
    candidates: Vec<(oxc_span::Span, String)>,
) {
    let declared: BTreeSet<String> = sink
        .nodes
        .iter()
        .filter_map(|node| node.name)
        .chain(sink.aux.specifiers.iter().map(|specifier| specifier.name))
        .map(|name| strings.lookup(name).to_string())
        .collect();
    for (span, name) in candidates {
        if !declared.contains(&name) {
            continue;
        }
        sink.aux.refs.push(Reference {
            span: to_span(span),
            functor: strings.intern(&name),
            position: RefPosition::Value,
        });
    }
}

// ── module specifiers (CallFAux.specifiers; commit 4b-ii) ───────────────────
//
// Port of v5's TS `module_binding` local-name semantics (src/graph/modgraph/
// ts.rs `parse_ts_module_bindings`) onto the 4a Specifier row: `name` is the
// BOUND local name as written (v5's local_name column; the module path for the
// path-only forms, per the row doc); `kind` is the seed's BindingKind
// vocabulary. THE FROM-MODULE GAP (the 4a ADDENDUM's open sub-question, flagged
// for human review): the row carries NO source module and NO imported name
// (v5's source_module/imported_name columns) — nothing consumes specifiers yet
// (Resolve<CallF> lands at 4c), so the seed's fuller Binding side table
// (local/source/imported, `_1_mask.rs`:67-76) stays the evolution path and no
// field is added silently. Covered: ES static imports (named / default /
// namespace / side-effect; type imports included — v5's string-level parse tags
// them identically) and export-FROM re-exports (`export {a} from`,
// `export * from`, `export * as ns from`). NOT covered (no row, matches v5's
// binding table): `export {a}` without a source (a local export marker, no
// module specifier). The runtime forms (`import()`, `require`) DO carry a row;
// a non-literal path has none to record and stays in `CallFAux.unresolved`.
fn module_specifiers(program: &Program<'_>, strings: &mut Strings, sink: &mut FamilyBundle<CallF>) {
    for row in scan_module_specifiers(program) {
        sink.aux.specifiers.push(Specifier {
            span: to_span(row.span),
            name: strings.intern(row.name),
            kind: row.kind,
            module: Some(strings.intern(row.module)),
            imported: row.imported.map(|text| strings.intern(text)),
        });
    }
}

/// One module specifier off the oxc walk, before interning. `span` is the seat
/// the `Specifier` row keeps; `module_span` is the literal a rewrite edits.
struct ScannedSpecifier<'a> {
    span: oxc_span::Span,
    name: &'a str,
    kind: SpecifierKind,
    module: &'a str,
    module_span: oxc_span::Span,
    imported: Option<&'a str>,
}

/// Every module specifier one program writes, in source order. The sort is
/// stable over already-ascending static rows, so their order does not move.
fn scan_module_specifiers<'a>(program: &Program<'a>) -> Vec<ScannedSpecifier<'a>> {
    let mut rows = Vec::new();
    for stmt in with_module_bodies(&program.body) {
        match stmt {
            ts::Statement::ImportDeclaration(import) => {
                let module = import.source.value.as_str();
                let module_span = import.source.span;
                match &import.specifiers {
                    // `import './m'`: path-only form — name = the module path.
                    None => rows.push(ScannedSpecifier {
                        span: module_span,
                        name: module,
                        kind: SpecifierKind::SideEffect,
                        module,
                        module_span,
                        imported: None,
                    }),
                    Some(specs) => {
                        for spec in specs {
                            let (span, name, kind, imported) = match spec {
                                ts::ImportDeclarationSpecifier::ImportSpecifier(named) => {
                                    let local = named.local.name.as_str();
                                    (
                                        named.span,
                                        local,
                                        SpecifierKind::Named,
                                        renamed(module_export_name(&named.imported), local),
                                    )
                                }
                                ts::ImportDeclarationSpecifier::ImportDefaultSpecifier(default) => {
                                    (
                                        default.span,
                                        default.local.name.as_str(),
                                        SpecifierKind::Default,
                                        Some("default"),
                                    )
                                }
                                ts::ImportDeclarationSpecifier::ImportNamespaceSpecifier(ns) => (
                                    ns.span,
                                    ns.local.name.as_str(),
                                    SpecifierKind::Namespace,
                                    None,
                                ),
                            };
                            rows.push(ScannedSpecifier {
                                span,
                                name,
                                kind,
                                module,
                                module_span,
                                imported,
                            });
                        }
                    }
                }
            }
            ts::Statement::ExportNamedDeclaration(export) => {
                // `export {a} from './m'` only; `export {a}` (no source) is a
                // local export marker, not a module specifier.
                if let Some(source) = &export.source {
                    for spec in &export.specifiers {
                        let name = module_export_name(&spec.exported);
                        rows.push(ScannedSpecifier {
                            span: spec.span,
                            name,
                            kind: SpecifierKind::Reexport,
                            module: source.value.as_str(),
                            module_span: source.span,
                            imported: renamed(module_export_name(&spec.local), name),
                        });
                    }
                }
            }
            ts::Statement::ExportAllDeclaration(export) => {
                let module = export.source.value.as_str();
                let module_span = export.source.span;
                // `export * as ns from './m'` binds the alias; `export * from
                // './m'` is a path-only form — name = the module path.
                let (span, name) = match &export.exported {
                    Some(exported) => (exported.span(), module_export_name(exported)),
                    None => (module_span, module),
                };
                rows.push(ScannedSpecifier {
                    span,
                    name,
                    kind: SpecifierKind::Reexport,
                    module,
                    module_span,
                    imported: None,
                });
            }
            _ => {}
        }
    }
    let mut runtime = RuntimeModuleWalker { out: Vec::new() };
    runtime.visit_program(program);
    rows.extend(runtime.out);
    rows.sort_by_key(|row| row.span.start);
    rows
}

/// The runtime module references anywhere in the tree. Only a string-literal
/// path becomes a row; a computed one has no path to record.
struct RuntimeModuleWalker<'a> {
    out: Vec<ScannedSpecifier<'a>>,
}

impl<'a> OxcVisit<'a> for RuntimeModuleWalker<'a> {
    fn visit_import_expression(&mut self, it: &ts::ImportExpression<'a>) {
        if let ts::Expression::StringLiteral(lit) = &it.source {
            let module = lit.value.as_str();
            self.out.push(ScannedSpecifier {
                span: lit.span,
                name: module,
                kind: SpecifierKind::DynamicImport,
                module,
                module_span: lit.span,
                imported: None,
            });
        }
        oxc_ast_visit::walk::walk_import_expression(self, it);
    }

    fn visit_call_expression(&mut self, it: &ts::CallExpression<'a>) {
        if let ts::Expression::Identifier(callee) = &it.callee {
            if callee.name == "require" {
                if let Some(ts::Expression::StringLiteral(lit)) =
                    it.arguments.first().and_then(|arg| arg.as_expression())
                {
                    let module = lit.value.as_str();
                    self.out.push(ScannedSpecifier {
                        span: lit.span,
                        name: module,
                        kind: SpecifierKind::Require,
                        module,
                        module_span: lit.span,
                        imported: None,
                    });
                }
            }
        }
        oxc_ast_visit::walk::walk_call_expression(self, it);
    }

    fn visit_ts_import_equals_declaration(&mut self, it: &ts::TSImportEqualsDeclaration<'a>) {
        if let ts::TSModuleReference::ExternalModuleReference(reference) = &it.module_reference {
            let module = reference.expression.value.as_str();
            self.out.push(ScannedSpecifier {
                span: it.id.span,
                name: it.id.name.as_str(),
                kind: SpecifierKind::Require,
                module,
                module_span: reference.expression.span,
                imported: None,
            });
        }
        oxc_ast_visit::walk::walk_ts_import_equals_declaration(self, it);
    }
}

/// The source module's name for a binding, kept only when it differs from the
/// local one: `import {a}` states nothing a second column can add.
fn renamed<'a>(imported: &'a str, local: &str) -> Option<&'a str> {
    (imported != local).then_some(imported)
}

/// A `ModuleExportName`'s text as written (identifier or string-literal name).
fn module_export_name<'a>(name: &ts::ModuleExportName<'a>) -> &'a str {
    match name {
        ts::ModuleExportName::IdentifierName(id) => id.name.as_str(),
        ts::ModuleExportName::IdentifierReference(id) => id.name.as_str(),
        ts::ModuleExportName::StringLiteral(s) => s.value.as_str(),
    }
}

// ── the move's per-file specifier rows (arc 2) ──────────────────────────────

/// A module specifier as written. `module_span` covers the whole string literal
/// including quotes; `module` is the parsed path, so a re-aim re-quotes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TsSpecifier {
    pub module_span: Span,
    pub module: String,
    pub kind: SpecifierKind,
    /// The quote character the literal was written with, preserved by a re-aim.
    pub quote: char,
}

/// Every module specifier one TS/JS file writes, in source order, off the one
/// oxc parse: the move's rewrite targets, with no second front end for TS.
pub fn ts_specifiers(path: &str, content: &str) -> Result<Vec<TsSpecifier>, ParseError> {
    let parser = OxcParser;
    let arena = parser.make_arena();
    let program = parser.parse(&arena, path, content.as_bytes())?;
    Ok(scan_module_specifiers(&program)
        .into_iter()
        .map(|row| TsSpecifier {
            module_span: to_span(row.module_span),
            module: row.module.to_string(),
            kind: row.kind,
            quote: quote_at(content, row.module_span),
        })
        .collect())
}

/// A literal's opening quote. Anything else at the span start means the caller
/// paired a parse with different text; `"` keeps the replacement well formed.
fn quote_at(content: &str, span: oxc_span::Span) -> char {
    match content.as_bytes().get(span.start as usize) {
        Some(b'\'') => '\'',
        Some(b'`') => '`',
        _ => '"',
    }
}

fn call_defs(program: &Program<'_>, strings: &mut Strings, sink: &mut FamilyBundle<CallF>) {
    for stmt in with_module_bodies(&program.body) {
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
            S::TSInterfaceDeclaration(interface) => {
                interface_member_defs(interface, strings, sink)
            }
            _ => {}
        }
    }
}

/// An interface's method signatures are callable defs: a member call on an
/// interface-typed receiver binds the SIGNATURE (the oracle's coordinate),
/// never an implementer. Class methods carry the same shape at their own
/// definition spans; a signature has no body, so no other def exists there.
fn interface_member_defs(
    interface: &ts::TSInterfaceDeclaration,
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    for member in &interface.body.body {
        if let ts::TSSignature::TSMethodSignature(method) = member {
            if let ts::PropertyKey::StaticIdentifier(key) = &method.key {
                push_def(
                    sink,
                    strings,
                    method.span,
                    key.name.to_string(),
                    CallKind::Method,
                );
            }
        }
    }
}

fn call_decl_def(decl: &ts::Declaration, strings: &mut Strings, sink: &mut FamilyBundle<CallF>) {
    match decl {
        ts::Declaration::ClassDeclaration(class) => class_call_defs(class, strings, sink),
        ts::Declaration::FunctionDeclaration(func) => fn_call_def(func, strings, sink),
        ts::Declaration::VariableDeclaration(var) => var_call_defs(var, strings, sink),
        ts::Declaration::TSInterfaceDeclaration(interface) => {
            interface_member_defs(interface, strings, sink)
        }
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
    push_def(
        sink,
        strings,
        func.span,
        id.name.to_string(),
        CallKind::Free,
    );
}

/// One def per class method. The CONSTRUCTOR's call-name is the CLASS name so a
/// `new Foo()` site resolves to it (v5 `ts_class_call_defs`); its kind is Method.
/// Abstract methods (no body) are skipped. Port of v5 `ts_class_call_defs`.
fn class_call_defs(class: &ts::Class, strings: &mut Strings, sink: &mut FamilyBundle<CallF>) {
    let Some(id) = &class.id else { return };
    let owner = id.name.to_string();
    for element in &class.body.body {
        let ts::ClassElement::MethodDefinition(method) = element else {
            continue;
        };
        let ts::PropertyKey::StaticIdentifier(key) = &method.key else {
            continue;
        };
        if method.value.body.is_none() {
            continue;
        }
        let is_ctor = method.kind == ts::MethodDefinitionKind::Constructor;
        let name = if is_ctor {
            owner.clone()
        } else {
            key.name.to_string()
        };
        push_def(sink, strings, method.span, name, CallKind::Method);
    }
}

/// `const foo = (...) => ...` / `const foo = function () {}`: a Free def owned
/// by the binding name. Bodiless function expressions are skipped. Port of v5
/// `ts_var_call_defs`.
fn var_call_defs(
    var: &ts::VariableDeclaration,
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    for declarator in &var.declarations {
        let ts::BindingPattern::BindingIdentifier(name) = &declarator.id else {
            continue;
        };
        let has_body = match &declarator.init {
            Some(ts::Expression::ArrowFunctionExpression(_)) => true,
            Some(ts::Expression::FunctionExpression(func)) => func.body.is_some(),
            _ => false,
        };
        if !has_body {
            continue;
        }
        push_def(
            sink,
            strings,
            declarator.span,
            name.name.to_string(),
            CallKind::Free,
        );
    }
}

/// Lambda defs (anonymous callables): one per inline arrow / function
/// expression the DfF lift reaches. v5 derives this set from the df `closure`
/// nodes (`ts_push_lambda_defs`), so the driver restricts the walk to the same
/// top-level scopes `ts_flow_stmt` covers: fn decl bodies (exported or not),
/// class method bodies, and top-level non-exported var/expr/return statements.
/// Exported var inits, class field inits, and export-default declarations are
/// NOT df-covered (v5 mints no closure nodes there) and mint no lambda defs.
/// Port of v5's emission SET, not its sym machinery (v6 needs no lam_sym).
fn lambda_entry_stmt(walker: &mut LambdaDefs, stmt: &ts::Statement) {
    use ts::Statement as S;
    match stmt {
        S::FunctionDeclaration(func) => {
            if let Some(body) = func.body.as_deref() {
                walker.visit_function_body(body);
            }
        }
        S::ExportNamedDeclaration(export) => {
            if let Some(decl) = &export.declaration {
                lambda_entry_decl(walker, decl);
            }
        }
        S::ClassDeclaration(class) => lambda_entry_class(walker, class),
        S::VariableDeclaration(_) | S::ExpressionStatement(_) | S::ReturnStatement(_) => {
            walker.visit_statement(stmt);
        }
        _ => {}
    }
}

fn lambda_entry_decl(walker: &mut LambdaDefs, decl: &ts::Declaration) {
    match decl {
        ts::Declaration::FunctionDeclaration(func) => {
            if let Some(body) = func.body.as_deref() {
                walker.visit_function_body(body);
            }
        }
        ts::Declaration::ClassDeclaration(class) => lambda_entry_class(walker, class),
        _ => {}
    }
}

/// Mirrors `df_flow_class`: each method body (ctor/instance/static/get/set) is
/// a covered scope; field initializers are not.
fn lambda_entry_class(walker: &mut LambdaDefs, class: &ts::Class) {
    for element in &class.body.body {
        let ts::ClassElement::MethodDefinition(method) = element else {
            continue;
        };
        let Some(body) = method.value.body.as_deref() else {
            continue;
        };
        walker.visit_function_body(body);
    }
}

/// Collects a Lambda def span per inline arrow / fn-expr. Every arrow / fn-expr
/// in a covered scope mints a def EXCEPT the direct init of an identifier-bound
/// declarator — a named callable (a Free def via `var_call_defs`; the df lift
/// keys it by the binding name and mints no closure node) — whose BODY is still
/// walked for nested inline lambdas. Param/destructuring defaults are not
/// df-covered in v5 (the lift seeds patterns only) and are skipped. (Accepted
/// gap, same class as the rust port's visitor discipline: statement kinds the
/// DfF walker does not recurse into — try/switch/throw — ARE walked here, so
/// an inline lambda nested only under those mints a def v5 lacks.)
struct LambdaDefs {
    out: Vec<oxc_span::Span>,
}

impl<'a> OxcVisit<'a> for LambdaDefs {
    fn visit_arrow_function_expression(&mut self, arrow: &ts::ArrowFunctionExpression<'a>) {
        self.out.push(arrow.span);
        oxc_ast_visit::walk::walk_arrow_function_expression(self, arrow);
    }
    fn visit_function(&mut self, func: &ts::Function<'a>, flags: oxc_syntax::scope::ScopeFlags) {
        // A function EXPRESSION is an anonymous callable -> a Lambda def
        // (bodiless ones lift no body and mint no closure node in v5). Fn
        // declarations (CallWalker's nested Free defs) and method values
        // (Method defs) are not lambdas.
        if func.r#type == ts::FunctionType::FunctionExpression && func.body.is_some() {
            self.out.push(func.span);
        }
        oxc_ast_visit::walk::walk_function(self, func, flags);
    }
    fn visit_variable_declarator(&mut self, declarator: &ts::VariableDeclarator<'a>) {
        // `const f = (...) => ...` / `const f = function () {}`: the bound
        // value is a named callable, not a lambda — skip its def, walk its
        // body for nested inline lambdas.
        if matches!(&declarator.id, ts::BindingPattern::BindingIdentifier(_)) {
            match &declarator.init {
                Some(ts::Expression::ArrowFunctionExpression(arrow)) => {
                    self.visit_function_body(&arrow.body);
                    return;
                }
                Some(ts::Expression::FunctionExpression(func)) => {
                    if let Some(body) = func.body.as_deref() {
                        self.visit_function_body(body);
                    }
                    return;
                }
                _ => {}
            }
        }
        oxc_ast_visit::walk::walk_variable_declarator(self, declarator);
    }
    fn visit_assignment_pattern(&mut self, _pattern: &ts::AssignmentPattern<'a>) {
        // Param/destructuring defaults hold no df-covered values in v5.
    }
}

/// Whether any call site sits outside every def in `sink`. Prefix-max of def
/// end, not `covering_def` per site: that re-sorts the def spans per site.
fn module_scope_owns_a_call(sink: &FamilyBundle<CallF>) -> bool {
    let mut reach: Vec<(u32, u32)> = sink
        .nodes
        .iter()
        .map(|node| (node.span.start, node.span.end()))
        .collect();
    reach.sort_unstable();
    let mut furthest = 0u32;
    for entry in reach.iter_mut() {
        furthest = furthest.max(entry.1);
        entry.1 = furthest;
    }
    sink.aux.sites.iter().any(|site| {
        let cut = reach.partition_point(|(start, _)| *start <= site.span.start);
        cut == 0 || reach[cut - 1].1 < site.span.end()
    })
}

fn push_def(
    sink: &mut FamilyBundle<CallF>,
    strings: &mut Strings,
    span: oxc_span::Span,
    name: impl AsRef<str>,
    kind: CallKind,
) {
    sink.nodes
        .push(Node::new(to_span(span), kind).with_name(strings.intern(name.as_ref())));
}

/// One collected call site. `path` is the callee as written when it is a
/// member expression, the receiver seat `Resolve<CallF>` reads.
struct CollectedSite {
    span: oxc_span::Span,
    callee: String,
    path: Option<String>,
}

/// Walks the whole program for (a) nested named function DECLARATIONS at
/// `depth > 0` (top-level ones are `call_defs`' job) and (b) every call site
/// (`foo()`, `new Foo()`, `<Card/>`). Collects into owned vecs; the projector
/// drains + interns after the walk. Port of v5 `TsNestedFnDefs` + `TsCallSites`.
struct CallWalker<'c> {
    content: &'c str,
    depth: u32,
    nested_defs: Vec<(oxc_span::Span, String)>,
    sites: Vec<CollectedSite>,
    /// Every identifier in call-argument position, before the projector keeps
    /// the ones this file declares or imports.
    value_refs: Vec<(oxc_span::Span, String)>,
}

impl<'a> OxcVisit<'a> for CallWalker<'_> {
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
            self.sites.push(CollectedSite {
                span: call.callee.span(),
                callee,
                path: callee_path(&call.callee, self.content),
            });
        }
        collect_value_refs(&mut self.value_refs, &call.arguments);
        oxc_ast_visit::walk::walk_call_expression(self, call);
    }

    fn visit_new_expression(&mut self, new_expr: &ts::NewExpression<'a>) {
        // `new Foo(x)`: the callee is the constructed name; resolves to the
        // class's ctor call_def (whose name is the class name).
        if let Some(callee) = callee_name(&new_expr.callee) {
            self.sites.push(CollectedSite {
                span: new_expr.span,
                callee,
                path: callee_path(&new_expr.callee, self.content),
            });
        }
        collect_value_refs(&mut self.value_refs, &new_expr.arguments);
        oxc_ast_visit::walk::walk_new_expression(self, new_expr);
    }

    fn visit_jsx_element(&mut self, element: &ts::JSXElement<'a>) {
        // `<Card/>` is a call (jsx(Card, props)); host elements (`<div/>`,
        // lowercase Identifier) have no def to resolve to and are skipped.
        use ts::JSXElementName as N;
        let (callee, path) = match &element.opening_element.name {
            N::IdentifierReference(reference) => (Some(reference.name.to_string()), None),
            N::MemberExpression(member) => (
                Some(member.property.name.to_string()),
                slice_at(self.content, member.span),
            ),
            _ => (None, None),
        };
        if let Some(callee) = callee {
            self.sites.push(CollectedSite {
                span: element.opening_element.span,
                callee,
                path,
            });
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

/// Every bare identifier among a call's arguments. `foo(bar)` names `bar`
/// without calling it, which mints no site and so reaches no resolve arm.
fn collect_value_refs(
    out: &mut Vec<(oxc_span::Span, String)>,
    arguments: &oxc_allocator::Vec<'_, ts::Argument<'_>>,
) {
    for argument in arguments {
        if let ts::Argument::Identifier(id) = argument {
            out.push((id.span, id.name.to_string()));
        }
    }
}

/// The callee AS WRITTEN when it is a member expression (`out.push`), the
/// `CallSite.callee_path` seat. A bare identifier is its own segment: None.
fn callee_path(expr: &ts::Expression, content: &str) -> Option<String> {
    match expr {
        ts::Expression::StaticMemberExpression(member) => slice_at(content, member.span),
        _ => None,
    }
}

/// The source text at `span`, None when the span is not a char boundary of
/// `content` (a lone surrogate half in the file, never in valid TS).
fn slice_at(content: &str, span: oxc_span::Span) -> Option<String> {
    content
        .get(span.start as usize..span.end as usize)
        .map(str::to_string)
}

// ── unresolved (CallFAux.unresolved; port of v5 `TsUnresolvedWalker`) ────────

/// Collects `(span, reason, detail)`; the projector interns and drains after
/// the walk (no `&mut strings` inside the visitor).
struct UnresolvedWalker<'s> {
    content: &'s str,
    out: Vec<(oxc_span::Span, UnresolvedReason, String)>,
}

impl UnresolvedWalker<'_> {
    fn slice(&self, span: oxc_span::Span) -> String {
        self.content
            .get(span.start as usize..span.end as usize)
            .unwrap_or_default()
            .to_string()
    }
}

impl<'a> OxcVisit<'a> for UnresolvedWalker<'_> {
    fn visit_import_expression(&mut self, it: &ts::ImportExpression<'a>) {
        if !matches!(it.source, ts::Expression::StringLiteral(_)) {
            self.out.push((
                it.source.span(),
                UnresolvedReason::DynamicImport,
                self.slice(it.source.span()),
            ));
        }
        oxc_ast_visit::walk::walk_import_expression(self, it);
    }

    fn visit_call_expression(&mut self, it: &ts::CallExpression<'a>) {
        if let ts::Expression::Identifier(callee) = &it.callee {
            if callee.name == "require" {
                if let Some(arg) = it.arguments.first().and_then(|a| a.as_expression()) {
                    if !matches!(arg, ts::Expression::StringLiteral(_)) {
                        self.out.push((
                            arg.span(),
                            UnresolvedReason::DynamicImport,
                            self.slice(arg.span()),
                        ));
                    }
                }
            }
        }
        if let ts::Expression::ComputedMemberExpression(m) = &it.callee {
            self.out.push((
                m.span,
                UnresolvedReason::ComputedMemberCall,
                self.slice(m.span),
            ));
        }
        for arg in &it.arguments {
            if let ts::Argument::SpreadElement(sp) = arg {
                self.out.push((
                    sp.span,
                    UnresolvedReason::SpreadCallArgs,
                    self.slice(sp.span),
                ));
            }
        }
        oxc_ast_visit::walk::walk_call_expression(self, it);
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
//  - `fn_sym` ON NODES: the enclosing callable is not stored on every df node;
//    it is threaded through the walk (v5's own mechanism) purely so the
//    `closure` VALUE node can carry v5's exact `lam_sym` name
//    (`{file}::function::{fn}::closure::{byte}`, nesting chains; `<top>` for
//    module level, `{file}::method::{Owner}.{m}` for methods, the binding name
//    for a const-bound arrow). No sym store: the name derives from the AST
//    walk's containment path + the closure's span start. The transient scope
//    HashMap (var name -> NodeRef) for intra-procedural resolution is kept.
//  - `line_at` / `line_index` / `line_col`: a node is a byte Span, never a line.
//  - the enrichment aux: `args` (positional slots), `fields` (object/array
//    field names), `lits` (literal texts), `param_pos`. Args
//    and parameter positions are emitted as DfArg/DfParam rows. The
//    EDGES already carry every value flow; the aux only labels slots/names/texts
//    for the later interprocedural (arg->param) + string-flow queries.
//  - JSX element/fragment flow (tsx-specific; the catch-all covers it for now).
// ════════════════════════════════════════════════════════════════════════════

/// The DfF projector: lifts each callable's body to its value-flow graph; the
/// `closure` node's NAME is v5's `lam_sym`, and `content` resolves raw-source
/// `df_lit` rows at the end of the walk.
pub struct DfProjector<'a> {
    pub file: &'a str,
    pub content: &'a str,
}

impl Project<DfF> for DfProjector<'_> {
    type Parsed<'a> = Program<'a>;

    fn project(&self, program: &Program<'_>, strings: &mut Strings, sink: &mut FamilyBundle<DfF>) {
        for stmt in with_module_bodies(&program.body) {
            df_flow_stmt(stmt, self.file, strings, sink);
        }
        // Resolve the pending template/concat spans into raw source-slice text.
        for (node, start, end, kind) in sink.aux.lit_spans.drain(..) {
            let text = self
                .content
                .get(start as usize..end as usize)
                .unwrap_or_default()
                .to_string();
            sink.aux.lits.push(DfLit { node, kind, text });
        }
        for (index, start, end) in std::mem::take(&mut sink.aux.loop_collection_spans) {
            sink.aux.loops[index].collection = self
                .content
                .get(start as usize..end as usize)
                .map(str::to_string);
        }
        sink.aux.nests = crate::types::compute_nests(&sink.nodes, &sink.aux.loops);
    }
}

type Scope = std::collections::HashMap<String, NodeRef>;

fn df_flow_stmt(
    stmt: &ts::Statement,
    file: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    use ts::Statement as S;
    match stmt {
        S::FunctionDeclaration(func) => {
            if let Some(body) = func.body.as_deref() {
                let name = func
                    .id
                    .as_ref()
                    .map(|id| id.name.to_string())
                    .unwrap_or_default();
                let fn_sym = format!("{file}::function::{name}");
                let mut scope = Scope::new();
                df_seed_params(&func.params, strings, &mut scope, sink);
                df_flow_body(body, file, &fn_sym, strings, &mut scope, sink);
            }
        }
        S::ExportNamedDeclaration(export) => {
            if let Some(decl) = &export.declaration {
                df_flow_decl(decl, file, strings, sink);
            }
        }
        S::ExportDefaultDeclaration(export) => match &export.declaration {
            ts::ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                if let Some(body) = func.body.as_deref() {
                    let name = func
                        .id
                        .as_ref()
                        .map(|id| id.name.to_string())
                        .unwrap_or_default();
                    let fn_sym = format!("{file}::function::{name}");
                    let mut scope = Scope::new();
                    df_seed_params(&func.params, strings, &mut scope, sink);
                    df_flow_body(body, file, &fn_sym, strings, &mut scope, sink);
                }
            }
            ts::ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                df_flow_class(class, file, strings, sink);
            }
            _ => {}
        },
        S::ClassDeclaration(class) => df_flow_class(class, file, strings, sink),
        // Top-level var/expr/return statements have no enclosing callable; walk
        // them under a fresh empty scope (v5 keys them `{file}::function::<top>`).
        S::VariableDeclaration(_) | S::ExpressionStatement(_) | S::ReturnStatement(_) => {
            let fn_sym = format!("{file}::function::<top>");
            let mut scope = Scope::new();
            df_flow_body_stmt(stmt, file, &fn_sym, strings, &mut scope, sink);
        }
        _ => {}
    }
}

fn df_flow_decl(
    decl: &ts::Declaration,
    file: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    use ts::Declaration as D;
    match decl {
        D::FunctionDeclaration(func) => {
            if let Some(body) = func.body.as_deref() {
                let name = func
                    .id
                    .as_ref()
                    .map(|id| id.name.to_string())
                    .unwrap_or_default();
                let fn_sym = format!("{file}::function::{name}");
                let mut scope = Scope::new();
                df_seed_params(&func.params, strings, &mut scope, sink);
                df_flow_body(body, file, &fn_sym, strings, &mut scope, sink);
            }
        }
        D::ClassDeclaration(class) => df_flow_class(class, file, strings, sink),
        _ => {}
    }
}

/// Each method body flows like a free function's, scoped under v5's
/// `{file}::method::{Owner}.{method}` sym. Field initializers are not covered
/// (no natural enclosing callable scope). Port of v5 `ts_flow_class`.
fn df_flow_class(
    class: &ts::Class,
    file: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    let owner = class
        .id
        .as_ref()
        .map(|id| id.name.to_string())
        .unwrap_or_default();
    for element in &class.body.body {
        let ts::ClassElement::MethodDefinition(method) = element else {
            continue;
        };
        let Some(body) = method.value.body.as_deref() else {
            continue;
        };
        let method_name = match &method.key {
            ts::PropertyKey::StaticIdentifier(key) => key.name.to_string(),
            _ => String::new(),
        };
        let fn_sym = format!("{file}::method::{owner}.{method_name}");
        let mut scope = Scope::new();
        df_seed_params(&method.value.params, strings, &mut scope, sink);
        df_flow_body(body, file, &fn_sym, strings, &mut scope, sink);
    }
}

fn df_flow_body(
    body: &ts::FunctionBody,
    file: &str,
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    for stmt in &body.statements {
        df_flow_body_stmt(stmt, file, fn_sym, strings, scope, sink);
    }
}

/// Lift a function value (arrow or function expression) as its own scope: seed
/// params, then walk the body. An expression-body arrow (`(x) => expr`) wraps
/// the expr as an implicit return into a `ret` node. Port of v5 `ts_lift_fn`.
/// `fn_sym` is the lambda's own sym (the binding-name sym for a const-bound
/// arrow, the `lam_sym` for an inline one): the body's nodes walk under it.
fn df_lift_fn(
    params: &ts::FormalParameters,
    body: &ts::FunctionBody,
    expression: bool,
    file: &str,
    fn_sym: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    let mut scope = Scope::new();
    df_seed_params(params, strings, &mut scope, sink);
    if expression {
        if let Some(ts::Statement::ExpressionStatement(expr_stmt)) = body.statements.first() {
            let value = df_flow_expr(
                &expr_stmt.expression,
                file,
                fn_sym,
                strings,
                &mut scope,
                sink,
            );
            let ret = df_push(sink, strings, expr_stmt.span, DfNodeKind::Ret, None);
            df_edge(sink, value, ret);
        }
    } else {
        for stmt in &body.statements {
            df_flow_body_stmt(stmt, file, fn_sym, strings, &mut scope, sink);
        }
    }
}

fn df_flow_body_stmt(
    stmt: &ts::Statement,
    file: &str,
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    use ts::Statement as S;
    match stmt {
        S::VariableDeclaration(var) => {
            for declarator in &var.declarations {
                // A const-bound arrow / function expression is a callable, not a
                // value: lift its body as its own scope keyed by the binding name
                // (v5 mints `{file}::function::{binding}` and NO closure node).
                if let ts::BindingPattern::BindingIdentifier(binding) = &declarator.id {
                    match &declarator.init {
                        Some(ts::Expression::ArrowFunctionExpression(arrow)) => {
                            let sym = format!("{file}::function::{}", binding.name);
                            df_lift_fn(
                                &arrow.params,
                                &arrow.body,
                                arrow.expression,
                                file,
                                &sym,
                                strings,
                                sink,
                            );
                            continue;
                        }
                        Some(ts::Expression::FunctionExpression(func)) => {
                            if let Some(body) = func.body.as_deref() {
                                let sym = format!("{file}::function::{}", binding.name);
                                df_lift_fn(&func.params, body, false, file, &sym, strings, sink);
                            }
                            continue;
                        }
                        _ => {}
                    }
                }
                let rhs = declarator
                    .init
                    .as_ref()
                    .map(|init| df_flow_expr(init, file, fn_sym, strings, scope, sink));
                if let Some(name) = binding_name(&declarator.id) {
                    let bind = df_push(
                        sink,
                        strings,
                        declarator.span,
                        DfNodeKind::LetBind,
                        Some(&name),
                    );
                    if let Some(rhs) = rhs {
                        df_edge(sink, rhs, bind);
                    }
                    scope.insert(name, bind);
                }
            }
        }
        S::ExpressionStatement(expr_stmt) => {
            let _ = df_flow_expr(&expr_stmt.expression, file, fn_sym, strings, scope, sink);
        }
        // `return EXPR`: the returned value flows into the fn's `ret` node (the
        // sink the interprocedural backward hop reads).
        S::ReturnStatement(ret_stmt) => {
            let ret = df_push(sink, strings, ret_stmt.span, DfNodeKind::Ret, None);
            if let Some(arg) = &ret_stmt.argument {
                let value = df_flow_expr(arg, file, fn_sym, strings, scope, sink);
                df_edge(sink, value, ret);
            }
        }
        S::BlockStatement(block) => {
            for inner in &block.body {
                df_flow_body_stmt(inner, file, fn_sym, strings, scope, sink);
            }
        }
        S::IfStatement(if_stmt) => {
            let _ = df_flow_expr(&if_stmt.test, file, fn_sym, strings, scope, sink);
            df_flow_body_stmt(&if_stmt.consequent, file, fn_sym, strings, scope, sink);
            if let Some(alternate) = &if_stmt.alternate {
                df_flow_body_stmt(alternate, file, fn_sym, strings, scope, sink);
            }
        }
        S::ForStatement(for_stmt) => {
            if let Some(ts::ForStatementInit::VariableDeclaration(var)) = &for_stmt.init {
                for declarator in &var.declarations {
                    let rhs = declarator
                        .init
                        .as_ref()
                        .map(|init| df_flow_expr(init, file, fn_sym, strings, scope, sink));
                    if let Some(name) = binding_name(&declarator.id) {
                        let bind = df_push(
                            sink,
                            strings,
                            declarator.span,
                            DfNodeKind::LetBind,
                            Some(&name),
                        );
                        if let Some(rhs) = rhs {
                            df_edge(sink, rhs, bind);
                        }
                        scope.insert(name, bind);
                    }
                }
            }
            if let Some(test) = &for_stmt.test {
                let _ = df_flow_expr(test, file, fn_sym, strings, scope, sink);
            }
            if let Some(update) = &for_stmt.update {
                let _ = df_flow_expr(update, file, fn_sym, strings, scope, sink);
            }
            // v5 records no var for a classic `for` (ts/flow.rs:346).
            df_loop_row(sink, for_stmt.span, None, None);
            df_flow_body_stmt(&for_stmt.body, file, fn_sym, strings, scope, sink);
        }
        S::ForOfStatement(for_stmt) => df_for_in_of(
            &for_stmt.left,
            &for_stmt.right,
            &for_stmt.body,
            for_stmt.span,
            file,
            fn_sym,
            strings,
            scope,
            sink,
        ),
        S::ForInStatement(for_stmt) => df_for_in_of(
            &for_stmt.left,
            &for_stmt.right,
            &for_stmt.body,
            for_stmt.span,
            file,
            fn_sym,
            strings,
            scope,
            sink,
        ),
        S::WhileStatement(while_stmt) => {
            let _ = df_flow_expr(&while_stmt.test, file, fn_sym, strings, scope, sink);
            df_loop_row(sink, while_stmt.span, None, None);
            df_flow_body_stmt(&while_stmt.body, file, fn_sym, strings, scope, sink);
        }
        S::DoWhileStatement(do_stmt) => {
            let _ = df_flow_expr(&do_stmt.test, file, fn_sym, strings, scope, sink);
            df_loop_row(sink, do_stmt.span, None, None);
            df_flow_body_stmt(&do_stmt.body, file, fn_sym, strings, scope, sink);
        }
        _ => {}
    }
}

/// Shared handling for `for (x of/in coll) body`: bind the loop variable, flow
/// the collection into it, record the loop row, then walk the body.
#[allow(clippy::too_many_arguments)]
fn df_for_in_of(
    left: &ts::ForStatementLeft,
    right: &ts::Expression,
    body: &ts::Statement,
    loop_span: oxc_span::Span,
    file: &str,
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    let collection = df_flow_expr(right, file, fn_sym, strings, scope, sink);
    let mut loop_var = None;
    if let ts::ForStatementLeft::VariableDeclaration(var) = left {
        if let Some(declarator) = var.declarations.first() {
            if let Some(name) = binding_name(&declarator.id) {
                let bind = df_push(
                    sink,
                    strings,
                    declarator.span,
                    DfNodeKind::LetBind,
                    Some(&name),
                );
                df_edge(sink, collection, bind);
                scope.insert(name.clone(), bind);
                loop_var = Some(name);
            }
        }
    }
    df_loop_row(sink, loop_span, loop_var, Some(right.span()));
    df_flow_body_stmt(body, file, fn_sym, strings, scope, sink);
}

/// `f(args)` / `recv.m(args)`: each argument flows into the call result; a
/// member callee flows its receiver in too. (The positional `args` slots are
/// deferred aux; the edges already carry the flow.) Port of v5 `ts_flow_call`.
fn df_flow_call(
    call: &ts::CallExpression,
    span: oxc_span::Span,
    file: &str,
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> NodeRef {
    use ts::Expression as E;
    let receiver = match &call.callee {
        E::StaticMemberExpression(member) => Some(df_flow_expr(
            &member.object,
            file,
            fn_sym,
            strings,
            scope,
            sink,
        )),
        E::ComputedMemberExpression(member) => Some(df_flow_expr(
            &member.object,
            file,
            fn_sym,
            strings,
            scope,
            sink,
        )),
        _ => None,
    };
    let mut arg_ids = Vec::new();
    for arg in &call.arguments {
        if let Some(expr) = arg.as_expression() {
            arg_ids.push(df_flow_expr(expr, file, fn_sym, strings, scope, sink));
        }
    }
    let call_res = df_push(sink, strings, span, DfNodeKind::CallRes, None);
    if let Some(recv) = receiver {
        df_edge(sink, recv, call_res);
        sink.aux.args.push(DfArg {
            call: call_res,
            pos: -1,
            arg: recv,
        });
    }
    for (pos, arg_id) in arg_ids.into_iter().enumerate() {
        df_edge(sink, arg_id, call_res);
        sink.aux.args.push(DfArg {
            call: call_res,
            pos: pos as i64,
            arg: arg_id,
        });
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
    file: &str,
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> NodeRef {
    let object_id = df_flow_expr(object, file, fn_sym, strings, scope, sink);
    let member = df_push(sink, strings, span, DfNodeKind::Member, property);
    df_edge(sink, object_id, member);
    member
}

/// Post-order value flow for one TS expression. Returns the node carrying its
/// value, or a generic `expr` node when the variant isn't chased (conservative:
/// may miss, never invents). Port of v5 `ts_flow_expr`. `fn_sym` is the enclosing
/// callable's sym — an inline lambda's `closure` node name derives from it.
fn df_flow_expr(
    expr: &ts::Expression,
    file: &str,
    fn_sym: &str,
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
        // A string literal carries its cooked value into `df_lit` (the only
        // literal kind that does; numbers/bools/regex stay textless `lit` nodes).
        E::StringLiteral(string) => {
            let node = df_push(sink, strings, span, DfNodeKind::Lit, None);
            sink.aux.lits.push(DfLit {
                node,
                kind: "lit",
                text: string.value.to_string(),
            });
            node
        }
        E::NumericLiteral(_)
        | E::BooleanLiteral(_)
        | E::NullLiteral(_)
        | E::BigIntLiteral(_)
        | E::RegExpLiteral(_) => df_push(sink, strings, span, DfNodeKind::Lit, None),
        E::CallExpression(call) => df_flow_call(call, span, file, fn_sym, strings, scope, sink),
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
                    arg_ids.push(df_flow_expr(expr, file, fn_sym, strings, scope, sink));
                }
            }
            let new_node = df_push(sink, strings, span, DfNodeKind::New, type_name.as_deref());
            for (pos, arg_id) in arg_ids.into_iter().enumerate() {
                df_edge(sink, arg_id, new_node);
                sink.aux.args.push(DfArg {
                    call: new_node,
                    pos: pos as i64,
                    arg: arg_id,
                });
            }
            new_node
        }
        // `{ a: x, ...rest }`: a composite `new` node; each named property
        // records a `df_field` row (spread under "..").
        E::ObjectExpression(object) => {
            let mut filled: Vec<(String, NodeRef)> = Vec::new();
            for property in &object.properties {
                match property {
                    ts::ObjectPropertyKind::ObjectProperty(prop) => {
                        let value = df_flow_expr(&prop.value, file, fn_sym, strings, scope, sink);
                        let name = match &prop.key {
                            ts::PropertyKey::StaticIdentifier(ident) => ident.name.to_string(),
                            ts::PropertyKey::StringLiteral(string) => string.value.to_string(),
                            _ => String::new(),
                        };
                        filled.push((name, value));
                    }
                    ts::ObjectPropertyKind::SpreadProperty(spread) => {
                        let value =
                            df_flow_expr(&spread.argument, file, fn_sym, strings, scope, sink);
                        filled.push(("..".into(), value));
                    }
                }
            }
            let new_node = df_push(sink, strings, span, DfNodeKind::New, None);
            for (name, value) in filled {
                df_edge(sink, value, new_node);
                if !name.is_empty() {
                    sink.aux.fields.push(DfField {
                        owner: new_node,
                        name,
                        value,
                    });
                }
            }
            new_node
        }
        E::ArrayExpression(array) => {
            let mut filled: Vec<(String, NodeRef)> = Vec::new();
            for element in &array.elements {
                match element {
                    ts::ArrayExpressionElement::SpreadElement(spread) => {
                        let value =
                            df_flow_expr(&spread.argument, file, fn_sym, strings, scope, sink);
                        filled.push(("..".into(), value));
                    }
                    ts::ArrayExpressionElement::Elision(_) => {}
                    _ => {
                        if let Some(expr) = element.as_expression() {
                            let value = df_flow_expr(expr, file, fn_sym, strings, scope, sink);
                            filled.push((String::new(), value));
                        }
                    }
                }
            }
            let new_node = df_push(sink, strings, span, DfNodeKind::New, None);
            for (name, value) in filled {
                df_edge(sink, value, new_node);
                if !name.is_empty() {
                    sink.aux.fields.push(DfField {
                        owner: new_node,
                        name,
                        value,
                    });
                }
            }
            new_node
        }
        // recv.prop / recv[prop]: receiver flows into a `member` node.
        E::StaticMemberExpression(member) => df_flow_member(
            &member.object,
            Some(member.property.name.as_str()),
            span,
            file,
            fn_sym,
            strings,
            scope,
            sink,
        ),
        E::ComputedMemberExpression(member) => df_flow_member(
            &member.object,
            None,
            span,
            file,
            fn_sym,
            strings,
            scope,
            sink,
        ),
        // `a + b` is its own `concat` kind (so a string-construction query matches
        // `kind IN (template, concat)`); any other binary op is `binop`.
        E::BinaryExpression(binary) => {
            let left = df_flow_expr(&binary.left, file, fn_sym, strings, scope, sink);
            let right = df_flow_expr(&binary.right, file, fn_sym, strings, scope, sink);
            let kind = if binary.operator == ts::BinaryOperator::Addition {
                CONCAT
            } else {
                DfNodeKind::Binop
            };
            let node = df_push(sink, strings, span, kind, None);
            df_edge(sink, left, node);
            df_edge(sink, right, node);
            if binary.operator == ts::BinaryOperator::Addition {
                sink.aux
                    .lit_spans
                    .push((node, binary.span.start, binary.span.end, "concat"));
            }
            node
        }
        // An INLINE lambda: lift its body as its own scope under v5's `lam_sym`
        // (`{enclosing}::closure::{byte}`; chains when nested), then mint the
        // `closure` VALUE node carrying that exact sym as its name.
        E::ArrowFunctionExpression(arrow) => {
            let lam_sym = format!("{fn_sym}::closure::{}", span.start);
            df_lift_fn(
                &arrow.params,
                &arrow.body,
                arrow.expression,
                file,
                &lam_sym,
                strings,
                sink,
            );
            df_push(sink, strings, span, DfNodeKind::Closure, Some(&lam_sym))
        }
        E::FunctionExpression(func) => match func.body.as_deref() {
            Some(body) => {
                let lam_sym = format!("{fn_sym}::closure::{}", span.start);
                df_lift_fn(&func.params, body, false, file, &lam_sym, strings, sink);
                df_push(sink, strings, span, DfNodeKind::Closure, Some(&lam_sym))
            }
            None => df_push(sink, strings, span, DfNodeKind::Expr, None),
        },
        // Transparent wrappers: flow the inner expression straight through.
        E::ParenthesizedExpression(paren) => {
            df_flow_expr(&paren.expression, file, fn_sym, strings, scope, sink)
        }
        E::TSAsExpression(inner) => {
            df_flow_expr(&inner.expression, file, fn_sym, strings, scope, sink)
        }
        E::TSSatisfiesExpression(inner) => {
            df_flow_expr(&inner.expression, file, fn_sym, strings, scope, sink)
        }
        E::TSNonNullExpression(inner) => {
            df_flow_expr(&inner.expression, file, fn_sym, strings, scope, sink)
        }
        E::AwaitExpression(inner) => {
            df_flow_expr(&inner.argument, file, fn_sym, strings, scope, sink)
        }
        E::TSTypeAssertion(inner) => {
            df_flow_expr(&inner.expression, file, fn_sym, strings, scope, sink)
        }
        E::TSInstantiationExpression(inner) => {
            df_flow_expr(&inner.expression, file, fn_sym, strings, scope, sink)
        }
        E::ChainExpression(chain) => {
            use ts::ChainElement as Chain;
            use ts::MemberExpression as Member;
            match &chain.expression {
                Chain::CallExpression(call) => {
                    df_flow_call(call, span, file, fn_sym, strings, scope, sink)
                }
                other => match other.member_expression() {
                    Some(Member::StaticMemberExpression(member)) => df_flow_member(
                        &member.object,
                        Some(member.property.name.as_str()),
                        span,
                        file,
                        fn_sym,
                        strings,
                        scope,
                        sink,
                    ),
                    Some(Member::ComputedMemberExpression(member)) => df_flow_member(
                        &member.object,
                        None,
                        span,
                        file,
                        fn_sym,
                        strings,
                        scope,
                        sink,
                    ),
                    Some(Member::PrivateFieldExpression(member)) => df_flow_member(
                        &member.object,
                        None,
                        span,
                        file,
                        fn_sym,
                        strings,
                        scope,
                        sink,
                    ),
                    None => df_push(sink, strings, span, DfNodeKind::Expr, None),
                },
            }
        }
        // `x = y` as a value evaluates to the assigned value.
        E::AssignmentExpression(assignment) => {
            df_flow_expr(&assignment.right, file, fn_sym, strings, scope, sink)
        }
        // `test ? cons : alt`: the value is EITHER branch (both flow in); the
        // test is a guard (walked, not edged).
        E::ConditionalExpression(cond) => {
            let _test = df_flow_expr(&cond.test, file, fn_sym, strings, scope, sink);
            let consequent = df_flow_expr(&cond.consequent, file, fn_sym, strings, scope, sink);
            let alternate = df_flow_expr(&cond.alternate, file, fn_sym, strings, scope, sink);
            let node = df_push(sink, strings, span, COND, None);
            df_edge(sink, consequent, node);
            df_edge(sink, alternate, node);
            node
        }
        // `&&` / `||` / `??`: for `||` / `??` the value is EITHER operand; for
        // `&&` the value is the right (left is a guard).
        E::LogicalExpression(logic) => {
            use ts::LogicalOperator as Op;
            let left = df_flow_expr(&logic.left, file, fn_sym, strings, scope, sink);
            let right = df_flow_expr(&logic.right, file, fn_sym, strings, scope, sink);
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
                last = df_flow_expr(sub, file, fn_sym, strings, scope, sink);
            }
            last
        }
        // `` `hello ${name}` ``: each interpolation flows into a `template` node;
        // the raw source slice is the `df_lit` text.
        E::TemplateLiteral(template) => {
            let node = df_push(sink, strings, span, TEMPLATE, None);
            for sub in &template.expressions {
                let value = df_flow_expr(sub, file, fn_sym, strings, scope, sink);
                df_edge(sink, value, node);
            }
            sink.aux
                .lit_spans
                .push((node, template.span.start, template.span.end, "template"));
            node
        }
        E::TaggedTemplateExpression(tagged) => {
            let _tag = df_flow_expr(&tagged.tag, file, fn_sym, strings, scope, sink);
            let node = df_push(sink, strings, span, TEMPLATE, None);
            for sub in &tagged.quasi.expressions {
                let value = df_flow_expr(sub, file, fn_sym, strings, scope, sink);
                df_edge(sink, value, node);
            }
            // The quasi (the TemplateLiteral portion) is the string source; its
            // span excludes the tag prefix.
            sink.aux.lit_spans.push((
                node,
                tagged.quasi.span.start,
                tagged.quasi.span.end,
                "template",
            ));
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
/// `param_pos` aux is emitted as DfParam rows).
fn df_seed_params(
    params: &ts::FormalParameters,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    for (pos, param) in params.items.iter().enumerate() {
        match &param.pattern {
            ts::BindingPattern::BindingIdentifier(binding) => {
                let name = binding.name.to_string();
                let node = df_push(sink, strings, param.span, DfNodeKind::Param, Some(&name));
                sink.aux.params.push(DfParam {
                    node,
                    pos: pos as u32,
                });
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
                        let node =
                            df_push(sink, strings, binding.span, DfNodeKind::Param, Some(&key));
                        sink.aux.params.push(DfParam {
                            node,
                            pos: pos as u32,
                        });
                        scope.insert(binding.name.to_string(), node);
                    }
                }
                if let Some(rest) = &object.rest {
                    if let ts::BindingPattern::BindingIdentifier(binding) = &rest.argument {
                        let name = binding.name.to_string();
                        let node =
                            df_push(sink, strings, binding.span, DfNodeKind::Param, Some(&name));
                        sink.aux.params.push(DfParam {
                            node,
                            pos: pos as u32,
                        });
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

/// One loop row. Port of v5 `ts_loop_fact`. The collection text is a source
/// SLICE, so it rides `loop_collection_spans` for the projector to resolve.
fn df_loop_row(
    sink: &mut FamilyBundle<DfF>,
    loop_span: oxc_span::Span,
    var: Option<String>,
    collection: Option<oxc_span::Span>,
) {
    let index = sink.aux.loops.len();
    sink.aux.loops.push(crate::types::DfLoop {
        span: to_span(loop_span),
        var,
        collection: None,
    });
    if let Some(collection) = collection {
        sink.aux
            .loop_collection_spans
            .push((index, collection.start, collection.end));
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
                let text = self
                    .content
                    .get(span.start as usize..span.end as usize)
                    .unwrap_or_default();
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
                                    self.strings
                                        .intern(&format!("{}.{key}", self.strings.lookup(parent))),
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
            let ts::BindingPattern::BindingIdentifier(name) = &declarator.id else {
                continue;
            };
            let Some(init) = &declarator.init else {
                continue;
            };
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
            self.entities.push(
                Node::new(owner, TypeEntityKind::Const).with_name(self.strings.intern(&name.name)),
            );
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
    let mut walker = ConstWalker {
        content,
        depth: 0,
        strings,
        entities: Vec::new(),
        values: Vec::new(),
    };
    for stmt in with_module_bodies(&program.body) {
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

/// Kinds only TS constructs: the core enums do not carry them (tests/6_kind_vocab.rs).
pub const COND: DfNodeKind = DfNodeKind::Ext(LangKind {
    lang: "ts",
    tag: "cond",
});
pub const CONCAT: DfNodeKind = DfNodeKind::Ext(LangKind {
    lang: "ts",
    tag: "concat",
});
pub const TEMPLATE: DfNodeKind = DfNodeKind::Ext(LangKind {
    lang: "ts",
    tag: "template",
});
/// The module scope as a callable def, so a top-level call site has a caller.
pub const MODULE: CallKind = CallKind::Ext(LangKind {
    lang: "ts",
    tag: "module",
});
/// The `<module>` def's name, and the `caller_name` a module-level edge wears.
pub const MODULE_DEF_NAME: &str = "<module>";

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
            let parsed = {
                let span = trace::parse_span("ts", "astgrep");
                let _entered = span.enter();
                AstGrepParser.parse(&arena, path, content).ok()
            };
            parsed.map(|parsed| {
                let span = trace::family_span("ts", "cst");
                let _entered = span.enter();
                let mut bundle = FamilyBundle::<CstF>::default();
                CstProjector.project(&parsed, &mut strings, &mut bundle);
                trace::record_bundle(&span, &bundle, 0);
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
            let parsed = {
                let span = trace::parse_span("ts", "oxc");
                let _entered = span.enter();
                OxcParser.parse(&arena, path, content)
            };
            if let Ok(parsed) = parsed {
                if mask.types {
                    let span = trace::family_span("ts", "type");
                    let _entered = span.enter();
                    let mut bundle = FamilyBundle::<TypeF>::default();
                    TypeProjector.project(&parsed, &mut strings, &mut bundle);
                    // const facet (port of v5 ts_const_facts_from): needs the
                    // source bytes for template slices. Appends Const entities +
                    // ConstValue rows to the same TypeF bundle.
                    if let Ok(src) = std::str::from_utf8(content) {
                        collect_const_facts(&parsed, src, &mut strings, &mut bundle);
                        ts_doc_facts(&parsed, src, &mut strings, &mut bundle);
                    }
                    trace::record_bundle(&span, &bundle, 0);
                    types = Some(bundle);
                }
                if mask.call {
                    let span = trace::family_span("ts", "call");
                    let _entered = span.enter();
                    let mut bundle = FamilyBundle::<CallF>::default();
                    if let Ok(src) = std::str::from_utf8(content) {
                        let call_projector = CallProjector { content: src };
                        call_projector.project(&parsed, &mut strings, &mut bundle);
                    }
                    trace::record_bundle(&span, &bundle, bundle.aux.sites.len());
                    call = Some(bundle);
                }
                if mask.df {
                    let span = trace::family_span("ts", "df");
                    let _entered = span.enter();
                    let mut bundle = FamilyBundle::<DfF>::default();
                    if let Ok(src) = std::str::from_utf8(content) {
                        let df_projector = DfProjector {
                            file: path,
                            content: src,
                        };
                        df_projector.project(&parsed, &mut strings, &mut bundle);
                    }
                    trace::record_bundle(&span, &bundle, 0);
                    df = Some(bundle);
                }
            }
        }

        ExtractOutput {
            strings,
            cst,
            types,
            call,
            df,
            data: None,
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// Resolve<TypeF> for TsSource (commit 4b-iii). Pure: candidates + sigs in, no
// AST. The candidate row IS the parity target (user ruling 2026-07-24, option
// (a)): v5's `type_edge.to` is free text, so text dsts STAY text — a candidate
// whose `to` names no corpus node (v5's synthetic `Owner::Member`, externals)
// emits a ZERO dst leg (ZERO_CONTENT_ID + Span::empty), never a fake node
// join. The genuinely-resolved span->blob legs are a v6-only ADDITIVE layer
// (reported, never asserted). Same-file blob leg: the TypeF node named `to` in
// THIS bundle gives the span, and the DefIndex span-join gives the blob (the
// output carries no hash of its own). Corpus fallback: a UNIQUE site only.
// ════════════════════════════════════════════════════════════════════════════

impl TsSource {
    /// The deduped, deterministically-ordered candidate list (v5's BTreeSet
    /// shaping): the aux candidates, deduped on (owner, to, kind). `resolve`
    /// emits its edges in EXACTLY this order, one per candidate — the parity
    /// golden zips the two (the zip discipline: edge i resolves candidate i).
    pub fn type_edge_candidates(output: &ExtractOutput) -> Vec<TypeEdgeCandidate> {
        let mut set: BTreeSet<TypeEdgeCandidate> = BTreeSet::new();
        if let Some(types) = &output.types {
            for candidate in &types.aux.candidates {
                set.insert(candidate.clone());
            }
        }
        set.into_iter().collect()
    }
}

/// The dst leg of one candidate: same-file TypeF entity first (its span joined
/// through the `DefIndex` for the blob), else a unique corpus site, else None
/// (text stays text — the zero leg). Name-only resolution, per the 4a ADDENDUM
/// site-key discipline (no receiver typing anywhere in commit 4).
fn resolve_type_dst(
    types: &FamilyBundle<TypeF>,
    strings: &Strings,
    index: Option<&DefIndex>,
    name: &str,
) -> Option<(ContentId, Span)> {
    let same_file = types
        .nodes
        .iter()
        .find(|node| node.name.map_or(false, |id| strings.lookup(id) == name));
    if let (Some(node), Some(index)) = (same_file, index) {
        return corpus_defs(index, name)
            .iter()
            .find(|site| site.span == node.span)
            .map(|site| (site.blob.clone(), site.span));
    }
    let sites = index.map(|index| corpus_defs(index, name)).unwrap_or(&[]);
    match sites {
        [only] => Some((only.blob.clone(), only.span)),
        _ => None,
    }
}

impl Resolve<TypeF> for TsSource {
    fn resolve(&self, output: &ExtractOutput, cx: &ProjectCx) -> Vec<ProjectEdge<TypeF>> {
        let Some(types) = &output.types else {
            return Vec::new();
        };
        let index = cx.indexes.def_index.get();
        // A type reference through an import binds the way the module system
        // binds it, exactly as a call does; the name match is the fallback.
        let modules = cx
            .indexes
            .ts_modules
            .get()
            .zip(own_path(output, cx))
            .filter(|(modules, path)| modules.knows(path));
        let mut edges = Vec::new();
        for candidate in TsSource::type_edge_candidates(output) {
            // src: the TypeF entity at the owner span. Exists by construction
            // (candidates are minted beside their entity); a miss would break
            // the parity golden's zip count loudly, so it is not hidden here.
            let Some(src_ix) = types
                .nodes
                .iter()
                .position(|node| node.span == candidate.owner)
            else {
                continue;
            };
            let referenced = output.strings.lookup(candidate.to);
            let (dst_blob, dst_span) = modules
                .and_then(|(modules, path)| module_target(modules, path, referenced, None).ok())
                .flatten()
                .map(|found| (found.target_blob, found.target_span))
                .or_else(|| resolve_type_dst(types, &output.strings, index, referenced))
                .unwrap_or((ZERO_CONTENT_ID, Span::empty()));
            edges.push(ProjectEdge::new(
                NodeRef(src_ix as u32),
                dst_blob,
                dst_span,
                candidate.kind,
            ));
        }
        edges
    }
}

// ════════════════════════════════════════════════════════════════════════════
// Resolve<CallF> for TsSource (commit 4c-ii). Two legs per the user rulings
// (2026-07-24: scip-override ALLOWED; the v5-shaped name-match stays primary):
//   NameResolve — callee name -> unique def. Same-file WINS via the span-join
//     (def_named in THIS CallF bundle -> its span -> the DefIndex gives the
//     blob); cross-file a UNIQUE corpus blob (CallF facet preferred);
//     ambiguous/absent -> NO ROW (the 4b-iii discipline).
//   ScipOverride — scip's occurrence resolution for the site disagrees with
//     the name-match outcome (a different corpus target, or any corpus target
//     where the name-match bound none): scip's target WINS the edge, the
//     name-match is displaced. The leg needs the corpus scip index
//     (cx.indexes.scip_index) AND the rev-correct reader (cx.reader); either
//     absent -> pure name-match (v5-shaped). scip-EXTERNAL (a library symbol,
//     an unresolved reference, or no occurrence at the site) is NOT a corpus
//     target: it never displaces a NameResolve row and never mints one.
// The arm learns its own blob by the DefIndex span-join (`own_blob`) and its
// scip document by content hash (`join_documents`) — the resolve seam carries
// no path and no bytes (the 4b-i gap), so identity flows through content.
// Per-site edges, no dedup: two calls to one callee are two resolutions. A
// site outside every CallF def (module level) emits no row — v5's call_edge
// has no module caller. `callee_path` stays None for ts: filling it would
// change the committed sample.callf.snap and UPDATE_SNAP is forbidden for
// this increment (the addendum's ts catch-up is DEFERRED to a declared
// snapshot increment — flagged in the 4c-ii report).
// ════════════════════════════════════════════════════════════════════════════

/// This file's supplied path, learned the way `own_blob` learns its blob: the
/// resolve seam carries neither, and the `PathIndex` is the join.
fn own_path<'a>(output: &ExtractOutput, cx: &'a ProjectCx) -> Option<&'a str> {
    let blob = own_blob(cx, output)?;
    cx.indexes.paths.get()?.get(&blob)
}

/// The sites ResolveExport judged AMBIGUOUS (two `export *` arms disagree).
/// ONLY those: a row per unbound free name is 23,894 rows over TS 5.9 `src/**`.
pub fn call_drops(
    output: &ExtractOutput,
    cx: &ProjectCx,
    edges: &[ProjectEdge<CallF>],
) -> Vec<crate::project::ResolveDrop> {
    let Some(call) = &output.call else {
        return Vec::new();
    };
    let Some((modules, path)) = cx.indexes.ts_modules.get().zip(own_path(output, cx)) else {
        return Vec::new();
    };
    let bound: BTreeSet<(u32, u32)> = edges
        .iter()
        .filter_map(|edge| edge.call_site.map(|span| (span.start, span.end())))
        .collect();
    call.aux
        .sites
        .iter()
        .filter(|site| !bound.contains(&(site.span.start, site.span.end())))
        .filter_map(|site| {
            let callee = output.strings.lookup(site.callee);
            let written = site.callee_path.map(|id| output.strings.lookup(id));
            // A traced receiver says WHY the site dropped, like the go arm:
            // an untyped (`inferred`) or union (`ambiguous`) receiver is a
            // policy boundary, never a missing declaration.
            let own_facts = own_blob(cx, output)
                .as_ref()
                .and_then(|blob| ts_receivers::facts_of(blob, None));
            let receiver = own_facts.as_ref().and_then(|facts| {
                facts
                    .rows
                    .iter()
                    .find(|(start, end, _)| {
                        *start == site.span.start && *end == site.span.end()
                    })
                    .map(|(_, _, outcome)| outcome)
            });
            if let Some(outcome) = receiver {
                let reason = match outcome {
                    ts_receivers::TypeBinding::Decl(_) => return None,
                    ts_receivers::TypeBinding::Inferred => UnresolvedReason::Inferred,
                    ts_receivers::TypeBinding::Ambiguous => UnresolvedReason::Ambiguous,
                };
                return Some(crate::project::ResolveDrop {
                    span: site.span,
                    reason,
                    detail: callee.to_string(),
                });
            }
            module_target(modules, path, callee, written)
                .err()
                .map(|()| crate::project::ResolveDrop {
                    span: site.span,
                    reason: UnresolvedReason::Ambiguous,
                    detail: written.unwrap_or(callee).to_string(),
                })
        })
        .collect()
}

/// The module-plane target of one name USED in `path`: an import binding here,
/// bound through ResolveExport. `Err(())` is the ambiguous star-export outcome.
fn module_target(
    modules: &TsModuleIndex,
    path: &str,
    name: &str,
    written: Option<&str>,
) -> Result<Option<ResolvedImport>, ()> {
    match written.and_then(|written| written.rsplit_once('.')) {
        Some((receiver, member)) => modules.member(path, receiver, member),
        None => modules.bind(path, name),
    }
}

impl TsSource {
    /// The name-match target of one callee (the NameResolve leg). Pub so the
    /// scip ratchet re-runs it to classify overrides — same discipline as
    /// `type_edge_candidates` in 4b-iii. Same-file wins via the span-join;
    /// cross-file a unique corpus blob (the CallF facet's site preferred);
    /// ambiguous/absent -> None.
    pub fn call_name_match(
        output: &ExtractOutput,
        index: &DefIndex,
        callee: &str,
    ) -> Option<(ContentId, Span)> {
        let call = output.call.as_ref()?;
        if let Some(r) = def_named(call, &output.strings, callee) {
            let span = call.node(r).span;
            if let Some(site) = corpus_defs(index, callee)
                .iter()
                .find(|site| site.span == span)
            {
                return Some((site.blob.clone(), site.span));
            }
        }
        let sites = corpus_defs(index, callee);
        let mut blobs: Vec<ContentId> = Vec::new();
        for site in sites {
            if !blobs.contains(&site.blob) {
                blobs.push(site.blob.clone());
            }
        }
        let [blob] = blobs.as_slice() else {
            return None;
        };
        let site = sites
            .iter()
            .find(|s| s.family == FamilyTag::Call)
            .unwrap_or(&sites[0]);
        Some((blob.clone(), site.span))
    }
}

/// The scip-resolved corpus target of one call site: the site's occurrence
/// (the shared `site_occurrence` convention) -> its symbol's definition
/// occurrence (scip's own resolution; `local ` symbols document-scoped) ->
/// the containing DefSite (scip's def range marks the identifier, which sits
/// inside v6's whole-declaration def span). None = scip has no corpus answer
/// (external library symbol, unresolved reference, no occurrence at the site,
/// or the target document is outside the corpus).
fn scip_call_target<'a>(
    index: &ScipIndex,
    joined: &[Option<(ContentId, Vec<u8>)>],
    doc_ix: usize,
    site: &CallSite,
    callee: &str,
    def_index: &'a DefIndex,
) -> Option<(ContentId, Span, &'a str)> {
    let doc = &index.documents[doc_ix];
    let (_, content) = joined[doc_ix].as_ref()?;
    let occ = site_occurrence(doc, content, site.span, callee)?;
    let (def_doc_ix, def_occ) = definition_of(index, doc_ix, &occ.symbol)?;
    let def_doc = &index.documents[def_doc_ix];
    let (def_blob, def_content) = joined[def_doc_ix].as_ref()?;
    let ident = byte_range(def_content, def_occ.range, def_doc.position_encoding)?;
    let (name, def_site) = containing_ts_def(def_index, def_blob.clone(), ident)?;
    Some((def_blob.clone(), def_site.span, name))
}

/// `containing_def_site` minus the `<module>` def, which spans the whole file
/// and would win the shared seam's CallF bias over any module-level entity.
fn containing_ts_def(index: &DefIndex, blob: ContentId, span: Span) -> Option<(&str, DefSite)> {
    let mut best: Option<(&str, DefSite)> = None;
    for (name, sites) in &index.map {
        if name == MODULE_DEF_NAME {
            continue;
        }
        for site in sites {
            if site.blob != blob
                || !(site.span.start <= span.start && span.end() <= site.span.end())
            {
                continue;
            }
            let better = match best {
                None => true,
                Some((_, ref incumbent)) => {
                    let call_bias = (
                        site.family == FamilyTag::Call,
                        incumbent.family == FamilyTag::Call,
                    );
                    call_bias.0 && !call_bias.1
                        || (call_bias.0 == call_bias.1
                            && site.span.end() - site.span.start
                                < incumbent.span.end() - incumbent.span.start)
                }
            };
            if better {
                best = Some((name.as_str(), site.clone()));
            }
        }
    }
    best
}

/// ECMAScript standard-library member and global-object function names. A
/// member call on an unknown receiver spelling one of these names a builtin,
/// never the corpus function that happens to share the spelling.
const BUILTIN_MEMBERS: &[&str] = &[
    "abs",
    "add",
    "all",
    "allSettled",
    "any",
    "apply",
    "assign",
    "at",
    "atan2",
    "bind",
    "call",
    "catch",
    "cbrt",
    "ceil",
    "charAt",
    "charCodeAt",
    "clear",
    "clz32",
    "codePointAt",
    "concat",
    "copyWithin",
    "create",
    "defineProperties",
    "defineProperty",
    "endsWith",
    "every",
    "exec",
    "exp",
    "fill",
    "fill",
    "filter",
    "finally",
    "find",
    "findIndex",
    "findLast",
    "findLastIndex",
    "flat",
    "flatMap",
    "floor",
    "forEach",
    "freeze",
    "fromCharCode",
    "fromCodePoint",
    "fromEntries",
    "fround",
    "getOwnPropertyDescriptor",
    "getOwnPropertyDescriptors",
    "getOwnPropertyNames",
    "getOwnPropertySymbols",
    "getPrototypeOf",
    "hasOwnProperty",
    "hypot",
    "imul",
    "includes",
    "indexOf",
    "isExtensible",
    "isFinite",
    "isFrozen",
    "isInteger",
    "isNaN",
    "isPrototypeOf",
    "isSafeInteger",
    "isSealed",
    "join",
    "lastIndexOf",
    "localeCompare",
    "log",
    "log10",
    "log2",
    "map",
    "match",
    "matchAll",
    "max",
    "min",
    "normalize",
    "padEnd",
    "padStart",
    "parse",
    "parseFloat",
    "parseInt",
    "pop",
    "pow",
    "preventExtensions",
    "propertyIsEnumerable",
    "push",
    "race",
    "random",
    "reduce",
    "reduceRight",
    "repeat",
    "replace",
    "replaceAll",
    "reverse",
    "round",
    "search",
    "seal",
    "setPrototypeOf",
    "shift",
    "sign",
    "slice",
    "some",
    "sort",
    "splice",
    "sqrt",
    "startsWith",
    "stringify",
    "substr",
    "substring",
    "test",
    "toExponential",
    "toFixed",
    "toISOString",
    "toLocaleString",
    "toLowerCase",
    "toPrecision",
    "toString",
    "toUpperCase",
    "trim",
    "trimEnd",
    "trimStart",
    "trunc",
    "unshift",
    "valueOf",
];

/// Whether the name match at `target` is a receiver-blind mismatch: a member
/// call whose receiver names no scope this file can see, spelling a builtin
/// member name, bound to something that is not a class member.
fn receiver_blind_builtin(
    output: &ExtractOutput,
    call: &FamilyBundle<CallF>,
    site: &CallSite,
    callee: &str,
    kinds: Option<&KindIndex>,
    target: &(ContentId, Span),
) -> bool {
    if !BUILTIN_MEMBERS.contains(&callee) || !unknown_receiver(output, call, site) {
        return false;
    }
    kinds.and_then(|index| index.get(&target.0, target.1)) != Some(CallKind::Method)
}

/// Whether a site's receiver names no scope this file can see, which makes the
/// trailing segment `call_name_match` reads (`out.push`) mean nothing.
fn unknown_receiver(output: &ExtractOutput, call: &FamilyBundle<CallF>, site: &CallSite) -> bool {
    let Some(path) = site.callee_path else {
        return false;
    };
    let Some((receiver, _)) = output.strings.lookup(path).rsplit_once('.') else {
        return false;
    };
    if receiver == "this" || receiver == "super" {
        return false;
    }
    !call
        .aux
        .specifiers
        .iter()
        .any(|specifier| output.strings.lookup(specifier.name) == receiver)
}

impl Resolve<CallF> for TsSource {
    fn resolve(&self, output: &ExtractOutput, cx: &ProjectCx) -> Vec<ProjectEdge<CallF>> {
        let Some(call) = &output.call else {
            return Vec::new();
        };
        let Some(def_index) = cx.indexes.def_index.get() else {
            return Vec::new();
        };
        let kinds = cx.indexes.kinds.get();
        // The scip leg: the corpus index + the rev-correct reader + this
        // file's own document (found by content hash). Any missing piece ->
        // pure name-match (v5-shaped).
        let scip = cx
            .indexes
            .scip_index
            .get()
            .zip(cx.reader)
            .and_then(|(index, reader)| {
                let joined = cx
                    .indexes
                    .joined_documents
                    .get_or_init(|| join_documents(index, reader));
                let blob = own_blob(cx, output)?;
                let doc_ix = joined
                    .iter()
                    .position(|j| j.as_ref().map_or(false, |(b, _)| *b == blob))?;
                Some((index, joined, doc_ix))
            });
        // The module plane binds an IMPORTED name; the name match is what a
        // free name falls to.
        let modules = cx
            .indexes
            .ts_modules
            .get()
            .zip(own_path(output, cx))
            .filter(|(modules, path)| modules.knows(path));
        let mut edges = Vec::new();
        // The receiver leg: phase 1 typed each member-call site's receiver
        // (one scope-threaded pass per body). A traceable receiver REPLACES
        // the name match: the member is looked up on the receiver's declared
        // type, and a missing member is a drop, never a fallback. The one-hop
        // `const x = f()` binds resolve in source order, so the sites are
        // processed sorted and emitted in file order.
        let own = own_blob(cx, output);
        let paths = cx.indexes.paths.get();
        let own_facts = own
            .as_ref()
            .and_then(|blob| ts_receivers::facts_of(blob, paths));
        let recv_map: HashMap<(u32, u32), &ts_receivers::TypeBinding> = own_facts
            .as_ref()
            .map(|facts| {
                facts
                    .rows
                    .iter()
                    .map(|(start, end, outcome)| ((*start, *end), outcome))
                    .collect()
            })
            .unwrap_or_default();
        let sites = &call.aux.sites;
        let mut order: Vec<usize> = (0..sites.len()).collect();
        order.sort_by_key(|&ix| (sites[ix].span.start, sites[ix].span.end()));
        let mut bound_types: HashMap<NodeRef, HashMap<String, String>> = HashMap::new();
        let mut results: Vec<Option<(NodeRef, ContentId, Span, CallEdgeKind)>> =
            vec![None; sites.len()];
        for ix in order {
            let site = &sites[ix];
            // The caller is the innermost covering CallF def (the 4a
            // caller-binding discipline); the `<module>` def covers the rest.
            let Some(caller) = covering_def(call, site.span) else {
                continue;
            };
            let callee = output.strings.lookup(site.callee);
            let written = site.callee_path.map(|id| output.strings.lookup(id));
            let import_t = modules
                .and_then(|(modules, path)| module_target(modules, path, callee, written).ok())
                .flatten();
            let receiver = recv_map.get(&(site.span.start, site.span.end()));
            let recv_spec: Option<ts_receivers::RecvSpec> = match receiver {
                Some(ts_receivers::TypeBinding::Decl(name)) => {
                    let base = name.clone();
                    Some(
                        own_facts
                            .as_ref()
                            .and_then(|facts| facts.field_recv.get(&site.span.start))
                            .map(|(_, field)| {
                                ts_receivers::RecvSpec::Field(base.clone(), field.clone())
                            })
                            .unwrap_or(ts_receivers::RecvSpec::Type(base)),
                    )
                }
                Some(ts_receivers::TypeBinding::Inferred) => own_facts
                    .as_ref()
                    .and_then(|facts| facts.inferred_recv.get(&site.span.start))
                    .and_then(|var| bound_types.get(&caller).and_then(|names| names.get(var)))
                    .map(|bound| ts_receivers::RecvSpec::Type(bound.clone())),
                Some(ts_receivers::TypeBinding::Ambiguous) | None => None,
            };
            // A traceable receiver owns the site: a member bind is the answer,
            // and a missing member is a drop, never a name-match fallback.
            let recv_t = match (&import_t, receiver) {
                (Some(_), _) | (_, None) => None,
                (None, Some(_)) => match (own.as_ref(), own_facts.as_ref()) {
                    (Some(blob), Some(facts)) => recv_spec.as_ref().and_then(|receiver| {
                        ts_receivers::receiver_member_target(
                            receiver,
                            callee,
                            blob,
                            facts,
                            own_path(output, cx),
                            modules.map(|(modules, _)| modules),
                            paths,
                        )
                    }),
                    _ => None,
                },
            };
            // The scip leg keeps its answer: it types the receiver, this does not.
            let own_t = match &import_t {
                Some(found) => Some((found.target_blob.clone(), found.target_span)),
                None if receiver.is_some() => recv_t,
                None => TsSource::call_name_match(output, def_index, callee)
                    .filter(|t| !receiver_blind_builtin(output, call, site, callee, kinds, t)),
            };
            let own_kind = match import_t {
                Some(_) => CallEdgeKind::ImportResolve,
                None => CallEdgeKind::NameResolve,
            };
            let scip_t = scip.as_ref().and_then(|(index, joined, doc_ix)| {
                scip_call_target(index, joined, *doc_ix, site, callee, def_index)
            });
            // Agreement is judged at (blob, name): the name-match binds the
            // call FACET (e.g. the ctor def) while scip may name the type
            // facet (the class) — one definition, two facet coordinates (the
            // ORACLE entry's "the models differ by construction").
            let final_t = match (own_t, scip_t) {
                (Some(n), Some(s)) if n.0 == s.0 && callee == s.2 => Some(((n.0, n.1), own_kind)),
                (_, Some(s)) => Some(((s.0, s.1), CallEdgeKind::ScipOverride)),
                (Some(n), None) => Some((n, own_kind)),
                (None, None) => None,
            };
            // The one-hop return-type inference: a `const x = f()` init call
            // that just resolved hands its declared return type to the var, in
            // source order, keyed by the covering def.
            if let (Some(facts), Some(((dst_blob, dst_span), _))) =
                (own_facts.as_ref(), final_t.as_ref())
            {
                if let Some(var) = facts.binds.get(&site.span.start) {
                    if let Some(dst_facts) = ts_receivers::facts_of(dst_blob, paths) {
                        if let Some(declared) = dst_facts.ret_of.get(&dst_span.start) {
                            bound_types
                                .entry(caller)
                                .or_default()
                                .insert(var.clone(), declared.clone());
                        }
                    }
                }
            }
            results[ix] = final_t.map(|((blob, span), kind)| (caller, blob, span, kind));
        }
        for (site, result) in call.aux.sites.iter().zip(results) {
            let Some((caller, dst_blob, dst_span, kind)) = result else {
                continue;
            };
            edges
                .push(ProjectEdge::new(caller, dst_blob, dst_span, kind).with_call_site(site.span));
        }
        // A callable NAMED as a value: no site, so no scip occurrence to
        // consult, and the name match is the whole answer.
        for reference in &call.aux.refs {
            if reference.position != RefPosition::Value {
                continue;
            }
            let Some(caller) = covering_def(call, reference.span) else {
                continue;
            };
            let named = output.strings.lookup(reference.functor);
            let bound = modules
                .and_then(|(modules, path)| modules.bind(path, named).ok())
                .flatten()
                .map(|found| (found.target_blob, found.target_span));
            let Some((blob, span)) =
                bound.or_else(|| TsSource::call_name_match(output, def_index, named))
            else {
                continue;
            };
            edges.push(
                ProjectEdge::new(caller, blob, span, CallEdgeKind::ValueRef)
                    .with_call_site(reference.span),
            );
        }
        edges
    }
}
