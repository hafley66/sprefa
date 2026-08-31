//! Receiver typing for ts member calls (`x.f()`): one scope-threaded pass per
//! function body binds each receiver to its declared type name (param
//! annotation, `const x: T`, class field, `this` inside a class, `new T()`,
//! one hop through a `const x = f()` initializer, one hop out of a
//! `const { field } = base` pattern), and the resolve phase binds `T.f` from
//! the declaring class/interface's members, folding merged declaration blocks
//! of one type onto one seat and walking `extends` for members and fields
//! alike. Union, primitive, and literal-inferred receivers never bind.
//! @comment-ok: module header, the seam list every lang file opens with

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, OnceLock};

use oxc_allocator::Allocator;
use oxc_ast::ast as ts;
use oxc_ast::ast::Program;
use oxc_ast_visit::Visit as OxcVisit;
use oxc_parser::Parser;
use oxc_span::GetSpan;

use crate::shape::{ContentId, Span};
use crate::types::PathIndex;

/// The per-file facts the receiver plane and the resolve leg share. Keyed by
/// the declaring node's byte span start; every map is one file only.
#[derive(Default)]
pub struct TsFileTypes {
    /// fn-like span start -> declared return type name (a plain
    /// `TSTypeReference` only; unions, primitives, aliases record nothing).
    pub ret_of: HashMap<u32, String>,
    /// canonical declaration span start -> `extends` base names as written.
    pub extends_of: HashMap<u32, Vec<String>>,
    /// type name -> the FIRST declaration span bearing it, the canonical seat
    /// every merged block folds onto.
    pub decl_span: HashMap<String, (u32, u32)>,
    /// Any merged block's span start -> the canonical seat's. Merged blocks of
    /// one `interface X` seat at different spans and share one member set.
    pub canonical_decl: HashMap<u32, u32>,
    /// canonical declaration span start -> declared method members (name,
    /// span), unioned over every merged block.
    pub members: HashMap<u32, Vec<(String, (u32, u32))>>,
    /// (type name, field name) -> field's declared type name, this file's
    /// classes and interfaces.
    pub fields: HashMap<(String, String), String>,
    /// `namespace X {}` seat span start (the declaration's own span AND its
    /// identifier's) -> the functions declared inside it.
    pub namespace_members: HashMap<u32, Vec<(String, (u32, u32))>>,
    /// `const x: T` binding identifier span start -> `T`. The module plane
    /// seats an exported const at its NAME, never at a def node.
    pub const_type: HashMap<u32, String>,
    /// Class and interface names declared in this file (static receivers).
    pub type_names: BTreeSet<String>,
    /// `const x = f()` init-call callee span start -> the bound name. The
    /// resolve phase fills the type after the init call itself resolves.
    pub binds: HashMap<u32, String>,
    /// Inferred receiver site span start -> the bound variable's name.
    pub inferred_recv: HashMap<u32, String>,
    /// Member-call site span start -> (operand base name, field name) for a
    /// one-level field receiver (`base.field`); the resolve leg hops through
    /// the field's declared type on base's declaration.
    pub field_recv: HashMap<u32, (String, String)>,
    /// One row per member-call site whose receiver this pass traced.
    pub rows: Vec<(u32, u32, TypeBinding)>,
}

/// One member-call site's receiver outcome, keyed by the callee member
/// expression span (the `CallSite.span` the rows join on). Rows live in the
/// per-blob facts store, never in the wired `CallFAux`, so the phase-1 wire
/// stays byte-identical to the golden.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeBinding {
    Decl(String),
    /// `const { field } = base`: the receiver IS `base`'s property `field`, so
    /// the member binds on the property's declared type, one hop out.
    Field(String, String),
    Inferred,
    Ambiguous,
}

type TypeScope = Vec<HashMap<String, TypeBinding>>;

fn scope_insert(scope: &mut TypeScope, name: String, binding: TypeBinding) {
    let Some(frame) = scope.last_mut() else {
        return;
    };
    match frame.get(&name) {
        Some(existing) if *existing != binding => {
            frame.insert(name, TypeBinding::Ambiguous);
        }
        _ => {
            frame.insert(name, binding);
        }
    }
}

fn scope_lookup<'a>(scope: &'a TypeScope, name: &str) -> Option<&'a TypeBinding> {
    scope.iter().rev().find_map(|frame| frame.get(name))
}

