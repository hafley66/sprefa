//! Receiver typing for ts member calls (`x.f()`): one scope-threaded pass per
//! function body binds each receiver to its declared type name (param
//! annotation, `const x: T`, class field, `this` inside a class, `new T()`,
//! one hop through a `const x = f()` initializer), and the resolve phase binds
//! `T.f` from the declaring class/interface's members. Union, primitive, and
//! literal-inferred receivers stay `Inferred`/`Ambiguous` and never bind.

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
    /// class/interface span start -> `extends` base name as written.
    pub extends_of: HashMap<u32, String>,
    /// type name -> declaration span, UNIQUE names only (a duplicate name in
    /// one file is an ambiguity this tier declines, never a coin flip).
    pub decl_span: HashMap<String, (u32, u32)>,
    /// type declaration span start -> declared method members (name, span).
    pub members: HashMap<u32, Vec<(String, (u32, u32))>>,
    /// (class name, field name) -> field's declared type name, this file's
    /// classes only. Feeds the one-level `this.field.recv()` leg.
    pub fields: HashMap<(String, String), String>,
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
            self.facts
                .decl_span
                .entry(name.clone())
                .or_insert((class.span.start, class.span.end));
            self.facts.type_names.insert(name.clone());
            if let Some(base) = class.super_class.as_ref().and_then(|ext| match ext {
                ts::Expression::Identifier(id) => Some(id.name.to_string()),
                _ => None,
            }) {
                self.facts.extends_of.insert(class.span.start, base);
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
                                .entry(class.span.start)
                                .or_default()
                                .push((
                                    key.name.to_string(),
                                    (method.span.start, method.span.end),
                                ));
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
        self.facts
            .decl_span
            .entry(name.clone())
            .or_insert((interface.span.start, interface.span.end));
        self.facts.type_names.insert(name);
        for ext in &interface.extends {
            if let ts::Expression::Identifier(id) = &ext.expression {
                self.facts
                    .extends_of
                    .insert(interface.span.start, id.name.to_string());
            }
        }
        for member in &interface.body.body {
            if let ts::TSSignature::TSMethodSignature(method) = member {
                if let ts::PropertyKey::StaticIdentifier(key) = &method.key {
                    self.facts
                        .members
                        .entry(interface.span.start)
                        .or_default()
                        .push((key.name.to_string(), (method.span.start, method.span.end)));
                }
            }
        }
        oxc_ast_visit::walk::walk_ts_interface_declaration(self, interface);
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
        if let ts::BindingPattern::BindingIdentifier(id) = &declarator.id {
            let name = id.name.to_string();
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
                if let (TypeBinding::Decl(base), ts::Expression::StaticMemberExpression(object)) =
                    (&outcome, &member.object)
                {
                    self.facts
                        .field_recv
                        .insert(member.span.start, (base.clone(), object.property.name.to_string()));
                }
                if outcome == TypeBinding::Inferred {
                    if let ts::Expression::Identifier(id) = &member.object {
                        self.facts
                            .inferred_recv
                            .insert(member.span.start, id.name.to_string());
                    }
                }
                self.facts.rows.push((member.span.start, member.span.end, outcome));
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
) -> Option<(ContentId, (u32, u32), Arc<TsFileTypes>)> {
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
    anchor: (ContentId, (u32, u32), Arc<TsFileTypes>),
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
    let base = facts.extends_of.get(&decl.0)?.clone();
    let ctx_path = paths.and_then(|paths| paths.get(&blob)).map(str::to_string);
    let (blob, decl, facts) = type_anchor(&base, &blob, &facts, ctx_path.as_deref(), modules, paths)?;
    member_in(&facts, decl, member).map(|span| (blob, to_span(span)))
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
            let field_type = anchor.2.fields.get(&(base.to_string(), field.to_string()))?;
            let ctx_path = paths.and_then(|paths| paths.get(&anchor.0)).map(str::to_string);
            let field_anchor =
                type_anchor(field_type, &anchor.0, &anchor.2, ctx_path.as_deref(), modules, paths)?;
            member_on(field_anchor, member, modules, paths)
        }
    }
}

fn member_in(facts: &TsFileTypes, decl: (u32, u32), member: &str) -> Option<(u32, u32)> {
    facts
        .members
        .get(&decl.0)?
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