/// The callables one `namespace X {}` body declares, as (name, span). Only the
/// body's own statements: a nested namespace seats its own members.
fn namespace_member_spans(module: &ts::TSModuleDeclaration) -> Vec<(String, (u32, u32))> {
    let Some(ts::TSModuleDeclarationBody::TSModuleBlock(block)) = &module.body else {
        return Vec::new();
    };
    let mut members = Vec::new();
    for statement in &block.body {
        let declaration = match statement {
            ts::Statement::ExportNamedDeclaration(export) => export.declaration.as_ref(),
            other => other.as_declaration(),
        };
        match declaration {
            Some(ts::Declaration::FunctionDeclaration(func)) => {
                if let Some(id) = &func.id {
                    if func.body.is_some() {
                        members.push((id.name.to_string(), (func.span.start, func.span.end)));
                    }
                }
            }
            Some(ts::Declaration::VariableDeclaration(var)) => {
                for declarator in &var.declarations {
                    let ts::BindingPattern::BindingIdentifier(id) = &declarator.id else {
                        continue;
                    };
                    if matches!(
                        declarator.init,
                        Some(ts::Expression::ArrowFunctionExpression(_))
                            | Some(ts::Expression::FunctionExpression(_))
                    ) {
                        members.push((
                            id.name.to_string(),
                            (declarator.span.start, declarator.span.end),
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    members
}

/// The type name a `TSType` binds, when it is a plain named reference.
fn named_ref_of(ty: &ts::TSType) -> Option<String> {
    match ty {
        ts::TSType::TSTypeReference(reference) => match &reference.type_name {
            ts::TSTypeName::IdentifierReference(id) => Some(id.name.to_string()),
            _ => None,
        },
        _ => None,
    }
}

/// A receiver outcome under the current scope. `None` means "no row", which
/// keeps the site on its pre-existing resolution path (imports, namespace
/// members, operands this tier does not trace).
fn receiver_of(
    expr: &ts::Expression,
    scope: &TypeScope,
    this_type: Option<&String>,
    facts: &TsFileTypes,
) -> Option<TypeBinding> {
    match expr {
        ts::Expression::Identifier(id) => {
            if id.name == "this" {
                return this_type.map(|name| TypeBinding::Decl(name.clone()));
            }
            match scope_lookup(scope, &id.name) {
                Some(binding) => Some(binding.clone()),
                // A bare identifier this file declares as a class/interface is
                // a static receiver (`Repo.load()`).
                None if facts.type_names.contains(id.name.as_str()) => {
                    Some(TypeBinding::Decl(id.name.to_string()))
                }
                None => None,
            }
        }
        ts::Expression::StaticMemberExpression(member) => {
            // The operand's own type: its base's type. The member is left to
            // the resolve leg, which hops through the field's declared type
            // on base's declaration (`field_recv`).
            match receiver_of(&member.object, scope, this_type, facts)? {
                TypeBinding::Decl(base) => Some(TypeBinding::Decl(base)),
                _ => None,
            }
        }
        ts::Expression::NewExpression(new_expr) => match &new_expr.callee {
            ts::Expression::Identifier(id) => Some(TypeBinding::Decl(id.name.to_string())),
            _ => None,
        },
        _ => None,
    }
}

struct ReceiverWalker {
    facts: TsFileTypes,
    scope: TypeScope,
    this_stack: Vec<String>,
}

impl ReceiverWalker {
    fn seed_params(&mut self, params: &ts::FormalParameters) {
        for item in &params.items {
            let ts::BindingPattern::BindingIdentifier(id) = &item.pattern else {
                continue;
            };
            let Some(ann) = &item.type_annotation else {
                continue;
            };
            let binding = match &ann.type_annotation {
                ts::TSType::TSUnionType(_) => TypeBinding::Ambiguous,
                ty => named_ref_of(ty)
                    .map(TypeBinding::Decl)
                    .unwrap_or(TypeBinding::Inferred),
            };
            scope_insert(&mut self.scope, id.name.to_string(), binding);
        }
    }

    /// The canonical member seat of one declared type name: the FIRST block's
    /// span start, with every later merged block mapped onto it.
    fn seat(&mut self, name: &str, start: u32, end: u32) -> u32 {
        let canonical = self
            .facts
            .decl_span
            .entry(name.to_string())
            .or_insert((start, end))
            .0;
        self.facts.canonical_decl.insert(start, canonical);
        self.facts.type_names.insert(name.to_string());
        canonical
    }

    /// `const { field, other: alias } = base`: every property binds to `base`'s
    /// declared type one hop out, which the resolve leg walks through `fields`.
    fn seed_destructured(&mut self, pattern: &ts::ObjectPattern, init: Option<&ts::Expression>) {
        let Some(init) = init else { return };
        let Some(TypeBinding::Decl(base)) =
            receiver_of(init, &self.scope, self.this_stack.last(), &self.facts)
        else {
            return;
        };
        for property in &pattern.properties {
            let ts::PropertyKey::StaticIdentifier(key) = &property.key else {
                continue;
            };
            let ts::BindingPattern::BindingIdentifier(local) = &property.value else {
                continue;
            };
            scope_insert(
                &mut self.scope,
                local.name.to_string(),
                TypeBinding::Field(base.clone(), key.name.to_string()),
            );
        }
    }

    fn enter_callable(
        &mut self,
        params: &ts::FormalParameters,
        ret: Option<&ts::TSTypeAnnotation>,
        fn_start: u32,
    ) {
        self.scope.push(HashMap::new());
        self.seed_params(params);
        if let Some(name) = ret.and_then(|ann| named_ref_of(&ann.type_annotation)) {
            self.facts.ret_of.insert(fn_start, name);
        }
    }
}

impl Default for ReceiverWalker {
    fn default() -> Self {
        Self {
            facts: TsFileTypes::default(),
            scope: vec![HashMap::new()],
            this_stack: Vec::new(),
        }
    }
}

impl<'a> OxcVisit<'a> for ReceiverWalker {
    fn visit_class(&mut self, class: &ts::Class<'a>) {
        if let Some(id) = &class.id {
            let name = id.name.to_string();
            let seat = self.seat(&name, class.span.start, class.span.end);
            if let Some(base) = class.super_class.as_ref().and_then(|ext| match ext {
                ts::Expression::Identifier(id) => Some(id.name.to_string()),
                _ => None,
            }) {
                self.facts.extends_of.entry(seat).or_default().push(base);
            }
            for element in &class.body.body {
                match element {
                    ts::ClassElement::PropertyDefinition(prop) => {
                        let Some(ann) = &prop.type_annotation else {
                            continue;
                        };
                        let Some(type_name) = named_ref_of(&ann.type_annotation) else {
                            continue;
                        };
                        if let ts::PropertyKey::StaticIdentifier(key) = &prop.key {
                            self.facts
                                .fields
                                .insert((name.clone(), key.name.to_string()), type_name);
                        }
                    }
                    ts::ClassElement::MethodDefinition(method) => {
                        if let ts::PropertyKey::StaticIdentifier(key) = &method.key {
                            self.facts
                                .members
                                .entry(seat)
                                .or_default()
                                .push((key.name.to_string(), (method.span.start, method.span.end)));
                        }
                    }
                    _ => {}
                }
            }
            self.this_stack.push(name);
        }
        oxc_ast_visit::walk::walk_class(self, class);
        self.this_stack.pop();
    }

    fn visit_ts_interface_declaration(&mut self, interface: &ts::TSInterfaceDeclaration<'a>) {
        let name = interface.id.name.to_string();
        let seat = self.seat(&name, interface.span.start, interface.span.end);
        for ext in &interface.extends {
            if let ts::Expression::Identifier(id) = &ext.expression {
                self.facts
                    .extends_of
                    .entry(seat)
                    .or_default()
                    .push(id.name.to_string());
            }
        }
        for member in &interface.body.body {
            match member {
                ts::TSSignature::TSMethodSignature(method) => {
                    if let ts::PropertyKey::StaticIdentifier(key) = &method.key {
                        self.facts
                            .members
                            .entry(seat)
                            .or_default()
                            .push((key.name.to_string(), (method.span.start, method.span.end)));
                    }
                }
                // A property signature is a FIELD of the interface: the one-hop
                // `holder.session.f()` and `const { session } = holder` legs.
                ts::TSSignature::TSPropertySignature(prop) => {
                    let Some(ann) = &prop.type_annotation else {
                        continue;
                    };
                    let Some(type_name) = named_ref_of(&ann.type_annotation) else {
                        continue;
                    };
                    if let ts::PropertyKey::StaticIdentifier(key) = &prop.key {
                        self.facts
                            .fields
                            .insert((name.clone(), key.name.to_string()), type_name);
                    }
                }
                _ => {}
            }
        }
        oxc_ast_visit::walk::walk_ts_interface_declaration(self, interface);
    }

    fn visit_ts_module_declaration(&mut self, module: &ts::TSModuleDeclaration<'a>) {
        if let ts::TSModuleDeclarationName::Identifier(id) = &module.id {
            let members = namespace_member_spans(module);
            for seat in [module.span.start, id.span.start] {
                self.facts
                    .namespace_members
                    .entry(seat)
                    .or_default()
                    .extend(members.iter().cloned());
            }
        }
        oxc_ast_visit::walk::walk_ts_module_declaration(self, module);
    }

    fn visit_function(&mut self, func: &ts::Function<'a>, flags: oxc_syntax::scope::ScopeFlags) {
        self.scope.push(HashMap::new());
        self.seed_params(&func.params);
        if let Some(name) = func
            .return_type
            .as_deref()
            .and_then(|ann| named_ref_of(&ann.type_annotation))
        {
            // The corpus def index seats a fn at the `function` keyword, the
            // module plane's export entry at its NAME: both keys, one type.
            if let Some(id) = &func.id {
                self.facts.ret_of.insert(id.span.start, name.clone());
            }
            self.facts.ret_of.insert(func.span.start, name);
        }
        oxc_ast_visit::walk::walk_function(self, func, flags);
        self.scope.pop();
    }

    fn visit_arrow_function_expression(&mut self, arrow: &ts::ArrowFunctionExpression<'a>) {
        self.scope.push(HashMap::new());
        self.seed_params(&arrow.params);
        if let Some(name) = arrow
            .return_type
            .as_deref()
            .and_then(|ann| named_ref_of(&ann.type_annotation))
        {
            self.facts.ret_of.insert(arrow.span.start, name);
        }
        oxc_ast_visit::walk::walk_arrow_function_expression(self, arrow);
        self.scope.pop();
    }

    fn visit_variable_declarator(&mut self, declarator: &ts::VariableDeclarator<'a>) {
        if let ts::BindingPattern::ObjectPattern(pattern) = &declarator.id {
            self.seed_destructured(pattern, declarator.init.as_ref());
        }
        if let ts::BindingPattern::BindingIdentifier(id) = &declarator.id {
            let name = id.name.to_string();
            if let Some(type_name) = declarator
                .type_annotation
                .as_ref()
                .and_then(|ann| named_ref_of(&ann.type_annotation))
            {
                self.facts.const_type.insert(id.span.start, type_name);
            }
            if let Some(ann) = &declarator.type_annotation {
                let binding = match &ann.type_annotation {
                    ts::TSType::TSUnionType(_) => TypeBinding::Ambiguous,
                    ty => named_ref_of(ty)
                        .map(TypeBinding::Decl)
                        .unwrap_or(TypeBinding::Inferred),
                };
                scope_insert(&mut self.scope, name, binding);
            } else if let Some(init) = &declarator.init {
                let binding = match init {
                    ts::Expression::NewExpression(new_expr) => match &new_expr.callee {
                        ts::Expression::Identifier(id) => TypeBinding::Decl(id.name.to_string()),
                        _ => TypeBinding::Inferred,
                    },
                    ts::Expression::CallExpression(call) => {
                        self.facts
                            .binds
                            .insert(call.callee.span().start, name.clone());
                        TypeBinding::Inferred
                    }
                    ts::Expression::Identifier(value) => scope_lookup(&self.scope, &value.name)
                        .cloned()
                        .unwrap_or(TypeBinding::Inferred),
                    _ => TypeBinding::Inferred,
                };
                scope_insert(&mut self.scope, name, binding);
            }
        }
        oxc_ast_visit::walk::walk_variable_declarator(self, declarator);
    }

    fn visit_call_expression(&mut self, call: &ts::CallExpression<'a>) {
        if let ts::Expression::StaticMemberExpression(member) = &call.callee {
            if let Some(outcome) = receiver_of(
                &member.object,
                &self.scope,
                self.this_stack.last(),
                &self.facts,
            ) {
                match (&outcome, &member.object) {
                    (TypeBinding::Decl(base), ts::Expression::StaticMemberExpression(object)) => {
                        self.facts.field_recv.insert(
                            member.span.start,
                            (base.clone(), object.property.name.to_string()),
                        );
                    }
                    (TypeBinding::Field(base, field), _) => {
                        self.facts
                            .field_recv
                            .insert(member.span.start, (base.clone(), field.clone()));
                    }
                    _ => {}
                }
                if outcome == TypeBinding::Inferred {
                    if let ts::Expression::Identifier(id) = &member.object {
                        self.facts
                            .inferred_recv
                            .insert(member.span.start, id.name.to_string());
                    }
                }
                self.facts
                    .rows
                    .push((member.span.start, member.span.end, outcome));
            }
        }
        oxc_ast_visit::walk::walk_call_expression(self, call);
    }
}

/// One pass over `program`: the member-call receiver rows plus the file's
/// declaration facts, as one storeable bundle.
pub fn collect(program: &Program<'_>) -> TsFileTypes {
    let mut walker = ReceiverWalker::default();
    walker.visit_program(program);
    walker.facts
}

fn facts_cache() -> &'static Mutex<HashMap<ContentId, Arc<TsFileTypes>>> {
    static CACHE: OnceLock<Mutex<HashMap<ContentId, Arc<TsFileTypes>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Store one file's facts (phase 1, keyed by the extraction's own content id;
/// the resolve phase then re-reads them with no second parse).
pub fn store_facts(blob: ContentId, facts: TsFileTypes) {
    let cache = facts_cache();
    let mut guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
    guard.insert(blob, Arc::new(facts));
}

/// The facts of one resolve-universe blob: the phase-1 store first (the own
/// file never re-parses), then a parse-on-demand of the supplied path, cached
/// per blob for the process like the go file-facts leg.
pub fn facts_of(
    blob: &ContentId,
    paths: Option<&crate::types::PathIndex>,
) -> Option<Arc<TsFileTypes>> {
    {
        let guard = facts_cache()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(existing) = guard.get(blob) {
            return Some(existing.clone());
        }
    }
    let path = paths?.get(blob)?;
    let bytes = std::fs::read(path).ok()?;
    let src = std::str::from_utf8(&bytes).ok()?;
    let source_type = super::ts::source_type_for(path)?;
    let allocator = Allocator::default();
    let ret = Parser::new(&allocator, src, source_type).parse();
    if ret.panicked {
        return None;
    }
    let mut walker = ReceiverWalker::default();
    walker.visit_program(&ret.program);
    let facts = Arc::new(walker.facts);
    let cache = facts_cache();
    let mut guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
    guard.insert(blob.clone(), Arc::clone(&facts));
    Some(facts)
}

/// One member-call site's receiver operand shape: the operand's declared type
/// name, or a one-level field read (`base.field`, the field's declared type
/// carries the member).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecvSpec {
    Type(String),
    Field(String, String),
}

/// One type name resolved to where it is DECLARED: the blob, the declaration
/// span, and that file's facts.
type TypeAnchor = (ContentId, (u32, u32), Arc<TsFileTypes>);

/// Where a type name used in `ctx_path` is DECLARED: a unique same-file class
/// / interface, else one import binding through the module plane. Returns the
/// declaring blob, the declaration span, and that file's facts.
fn type_anchor(
    type_name: &str,
    ctx_blob: &ContentId,
    ctx_facts: &Arc<TsFileTypes>,
    ctx_path: Option<&str>,
    modules: Option<&crate::lang::ts_resolve::TsModuleIndex>,
    paths: Option<&PathIndex>,
) -> Option<TypeAnchor> {
    if let Some(span) = ctx_facts.decl_span.get(type_name) {
        return Some((ctx_blob.clone(), *span, Arc::clone(ctx_facts)));
    }
    let modules = modules?;
    let found = modules.bind(ctx_path?, type_name).ok()??;
    let facts = facts_of(&found.target_blob, paths)?;
    Some((
        found.target_blob,
        (found.target_span.start, found.target_span.end()),
        facts,
    ))
}

/// The member `member` of one anchored type as a corpus def site: the
/// member's span in the declaring class/interface, `extends` one hop up when
/// the member is inherited. Deterministic on overloads (the largest span wins,
/// which is the implementation, never an overload signature).
fn member_on(
    anchor: TypeAnchor,
    member: &str,
    modules: Option<&crate::lang::ts_resolve::TsModuleIndex>,
    paths: Option<&PathIndex>,
) -> Option<(ContentId, Span)> {
    let (blob, decl, facts) = anchor;
    if let Some(found) = member_in(&facts, decl, member) {
        return Some((blob, to_span(found)));
    }
    // One extends hop: the base is declared in the same file as the derived
    // type, or imported into it (resolved through the declaring file's path).
    let seat = facts.canonical_decl.get(&decl.0).copied().unwrap_or(decl.0);
    let bases = facts.extends_of.get(&seat)?.clone();
    let ctx_path = paths.and_then(|paths| paths.get(&blob)).map(str::to_string);
    bases.iter().find_map(|base| {
        let (blob, decl, facts) =
            type_anchor(base, &blob, &facts, ctx_path.as_deref(), modules, paths)?;
        member_in(&facts, decl, member).map(|span| (blob, to_span(span)))
    })
}

/// The corpus def site of `member` on a receiver: a declared type name, or one
/// field hop (`base.field` -> the field's declared type on base's
/// declaration, then the member on THAT type). Deterministic on overloads
/// (the largest span wins, which is the implementation, never an overload
/// signature).
pub fn receiver_member_target(
    receiver: &RecvSpec,
    member: &str,
    own_blob: &ContentId,
    own_facts: &Arc<TsFileTypes>,
    own_path: Option<&str>,
    modules: Option<&crate::lang::ts_resolve::TsModuleIndex>,
    paths: Option<&PathIndex>,
) -> Option<(ContentId, Span)> {
    match receiver {
        RecvSpec::Type(type_name) => {
            let anchor = type_anchor(type_name, own_blob, own_facts, own_path, modules, paths)?;
            member_on(anchor, member, modules, paths)
        }
        RecvSpec::Field(base, field) => {
            let anchor = type_anchor(base, own_blob, own_facts, own_path, modules, paths)?;
            let (owner, field_type) = field_on(anchor, base, field, modules, paths)?;
            let ctx_path = paths
                .and_then(|paths| paths.get(&owner.0))
                .map(str::to_string);
            let field_anchor = type_anchor(
                &field_type,
                &owner.0,
                &owner.2,
                ctx_path.as_deref(),
                modules,
                paths,
            )?;
            member_on(field_anchor, member, modules, paths)
        }
    }
}

/// One anchored type's declared FIELD, its own or a base's, as (the anchor
/// that OWNS it, the field's written type name).
fn field_on(
    anchor: TypeAnchor,
    type_name: &str,
    field: &str,
    modules: Option<&crate::lang::ts_resolve::TsModuleIndex>,
    paths: Option<&PathIndex>,
) -> Option<(TypeAnchor, String)> {
    let mut seen = BTreeSet::new();
    let mut frontier = vec![(anchor, type_name.to_string())];
    while let Some((anchor, name)) = frontier.pop() {
        if !seen.insert((anchor.0.clone(), anchor.1)) {
            continue;
        }
        if let Some(found) = anchor.2.fields.get(&(name.clone(), field.to_string())) {
            let found = found.clone();
            return Some((anchor, found));
        }
        let seat = anchor
            .2
            .canonical_decl
            .get(&anchor.1 .0)
            .copied()
            .unwrap_or(anchor.1 .0);
        let Some(bases) = anchor.2.extends_of.get(&seat).cloned() else {
            continue;
        };
        let ctx_path = paths
            .and_then(|paths| paths.get(&anchor.0))
            .map(str::to_string);
        for base in bases {
            if let Some(next) = type_anchor(
                &base,
                &anchor.0,
                &anchor.2,
                ctx_path.as_deref(),
                modules,
                paths,
            ) {
                frontier.push((next, base));
            }
        }
    }
    None
}

fn member_in(facts: &TsFileTypes, decl: (u32, u32), member: &str) -> Option<(u32, u32)> {
    let seat = facts.canonical_decl.get(&decl.0).copied().unwrap_or(decl.0);
    facts
        .members
        .get(&seat)?
        .iter()
        .filter(|(name, _)| name == member)
        .map(|(_, span)| *span)
        .max_by_key(|(start, end)| end - start)
}

fn to_span(span: (u32, u32)) -> Span {
    Span {
        start: span.0,
        len: span.1 - span.0,
    }
}
