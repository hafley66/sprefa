//! Diet type graph extractors. Intentionally syntax-only: parse a file (`syn`
//! for Rust, tree-sitter for Kotlin, oxc for TypeScript/TSX), walk item/type
//! shapes, and emit deterministic edges the engine stores as
//! `type_edge(from, to, kind)`. All languages share one kind vocabulary —
//! field | variant | impl | generic — so closure queries written for one
//! work on the others. The TS extractor additionally treats functions as
//! edge owners: param | returns | uses (input types, the output type, and
//! types referenced in the body), so a function node reaches the types it
//! consumes, produces, and mentions.

use std::collections::BTreeSet;

use syn::{
    AngleBracketedGenericArguments, Fields, GenericArgument, GenericParam, Generics, Item, Path,
    PathArguments, ReturnType, Type, TypeParamBound, WherePredicate,
};
use syn::spanned::Spanned;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TypeEdge {
    pub from: String,
    pub to: String,
    pub kind: &'static str,
}

/// What a declared symbol is. sem's entity_type, shared across languages so the
/// deck can style a function differently from a data type. The `tag` is the
/// short slug used in a symbol id and in the `type_entity.kind` column.
/// Matched exhaustively ONLY in `tag()`, so adding a variant means one new arm
/// there and nothing else engine-side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityKind {
    Struct,
    Enum,
    Trait,
    Class,
    Interface,
    Alias,
    Function,
    Method,
    Const,
    /// A Python source file's module scope, only minted so a module docstring
    /// (which documents no class/function) has a `type_entity` row to join.
    Module,
}

impl EntityKind {
    pub fn tag(self) -> &'static str {
        match self {
            EntityKind::Struct => "struct",
            EntityKind::Enum => "enum",
            EntityKind::Trait => "trait",
            EntityKind::Class => "class",
            EntityKind::Interface => "interface",
            EntityKind::Alias => "alias",
            EntityKind::Function => "function",
            EntityKind::Method => "method",
            EntityKind::Const => "const",
            EntityKind::Module => "module",
        }
    }
    /// Functions and methods carry an arrow type; everything else is a data type.
    pub fn is_callable(self) -> bool {
        matches!(self, EntityKind::Function | EntityKind::Method)
    }
}

/// One slot in a function's arrow type. Resolution later binds `Named` to a
/// definition symbol (`Resolved`) or leaves it `Unresolved` (stdlib/extern).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeRef {
    Named(String),
    Resolved(String),
    Unresolved(String),
}

impl TypeRef {
    pub fn name(&self) -> &str {
        match self {
            TypeRef::Named(s) | TypeRef::Resolved(s) | TypeRef::Unresolved(s) => s,
        }
    }
}

/// A function *is* a type: `[...A] => B`. `params` is the ordered input refs,
/// `ret` the output ref. A param slot with several refs (e.g. a union) keeps
/// them all; an empty slot means a non-type (keyword/primitive) parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeExpr {
    pub params: Vec<Vec<TypeRef>>,
    pub ret: Vec<TypeRef>,
}

/// A declared type-or-function entity: sem's SemanticEntity trimmed to what the
/// type graph needs (identity, kind, location, parent, and -- for callables --
/// the arrow type). No content/hashes; those live in the spine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeEntity {
    pub sym: String,
    pub name: String,
    pub kind: EntityKind,
    pub parent: Option<String>,
    pub file: String,
    pub line: u32,
    pub ty: Option<TypeExpr>,
}

/// One language's extraction of a file: declared entities + the flat edge graph
/// + the doc comment attached to each entity (Tier 1/2 doc gen) + the resolved
/// string values of its immutable const bindings (item 3 of the string-values
/// arc, plans/2026-07-10-string-values-const-value.md).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypeFacts {
    pub entities: Vec<TypeEntity>,
    pub edges: Vec<TypeEdge>,
    pub docs: Vec<DocFact>,
    pub consts: Vec<ConstValueFact>,
    /// How many object-literal spread properties were skipped (never followed
    /// — the value is opaque without evaluating the spread source). Loud-skip
    /// counter, summed and reported once per `refresh_type_rels` call.
    pub const_spread_skips: usize,
    /// How many `let`/`var` string initializers were skipped (soundness rule:
    /// only `const` and `as const` bindings are honest to fold — a mutable
    /// binding can change under your feet). Loud-skip counter, same reporting
    /// shape as `const_spread_skips`.
    pub const_mutable_skips: usize,
}

/// One string value folded from a `const` (or `as const`) binding — the
/// `const_value` relation's per-language payload. `sym` is the OWNING entity's
/// sym: the const's own sym for a plain/object-literal const, or the ENUM's
/// sym for a string enum member (the member name lives in `field` instead of
/// a second entity). `field` is `""` for a bare `const name = "..."`, else a
/// dotted key path (`"home"`, `"nested.a"`) for an object-literal property, or
/// the bare member name for an enum member. `text` is the cooked value for a
/// plain string literal, or the raw source slice (`${}` holes intact) for a
/// template. `kind` is `lit` or `template` (see `builtin_enum_brands`'s
/// `const_value_kind`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstValueFact {
    pub sym: String,
    pub field: String,
    pub text: String,
    pub kind: &'static str,
    pub file: String,
    pub line: u32,
}

/// The doc comment bound to one declared entity. `sym` is the same
/// `file::kind::name` minted for the entity, so `doc_comment` joins `type_entity`
/// 1:1. `text` is the cleaned block (markers + per-line `*`/`///` stripped, leading
/// space dropped). `tags` is the structured split (Tier 2). The locator is
/// per-language and AST-anchored: Rust reads `#[doc]` attrs, Kotlin the preceding
/// KDoc sibling, TS the `/** */` block that immediately precedes the decl.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocFact {
    pub sym: String,
    pub line: u32,
    pub text: String,
    pub tags: Vec<DocTag>,
}

/// One structured doc tag. `tag` is the bare tag word (`param`, `returns`,
/// `deprecated`, `throws`, or `section` for a rustdoc `# Heading`). `arg` is the
/// name a `@param name` / `@property name` carries, else "". `text` is the
/// description body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocTag {
    pub tag: String,
    pub arg: String,
    pub text: String,
}

/// The first non-empty line of a doc block — the Tier-0 summary.
pub fn doc_summary(text: &str) -> &str {
    text.lines().map(|l| l.trim()).find(|l| !l.is_empty()).unwrap_or("")
}

/// Strip a `/** ... */` (or `/* ... */` / `/*! ... */`) block down to its
/// prose: drop the delimiters, the leading `*` and one space on each inner line,
/// and the blank leading/trailing lines. Shared by the Kotlin (KDoc) and TS
/// (JSDoc) locators, and by the `comment_node` classifier (`crate::cst`).
pub(crate) fn clean_block_comment(raw: &str) -> String {
    let inner = raw.trim();
    let inner = inner.strip_prefix("/**")
        .or_else(|| inner.strip_prefix("/*!"))
        .or_else(|| inner.strip_prefix("/*")).unwrap_or(inner);
    let inner = inner.strip_suffix("*/").unwrap_or(inner);
    let mut lines: Vec<String> = inner.lines().map(|l| {
        let t = l.trim_start();
        let t = t.strip_prefix('*').unwrap_or(t);
        t.strip_prefix(' ').unwrap_or(t).to_string()
    }).collect();
    while lines.first().is_some_and(|s| s.trim().is_empty()) { lines.remove(0); }
    while lines.last().is_some_and(|s| s.trim().is_empty()) { lines.pop(); }
    lines.join("\n")
}

/// Split a JSDoc/KDoc block into `@tag` rows: `@tag [{type}] [name] description`.
/// `@param`/`@property`/`@throws`/type-param tags carry a leading name into `arg`;
/// others put the whole body in `text`. A leading `{type}` annotation is dropped
/// (the type graph already carries types via `type_sig`).
fn parse_jsdoc_tags(text: &str) -> Vec<DocTag> {
    let mut out = Vec::new();
    for line in text.lines() {
        let l = line.trim_start();
        let Some(rest) = l.strip_prefix('@') else { continue };
        let mut it = rest.splitn(2, char::is_whitespace);
        let tag = it.next().unwrap_or("").to_string();
        let mut body = it.next().unwrap_or("").trim_start();
        if body.starts_with('{') {
            if let Some(end) = body.find('}') { body = body[end + 1..].trim_start(); }
        }
        let named = matches!(tag.as_str(),
            "param" | "arg" | "argument" | "property" | "prop" | "throws" | "exception" | "typeparam" | "tparam");
        let (arg, desc) = if named {
            let mut bi = body.splitn(2, char::is_whitespace);
            (bi.next().unwrap_or("").to_string(), bi.next().unwrap_or("").trim().to_string())
        } else {
            (String::new(), body.trim().to_string())
        };
        out.push(DocTag { tag, arg, text: desc });
    }
    out
}

/// Split a rustdoc body into its markdown `# Heading` sections (`# Panics`,
/// `# Safety`, `# Examples`, ...). rustdoc has no `@`-tags; sections ARE the
/// structure. Each heading becomes a `section` tag whose `arg` is the heading
/// text and `text` is the lines until the next heading.
fn parse_rust_sections(text: &str) -> Vec<DocTag> {
    let mut out = Vec::new();
    let mut cur: Option<(String, Vec<&str>)> = None;
    let flush = |cur: Option<(String, Vec<&str>)>, out: &mut Vec<DocTag>| {
        if let Some((name, body)) = cur {
            out.push(DocTag { tag: "section".into(), arg: name, text: body.join("\n").trim().to_string() });
        }
    };
    for line in text.lines() {
        if let Some(rest) = line.trim_start().strip_prefix("# ") {
            flush(cur.take(), &mut out);
            cur = Some((rest.trim().to_string(), Vec::new()));
        } else if let Some((_, body)) = cur.as_mut() {
            body.push(line);
        }
    }
    flush(cur, &mut out);
    out
}

/// Phase D call-graph extraction: callable definitions + the raw call sites a
/// file contains. Caller resolution (which def encloses a site) is a second
/// pass in the engine, not the extractor's job; extractors emit sites with the
/// callee text as it appears (bare or qualified), and the engine resolves to a
/// def sym when unique, the same path `type_link` uses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CallFacts {
    pub defs: Vec<CallDef>,
    pub sites: Vec<CallSite>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallDef {
    pub sym: String,        // file::function::name (free) or file::method::Parent.name
    pub name: String,       // bare callable name, for callee resolution (not written)
    pub kind: CallKind,
    pub file: String,
    pub line: u32,
    pub end: u32,           // body span end (1-based line), for callsite containment
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallSite {
    pub caller_sym: Option<String>,   // filled by the engine's span-containment pass
    pub callee: String,               // trailing segment (bare name) for resolution
    pub callee_path: Option<String>,  // full qualified path when >1 segment (e.g. sprefa_v5::cli::run)
    pub file: String,
    pub line: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallKind {
    Free,
    Method,
    Closure,
}

impl CallKind {
    pub fn tag(self) -> &'static str {
        match self {
            CallKind::Free => "function",
            CallKind::Method => "method",
            CallKind::Closure => "closure",
        }
    }
}

/// Intra-procedural dataflow facts (the lift-to-node model). Each value-bearing
/// position in a fn body becomes a `DfNode` whose id is `file:line:col` of its
/// span start (unique per program point); local value flow becomes `DfEdge`. The
/// engine stores these as `df_node` / `df_edge`, and a rule
/// `df_reaches(a,b) <- closure(df_edge)` walks the lifted graph transitively on
/// the SAME SCC engine the call/type/module graphs already use. Approximate by
/// design: no SSA, no borrow/alias unification, no interprocedural arg/return
/// stitching yet — a deliberate first slice proving the engine-side work is done.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DataflowFacts {
    pub nodes: Vec<DfNode>,
    pub edges: Vec<DfEdge>,
    pub loops: Vec<LoopFact>,
    pub allocators: std::collections::HashSet<String>, // fn syms whose body builds a collection
    pub nests: Vec<NestFact>,
    pub param_pos: Vec<(String, u32)>, // (param node id, positional index) for node-level type joins
    /// (call/new node id, position, arg node id): which argument slot a value
    /// feeds. Position is 0-based and aligns with `param_pos`/`type_sig.pos`
    /// (Rust method receivers are pos -1, mirroring the skipped `self` param).
    pub args: Vec<(String, i64, String)>,
    /// (new/call node id, field name, value node id): named value flow into a
    /// composite — Rust struct-literal fields, TS object-literal properties,
    /// Kotlin named arguments.
    pub fields: Vec<(String, String, String)>,
    /// (df_node id, text, kind∈lit|template|concat): the `df_lit` relation's
    /// payload — one row per STRING-carrying value node. `lit` rows carry the
    /// cooked literal value (numbers/bools/regex are never pushed here, only
    /// `syn::Lit::Str`/oxc `StringLiteral`); `template`/`concat` rows carry the
    /// RAW source slice (`${}` holes intact for a template, the written
    /// operands for a `+` concat — a syntactic label, not a type judgment, so
    /// a numeric `+` mints a concat row too). TS/TSX/JS populate `template`/
    /// `concat`; Rust populates `lit` only (Kotlin/Go/Python ledgered).
    pub lits: Vec<(String, String, &'static str)>,
    /// Pending (df_node id, byte_start, byte_end, kind) rows for `template`/
    /// `concat` nodes, whose text is a source SLICE the per-node lift doesn't
    /// have handy (`ts_flow_expr` only carries the line-offset table, not the
    /// raw file text). `ts_dataflow_from` drains this into `lits` once, after
    /// the walk — the one place that already holds `content` — so no
    /// recursive function between the two needs it threaded through.
    pub lit_spans: Vec<(String, u32, u32, &'static str)>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoopFact {
    pub file: String,
    pub start: u32,        // start line of the loop header
    pub end: u32,          // close line of the loop body (span end)
    pub var: String,       // loop variable name, "" when none (while/loop)
    pub collection: String, // textual form of the iterated collection, "" when none
    pub fn_sym: String,
}

/// One row of the `nest` relation: a `call_res` node, the loop it sits in, the
/// loop's depth in the surrounding nest (1 = outermost), and that loop's
/// iterated collection. Composed over `call_edge` this gives the symbolic
/// cost shape "depth-N over C" without resolving trip counts. Emitted by the
/// post-pass `compute_nests` from already-extracted `DfNode`/`LoopFact` rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NestFact {
    pub call_id: String,
    pub loop_id: String,    // "{file}:{start}", joins back to loop_over by (file, start)
    pub depth: u32,         // 1 = outermost enclosing loop
    pub collection: String, // the inner loop's collection text ("" until extractors fill it)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DfNode {
    pub id: String,
    pub kind: String,   // param | let_bind | var_read | var_write | lit | call_res | new | member | ret | borrow | binop | unop | loop | if | match | block | closure | try | expr
    pub var: String,    // variable name when the node is var-related, else ""
    pub fn_sym: String, // enclosing def sym (file::function::name), joins call_def
    pub file: String,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DfEdge {
    pub from: String,
    pub to: String,
}

/// sem-style symbol id: `file::kind::name`, scoped by an optional parent for
/// methods (`file::method::Class.name`). Stable, index-free, human-readable.
pub fn mint_sym(file: &str, kind: EntityKind, name: &str, parent: Option<&str>) -> String {
    match parent {
        Some(p) => format!("{file}::{}::{p}.{name}", kind.tag()),
        None => format!("{file}::{}::{name}", kind.tag()),
    }
}

/// The common interface: a language front-end that recognizes paths and turns a
/// file's source into `TypeFacts`. The per-language specifics (syn, tree-sitter,
/// oxc) live behind this; the engine asks the registry, never the extension.
pub trait TypeLang: Sync {
    fn name(&self) -> &'static str;
    fn matches(&self, path: &str) -> bool;
    fn extract(&self, file: &str, content: &str) -> TypeFacts;
    /// Phase D call-graph extraction. The default returns empty `CallFacts` so
    /// the lazy-indexer wiring (`CALL_RELS`) is live end to end with zero rows;
    /// each front-end overrides this as its extractor lands. One parse can feed
    /// both `extract` and `extract_calls`, but that join is a follow-up.
    fn extract_calls(&self, _file: &str, _content: &str) -> CallFacts { CallFacts::default() }
    /// Intra-procedural dataflow lift (see `DataflowFacts`). Default empty so the
    /// lazy `DATAFLOW_RELS` wiring is live end to end with zero rows; each
    /// front-end overrides as its extractor lands.
    fn extract_dataflow(&self, _file: &str, _content: &str) -> DataflowFacts { DataflowFacts::default() }

    /// Whether `extract_bundle` actually shares one parse across projections.
    /// The engine uses this to avoid routing languages through the bundle seam
    /// when their default implementation would still parse once per family.
    fn supports_analysis_bundle(&self) -> bool { false }

    /// Experimental one-parse/many-projection seam. Languages can override
    /// this when their three extractors share a parse representation.
    fn extract_bundle(&self, file: &str, content: &str, mask: AnalysisMask) -> AnalysisBundle {
        AnalysisBundle {
            types: mask.types.then(|| self.extract(file, content)),
            calls: mask.calls.then(|| self.extract_calls(file, content)),
            dataflow: mask.dataflow.then(|| self.extract_dataflow(file, content)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnalysisMask {
    pub types: bool,
    pub calls: bool,
    pub dataflow: bool,
}

impl AnalysisMask {
    pub const ALL: Self = Self { types: true, calls: true, dataflow: true };
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AnalysisBundle {
    pub types: Option<TypeFacts>,
    pub calls: Option<CallFacts>,
    pub dataflow: Option<DataflowFacts>,
}

// LANG-JUNCTION(typelang-registry): impl `TypeLang { name, matches, extract }` and register it here; buys type_entity/type_edge/type_sig/call_*/df_*/doc_comment for the language (the index-free diet tier, Kotlin-sized)
/// Registry order matters: `.kts` matches before `.ts` would, so KotlinTypes
/// must precede TsTypes. `.go` doesn't overlap any other extension, so
/// GoTypes' position is arbitrary. The engine picks the first `matches` hit.
pub fn type_langs() -> &'static [&'static dyn TypeLang] {
    &[&RustTypes, &KotlinTypes, &TsTypes, &GoTypes, &PyTypes]
}

pub struct RustTypes;
pub struct KotlinTypes;
pub struct TsTypes;
pub struct GoTypes;
pub struct PyTypes;

impl TypeLang for GoTypes {
    fn name(&self) -> &'static str { "go" }
    fn matches(&self, path: &str) -> bool { path.ends_with(".go") }
    // One tree-sitter parse feeds the entity, edge, and doc walks.
    fn extract(&self, file: &str, content: &str) -> TypeFacts {
        let Some(tree) = go_parse(content) else { return TypeFacts::default(); };
        let src = content.as_bytes();
        let root = tree.root_node();
        let owners = go_owner_kinds(root, src);
        let mut entities = Vec::new();
        walk_go_entities(root, src, file, &owners, &mut entities);
        let mut docs = Vec::new();
        walk_go_docs(root, src, file, &mut docs);
        TypeFacts { entities, edges: go_edges_from(root, src), docs, ..Default::default() }
    }
    fn extract_calls(&self, file: &str, content: &str) -> CallFacts {
        let Some(tree) = go_parse(content) else { return CallFacts::default(); };
        let src = content.as_bytes();
        let root = tree.root_node();
        let mut defs = Vec::new();
        go_walk_call_defs(root, src, file, &mut defs);
        let mut sites = Vec::new();
        go_walk_call_sites(root, src, file, &mut sites);
        CallFacts { defs, sites }
    }
    fn extract_dataflow(&self, file: &str, content: &str) -> DataflowFacts {
        let Some(tree) = go_parse(content) else { return DataflowFacts::default(); };
        go_dataflow_from(tree.root_node(), content.as_bytes(), file)
    }
}

impl TypeLang for RustTypes {
    fn name(&self) -> &'static str { "rust" }
    fn matches(&self, path: &str) -> bool { path.ends_with(".rs") }
    fn supports_analysis_bundle(&self) -> bool { true }
    // One syn parse feeds both the entity pass and the edge pass.
    fn extract(&self, file: &str, content: &str) -> TypeFacts {
        let Ok(parsed) = syn::parse_file(content) else {
            return TypeFacts::default();
        };
        let mut entities = rust_entities_from(&parsed, file);
        let (const_entities, consts) = rust_const_values_from(&parsed, file);
        entities.extend(const_entities);
        TypeFacts {
            entities,
            edges: edges_from(&parsed),
            docs: rust_docs_from(&parsed, file),
            consts,
            ..Default::default()
        }
    }
    // One syn parse feeds defs + sites; a follow-up folds this into `extract`
    // so a file parses once per tick instead of twice.
    fn extract_calls(&self, file: &str, content: &str) -> CallFacts {
        let Ok(parsed) = syn::parse_file(content) else {
            return CallFacts::default();
        };
        CallFacts {
            defs: rust_call_defs_from(&parsed, file),
            sites: rust_call_sites_from(&parsed, file),
        }
    }
    // One syn parse feeds the node + edge lift. Same `parse_file` cost as
    // extract/extract_calls; folding all three into one parse is a follow-up.
    fn extract_dataflow(&self, file: &str, content: &str) -> DataflowFacts {
        let Ok(parsed) = syn::parse_file(content) else {
            return DataflowFacts::default();
        };
        rust_dataflow_from(&parsed, file)
    }

    fn extract_bundle(&self, file: &str, content: &str, mask: AnalysisMask) -> AnalysisBundle {
        let Ok(parsed) = syn::parse_file(content) else {
            return AnalysisBundle::default();
        };
        let types = mask.types.then(|| {
            let mut entities = rust_entities_from(&parsed, file);
            let (const_entities, consts) = rust_const_values_from(&parsed, file);
            entities.extend(const_entities);
            TypeFacts {
                entities,
                edges: edges_from(&parsed),
                docs: rust_docs_from(&parsed, file),
                consts,
                ..Default::default()
            }
        });
        let calls = mask.calls.then(|| CallFacts {
            defs: rust_call_defs_from(&parsed, file),
            sites: rust_call_sites_from(&parsed, file),
        });
        let dataflow = mask.dataflow.then(|| rust_dataflow_from(&parsed, file));
        AnalysisBundle { types, calls, dataflow }
    }
}

impl TypeLang for KotlinTypes {
    fn name(&self) -> &'static str { "kotlin" }
    fn matches(&self, path: &str) -> bool { path.ends_with(".kt") || path.ends_with(".kts") }
    // One tree-sitter parse feeds both walks.
    fn extract(&self, file: &str, content: &str) -> TypeFacts {
        let mut parser = tree_sitter::Parser::new();
        let lang = tree_sitter::Language::new(tree_sitter_kotlin_sg::LANGUAGE);
        if parser.set_language(&lang).is_err() {
            return TypeFacts::default();
        }
        let Some(tree) = parser.parse(content, None) else {
            return TypeFacts::default();
        };
        let src = content.as_bytes();
        let root = tree.root_node();
        let mut entities = Vec::new();
        walk_kotlin_entities(root, src, file, &mut entities);
        let mut docs = Vec::new();
        walk_kotlin_docs(root, src, file, &mut docs);
        TypeFacts { entities, edges: kotlin_edges_from(root, src), docs, ..Default::default() }
    }
    // One tree-sitter parse feeds defs + sites, same shape as the Rust pass.
    fn extract_calls(&self, file: &str, content: &str) -> CallFacts {
        let mut parser = tree_sitter::Parser::new();
        let lang = tree_sitter::Language::new(tree_sitter_kotlin_sg::LANGUAGE);
        if parser.set_language(&lang).is_err() {
            return CallFacts::default();
        }
        let Some(tree) = parser.parse(content, None) else {
            return CallFacts::default();
        };
        let src = content.as_bytes();
        let root = tree.root_node();
        let mut defs = Vec::new();
        kt_walk_call_defs(root, src, file, None, &mut defs);
        let mut sites = Vec::new();
        kt_walk_call_sites(root, src, file, &mut sites);
        CallFacts { defs, sites }
    }
    fn extract_dataflow(&self, file: &str, content: &str) -> DataflowFacts {
        let mut parser = tree_sitter::Parser::new();
        let lang = tree_sitter::Language::new(tree_sitter_kotlin_sg::LANGUAGE);
        if parser.set_language(&lang).is_err() { return DataflowFacts::default(); }
        let Some(tree) = parser.parse(content, None) else { return DataflowFacts::default(); };
        kotlin_dataflow_from(tree.root_node(), content.as_bytes(), file)
    }
}

// --- Kotlin intra-procedural dataflow lift (tree-sitter). Same two-rule model
// as the Rust syn lift: value-bearing children flow into their parent, and a
// `val/var x = rhs` binds rhs -> x_slot with later reads flowing slot -> read.
// Node id is `file:row:col` from the tree-sitter start position (0-based). A
// `simple_identifier`'s role is decided by its parent: under variable_declaration
// it's a binding target, under parameter it's a param, under call_expression it's
// the callee (skipped), otherwise it's a var_read. Conservative on unsupported
// constructs: may miss flows, never invents.

fn kt_first_child<'a>(node: tree_sitter::Node<'a>, kind: &str) -> Option<tree_sitter::Node<'a>> {
    let mut cur = node.walk();
    let kids: Vec<tree_sitter::Node<'a>> = node.children(&mut cur).collect();
    kids.into_iter().find(|c| c.kind() == kind)
}

fn kotlin_dataflow_from(root: tree_sitter::Node, src: &[u8], file: &str) -> DataflowFacts {
    let mut out = DataflowFacts::default();
    kt_walk_fns(root, src, file, &mut out);
    // tree-sitter rows are 0-based; the df contract is 1-based (syn and the TS
    // line_at both emit 1-based), so a (file, line) join against call_site —
    // the call_node bridge every interprocedural hop rides — is a single
    // equality across languages. Nodes and loop spans bump together, so the
    // nest containment below stays internally consistent. Node IDS keep the
    // raw 0-based row (they are opaque; only uniqueness matters).
    for n in &mut out.nodes { n.line += 1; }
    for l in &mut out.loops { l.start += 1; l.end += 1; }
    out.nests = compute_nests(&out.nodes, &out.loops);
    out
}

fn kt_walk_fns(node: tree_sitter::Node, src: &[u8], file: &str, out: &mut DataflowFacts) {
    let mut cur = node.walk();
    for c in node.children(&mut cur) {
        if c.kind() == "function_declaration" {
            kt_flow_fn(c, src, file, out);
        }
        kt_walk_fns(c, src, file, out);
    }
}

fn kt_flow_fn(fn_node: tree_sitter::Node, src: &[u8], file: &str, out: &mut DataflowFacts) {
    let name = kt_first_child(fn_node, "simple_identifier")
        .map(|n| n.utf8_text(src).unwrap_or("").to_string())
        .unwrap_or_default();
    let fn_sym = mint_sym(file, EntityKind::Function, &name, None);
    let mut scope: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if let Some(params) = kt_first_child(fn_node, "function_value_parameters") {
        let mut cur = params.walk();
        for (pos, p) in params.children(&mut cur).filter(|n| n.kind() == "parameter").enumerate() {
            if let Some(idn) = kt_first_child(p, "simple_identifier") {
                let ppos = idn.start_position();
                let v = idn.utf8_text(src).unwrap_or("").to_string();
                let id = push_node(out, file, ppos.row as u32, ppos.column as u32, "param", &v, &fn_sym);
                out.param_pos.push((id.clone(), pos as u32));
                scope.insert(v, id);
            }
        }
    }
    if let Some(body) = kt_first_child(fn_node, "function_body") {
        // The body's tail value is the implicit return (block tail, or the
        // expression of `fun f() = expr`): flow it into the fn's `ret` node.
        // Explicit `return EXPR` is handled in the jump_expression arm.
        if let Some(tail) = flow_kt(body, src, file, &fn_sym, &mut scope, out) {
            let bpos = body.start_position();
            let ret = push_node(out, file, bpos.row as u32, bpos.column as u32, "ret", "", &fn_sym);
            out.edges.push(DfEdge { from: tail, to: ret });
        }
    }
}

/// Returns the node id carrying the value of this subtree, or None when the
/// subtree is not value-bearing (statements, wrappers, bindings handled inline).
fn flow_kt(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) -> Option<String> {
    let pos = node.start_position();
    match node.kind() {
        // a name in expression position is a read; role decided by parent.
        "simple_identifier" => {
            let parent_kind = node.parent().map(|p| p.kind());
            match parent_kind.as_deref() {
                Some("variable_declaration") | Some("parameter") | Some("call_expression") => None,
                _ => {
                    let v = node.utf8_text(src).unwrap_or("").to_string();
                    let id = push_node(out, file, pos.row as u32, pos.column as u32, "var_read", &v, fn_sym);
                    if let Some(b) = scope.get(&v) {
                        out.edges.push(DfEdge { from: b.clone(), to: id.clone() });
                    }
                    Some(id)
                }
            }
        }
        // f(args): every argument value flows into the call result, and
        // `df_arg` records its 0-based source position (named args keep their
        // source index — an approximation when Kotlin reorders them). A named
        // argument `f(x = v)` also lands in `df_field` under its name; the
        // name ident is a label, not a read, so it is never walked. A
        // navigation callee `recv.m(a)` flows the receiver in at slot -1; a
        // capitalized callee is a constructor call (Kotlin classes are
        // UpperCamelCase), minted as a `new` node carrying the type name.
        "call_expression" => {
            let callee = node.child(0);
            let mut recv: Option<String> = None;
            let mut callee_name = String::new();
            match callee.map(|c| c.kind()) {
                Some("simple_identifier") => {
                    callee_name = callee.unwrap().utf8_text(src).unwrap_or("").to_string();
                }
                Some("navigation_expression") => {
                    let nav = callee.unwrap();
                    if let Some(obj) = nav.child(0) {
                        recv = flow_kt(obj, src, file, fn_sym, scope, out);
                    }
                    if let Some(idn) = kt_first_child(nav, "navigation_suffix")
                        .and_then(|s| kt_first_child(s, "simple_identifier"))
                    {
                        callee_name = idn.utf8_text(src).unwrap_or("").to_string();
                    }
                }
                _ => {}
            }
            // (source position, named-arg name if any, value node id)
            let mut arg_ids: Vec<(Option<String>, String)> = Vec::new();
            if let Some(suffix) = kt_first_child(node, "call_suffix") {
                if let Some(vargs) = kt_first_child(suffix, "value_arguments") {
                    let mut cur = vargs.walk();
                    for va in vargs.children(&mut cur).filter(|n| n.kind() == "value_argument") {
                        // named form: value_argument = simple_identifier '=' expr
                        let mut kids = Vec::new();
                        let mut vc = va.walk();
                        for k in va.children(&mut vc) { kids.push(k); }
                        let eq_at = kids.iter().position(|k| k.kind() == "=");
                        let (name, val_node) = match eq_at {
                            Some(i) if i >= 1 && kids[i - 1].kind() == "simple_identifier" => {
                                (Some(kids[i - 1].utf8_text(src).unwrap_or("").to_string()),
                                 kids.get(i + 1).copied())
                            }
                            _ => (None, None),
                        };
                        let vid = match val_node {
                            Some(v) => flow_kt(v, src, file, fn_sym, scope, out),
                            None => flow_kt(va, src, file, fn_sym, scope, out),
                        };
                        if let Some(vid) = vid {
                            arg_ids.push((name, vid));
                        }
                    }
                }
                // A trailing lambda (`xs.map { it + 1 }`) is the call's last
                // positional argument; the lambda_literal arm lifts it and
                // returns its `closure` value node.
                if let Some(al) = kt_first_child(suffix, "annotated_lambda") {
                    if let Some(ll) = kt_first_child(al, "lambda_literal") {
                        if let Some(vid) = flow_kt(ll, src, file, fn_sym, scope, out) {
                            arg_ids.push((None, vid));
                        }
                    }
                }
            }
            let is_ctor = callee_name.chars().next().is_some_and(|c| c.is_uppercase());
            let (kind, var) = if is_ctor { ("new", callee_name.as_str()) } else { ("call_res", "") };
            let id = push_node(out, file, pos.row as u32, pos.column as u32, kind, var, fn_sym);
            if let Some(r) = recv {
                out.edges.push(DfEdge { from: r.clone(), to: id.clone() });
                out.args.push((id.clone(), -1, r));
            }
            for (p, (name, vid)) in arg_ids.into_iter().enumerate() {
                out.edges.push(DfEdge { from: vid.clone(), to: id.clone() });
                out.args.push((id.clone(), p as i64, vid.clone()));
                if let Some(n) = name {
                    out.fields.push((id.clone(), n, vid));
                }
            }
            Some(id)
        }
        // `base.f` outside a call: a member read. The base flows into a
        // `member` node whose var is the accessed name, so a `df_field` write
        // can be matched against the read of the same field. As a call's
        // callee (parent == call_expression) the call arm owns it instead —
        // receiver at slot -1, name on the call node.
        "navigation_expression" => {
            if node.parent().map(|p| p.kind()) == Some("call_expression") {
                return None;
            }
            let obj = node.child(0).and_then(|c| flow_kt(c, src, file, fn_sym, scope, out));
            let name = kt_first_child(node, "navigation_suffix")
                .and_then(|s| kt_first_child(s, "simple_identifier"))
                .map(|n| n.utf8_text(src).unwrap_or("").to_string())
                .unwrap_or_default();
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "member", &name, fn_sym);
            if let Some(o) = obj {
                out.edges.push(DfEdge { from: o, to: id.clone() });
            }
            Some(id)
        }
        // val/var x = rhs: mint the binding slot, flow rhs -> slot, register.
        "property_declaration" => {
            let mut bind: Option<(String, String)> = None;
            let mut rhs_id: Option<String> = None;
            let mut cur = node.walk();
            for c in node.children(&mut cur) {
                match c.kind() {
                    "variable_declaration" => {
                        if let Some(si) = kt_first_child(c, "simple_identifier") {
                            let sp = si.start_position();
                            let v = si.utf8_text(src).unwrap_or("").to_string();
                            let id = push_node(out, file, sp.row as u32, sp.column as u32, "let_bind", &v, fn_sym);
                            bind = Some((v, id));
                        }
                    }
                    "=" | "binding_pattern_kind" | "val" | "var" => {}
                    _ => {
                        if let Some(id) = flow_kt(c, src, file, fn_sym, scope, out) {
                            rhs_id = Some(id);
                        }
                    }
                }
            }
            if let (Some((v, bid)), Some(rhs)) = (bind, rhs_id) {
                out.edges.push(DfEdge { from: rhs, to: bid.clone() });
                scope.insert(v, bid);
            }
            None
        }
        // wrappers / statements: flow the last value-bearing child through.
        "value_argument" | "statements" | "function_body" | "source_file" => {
            let mut last = None;
            let mut cur = node.walk();
            for c in node.children(&mut cur) {
                if let Some(id) = flow_kt(c, src, file, fn_sym, scope, out) {
                    last = Some(id);
                }
            }
            last
        }
        // `{ x -> body }` / `{ it + 1 }`: lift the lambda as its OWN fn scope —
        // "param" nodes with df_param slots (the implicit `it` when no
        // parameter list is declared), body walked under the lambda sym, tail
        // value into a "ret" node — and mint the `closure` VALUE node in the
        // enclosing fn, carrying the lambda sym in `var` (the join key a
        // higher-order hop uses; see std/flow.dl flow_lambda). The enclosing
        // scope is shared, so captures still resolve.
        "lambda_literal" => {
            let lam_sym = format!("{fn_sym}::closure::{}_{}", pos.row, pos.column);
            let mut seeded = false;
            if let Some(lp) = kt_first_child(node, "lambda_parameters") {
                let mut cur = lp.walk();
                for (i, vd) in lp.children(&mut cur).filter(|n| n.kind() == "variable_declaration").enumerate() {
                    if let Some(idn) = kt_first_child(vd, "simple_identifier") {
                        let ppos = idn.start_position();
                        let v = idn.utf8_text(src).unwrap_or("").to_string();
                        let id = push_node(out, file, ppos.row as u32, ppos.column as u32, "param", &v, &lam_sym);
                        out.param_pos.push((id.clone(), i as u32));
                        scope.insert(v, id);
                        seeded = true;
                    }
                }
            }
            if !seeded {
                // No declared parameter list: Kotlin's implicit `it`, slot 0.
                let id = push_node(out, file, pos.row as u32, pos.column as u32, "param", "it", &lam_sym);
                out.param_pos.push((id.clone(), 0));
                scope.insert("it".into(), id);
            }
            let tail = kt_first_child(node, "statements")
                .and_then(|s| flow_kt(s, src, file, &lam_sym, scope, out));
            if let Some(t) = tail {
                let end = node.end_position();
                let ret = push_node(out, file, end.row as u32, end.column as u32, "ret", "", &lam_sym);
                out.edges.push(DfEdge { from: t, to: ret });
            }
            Some(push_node(out, file, pos.row as u32, pos.column as u32, "closure", &lam_sym, fn_sym))
        }
        // return EXPR: the returned value flows into the fn's `ret` node — the
        // sink the interprocedural backward hop reads.
        "jump_expression" => {
            let mut inner = None;
            let mut cur = node.walk();
            for c in node.children(&mut cur) {
                if c.kind() != "return" {
                    if let Some(id) = flow_kt(c, src, file, fn_sym, scope, out) {
                        inner = Some(id);
                    }
                }
            }
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "ret", "", fn_sym);
            if let Some(v) = inner { out.edges.push(DfEdge { from: v, to: id.clone() }); }
            Some(id)
        }
        // a OP b: both operands taint the result. This is the taint-vs-dataflow
        // distinction in one arm — exact dataflow would say `a + 1` is not `a`,
        // taint propagates `a` through the operation into the result. Kotlin
        // splits operators across additive/multiplicative/infix expression kinds
        // (no named fields), so take the first and last named children as the
        // two operands and skip the anonymous operator token between them.
        "additive_expression" | "multiplicative_expression" | "infix_expression" => {
            let mut cur = node.walk();
            let kids: Vec<tree_sitter::Node> = node.named_children(&mut cur).collect();
            let l = kids.first().and_then(|n| flow_kt(*n, src, file, fn_sym, scope, out));
            let r = kids.last().and_then(|n| flow_kt(*n, src, file, fn_sym, scope, out));
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "binop", "", fn_sym);
            if let Some(lid) = l { out.edges.push(DfEdge { from: lid, to: id.clone() }); }
            if let Some(rid) = r { out.edges.push(DfEdge { from: rid, to: id.clone() }); }
            Some(id)
        }
        "string_literal" | "integer_literal" | "real_literal" | "boolean_literal" | "character_literal" | "long_literal" => {
            Some(push_node(out, file, pos.row as u32, pos.column as u32, "lit", "", fn_sym))
        }
        // `for (x in coll) body`: record the span + loop var so loop_over can flag
        // loop-invariant calls inside the body. The body is then walked by the
        // conservative recursion below (Kotlin has no named fields on for_statement).
        "for_statement" => {
            let lvar = {
                let mut cur = node.walk();
                let kids: Vec<tree_sitter::Node> = node.named_children(&mut cur).collect();
                kids.iter().find(|c| c.kind() == "simple_identifier")
                    .map(|n| n.utf8_text(src).unwrap_or("").to_string())
                    .unwrap_or_default()
            };
            let end = node.end_position();
            out.loops.push(LoopFact {
                file: file.into(), start: pos.row as u32, end: end.row as u32,
                var: lvar, collection: String::new(), fn_sym: fn_sym.into(),
            });
            kt_recurse_children(node, src, file, fn_sym, scope, out)
        }
        "while_statement" | "do_while_statement" => {
            let end = node.end_position();
            out.loops.push(LoopFact {
                file: file.into(), start: pos.row as u32, end: end.row as u32,
                var: String::new(), collection: String::new(), fn_sym: fn_sym.into(),
            });
            kt_recurse_children(node, src, file, fn_sym, scope, out)
        }
        // anything else (when-arms, lambda bodies, etc.): recurse conservatively,
        // surface the last value if any. May miss, never invents.
        _ => kt_recurse_children(node, src, file, fn_sym, scope, out),
    }
}

/// Walk all children of a node conservatively, surfacing the last value-bearing
/// child's id. Factored out of the flow_kt default arm so loop arms reuse it.
fn kt_recurse_children(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) -> Option<String> {
    let mut last = None;
    let mut cur = node.walk();
    for c in node.children(&mut cur) {
        if let Some(id) = flow_kt(c, src, file, fn_sym, scope, out) {
            last = Some(id);
        }
    }
    last
}

// --- TypeScript/JavaScript intra-procedural dataflow lift (oxc). Same two-rule
// model: value-bearing children flow into their parent, and `const/let/var x =
// rhs` binds rhs -> x_slot with later reads flowing slot -> read. Node id is
// `file:<byte_off>` (oxc's native byte-offset span); `line_at` recovers the
// 1-based line for the `line` column. Conservative on unsupported constructs.

fn ts_push(out: &mut DataflowFacts, file: &str, starts: &[usize], byte_off: u32, kind: &str, var: &str, fn_sym: &str) -> String {
    // kind suffix disambiguates a parent from its first child where spans share
    // a start byte (see push_node); byte_off alone is not unique for `a + 1`.
    let id = format!("{file}:{byte_off}:{kind}");
    let line = line_at(starts, byte_off as usize);
    out.nodes.push(DfNode {
        id: id.clone(),
        kind: kind.into(),
        var: var.into(),
        fn_sym: fn_sym.into(),
        file: file.into(),
        line,
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

fn ts_dataflow_from(program: &ts_ast::Program, file: &str, content: &str) -> DataflowFacts {
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
        _ => {}
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
            let lam_sym = format!("{fn_sym}::closure::{off}");
            ts_lift_fn(&a.params, &a.body, a.expression, &lam_sym, file, starts, out);
            ts_push(out, file, starts, off, "closure", &lam_sym, fn_sym)
        }
        E::FunctionExpression(f) => match f.body.as_deref() {
            Some(body) => {
                let lam_sym = format!("{fn_sym}::closure::{off}");
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
fn span_off(e: &ts_ast::Expression) -> u32 {
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

impl TypeLang for TsTypes {
    fn name(&self) -> &'static str { "ts" }
    // Plain JS rides the same oxc front-end as TS: `.js`/`.jsx`/`.mjs`/`.cjs`
    // parse fine as JSX-enabled JavaScript, so type_entity/call_*/df_*/
    // doc_comment all populate for JS too (type_link/type_sig stay thin, a
    // JS file carries no type annotations to resolve). Nothing else in the
    // `type_langs()` registry claims these extensions.
    fn matches(&self, path: &str) -> bool {
        path.ends_with(".ts") || path.ends_with(".tsx")
            || path.ends_with(".js") || path.ends_with(".jsx")
            || path.ends_with(".mjs") || path.ends_with(".cjs")
            || path.ends_with(".mts") || path.ends_with(".cts")
    }
    // One oxc parse feeds both walks.
    fn extract(&self, file: &str, content: &str) -> TypeFacts {
        let alloc = oxc_allocator::Allocator::default();
        let st = source_type_for(file);
        let ret = oxc_parser::Parser::new(&alloc, content, st).parse();
        if ret.panicked {
            return TypeFacts::default();
        }
        let mut entities = ts_entities_from(&ret.program, file, content);
        let (const_entities, consts, const_spread_skips, const_mutable_skips) =
            ts_const_facts_from(&ret.program, file, content);
        entities.extend(const_entities);
        TypeFacts {
            entities,
            edges: ts_edges_from(&ret.program),
            docs: ts_docs_from(&ret.program, file, content),
            consts,
            const_spread_skips,
            const_mutable_skips,
        }
    }
    // One oxc parse feeds defs + sites, same shape as the Rust pass. `line_at`
    // recovers 1-based lines from oxc's byte-offset spans.
    fn extract_calls(&self, file: &str, content: &str) -> CallFacts {
        let alloc = oxc_allocator::Allocator::default();
        let st = source_type_for(file);
        let ret = oxc_parser::Parser::new(&alloc, content, st).parse();
        if ret.panicked {
            return CallFacts::default();
        }
        let starts = line_index(content);
        let defs = ts_call_defs_from(&ret.program, file, &starts);
        let mut sites = TsCallSites { file, starts: &starts, sites: Vec::new() };
        sites.visit_program(&ret.program);
        CallFacts { defs, sites: sites.sites }
    }
    // One oxc parse feeds the node + edge lift. Byte-offset spans (oxc's native
    // shape) become node ids `file:<byte_off>`; `line_at` recovers the 1-based
    // line for the `line` column.
    fn extract_dataflow(&self, file: &str, content: &str) -> DataflowFacts {
        let alloc = oxc_allocator::Allocator::default();
        let st = source_type_for(file);
        let ret = oxc_parser::Parser::new(&alloc, content, st).parse();
        if ret.panicked {
            return DataflowFacts::default();
        }
        ts_dataflow_from(&ret.program, file, content)
    }
}

/// Extension-based oxc `SourceType` for every path `TsTypes::matches` claims.
/// `.tsx` is TypeScript+JSX, `.ts` is plain TypeScript, and the JS extensions
/// (`.jsx`/`.js`/`.mjs`/`.cjs`) all parse as JSX-enabled JavaScript modules.
/// JSX shows up in bare `.js`/`.mjs` React code often enough that allowing it
/// unconditionally costs nothing for files that never use it.
fn source_type_for(file: &str) -> oxc_span::SourceType {
    if file.ends_with(".tsx") {
        oxc_span::SourceType::tsx()
    } else if file.ends_with(".ts") {
        oxc_span::SourceType::ts()
    } else {
        oxc_span::SourceType::jsx()
    }
}

/// Map a byte offset to a 1-based line number. Built once per file by the oxc
/// entity pass (oxc spans are byte offsets, unlike syn's line/col).
fn line_index(content: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, b) in content.bytes().enumerate() {
        if b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

fn line_at(starts: &[usize], offset: usize) -> u32 {
    match starts.binary_search(&offset) {
        Ok(i) => (i + 1) as u32,
        Err(i) => i as u32, // i = count of starts <= offset
    }
}

/// Map a byte offset to (1-based line, 0-based byte column within the line).
/// Same line base as `line_at`; the column is the byte distance from the line's
/// start offset, matching the `sg`/`diag` "1-based line, 0-based byte col"
/// convention the `comment_node` rel follows.
fn line_col(starts: &[usize], offset: usize) -> (u32, u32) {
    let line = line_at(starts, offset).max(1);
    let line_start = starts[(line - 1) as usize];
    (line, (offset.saturating_sub(line_start)) as u32)
}

pub fn edges(content: &str) -> Vec<TypeEdge> {
    let Ok(file) = syn::parse_file(content) else {
        return Vec::new();
    };
    edges_from(&file)
}

fn edges_from(file: &syn::File) -> Vec<TypeEdge> {
    let mut out: BTreeSet<(String, String, &'static str)> = BTreeSet::new();
    for item in &file.items {
        item_edges(item, &mut out);
    }
    out.into_iter()
        .map(|(from, to, kind)| TypeEdge { from, to, kind })
        .collect()
}

fn item_edges(item: &Item, out: &mut BTreeSet<(String, String, &'static str)>) {
    match item {
        Item::Struct(s) => {
            let owner = s.ident.to_string();
            generic_edges(&owner, &s.generics, out);
            field_edges(&owner, &s.fields, out);
        }
        Item::Enum(e) => {
            let owner = e.ident.to_string();
            generic_edges(&owner, &e.generics, out);
            for v in &e.variants {
                let variant = format!("{owner}::{}", v.ident);
                push(out, &owner, &variant, "variant");
                field_edges(&variant, &v.fields, out);
            }
        }
        Item::Union(u) => {
            let owner = u.ident.to_string();
            generic_edges(&owner, &u.generics, out);
            field_edges(&owner, &Fields::Named(u.fields.clone()), out);
        }
        Item::Trait(t) => {
            let owner = t.ident.to_string();
            generic_edges(&owner, &t.generics, out);
            for bound in &t.supertraits {
                bound_edge(&owner, bound, "generic", out);
            }
        }
        Item::Impl(i) => {
            let Some(owner) = primary_type(&i.self_ty) else {
                return;
            };
            generic_edges(&owner, &i.generics, out);
            if let Some((_, path, _)) = &i.trait_ {
                if let Some(to) = path_name(path) {
                    push(out, &owner, &to, "impl");
                }
            }
        }
        _ => {}
    }
}

fn field_edges(from: &str, fields: &Fields, out: &mut BTreeSet<(String, String, &'static str)>) {
    for field in fields.iter() {
        for to in type_refs(&field.ty) {
            push(out, from, &to, "field");
        }
    }
}

fn generic_edges(
    from: &str,
    generics: &Generics,
    out: &mut BTreeSet<(String, String, &'static str)>,
) {
    for param in &generics.params {
        if let GenericParam::Type(t) = param {
            for bound in &t.bounds {
                bound_edge(from, bound, "generic", out);
            }
        }
    }
    if let Some(where_clause) = &generics.where_clause {
        for pred in &where_clause.predicates {
            if let WherePredicate::Type(t) = pred {
                for bound in &t.bounds {
                    bound_edge(from, bound, "generic", out);
                }
            }
        }
    }
}

fn bound_edge(
    from: &str,
    bound: &TypeParamBound,
    kind: &'static str,
    out: &mut BTreeSet<(String, String, &'static str)>,
) {
    if let TypeParamBound::Trait(t) = bound {
        if let Some(to) = path_name(&t.path) {
            push(out, from, &to, kind);
        }
    }
}

fn type_refs(ty: &Type) -> Vec<String> {
    let mut out = Vec::new();
    collect_type_refs(ty, &mut out);
    out.sort();
    out.dedup();
    out
}

fn collect_type_refs(ty: &Type, out: &mut Vec<String>) {
    match ty {
        Type::Array(t) => collect_type_refs(&t.elem, out),
        Type::BareFn(t) => {
            for input in &t.inputs {
                collect_type_refs(&input.ty, out);
            }
            if let ReturnType::Type(_, ty) = &t.output {
                collect_type_refs(ty, out);
            }
        }
        Type::Group(t) => collect_type_refs(&t.elem, out),
        Type::ImplTrait(t) => {
            for bound in &t.bounds {
                collect_bound_ref(bound, out);
            }
        }
        Type::Paren(t) => collect_type_refs(&t.elem, out),
        Type::Path(t) => {
            if let Some(qself) = &t.qself {
                collect_type_refs(&qself.ty, out);
            }
            if let Some(name) = path_name(&t.path) {
                out.push(name);
            }
            collect_path_args(&t.path, out);
        }
        Type::Ptr(t) => collect_type_refs(&t.elem, out),
        Type::Reference(t) => collect_type_refs(&t.elem, out),
        Type::Slice(t) => collect_type_refs(&t.elem, out),
        Type::TraitObject(t) => {
            for bound in &t.bounds {
                collect_bound_ref(bound, out);
            }
        }
        Type::Tuple(t) => {
            for elem in &t.elems {
                collect_type_refs(elem, out);
            }
        }
        _ => {}
    }
}

fn collect_bound_ref(bound: &TypeParamBound, out: &mut Vec<String>) {
    if let TypeParamBound::Trait(t) = bound {
        if let Some(name) = path_name(&t.path) {
            out.push(name);
        }
        collect_path_args(&t.path, out);
    }
}

fn collect_path_args(path: &Path, out: &mut Vec<String>) {
    for seg in &path.segments {
        match &seg.arguments {
            PathArguments::AngleBracketed(AngleBracketedGenericArguments { args, .. }) => {
                for arg in args {
                    match arg {
                        GenericArgument::Type(t) => collect_type_refs(t, out),
                        GenericArgument::AssocType(t) => collect_type_refs(&t.ty, out),
                        GenericArgument::Constraint(c) => {
                            for bound in &c.bounds {
                                collect_bound_ref(bound, out);
                            }
                        }
                        _ => {}
                    }
                }
            }
            PathArguments::Parenthesized(p) => {
                for input in &p.inputs {
                    collect_type_refs(input, out);
                }
                if let ReturnType::Type(_, ty) = &p.output {
                    collect_type_refs(ty, out);
                }
            }
            PathArguments::None => {}
        }
    }
}

fn primary_type(ty: &Type) -> Option<String> {
    match ty {
        Type::Group(t) => primary_type(&t.elem),
        Type::Paren(t) => primary_type(&t.elem),
        Type::Path(t) => path_name(&t.path),
        Type::Ptr(t) => primary_type(&t.elem),
        Type::Reference(t) => primary_type(&t.elem),
        _ => None,
    }
}

fn path_name(path: &Path) -> Option<String> {
    let parts: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    if parts.is_empty() {
        return None;
    }
    let name = parts.join("::");
    if is_noise_type(&name) {
        None
    } else {
        Some(name)
    }
}

fn push(
    out: &mut BTreeSet<(String, String, &'static str)>,
    from: &str,
    to: &str,
    kind: &'static str,
) {
    if from == to || is_noise_type(to) {
        return;
    }
    out.insert((from.to_string(), to.to_string(), kind));
}

fn is_noise_type(name: &str) -> bool {
    matches!(
        name,
        "Self"
            | "bool"
            | "char"
            | "str"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "f32"
            | "f64"
    )
}

// ── Kotlin ──────────────────────────────────────────────────────────────────
//
// The tree-sitter-kotlin grammar folds `interface` into `class_declaration`
// (and `enum class` is a modifier + `enum_class_body`), so decl flavor comes
// from keyword-level token inspection, not the node kind. Edge mapping mirrors
// Rust: an interface's supertypes are "generic" (trait supertraits), a
// class/object's supertypes are "impl", val/var constructor params and body
// properties are "field", enum entries are "variant". Declared type-parameter
// names are excluded from refs (`Repo<T>(x: T)` has no edge to T), unlike the
// syn extractor which leaks them — tree-sitter hands us the param list cheap.

pub fn kotlin_edges(content: &str) -> Vec<TypeEdge> {
    let mut parser = tree_sitter::Parser::new();
    let lang = tree_sitter::Language::new(tree_sitter_kotlin_sg::LANGUAGE);
    if parser.set_language(&lang).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(content, None) else {
        return Vec::new();
    };
    kotlin_edges_from(tree.root_node(), content.as_bytes())
}

fn kotlin_edges_from(root: tree_sitter::Node, src: &[u8]) -> Vec<TypeEdge> {
    let mut out: BTreeSet<(String, String, &'static str)> = BTreeSet::new();
    walk_kotlin(root, src, &mut out);
    out.into_iter()
        .map(|(from, to, kind)| TypeEdge { from, to, kind })
        .collect()
}

fn walk_kotlin(node: tree_sitter::Node, src: &[u8], out: &mut BTreeSet<(String, String, &'static str)>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(child.kind(), "class_declaration" | "object_declaration") {
            kotlin_decl_edges(child, src, out);
        }
        walk_kotlin(child, src, out);
    }
}

fn kotlin_decl_edges(decl: tree_sitter::Node, src: &[u8], out: &mut BTreeSet<(String, String, &'static str)>) {
    let text = |n: tree_sitter::Node| n.utf8_text(src).unwrap_or("").to_string();
    let mut cursor = decl.walk();
    let children: Vec<tree_sitter::Node> = decl.children(&mut cursor).collect();

    let Some(owner) = children.iter().find(|n| n.kind() == "type_identifier").map(|n| text(*n)) else {
        return;
    };
    // keyword-level split: `interface` is an anonymous token under the same
    // class_declaration node kind as `class`
    let is_interface = children.iter().any(|n| n.kind() == "interface");
    let super_kind: &'static str = if is_interface { "generic" } else { "impl" };

    // declared type-parameter names; their bounds are "generic" edges and the
    // names themselves are not type refs
    let mut params: BTreeSet<String> = BTreeSet::new();
    for n in &children {
        if n.kind() != "type_parameters" { continue; }
        let mut c = n.walk();
        for tp in n.children(&mut c).filter(|n| n.kind() == "type_parameter") {
            let mut cc = tp.walk();
            let kids: Vec<tree_sitter::Node> = tp.children(&mut cc).collect();
            if let Some(name) = kids.iter().find(|n| n.kind() == "type_identifier") {
                params.insert(text(*name));
            }
            for bound in kids.iter().filter(|n| n.kind() != "type_identifier") {
                for to in kotlin_type_refs(*bound, src, &params) {
                    push(out, &owner, &to, "generic");
                }
            }
        }
    }

    for n in &children {
        match n.kind() {
            "delegation_specifier" => {
                // constructor_invocation = superclass call, bare user_type =
                // interface; both are supertypes, kind set by the owner flavor
                for to in kotlin_type_refs(*n, src, &params) {
                    push(out, &owner, &to, super_kind);
                }
            }
            "primary_constructor" => {
                let mut c = n.walk();
                for param in n.children(&mut c).filter(|n| n.kind() == "class_parameter") {
                    let mut cc = param.walk();
                    let kids: Vec<tree_sitter::Node> = param.children(&mut cc).collect();
                    // val/var (binding_pattern_kind) makes it a field; a bare
                    // constructor arg is not part of the type's shape
                    if !kids.iter().any(|n| n.kind() == "binding_pattern_kind") { continue; }
                    for kid in kids.iter().filter(|n| n.kind() != "simple_identifier") {
                        for to in kotlin_type_refs(*kid, src, &params) {
                            push(out, &owner, &to, "field");
                        }
                    }
                }
            }
            "class_body" => {
                let mut c = n.walk();
                for prop in n.children(&mut c).filter(|n| n.kind() == "property_declaration") {
                    let mut cc = prop.walk();
                    for vd in prop.children(&mut cc).filter(|n| n.kind() == "variable_declaration") {
                        for to in kotlin_type_refs(vd, src, &params) {
                            push(out, &owner, &to, "field");
                        }
                    }
                }
            }
            "enum_class_body" => {
                let mut c = n.walk();
                for entry in n.children(&mut c).filter(|n| n.kind() == "enum_entry") {
                    let mut cc = entry.walk();
                    let name = entry.children(&mut cc).find(|n| n.kind() == "simple_identifier");
                    if let Some(name) = name {
                        let variant = format!("{owner}::{}", text(name));
                        push(out, &owner, &variant, "variant");
                    }
                }
            }
            _ => {}
        }
    }
}

/// Collect type names referenced anywhere under `node`: each `user_type`'s own
/// dotted path is one ref, its `type_arguments` recurse into more refs.
fn kotlin_type_refs(node: tree_sitter::Node, src: &[u8], params: &BTreeSet<String>) -> Vec<String> {
    let mut out = Vec::new();
    collect_kotlin_refs(node, src, params, &mut out);
    out.sort();
    out.dedup();
    out
}

fn collect_kotlin_refs(node: tree_sitter::Node, src: &[u8], params: &BTreeSet<String>, out: &mut Vec<String>) {
    if node.kind() == "user_type" {
        let mut cursor = node.walk();
        let segs: Vec<String> = node.children(&mut cursor)
            .filter(|n| n.kind() == "type_identifier")
            .map(|n| n.utf8_text(src).unwrap_or("").to_string())
            .collect();
        let name = segs.join(".");
        if !name.is_empty() && !params.contains(&name) && !is_noise_kotlin(&name) {
            out.push(name);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor).filter(|n| n.kind() != "type_identifier") {
            collect_kotlin_refs(child, src, params, out);
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_kotlin_refs(child, src, params, out);
    }
}

fn is_noise_kotlin(name: &str) -> bool {
    matches!(
        name,
        "Int" | "Long" | "Short" | "Byte" | "Float" | "Double" | "Boolean" | "Char"
            | "String" | "Unit" | "Any" | "Nothing"
    )
}

// --- Kotlin call-graph pass (tree-sitter): `function_declaration` nodes become
// CallDefs (a fn inside a class/object/interface body is a Method keyed to the
// enclosing type, a top-level fun is Free), and every `call_expression` becomes
// a CallSite whose callee is the called name as written. Caller resolution is
// the engine's span-containment pass; mirror the Rust convention (bare callee
// name, body span end line for containment). ---

/// Walk for `function_declaration` defs, tracking the enclosing type name so a
/// member fn keys to its owner. Descending into a class/object body carries the
/// owner; descending into a fn body resets to None (a local fun is not a method
/// of the surrounding type).
fn kt_walk_call_defs(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    parent: Option<&str>,
    out: &mut Vec<CallDef>,
) {
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        match child.kind() {
            "class_declaration" | "object_declaration" => {
                let owner = kt_first_child(child, "type_identifier")
                    .map(|n| n.utf8_text(src).unwrap_or("").to_string());
                kt_walk_call_defs(child, src, file, owner.as_deref(), out);
            }
            "function_declaration" => {
                let name = kt_first_child(child, "simple_identifier")
                    .map(|n| n.utf8_text(src).unwrap_or("").to_string())
                    .unwrap_or_default();
                let (kind, ekind) = match parent {
                    Some(_) => (CallKind::Method, EntityKind::Method),
                    None => (CallKind::Free, EntityKind::Function),
                };
                // body span end (1-based) bounds the def for callsite containment;
                // abstract/interface fns have no body, so fall back to the decl end.
                let end = kt_first_child(child, "function_body")
                    .unwrap_or(child)
                    .end_position()
                    .row as u32
                    + 1;
                out.push(CallDef {
                    sym: mint_sym(file, ekind, &name, parent),
                    name,
                    kind,
                    file: file.to_string(),
                    line: child.start_position().row as u32 + 1,
                    end,
                });
                // a nested local fun is Free w.r.t. the enclosing scope.
                kt_walk_call_defs(child, src, file, None, out);
            }
            _ => kt_walk_call_defs(child, src, file, parent, out),
        }
    }
}

/// Walk for `call_expression` sites. The callee is the call's leading child: a
/// bare `simple_identifier`, or the trailing `simple_identifier` of a
/// `navigation_expression` (`recv.qux()` -> "qux"), matching the Rust trailing-
/// segment convention.
fn kt_walk_call_sites(node: tree_sitter::Node, src: &[u8], file: &str, out: &mut Vec<CallSite>) {
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        if child.kind() == "call_expression" {
            if let Some((callee, line)) = kt_callee(child, src) {
                out.push(CallSite { caller_sym: None, callee, callee_path: None, file: file.to_string(), line });
            }
        }
        kt_walk_call_sites(child, src, file, out);
    }
}

/// (callee name, 1-based call line) for a `call_expression`, or None when the
/// callee is not a plain/navigation name (e.g. an invoked lambda value).
fn kt_callee(call: tree_sitter::Node, src: &[u8]) -> Option<(String, u32)> {
    let mut cur = call.walk();
    let lead = call.children(&mut cur).find(|c| c.kind() != "call_suffix")?;
    let line = lead.start_position().row as u32 + 1;
    match lead.kind() {
        "simple_identifier" => Some((lead.utf8_text(src).unwrap_or("").to_string(), line)),
        "navigation_expression" => {
            let nav = kt_first_child(lead, "navigation_suffix")?;
            let id = kt_first_child(nav, "simple_identifier")?;
            Some((id.utf8_text(src).unwrap_or("").to_string(), line))
        }
        _ => None,
    }
}


// ── TypeScript / JavaScript (oxc) ───────────────────────────────────────────
//
// Same diet-extractor contract as the syn and tree-sitter passes: parse one
// file, walk declaration shapes, emit edges in the shared kind vocabulary.
// Mapping: an interface's `extends` are "generic" (trait supertraits), a
// class's `extends`/`implements` are "impl", property/parameter-property
// types are "field", enum members are `Owner::Name` "variant" rows, and a
// union type alias's referenced alternatives are "variant" (a sum type).
// Declared type-parameter names are excluded from refs, like Kotlin.
// Method signatures/bodies are skipped everywhere — shape only.
// Top-level + exported declarations only (namespaces wait on demand).

use oxc_ast::ast as ts_ast;
use oxc_ast_visit::Visit as OxcVisit;

pub fn ts_edges(content: &str, tsx: bool) -> Vec<TypeEdge> {
    let alloc = oxc_allocator::Allocator::default();
    let st = if tsx { oxc_span::SourceType::tsx() } else { oxc_span::SourceType::ts() };
    let ret = oxc_parser::Parser::new(&alloc, content, st).parse();
    if ret.panicked {
        return Vec::new();
    }
    ts_edges_from(&ret.program)
}

/// Every comment in a TS/TSX file, grammar-backed by oxc's comment table
/// (`program.comments`). TS/TSX is NOT in the tree-sitter `AST_LANG_TABLE`
/// (oxc is the front-end), so the generic `cst::walk_comments` can't see it —
/// this is the TS arm of `comment_node`. oxc's `Comment.span` covers the FULL
/// comment INCLUDING delimiters (`//`, `/* */`), which is exactly the raw span
/// `comment_node` records; a `//` inside a string is a token, never a comment
/// row, because the lexer produced these (string-literal safety, the whole
/// point). Byte offsets are mapped to 1-based line / 0-based column via a line
/// index, matching the tree-sitter arm and the `sg`/`diag` convention.
pub fn ts_comments(content: &str, tsx: bool) -> Vec<crate::cst::RawComment> {
    let alloc = oxc_allocator::Allocator::default();
    let st = if tsx { oxc_span::SourceType::tsx() } else { oxc_span::SourceType::ts() };
    let ret = oxc_parser::Parser::new(&alloc, content, st).parse();
    // oxc still populates the comment table on a partial parse; `panicked` only
    // means the AST is incomplete, so comments are usable regardless.
    let idx = line_index(content);
    ret.program.comments.iter().filter_map(|c| {
        let (lo, hi) = (c.span.start as usize, c.span.end as usize);
        let raw = content.get(lo..hi)?.to_string();
        let (sl, sc) = line_col(&idx, lo);
        let (el, ec) = line_col(&idx, hi);
        Some(crate::cst::RawComment { start_row: sl, start_col: sc, end_row: el, end_col: ec, raw })
    }).collect()
}

/// One piece of a template literal, in source order: `` `GET /users/${id}` ``
/// splits into `[(static, "GET /users/"), (expr, "id")]`. `node` is the
/// `df_node`/`df_lit` id the DATAFLOW lift mints for the SAME occurrence —
/// `{file}:{anchor}:template` (`ts_push`'s exact `{file}:{byte_off}:{kind}`
/// scheme, `typegraph.rs`'s `fn ts_push`) — so a consumer joins a piece
/// straight to `df_lit`/`df_node`/`df_edge` with no extra id math: a
/// template's static chunk row (`kind = "static"`) joins `df_lit.id` (the
/// same template's raw-source `df_lit` row), and `node` joins `df_edge`'s
/// `to` column for whatever flows INTO the template (an interpolated var's
/// `var_read` node has its own edge `to = node`). `anchor` is the plain
/// template literal's own span start (the opening backtick); for a TAGGED
/// template it is the `TaggedTemplateExpression`'s own span start (the tag's
/// position, NOT the quasi's) — `ts_flow_expr`'s `off = span_off(e)` mints
/// the df id off the OUTER expression node for a tagged template, so
/// `template_parts` anchors there too rather than at the quasi (the two
/// walks would otherwise disagree on `node` for every tagged template in the
/// corpus). Shared by every piece of the SAME occurrence so a consumer
/// groups pieces by `node` and orders them by `idx`; stable across ticks for
/// unchanged content since it is derived from the byte content itself, not a
/// counter. `line` is the template literal's own 1-based start line (the
/// `comment_node`/`sg`/`diag` convention: 1-based line, byte offsets for
/// everything finer-grained).
///
/// A nested template literal (an interpolation whose value is itself a
/// template, e.g. `` `outer ${`inner ${x}`}` ``) mints its OWN independent
/// node/idx sequence — the outer occurrence's `expr` piece for that slot
/// still carries the nested template's full verbatim source text (backticks
/// included), the same treatment any other expression gets.
#[derive(Clone, Debug)]
pub struct TemplatePart {
    pub node: String,
    pub line: u32,
    pub idx: u32,
    pub kind: &'static str,
    pub text: String,
}

/// Every template-literal piece in a TS/TSX/JS/JSX file (`template_parts`'
/// TS-family extractor; the walk needs the byte offsets `line_index` and a
/// content slice, not `program.comments`, so it takes `file`/`content`
/// directly rather than sharing `ts_comments`' `tsx: bool` shape). Dispatch by
/// extension via `source_type_for`, matching `TsTypes::extract`'s file set
/// (`.ts`/`.tsx`/`.js`/`.jsx`/`.mjs`/`.cjs`).
pub fn ts_template_parts(file: &str, content: &str) -> Vec<TemplatePart> {
    let alloc = oxc_allocator::Allocator::default();
    let st = source_type_for(file);
    let ret = oxc_parser::Parser::new(&alloc, content, st).parse();
    if ret.panicked {
        return Vec::new();
    }
    let starts = line_index(content);
    let mut walker = TsTemplateWalker { file, content, starts: &starts, out: Vec::new(), tag_anchor: None };
    walker.visit_program(&ret.program);
    walker.out
}

struct TsTemplateWalker<'s> {
    file: &'s str,
    content: &'s str,
    starts: &'s [usize],
    out: Vec<TemplatePart>,
    /// Set by `visit_tagged_template_expression` right before the walk
    /// descends into `it.quasi` (oxc dispatches a tagged template's quasi
    /// through `visit_template_literal`, same as a plain template — see the
    /// doc comment on `TemplatePart`); consumed (taken) by the very next
    /// `visit_template_literal` call, which is exactly that quasi. Any
    /// FURTHER nested template reached during that same walk (an
    /// interpolation whose value is itself a template) sees `None` again by
    /// then and anchors at its own span start, unaffected.
    tag_anchor: Option<u32>,
}

impl<'a, 's> OxcVisit<'a> for TsTemplateWalker<'s> {
    fn visit_tagged_template_expression(&mut self, it: &ts_ast::TaggedTemplateExpression<'a>) {
        // Matches `ts_flow_expr`'s `off = span_off(e)` for the WHOLE
        // `TaggedTemplateExpression` — the tag's own span start, not the
        // quasi's — so `df_lit`'s id for this occurrence and this walk's
        // `node` agree exactly.
        let prev = self.tag_anchor.replace(it.span.start);
        oxc_ast_visit::walk::walk_tagged_template_expression(self, it);
        self.tag_anchor = prev;
    }

    fn visit_template_literal(&mut self, it: &ts_ast::TemplateLiteral<'a>) {
        let anchor = self.tag_anchor.take().unwrap_or(it.span.start);
        let node = format!("{}:{anchor}:template", self.file);
        let line = line_at(self.starts, anchor as usize);
        let mut idx = 0u32;
        // `quasis`/`expressions` strictly alternate (quasis.len() ==
        // expressions.len() + 1): static, expr, static, expr, ..., static. An
        // empty static chunk (adjacent interpolations, `` `${a}${b}` ``) still
        // emits its own row with `text = ""` — never skipped, so `idx` always
        // matches the piece's real position and an empty template (a bare
        // `` ` ` ``) still yields one static row.
        for (slot, quasi) in it.quasis.iter().enumerate() {
            self.out.push(TemplatePart {
                node: node.clone(), line, idx, kind: "static", text: quasi.value.raw.to_string(),
            });
            idx += 1;
            if let Some(expr) = it.expressions.get(slot) {
                use oxc_span::GetSpan;
                let span = expr.span();
                let text = self.content.get(span.start as usize..span.end as usize)
                    .unwrap_or_default().to_string();
                self.out.push(TemplatePart { node: node.clone(), line, idx, kind: "expr", text });
                idx += 1;
            }
        }
        // Recurse: a tagged template's own tag expression, and any nested
        // template literal inside an interpolation, get their own
        // `visit_template_literal` call through the normal walk (oxc dispatches
        // `Expression::TemplateLiteral` and `TaggedTemplateExpression.quasi`
        // both through this method), minting their own independent node/idx
        // sequence rather than being folded into this one.
        oxc_ast_visit::walk::walk_template_literal(self, it);
    }
}

/// One `unresolved` marker occurrence: an edge that COULD exist but whose
/// target is computed at runtime rather than a static literal — as opposed to
/// `module_unresolved`, which flags a specifier that resolved to NO project
/// file at all (a genuinely missing target, a different flavor this rel does
/// NOT duplicate). `unresolved`'s own TS/JS-only oxc walk (own
/// `ExtractFamily`, no cross-family reads, so its digest stays self-contained,
/// matching the `template_parts`/`comment_node` precedent) covers three reason
/// buckets, each re-derived from an AST shape another pass in this file
/// already visits for a different purpose, never a wholly new detection
/// concept:
///
/// - `dynamic-import`: a `import(expr)` / `require(expr)` call whose argument
///   is not a plain string literal. The ES grammar requires a static `import
///   ... from` specifier to be a literal, so a computed specifier can only
///   ever show up in call form — the same "not a literal" signal
///   `module_unresolved`'s `"{spec}: dynamic"` case already flags for the
///   template-literal-interpolated case the modgraph regex resolver sees.
/// - `computed-member-call`: `obj[key]()` — the call-site walk that resolves
///   `a.b.c()` to `"c"` (`ts_callee_name`) already visits this exact callee
///   shape and silently drops it today.
/// - `spread-call-args`: `f(...args)` — the dataflow arg walk (`ts_flow_call`)
///   already iterates `c.arguments` and silently drops a `SpreadElement` via
///   `arg.as_expression()` returning `None`.
///
/// `detail` is the computed thing's exact source text, verbatim. `line` is
/// 1-based (the `comment_node`/`sg`/`diag` convention).
///
/// OUT of v1 scope, on purpose: Python star-imports and `sys.path` mutation
/// (both already surfaced today — `module_unresolved`'s `"star import not
/// expanded"` row and a loud `eprintln`, respectively) are not unioned in
/// here, to avoid a cross-family digest dependency (this family's digest
/// would otherwise need to key off `module_unresolved`'s content, not just
/// its own TS/JS file set — the exact "hidden cross-family dependency" shape
/// flagged as a debt item elsewhere). A future widening can revisit this once
/// a safe cross-family digest composition exists.
#[derive(Clone, Debug)]
pub struct UnresolvedRef {
    pub line: u32,
    pub reason: &'static str,
    pub detail: String,
}

/// Every `unresolved` marker in a TS/TSX/JS/JSX/MJS/CJS file (see
/// `UnresolvedRef`).
pub fn ts_unresolved_refs(file: &str, content: &str) -> Vec<UnresolvedRef> {
    let alloc = oxc_allocator::Allocator::default();
    let st = source_type_for(file);
    let ret = oxc_parser::Parser::new(&alloc, content, st).parse();
    if ret.panicked {
        return Vec::new();
    }
    let starts = line_index(content);
    let mut walker = TsUnresolvedWalker { content, starts: &starts, out: Vec::new() };
    walker.visit_program(&ret.program);
    walker.out
}

struct TsUnresolvedWalker<'s> {
    content: &'s str,
    starts: &'s [usize],
    out: Vec<UnresolvedRef>,
}

impl<'s> TsUnresolvedWalker<'s> {
    fn slice(&self, span: oxc_span::Span) -> String {
        self.content.get(span.start as usize..span.end as usize).unwrap_or_default().to_string()
    }
}

impl<'a, 's> OxcVisit<'a> for TsUnresolvedWalker<'s> {
    fn visit_import_expression(&mut self, it: &ts_ast::ImportExpression<'a>) {
        if !matches!(it.source, ts_ast::Expression::StringLiteral(_)) {
            use oxc_span::GetSpan;
            self.out.push(UnresolvedRef {
                line: line_at(self.starts, it.span.start as usize),
                reason: "dynamic-import",
                detail: self.slice(it.source.span()),
            });
        }
        oxc_ast_visit::walk::walk_import_expression(self, it);
    }

    fn visit_call_expression(&mut self, it: &ts_ast::CallExpression<'a>) {
        use oxc_span::GetSpan;
        // `require(expr)`: only a bare `require` callee counts (matching the
        // module resolver's own CJS convention), and only when the sole
        // argument isn't a plain string literal — a static string keeps the
        // dependency statically resolvable, already handled by
        // `module_import`/`module_unresolved`.
        if let ts_ast::Expression::Identifier(callee) = &it.callee {
            if callee.name == "require" {
                if let Some(arg) = it.arguments.first().and_then(|a| a.as_expression()) {
                    if !matches!(arg, ts_ast::Expression::StringLiteral(_)) {
                        self.out.push(UnresolvedRef {
                            line: line_at(self.starts, it.span.start as usize),
                            reason: "dynamic-import",
                            detail: self.slice(arg.span()),
                        });
                    }
                }
            }
        }
        // `obj[key]()`: a computed-member callee, the shape `ts_callee_name`
        // already recognizes and silently drops (returns `None`).
        if let ts_ast::Expression::ComputedMemberExpression(m) = &it.callee {
            self.out.push(UnresolvedRef {
                line: line_at(self.starts, m.span.start as usize),
                reason: "computed-member-call",
                detail: self.slice(m.span),
            });
        }
        // `f(...args)`: a spread argument, the shape `ts_flow_call`'s arg loop
        // already visits and silently drops (`arg.as_expression()` is `None`
        // for `Argument::SpreadElement`).
        for arg in &it.arguments {
            if let ts_ast::Argument::SpreadElement(sp) = arg {
                self.out.push(UnresolvedRef {
                    line: line_at(self.starts, sp.span.start as usize),
                    reason: "spread-call-args",
                    detail: self.slice(sp.span),
                });
            }
        }
        oxc_ast_visit::walk::walk_call_expression(self, it);
    }
}

fn ts_edges_from(program: &ts_ast::Program) -> Vec<TypeEdge> {
    let mut out: BTreeSet<(String, String, &'static str)> = BTreeSet::new();
    for stmt in &program.body {
        use ts_ast::Statement as S;
        match stmt {
            S::ExportNamedDeclaration(e) => {
                if let Some(d) = &e.declaration {
                    ts_decl_edges(d, &mut out);
                }
            }
            S::ExportDefaultDeclaration(e) => match &e.declaration {
                ts_ast::ExportDefaultDeclarationKind::ClassDeclaration(c) => ts_class_edges(c, &mut out),
                ts_ast::ExportDefaultDeclarationKind::TSInterfaceDeclaration(i) => ts_interface_edges(i, &mut out),
                ts_ast::ExportDefaultDeclarationKind::FunctionDeclaration(f) => ts_function_edges(f, &mut out),
                _ => {}
            },
            S::ClassDeclaration(c) => ts_class_edges(c, &mut out),
            S::TSInterfaceDeclaration(i) => ts_interface_edges(i, &mut out),
            S::TSTypeAliasDeclaration(a) => ts_alias_edges(a, &mut out),
            S::TSEnumDeclaration(e) => ts_enum_edges(e, &mut out),
            S::FunctionDeclaration(f) => ts_function_edges(f, &mut out),
            S::VariableDeclaration(v) => ts_var_fn_edges(v, &mut out),
            _ => {}
        }
    }
    out.into_iter()
        .map(|(from, to, kind)| TypeEdge { from, to, kind })
        .collect()
}

/// Collect every `TSTypeReference` name under a type subtree, excluding the
/// declaration's own type-parameter names. Keyword types (string, number, ...)
/// are distinct AST variants, so primitives never show up.
struct TsRefs<'p> {
    params: &'p BTreeSet<String>,
    out: Vec<String>,
}

impl<'a, 'p> OxcVisit<'a> for TsRefs<'p> {
    fn visit_ts_type_reference(&mut self, it: &ts_ast::TSTypeReference<'a>) {
        if let Some(name) = ts_type_name(&it.type_name) {
            if !self.params.contains(&name) {
                self.out.push(name);
            }
        }
        oxc_ast_visit::walk::walk_ts_type_reference(self, it);
    }
}

fn ts_type_name(n: &ts_ast::TSTypeName) -> Option<String> {
    match n {
        ts_ast::TSTypeName::IdentifierReference(id) => Some(id.name.to_string()),
        ts_ast::TSTypeName::QualifiedName(q) => {
            ts_type_name(&q.left).map(|l| format!("{l}.{}", q.right.name))
        }
        ts_ast::TSTypeName::ThisExpression(_) => None,
    }
}

fn ts_refs_in_type(ty: &ts_ast::TSType, params: &BTreeSet<String>) -> Vec<String> {
    let mut c = TsRefs { params, out: Vec::new() };
    c.visit_ts_type(ty);
    c.out.sort();
    c.out.dedup();
    c.out
}

/// Declared type-parameter names + their constraint refs as "generic" edges.
fn ts_param_edges(
    owner: &str,
    tp: &Option<oxc_allocator::Box<ts_ast::TSTypeParameterDeclaration>>,
    out: &mut BTreeSet<(String, String, &'static str)>,
) -> BTreeSet<String> {
    let mut params = BTreeSet::new();
    let Some(tp) = tp else { return params };
    for p in &tp.params {
        params.insert(p.name.name.to_string());
    }
    for p in &tp.params {
        if let Some(c) = &p.constraint {
            for to in ts_refs_in_type(c, &params) {
                push(out, owner, &to, "generic");
            }
        }
    }
    params
}

fn ts_decl_edges(decl: &ts_ast::Declaration, out: &mut BTreeSet<(String, String, &'static str)>) {
    match decl {
        ts_ast::Declaration::ClassDeclaration(c) => ts_class_edges(c, out),
        ts_ast::Declaration::TSInterfaceDeclaration(i) => ts_interface_edges(i, out),
        ts_ast::Declaration::TSTypeAliasDeclaration(a) => ts_alias_edges(a, out),
        ts_ast::Declaration::TSEnumDeclaration(e) => ts_enum_edges(e, out),
        ts_ast::Declaration::FunctionDeclaration(f) => ts_function_edges(f, out),
        ts_ast::Declaration::VariableDeclaration(v) => ts_var_fn_edges(v, out),
        _ => {}
    }
}

/// A named `function foo(...)`. Anonymous functions have no owner, so skip.
fn ts_function_edges(f: &ts_ast::Function, out: &mut BTreeSet<(String, String, &'static str)>) {
    let Some(id) = &f.id else { return };
    ts_fn_signature_edges(
        &id.name,
        &f.type_parameters,
        &f.params,
        &f.return_type,
        f.body.as_deref(),
        out,
    );
}

/// `const foo = (...) => ...` / `const foo = function (...) {...}` at the top
/// level: the binding name owns the function's edges. Plain value consts (no
/// function initializer) carry no type shape and are skipped.
fn ts_var_fn_edges(v: &ts_ast::VariableDeclaration, out: &mut BTreeSet<(String, String, &'static str)>) {
    for d in &v.declarations {
        let ts_ast::BindingPattern::BindingIdentifier(name) = &d.id else { continue };
        match &d.init {
            Some(ts_ast::Expression::ArrowFunctionExpression(a)) => ts_fn_signature_edges(
                &name.name,
                &a.type_parameters,
                &a.params,
                &a.return_type,
                Some(&a.body),
                out,
            ),
            Some(ts_ast::Expression::FunctionExpression(f)) => ts_fn_signature_edges(
                &name.name,
                &f.type_parameters,
                &f.params,
                &f.return_type,
                f.body.as_deref(),
                out,
            ),
            _ => {}
        }
    }
}

/// The shared body of every function form: type-parameter bounds are "generic"
/// (and excluded from refs), parameter types are "param", the return type is
/// "returns", and every TSTypeReference inside the body is "uses".
fn ts_fn_signature_edges(
    owner: &str,
    type_parameters: &Option<oxc_allocator::Box<ts_ast::TSTypeParameterDeclaration>>,
    params: &ts_ast::FormalParameters,
    return_type: &Option<oxc_allocator::Box<ts_ast::TSTypeAnnotation>>,
    body: Option<&ts_ast::FunctionBody>,
    out: &mut BTreeSet<(String, String, &'static str)>,
) {
    let tp = ts_param_edges(owner, type_parameters, out);
    for p in &params.items {
        if let Some(ann) = &p.type_annotation {
            for to in ts_refs_in_type(&ann.type_annotation, &tp) {
                push(out, owner, &to, "param");
            }
        }
    }
    if let Some(rt) = return_type {
        for to in ts_refs_in_type(&rt.type_annotation, &tp) {
            push(out, owner, &to, "returns");
        }
    }
    if let Some(b) = body {
        let mut v = TsRefs { params: &tp, out: Vec::new() };
        v.visit_function_body(b);
        v.out.sort();
        v.out.dedup();
        for to in v.out {
            push(out, owner, &to, "uses");
        }
    }
}

fn ts_class_edges(class: &ts_ast::Class, out: &mut BTreeSet<(String, String, &'static str)>) {
    let Some(id) = &class.id else { return };
    let owner = id.name.to_string();
    let params = ts_param_edges(&owner, &class.type_parameters, out);

    if let Some(sup) = &class.super_class {
        if let ts_ast::Expression::Identifier(idr) = sup {
            push(out, &owner, idr.name.as_str(), "impl");
        }
    }
    if let Some(args) = &class.super_type_arguments {
        for ty in &args.params {
            for to in ts_refs_in_type(ty, &params) {
                push(out, &owner, &to, "impl");
            }
        }
    }
    for imp in &class.implements {
        if let Some(to) = ts_type_name(&imp.expression) {
            push(out, &owner, &to, "impl");
        }
        if let Some(args) = &imp.type_arguments {
            for ty in &args.params {
                for to in ts_refs_in_type(ty, &params) {
                    push(out, &owner, &to, "impl");
                }
            }
        }
    }
    for el in &class.body.body {
        match el {
            ts_ast::ClassElement::PropertyDefinition(p) => {
                if let Some(ann) = &p.type_annotation {
                    for to in ts_refs_in_type(&ann.type_annotation, &params) {
                        push(out, &owner, &to, "field");
                    }
                }
            }
            ts_ast::ClassElement::AccessorProperty(p) => {
                if let Some(ann) = &p.type_annotation {
                    for to in ts_refs_in_type(&ann.type_annotation, &params) {
                        push(out, &owner, &to, "field");
                    }
                }
            }
            // constructor parameter properties (`constructor(private db: Db)`)
            // declare fields; plain constructor args are not part of the shape
            ts_ast::ClassElement::MethodDefinition(m) => {
                if m.kind != ts_ast::MethodDefinitionKind::Constructor {
                    continue;
                }
                for fp in &m.value.params.items {
                    if fp.accessibility.is_none() && !fp.readonly {
                        continue;
                    }
                    if let Some(ann) = &fp.type_annotation {
                        for to in ts_refs_in_type(&ann.type_annotation, &params) {
                            push(out, &owner, &to, "field");
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn ts_interface_edges(
    i: &ts_ast::TSInterfaceDeclaration,
    out: &mut BTreeSet<(String, String, &'static str)>,
) {
    let owner = i.id.name.to_string();
    let params = ts_param_edges(&owner, &i.type_parameters, out);
    for ext in &i.extends {
        if let ts_ast::Expression::Identifier(idr) = &ext.expression {
            push(out, &owner, idr.name.as_str(), "generic");
        }
        if let Some(args) = &ext.type_arguments {
            for ty in &args.params {
                for to in ts_refs_in_type(ty, &params) {
                    push(out, &owner, &to, "generic");
                }
            }
        }
    }
    for member in &i.body.body {
        if let ts_ast::TSSignature::TSPropertySignature(p) = member {
            if let Some(ann) = &p.type_annotation {
                for to in ts_refs_in_type(&ann.type_annotation, &params) {
                    push(out, &owner, &to, "field");
                }
            }
        }
    }
}

fn ts_alias_edges(
    a: &ts_ast::TSTypeAliasDeclaration,
    out: &mut BTreeSet<(String, String, &'static str)>,
) {
    let owner = a.id.name.to_string();
    let params = ts_param_edges(&owner, &a.type_parameters, out);
    // a union alias is a sum type: alternatives that are plain refs are
    // "variant" edges (their type args stay "field"); anything else is shape
    if let ts_ast::TSType::TSUnionType(u) = &a.type_annotation {
        for member in &u.types {
            if let ts_ast::TSType::TSTypeReference(r) = member {
                if let Some(to) = ts_type_name(&r.type_name) {
                    if !params.contains(&to) {
                        push(out, &owner, &to, "variant");
                    }
                }
                if let Some(args) = &r.type_arguments {
                    for ty in &args.params {
                        for to in ts_refs_in_type(ty, &params) {
                            push(out, &owner, &to, "field");
                        }
                    }
                }
            } else {
                for to in ts_refs_in_type(member, &params) {
                    push(out, &owner, &to, "field");
                }
            }
        }
        return;
    }
    for to in ts_refs_in_type(&a.type_annotation, &params) {
        push(out, &owner, &to, "field");
    }
}

fn ts_enum_edges(e: &ts_ast::TSEnumDeclaration, out: &mut BTreeSet<(String, String, &'static str)>) {
    let owner = e.id.name.to_string();
    for m in &e.body.members {
        let name = match &m.id {
            ts_ast::TSEnumMemberName::Identifier(id) => id.name.to_string(),
            ts_ast::TSEnumMemberName::String(s) => s.value.to_string(),
            _ => continue,
        };
        push(out, &owner, &format!("{owner}::{name}"), "variant");
    }
}

// --- entity pass: declared symbols with kind, location, and (for callables)
// the arrow type. Parses a second time (independent of the edge pass) so the
// tested edge extraction stays untouched; one file, two cheap syntax walks. ---

#[cfg(test)]
fn ts_entities(file: &str, content: &str, tsx: bool) -> Vec<TypeEntity> {
    let alloc = oxc_allocator::Allocator::default();
    let st = if tsx { oxc_span::SourceType::tsx() } else { oxc_span::SourceType::ts() };
    let ret = oxc_parser::Parser::new(&alloc, content, st).parse();
    if ret.panicked {
        return Vec::new();
    }
    ts_entities_from(&ret.program, file, content)
}

fn ts_entities_from(program: &ts_ast::Program, file: &str, content: &str) -> Vec<TypeEntity> {
    let starts = line_index(content);
    let mut out = Vec::new();
    for stmt in &program.body {
        use ts_ast::Statement as S;
        match stmt {
            S::ExportNamedDeclaration(e) => {
                if let Some(d) = &e.declaration {
                    ts_decl_entity(d, file, &starts, &mut out);
                }
            }
            S::ExportDefaultDeclaration(e) => match &e.declaration {
                ts_ast::ExportDefaultDeclarationKind::ClassDeclaration(c) => ts_class_entity(c, file, &starts, &mut out),
                ts_ast::ExportDefaultDeclarationKind::TSInterfaceDeclaration(i) => {
                    push_entity(&mut out, file, &starts, &i.id.name, i.span.start, EntityKind::Interface, None, None)
                }
                ts_ast::ExportDefaultDeclarationKind::FunctionDeclaration(f) => ts_fn_entity(f, file, &starts, &mut out),
                _ => {}
            },
            S::ClassDeclaration(c) => ts_class_entity(c, file, &starts, &mut out),
            S::TSInterfaceDeclaration(i) => {
                push_entity(&mut out, file, &starts, &i.id.name, i.span.start, EntityKind::Interface, None, None)
            }
            S::TSTypeAliasDeclaration(a) => {
                push_entity(&mut out, file, &starts, &a.id.name, a.span.start, EntityKind::Alias, None, None)
            }
            S::TSEnumDeclaration(e) => {
                push_entity(&mut out, file, &starts, &e.id.name, e.span.start, EntityKind::Enum, None, None)
            }
            S::FunctionDeclaration(f) => ts_fn_entity(f, file, &starts, &mut out),
            S::VariableDeclaration(v) => ts_var_fn_entity(v, file, &starts, &mut out),
            _ => {}
        }
    }
    out
}

fn ts_decl_entity(d: &ts_ast::Declaration, file: &str, starts: &[usize], out: &mut Vec<TypeEntity>) {
    match d {
        ts_ast::Declaration::ClassDeclaration(c) => ts_class_entity(c, file, starts, out),
        ts_ast::Declaration::TSInterfaceDeclaration(i) => {
            push_entity(out, file, starts, &i.id.name, i.span.start, EntityKind::Interface, None, None)
        }
        ts_ast::Declaration::TSTypeAliasDeclaration(a) => {
            push_entity(out, file, starts, &a.id.name, a.span.start, EntityKind::Alias, None, None)
        }
        ts_ast::Declaration::TSEnumDeclaration(e) => {
            push_entity(out, file, starts, &e.id.name, e.span.start, EntityKind::Enum, None, None)
        }
        ts_ast::Declaration::FunctionDeclaration(f) => ts_fn_entity(f, file, starts, out),
        ts_ast::Declaration::VariableDeclaration(v) => ts_var_fn_entity(v, file, starts, out),
        _ => {}
    }
}

fn ts_class_entity(c: &ts_ast::Class, file: &str, starts: &[usize], out: &mut Vec<TypeEntity>) {
    let Some(id) = &c.id else { return };
    let owner = id.name.to_string();
    push_entity(out, file, starts, &id.name, c.span.start, EntityKind::Class, None, None);
    for el in &c.body.body {
        if let ts_ast::ClassElement::MethodDefinition(m) = el {
            // normal method name `foo()`; skip computed/private/constructor keys
            if m.kind == ts_ast::MethodDefinitionKind::Constructor {
                continue;
            }
            if let ts_ast::PropertyKey::StaticIdentifier(k) = &m.key {
                let ty = ts_fn_type(&m.value.type_parameters, &m.value.params, &m.value.return_type);
                // a TS method's owner is always the enclosing class.
                push_entity(out, file, starts, &k.name, m.span.start, EntityKind::Method, Some((&owner, EntityKind::Class)), Some(ty));
            }
        }
    }
}

fn ts_fn_entity(f: &ts_ast::Function, file: &str, starts: &[usize], out: &mut Vec<TypeEntity>) {
    let Some(id) = &f.id else { return };
    let ty = ts_fn_type(&f.type_parameters, &f.params, &f.return_type);
    push_entity(out, file, starts, &id.name, f.span.start, EntityKind::Function, None, Some(ty));
}

fn ts_var_fn_entity(v: &ts_ast::VariableDeclaration, file: &str, starts: &[usize], out: &mut Vec<TypeEntity>) {
    for d in &v.declarations {
        let ts_ast::BindingPattern::BindingIdentifier(name) = &d.id else { continue };
        let ty = match &d.init {
            Some(ts_ast::Expression::ArrowFunctionExpression(a)) => {
                ts_fn_type(&a.type_parameters, &a.params, &a.return_type)
            }
            Some(ts_ast::Expression::FunctionExpression(f)) => {
                ts_fn_type(&f.type_parameters, &f.params, &f.return_type)
            }
            _ => continue,
        };
        push_entity(out, file, starts, &name.name, d.span.start, EntityKind::Function, None, Some(ty));
    }
}

/// Strip the type-level wrappers that are transparent to a const's runtime
/// value — `as const`, `satisfies T`, parens — same transparency `ts_flow_expr`
/// already gives these forms, so the initializer underneath is reached the
/// same way whether we're lifting dataflow or folding a constant.
fn ts_unwrap_const<'a, 'b>(e: &'b ts_ast::Expression<'a>) -> &'b ts_ast::Expression<'a> {
    match e {
        ts_ast::Expression::TSAsExpression(t) => ts_unwrap_const(&t.expression),
        ts_ast::Expression::TSSatisfiesExpression(t) => ts_unwrap_const(&t.expression),
        ts_ast::Expression::ParenthesizedExpression(p) => ts_unwrap_const(&p.expression),
        _ => e,
    }
}

/// Whether an expression (after unwrapping `as const`/`satisfies`/parens)
/// carries a string value somewhere — a plain string literal, a template, or
/// an object literal with at least one string-bearing property (recursively).
/// Gates entity-minting: a const whose value has no string anywhere gains
/// neither a `type_entity` row nor any `const_value` rows (the "don't mint an
/// entity for every const in the corpus" rule).
fn ts_expr_string_bearing(e: &ts_ast::Expression) -> bool {
    match ts_unwrap_const(e) {
        ts_ast::Expression::StringLiteral(_) | ts_ast::Expression::TemplateLiteral(_) => true,
        ts_ast::Expression::ObjectExpression(o) => o.properties.iter().any(|p| match p {
            ts_ast::ObjectPropertyKind::ObjectProperty(prop) => ts_expr_string_bearing(&prop.value),
            // A spread's value is opaque without evaluating its source; it
            // can't make the object string-bearing on its own (the caller
            // counts it separately when walking for real).
            ts_ast::ObjectPropertyKind::SpreadProperty(_) => false,
        }),
        _ => false,
    }
}

/// Recursively collect `ConstValueFact` rows from a const initializer.
/// `prefix` is the dotted field path built so far ("" at the top, "home",
/// "nested.a", ...). A computed object key (`[expr]: v`) is skipped — there is
/// no static name to hang the field on. A spread property is counted (never
/// followed: its value lives in another symbol this walk hasn't resolved).
fn ts_collect_const_values(
    e: &ts_ast::Expression,
    sym: &str,
    prefix: &str,
    file: &str,
    starts: &[usize],
    content: &str,
    out: &mut Vec<ConstValueFact>,
    spread_skips: &mut usize,
) {
    use oxc_span::GetSpan;
    match ts_unwrap_const(e) {
        ts_ast::Expression::StringLiteral(s) => {
            out.push(ConstValueFact {
                sym: sym.to_string(), field: prefix.to_string(), text: s.value.to_string(),
                kind: "lit", file: file.to_string(), line: line_at(starts, s.span.start as usize),
            });
        }
        ts_ast::Expression::TemplateLiteral(t) => {
            let span = t.span();
            let text = content.get(span.start as usize..span.end as usize).unwrap_or_default().to_string();
            out.push(ConstValueFact {
                sym: sym.to_string(), field: prefix.to_string(), text,
                kind: "template", file: file.to_string(), line: line_at(starts, span.start as usize),
            });
        }
        ts_ast::Expression::ObjectExpression(o) => {
            for p in &o.properties {
                match p {
                    ts_ast::ObjectPropertyKind::ObjectProperty(prop) => {
                        let key = match &prop.key {
                            ts_ast::PropertyKey::StaticIdentifier(i) => i.name.to_string(),
                            ts_ast::PropertyKey::StringLiteral(s) => s.value.to_string(),
                            _ => continue, // computed key: no static field name
                        };
                        let field = if prefix.is_empty() { key } else { format!("{prefix}.{key}") };
                        ts_collect_const_values(&prop.value, sym, &field, file, starts, content, out, spread_skips);
                    }
                    ts_ast::ObjectPropertyKind::SpreadProperty(_) => { *spread_skips += 1; }
                }
            }
        }
        _ => {}
    }
}

/// A `const`/`let`/`var` binding, entity + value pass. `scope` is the name of
/// the nearest enclosing function/closure for a binding found INSIDE a
/// function body (`None` at true module level) — folded into `mint_sym`'s
/// `parent` slot so a lookup table declared inside two different functions in
/// the same file mints two distinct syms rather than colliding. Module-level
/// callers (`ts_const_facts_from`'s own top-level loop) pass `None`;
/// `TsNestedConstWalker` (below) passes the enclosing scope name for anything
/// found inside a function/arrow body — this is what gives `const_value`
/// parity with the retired `const_string_member`'s "generically discovered,
/// not scope-restricted" coverage (a lookup table inside a function body
/// counts too). Arrow/function-expression consts are `ts_var_fn_entity`'s job
/// (a Function entity, untouched here); this walk only looks at bindings that
/// carry a string value: `const name = "..."`. SOUNDNESS RULE: only `const`
/// (or a `let`/`var` marked `as const`) is honest to fold — a plain `let`/`var`
/// string initializer can change under your feet, so it is counted loudly
/// (`const_mutable_skips`) and never emitted.
#[allow(clippy::too_many_arguments)]
fn ts_var_const_facts(
    v: &ts_ast::VariableDeclaration,
    file: &str,
    starts: &[usize],
    content: &str,
    scope: Option<&str>,
    entities: &mut Vec<TypeEntity>,
    consts: &mut Vec<ConstValueFact>,
    spread_skips: &mut usize,
    mutable_skips: &mut usize,
) {
    for d in &v.declarations {
        let ts_ast::BindingPattern::BindingIdentifier(name) = &d.id else { continue };
        let Some(init) = &d.init else { continue };
        // Arrow/function-expression consts are ts_var_fn_entity's Function
        // entities; leave those exactly as they are.
        if matches!(init, ts_ast::Expression::ArrowFunctionExpression(_) | ts_ast::Expression::FunctionExpression(_)) {
            continue;
        }
        if !ts_expr_string_bearing(init) { continue; }
        let as_const = matches!(init, ts_ast::Expression::TSAsExpression(t) if t.type_annotation.is_const_type_reference());
        if !v.kind.is_const() && !as_const {
            *mutable_skips += 1;
            continue;
        }
        let sym = mint_sym(file, EntityKind::Const, &name.name, scope);
        entities.push(TypeEntity {
            sym: sym.clone(), name: name.name.to_string(), kind: EntityKind::Const,
            parent: None, file: file.to_string(), line: line_at(starts, d.span.start as usize), ty: None,
        });
        ts_collect_const_values(init, &sym, "", file, starts, content, consts, spread_skips);
    }
}

/// String enum members (`enum Routes { Home = '/home' }`): `sym` is the
/// ENUM's own entity sym (already minted by `ts_entities_from`'s
/// `TSEnumDeclaration` arm) — a member is a field of its enum, not a second
/// entity. Only a plain string initializer qualifies; a computed/numeric
/// member yields no row.
fn ts_enum_const_values(e: &ts_ast::TSEnumDeclaration, file: &str, starts: &[usize], out: &mut Vec<ConstValueFact>) {
    let owner_sym = mint_sym(file, EntityKind::Enum, &e.id.name, None);
    for m in &e.body.members {
        let name = match &m.id {
            ts_ast::TSEnumMemberName::Identifier(id) => id.name.to_string(),
            ts_ast::TSEnumMemberName::String(s) => s.value.to_string(),
            _ => continue,
        };
        let Some(init) = &m.initializer else { continue };
        if let ts_ast::Expression::StringLiteral(s) = ts_unwrap_const(init) {
            out.push(ConstValueFact {
                sym: owner_sym.clone(), field: name, text: s.value.to_string(),
                kind: "lit", file: file.to_string(), line: line_at(starts, m.span.start as usize),
            });
        }
    }
}

/// Top-level driver for the const-value pass (item 3 of the string-values
/// arc): walks `program.body` once for top-level `const`/`let`/`var`
/// declarations (bare or `export`-wrapped) and `enum` declarations, returning
/// the const entities to fold into `ts_entities_from`'s output plus the
/// `const_value` rows and the two loud-skip counters. A SEPARATE statement
/// walk from `ts_entities_from`/`ts_edges_from`/`ts_docs_from` (same "one
/// file, several cheap syntax walks" shape those already use) rather than
/// retrofitting those recursive helpers, which are reused by call-graph/
/// dataflow passes with a narrower `Vec<TypeEntity>`-only signature.
///
/// After the top-level loop, `TsNestedConstWalker` descends into every
/// function/arrow body in the file for the SAME string-bearing-const shape,
/// scoped by the nearest enclosing function/closure name — this is the
/// evidence-diff fix from the `const_string_member` retirement (plans/
/// 2026-07-10-string-values-const-value.md follow-up): `const_string_member`
/// was "generically discovered" (every `const` declarator in the file, no
/// scope restriction), so a lookup table declared inside a function body
/// counted there but was invisible to `const_value`'s module-level-only walk.
/// Enum declarations stay top-level-only (no known corpus case of a
/// function-local enum feeding a route table; `const_string_member` never
/// covered enums either).
fn ts_const_facts_from(program: &ts_ast::Program, file: &str, content: &str) -> (Vec<TypeEntity>, Vec<ConstValueFact>, usize, usize) {
    let starts = line_index(content);
    let mut entities = Vec::new();
    let mut consts = Vec::new();
    let mut spread_skips = 0usize;
    let mut mutable_skips = 0usize;
    for stmt in &program.body {
        use ts_ast::Statement as S;
        let var_decl: Option<&ts_ast::VariableDeclaration> = match stmt {
            S::VariableDeclaration(v) => Some(v),
            S::ExportNamedDeclaration(exp) => match &exp.declaration {
                Some(ts_ast::Declaration::VariableDeclaration(v)) => Some(v),
                _ => None,
            },
            _ => None,
        };
        if let Some(v) = var_decl {
            ts_var_const_facts(v, file, &starts, content, None, &mut entities, &mut consts, &mut spread_skips, &mut mutable_skips);
        }
        let enum_decl: Option<&ts_ast::TSEnumDeclaration> = match stmt {
            S::TSEnumDeclaration(en) => Some(en),
            S::ExportNamedDeclaration(exp) => match &exp.declaration {
                Some(ts_ast::Declaration::TSEnumDeclaration(en)) => Some(en),
                _ => None,
            },
            _ => None,
        };
        if let Some(en) = enum_decl {
            ts_enum_const_values(en, file, &starts, &mut consts);
        }
    }
    let mut nested = TsNestedConstWalker {
        file, content, starts: &starts, scope: Vec::new(),
        entities: &mut entities, consts: &mut consts,
        spread_skips: &mut spread_skips, mutable_skips: &mut mutable_skips,
    };
    nested.visit_program(program);
    (entities, consts, spread_skips, mutable_skips)
}

/// Descends into every function/arrow body for string-bearing `const`
/// declarations found there — see `ts_const_facts_from`'s doc comment. Only
/// fires INSIDE a function scope (`scope` non-empty); top-level statements
/// are the existing loop's job, so `visit_variable_declaration` is a no-op at
/// depth 0 (avoids double-emitting a module-level const). `visit_function`/
/// `visit_arrow_function_expression` push a scope name — the function's own
/// name when named, else a byte-offset-derived `closure_<span-start>` tag
/// (stable across ticks for unchanged content, matching the `df_lit`/
/// `template_parts` `node` id convention) — so two same-named local consts in
/// two different functions in the same file mint distinct syms.
struct TsNestedConstWalker<'s> {
    file: &'s str,
    content: &'s str,
    starts: &'s [usize],
    scope: Vec<String>,
    entities: &'s mut Vec<TypeEntity>,
    consts: &'s mut Vec<ConstValueFact>,
    spread_skips: &'s mut usize,
    mutable_skips: &'s mut usize,
}

impl<'a, 's> OxcVisit<'a> for TsNestedConstWalker<'s> {
    fn visit_function(&mut self, it: &ts_ast::Function<'a>, flags: oxc_syntax::scope::ScopeFlags) {
        let name = it.id.as_ref().map(|id| id.name.to_string())
            .unwrap_or_else(|| format!("closure_{}", it.span.start));
        self.scope.push(name);
        oxc_ast_visit::walk::walk_function(self, it, flags);
        self.scope.pop();
    }

    fn visit_arrow_function_expression(&mut self, it: &ts_ast::ArrowFunctionExpression<'a>) {
        self.scope.push(format!("closure_{}", it.span.start));
        oxc_ast_visit::walk::walk_arrow_function_expression(self, it);
        self.scope.pop();
    }

    fn visit_variable_declaration(&mut self, it: &ts_ast::VariableDeclaration<'a>) {
        if let Some(scope) = self.scope.last() {
            let scope = scope.clone();
            ts_var_const_facts(
                it, self.file, self.starts, self.content, Some(&scope),
                self.entities, self.consts, self.spread_skips, self.mutable_skips,
            );
        }
        oxc_ast_visit::walk::walk_variable_declaration(self, it);
    }
}

/// Doc-comment pass (oxc): oxc keeps comments out of the AST, so each `/** */`
/// block in the source is associated with the entity it documents by byte
/// position — the nearest anchor at or after the block's end, with only
/// whitespace between (so an `export`/`default` prefix, which sits before the
/// anchored statement start, is fine; a decorator or another statement is not,
/// and the block is dropped). Syms match `ts_entities_from` exactly so
/// `doc_comment` joins `type_entity`.
fn ts_docs_from(program: &ts_ast::Program, file: &str, content: &str) -> Vec<DocFact> {
    let anchors = ts_doc_anchors(program, file);
    if anchors.is_empty() { return Vec::new(); }
    let starts = line_index(content);
    let mut out = Vec::new();
    for (cstart, cend) in ts_block_comments(content) {
        let raw = &content[cstart..cend];
        if !raw.trim_start().starts_with("/**") { continue; }
        let Some((sym, at)) = anchors.iter().filter(|(_, s)| (*s as usize) >= cend).min_by_key(|(_, s)| *s) else { continue };
        if !content[cend..*at as usize].trim().is_empty() { continue; }
        let text = clean_block_comment(raw);
        out.push(DocFact { sym: sym.clone(), line: line_at(&starts, *at as usize), tags: parse_jsdoc_tags(&text), text });
    }
    out
}

/// `(sym, byte)` for every entity `ts_entities_from` emits. Top-level decls
/// anchor at the STATEMENT start; class methods at the method span start.
fn ts_doc_anchors(program: &ts_ast::Program, file: &str) -> Vec<(String, u32)> {
    use oxc_span::GetSpan;
    let mut out = Vec::new();
    for stmt in &program.body {
        use ts_ast::Statement as S;
        let at = stmt.span().start;
        match stmt {
            S::ExportNamedDeclaration(e) => { if let Some(d) = &e.declaration { ts_decl_anchor(d, file, at, &mut out); } }
            S::ExportDefaultDeclaration(e) => match &e.declaration {
                ts_ast::ExportDefaultDeclarationKind::ClassDeclaration(c) => ts_class_anchor(c, file, at, &mut out),
                ts_ast::ExportDefaultDeclarationKind::TSInterfaceDeclaration(i) => out.push((mint_sym(file, EntityKind::Interface, &i.id.name, None), at)),
                ts_ast::ExportDefaultDeclarationKind::FunctionDeclaration(f) => { if let Some(id) = &f.id { out.push((mint_sym(file, EntityKind::Function, &id.name, None), at)); } }
                _ => {}
            },
            S::ClassDeclaration(c) => ts_class_anchor(c, file, at, &mut out),
            S::TSInterfaceDeclaration(i) => out.push((mint_sym(file, EntityKind::Interface, &i.id.name, None), at)),
            S::TSTypeAliasDeclaration(a) => out.push((mint_sym(file, EntityKind::Alias, &a.id.name, None), at)),
            S::TSEnumDeclaration(en) => out.push((mint_sym(file, EntityKind::Enum, &en.id.name, None), at)),
            S::FunctionDeclaration(f) => { if let Some(id) = &f.id { out.push((mint_sym(file, EntityKind::Function, &id.name, None), at)); } }
            S::VariableDeclaration(v) => ts_var_anchor(v, file, at, &mut out),
            _ => {}
        }
    }
    out
}

fn ts_decl_anchor(d: &ts_ast::Declaration, file: &str, at: u32, out: &mut Vec<(String, u32)>) {
    match d {
        ts_ast::Declaration::ClassDeclaration(c) => ts_class_anchor(c, file, at, out),
        ts_ast::Declaration::TSInterfaceDeclaration(i) => out.push((mint_sym(file, EntityKind::Interface, &i.id.name, None), at)),
        ts_ast::Declaration::TSTypeAliasDeclaration(a) => out.push((mint_sym(file, EntityKind::Alias, &a.id.name, None), at)),
        ts_ast::Declaration::TSEnumDeclaration(en) => out.push((mint_sym(file, EntityKind::Enum, &en.id.name, None), at)),
        ts_ast::Declaration::FunctionDeclaration(f) => { if let Some(id) = &f.id { out.push((mint_sym(file, EntityKind::Function, &id.name, None), at)); } }
        ts_ast::Declaration::VariableDeclaration(v) => ts_var_anchor(v, file, at, out),
        _ => {}
    }
}

fn ts_class_anchor(c: &ts_ast::Class, file: &str, at: u32, out: &mut Vec<(String, u32)>) {
    let Some(id) = &c.id else { return };
    let owner = id.name.to_string();
    out.push((mint_sym(file, EntityKind::Class, &id.name, None), at));
    for el in &c.body.body {
        if let ts_ast::ClassElement::MethodDefinition(m) = el {
            if m.kind == ts_ast::MethodDefinitionKind::Constructor { continue; }
            if let ts_ast::PropertyKey::StaticIdentifier(k) = &m.key {
                out.push((mint_sym(file, EntityKind::Method, &k.name, Some(&owner)), m.span.start));
            }
        }
    }
}

fn ts_var_anchor(v: &ts_ast::VariableDeclaration, file: &str, at: u32, out: &mut Vec<(String, u32)>) {
    for d in &v.declarations {
        let ts_ast::BindingPattern::BindingIdentifier(name) = &d.id else { continue };
        if matches!(&d.init, Some(ts_ast::Expression::ArrowFunctionExpression(_)) | Some(ts_ast::Expression::FunctionExpression(_))) {
            out.push((mint_sym(file, EntityKind::Function, &name.name, None), at));
        }
    }
}

/// Byte ranges of every `/* ... */` block comment, including delimiters. A naive
/// scan: good enough for doc association (non-`/**` blocks are filtered by the
/// caller, and `/*` inside a string is rare and harmless here).
fn ts_block_comments(content: &str) -> Vec<(usize, usize)> {
    let b = content.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < b.len() {
        if b[i] == b'/' && b[i + 1] == b'*' {
            match content[i + 2..].find("*/") {
                Some(rel) => { let end = i + 2 + rel + 2; out.push((i, end)); i = end; continue; }
                None => break,
            }
        }
        i += 1;
    }
    out
}

/// Build the arrow `[...A] => B` for a function form. Each param slot collects
/// its referenced type names (declared type-param names excluded); the return
/// slot likewise. Keyword/primitive slots come back empty.
fn ts_fn_type(
    type_parameters: &Option<oxc_allocator::Box<ts_ast::TSTypeParameterDeclaration>>,
    params: &ts_ast::FormalParameters,
    return_type: &Option<oxc_allocator::Box<ts_ast::TSTypeAnnotation>>,
) -> TypeExpr {
    let mut tp = BTreeSet::new();
    if let Some(tps) = type_parameters {
        for p in &tps.params {
            tp.insert(p.name.name.to_string());
        }
    }
    let named = |refs: Vec<String>| refs.into_iter().map(TypeRef::Named).collect::<Vec<_>>();
    let params = params
        .items
        .iter()
        .map(|p| match &p.type_annotation {
            Some(ann) => named(ts_refs_in_type(&ann.type_annotation, &tp)),
            None => Vec::new(),
        })
        .collect();
    let ret = match return_type {
        Some(rt) => named(ts_refs_in_type(&rt.type_annotation, &tp)),
        None => Vec::new(),
    };
    TypeExpr { params, ret }
}

// `parent` is `(owner_name, owner_kind)`: the method sym embeds the owner NAME
// (`Owner.name`), while the stored `parent` field is the owner's OWN entity sym
// minted with the owner's REAL kind — so `type_entity.parent` joins
// `type_entity.sym` with no normalization.
fn push_entity(
    out: &mut Vec<TypeEntity>,
    file: &str,
    starts: &[usize],
    name: &str,
    span_start: u32,
    kind: EntityKind,
    parent: Option<(&str, EntityKind)>,
    ty: Option<TypeExpr>,
) {
    out.push(TypeEntity {
        sym: mint_sym(file, kind, name, parent.map(|(p, _)| p)),
        name: name.to_string(),
        kind,
        parent: parent.map(|(p, pk)| mint_sym(file, pk, p, None)),
        file: file.to_string(),
        line: line_at(starts, span_start as usize),
        ty,
    });
}

// --- TypeScript call-graph pass (oxc): function declarations, exported/const
// arrow + function-expression bindings, and class methods become CallDefs (Free
// for standalone callables, Method for class members keyed to the class); every
// `CallExpression` becomes a CallSite whose callee is the called name as written
// (identifier, or the trailing property of a member expression). `end` is the
// body span end converted to a 1-based line; caller resolution is the engine's
// span-containment pass, same as Rust. ---

fn ts_call_defs_from(program: &ts_ast::Program, file: &str, starts: &[usize]) -> Vec<CallDef> {
    let mut out = Vec::new();
    for stmt in &program.body {
        use ts_ast::Statement as S;
        match stmt {
            S::FunctionDeclaration(f) => ts_fn_call_def(f, file, starts, &mut out),
            S::ExportNamedDeclaration(e) => {
                if let Some(d) = &e.declaration {
                    ts_decl_call_def(d, file, starts, &mut out);
                }
            }
            S::ExportDefaultDeclaration(e) => match &e.declaration {
                ts_ast::ExportDefaultDeclarationKind::ClassDeclaration(c) => {
                    ts_class_call_defs(c, file, starts, &mut out)
                }
                ts_ast::ExportDefaultDeclarationKind::FunctionDeclaration(f) => {
                    ts_fn_call_def(f, file, starts, &mut out)
                }
                _ => {}
            },
            S::ClassDeclaration(c) => ts_class_call_defs(c, file, starts, &mut out),
            S::VariableDeclaration(v) => ts_var_call_defs(v, file, starts, &mut out),
            _ => {}
        }
    }
    out
}

fn ts_decl_call_def(d: &ts_ast::Declaration, file: &str, starts: &[usize], out: &mut Vec<CallDef>) {
    match d {
        ts_ast::Declaration::FunctionDeclaration(f) => ts_fn_call_def(f, file, starts, out),
        ts_ast::Declaration::ClassDeclaration(c) => ts_class_call_defs(c, file, starts, out),
        ts_ast::Declaration::VariableDeclaration(v) => ts_var_call_defs(v, file, starts, out),
        _ => {}
    }
}

fn ts_fn_call_def(f: &ts_ast::Function, file: &str, starts: &[usize], out: &mut Vec<CallDef>) {
    let Some(id) = &f.id else { return };
    let Some(body) = f.body.as_deref() else { return };
    let name = id.name.to_string();
    out.push(CallDef {
        sym: mint_sym(file, EntityKind::Function, &name, None),
        name,
        kind: CallKind::Free,
        file: file.to_string(),
        line: line_at(starts, id.span.start as usize),
        end: line_at(starts, body.span.end as usize),
    });
}

fn ts_class_call_defs(c: &ts_ast::Class, file: &str, starts: &[usize], out: &mut Vec<CallDef>) {
    let Some(id) = &c.id else { return };
    let owner = id.name.to_string();
    for el in &c.body.body {
        let ts_ast::ClassElement::MethodDefinition(m) = el else { continue };
        // skip the constructor (no callable name) and computed/private keys.
        if m.kind == ts_ast::MethodDefinitionKind::Constructor {
            continue;
        }
        let ts_ast::PropertyKey::StaticIdentifier(k) = &m.key else { continue };
        let Some(body) = m.value.body.as_deref() else { continue };
        let name = k.name.to_string();
        out.push(CallDef {
            sym: mint_sym(file, EntityKind::Method, &name, Some(&owner)),
            name,
            kind: CallKind::Method,
            file: file.to_string(),
            line: line_at(starts, m.span.start as usize),
            end: line_at(starts, body.span.end as usize),
        });
    }
}

fn ts_var_call_defs(v: &ts_ast::VariableDeclaration, file: &str, starts: &[usize], out: &mut Vec<CallDef>) {
    for d in &v.declarations {
        let ts_ast::BindingPattern::BindingIdentifier(name) = &d.id else { continue };
        let body_end = match &d.init {
            Some(ts_ast::Expression::ArrowFunctionExpression(a)) => a.body.span.end,
            Some(ts_ast::Expression::FunctionExpression(f)) => match f.body.as_deref() {
                Some(b) => b.span.end,
                None => continue,
            },
            _ => continue,
        };
        let nm = name.name.to_string();
        out.push(CallDef {
            sym: mint_sym(file, EntityKind::Function, &nm, None),
            name: nm,
            kind: CallKind::Free,
            file: file.to_string(),
            line: line_at(starts, d.span.start as usize),
            end: line_at(starts, body_end as usize),
        });
    }
}

/// Collect every `CallExpression` anywhere in the program (including method and
/// nested bodies); the engine's containment pass attaches each to its caller.
struct TsCallSites<'p> {
    file: &'p str,
    starts: &'p [usize],
    sites: Vec<CallSite>,
}

impl<'a, 'p> OxcVisit<'a> for TsCallSites<'p> {
    fn visit_call_expression(&mut self, c: &ts_ast::CallExpression<'a>) {
        if let Some(callee) = ts_callee_name(&c.callee) {
            self.sites.push(CallSite {
                caller_sym: None,
                callee,
                callee_path: None,
                file: self.file.to_string(),
                line: line_at(self.starts, span_off(&c.callee) as usize),
            });
        }
        oxc_ast_visit::walk::walk_call_expression(self, c);
    }
    // `<Card .../>` is a call — jsx(Card, props) — so a component usage is a
    // call site and call_edge resolves caller -> Card like any other callee.
    // Host elements (`<div/>`, lowercase = JSXElementName::Identifier) are
    // skipped at the source: there is no def to resolve to.
    fn visit_jsx_element(&mut self, el: &ts_ast::JSXElement<'a>) {
        use ts_ast::JSXElementName as N;
        let callee = match &el.opening_element.name {
            N::IdentifierReference(r) => Some(r.name.to_string()),
            N::MemberExpression(m) => Some(m.property.name.to_string()),
            _ => None,
        };
        if let Some(callee) = callee {
            self.sites.push(CallSite {
                caller_sym: None,
                callee,
                callee_path: None,
                file: self.file.to_string(),
                line: line_at(self.starts, el.opening_element.span.start as usize),
            });
        }
        oxc_ast_visit::walk::walk_jsx_element(self, el);
    }
}

/// The called name as written: a bare identifier, or the trailing property of a
/// member expression (`a.b.c()` -> "c"), matching the Rust trailing-segment
/// convention. Computed/other callee shapes resolve to nothing.
fn ts_callee_name(e: &ts_ast::Expression) -> Option<String> {
    use ts_ast::Expression as E;
    match e {
        E::Identifier(id) => Some(id.name.to_string()),
        E::StaticMemberExpression(m) => Some(m.property.name.to_string()),
        _ => None,
    }
}

// --- Rust entity pass (syn): structs/enums/unions/traits as data types, free
// functions and impl methods as callables with arrow types. Lines come from
// proc-macro2 span-locations (the `Spanned` ident span). ---

#[cfg(test)]
fn rust_entities(file: &str, content: &str) -> Vec<TypeEntity> {
    let Ok(parsed) = syn::parse_file(content) else {
        return Vec::new();
    };
    rust_entities_from(&parsed, file)
}

fn rust_entities_from(parsed: &syn::File, file: &str) -> Vec<TypeEntity> {
    let owner_kinds = rust_owner_kinds(parsed);
    let mut out = Vec::new();
    for item in &parsed.items {
        rust_item_entity(item, file, &owner_kinds, &mut out);
    }
    out
}

/// Top-level `const X: &str = "...";` string values (item 3's Rust slice,
/// ledgered as "if cheap" — a plain `syn::Lit::Str` initializer on a
/// module-level `const` is). Non-goals: consts inside `impl`/`mod`/fn bodies,
/// non-string consts (no entity, no row — same "don't mint for every const"
/// rule the TS lift follows), and no `as const` equivalent (Rust has none).
fn rust_const_values_from(parsed: &syn::File, file: &str) -> (Vec<TypeEntity>, Vec<ConstValueFact>) {
    let mut entities = Vec::new();
    let mut consts = Vec::new();
    for item in &parsed.items {
        let Item::Const(c) = item else { continue };
        let syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. }) = &*c.expr else { continue };
        let name = c.ident.to_string();
        let sym = mint_sym(file, EntityKind::Const, &name, None);
        let line = rust_line(c.ident.span());
        entities.push(TypeEntity {
            sym: sym.clone(), name, kind: EntityKind::Const,
            parent: None, file: file.to_string(), line, ty: None,
        });
        consts.push(ConstValueFact { sym, field: String::new(), text: s.value(), kind: "lit", file: file.to_string(), line });
    }
    (entities, consts)
}

/// Map each top-level type declaration's name to its real `EntityKind`, so an
/// `impl` block's methods can key their `parent` to the owner's OWN entity sym
/// (`file::struct::Foo`, not a hardcoded `file::class::Foo`). An `impl` for a
/// type declared in another file has no local owner row, so no parent sym could
/// join regardless of kind; the default guess is harmless there.
fn rust_owner_kinds(parsed: &syn::File) -> std::collections::HashMap<String, EntityKind> {
    let mut m = std::collections::HashMap::new();
    for item in &parsed.items {
        match item {
            Item::Struct(s) => { m.insert(s.ident.to_string(), EntityKind::Struct); }
            Item::Enum(en) => { m.insert(en.ident.to_string(), EntityKind::Enum); }
            Item::Union(u) => { m.insert(u.ident.to_string(), EntityKind::Struct); }
            Item::Trait(t) => { m.insert(t.ident.to_string(), EntityKind::Trait); }
            _ => {}
        }
    }
    m
}

fn rust_line(span: proc_macro2::Span) -> u32 {
    span.start().line as u32
}

fn rust_item_entity(
    item: &Item,
    file: &str,
    owner_kinds: &std::collections::HashMap<String, EntityKind>,
    out: &mut Vec<TypeEntity>,
) {
    // `parent` is the bare owner name (e.g. "Engine"); the method sym uses it
    // as `Owner.name`, while the stored parent field is the owner's own sym
    // minted with the owner's REAL kind (looked up in `owner_kinds`) so it
    // equality-joins `type_entity.sym`.
    let mut e = |name: String, line: u32, kind: EntityKind, parent: Option<String>, ty: Option<TypeExpr>| {
        let parent_sym = parent.as_deref().map(|p| {
            let pk = owner_kinds.get(p).copied().unwrap_or(EntityKind::Struct);
            mint_sym(file, pk, p, None)
        });
        out.push(TypeEntity {
            sym: mint_sym(file, kind, &name, parent.as_deref()),
            name,
            kind,
            parent: parent_sym,
            file: file.to_string(),
            line,
            ty,
        });
    };
    match item {
        Item::Struct(s) => e(s.ident.to_string(), rust_line(s.ident.span()), EntityKind::Struct, None, None),
        Item::Enum(en) => e(en.ident.to_string(), rust_line(en.ident.span()), EntityKind::Enum, None, None),
        Item::Union(u) => e(u.ident.to_string(), rust_line(u.ident.span()), EntityKind::Struct, None, None),
        Item::Trait(t) => {
            e(t.ident.to_string(), rust_line(t.ident.span()), EntityKind::Trait, None, None);
            let owner = Some(t.ident.to_string());
            for ti in &t.items {
                // Only default methods (a body inside the trait block) get an
                // entity row here; a bare signature (no body) has no code to
                // hang a `type_entity` on and is left to the impl side.
                if let syn::TraitItem::Fn(m) = ti {
                    if m.default.is_some() {
                        e(
                            m.sig.ident.to_string(),
                            rust_line(m.sig.ident.span()),
                            EntityKind::Method,
                            owner.clone(),
                            Some(rust_fn_type(&m.sig)),
                        );
                    }
                }
            }
        }
        Item::Fn(f) => e(
            f.sig.ident.to_string(),
            rust_line(f.sig.ident.span()),
            EntityKind::Function,
            None,
            Some(rust_fn_type(&f.sig)),
        ),
        Item::Impl(i) => {
            let owner = primary_type(&i.self_ty);
            for ii in &i.items {
                if let syn::ImplItem::Fn(m) = ii {
                    e(
                        m.sig.ident.to_string(),
                        rust_line(m.sig.ident.span()),
                        EntityKind::Method,
                        owner.clone(),
                        Some(rust_fn_type(&m.sig)),
                    );
                }
            }
        }
        _ => {}
    }
}

/// Doc-comment pass (syn): every `#[doc]` attribute on an item — the desugared
/// form of `///` / `//!` / `/** */` — becomes a `DocFact` keyed by the same sym
/// `rust_item_entity` mints. Reading attrs (not scanning lines above the decl)
/// is what makes a `#[derive(..)]` between the doc and the `struct` keyword a
/// non-issue. Tags are the rustdoc `# Section` headings.
fn rust_docs_from(parsed: &syn::File, file: &str) -> Vec<DocFact> {
    let mut out = Vec::new();
    for item in &parsed.items {
        rust_item_docs(item, file, &mut out);
    }
    out
}

fn rust_item_docs(item: &Item, file: &str, out: &mut Vec<DocFact>) {
    match item {
        Item::Struct(s) => push_doc(out, file, &s.attrs, &s.ident.to_string(), rust_line(s.ident.span()), EntityKind::Struct, None),
        Item::Enum(en) => push_doc(out, file, &en.attrs, &en.ident.to_string(), rust_line(en.ident.span()), EntityKind::Enum, None),
        Item::Union(u) => push_doc(out, file, &u.attrs, &u.ident.to_string(), rust_line(u.ident.span()), EntityKind::Struct, None),
        Item::Trait(t) => push_doc(out, file, &t.attrs, &t.ident.to_string(), rust_line(t.ident.span()), EntityKind::Trait, None),
        Item::Fn(f) => push_doc(out, file, &f.attrs, &f.sig.ident.to_string(), rust_line(f.sig.ident.span()), EntityKind::Function, None),
        Item::Impl(i) => {
            let owner = primary_type(&i.self_ty);
            for ii in &i.items {
                if let syn::ImplItem::Fn(m) = ii {
                    push_doc(out, file, &m.attrs, &m.sig.ident.to_string(), rust_line(m.sig.ident.span()), EntityKind::Method, owner.as_deref());
                }
            }
        }
        _ => {}
    }
}

fn push_doc(out: &mut Vec<DocFact>, file: &str, attrs: &[syn::Attribute], name: &str, line: u32, kind: EntityKind, parent: Option<&str>) {
    let lines = rust_doc_lines(attrs);
    if lines.is_empty() { return; }
    let text = lines.join("\n");
    out.push(DocFact {
        sym: mint_sym(file, kind, name, parent),
        line,
        tags: parse_rust_sections(&text),
        text,
    });
}

/// Collect the string values of an item's `#[doc = "..."]` attributes, one per
/// `///` line, dropping the single leading space syn keeps from `/// foo`.
fn rust_doc_lines(attrs: &[syn::Attribute]) -> Vec<String> {
    let mut lines = Vec::new();
    for a in attrs {
        if !a.path().is_ident("doc") { continue; }
        if let syn::Meta::NameValue(nv) = &a.meta {
            if let syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. }) = &nv.value {
                let v = s.value();
                lines.push(v.strip_prefix(' ').unwrap_or(&v).to_string());
            }
        }
    }
    lines
}

fn rust_fn_type(sig: &syn::Signature) -> TypeExpr {
    let named = |refs: Vec<String>| refs.into_iter().map(TypeRef::Named).collect::<Vec<_>>();
    // `self` receivers are not value params; drop them so positions line up
    // with the written argument list.
    let params = sig
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            syn::FnArg::Typed(pt) => Some(named(type_refs(&pt.ty))),
            syn::FnArg::Receiver(_) => None,
        })
        .collect();
    let ret = match &sig.output {
        ReturnType::Type(_, ty) => named(type_refs(ty)),
        ReturnType::Default => Vec::new(),
    };
    TypeExpr { params, ret }
}

// --- Rust call-graph pass (syn): free functions and impl methods become
// CallDefs (sym + body span for callsite containment), and every call
// expression becomes a CallSite whose caller is left blank for the engine's
// span-containment pass to fill. Closures are collected as anonymous defs in a
// follow-up; the visitor still walks into them so calls inside a closure body
// attribute to the enclosing named def. ---

fn rust_call_defs_from(parsed: &syn::File, file: &str) -> Vec<CallDef> {
    let mut out = Vec::new();
    let push = |out: &mut Vec<CallDef>, sym: String, name: String, kind: CallKind, line: u32, end: u32| {
        out.push(CallDef {
            sym, name, kind, file: file.to_string(), line, end,
        });
    };
    for item in &parsed.items {
        match item {
            Item::Fn(f) => {
                let name = f.sig.ident.to_string();
                let line = rust_line(f.sig.ident.span());
                let end = f.block.span().end().line as u32;
                push(&mut out, mint_sym(file, EntityKind::Function, &name, None), name, CallKind::Free, line, end);
            }
            Item::Impl(i) => {
                let owner = primary_type(&i.self_ty);
                for ii in &i.items {
                    if let syn::ImplItem::Fn(m) = ii {
                        let name = m.sig.ident.to_string();
                        let line = rust_line(m.sig.ident.span());
                        let end = m.block.span().end().line as u32;
                        push(&mut out,
                            mint_sym(file, EntityKind::Method, &name, owner.as_deref()),
                            name, CallKind::Method, line, end);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// The trailing identifier of a callee expression's source text: the last run
/// of alnum/underscore. `helper` -> "helper", `Vec::new` -> "new",
/// `self.foo.bar` -> "bar". Used to key the bare-name resolver the same way
/// `type_link` resolves a type reference.
fn rust_call_sites_from(parsed: &syn::File, file: &str) -> Vec<CallSite> {
    let mut v = CallCollector { file, sites: Vec::new() };
    syn::visit::visit_file(&mut v, parsed);
    v.sites
}

struct CallCollector<'a> {
    file: &'a str,
    sites: Vec<CallSite>,
}

impl<'a> syn::visit::Visit<'a> for CallCollector<'a> {
    fn visit_expr(&mut self, e: &'a syn::Expr) {
        match e {
            // `f(args)` / `Foo(args)`: callee is the path's trailing segment;
            // callee_path carries the full qualified path when >1 segment.
            syn::Expr::Call(c) => {
                let func = peel_parens(&c.func);
                if let syn::Expr::Path(p) = func {
                    if let Some(seg) = p.path.segments.last() {
                        let path_str = path_string(&p.path);
                        self.sites.push(CallSite {
                            caller_sym: None,
                            callee: seg.ident.to_string(),
                            callee_path: (p.path.segments.len() > 1).then_some(path_str),
                            file: self.file.to_string(),
                            line: c.func.span().start().line as u32,
                        });
                    }
                }
                syn::visit::visit_expr(self, e);
            }
            // `recv.m(args)`: callee is the method ident.
            syn::Expr::MethodCall(m) => {
                self.sites.push(CallSite {
                    caller_sym: None,
                    callee: m.method.to_string(),
                    callee_path: None,
                    file: self.file.to_string(),
                    line: m.method.span().start().line as u32,
                });
                syn::visit::visit_expr(self, e);
            }
            // `Foo { x: 1 }`: struct literal constructor. callee is the type
            // path's trailing segment; callee_path carries the full path.
            syn::Expr::Struct(s) => {
                if let Some(seg) = s.path.segments.last() {
                    let path_str = path_string(&s.path);
                    self.sites.push(CallSite {
                        caller_sym: None,
                        callee: seg.ident.to_string(),
                        callee_path: (s.path.segments.len() > 1).then_some(path_str),
                        file: self.file.to_string(),
                        line: s.path.span().start().line as u32,
                    });
                }
                syn::visit::visit_expr(self, e);
            }
            _ => syn::visit::visit_expr(self, e),
        }
    }
}

/// Render a syn::Path as `a::b::c`.
fn path_string(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

/// Strip nested `Expr::Paren` to find the inner expression.
fn peel_parens(e: &syn::Expr) -> &syn::Expr {
    let mut cur = e;
    while let syn::Expr::Paren(p) = cur {
        cur = &p.expr;
    }
    cur
}

// --- Rust intra-procedural dataflow lift (syn). The lift is two rules:
//   (1) post-order, every value-bearing child expression flows into its
//       value-bearing parent (args into a call, operands into a binop, the
//       referent into a borrow);
//   (2) storage — `let x = rhs` binds rhs -> x_slot, and a later read of x
//       flows slot -> read. Params seed the scope as pre-bound slots.
// Macros, control-flow arms, and anything syn doesn't expose as a child Expr
// get a node but no chased edges (conservative: may miss flows, never invents
// them). `df_reaches <- closure(df_edge)` does the rest on the shared SCC engine.

fn rust_dataflow_from(parsed: &syn::File, file: &str) -> DataflowFacts {
    let mut out = DataflowFacts::default();
    for item in &parsed.items {
        match item {
            Item::Fn(f) => {
                let sym = mint_sym(file, EntityKind::Function, &f.sig.ident.to_string(), None);
                flow_fn_body(&sym, &f.sig, &f.block, file, &mut out);
            }
            Item::Impl(i) => {
                let owner = primary_type(&i.self_ty);
                for ii in &i.items {
                    if let syn::ImplItem::Fn(m) = ii {
                        let sym = match &owner {
                            Some(o) => mint_sym(file, EntityKind::Method, &m.sig.ident.to_string(), Some(o)),
                            None => mint_sym(file, EntityKind::Function, &m.sig.ident.to_string(), None),
                        };
                        flow_fn_body(&sym, &m.sig, &m.block, file, &mut out);
                    }
                }
            }
            _ => {}
        }
    }
    out.nests = compute_nests(&out.nodes, &out.loops);
    out
}

/// Post-pass over the lifted graph: for every `call_res` node, find each
/// enclosing loop in the same fn (by span containment against `LoopFact`) and
/// emit one `NestFact` per (call, loop) pair. `depth` is the loop's rank in the
/// nesting: 1 for the outermost enclosing loop, 2 for the next, etc. Structured
/// loops cannot partially overlap, so sorting the enclosing set by start gives
/// the nesting order without a separate containment check. The relation
/// `nest(call_id, loop_id, depth, collection)` then composes over `call_edge`
/// to give symbolic Big-O ("depth-N over C") without resolving trip counts.
fn compute_nests(nodes: &[DfNode], loops: &[LoopFact]) -> Vec<NestFact> {
    let mut out = Vec::new();
    for n in nodes {
        // `new` nodes count too: a constructor in a loop allocates per
        // iteration, the exact cost shape nest exists to surface.
        if n.kind != "call_res" && n.kind != "new" { continue; }
        // A lifted lambda's sym is `<enclosing fn>::closure::<pos>` (chained for
        // nesting), so a call inside a closure inside a loop still counts: the
        // loop's fn either matches exactly or is a `::closure::` ancestor.
        let in_fn = |l: &LoopFact| {
            l.fn_sym == n.fn_sym
                || (n.fn_sym.starts_with(&l.fn_sym)
                    && n.fn_sym[l.fn_sym.len()..].starts_with("::closure::"))
        };
        let mut enclosing: Vec<&LoopFact> = loops.iter()
            .filter(|l| in_fn(l) && n.line >= l.start && n.line <= l.end)
            .collect();
        enclosing.sort_by_key(|l| l.start);
        for (i, l) in enclosing.iter().enumerate() {
            out.push(NestFact {
                call_id: n.id.clone(),
                loop_id: format!("{}:{}", l.file, l.start),
                depth: (i + 1) as u32,
                collection: l.collection.clone(),
            });
        }
    }
    out
}

/// Seed the scope with param nodes, then walk the body. The scope maps a var
/// name to the id of its binding node (param or `let`); a read looks it up and
/// emits slot -> read. Flat function-level scope (no block shadowing) is the
/// v0 approximation.
fn flow_fn_body(
    fn_sym: &str,
    sig: &syn::Signature,
    block: &syn::Block,
    file: &str,
    out: &mut DataflowFacts,
) {
    let mut scope: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    // Position counts only typed params (the receiver `self` is skipped), so the
    // index aligns with `type_sig`, which also drops self.
    let mut pos: u32 = 0;
    for arg in &sig.inputs {
        if let syn::FnArg::Typed(pt) = arg {
            if let syn::Pat::Ident(pi) = &*pt.pat {
                let (l, c) = (pi.ident.span().start().line as u32, pi.ident.span().start().column as u32);
                let id = push_node(out, file, l, c, "param", &pi.ident.to_string(), fn_sym);
                out.param_pos.push((id.clone(), pos));
                scope.insert(pi.ident.to_string(), id);
            }
            pos += 1;
        }
    }
    // The block's tail expression (last stmt, no semicolon) is the fn's implicit
    // return value: mint a `ret` node and flow the tail into it. `return EXPR`
    // anywhere in the body is handled in `flow_expr` (Expr::Return). The `ret`
    // node is the interprocedural sink the backward flow hop reads — a callee's
    // returned value reaches the caller's `call_res`.
    if let Some((tail, l, c)) = flow_block(block, file, fn_sym, &mut scope, out) {
        let ret = push_node(out, file, l, c, "ret", "", fn_sym);
        out.edges.push(DfEdge { from: tail, to: ret });
    }
}

/// Walk a block. Returns the (id, line, col) of the block's tail value — the last
/// statement when it is a no-semicolon expression — so a caller (a fn body) can
/// treat it as an implicit return. Nested-block callers ignore the result.
fn flow_block(
    b: &syn::Block,
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) -> Option<(String, u32, u32)> {
    let mut tail = None;
    let n = b.stmts.len();
    for (idx, stmt) in b.stmts.iter().enumerate() {
        match stmt {
            syn::Stmt::Local(loc) => {
                if let Some(init) = loc.init.as_ref() {
                    let rhs = flow_expr(&init.expr, file, fn_sym, scope, out);
                    // bind every ident in the pattern (handles `let (a, b) = pair`),
                    // each tainted by the rhs conservatively.
                    for (_, bid) in bind_pat(&loc.pat, file, fn_sym, scope, out) {
                        out.edges.push(DfEdge { from: rhs.clone(), to: bid });
                    }
                }
            }
            syn::Stmt::Expr(e, semi) => {
                let start = e.span().start();
                let id = flow_expr(e, file, fn_sym, scope, out);
                if idx + 1 == n && semi.is_none() {
                    tail = Some((id, start.line as u32, start.column as u32));
                }
            }
            syn::Stmt::Item(_) => {}
            syn::Stmt::Macro(_) => {}
        }
    }
    tail
}

/// Mint a node and return its id. Free helper (not a closure) so the recursive
/// `flow_expr` calls can borrow `out` without holding a second `&mut` alive. The
/// id is `file:line:col:kind`: a parent expression and its first child share a
/// start position (e.g. `a + 1` starts where `a` starts), so the kind suffix
/// disambiguates them — every lifted node is a distinct (position, kind) pair.
fn push_node(
    out: &mut DataflowFacts,
    file: &str,
    line: u32,
    col: u32,
    kind: &str,
    var: &str,
    fn_sym: &str,
) -> String {
    let id = format!("{file}:{line}:{col}:{kind}");
    out.nodes.push(DfNode {
        id: id.clone(),
        kind: kind.into(),
        var: var.into(),
        fn_sym: fn_sym.into(),
        file: file.into(),
        line,
    });
    id
}

/// A call expression whose callee is a bare path with a capitalized last
/// segment is a tuple-struct or enum-variant constructor (`Foo(x)`,
/// `Some(x)`, `mod::Variant(x)`) under Rust naming convention — functions are
/// snake_case. Returns the constructed type/variant name, or None for an
/// ordinary call. `Foo::new(x)` stays a call: `new` is lowercase.
fn ctor_name(e: &syn::Expr) -> Option<String> {
    if let syn::Expr::Path(p) = e {
        let last = p.path.segments.last()?.ident.to_string();
        if last.chars().next().is_some_and(|c| c.is_uppercase()) {
            return Some(last);
        }
    }
    None
}

/// A call whose callee is a collection constructor (`Vec::new`, `HashMap::new`,
/// `String::new`, ...) marks its enclosing fn as allocating — the cost signal
/// for the loop-invariant-call flag. Conservative: catches the common shapes,
/// may miss ad-hoc allocators behind wrappers or macros.
fn is_allocator_call(e: &syn::Expr) -> bool {
    if let syn::Expr::Path(p) = e {
        let segs: Vec<String> = p.path.segments.iter().map(|s| s.ident.to_string()).collect();
        let full = segs.join("::");
        if full.ends_with("::new") {
            return segs.iter().any(|s| matches!(s.as_str(),
                "Vec" | "HashMap" | "BTreeMap" | "HashSet" | "BTreeSet" | "VecDeque"
                | "String" | "LinkedList"));
        }
        if matches!(full.as_str(), "Vec::with_capacity" | "HashMap::with_capacity" | "String::with_capacity") {
            return true;
        }
    }
    false
}

/// A method that builds a fresh owned value per call: `.collect()`, `.to_vec()`,
/// `.to_string()`, `.to_owned()`, `.clone()`. Conservative — `.clone()` of a
/// cheap-Copy type does not allocate, but the false positive is benign for a
/// suspect-list filter.
fn is_allocator_method(ident: &syn::Ident) -> bool {
    matches!(ident.to_string().as_str(),
        "collect" | "to_vec" | "to_string" | "to_owned" | "clone" | "format")
}

/// Post-order value flow for one expression. Returns the node id for `e` and
/// emits every internal edge as a side effect.
fn flow_expr(
    e: &syn::Expr,
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) -> String {
    let start = e.span().start();
    let (line, col) = (start.line as u32, start.column as u32);
    match e {
        // a read of a variable: flow from its binding slot to this read.
        syn::Expr::Path(p) => {
            let name = p.path.segments.last().map(|s| s.ident.to_string()).unwrap_or_default();
            let id = push_node(out, file, line, col, "var_read", &name, fn_sym);
            if let Some(b) = scope.get(&name) {
                out.edges.push(DfEdge { from: b.clone(), to: id.clone() });
            }
            id
        }
        syn::Expr::Lit(lit_expr) => {
            let id = push_node(out, file, line, col, "lit", "", fn_sym);
            if let syn::Lit::Str(s) = &lit_expr.lit {
                out.lits.push((id.clone(), s.value(), "lit"));
            }
            id
        }
        // f(args): each argument flows into the call result, and `df_arg`
        // records its 0-based slot so the interprocedural hop can join it
        // against `df_param`/`type_sig` by position. A capitalized last path
        // segment is a tuple-struct / enum-variant constructor (`Foo(x)`,
        // `Some(x)`) — those become `new` nodes carrying the type name, since
        // they build a value rather than resolve through the call graph.
        syn::Expr::Call(c) => {
            if is_allocator_call(&c.func) { out.allocators.insert(fn_sym.to_string()); }
            let ctor = ctor_name(&c.func);
            let mut children = Vec::new();
            for arg in &c.args {
                children.push(flow_expr(arg, file, fn_sym, scope, out));
            }
            let (kind, var) = match &ctor {
                Some(n) => ("new", n.as_str()),
                None => ("call_res", ""),
            };
            let id = push_node(out, file, line, col, kind, var, fn_sym);
            for (pos, child) in children.into_iter().enumerate() {
                out.edges.push(DfEdge { from: child.clone(), to: id.clone() });
                out.args.push((id.clone(), pos as i64, child));
            }
            id
        }
        // recv.m(args): receiver + args flow into the result; method name
        // skipped. The receiver is `df_arg` slot -1 (mirroring the skipped
        // `self` in `df_param`), args count 0.. so they align with the
        // callee's typed params.
        syn::Expr::MethodCall(m) => {
            if is_allocator_method(&m.method) { out.allocators.insert(fn_sym.to_string()); }
            let recv = flow_expr(&m.receiver, file, fn_sym, scope, out);
            let mut children = Vec::new();
            for arg in &m.args {
                children.push(flow_expr(arg, file, fn_sym, scope, out));
            }
            // The node sits at the METHOD ident, not the receiver expression's
            // start — the same line the call-site extractor records, so the
            // (file, line) call_node join holds for a multiline builder chain.
            let msp = m.method.span().start();
            let id = push_node(out, file, msp.line as u32, msp.column as u32, "call_res", "", fn_sym);
            out.edges.push(DfEdge { from: recv.clone(), to: id.clone() });
            out.args.push((id.clone(), -1, recv));
            for (pos, child) in children.into_iter().enumerate() {
                out.edges.push(DfEdge { from: child.clone(), to: id.clone() });
                out.args.push((id.clone(), pos as i64, child));
            }
            id
        }
        // `Foo { a: x, ..base }`: an instantiation. Each field value flows into
        // the `new` node and `df_field` records which field it fills — the
        // field-sensitive half the blanket edge can't express. A functional-
        // update base flows in under the pseudo-field "..".
        syn::Expr::Struct(s) => {
            let ty = s.path.segments.last().map(|sg| sg.ident.to_string()).unwrap_or_default();
            let mut filled: Vec<(String, String)> = Vec::new();
            for f in &s.fields {
                let v = flow_expr(&f.expr, file, fn_sym, scope, out);
                let name = match &f.member {
                    syn::Member::Named(i) => i.to_string(),
                    syn::Member::Unnamed(i) => i.index.to_string(),
                };
                filled.push((name, v));
            }
            let base = s.rest.as_ref().map(|r| flow_expr(r, file, fn_sym, scope, out));
            let id = push_node(out, file, line, col, "new", &ty, fn_sym);
            for (name, v) in filled {
                out.edges.push(DfEdge { from: v.clone(), to: id.clone() });
                out.fields.push((id.clone(), name, v));
            }
            if let Some(b) = base {
                out.edges.push(DfEdge { from: b.clone(), to: id.clone() });
                out.fields.push((id.clone(), "..".into(), b));
            }
            id
        }
        // `base.f` / `tuple.0`: a field read. The base flows into a `member`
        // node whose var is the field name, so a query can match a `df_field`
        // write against the read of the same field (field-sensitive flow).
        syn::Expr::Field(f) => {
            let base = flow_expr(&f.base, file, fn_sym, scope, out);
            let name = match &f.member {
                syn::Member::Named(i) => i.to_string(),
                syn::Member::Unnamed(i) => i.index.to_string(),
            };
            let id = push_node(out, file, line, col, "member", &name, fn_sym);
            out.edges.push(DfEdge { from: base, to: id.clone() });
            id
        }
        syn::Expr::Paren(p) => flow_expr(&p.expr, file, fn_sym, scope, out),
        syn::Expr::Reference(r) => {
            let inner = flow_expr(&r.expr, file, fn_sym, scope, out);
            let id = push_node(out, file, line, col, "borrow", "", fn_sym);
            out.edges.push(DfEdge { from: inner, to: id.clone() });
            id
        }
        syn::Expr::Binary(b) => {
            let l = flow_expr(&b.left, file, fn_sym, scope, out);
            let r = flow_expr(&b.right, file, fn_sym, scope, out);
            let id = push_node(out, file, line, col, "binop", "", fn_sym);
            out.edges.push(DfEdge { from: l, to: id.clone() });
            out.edges.push(DfEdge { from: r, to: id.clone() });
            id
        }
        syn::Expr::Unary(u) => {
            let inner = flow_expr(&u.expr, file, fn_sym, scope, out);
            let id = push_node(out, file, line, col, "unop", "", fn_sym);
            out.edges.push(DfEdge { from: inner, to: id.clone() });
            id
        }
        // transparent pass-through: the ? operator does not alter value flow.
        syn::Expr::Try(t) => flow_expr(&t.expr, file, fn_sym, scope, out),
        // `return EXPR`: the returned value flows into the fn's `ret` node — the
        // sink the interprocedural backward hop reads.
        syn::Expr::Return(r) => {
            let id = push_node(out, file, line, col, "ret", "", fn_sym);
            if let Some(inner) = &r.expr {
                let v = flow_expr(inner, file, fn_sym, scope, out);
                out.edges.push(DfEdge { from: v, to: id.clone() });
            }
            id
        }
        // `for pat in coll { body }`: bind the loop variable from the collection,
        // record the loop span so loop_over can flag loop-invariant calls inside
        // it, then walk the body. Each element taints the loop var conservatively.
        syn::Expr::ForLoop(f) => {
            let coll = flow_expr(&f.expr, file, fn_sym, scope, out);
            let binds = bind_pat(&f.pat, file, fn_sym, scope, out);
            // the whole collection taints each bound element conservatively
            // (a tuple element derives from the iterator's yield value).
            for (_, bid) in &binds {
                out.edges.push(DfEdge { from: coll.clone(), to: bid.clone() });
            }
            let lvar = binds.first().map(|(n, _)| n.clone()).unwrap_or_default();
            let end = f.body.span().end().line as u32;
            out.loops.push(LoopFact {
                file: file.into(), start: line, end,
                var: lvar.clone(),
                collection: String::new(),
                fn_sym: fn_sym.into(),
            });
            flow_block(&f.body, file, fn_sym, scope, out);
            push_node(out, file, line, col, "loop", &lvar, fn_sym)
        }
        // `while cond { body }`: `while let` is ExprWhile with cond = Expr::Let.
        // No collection, but the span is still recorded so calls in the body can
        // be flagged.
        syn::Expr::While(w) => {
            let _ = flow_expr(&w.cond, file, fn_sym, scope, out);
            if let syn::Expr::Let(l) = &*w.cond { let _ = bind_pat(&l.pat, file, fn_sym, scope, out); }
            let end = w.body.span().end().line as u32;
            out.loops.push(LoopFact { file: file.into(), start: line, end, var: String::new(), collection: String::new(), fn_sym: fn_sym.into() });
            flow_block(&w.body, file, fn_sym, scope, out);
            push_node(out, file, line, col, "loop", "", fn_sym)
        }
        syn::Expr::Loop(l) => {
            let end = l.body.span().end().line as u32;
            out.loops.push(LoopFact { file: file.into(), start: line, end, var: String::new(), collection: String::new(), fn_sym: fn_sym.into() });
            flow_block(&l.body, file, fn_sym, scope, out);
            push_node(out, file, line, col, "loop", "", fn_sym)
        }
        // `if cond { then } else { els }`: flow each branch; taint is the union.
        syn::Expr::If(i) => {
            let _ = flow_expr(&i.cond, file, fn_sym, scope, out);
            flow_block(&i.then_branch, file, fn_sym, scope, out);
            if let Some((_, els)) = &i.else_branch {
                let _ = flow_expr(els, file, fn_sym, scope, out);
            }
            push_node(out, file, line, col, "if", "", fn_sym)
        }
        // `match scrut { arms }`: scrut + each arm body; guards too. Arm-bound
        // patterns (`Stmt::Expr(e) => ...`) derive from the scrutinee, so each is
        // tainted by it — this is what makes match-bound vars track as loop-carried
        // when the scrutinee is the loop variable.
        syn::Expr::Match(m) => {
            let scrut = flow_expr(&m.expr, file, fn_sym, scope, out);
            for arm in &m.arms {
                for (_, bid) in bind_pat(&arm.pat, file, fn_sym, scope, out) {
                    out.edges.push(DfEdge { from: scrut.clone(), to: bid });
                }
                if let Some((_, g)) = &arm.guard { let _ = flow_expr(g, file, fn_sym, scope, out); }
                let _ = flow_expr(&arm.body, file, fn_sym, scope, out);
            }
            push_node(out, file, line, col, "match", "", fn_sym)
        }
        // `{ stmts }` as an expression: reuse the block walker.
        syn::Expr::Block(b) => {
            flow_block(&b.block, file, fn_sym, scope, out);
            push_node(out, file, line, col, "block", "", fn_sym)
        }
        // `|params| body`: lift the lambda as its OWN fn scope — kind "param"
        // nodes with df_param slots, body walked under the lambda sym, the body
        // result flowing into a "ret" node — so a higher-order hop (see
        // std/flow.dl flow_lambda) can feed its params and read its result. The
        // `closure` VALUE node stays in the enclosing fn (it is the argument a
        // df_arg row records) and carries the lambda sym in `var`, the join key
        // between the value and its lifted scope. The enclosing scope is shared,
        // so captures still resolve (a read of an outer var links to its slot).
        syn::Expr::Closure(c) => {
            let lam_sym = format!("{fn_sym}::closure::{line}_{col}");
            let mut pos: u32 = 0;
            for inp in &c.inputs {
                // `|x|` is Pat::Ident; `|x: T|` wraps it in Pat::Type. Either
                // way the single-ident case gets a positional param node;
                // destructuring patterns bind without a slot (conservative).
                let ident_pat = match inp {
                    syn::Pat::Type(pt) => pt.pat.as_ref(),
                    other => other,
                };
                if let syn::Pat::Ident(pi) = ident_pat {
                    let sp = pi.ident.span().start();
                    let id = push_node(out, file, sp.line as u32, sp.column as u32, "param", &pi.ident.to_string(), &lam_sym);
                    out.param_pos.push((id.clone(), pos));
                    scope.insert(pi.ident.to_string(), id);
                } else {
                    let _ = bind_pat(inp, file, &lam_sym, scope, out);
                }
                pos += 1;
            }
            let body_val = match c.body.as_ref() {
                syn::Expr::Block(b) => flow_block(&b.block, file, &lam_sym, scope, out),
                other => {
                    let sp = other.span().start();
                    Some((flow_expr(other, file, &lam_sym, scope, out), sp.line as u32, sp.column as u32))
                }
            };
            if let Some((v, l, cl)) = body_val {
                let ret = push_node(out, file, l, cl, "ret", "", &lam_sym);
                out.edges.push(DfEdge { from: v, to: ret });
            }
            push_node(out, file, line, col, "closure", &lam_sym, fn_sym)
        }
        // `lhs = rhs`: flow rhs, rebind a write slot so later reads see the new
        // value (taint-correct for reassignment). Compound assignment (`+=`) and
        // macros fall through to the conservative default below.
        syn::Expr::Assign(a) => assign_flow(&a.left, &a.right, file, line, col, fn_sym, scope, out),
        // macros (format!/println!), verbatim, and remaining variants: syn exposes
        // these as token streams or non-Expr children, so mint a node but don't
        // chase. Conservative — may miss flows into macro args, never invents.
        _ => push_node(out, file, line, col, "expr", "", fn_sym),
    }
}

/// Bind every identifier in a pattern into scope, returning `(name, bind_id)`
/// for each. Handles the common single-ident case plus tuple / tuple-struct /
/// struct / reference / paren destructuring — so `for (r, cs) in iter` and
/// `let (a, b) = pair` bind both elements, not just the outer pattern. Wildcards
/// and literals in patterns bind nothing.
fn bind_pat(
    pat: &syn::Pat,
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) -> Vec<(String, String)> {
    let mut acc = Vec::new();
    bind_pat_rec(pat, file, fn_sym, scope, out, &mut acc);
    acc
}

fn bind_pat_rec(
    pat: &syn::Pat,
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
    acc: &mut Vec<(String, String)>,
) {
    match pat {
        syn::Pat::Ident(pi) => {
            let (l, c) = (pi.ident.span().start().line as u32, pi.ident.span().start().column as u32);
            let bind = push_node(out, file, l, c, "let_bind", &pi.ident.to_string(), fn_sym);
            scope.insert(pi.ident.to_string(), bind.clone());
            acc.push((pi.ident.to_string(), bind));
        }
        syn::Pat::Tuple(t) => {
            for e in &t.elems { bind_pat_rec(e, file, fn_sym, scope, out, acc); }
        }
        syn::Pat::TupleStruct(ts) => {
            for e in &ts.elems { bind_pat_rec(e, file, fn_sym, scope, out, acc); }
        }
        syn::Pat::Struct(s) => {
            for f in &s.fields { bind_pat_rec(&f.pat, file, fn_sym, scope, out, acc); }
        }
        syn::Pat::Reference(r) => bind_pat_rec(&r.pat, file, fn_sym, scope, out, acc),
        syn::Pat::Paren(p) => bind_pat_rec(&p.pat, file, fn_sym, scope, out, acc),
        syn::Pat::Slice(s) => {
            for e in &s.elems { bind_pat_rec(e, file, fn_sym, scope, out, acc); }
        }
        _ => {}
    }
}

/// `lhs = rhs`: flow the rhs; if the lhs is a bare path, mint a var_write slot,
/// edge rhs -> slot, and rebind in scope so later reads pick up the new reaching
/// def (taint-correct under reassignment).
fn assign_flow(
    lhs: &syn::Expr,
    rhs: &syn::Expr,
    file: &str,
    line: u32,
    col: u32,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) -> String {
    let r = flow_expr(rhs, file, fn_sym, scope, out);
    if let syn::Expr::Path(p) = lhs {
        if let Some(name) = p.path.segments.last().map(|s| s.ident.to_string()) {
            let id = push_node(out, file, line, col, "var_write", &name, fn_sym);
            out.edges.push(DfEdge { from: r.clone(), to: id.clone() });
            scope.insert(name, id.clone());
            return id;
        }
    }
    r
}

// --- Kotlin entity pass (tree-sitter): declared types plus functions, the
// latter carrying their arrow `[...A] => B` like Rust/TS. Line is the node's
// row. ---

fn walk_kotlin_entities(node: tree_sitter::Node, src: &[u8], file: &str, out: &mut Vec<TypeEntity>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        // `companion_object` is a distinct grammar node from `object_declaration`
        // (a top-level/nested `object Name { ... }`); both mint a `type_entity`
        // the same way a plain class does.
        if matches!(child.kind(), "class_declaration" | "object_declaration" | "companion_object") {
            let mut c = child.walk();
            let kids: Vec<tree_sitter::Node> = child.children(&mut c).collect();
            if let Some(id) = kids.iter().find(|n| n.kind() == "type_identifier") {
                let name = id.utf8_text(src).unwrap_or("").to_string();
                let kind = if kids.iter().any(|n| n.kind() == "interface") {
                    EntityKind::Interface
                } else if kids.iter().any(|n| n.kind() == "enum") {
                    EntityKind::Enum
                } else {
                    EntityKind::Class
                };
                out.push(TypeEntity {
                    sym: mint_sym(file, kind, &name, None),
                    name,
                    kind,
                    parent: None,
                    file: file.to_string(),
                    line: (child.start_position().row + 1) as u32,
                    ty: None,
                });
            }
        } else if child.kind() == "function_declaration" {
            // top-level / member `fun name(...)`; the name is a simple_identifier
            let mut c = child.walk();
            let kids: Vec<tree_sitter::Node> = child.children(&mut c).collect();
            if let Some(id) = kids.iter().find(|n| n.kind() == "simple_identifier") {
                let name = id.utf8_text(src).unwrap_or("").to_string();
                out.push(TypeEntity {
                    sym: mint_sym(file, EntityKind::Function, &name, None),
                    name,
                    kind: EntityKind::Function,
                    parent: None,
                    file: file.to_string(),
                    line: (child.start_position().row + 1) as u32,
                    ty: Some(kotlin_fn_type(child, src)),
                });
            }
        }
        walk_kotlin_entities(child, src, file, out);
    }
}

/// Doc-comment pass (tree-sitter): the KDoc `/** */` that immediately precedes a
/// class/object/function declaration is its previous sibling (annotations and
/// modifiers are children of the decl, so they don't sit between). Same sym as
/// `walk_kotlin_entities`. Tags via the shared JSDoc/KDoc splitter.
fn walk_kotlin_docs(node: tree_sitter::Node, src: &[u8], file: &str, out: &mut Vec<DocFact>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let named = if matches!(child.kind(), "class_declaration" | "object_declaration") {
            let mut c = child.walk();
            let kids: Vec<tree_sitter::Node> = child.children(&mut c).collect();
            kids.iter().find(|n| n.kind() == "type_identifier").map(|id| {
                let kind = if kids.iter().any(|n| n.kind() == "interface") {
                    EntityKind::Interface
                } else if kids.iter().any(|n| n.kind() == "enum") {
                    EntityKind::Enum
                } else {
                    EntityKind::Class
                };
                (id.utf8_text(src).unwrap_or("").to_string(), kind)
            })
        } else if child.kind() == "function_declaration" {
            let mut c = child.walk();
            let kids: Vec<tree_sitter::Node> = child.children(&mut c).collect();
            kids.iter().find(|n| n.kind() == "simple_identifier")
                .map(|id| (id.utf8_text(src).unwrap_or("").to_string(), EntityKind::Function))
        } else {
            None
        };
        if let Some((name, kind)) = named {
            if let Some(text) = kotlin_leading_kdoc(child, src) {
                out.push(DocFact {
                    sym: mint_sym(file, kind, &name, None),
                    line: (child.start_position().row + 1) as u32,
                    tags: parse_jsdoc_tags(&text),
                    text,
                });
            }
        }
        walk_kotlin_docs(child, src, file, out);
    }
}

/// The cleaned KDoc block directly above `node`, or None. A KDoc is a
/// `*comment*` previous sibling whose text opens with `/**`.
fn kotlin_leading_kdoc(node: tree_sitter::Node, src: &[u8]) -> Option<String> {
    let prev = node.prev_sibling()?;
    if !prev.kind().contains("comment") { return None; }
    let raw = prev.utf8_text(src).ok()?;
    if !raw.trim_start().starts_with("/**") { return None; }
    Some(clean_block_comment(raw))
}

/// Build the arrow `[...A] => B` for a `fun`: each `parameter` under
/// `function_value_parameters` becomes a slot of its referenced type names
/// (declared type-param names and Kotlin builtins excluded), and the return
/// type node after the parameter list fills `ret`. A function with no declared
/// return type leaves `ret` empty (Unit), matching the keyword-slot convention.
fn kotlin_fn_type(node: tree_sitter::Node, src: &[u8]) -> TypeExpr {
    let mut cursor = node.walk();
    let children: Vec<tree_sitter::Node> = node.children(&mut cursor).collect();

    // declared type-parameter names: excluded from refs, like the decl pass
    let mut tparams: BTreeSet<String> = BTreeSet::new();
    for n in &children {
        if n.kind() != "type_parameters" { continue; }
        let mut c = n.walk();
        for tp in n.children(&mut c).filter(|n| n.kind() == "type_parameter") {
            let mut cc = tp.walk();
            let kids: Vec<tree_sitter::Node> = tp.children(&mut cc).collect();
            if let Some(name) = kids.iter().find(|n| n.kind() == "type_identifier") {
                tparams.insert(name.utf8_text(src).unwrap_or("").to_string());
            }
        }
    }

    let named = |refs: Vec<String>| refs.into_iter().map(TypeRef::Named).collect::<Vec<_>>();
    let mut params = Vec::new();
    let mut ret = Vec::new();
    for n in &children {
        match n.kind() {
            "function_value_parameters" => {
                let mut c = n.walk();
                for p in n.children(&mut c).filter(|n| n.kind() == "parameter") {
                    // the parameter's name is a simple_identifier (not collected,
                    // collect_kotlin_refs only reads user_type); its type recurses
                    params.push(named(kotlin_type_refs(p, src, &tparams)));
                }
            }
            // the return type is a type-node sibling after the parameter list
            k if is_kotlin_type_node(k) => ret = named(kotlin_type_refs(*n, src, &tparams)),
            _ => {}
        }
    }
    TypeExpr { params, ret }
}

fn is_kotlin_type_node(kind: &str) -> bool {
    matches!(
        kind,
        "user_type" | "nullable_type" | "function_type" | "parenthesized_type"
    )
}

// ── Go (tree-sitter) ─────────────────────────────────────────────────────────
//
// Same diet-extractor contract as Rust/Kotlin/TS, but tree-sitter-go's grammar
// exposes named FIELDS on every node that matters (`name`, `type`, `parameters`,
// `receiver`, `result`, ...), so this front end reads structured fields instead
// of Kotlin's manual child-kind scanning — closer in spirit to Rust's syn
// AST, just via tree-sitter. Method receivers carry their type in the syntax
// (`func (r *Repo) Name()`), so method -> owner parenting is deterministic; one
// package per directory means module resolution never needs symbol-level
// disambiguation (see `GoResolver` in modgraph.rs). Kind vocabulary: struct
// field types (named) -> `field`; an EMBEDDED struct/interface type (no field
// name, or interface `type_elem`) -> `impl`; a declared type parameter's
// constraint -> `generic`. NON-GOALS (syntactic tier, honest): implicit
// interface-satisfaction edges (method-set computation is a heuristic, not
// attempted), cgo, build-tag-conditional files, cross-module resolution
// outside the workspace.

fn go_parse(content: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    let lang = tree_sitter::Language::new(tree_sitter_go::LANGUAGE);
    parser.set_language(&lang).ok()?;
    parser.parse(content, None)
}

fn go_text<'a>(node: tree_sitter::Node, src: &'a [u8]) -> &'a str {
    node.utf8_text(src).unwrap_or("")
}

fn is_noise_go(name: &str) -> bool {
    matches!(
        name,
        "int" | "int8" | "int16" | "int32" | "int64"
            | "uint" | "uint8" | "uint16" | "uint32" | "uint64" | "uintptr"
            | "float32" | "float64" | "complex64" | "complex128"
            | "bool" | "string" | "byte" | "rune" | "error" | "any" | "comparable"
    )
}

/// Collect the named type references anywhere under `node`. A `qualified_type`
/// (`pkg.Type`) is one ref, kept as `pkg.Type` (the package qualifier stays —
/// unlike Kotlin's fully-dotted package path this is just the two segments
/// tree-sitter-go exposes) and NOT recursed into further (its own `name` field
/// is a `type_identifier` that would otherwise double-count). A bare
/// `type_identifier` is a ref unless it names a declared type parameter or a
/// predeclared/builtin type.
fn go_type_refs(node: tree_sitter::Node, src: &[u8], params: &BTreeSet<String>) -> Vec<String> {
    let mut out = Vec::new();
    collect_go_refs(node, src, params, &mut out);
    out.sort();
    out.dedup();
    out
}

fn collect_go_refs(node: tree_sitter::Node, src: &[u8], params: &BTreeSet<String>, out: &mut Vec<String>) {
    match node.kind() {
        "type_identifier" => {
            let name = go_text(node, src).to_string();
            if !params.contains(&name) && !is_noise_go(&name) {
                out.push(name);
            }
        }
        "qualified_type" => {
            let pkg = node.child_by_field_name("package").map(|n| go_text(n, src)).unwrap_or("");
            let name = node.child_by_field_name("name").map(|n| go_text(n, src)).unwrap_or("");
            if !pkg.is_empty() && !name.is_empty() {
                out.push(format!("{pkg}.{name}"));
            }
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_go_refs(child, src, params, out);
            }
        }
    }
}

/// The textual name of a composite literal's element type, for the `new`
/// dataflow node's `var`: a bare/qualified named type keeps its name; an
/// anonymous array/slice/map/struct literal type has no name (`""`).
fn go_type_name_text(node: tree_sitter::Node, src: &[u8]) -> String {
    match node.kind() {
        "type_identifier" => go_text(node, src).to_string(),
        "qualified_type" => node.child_by_field_name("name").map(|n| go_text(n, src).to_string()).unwrap_or_default(),
        "generic_type" => node.child_by_field_name("type").map(|t| go_type_name_text(t, src)).unwrap_or_default(),
        _ => String::new(),
    }
}

/// A method's receiver base type name, `*`/generic-args stripped
/// (`(r *Repo[T])` -> `"Repo"`). None for a malformed/absent receiver.
fn go_receiver_type(method: tree_sitter::Node, src: &[u8]) -> Option<String> {
    let recv_list = method.child_by_field_name("receiver")?;
    let mut cursor = recv_list.walk();
    let param = recv_list.children(&mut cursor).find(|n| n.kind() == "parameter_declaration")?;
    let mut ty = param.child_by_field_name("type")?;
    loop {
        match ty.kind() {
            "pointer_type" => ty = ty.named_child(0)?,
            "generic_type" => ty = ty.child_by_field_name("type")?,
            _ => break,
        }
    }
    match ty.kind() {
        "type_identifier" => Some(go_text(ty, src).to_string()),
        "qualified_type" => ty.child_by_field_name("name").map(|n| go_text(n, src).to_string()),
        _ => None,
    }
}

/// First-pass file-local owner-kind lookup (mirrors Rust's `rust_owner_kinds`):
/// for each package-level `type X struct{}`/`interface{}` declared in THIS
/// file, record its real `EntityKind` so a same-file method's receiver mints
/// the correctly-kinded parent sym. A method whose receiver type is declared
/// in a DIFFERENT file (common — Go methods are routinely split across files
/// in one package) defaults to `Struct`; the engine's cross-file owner-name
/// resolution (same as Rust) still finds the real declaring sym, kind-agnostic.
fn go_owner_kinds(root: tree_sitter::Node, src: &[u8]) -> std::collections::HashMap<String, EntityKind> {
    let mut out = std::collections::HashMap::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() != "type_declaration" { continue; }
        let mut c2 = child.walk();
        for spec in child.children(&mut c2) {
            if spec.kind() != "type_spec" { continue; }
            let Some(name) = spec.child_by_field_name("name") else { continue };
            let kind = match spec.child_by_field_name("type").map(|t| t.kind()) {
                Some("interface_type") => EntityKind::Interface,
                Some("struct_type") => EntityKind::Struct,
                _ => EntityKind::Alias,
            };
            out.insert(go_text(name, src).to_string(), kind);
        }
    }
    out
}

// --- Go entity pass: struct/interface/alias type declarations, functions, and
// methods (parent = receiver base type, real-kinded via `go_owner_kinds`). ---

fn walk_go_entities(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    owners: &std::collections::HashMap<String, EntityKind>,
    out: &mut Vec<TypeEntity>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "type_declaration" => {
                let mut c2 = child.walk();
                for spec in child.children(&mut c2) {
                    let (name_node, kind) = match spec.kind() {
                        "type_spec" => {
                            let k = match spec.child_by_field_name("type").map(|t| t.kind()) {
                                Some("struct_type") => EntityKind::Struct,
                                Some("interface_type") => EntityKind::Interface,
                                _ => EntityKind::Alias,
                            };
                            (spec.child_by_field_name("name"), k)
                        }
                        "type_alias" => (spec.child_by_field_name("name"), EntityKind::Alias),
                        _ => continue,
                    };
                    let Some(name_node) = name_node else { continue };
                    let name = go_text(name_node, src).to_string();
                    out.push(TypeEntity {
                        sym: mint_sym(file, kind, &name, None),
                        name,
                        kind,
                        parent: None,
                        file: file.to_string(),
                        line: (spec.start_position().row + 1) as u32,
                        ty: None,
                    });
                }
            }
            "function_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = go_text(name_node, src).to_string();
                    out.push(TypeEntity {
                        sym: mint_sym(file, EntityKind::Function, &name, None),
                        name,
                        kind: EntityKind::Function,
                        parent: None,
                        file: file.to_string(),
                        line: (child.start_position().row + 1) as u32,
                        ty: Some(go_fn_type(child, src)),
                    });
                }
            }
            "method_declaration" => {
                if let (Some(name_node), Some(owner_name)) =
                    (child.child_by_field_name("name"), go_receiver_type(child, src))
                {
                    let name = go_text(name_node, src).to_string();
                    let owner_kind = owners.get(&owner_name).copied().unwrap_or(EntityKind::Struct);
                    out.push(TypeEntity {
                        sym: mint_sym(file, EntityKind::Method, &name, Some(&owner_name)),
                        name,
                        kind: EntityKind::Method,
                        parent: Some(mint_sym(file, owner_kind, &owner_name, None)),
                        file: file.to_string(),
                        line: (child.start_position().row + 1) as u32,
                        ty: Some(go_fn_type(child, src)),
                    });
                }
            }
            _ => {}
        }
        walk_go_entities(child, src, file, owners, out);
    }
}

/// Build the arrow `[...A] => B` for a `function_declaration`/`method_declaration`.
/// The receiver (methods only) is never read here, so params stay aligned with
/// the written argument list — same convention as Rust dropping `self`. A
/// grouped parameter (`a, b int`) is ONE grammar node but TWO positional
/// params, so each declared name gets its own slot sharing the group's type.
/// Go's multi-value return has no per-slot structure in `type_sig` (which
/// stores one flat `ret` list at position 0 regardless of language, see
/// `refresh_type_rels`): every result type's refs are unioned into that one
/// list rather than kept per-slot. A caller wanting per-return precision reads
/// `df_arg`/the dataflow `ret` nodes, not `type_sig`, for a multi-return fn.
fn go_fn_type(node: tree_sitter::Node, src: &[u8]) -> TypeExpr {
    let named = |refs: Vec<String>| refs.into_iter().map(TypeRef::Named).collect::<Vec<_>>();
    let mut tparams: BTreeSet<String> = BTreeSet::new();
    if let Some(tp_list) = node.child_by_field_name("type_parameters") {
        let mut cursor = tp_list.walk();
        for tp in tp_list.children(&mut cursor).filter(|n| n.kind() == "type_parameter_declaration") {
            let mut cc = tp.walk();
            for n in tp.children(&mut cc) {
                if n.kind() == "identifier" {
                    tparams.insert(go_text(n, src).to_string());
                }
            }
        }
    }
    let mut params = Vec::new();
    if let Some(plist) = node.child_by_field_name("parameters") {
        let mut cursor = plist.walk();
        for p in plist.children(&mut cursor) {
            if !matches!(p.kind(), "parameter_declaration" | "variadic_parameter_declaration") { continue; }
            let Some(ty) = p.child_by_field_name("type") else { continue };
            let mut nc = p.walk();
            let count = p.children(&mut nc).filter(|n| n.kind() == "identifier").count().max(1);
            for _ in 0..count {
                params.push(named(go_type_refs(ty, src, &tparams)));
            }
        }
    }
    let mut ret = Vec::new();
    if let Some(result) = node.child_by_field_name("result") {
        if result.kind() == "parameter_list" {
            let mut cursor = result.walk();
            for p in result.children(&mut cursor)
                .filter(|n| matches!(n.kind(), "parameter_declaration" | "variadic_parameter_declaration"))
            {
                if let Some(ty) = p.child_by_field_name("type") {
                    ret.extend(named(go_type_refs(ty, src, &tparams)));
                }
            }
        } else {
            ret.extend(named(go_type_refs(result, src, &tparams)));
        }
    }
    TypeExpr { params, ret }
}

// --- Go type-graph edges: struct fields (named -> `field`, embedded ->
// `impl`), interface embeds (`impl`), declared type-parameter constraints
// (`generic`). Method signatures are NOT edge sources (entity-level
// `type_sig` covers callables; type_edge is shape-only, matching Kotlin/TS). ---

fn go_edges_from(root: tree_sitter::Node, src: &[u8]) -> Vec<TypeEdge> {
    let mut out: BTreeSet<(String, String, &'static str)> = BTreeSet::new();
    walk_go_types(root, src, &mut out);
    out.into_iter().map(|(from, to, kind)| TypeEdge { from, to, kind }).collect()
}

fn walk_go_types(node: tree_sitter::Node, src: &[u8], out: &mut BTreeSet<(String, String, &'static str)>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "type_declaration" {
            let mut c2 = child.walk();
            for spec in child.children(&mut c2) {
                if spec.kind() == "type_spec" {
                    go_type_spec_edges(spec, src, out);
                }
            }
        }
        walk_go_types(child, src, out);
    }
}

fn go_type_spec_edges(spec: tree_sitter::Node, src: &[u8], out: &mut BTreeSet<(String, String, &'static str)>) {
    let Some(name_node) = spec.child_by_field_name("name") else { return };
    let owner = go_text(name_node, src).to_string();

    let mut params: BTreeSet<String> = BTreeSet::new();
    if let Some(tp_list) = spec.child_by_field_name("type_parameters") {
        let mut cursor = tp_list.walk();
        for tp in tp_list.children(&mut cursor).filter(|n| n.kind() == "type_parameter_declaration") {
            let mut cc = tp.walk();
            let kids: Vec<tree_sitter::Node> = tp.children(&mut cc).collect();
            for n in kids.iter().filter(|n| n.kind() == "identifier") {
                params.insert(go_text(*n, src).to_string());
            }
            if let Some(constraint) = tp.child_by_field_name("type") {
                for to in go_type_refs(constraint, src, &params) {
                    out.insert((owner.clone(), to, "generic"));
                }
            }
        }
    }

    let Some(ty) = spec.child_by_field_name("type") else { return };
    match ty.kind() {
        "struct_type" => {
            let mut c = ty.walk();
            let Some(list) = ty.children(&mut c).find(|n| n.kind() == "field_declaration_list") else { return };
            let mut c2 = list.walk();
            for field in list.children(&mut c2).filter(|n| n.kind() == "field_declaration") {
                let Some(ftype) = field.child_by_field_name("type") else { continue };
                let kind: &'static str = if field.child_by_field_name("name").is_some() { "field" } else { "impl" };
                for to in go_type_refs(ftype, src, &params) {
                    out.insert((owner.clone(), to, kind));
                }
            }
        }
        "interface_type" => {
            let mut c = ty.walk();
            for elem in ty.children(&mut c).filter(|n| n.kind() == "type_elem") {
                for to in go_type_refs(elem, src, &params) {
                    out.insert((owner.clone(), to, "impl"));
                }
            }
            // `method_elem` (interface method signatures) intentionally
            // skipped: no type_sig-equivalent exists for an interface's own
            // method specs at the type_edge level.
        }
        _ => {}
    }
}

// --- Go call-graph pass: `function_declaration`/`method_declaration` become
// CallDefs (method keyed to its receiver's base type, matching the entity
// pass); every `call_expression` becomes a CallSite whose callee is the bare
// name (a selector callee's trailing field name, matching the Rust/Kotlin
// trailing-segment convention). ---

fn go_walk_call_defs(node: tree_sitter::Node, src: &[u8], file: &str, out: &mut Vec<CallDef>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = go_text(name_node, src).to_string();
                    let end = child.child_by_field_name("body").unwrap_or(child).end_position().row as u32 + 1;
                    out.push(CallDef {
                        sym: mint_sym(file, EntityKind::Function, &name, None),
                        name,
                        kind: CallKind::Free,
                        file: file.to_string(),
                        line: child.start_position().row as u32 + 1,
                        end,
                    });
                }
            }
            "method_declaration" => {
                if let (Some(name_node), Some(owner)) =
                    (child.child_by_field_name("name"), go_receiver_type(child, src))
                {
                    let name = go_text(name_node, src).to_string();
                    let end = child.child_by_field_name("body").unwrap_or(child).end_position().row as u32 + 1;
                    out.push(CallDef {
                        sym: mint_sym(file, EntityKind::Method, &name, Some(&owner)),
                        name,
                        kind: CallKind::Method,
                        file: file.to_string(),
                        line: child.start_position().row as u32 + 1,
                        end,
                    });
                }
            }
            _ => {}
        }
        go_walk_call_defs(child, src, file, out);
    }
}

fn go_walk_call_sites(node: tree_sitter::Node, src: &[u8], file: &str, out: &mut Vec<CallSite>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "call_expression" {
            if let Some((callee, line)) = go_callee(child, src) {
                out.push(CallSite { caller_sym: None, callee, callee_path: None, file: file.to_string(), line });
            }
        }
        go_walk_call_sites(child, src, file, out);
    }
}

/// (callee name, 1-based call line) for a `call_expression`, or None when the
/// callee is not a plain/selector name (a type conversion `T(x)` where `T` is
/// a `type_identifier` callee is NOT skipped here — it reads as an ordinary
/// call, honest: the syntactic tier can't tell a conversion from a call).
fn go_callee(call: tree_sitter::Node, src: &[u8]) -> Option<(String, u32)> {
    let func = call.child_by_field_name("function")?;
    let line = func.start_position().row as u32 + 1;
    match func.kind() {
        "identifier" => Some((go_text(func, src).to_string(), line)),
        "selector_expression" => {
            let field = func.child_by_field_name("field")?;
            Some((go_text(field, src).to_string(), line))
        }
        _ => None,
    }
}

// --- Go doc-comment pass: the contiguous run of `//` line comments (or a
// single leading `/* */` block) immediately above a decl, godoc convention.
// Tags: only "Deprecated:" — plain godoc has no `@`-style annotations. ---

/// The cleaned doc block directly above `node`, or None. Walks BACKWARD over
/// `prev_sibling`s while each one is a `comment` node whose last line ends
/// exactly one row before the block collected so far starts (no blank-line
/// gap) — so a multi-line `// foo\n// bar` godoc block joins into one text,
/// and a comment separated by a blank line (not a doc comment, by convention)
/// is left alone.
fn go_leading_doc(node: tree_sitter::Node, src: &[u8]) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut expected_row = node.start_position().row;
    let mut cur = node.prev_sibling()?;
    loop {
        if cur.kind() != "comment" || cur.end_position().row + 1 != expected_row {
            break;
        }
        let raw = go_text(cur, src);
        if raw.trim_start().starts_with("/*") {
            lines.insert(0, clean_block_comment(raw));
            break;
        }
        let body = raw.trim_start().strip_prefix("//").unwrap_or(raw).trim_start().to_string();
        lines.insert(0, body);
        expected_row = cur.start_position().row;
        let Some(prev) = cur.prev_sibling() else { break };
        cur = prev;
    }
    if lines.is_empty() { None } else { Some(lines.join("\n")) }
}

/// godoc's one structured convention: a paragraph (blank-line-separated block)
/// starting `Deprecated:` marks the decl deprecated. No `@`-tags exist in
/// plain godoc, so this is the only tag this extractor ever emits.
fn parse_go_doc_tags(text: &str) -> Vec<DocTag> {
    let mut out = Vec::new();
    for para in text.split("\n\n") {
        if let Some(rest) = para.trim_start().strip_prefix("Deprecated:") {
            out.push(DocTag { tag: "deprecated".to_string(), arg: String::new(), text: rest.trim().to_string() });
        }
    }
    out
}

fn push_go_doc(out: &mut Vec<DocFact>, file: &str, name: &str, kind: EntityKind, line: u32, text: String) {
    out.push(DocFact { sym: mint_sym(file, kind, name, None), line, tags: parse_go_doc_tags(&text), text });
}

fn walk_go_docs(node: tree_sitter::Node, src: &[u8], file: &str, out: &mut Vec<DocFact>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "type_declaration" => {
                let mut c2 = child.walk();
                for spec in child.children(&mut c2) {
                    let (name_node, kind) = match spec.kind() {
                        "type_spec" => {
                            let k = match spec.child_by_field_name("type").map(|t| t.kind()) {
                                Some("struct_type") => EntityKind::Struct,
                                Some("interface_type") => EntityKind::Interface,
                                _ => EntityKind::Alias,
                            };
                            (spec.child_by_field_name("name"), k)
                        }
                        "type_alias" => (spec.child_by_field_name("name"), EntityKind::Alias),
                        _ => continue,
                    };
                    let Some(name_node) = name_node else { continue };
                    // Try the spec itself first (a grouped `type ( ... )` decl
                    // has its doc comment directly above the spec); a lone
                    // `type X struct{}` decl's comment sits above the whole
                    // `type_declaration` instead, so fall back to the parent.
                    let text = go_leading_doc(spec, src).or_else(|| go_leading_doc(child, src));
                    if let Some(text) = text {
                        push_go_doc(out, file, &go_text(name_node, src).to_string(), kind,
                                    spec.start_position().row as u32 + 1, text);
                    }
                }
            }
            "function_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    if let Some(text) = go_leading_doc(child, src) {
                        push_go_doc(out, file, &go_text(name_node, src).to_string(), EntityKind::Function,
                                    child.start_position().row as u32 + 1, text);
                    }
                }
            }
            "method_declaration" => {
                if let (Some(name_node), Some(owner)) =
                    (child.child_by_field_name("name"), go_receiver_type(child, src))
                {
                    if let Some(text) = go_leading_doc(child, src) {
                        let sym = mint_sym(file, EntityKind::Method, go_text(name_node, src), Some(&owner));
                        out.push(DocFact {
                            sym,
                            line: child.start_position().row as u32 + 1,
                            tags: parse_go_doc_tags(&text),
                            text,
                        });
                    }
                }
            }
            _ => {}
        }
        walk_go_docs(child, src, file, out);
    }
}

// --- Go intra-procedural dataflow lift (tree-sitter, fields). Same lift-to-
// node model as Rust/Kotlin: value-bearing subtrees mint a `DfNode`, local
// value flow becomes `DfEdge`. Unlike Rust/Kotlin, Go has no implicit tail
// return (every return is a `return_statement`), so there is no fn-level
// "wrap the tail in a ret node" step — each `return_statement` mints its own
// `ret` node(s) directly, one per returned value (multi-value return). ---

fn go_dataflow_from(root: tree_sitter::Node, src: &[u8], file: &str) -> DataflowFacts {
    let mut out = DataflowFacts::default();
    go_walk_fns(root, src, file, &mut out);
    // tree-sitter rows are 0-based; the df contract is 1-based (see Kotlin's
    // identical bump), so bump reported node lines and loop spans. Node ids
    // keep the raw 0-based row (opaque; only uniqueness matters).
    for n in &mut out.nodes { n.line += 1; }
    for l in &mut out.loops { l.start += 1; l.end += 1; }
    out.nests = compute_nests(&out.nodes, &out.loops);
    out
}

fn go_walk_fns(node: tree_sitter::Node, src: &[u8], file: &str, out: &mut DataflowFacts) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let fn_sym = mint_sym(file, EntityKind::Function, go_text(name_node, src), None);
                    go_flow_fn(child, src, file, &fn_sym, out);
                }
            }
            "method_declaration" => {
                if let (Some(name_node), Some(owner)) =
                    (child.child_by_field_name("name"), go_receiver_type(child, src))
                {
                    let fn_sym = mint_sym(file, EntityKind::Method, go_text(name_node, src), Some(&owner));
                    go_flow_fn(child, src, file, &fn_sym, out);
                }
            }
            _ => {}
        }
        go_walk_fns(child, src, file, out);
    }
}

/// Seed `param` nodes from the (non-receiver) parameter list, then walk the
/// body. A grouped parameter (`a, b int`) mints one param node PER declared
/// name, matching `go_fn_type`'s slot count; an unnamed parameter still
/// advances the position counter so later named params keep the right index.
fn go_flow_fn(fn_node: tree_sitter::Node, src: &[u8], file: &str, fn_sym: &str, out: &mut DataflowFacts) {
    let mut scope: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut pos: u32 = 0;
    if let Some(params) = fn_node.child_by_field_name("parameters") {
        let mut cursor = params.walk();
        for p in params.children(&mut cursor) {
            if !matches!(p.kind(), "parameter_declaration" | "variadic_parameter_declaration") { continue; }
            let mut nc = p.walk();
            let names: Vec<tree_sitter::Node> = p.children(&mut nc).filter(|n| n.kind() == "identifier").collect();
            if names.is_empty() { pos += 1; continue; }
            for name_node in names {
                let sp = name_node.start_position();
                let v = go_text(name_node, src).to_string();
                let id = push_node(out, file, sp.row as u32, sp.column as u32, "param", &v, fn_sym);
                out.param_pos.push((id.clone(), pos));
                scope.insert(v, id);
                pos += 1;
            }
        }
    }
    if let Some(body) = fn_node.child_by_field_name("body") {
        flow_go(body, src, file, fn_sym, &mut scope, out);
    }
}

/// Returns the node id carrying the value of this subtree, or None when the
/// subtree is a pure statement/binder handled inline (bindings, control-flow
/// headers). Unhandled node kinds fall through to `go_recurse_children`,
/// conservative — may miss a flow, never invents one.
fn flow_go(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) -> Option<String> {
    let pos = node.start_position();
    match node.kind() {
        "identifier" => {
            let v = go_text(node, src).to_string();
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "var_read", &v, fn_sym);
            if let Some(b) = scope.get(&v) {
                out.edges.push(DfEdge { from: b.clone(), to: id.clone() });
            }
            Some(id)
        }
        "interpreted_string_literal" | "raw_string_literal" | "int_literal" | "float_literal"
        | "imaginary_literal" | "rune_literal" | "true" | "false" | "nil" | "iota" => {
            Some(push_node(out, file, pos.row as u32, pos.column as u32, "lit", "", fn_sym))
        }
        // f(args): every argument flows into the call result; `df_arg` records
        // its 0-based slot. A selector callee `recv.M(args)` flows the
        // receiver in at slot -1 (mirroring the skipped receiver in
        // `df_param`), the bare method name carried on the node text-side by
        // `call_node`'s (file, line) join, not here. Go has no syntactic ctor
        // marker (capitalization means EXPORTED, not "constructor"), so every
        // call is `call_res`; instantiation rides `composite_literal` below.
        "call_expression" => {
            let func = node.child_by_field_name("function");
            let mut recv: Option<String> = None;
            if let Some(func) = func {
                if func.kind() == "selector_expression" {
                    if let Some(operand) = func.child_by_field_name("operand") {
                        recv = flow_go(operand, src, file, fn_sym, scope, out);
                    }
                }
            }
            let mut arg_ids = Vec::new();
            if let Some(args) = node.child_by_field_name("arguments") {
                let mut cursor = args.walk();
                for a in args.children(&mut cursor) {
                    if let Some(id) = flow_go(a, src, file, fn_sym, scope, out) {
                        arg_ids.push(id);
                    }
                }
            }
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "call_res", "", fn_sym);
            if let Some(r) = recv {
                out.edges.push(DfEdge { from: r.clone(), to: id.clone() });
                out.args.push((id.clone(), -1, r));
            }
            for (p, vid) in arg_ids.into_iter().enumerate() {
                out.edges.push(DfEdge { from: vid.clone(), to: id.clone() });
                out.args.push((id.clone(), p as i64, vid));
            }
            Some(id)
        }
        // `base.Field` outside a call: a member read. As a call's callee
        // (parent is the enclosing call_expression) the call arm above owns
        // it instead — receiver at slot -1, bare name on the call node.
        "selector_expression" => {
            if node.parent().map(|p| p.kind()) == Some("call_expression") {
                return None;
            }
            let operand = node.child_by_field_name("operand")
                .and_then(|o| flow_go(o, src, file, fn_sym, scope, out));
            let name = node.child_by_field_name("field").map(|n| go_text(n, src).to_string()).unwrap_or_default();
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "member", &name, fn_sym);
            if let Some(o) = operand {
                out.edges.push(DfEdge { from: o, to: id.clone() });
            }
            Some(id)
        }
        // `T{...}` / `[]T{...}` / `map[K]V{...}`: an instantiation. Each
        // element flows into the `new` node and `df_field` records which
        // field it fills (a keyed struct field's name, else the 0-based
        // positional index as a string — array/slice/map literals have no
        // field name). The key subtree of a `keyed_element` is a LABEL, never
        // walked as a read (mirrors Kotlin's named-argument convention) —
        // even though a map literal's key COULD be a real expression, the
        // syntactic tier can't tell a struct field label from a map key
        // without type info, so it is read as text only, conservative.
        "composite_literal" => {
            let type_name = node.child_by_field_name("type").map(|t| go_type_name_text(t, src)).unwrap_or_default();
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "new", &type_name, fn_sym);
            if let Some(body) = node.child_by_field_name("body") {
                go_flow_literal_fields(body, src, file, fn_sym, scope, out, &id);
            }
            Some(id)
        }
        // A `literal_value` reached directly (not via `composite_literal`):
        // a nested element literal whose type is implied by the enclosing
        // composite (`[]Foo{ {A: 1} }`'s inner `{A: 1}`). Anonymous `new`.
        "literal_value" => {
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "new", "", fn_sym);
            go_flow_literal_fields(node, src, file, fn_sym, scope, out, &id);
            Some(id)
        }
        "binary_expression" => {
            let l = node.child_by_field_name("left").and_then(|n| flow_go(n, src, file, fn_sym, scope, out));
            let r = node.child_by_field_name("right").and_then(|n| flow_go(n, src, file, fn_sym, scope, out));
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "binop", "", fn_sym);
            if let Some(l) = l { out.edges.push(DfEdge { from: l, to: id.clone() }); }
            if let Some(r) = r { out.edges.push(DfEdge { from: r, to: id.clone() }); }
            Some(id)
        }
        "unary_expression" => {
            let inner = node.child_by_field_name("operand").and_then(|n| flow_go(n, src, file, fn_sym, scope, out));
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "unop", "", fn_sym);
            if let Some(inner) = inner { out.edges.push(DfEdge { from: inner, to: id.clone() }); }
            Some(id)
        }
        // `x := rhs` (possibly multi-value): bind each declared name to a
        // fresh `let_bind` node. A matching-arity rhs pairs positionally; a
        // mismatched arity (`a, b := f()` — one call, two targets) taints
        // every target from that one rhs value conservatively.
        "short_var_declaration" => {
            let rhs_ids = node.child_by_field_name("right")
                .map(|right| go_flow_expr_list(right, src, file, fn_sym, scope, out))
                .unwrap_or_default();
            if let Some(left) = node.child_by_field_name("left") {
                let mut cursor = left.walk();
                let names: Vec<tree_sitter::Node> = left.children(&mut cursor).filter(|n| n.kind() == "identifier").collect();
                go_bind(&names, &rhs_ids, "let_bind", src, file, fn_sym, scope, out);
            }
            None
        }
        "var_declaration" | "const_declaration" => {
            let mut cursor = node.walk();
            for spec in node.children(&mut cursor).filter(|n| matches!(n.kind(), "var_spec" | "const_spec")) {
                go_flow_spec(spec, src, file, fn_sym, scope, out);
            }
            None
        }
        // `lhs = rhs` (incl. compound `+=`/etc, treated the same — a write
        // either way): rebind so later reads see the new value. Non-identifier
        // targets (`x.Field = v`, `arr[i] = v`) still flow for side-effect
        // visibility without a scope rebind.
        "assignment_statement" => {
            let rhs_ids = node.child_by_field_name("right")
                .map(|right| go_flow_expr_list(right, src, file, fn_sym, scope, out))
                .unwrap_or_default();
            if let Some(left) = node.child_by_field_name("left") {
                let mut cursor = left.walk();
                let targets: Vec<tree_sitter::Node> = left.children(&mut cursor).collect();
                let names: Vec<tree_sitter::Node> = targets.iter().filter(|n| n.kind() == "identifier").copied().collect();
                go_bind(&names, &rhs_ids, "var_write", src, file, fn_sym, scope, out);
                for t in targets.iter().filter(|n| n.kind() != "identifier" && n.kind() != ",") {
                    flow_go(*t, src, file, fn_sym, scope, out);
                }
            }
            None
        }
        // `return a, b`: one `ret` node PER returned value (multi-value
        // return), each fed by its own expression — the sink the
        // interprocedural backward hop reads. A naked `return` still mints
        // one empty `ret` node so the fn has a visible graph endpoint.
        "return_statement" => {
            let mut cursor = node.walk();
            let list = node.children(&mut cursor).find(|n| n.kind() == "expression_list");
            let mut minted = false;
            if let Some(list) = list {
                let mut c2 = list.walk();
                for e in list.children(&mut c2) {
                    if let Some(vid) = flow_go(e, src, file, fn_sym, scope, out) {
                        let rp = e.start_position();
                        let ret = push_node(out, file, rp.row as u32, rp.column as u32, "ret", "", fn_sym);
                        out.edges.push(DfEdge { from: vid, to: ret });
                        minted = true;
                    }
                }
            }
            if !minted {
                push_node(out, file, pos.row as u32, pos.column as u32, "ret", "", fn_sym);
            }
            None
        }
        "if_statement" => {
            if let Some(init) = node.child_by_field_name("initializer") {
                flow_go(init, src, file, fn_sym, scope, out);
            }
            if let Some(cond) = node.child_by_field_name("condition") {
                flow_go(cond, src, file, fn_sym, scope, out);
            }
            if let Some(cons) = node.child_by_field_name("consequence") {
                flow_go(cons, src, file, fn_sym, scope, out);
            }
            if let Some(alt) = node.child_by_field_name("alternative") {
                flow_go(alt, src, file, fn_sym, scope, out);
            }
            Some(push_node(out, file, pos.row as u32, pos.column as u32, "if", "", fn_sym))
        }
        // `for range/clause/cond { body }`: record the loop span (+ the range
        // variable, when present) so `loop_over`/`nest` see loop-invariant
        // calls inside it, then walk the body. A for_statement's non-`body`,
        // non-`for`-keyword child is at most ONE of {bare condition
        // expression, `for_clause`, `range_clause`} per the grammar.
        "for_statement" => {
            let mut lvar = String::new();
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "range_clause" => {
                        if let Some(right) = child.child_by_field_name("right") {
                            flow_go(right, src, file, fn_sym, scope, out);
                        }
                        if let Some(left) = child.child_by_field_name("left") {
                            let mut lc = left.walk();
                            let names: Vec<tree_sitter::Node> =
                                left.children(&mut lc).filter(|n| n.kind() == "identifier").collect();
                            for name_node in &names {
                                let v = go_text(*name_node, src).to_string();
                                if v == "_" { continue; }
                                let sp = name_node.start_position();
                                let id = push_node(out, file, sp.row as u32, sp.column as u32, "let_bind", &v, fn_sym);
                                scope.insert(v.clone(), id);
                                if lvar.is_empty() { lvar = v; }
                            }
                        }
                    }
                    "for_clause" => {
                        if let Some(init) = child.child_by_field_name("initializer") {
                            flow_go(init, src, file, fn_sym, scope, out);
                        }
                        if let Some(cond) = child.child_by_field_name("condition") {
                            flow_go(cond, src, file, fn_sym, scope, out);
                        }
                        if let Some(upd) = child.child_by_field_name("update") {
                            flow_go(upd, src, file, fn_sym, scope, out);
                        }
                    }
                    "block" | "for" => {}
                    _ => { flow_go(child, src, file, fn_sym, scope, out); }
                }
            }
            let end = node.end_position();
            out.loops.push(LoopFact {
                file: file.into(), start: pos.row as u32, end: end.row as u32,
                var: lvar.clone(), collection: String::new(), fn_sym: fn_sym.into(),
            });
            if let Some(body) = node.child_by_field_name("body") {
                flow_go(body, src, file, fn_sym, scope, out);
            }
            Some(push_node(out, file, pos.row as u32, pos.column as u32, "loop", &lvar, fn_sym))
        }
        // `func(...) {...}`: lift as its OWN fn scope, same shape as Rust
        // closures/Kotlin lambda literals — `param` nodes with `df_param`
        // slots, body walked under the lifted sym. The enclosing `scope` is
        // shared, so a captured outer variable's read still resolves. The
        // `closure` VALUE node stays in the enclosing fn (it's the argument a
        // `df_arg` row records when the literal is passed straight to a call,
        // e.g. `go func(){ ... }()`/`defer func(){ ... }()`).
        "func_literal" => {
            let lam_sym = format!("{fn_sym}::closure::{}_{}", pos.row, pos.column);
            let mut lpos: u32 = 0;
            if let Some(params) = node.child_by_field_name("parameters") {
                let mut cursor = params.walk();
                for p in params.children(&mut cursor) {
                    if !matches!(p.kind(), "parameter_declaration" | "variadic_parameter_declaration") { continue; }
                    let mut nc = p.walk();
                    let names: Vec<tree_sitter::Node> = p.children(&mut nc).filter(|n| n.kind() == "identifier").collect();
                    if names.is_empty() { lpos += 1; continue; }
                    for name_node in names {
                        let sp = name_node.start_position();
                        let v = go_text(name_node, src).to_string();
                        let id = push_node(out, file, sp.row as u32, sp.column as u32, "param", &v, &lam_sym);
                        out.param_pos.push((id.clone(), lpos));
                        scope.insert(v, id);
                        lpos += 1;
                    }
                }
            }
            if let Some(body) = node.child_by_field_name("body") {
                flow_go(body, src, file, &lam_sym, scope, out);
            }
            Some(push_node(out, file, pos.row as u32, pos.column as u32, "closure", &lam_sym, fn_sym))
        }
        // everything else (blocks/statement lists, expression statements,
        // parenthesized/index/slice/type-assertion/conversion expressions,
        // go/defer/send/select/switch/labeled statements, ...): recurse
        // conservatively, surfacing the last value-bearing child.
        _ => go_recurse_children(node, src, file, fn_sym, scope, out),
    }
}

/// Flow every element of an `expression_list`, in source order, returning one
/// `Option<String>` per element (mismatched-arity binds use this alongside a
/// binding target list of a different length).
fn go_flow_expr_list(
    list: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) -> Vec<Option<String>> {
    let mut cursor = list.walk();
    list.children(&mut cursor).map(|e| flow_go(e, src, file, fn_sym, scope, out)).collect()
}

/// Bind each name in `names` to a fresh node of `kind` ("let_bind" for a
/// declaration, "var_write" for a plain assignment), wiring the matching rhs
/// value when arity lines up (else every target derives from the first rhs
/// value, conservative). `_` binds nothing.
fn go_bind(
    names: &[tree_sitter::Node],
    rhs_ids: &[Option<String>],
    kind: &str,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) {
    for (i, name_node) in names.iter().enumerate() {
        let v = go_text(*name_node, src).to_string();
        if v == "_" { continue; }
        let sp = name_node.start_position();
        let id = push_node(out, file, sp.row as u32, sp.column as u32, kind, &v, fn_sym);
        let rhs = if rhs_ids.len() == names.len() { rhs_ids.get(i).cloned().flatten() } else { rhs_ids.first().cloned().flatten() };
        if let Some(rhs) = rhs {
            out.edges.push(DfEdge { from: rhs, to: id.clone() });
        }
        scope.insert(v, id);
    }
}

fn go_flow_spec(
    spec: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) {
    let mut cursor = spec.walk();
    let names: Vec<tree_sitter::Node> = spec.children(&mut cursor).filter(|n| n.kind() == "identifier").collect();
    let rhs_ids = spec.child_by_field_name("value")
        .map(|value| go_flow_expr_list(value, src, file, fn_sym, scope, out))
        .unwrap_or_default();
    go_bind(&names, &rhs_ids, "let_bind", src, file, fn_sym, scope, out);
}

/// A composite literal's body (`literal_value`): each `keyed_element`'s value
/// (and each bare `literal_element`'s value, keyed by its 0-based position)
/// flows into `owner_id` and records a `df_field` row.
fn go_flow_literal_fields(
    lit: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
    owner_id: &str,
) {
    let mut cursor = lit.walk();
    let mut pos_idx: usize = 0;
    for child in lit.children(&mut cursor) {
        let (key_text, value_wrap) = match child.kind() {
            "keyed_element" => {
                let key_text = child.child_by_field_name("key")
                    .and_then(|k| k.named_child(0))
                    .filter(|inner| inner.kind() == "identifier")
                    .map(|inner| go_text(inner, src).to_string());
                (key_text, child.child_by_field_name("value"))
            }
            "literal_element" => (None, Some(child)),
            _ => continue,
        };
        let Some(value_wrap) = value_wrap else { continue };
        let Some(inner) = value_wrap.named_child(0) else { continue };
        if let Some(vid) = flow_go(inner, src, file, fn_sym, scope, out) {
            out.edges.push(DfEdge { from: vid.clone(), to: owner_id.to_string() });
            let field = key_text.unwrap_or_else(|| pos_idx.to_string());
            out.fields.push((owner_id.to_string(), field, vid));
        }
        pos_idx += 1;
    }
}

/// Walk all children conservatively, surfacing the last value-bearing child's
/// id. The generic fallback `flow_go` reaches for every node kind it doesn't
/// special-case.
fn go_recurse_children(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) -> Option<String> {
    let mut cursor = node.walk();
    let mut last = None;
    for child in node.children(&mut cursor) {
        if let Some(id) = flow_go(child, src, file, fn_sym, scope, out) {
            last = Some(id);
        }
    }
    last
}

/// Field-tree Merkle hash per type name, for shape-isomorphism detection.
///
/// Hashes the structural shape of each type's field tree using only `field`
/// and `variant` edges — the data shape. `impl` and `generic` edges are
/// cross-cutting, not shape, and are excluded. Two types with the same hash
/// are field-tree-isomorphic: same arity, depth, and leaf shape, regardless
/// of names. Names are NOT in the hash; this is pure shape.
///
/// Fixpoint iteration handles recursive types (`struct List { tail: Box<List> }`)
/// — a self-reference stabilizes because each round only mixes in the prior
/// round's hash, and the structure converges. The leaf sentinel (`LEAF`) is
/// the hash of any name with no field/variant children, so all primitives and
/// external types hash alike.
///
/// `edges` is the engine's full `type_edge(from, to, kind)` row set. Returns
/// one `(name, hex_hash)` per name that appears in any data edge, sorted by
/// name for deterministic output. blake3 matches the rest of the codebase's
/// persistent hash convention.
pub fn type_shape_hashes(edges: &[(String, String, String)]) -> Vec<(String, String)> {
    use std::collections::{BTreeMap, BTreeSet};

    // Keep only field/variant edges (the data shape). Drop impl/generic.
    let data_edges: Vec<(&str, &str)> = edges
        .iter()
        .filter(|(_, _, k)| k == "field" || k == "variant")
        .map(|(a, b, _)| (a.as_str(), b.as_str()))
        .collect();

    // Names appearing anywhere in the data graph.
    let mut names: BTreeSet<String> = BTreeSet::new();
    for (a, b) in &data_edges {
        names.insert((*a).to_string());
        names.insert((*b).to_string());
    }

    // Adjacency: name -> sorted unique child-name list. Duplicates collapse
    // (two fields of type T = shape {T}). Switch to a sorted Vec WITH dups to
    // make multiplicity count (struct{x: T, y: T} distinct from {z: T}).
    let mut adj: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (a, b) in &data_edges {
        adj.entry((*a).to_string()).or_default().push((*b).to_string());
    }
    for v in adj.values_mut() {
        v.sort();
        v.dedup();
    }

    let leaf_hash = *blake3::hash(b"LEAF").as_bytes();

    // Initial hash: every name starts as a leaf.
    let mut cur: BTreeMap<String, [u8; 32]> = names
        .iter()
        .map(|n| (n.clone(), leaf_hash))
        .collect();

    // Fixpoint: re-hash until stable or we hit the iter cap. The cap is a
    // safety net; in practice convergence is at most depth-of-the-deepest-tree
    // iterations (or never, only if the graph is pathologically oscillating).
    for _ in 0..64 {
        let mut next: BTreeMap<String, [u8; 32]> = BTreeMap::new();
        let mut stable = true;
        for n in &names {
            let mut h = blake3::Hasher::new();
            match adj.get(n) {
                None => { h.update(b"LEAF"); }
                Some(cs) if cs.is_empty() => { h.update(b"LEAF"); }
                Some(cs) => {
                    for c in cs {
                        match cur.get(c) {
                            Some(ch) => { h.update(ch); }
                            None => { h.update(b"EXT"); }
                        }
                    }
                }
            }
            let bytes = *h.finalize().as_bytes();
            if bytes != cur[n] {
                stable = false;
            }
            next.insert(n.clone(), bytes);
        }
        cur = next;
        if stable {
            break;
        }
    }

    cur.into_iter()
        .map(|(n, h)| (n, hex_string(&h)))
        .collect()
}

fn hex_string(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Anti-unification (Plotkin's LGG, Least General Generalization) for type trees.
///
/// For each ordered pair of distinct type names `(a, b)` with `a < b`, computes
/// how many "fresh variables" the LGG introduces. Two identical types produce 0.
/// Two types that differ only in N leaf positions produce N. Two unrelated types
/// produce a number bounded by their combined tree size.
///
/// Algorithm (simplified; treats type_edge as a tree, ignores sharing/cycles
/// via memoization that treats a revisit as opaque):
///   - `lgg(a, a)`         → 0 vars (identical)
///   - `lgg(leaf_a, leaf_b)` → 1 var  (distinct leaves)
///   - `lgg(a, b)` when both have field/variant children, same arity, same
///     kind sequence (after sorting by (kind, name)) → recurse pairwise,
///     sum the var counts.
///   - otherwise → 1 var (shape diverges; generalize the whole node away).
///
/// `edges` is the engine's full `type_edge` row set; only field/variant edges
/// contribute (impl/generic are excluded, same as `type_shape_hashes`).
///
/// Output: one `(a, b, vars)` per canonical pair with `a < b` and `vars >= 1`.
/// Pairs with `vars == 0` are identical and covered by `type_shape` already.
/// Sorted for deterministic output.
pub fn type_lgg_pairs(edges: &[(String, String, String)]) -> Vec<(String, String, i64)> {
    use std::collections::BTreeMap;

    // Build adjacency: name -> sorted Vec<(kind, child_name)>. field/variant only.
    let mut adj: BTreeMap<String, Vec<(&'static str, String)>> = BTreeMap::new();
    for (a, b, k) in edges {
        let kind: Option<&'static str> = match k.as_str() {
            "field" => Some("field"),
            "variant" => Some("variant"),
            _ => None,
        };
        if let Some(kind) = kind {
            adj.entry(a.clone()).or_default().push((kind, b.clone()));
        }
    }
    for v in adj.values_mut() {
        v.sort();
        // Don't dedup: a struct with two fields of the same type has shape
        // {T, T}, distinct from {T}. Keep multiplicity.
    }

    // All distinct names (including leaves that only appear as `to`).
    let mut names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for (a, b, _) in edges {
        names.insert(a.clone());
        names.insert(b.clone());
    }
    let names: Vec<String> = names.into_iter().collect();

    // Pair cache: (a, b) -> var count. Memoizes to handle DAG sharing and
    // breaks cycles (a revisit mid-recursion returns the cached value, which
    // is conservative — it pretends the recursion already terminated).
    let mut cache: BTreeMap<(String, String), i64> = BTreeMap::new();

    let mut out: Vec<(String, String, i64)> = Vec::new();
    for i in 0..names.len() {
        for j in (i + 1)..names.len() {
            let a = &names[i];
            let b = &names[j];
            // a < b lexicographically because names is sorted.
            let vars = lgg_var_count(a, b, &adj, &mut cache);
            if vars >= 1 {
                out.push((a.clone(), b.clone(), vars));
            }
        }
    }
    out.sort();
    out
}

fn lgg_var_count(
    a: &str,
    b: &str,
    adj: &std::collections::BTreeMap<String, Vec<(&'static str, String)>>,
    cache: &mut std::collections::BTreeMap<(String, String), i64>,
) -> i64 {
    if a == b {
        return 0;
    }
    // Canonicalize the cache key so (a,b) and (b,a) share.
    let key = if a < b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    };
    if let Some(&v) = cache.get(&key) {
        return v;
    }
    // Tentative 1 to break cycles: a recursive revisit returns 1 (conservative).
    cache.insert(key.clone(), 1);

    let ca = adj.get(a);
    let cb = adj.get(b);
    let result = match (ca, cb) {
        (None, None) => 1, // two distinct leaves
        (Some(ca), Some(cb)) if ca.len() == cb.len() => {
            // Same arity. Pairwise-align children by sorted position. If the
            // kind sequence matches, recurse and sum; else diverge.
            let kinds_match = ca.iter().zip(cb.iter()).all(|((ka, _), (kb, _))| ka == kb);
            if kinds_match {
                let mut sum = 0i64;
                for ((_, na), (_, nb)) in ca.iter().zip(cb.iter()) {
                    sum += lgg_var_count(na, nb, adj, cache);
                }
                sum
            } else {
                1
            }
        }
        _ => 1, // arity differs or one is leaf
    };

    cache.insert(key, result);
    result
}

// ── Python (tree-sitter) ─────────────────────────────────────────────────────
//
// Same diet-extractor contract as Kotlin: one tree-sitter parse feeds the
// entity, edge, call, dataflow, and doc walks. Python has no static type system,
// so entities/edges/dataflow are honest about what a syntax-only pass can see:
// `type_edge`/`type_sig` come ONLY from PEP 484 annotations (class bases,
// annotated attributes, annotated params/returns, annotated local assignments
// in a body — the TS "uses" convention); un-annotated code still gets full
// entity/call/dataflow/doc coverage. type_link (name resolution) is SCOPED OUT
// of this extractor entirely — an attribute-chain callee (`obj.method()`) is
// emitted with its bare trailing name and left for the engine's existing
// by_name resolver, exactly like Kotlin/TS; nothing here tries to guess a
// receiver's type. `self`/`cls` are dropped from parameter lists so positions
// align with `type_sig.pos`/`df_param.pos` (the Rust/Kotlin receiver
// convention) — matched by a literal name check since Python has no syntactic
// receiver marker. `EntityKind::Module` exists only so a module docstring (no
// enclosing class/def) has a `type_entity` row to join.

fn py_parse(content: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    let lang = tree_sitter::Language::new(tree_sitter_python::LANGUAGE);
    if parser.set_language(&lang).is_err() {
        return None;
    }
    parser.parse(content, None)
}

impl TypeLang for PyTypes {
    fn name(&self) -> &'static str { "python" }
    fn matches(&self, path: &str) -> bool { path.ends_with(".py") }
    // One tree-sitter parse feeds entities + edges + docs.
    fn extract(&self, file: &str, content: &str) -> TypeFacts {
        let Some(tree) = py_parse(content) else { return TypeFacts::default() };
        let src = content.as_bytes();
        let root = tree.root_node();
        TypeFacts {
            entities: py_entities_from(root, src, file),
            edges: py_edges_from(root, src),
            docs: py_docs_from(root, src, file),
            ..Default::default()
        }
    }
    // A second tree-sitter parse feeds defs + sites, same shape as Kotlin.
    fn extract_calls(&self, file: &str, content: &str) -> CallFacts {
        let Some(tree) = py_parse(content) else { return CallFacts::default() };
        let src = content.as_bytes();
        let root = tree.root_node();
        CallFacts {
            defs: py_call_defs_from(root, src, file),
            sites: py_call_sites_from(root, src, file),
        }
    }
    fn extract_dataflow(&self, file: &str, content: &str) -> DataflowFacts {
        let Some(tree) = py_parse(content) else { return DataflowFacts::default() };
        py_dataflow_from(tree.root_node(), content.as_bytes(), file)
    }
}

fn py_text(node: tree_sitter::Node, src: &[u8]) -> String {
    node.utf8_text(src).unwrap_or("").to_string()
}

fn py_row1(node: tree_sitter::Node) -> u32 {
    node.start_position().row as u32 + 1
}

/// Unwrap a `decorated_definition` down to its inner `class_definition` /
/// `function_definition` (a decorated def still emits its entity/edges/calls;
/// decorator identity rewriting is a stated non-goal). Any other node passes
/// through unchanged.
fn py_unwrap_decorated(node: tree_sitter::Node) -> tree_sitter::Node {
    if node.kind() == "decorated_definition" {
        node.child_by_field_name("definition").unwrap_or(node)
    } else {
        node
    }
}

/// (name, type-annotation node) for one `parameter` subtype. `self`/`cls`
/// receivers are plain `identifier` params like any other — the caller decides
/// whether to skip the first one. Lambda params (always untyped) reuse the
/// `identifier`/`default_parameter`/splat arms; only `typed_parameter`/
/// `typed_default_parameter` (regular-function-only syntax) carry a type.
fn py_param_name_and_type<'t>(
    p: tree_sitter::Node<'t>,
    src: &[u8],
) -> (Option<String>, Option<tree_sitter::Node<'t>>) {
    match p.kind() {
        "identifier" => (Some(py_text(p, src)), None),
        "typed_parameter" => {
            let mut cur = p.walk();
            let name = p.named_children(&mut cur)
                .find(|n| n.kind() == "identifier")
                .map(|n| py_text(n, src));
            (name, p.child_by_field_name("type"))
        }
        "default_parameter" => {
            let name = p.child_by_field_name("name")
                .filter(|n| n.kind() == "identifier")
                .map(|n| py_text(n, src));
            (name, None)
        }
        "typed_default_parameter" => {
            let name = p.child_by_field_name("name").map(|n| py_text(n, src));
            (name, p.child_by_field_name("type"))
        }
        "list_splat_pattern" | "dictionary_splat_pattern" => {
            let mut cur = p.walk();
            let name = p.named_children(&mut cur)
                .find(|n| n.kind() == "identifier")
                .map(|n| py_text(n, src));
            (name, None)
        }
        _ => (None, None),
    }
}

/// Declared PEP-695 type-parameter names (`def f[T](...)` / `class C[T]:`),
/// excluded from ref collection like Kotlin/TS's declared-generic exclusion.
/// Broad by design: every identifier under the `type_parameters` field counts,
/// including bound expressions — over-excluding a rare bound name is harmless.
fn py_collect_type_params(node: tree_sitter::Node, src: &[u8], field: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    if let Some(tp) = node.child_by_field_name(field) {
        py_collect_identifiers_rec(tp, src, &mut out);
    }
    out
}

fn py_collect_identifiers_rec(node: tree_sitter::Node, src: &[u8], out: &mut BTreeSet<String>) {
    if node.kind() == "identifier" {
        out.insert(py_text(node, src));
    }
    let mut cur = node.walk();
    for c in node.named_children(&mut cur) {
        py_collect_identifiers_rec(c, src, out);
    }
}

/// Collect every type name referenced under an annotation node. `subscript`
/// (`Optional[Foo]`, `list[Bar]`) recurses into BOTH the container (`Optional`/
/// `list`, itself noise-filtered) and each subscripted argument — never the raw
/// subscript text — so `Optional[Foo]` yields `Foo` (and `Optional` is dropped
/// as noise). `attribute` (`typing.Optional`, `module.Class`) keeps only the
/// trailing bare name, matching the callee-resolution convention elsewhere.
/// A string forward-ref (`"Foo"`) is not parsed (non-goal).
fn py_type_refs(node: tree_sitter::Node, src: &[u8], tparams: &BTreeSet<String>, out: &mut Vec<String>) {
    match node.kind() {
        "identifier" => {
            let name = py_text(node, src);
            if !tparams.contains(&name) && !is_noise_python(&name) {
                out.push(name);
            }
        }
        "attribute" => {
            if let Some(attr) = node.child_by_field_name("attribute") {
                let name = py_text(attr, src);
                if !tparams.contains(&name) && !is_noise_python(&name) {
                    out.push(name);
                }
            }
        }
        "subscript" => {
            if let Some(value) = node.child_by_field_name("value") {
                py_type_refs(value, src, tparams, out);
            }
            let mut cur = node.walk();
            for sub in node.children_by_field_name("subscript", &mut cur) {
                py_type_refs(sub, src, tparams, out);
            }
        }
        "string" | "concatenated_string" => {}
        _ => {
            let mut cur = node.walk();
            for child in node.named_children(&mut cur) {
                py_type_refs(child, src, tparams, out);
            }
        }
    }
}

fn py_type_refs_collect(node: tree_sitter::Node, src: &[u8], tparams: &BTreeSet<String>) -> Vec<String> {
    let mut out = Vec::new();
    py_type_refs(node, src, tparams, &mut out);
    out.sort();
    out.dedup();
    out
}

/// Builtin scalar/container names and common `typing` wrapper names: noise for
/// ref collection so `Optional[Foo]`/`list[Bar]` surface the inner `Foo`/`Bar`
/// without also emitting an edge to the wrapper itself.
fn is_noise_python(name: &str) -> bool {
    matches!(
        name,
        "int" | "str" | "float" | "bool" | "bytes" | "complex" | "object" | "type"
            | "list" | "dict" | "set" | "tuple" | "frozenset" | "None" | "Self"
            | "Any" | "Optional" | "Union" | "List" | "Dict" | "Tuple" | "Set"
            | "FrozenSet" | "Callable" | "ClassVar" | "Final" | "Type" | "Sequence"
            | "Iterable" | "Iterator" | "Mapping" | "Awaitable" | "Coroutine"
    )
}

/// Build the arrow `[...A] => B` for a `def`. Each declared parameter is a slot
/// (untyped slots stay empty, matching the TS/Kotlin convention); `self`/`cls`
/// are dropped entirely (not even an empty slot), mirroring Rust's receiver
/// skip so positions align with `type_sig.pos`/`df_param.pos`.
fn py_fn_type(node: tree_sitter::Node, src: &[u8]) -> TypeExpr {
    let tparams = py_collect_type_params(node, src, "type_parameters");
    let mut params = Vec::new();
    if let Some(plist) = node.child_by_field_name("parameters") {
        let mut cur = plist.walk();
        let mut first = true;
        for p in plist.named_children(&mut cur) {
            if matches!(p.kind(), "keyword_separator" | "positional_separator") {
                continue;
            }
            let (name_opt, type_node) = py_param_name_and_type(p, src);
            if first {
                first = false;
                if matches!(name_opt.as_deref(), Some("self") | Some("cls")) {
                    continue;
                }
            }
            let refs = type_node.map(|t| py_type_refs_collect(t, src, &tparams)).unwrap_or_default();
            params.push(refs.into_iter().map(TypeRef::Named).collect());
        }
    }
    let ret = node.child_by_field_name("return_type")
        .map(|rt| py_type_refs_collect(rt, src, &tparams))
        .unwrap_or_default()
        .into_iter().map(TypeRef::Named).collect();
    TypeExpr { params, ret }
}

// --- Python entity pass: module + class + function/method, functions carrying
// their arrow type like Rust/Kotlin/TS. `class_owner` threads the enclosing
// class's bare name while walking a class body's DIRECT statements (including
// through pass-through compound statements like `if`/`try`/`with`) and resets
// to None on entering ANY function body, so a def nested inside a method is a
// free function, not a second-level method (matches Kotlin/TS). ---

fn py_entities_from(root: tree_sitter::Node, src: &[u8], file: &str) -> Vec<TypeEntity> {
    let mut out = vec![TypeEntity {
        sym: mint_sym(file, EntityKind::Module, "<module>", None),
        name: "<module>".to_string(),
        kind: EntityKind::Module,
        parent: None,
        file: file.to_string(),
        line: 1,
        ty: None,
    }];
    walk_py_entities(root, src, file, None, &mut out);
    out
}

fn walk_py_entities(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    class_owner: Option<&str>,
    out: &mut Vec<TypeEntity>,
) {
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        let target = py_unwrap_decorated(child);
        match target.kind() {
            "class_definition" => {
                if let Some(name) = target.child_by_field_name("name").map(|n| py_text(n, src)) {
                    out.push(TypeEntity {
                        sym: mint_sym(file, EntityKind::Class, &name, None),
                        name: name.clone(),
                        kind: EntityKind::Class,
                        parent: None,
                        file: file.to_string(),
                        line: py_row1(target),
                        ty: None,
                    });
                    if let Some(body) = target.child_by_field_name("body") {
                        walk_py_entities(body, src, file, Some(&name), out);
                    }
                }
            }
            "function_definition" => {
                if let Some(name) = target.child_by_field_name("name").map(|n| py_text(n, src)) {
                    let (kind, parent_name) = match class_owner {
                        Some(o) => (EntityKind::Method, Some(o)),
                        None => (EntityKind::Function, None),
                    };
                    out.push(TypeEntity {
                        sym: mint_sym(file, kind, &name, parent_name),
                        name,
                        kind,
                        parent: parent_name.map(|p| mint_sym(file, EntityKind::Class, p, None)),
                        file: file.to_string(),
                        line: py_row1(target),
                        ty: Some(py_fn_type(target, src)),
                    });
                    if let Some(body) = target.child_by_field_name("body") {
                        walk_py_entities(body, src, file, None, out);
                    }
                }
            }
            _ => walk_py_entities(target, src, file, class_owner, out),
        }
    }
}

// --- Python type_edge pass: class bases = "impl"; annotated class-body
// attributes = "field"; def param annotations = "param", return annotation =
// "returns", annotations on locally-annotated assignments IN the body =
// "uses" (the TS function-edge vocabulary, applied to the closest Python
// analogue of "types mentioned in the body"). ---

fn py_edges_from(root: tree_sitter::Node, src: &[u8]) -> Vec<TypeEdge> {
    let mut out: BTreeSet<(String, String, &'static str)> = BTreeSet::new();
    walk_py_edges(root, src, &mut out);
    out.into_iter().map(|(from, to, kind)| TypeEdge { from, to, kind }).collect()
}

fn walk_py_edges(node: tree_sitter::Node, src: &[u8], out: &mut BTreeSet<(String, String, &'static str)>) {
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        let target = py_unwrap_decorated(child);
        match target.kind() {
            "class_definition" => {
                py_class_edges(target, src, out);
                if let Some(body) = target.child_by_field_name("body") {
                    walk_py_edges(body, src, out);
                }
            }
            "function_definition" => {
                py_function_edges(target, src, out);
                if let Some(body) = target.child_by_field_name("body") {
                    walk_py_edges(body, src, out);
                }
            }
            _ => walk_py_edges(target, src, out),
        }
    }
}

fn py_class_edges(node: tree_sitter::Node, src: &[u8], out: &mut BTreeSet<(String, String, &'static str)>) {
    let Some(owner) = node.child_by_field_name("name").map(|n| py_text(n, src)) else { return };
    let tparams = py_collect_type_params(node, src, "type_parameters");
    if let Some(supers) = node.child_by_field_name("superclasses") {
        let mut cur = supers.walk();
        for arg in supers.named_children(&mut cur) {
            // `metaclass=Foo` is a keyword arg, not a base type.
            if arg.kind() == "keyword_argument" {
                continue;
            }
            for to in py_type_refs_collect(arg, src, &tparams) {
                push(out, &owner, &to, "impl");
            }
        }
    }
    if let Some(body) = node.child_by_field_name("body") {
        let mut cur = body.walk();
        for stmt in body.named_children(&mut cur) {
            if stmt.kind() != "expression_statement" {
                continue;
            }
            let Some(inner) = stmt.named_child(0) else { continue };
            if inner.kind() != "assignment" {
                continue;
            }
            if let Some(ty) = inner.child_by_field_name("type") {
                for to in py_type_refs_collect(ty, src, &tparams) {
                    push(out, &owner, &to, "field");
                }
            }
        }
    }
}

fn py_function_edges(node: tree_sitter::Node, src: &[u8], out: &mut BTreeSet<(String, String, &'static str)>) {
    let Some(owner) = node.child_by_field_name("name").map(|n| py_text(n, src)) else { return };
    let tparams = py_collect_type_params(node, src, "type_parameters");
    if let Some(plist) = node.child_by_field_name("parameters") {
        let mut cur = plist.walk();
        let mut first = true;
        for p in plist.named_children(&mut cur) {
            if matches!(p.kind(), "keyword_separator" | "positional_separator") {
                continue;
            }
            let (name_opt, type_node) = py_param_name_and_type(p, src);
            if first {
                first = false;
                if matches!(name_opt.as_deref(), Some("self") | Some("cls")) {
                    continue;
                }
            }
            if let Some(t) = type_node {
                for to in py_type_refs_collect(t, src, &tparams) {
                    push(out, &owner, &to, "param");
                }
            }
        }
    }
    if let Some(rt) = node.child_by_field_name("return_type") {
        for to in py_type_refs_collect(rt, src, &tparams) {
            push(out, &owner, &to, "returns");
        }
    }
    if let Some(body) = node.child_by_field_name("body") {
        let mut uses = Vec::new();
        py_collect_body_annotation_refs(body, src, &tparams, &mut uses);
        uses.sort();
        uses.dedup();
        for to in uses {
            push(out, &owner, &to, "uses");
        }
    }
}

/// Every annotated local assignment (`x: Foo = ...`) anywhere under a function
/// body, including inside nested defs (same imprecision TS accepts: its body
/// visitor doesn't stop at a nested closure either).
fn py_collect_body_annotation_refs(node: tree_sitter::Node, src: &[u8], tparams: &BTreeSet<String>, out: &mut Vec<String>) {
    if node.kind() == "assignment" {
        if let Some(ty) = node.child_by_field_name("type") {
            out.extend(py_type_refs_collect(ty, src, tparams));
        }
    }
    let mut cur = node.walk();
    for child in node.named_children(&mut cur) {
        py_collect_body_annotation_refs(child, src, tparams, out);
    }
}

// --- Python call-graph pass: `function_definition` nodes become CallDefs (a
// def inside a class body is a Method keyed to the enclosing class, a
// top-level or nested-in-function def is Free); every `call` node becomes a
// CallSite whose callee is the called name as written — a bare identifier, or
// the trailing attribute name of `recv.method(...)` (the bare-attribute
// convention; attribute-chain resolution is scoped out). ---

fn py_call_defs_from(root: tree_sitter::Node, src: &[u8], file: &str) -> Vec<CallDef> {
    let mut out = Vec::new();
    py_walk_call_defs(root, src, file, None, &mut out);
    out
}

fn py_walk_call_defs(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    parent: Option<&str>,
    out: &mut Vec<CallDef>,
) {
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        let target = py_unwrap_decorated(child);
        match target.kind() {
            "class_definition" => {
                let owner = target.child_by_field_name("name").map(|n| py_text(n, src));
                if let Some(body) = target.child_by_field_name("body") {
                    py_walk_call_defs(body, src, file, owner.as_deref(), out);
                }
            }
            "function_definition" => {
                let name = target.child_by_field_name("name").map(|n| py_text(n, src)).unwrap_or_default();
                let (kind, ekind) = match parent {
                    Some(_) => (CallKind::Method, EntityKind::Method),
                    None => (CallKind::Free, EntityKind::Function),
                };
                let end = target.child_by_field_name("body").unwrap_or(target).end_position().row as u32 + 1;
                out.push(CallDef {
                    sym: mint_sym(file, ekind, &name, parent),
                    name,
                    kind,
                    file: file.to_string(),
                    line: py_row1(target),
                    end,
                });
                // a nested `def` is Free with respect to the enclosing scope.
                if let Some(body) = target.child_by_field_name("body") {
                    py_walk_call_defs(body, src, file, None, out);
                }
            }
            _ => py_walk_call_defs(target, src, file, parent, out),
        }
    }
}

fn py_call_sites_from(root: tree_sitter::Node, src: &[u8], file: &str) -> Vec<CallSite> {
    let mut out = Vec::new();
    py_walk_call_sites(root, src, file, &mut out);
    out
}

fn py_walk_call_sites(node: tree_sitter::Node, src: &[u8], file: &str, out: &mut Vec<CallSite>) {
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        if child.kind() == "call" {
            if let Some((callee, line)) = py_callee(child, src) {
                out.push(CallSite { caller_sym: None, callee, callee_path: None, file: file.to_string(), line });
            }
        }
        py_walk_call_sites(child, src, file, out);
    }
}

/// (callee name, 1-based call line) for a `call` node, or None when the callee
/// is not a plain identifier or attribute access (e.g. an invoked subscript or
/// a called lambda expression).
fn py_callee(call: tree_sitter::Node, src: &[u8]) -> Option<(String, u32)> {
    let func = call.child_by_field_name("function")?;
    let line = py_row1(func);
    match func.kind() {
        "identifier" => Some((py_text(func, src), line)),
        "attribute" => {
            let attr = func.child_by_field_name("attribute")?;
            Some((py_text(attr, src), line))
        }
        _ => None,
    }
}

// --- Python doc pass: the docstring is the first expression-statement STRING
// of a module/class/def body (PEP 257); quote/prefix-stripped and dedented.
// Sphinx-field tags only (`:param name: text`, `:return:`/`:returns: text`) —
// Google-style (`Args:` sections) is a stated non-goal. ---

fn py_docs_from(root: tree_sitter::Node, src: &[u8], file: &str) -> Vec<DocFact> {
    let mut out = Vec::new();
    if let Some(text) = py_docstring_of(root, src) {
        out.push(DocFact {
            sym: mint_sym(file, EntityKind::Module, "<module>", None),
            line: 1,
            tags: py_parse_sphinx_tags(&text),
            text,
        });
    }
    walk_py_docs(root, src, file, None, &mut out);
    out
}

fn walk_py_docs(node: tree_sitter::Node, src: &[u8], file: &str, class_owner: Option<&str>, out: &mut Vec<DocFact>) {
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        let target = py_unwrap_decorated(child);
        match target.kind() {
            "class_definition" => {
                if let Some(name) = target.child_by_field_name("name").map(|n| py_text(n, src)) {
                    if let Some(body) = target.child_by_field_name("body") {
                        if let Some(text) = py_docstring_of(body, src) {
                            out.push(DocFact {
                                sym: mint_sym(file, EntityKind::Class, &name, None),
                                line: py_row1(target),
                                tags: py_parse_sphinx_tags(&text),
                                text,
                            });
                        }
                        walk_py_docs(body, src, file, Some(&name), out);
                    }
                }
            }
            "function_definition" => {
                if let Some(name) = target.child_by_field_name("name").map(|n| py_text(n, src)) {
                    let kind = if class_owner.is_some() { EntityKind::Method } else { EntityKind::Function };
                    if let Some(body) = target.child_by_field_name("body") {
                        if let Some(text) = py_docstring_of(body, src) {
                            out.push(DocFact {
                                sym: mint_sym(file, kind, &name, class_owner),
                                line: py_row1(target),
                                tags: py_parse_sphinx_tags(&text),
                                text,
                            });
                        }
                        walk_py_docs(body, src, file, None, out);
                    }
                }
            }
            _ => walk_py_docs(target, src, file, class_owner, out),
        }
    }
}

/// The docstring at the head of a module/class/def body block: the block's
/// first named child must be a bare `string` expression statement.
fn py_docstring_of(body: tree_sitter::Node, src: &[u8]) -> Option<String> {
    let mut cur = body.walk();
    let first = body.named_children(&mut cur).next()?;
    if first.kind() != "expression_statement" {
        return None;
    }
    let inner = first.named_child(0)?;
    if inner.kind() != "string" {
        return None;
    }
    let raw = inner.utf8_text(src).ok()?;
    Some(py_clean_docstring(raw))
}

/// Strip an (optional) `r`/`b`/`f`/`u` prefix and the enclosing quotes (`"""`/
/// `'''`/`"`/`'`), then dedent. Raw-string escapes are not unescaped (honest:
/// the doc text keeps whatever backslash sequences the source has).
fn py_clean_docstring(raw: &str) -> String {
    let trimmed = raw.trim();
    let quote_at = trimmed.find(['"', '\'']).unwrap_or(0);
    let body = &trimmed[quote_at..];
    let (quote, _) = if body.starts_with("\"\"\"") {
        ("\"\"\"", 3)
    } else if body.starts_with("'''") {
        ("'''", 3)
    } else if body.starts_with('"') {
        ("\"", 1)
    } else if body.starts_with('\'') {
        ("'", 1)
    } else {
        return trimmed.to_string();
    };
    let inner = body.strip_prefix(quote).and_then(|r| r.strip_suffix(quote)).unwrap_or(body);
    py_dedent(inner)
}

/// PEP 257 dedent: the minimum leading whitespace over every non-blank line
/// AFTER the first (which sits right after the opening quote, so it carries no
/// meaningful indent) is stripped from every subsequent line.
fn py_dedent(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= 1 {
        return text.trim().to_string();
    }
    let min_indent = lines.iter().skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    let mut out = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            out.push(line.trim().to_string());
        } else {
            out.push(line.get(min_indent.min(line.len())..).unwrap_or("").to_string());
        }
    }
    out.join("\n").trim().to_string()
}

/// Sphinx field-list tags: `:param name: text` -> tag "param" arg "name";
/// `:return:`/`:returns: text` -> tag "returns" (no arg). Any other `:tag:`
/// passes through with its raw arg/body; a continuation line (no leading `:`)
/// appends to the previous tag's text. Google-style (`Args:` sections) is a
/// stated non-goal — not recognized here.
fn py_parse_sphinx_tags(text: &str) -> Vec<DocTag> {
    let mut out: Vec<DocTag> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(':') {
            if let Some(colon) = rest.find(':') {
                let head = rest[..colon].trim();
                let body = rest[colon + 1..].trim().to_string();
                let mut it = head.splitn(2, char::is_whitespace);
                let tag_word = it.next().unwrap_or("");
                let head_arg = it.next().unwrap_or("").trim();
                let (tag, arg) = match tag_word {
                    "param" | "parameter" => ("param", head_arg),
                    "return" | "returns" => ("returns", ""),
                    other => (other, head_arg),
                };
                out.push(DocTag { tag: tag.to_string(), arg: arg.to_string(), text: body });
                continue;
            }
        }
        if let Some(last) = out.last_mut() {
            if !trimmed.is_empty() {
                if !last.text.is_empty() {
                    last.text.push(' ');
                }
                last.text.push_str(trimmed);
            }
        }
    }
    out
}

// --- Python intra-procedural dataflow lift (tree-sitter). Same two-rule model
// as Kotlin/Rust: value-bearing children flow into their parent, and a bound
// name (assignment target, param, loop variable, comprehension variable, or
// lambda param) registers a scope slot that a later read flows from. Node id
// is `file:row:col:kind` (`push_node`'s shared format); rows are 0-based from
// tree-sitter and bumped +1 at the end, matching Kotlin exactly. Every named
// `def` (top-level, method, or nested) is discovered by one full-tree walk
// (`py_walk_fns`, mirrors `kt_walk_fns`) and flowed with a FRESH, unshared
// scope — captures are only modeled for LAMBDAS, which explicitly share the
// enclosing `scope` map. `self`/`cls` are skipped as params so `df_param.pos`
// aligns with `type_sig.pos`. ---

fn py_dataflow_from(root: tree_sitter::Node, src: &[u8], file: &str) -> DataflowFacts {
    let mut out = DataflowFacts::default();
    py_walk_fns(root, src, file, &mut out);
    for n in &mut out.nodes { n.line += 1; }
    for l in &mut out.loops { l.start += 1; l.end += 1; }
    out.nests = compute_nests(&out.nodes, &out.loops);
    out
}

fn py_walk_fns(node: tree_sitter::Node, src: &[u8], file: &str, out: &mut DataflowFacts) {
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        let target = py_unwrap_decorated(child);
        if target.kind() == "function_definition" {
            py_flow_fn(target, src, file, out);
        }
        py_walk_fns(target, src, file, out);
    }
}

/// Seed non-receiver param nodes into a fresh scope, then flow the body's
/// statements. Unlike Rust/Kotlin, a Python function body has no implicit
/// tail-return: only an explicit `return` (handled in `py_flow_stmt`) reaches
/// the fn's `ret` sink. `fn_sym` always mints `EntityKind::Function` with no
/// parent, even for a method — matching Kotlin's `kt_flow_fn` exactly (the
/// dataflow fn_sym is a grouping key, not the entity/call_def sym).
fn py_flow_fn(node: tree_sitter::Node, src: &[u8], file: &str, out: &mut DataflowFacts) {
    let Some(name_node) = node.child_by_field_name("name") else { return };
    let name = py_text(name_node, src);
    let fn_sym = mint_sym(file, EntityKind::Function, &name, None);
    let mut scope: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if let Some(params) = node.child_by_field_name("parameters") {
        let mut cur = params.walk();
        let mut pos: u32 = 0;
        let mut first = true;
        for p in params.named_children(&mut cur) {
            if matches!(p.kind(), "keyword_separator" | "positional_separator") {
                continue;
            }
            let (name_opt, _ty) = py_param_name_and_type(p, src);
            if first {
                first = false;
                if matches!(name_opt.as_deref(), Some("self") | Some("cls")) {
                    continue;
                }
            }
            if let Some(pname) = name_opt {
                let ppos = p.start_position();
                let id = push_node(out, file, ppos.row as u32, ppos.column as u32, "param", &pname, &fn_sym);
                out.param_pos.push((id.clone(), pos));
                scope.insert(pname, id);
                pos += 1;
            }
        }
    }
    if let Some(body) = node.child_by_field_name("body") {
        py_flow_stmt(body, src, file, &fn_sym, &mut scope, out);
    }
}

/// Flow one statement. A nested `function_definition`/`decorated_definition`/
/// `class_definition` is deliberately SKIPPED here (not recursed into): the
/// top-level `py_walk_fns` full-tree walk independently discovers and flows it
/// with its own fresh scope, so flowing it again here would double-count.
fn py_flow_stmt(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) {
    match node.kind() {
        "function_definition" | "decorated_definition" | "class_definition" => {}
        "expression_statement" => {
            if let Some(inner) = node.named_child(0) {
                if inner.kind() == "assignment" {
                    py_flow_assignment(inner, src, file, fn_sym, scope, out);
                } else {
                    let _ = py_flow_expr(inner, src, file, fn_sym, scope, out);
                }
            }
        }
        "assignment" => py_flow_assignment(node, src, file, fn_sym, scope, out),
        "return_statement" => {
            let pos = node.start_position();
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "ret", "", fn_sym);
            if let Some(val) = node.named_child(0) {
                let v = py_flow_expr(val, src, file, fn_sym, scope, out);
                out.edges.push(DfEdge { from: v, to: id });
            }
        }
        "for_statement" => py_flow_for(node, src, file, fn_sym, scope, out),
        "while_statement" => py_flow_while(node, src, file, fn_sym, scope, out),
        _ => {
            // block/if_statement/try_statement/with_statement/else_clause/... :
            // conservative pass-through recursion into every named child.
            let mut cur = node.walk();
            for c in node.named_children(&mut cur) {
                py_flow_stmt(c, src, file, fn_sym, scope, out);
            }
        }
    }
}

fn py_flow_assignment(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) {
    let Some(right) = node.child_by_field_name("right") else { return };
    let rhs = py_flow_expr(right, src, file, fn_sym, scope, out);
    if let Some(left) = node.child_by_field_name("left") {
        py_bind_pattern(left, &rhs, src, file, fn_sym, scope, out);
    }
}

/// Bind an assignment target. `identifier` mints a `let_bind` slot edged from
/// the rhs; tuple/list unpacking mints one slot PER identifier, each edged
/// from the SAME rhs value (kept simple, no per-position slicing); `attribute`
/// (`self.x = ...`) and `subscript` (`d[k] = ...`) track no local binding
/// (honest limit — attribute-chain flow is scoped out).
fn py_bind_pattern(
    node: tree_sitter::Node,
    rhs: &str,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) {
    match node.kind() {
        "identifier" => {
            let name = py_text(node, src);
            let pos = node.start_position();
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "let_bind", &name, fn_sym);
            out.edges.push(DfEdge { from: rhs.to_string(), to: id.clone() });
            scope.insert(name, id);
        }
        "tuple_pattern" | "list_pattern" | "pattern_list" => {
            let mut cur = node.walk();
            for child in node.named_children(&mut cur) {
                py_bind_pattern(child, rhs, src, file, fn_sym, scope, out);
            }
        }
        _ => {}
    }
}

/// Identifiers bound by a `for`/comprehension pattern (tuple unpacking flattens
/// to every leaf identifier); returns `(name, the identifier's own node)` pairs
/// so the caller can mint a correctly-positioned `let_bind`.
fn py_pattern_identifiers<'t>(node: tree_sitter::Node<'t>, src: &[u8], out: &mut Vec<(String, tree_sitter::Node<'t>)>) {
    match node.kind() {
        "identifier" => out.push((py_text(node, src), node)),
        "tuple_pattern" | "list_pattern" | "pattern_list" => {
            let mut cur = node.walk();
            for c in node.named_children(&mut cur) {
                py_pattern_identifiers(c, src, out);
            }
        }
        _ => {}
    }
}

fn py_flow_for(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) {
    let pos = node.start_position();
    let mut rcur = node.walk();
    let iter_expr = node.children_by_field_name("right", &mut rcur).find(|n| n.is_named());
    let coll = iter_expr.map(|e| py_flow_expr(e, src, file, fn_sym, scope, out));
    let mut var_name = String::new();
    if let Some(left) = node.child_by_field_name("left") {
        let mut names = Vec::new();
        py_pattern_identifiers(left, src, &mut names);
        for (i, (name, nnode)) in names.iter().enumerate() {
            let npos = nnode.start_position();
            let id = push_node(out, file, npos.row as u32, npos.column as u32, "let_bind", name, fn_sym);
            if let Some(c) = &coll {
                out.edges.push(DfEdge { from: c.clone(), to: id.clone() });
            }
            scope.insert(name.clone(), id);
            if i == 0 { var_name = name.clone(); }
        }
    }
    let end = node.end_position();
    out.loops.push(LoopFact {
        file: file.into(), start: pos.row as u32, end: end.row as u32,
        var: var_name, collection: String::new(), fn_sym: fn_sym.into(),
    });
    if let Some(body) = node.child_by_field_name("body") {
        py_flow_stmt(body, src, file, fn_sym, scope, out);
    }
}

fn py_flow_while(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) {
    let pos = node.start_position();
    if let Some(cond) = node.child_by_field_name("condition") {
        let _ = py_flow_expr(cond, src, file, fn_sym, scope, out);
    }
    let end = node.end_position();
    out.loops.push(LoopFact {
        file: file.into(), start: pos.row as u32, end: end.row as u32,
        var: String::new(), collection: String::new(), fn_sym: fn_sym.into(),
    });
    if let Some(body) = node.child_by_field_name("body") {
        py_flow_stmt(body, src, file, fn_sym, scope, out);
    }
}

/// Comprehensions/generator expressions walk their `for_in_clause`(s) and
/// `if_clause`(s) IN THE ENCLOSING SCOPE (Python creates its own comprehension
/// scope at runtime; this diet lift shares the caller's `scope` map instead,
/// same simplification as everywhere else here), binding each loop variable
/// from its iterable, then flows the body (or, for a dict comprehension, both
/// the key and value of its `pair`) into a `new` node representing the
/// assembled collection. Also records the comprehension's own span as a loop
/// fact so `nest` counts calls made per iteration.
fn py_comprehension_flow(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) -> String {
    let pos = node.start_position();
    let mut loop_var = String::new();
    let mut cur = node.walk();
    let clauses: Vec<tree_sitter::Node> = node.named_children(&mut cur).collect();
    for clause in &clauses {
        match clause.kind() {
            "for_in_clause" => {
                let mut rcur = clause.walk();
                let iter_expr = clause.children_by_field_name("right", &mut rcur).find(|n| n.is_named());
                let coll = iter_expr.map(|e| py_flow_expr(e, src, file, fn_sym, scope, out));
                if let Some(left) = clause.child_by_field_name("left") {
                    let mut names = Vec::new();
                    py_pattern_identifiers(left, src, &mut names);
                    for (name, nnode) in &names {
                        if loop_var.is_empty() { loop_var = name.clone(); }
                        let npos = nnode.start_position();
                        let id = push_node(out, file, npos.row as u32, npos.column as u32, "let_bind", name, fn_sym);
                        if let Some(c) = &coll {
                            out.edges.push(DfEdge { from: c.clone(), to: id.clone() });
                        }
                        scope.insert(name.clone(), id);
                    }
                }
            }
            "if_clause" => {
                let mut ccur = clause.walk();
                for e in clause.named_children(&mut ccur) {
                    let _ = py_flow_expr(e, src, file, fn_sym, scope, out);
                }
            }
            _ => {}
        }
    }
    let mut fill_ids = Vec::new();
    if node.kind() == "dictionary_comprehension" {
        if let Some(pair) = node.child_by_field_name("body") {
            if let Some(k) = pair.child_by_field_name("key") {
                fill_ids.push(py_flow_expr(k, src, file, fn_sym, scope, out));
            }
            if let Some(v) = pair.child_by_field_name("value") {
                fill_ids.push(py_flow_expr(v, src, file, fn_sym, scope, out));
            }
        }
    } else if let Some(body_expr) = node.child_by_field_name("body") {
        fill_ids.push(py_flow_expr(body_expr, src, file, fn_sym, scope, out));
    }
    let id = push_node(out, file, pos.row as u32, pos.column as u32, "new", "", fn_sym);
    for f in fill_ids {
        out.edges.push(DfEdge { from: f, to: id.clone() });
    }
    let end = node.end_position();
    out.loops.push(LoopFact {
        file: file.into(), start: pos.row as u32, end: end.row as u32,
        var: loop_var, collection: String::new(), fn_sym: fn_sym.into(),
    });
    id
}

/// Post-order value flow for one Python expression. Returns the node id
/// carrying its value; unhandled shapes fall through to a conservative
/// catch-all that recurses and surfaces the last value-bearing child (or, if
/// none, a generic `expr` node) — may miss flows, never invents one.
fn py_flow_expr(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, String>,
    out: &mut DataflowFacts,
) -> String {
    let pos = node.start_position();
    match node.kind() {
        "identifier" => {
            let name = py_text(node, src);
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "var_read", &name, fn_sym);
            if let Some(b) = scope.get(&name) {
                out.edges.push(DfEdge { from: b.clone(), to: id.clone() });
            }
            id
        }
        "true" | "false" | "none" | "integer" | "float" | "string" | "concatenated_string" => {
            push_node(out, file, pos.row as u32, pos.column as u32, "lit", "", fn_sym)
        }
        // f(args) / recv.method(args): each positional argument flows into the
        // call result with `df_arg` recording its 0-based slot; a keyword
        // argument ALSO lands in `df_field` under its name (the Kotlin
        // named-arg precedent); a member callee flows the receiver in at slot
        // -1; a CAPITALIZED bare callee is a constructor call (PEP 8
        // convention), minted as a `new` node carrying the type name.
        "call" => {
            let func = node.child_by_field_name("function");
            let mut recv: Option<String> = None;
            let mut callee_name = String::new();
            match func.map(|f| f.kind()) {
                Some("identifier") => { callee_name = py_text(func.unwrap(), src); }
                Some("attribute") => {
                    let f = func.unwrap();
                    if let Some(obj) = f.child_by_field_name("object") {
                        recv = Some(py_flow_expr(obj, src, file, fn_sym, scope, out));
                    }
                    if let Some(attr) = f.child_by_field_name("attribute") {
                        callee_name = py_text(attr, src);
                    }
                }
                _ => {
                    if let Some(f) = func {
                        let _ = py_flow_expr(f, src, file, fn_sym, scope, out);
                    }
                }
            }
            let mut arg_ids: Vec<(Option<String>, String)> = Vec::new();
            if let Some(args) = node.child_by_field_name("arguments") {
                let mut cur = args.walk();
                for a in args.named_children(&mut cur) {
                    match a.kind() {
                        "keyword_argument" => {
                            let name = a.child_by_field_name("name").map(|n| py_text(n, src));
                            if let Some(val) = a.child_by_field_name("value") {
                                let vid = py_flow_expr(val, src, file, fn_sym, scope, out);
                                arg_ids.push((name, vid));
                            }
                        }
                        "dictionary_splat" | "list_splat" => {
                            if let Some(inner) = a.named_child(0) {
                                let vid = py_flow_expr(inner, src, file, fn_sym, scope, out);
                                arg_ids.push((None, vid));
                            }
                        }
                        _ => {
                            let vid = py_flow_expr(a, src, file, fn_sym, scope, out);
                            arg_ids.push((None, vid));
                        }
                    }
                }
            }
            let is_ctor = callee_name.chars().next().is_some_and(|c| c.is_uppercase());
            let (kind, var) = if is_ctor { ("new", callee_name.as_str()) } else { ("call_res", "") };
            let id = push_node(out, file, pos.row as u32, pos.column as u32, kind, var, fn_sym);
            if let Some(r) = recv {
                out.edges.push(DfEdge { from: r.clone(), to: id.clone() });
                out.args.push((id.clone(), -1, r));
            }
            for (p, (name, vid)) in arg_ids.into_iter().enumerate() {
                out.edges.push(DfEdge { from: vid.clone(), to: id.clone() });
                out.args.push((id.clone(), p as i64, vid.clone()));
                if let Some(n) = name {
                    out.fields.push((id.clone(), n, vid));
                }
            }
            id
        }
        // `base.name` outside call-callee position: a member read, `var` is
        // the accessed name so a `df_field` write can be matched against a
        // read of the same field.
        "attribute" => {
            let obj = node.child_by_field_name("object").map(|o| py_flow_expr(o, src, file, fn_sym, scope, out));
            let name = node.child_by_field_name("attribute").map(|a| py_text(a, src)).unwrap_or_default();
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "member", &name, fn_sym);
            if let Some(o) = obj {
                out.edges.push(DfEdge { from: o, to: id.clone() });
            }
            id
        }
        "subscript" => {
            let val = node.child_by_field_name("value").map(|v| py_flow_expr(v, src, file, fn_sym, scope, out));
            let mut cur = node.walk();
            for sub in node.children_by_field_name("subscript", &mut cur) {
                let _ = py_flow_expr(sub, src, file, fn_sym, scope, out);
            }
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "member", "", fn_sym);
            if let Some(v) = val {
                out.edges.push(DfEdge { from: v, to: id.clone() });
            }
            id
        }
        "binary_operator" | "boolean_operator" | "comparison_operator" => {
            let mut cur = node.walk();
            let kids: Vec<tree_sitter::Node> = node.named_children(&mut cur).collect();
            let l = kids.first().map(|n| py_flow_expr(*n, src, file, fn_sym, scope, out));
            let r = kids.last().map(|n| py_flow_expr(*n, src, file, fn_sym, scope, out));
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "binop", "", fn_sym);
            if let Some(l) = l { out.edges.push(DfEdge { from: l, to: id.clone() }); }
            if let Some(r) = r { out.edges.push(DfEdge { from: r, to: id.clone() }); }
            id
        }
        "not_operator" | "unary_operator" => {
            let mut cur = node.walk();
            let v = node.named_children(&mut cur).next().map(|n| py_flow_expr(n, src, file, fn_sym, scope, out));
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "unop", "", fn_sym);
            if let Some(v) = v { out.edges.push(DfEdge { from: v, to: id.clone() }); }
            id
        }
        // `<true_expr> if <cond> else <false_expr>`: the value is EITHER
        // branch; the condition is walked for its own nested facts, never
        // edged in as a value (mirrors TS's ternary).
        "conditional_expression" => {
            let mut cur = node.walk();
            let kids: Vec<tree_sitter::Node> = node.named_children(&mut cur).collect();
            let cons = kids.first().map(|n| py_flow_expr(*n, src, file, fn_sym, scope, out));
            if let Some(cond) = kids.get(1) {
                let _ = py_flow_expr(*cond, src, file, fn_sym, scope, out);
            }
            let alt = kids.get(2).map(|n| py_flow_expr(*n, src, file, fn_sym, scope, out));
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "cond", "", fn_sym);
            if let Some(c) = cons { out.edges.push(DfEdge { from: c, to: id.clone() }); }
            if let Some(a) = alt { out.edges.push(DfEdge { from: a, to: id.clone() }); }
            id
        }
        "parenthesized_expression" | "await" => {
            let mut cur = node.walk();
            let inner = node.named_children(&mut cur).next();
            match inner {
                Some(inner) => py_flow_expr(inner, src, file, fn_sym, scope, out),
                None => push_node(out, file, pos.row as u32, pos.column as u32, "expr", "", fn_sym),
            }
        }
        // `lambda params: body`: lift as its OWN fn scope under a synthetic
        // `<enclosing>::closure::<row>_<col>` sym (mirrors Kotlin/TS inline
        // lambdas exactly) — param nodes + a `ret` node for the single body
        // expression (a lambda has no `return`, its body IS the return value)
        // — and mint the `closure` VALUE node here, carrying the lifted sym in
        // `var` (the join key `std/flow.dl`'s higher-order hop reads). The
        // enclosing `scope` is shared so captures resolve.
        "lambda" => {
            let lam_sym = format!("{fn_sym}::closure::{}_{}", pos.row, pos.column);
            if let Some(params) = node.child_by_field_name("parameters") {
                let mut cur = params.walk();
                for (i, p) in params.named_children(&mut cur).enumerate() {
                    let (name_opt, _ty) = py_param_name_and_type(p, src);
                    if let Some(pname) = name_opt {
                        let ppos = p.start_position();
                        let id = push_node(out, file, ppos.row as u32, ppos.column as u32, "param", &pname, &lam_sym);
                        out.param_pos.push((id.clone(), i as u32));
                        scope.insert(pname, id);
                    }
                }
            }
            if let Some(body) = node.child_by_field_name("body") {
                let v = py_flow_expr(body, src, file, &lam_sym, scope, out);
                let end = node.end_position();
                let ret = push_node(out, file, end.row as u32, end.column as u32, "ret", "", &lam_sym);
                out.edges.push(DfEdge { from: v, to: ret });
            }
            push_node(out, file, pos.row as u32, pos.column as u32, "closure", &lam_sym, fn_sym)
        }
        "list_comprehension" | "set_comprehension" | "generator_expression" | "dictionary_comprehension" => {
            py_comprehension_flow(node, src, file, fn_sym, scope, out)
        }
        "list" | "set" | "tuple" => {
            let mut cur = node.walk();
            let ids: Vec<String> = node.named_children(&mut cur)
                .map(|el| py_flow_expr(el, src, file, fn_sym, scope, out))
                .collect();
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "new", "", fn_sym);
            for v in ids {
                out.edges.push(DfEdge { from: v, to: id.clone() });
            }
            id
        }
        // `{...}`: each `pair`'s value flows into a `new` node; a plain-string
        // key becomes the `df_field` name (mirrors TS's ObjectExpression);
        // `**spread` lands under the ".." pseudo-field (the FRU convention).
        "dictionary" => {
            let mut cur = node.walk();
            let mut filled: Vec<(String, String)> = Vec::new();
            for child in node.named_children(&mut cur) {
                match child.kind() {
                    "pair" => {
                        let key = child.child_by_field_name("key");
                        let val = child.child_by_field_name("value")
                            .map(|v| py_flow_expr(v, src, file, fn_sym, scope, out));
                        let name = key.filter(|k| k.kind() == "string")
                            .and_then(|k| k.utf8_text(src).ok())
                            .map(|s| s.trim_matches(['"', '\'']).to_string())
                            .unwrap_or_default();
                        if let Some(v) = val {
                            filled.push((name, v));
                        }
                    }
                    "dictionary_splat" => {
                        if let Some(inner) = child.named_child(0) {
                            let v = py_flow_expr(inner, src, file, fn_sym, scope, out);
                            filled.push(("..".into(), v));
                        }
                    }
                    _ => {}
                }
            }
            let id = push_node(out, file, pos.row as u32, pos.column as u32, "new", "", fn_sym);
            for (name, v) in filled {
                out.edges.push(DfEdge { from: v.clone(), to: id.clone() });
                if !name.is_empty() {
                    out.fields.push((id.clone(), name, v));
                }
            }
            id
        }
        _ => {
            let mut cur = node.walk();
            let mut last = None;
            for c in node.named_children(&mut cur) {
                last = Some(py_flow_expr(c, src, file, fn_sym, scope, out));
            }
            last.unwrap_or_else(|| push_node(out, file, pos.row as u32, pos.column as u32, "expr", "", fn_sym))
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn has(got: &[TypeEdge], from: &str, to: &str, kind: &'static str) -> bool {
        got.contains(&TypeEdge { from: from.into(), to: to.into(), kind })
    }

    #[test]
    fn kotlin_fields_supers_variants_and_generics() {
        let src = r#"
package com.app
interface Pricing
abstract class Repo<T : Entity>(val store: Store, var meta: Meta?, ctor: Wire) : Base(1), Pricing {
    val cache: Cache<Item> = Cache()
}
object Single : Pricing
enum class Color(val rgb: Int) { RED, GREEN }
"#;
        let got = kotlin_edges(src);
        assert!(has(&got, "Repo", "Store", "field"), "{got:?}");
        assert!(has(&got, "Repo", "Meta", "field"), "{got:?}");
        assert!(has(&got, "Repo", "Cache", "field"), "{got:?}");
        assert!(has(&got, "Repo", "Item", "field"), "{got:?}");
        assert!(has(&got, "Repo", "Base", "impl"), "{got:?}");
        assert!(has(&got, "Repo", "Pricing", "impl"), "{got:?}");
        assert!(has(&got, "Repo", "Entity", "generic"), "{got:?}");
        assert!(has(&got, "Single", "Pricing", "impl"), "{got:?}");
        assert!(has(&got, "Color", "Color::RED", "variant"), "{got:?}");
        assert!(has(&got, "Color", "Color::GREEN", "variant"), "{got:?}");
        // bare ctor arg is not a field; type params and builtins are not refs
        assert!(!got.iter().any(|e| e.to == "Wire"), "{got:?}");
        assert!(!got.iter().any(|e| e.to == "T"), "{got:?}");
        assert!(!got.iter().any(|e| e.to == "Int"), "{got:?}");
    }

    #[test]
    fn kotlin_function_entities_carry_arrow_types() {
        let src = "\
package com.app
fun resolve(model: Model, n: Int): NodeId { return n }
fun <T : Entity> wrap(item: T, sink: Sink<Report>) {}
";
        let es = KotlinTypes.extract("src/app.kt", src).entities;
        let by = |name: &str| es.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("missing {name}: {es:?}"));
        let resolve = by("resolve");
        assert_eq!(resolve.kind, EntityKind::Function);
        let ty = resolve.ty.as_ref().unwrap();
        assert_eq!(ty.params[0], vec![TypeRef::Named("Model".into())]);
        assert!(ty.params[1].is_empty(), "Int is a builtin, no ref: {ty:?}");
        assert_eq!(ty.ret, vec![TypeRef::Named("NodeId".into())]);
        // declared type-param T excluded; owner + nested generic arg both kept;
        // no return type -> empty ret
        let wrap = by("wrap").ty.as_ref().unwrap();
        assert!(wrap.params[0].is_empty(), "type-param T is not a ref: {wrap:?}");
        assert!(wrap.params[1].contains(&TypeRef::Named("Sink".into())), "owner: {wrap:?}");
        assert!(wrap.params[1].contains(&TypeRef::Named("Report".into())), "nested arg: {wrap:?}");
        assert!(wrap.ret.is_empty(), "no declared return: {wrap:?}");
    }

    #[test]
    fn kotlin_interface_supertypes_are_generic_kind() {
        let src = "interface Tiered : Pricing\nclass Flat : Pricing\n";
        let got = kotlin_edges(src);
        assert!(has(&got, "Tiered", "Pricing", "generic"), "{got:?}");
        assert!(has(&got, "Flat", "Pricing", "impl"), "{got:?}");
    }

    #[test]
    fn kotlin_nested_and_qualified_types() {
        let src = r#"
class Outer {
    class Inner(val link: com.lib.Remote)
}
"#;
        let got = kotlin_edges(src);
        assert!(has(&got, "Inner", "com.lib.Remote", "field"), "{got:?}");
    }

    #[test]
    fn extracts_fields_variants_impls_and_generics() {
        let src = r#"
            trait Identity {}
            trait Store {}
            struct Id;
            struct Meta<T>(T);
            struct User<T: Identity> { id: Id, meta: Option<Meta<T>> }
            enum Event { Created(User<Id>), Deleted { id: Id } }
            impl<T: Identity> Store for User<T> {}
        "#;
        let got = edges(src);
        assert!(got.contains(&TypeEdge {
            from: "User".into(),
            to: "Id".into(),
            kind: "field"
        }));
        assert!(got.contains(&TypeEdge {
            from: "User".into(),
            to: "Identity".into(),
            kind: "generic"
        }));
        assert!(got.contains(&TypeEdge {
            from: "Event".into(),
            to: "Event::Created".into(),
            kind: "variant"
        }));
        assert!(got.contains(&TypeEdge {
            from: "Event::Created".into(),
            to: "User".into(),
            kind: "field"
        }));
        assert!(got.contains(&TypeEdge {
            from: "User".into(),
            to: "Store".into(),
            kind: "impl"
        }));
    }

    #[test]
    fn ts_fields_supers_variants_and_generics() {
        let src = r#"
interface Pricing {}
interface Entity { id: Id }
export interface Catalog<T extends Entity> extends Pricing {
    items: Map<Sku, T>
    name: string
}
export class Repo extends Base implements Pricing {
    cache: Cache<Item>
    constructor(private db: Db, wire: Wire) {}
}
export type Event = Created | Deleted<Reason> | "tombstone"
type Pair = [Left, Right]
enum Color { Red, Green = "g" }
"#;
        let got = ts_edges(src, false);
        assert!(has(&got, "Entity", "Id", "field"), "interface property: {got:?}");
        assert!(has(&got, "Catalog", "Pricing", "generic"), "interface extends: {got:?}");
        assert!(has(&got, "Catalog", "Entity", "generic"), "type-param bound: {got:?}");
        assert!(has(&got, "Catalog", "Sku", "field"), "generic arg in property: {got:?}");
        assert!(!got.iter().any(|e| e.to == "T"), "type-param name leaked: {got:?}");
        assert!(has(&got, "Repo", "Base", "impl"), "class extends: {got:?}");
        assert!(has(&got, "Repo", "Pricing", "impl"), "class implements: {got:?}");
        assert!(has(&got, "Repo", "Cache", "field"), "class property: {got:?}");
        assert!(has(&got, "Repo", "Item", "field"), "property generic arg: {got:?}");
        assert!(has(&got, "Repo", "Db", "field"), "ctor parameter property: {got:?}");
        assert!(!got.iter().any(|e| e.to == "Wire"), "plain ctor arg is not a field: {got:?}");
        assert!(has(&got, "Event", "Created", "variant"), "union alternative: {got:?}");
        assert!(has(&got, "Event", "Deleted", "variant"), "generic union alternative: {got:?}");
        assert!(has(&got, "Event", "Reason", "field"), "union alternative arg: {got:?}");
        assert!(has(&got, "Pair", "Left", "field"), "tuple alias member: {got:?}");
        assert!(has(&got, "Color", "Color::Red", "variant"), "enum member: {got:?}");
        assert!(has(&got, "Color", "Color::Green", "variant"), "initialized enum member: {got:?}");
        assert!(!got.iter().any(|e| e.to == "string"), "keyword type leaked: {got:?}");
    }

    #[test]
    fn tsx_parses_and_extracts() {
        let src = r#"
interface CardProps { item: Item; onPick: (s: Sku) => void }
export function Card({ item }: CardProps) { return <div>{item.name}</div> }
"#;
        let got = ts_edges(src, true);
        assert!(has(&got, "CardProps", "Item", "field"), "tsx interface prop: {got:?}");
        assert!(has(&got, "CardProps", "Sku", "field"), "function-type param ref: {got:?}");
    }

    #[test]
    fn ts_entities_kinds_lines_and_arrow_types() {
        let src = "\
export interface Entity { id: Id }
export type Event = A | B
export enum Color { Red }
export class Repo {
    find(q: Query): Entity { return q as Entity }
}
export function resolveIdent(model: Model, n: string): NodeId[] { return [] }
export const cone = (model: Model, mode: ConeMode): View => view()
";
        let es = ts_entities("src/core/model.ts", src, false);
        let by = |name: &str| es.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("missing {name}: {es:?}"));
        // kinds
        assert_eq!(by("Entity").kind, EntityKind::Interface);
        assert_eq!(by("Event").kind, EntityKind::Alias);
        assert_eq!(by("Color").kind, EntityKind::Enum);
        assert_eq!(by("Repo").kind, EntityKind::Class);
        assert_eq!(by("resolveIdent").kind, EntityKind::Function);
        assert_eq!(by("cone").kind, EntityKind::Function);
        // sem-style symbol + declaration line (1-based)
        assert_eq!(by("Entity").sym, "src/core/model.ts::interface::Entity");
        assert_eq!(by("Entity").line, 1);
        assert_eq!(by("resolveIdent").line, 7);
        // method: parented to the class, callable
        let find = by("find");
        assert_eq!(find.kind, EntityKind::Method);
        assert_eq!(find.parent.as_deref(), Some("src/core/model.ts::class::Repo"));
        assert_eq!(find.sym, "src/core/model.ts::method::Repo.find");
        // a function IS a type: [...A] => B
        let f = by("resolveIdent").ty.as_ref().unwrap();
        assert_eq!(f.params[0], vec![TypeRef::Named("Model".into())]);  // first param type
        assert!(f.params[1].is_empty(), "string is a keyword, no ref: {f:?}");
        assert_eq!(f.ret, vec![TypeRef::Named("NodeId".into())]);
        let a = by("cone").ty.as_ref().unwrap();
        assert_eq!(a.params[1], vec![TypeRef::Named("ConeMode".into())]);
        assert_eq!(a.ret, vec![TypeRef::Named("View".into())]);
    }

    #[test]
    fn rust_entities_kinds_and_arrow_types() {
        let src = "\
pub struct Engine { db: Db }
pub enum Mode { A, B }
pub trait Sink {}
pub fn run(e: Engine, n: usize) -> Report { todo!() }
impl Engine {
    pub fn tick(&self, db: Db) -> Result { todo!() }
}
";
        let es = rust_entities("src/engine.rs", src);
        let by = |name: &str| es.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("missing {name}: {es:?}"));
        assert_eq!(by("Engine").kind, EntityKind::Struct);
        assert_eq!(by("Mode").kind, EntityKind::Enum);
        assert_eq!(by("Sink").kind, EntityKind::Trait);
        assert_eq!(by("run").kind, EntityKind::Function);
        assert_eq!(by("Engine").line, 1);
        assert_eq!(by("run").line, 4);
        // free fn arrow type, receiver excluded on the method
        let run = by("run").ty.as_ref().unwrap();
        assert_eq!(run.params[0], vec![TypeRef::Named("Engine".into())]);
        assert!(run.params[1].is_empty(), "usize is primitive: {run:?}");
        assert_eq!(run.ret, vec![TypeRef::Named("Report".into())]);
        let tick = by("tick");
        assert_eq!(tick.kind, EntityKind::Method);
        assert_eq!(tick.parent.as_deref(), Some("src/engine.rs::struct::Engine"));
        let tty = tick.ty.as_ref().unwrap();
        assert_eq!(tty.params, vec![vec![TypeRef::Named("Db".into())]], "self dropped: {tty:?}");
    }

    // The invariant: for every entity E with `parent = Some(P)`, P is the EXACT
    // sym of some other entity in the same file — so `type_entity.parent` joins
    // `type_entity.sym` with zero normalization. Regression guard for the old
    // hardcoded `::class::` owner tag (a struct method read `::class::Foo`).
    #[test]
    fn entity_parent_joins_owner_sym_across_langs() {
        let check = |es: &[TypeEntity], lang: &str| {
            let syms: std::collections::HashSet<&str> = es.iter().map(|e| e.sym.as_str()).collect();
            for e in es {
                if let Some(p) = &e.parent {
                    assert!(syms.contains(p.as_str()), "[{lang}] dangling parent {p} on {}: {es:?}", e.sym);
                }
            }
        };
        let find = |es: &[TypeEntity], n: &str| {
            es.iter().find(|e| e.name == n).unwrap_or_else(|| panic!("missing {n}: {es:?}")).clone()
        };

        // Rust: struct + enum owners each get an impl method; trait present as a
        // top-level entity (trait default methods aren't emitted as entities, so
        // a trait is never itself a method owner here).
        let rust = "\
struct S { x: i32 }
enum E { A }
trait T {}
impl S { fn sm(&self) {} }
impl E { fn em(&self) {} }
";
        let re = RustTypes.extract("src/lib.rs", rust).entities;
        check(&re, "rust");
        assert_eq!(find(&re, "sm").parent.as_deref(), Some("src/lib.rs::struct::S"));
        assert_eq!(find(&re, "em").parent.as_deref(), Some("src/lib.rs::enum::E"));
        assert!(re.iter().any(|e| e.sym == "src/lib.rs::trait::T"), "trait entity: {re:?}");

        // TS: interface + class; a class method parents to the class sym.
        let ts = "\
export interface I { }
export class C { m(): void {} }
";
        let te = TsTypes.extract("src/m.ts", ts).entities;
        check(&te, "ts");
        assert_eq!(find(&te, "m").parent.as_deref(), Some("src/m.ts::class::C"));
        assert!(te.iter().any(|e| e.sym == "src/m.ts::interface::I"), "interface entity: {te:?}");

        // Kotlin: class + interface top-level entities (member fns are flat,
        // `parent = None`, so `check` asserts zero dangling parents). An
        // `object` decl carries a member fn too but isn't itself emitted as an
        // entity today (grammar gap, unrelated to the parent-kind bug).
        let kt = "\
class K { fun km() {} }
object O { fun om() {} }
interface Itf { fun im() }
";
        let ke = KotlinTypes.extract("src/K.kt", kt).entities;
        check(&ke, "kotlin");
        assert!(ke.iter().any(|e| e.sym == "src/K.kt::class::K"), "class entity: {ke:?}");
        assert!(ke.iter().any(|e| e.sym == "src/K.kt::interface::Itf"), "interface entity: {ke:?}");
    }

    #[test]
    fn ts_function_param_return_and_body_edges() {
        let src = r#"
export function resolveIdent(model: Model, ident: string): NodeId[] {
    const seen: Visited = new Map()
    return model.lookup(ident) as NodeId[]
}
export const cone = <C extends Ctx>(model: Model, mode: ConeMode): View => {
    const acc: Accumulator = init()
    return acc.done()
}
function helper(raw: Raw) {}
"#;
        let got = ts_edges(src, false);
        // function declaration: params in, return out, body refs internal
        assert!(has(&got, "resolveIdent", "Model", "param"), "fn param type: {got:?}");
        assert!(has(&got, "resolveIdent", "NodeId", "returns"), "fn return type: {got:?}");
        assert!(has(&got, "resolveIdent", "Visited", "uses"), "body annotation: {got:?}");
        assert!(has(&got, "resolveIdent", "NodeId", "uses"), "body cast `as NodeId[]`: {got:?}");
        // arrow const: same three kinds, type-param bound is generic + excluded
        assert!(has(&got, "cone", "Model", "param"), "arrow param: {got:?}");
        assert!(has(&got, "cone", "ConeMode", "param"), "arrow param 2: {got:?}");
        assert!(has(&got, "cone", "View", "returns"), "arrow return: {got:?}");
        assert!(has(&got, "cone", "Accumulator", "uses"), "arrow body: {got:?}");
        assert!(has(&got, "cone", "Ctx", "generic"), "type-param bound: {got:?}");
        assert!(!got.iter().any(|e| e.from == "cone" && e.to == "C"), "type-param name leaked: {got:?}");
        // un-exported function still owns edges; keyword param type is no ref
        assert!(has(&got, "helper", "Raw", "param"), "non-exported fn: {got:?}");
        assert!(!got.iter().any(|e| e.to == "string"), "keyword param leaked: {got:?}");
    }

    fn edge(a: &str, b: &str, k: &str) -> (String, String, String) {
        (a.into(), b.into(), k.into())
    }
    fn shape_hash_of(hashes: &[(String, String)], name: &str) -> String {
        hashes.iter().find(|(n, _)| n == name).map(|(_, h)| h.clone())
            .unwrap_or_else(|| panic!("no hash for {name}: {hashes:?}"))
    }

    #[test]
    fn type_shape_iso_two_structs_same_arity_same_hash() {
        // Point{x: f64, y: f64} and Coord{lat: f64, lon: f64} — both hold
        // two leaves. Names differ; shape identical.
        let edges = vec![
            edge("Point", "f64", "field"), edge("Point", "g64", "field"),
            edge("Coord", "h64", "field"), edge("Coord", "i64", "field"),
        ];
        let h = type_shape_hashes(&edges);
        assert_eq!(shape_hash_of(&h, "Point"), shape_hash_of(&h, "Coord"));
        // The leaves themselves all hash alike (LEAF sentinel).
        assert_eq!(shape_hash_of(&h, "f64"), shape_hash_of(&h, "h64"));
    }

    #[test]
    fn type_shape_different_arity_different_hash() {
        let edges = vec![
            edge("One", "f", "field"),
            edge("Two", "g", "field"), edge("Two", "h", "field"),
        ];
        let h = type_shape_hashes(&edges);
        assert_ne!(shape_hash_of(&h, "One"), shape_hash_of(&h, "Two"));
    }

    #[test]
    fn type_shape_recursive_type_converges() {
        // struct List { head: i32, tail: Box<List> } — self-reference.
        let edges = vec![
            edge("List", "i32", "field"),
            edge("List", "Box_List", "field"),
            edge("Box_List", "List", "field"),
        ];
        let h = type_shape_hashes(&edges);
        // Smoke: the function terminates and produces a stable hash for List.
        let list_hash = shape_hash_of(&h, "List");
        // Running twice gives the same answer (fixpoint is deterministic).
        let h2 = type_shape_hashes(&edges);
        assert_eq!(list_hash, shape_hash_of(&h2, "List"));
        // And the self-referential shape differs from a flat 2-field struct.
        let flat = vec![edge("Flat", "a", "field"), edge("Flat", "b", "field")];
        let hf = type_shape_hashes(&flat);
        assert_ne!(list_hash, shape_hash_of(&hf, "Flat"));
    }

    #[test]
    fn type_shape_impl_and_generic_excluded() {
        // Two structs with the same fields but different impls/generics should
        // hash alike — impl/generic aren't shape.
        let a = vec![
            edge("Foo", "i32", "field"),
            edge("Foo", "u32", "field"),
            edge("Foo", "Drop", "impl"),
            edge("Foo", "T", "generic"),
        ];
        let b = vec![
            edge("Bar", "i32", "field"),
            edge("Bar", "u32", "field"),
        ];
        assert_eq!(shape_hash_of(&type_shape_hashes(&a), "Foo"),
                   shape_hash_of(&type_shape_hashes(&b), "Bar"));
    }

    #[test]
    fn type_shape_variant_edges_count_as_shape() {
        // enum Action { Save(Path), Quit } vs struct Wrapper{ a: Path, b: Leaf }
        // — both have two data children, one of which is Path.
        let a = vec![
            edge("Action", "Path", "variant"),
            edge("Action", "Quit", "variant"),
        ];
        let b = vec![
            edge("Wrapper", "Path", "field"),
            edge("Wrapper", "Leaf", "field"),
        ];
        assert_eq!(shape_hash_of(&type_shape_hashes(&a), "Action"),
                   shape_hash_of(&type_shape_hashes(&b), "Wrapper"));
    }

    #[test]
    fn lgg_identical_types_zero_vars() {
        // Two types with identical field structure but distinct names. A and A2
        // have 2 fields each pointing at distinct leaves (a, b vs c, d), so
        // var_count(A, A2) = 2 (two slots each generalizing to a fresh var).
        let edges = vec![
            edge("A", "a", "field"), edge("A", "b", "field"),
            edge("A2", "c", "field"), edge("A2", "d", "field"),
        ];
        let pairs = type_lgg_pairs(&edges);
        let aa2 = pairs.iter().find(|(x, y, _)| x == "A" && y == "A2").map(|(_, _, v)| *v);
        assert_eq!(aa2, Some(2));
        // Every emitted pair has var_count >= 1 (vars == 0 is filtered).
        assert!(pairs.iter().all(|(_, _, v)| *v >= 1));
        // And no pair has identical names.
        assert!(pairs.iter().all(|(a, b, _)| a != b));
    }

    #[test]
    fn lgg_completely_identical_zero_skipped() {
        // Identical type names: lgg(A, A) returns 0, not emitted.
        let edges = vec![edge("A", "x", "field")];
        // Only one type name with edges + x; no a<b pair where a==b.
        let pairs = type_lgg_pairs(&edges);
        assert!(pairs.iter().all(|(a, b, _)| a != b));
    }

    #[test]
    fn lgg_different_arity_one_var() {
        // A has 2 fields, B has 1. Arity differs → opaque generalization.
        let edges = vec![
            edge("A", "x", "field"), edge("A", "y", "field"),
            edge("B", "z", "field"),
        ];
        let pairs = type_lgg_pairs(&edges);
        let ab = pairs.iter().find(|(p, q, _)| p == "A" && q == "B").map(|(_, _, v)| *v);
        assert_eq!(ab, Some(1));
    }

    #[test]
    fn lgg_shared_child_zero_vars_for_that_slot() {
        // A and B both have a field of the SAME type C. The C/C slot is 0 vars.
        // The other slot differs (X vs Y) → 1 var. Total = 1.
        let edges = vec![
            edge("A", "C", "field"), edge("A", "X", "field"),
            edge("B", "C", "field"), edge("B", "Y", "field"),
        ];
        let pairs = type_lgg_pairs(&edges);
        let ab = pairs.iter().find(|(p, q, _)| p == "A" && q == "B").map(|(_, _, v)| *v);
        assert_eq!(ab, Some(1));
    }

    // --- dataflow lift: instantiations, positional args, named fields, members

    fn dnode<'a>(df: &'a DataflowFacts, kind: &str, var: &str) -> &'a DfNode {
        df.nodes
            .iter()
            .find(|n| n.kind == kind && n.var == var)
            .unwrap_or_else(|| panic!("no node {kind}/{var}: {:?}", df.nodes))
    }

    fn has_arg(df: &DataflowFacts, call: &str, pos: i64, arg: &str) -> bool {
        df.args.iter().any(|(c, p, a)| c == call && *p == pos && a == arg)
    }

    fn has_field(df: &DataflowFacts, id: &str, field: &str, value: &str) -> bool {
        df.fields.iter().any(|(i, f, v)| i == id && f == field && v == value)
    }

    #[test]
    fn rust_lift_ctors_args_fields_members() {
        let src = "struct Cfg { host: i32, port: i32 }\n\
                   fn go(h: i32, items: Vec<i32>) {\n    \
                       let c = Cfg { host: h, port: 1 };\n    \
                       let x = c.host;\n    \
                       let w = Wrap(x);\n    \
                       let n = items.len();\n    \
                       eat(n, x);\n\
                   }\n";
        let df = RustTypes.extract_dataflow("f.rs", src);

        // struct literal and tuple-struct ctor are `new` nodes with type names.
        let cfg = dnode(&df, "new", "Cfg").id.clone();
        let wrap = dnode(&df, "new", "Wrap").id.clone();
        // struct-literal fields land in df_field by name.
        let h_read = df.nodes.iter().find(|n| n.kind == "var_read" && n.var == "h").unwrap().id.clone();
        assert!(has_field(&df, &cfg, "host", &h_read), "{:?}", df.fields);
        assert!(df.fields.iter().any(|(i, f, _)| i == &cfg && f == "port"), "{:?}", df.fields);
        // `.host` is a member read carrying the field name.
        let member = dnode(&df, "member", "host");
        assert!(df.edges.iter().any(|e| e.to == member.id), "member has a base edge");
        // tuple-struct ctor arg at slot 0.
        let x_reads: Vec<&DfNode> = df.nodes.iter().filter(|n| n.kind == "var_read" && n.var == "x").collect();
        assert!(x_reads.iter().any(|x| has_arg(&df, &wrap, 0, &x.id)), "{:?}", df.args);
        // method receiver at slot -1: items.len().
        let items_read = df.nodes.iter().find(|n| n.kind == "var_read" && n.var == "items").unwrap();
        assert!(df.args.iter().any(|(_, p, a)| *p == -1 && a == &items_read.id), "{:?}", df.args);
        // eat(n, x): slots 0 and 1 on the same call.
        let n_read = df.nodes.iter().find(|n| n.kind == "var_read" && n.var == "n").unwrap();
        let eat_call = df.args.iter().find(|(_, p, a)| *p == 0 && a == &n_read.id).map(|(c, _, _)| c.clone())
            .expect("eat call with n at slot 0");
        assert!(df.args.iter().any(|(c, p, a)| c == &eat_call && *p == 1
            && x_reads.iter().any(|x| a == &x.id)), "{:?}", df.args);
    }

    #[test]
    fn kotlin_lift_ctor_named_args_and_members() {
        let src = "class Cfg(val host: Int, val port: Int)\n\
                   fun go(h: Int) {\n    \
                       val c = Cfg(host = h, port = 1)\n    \
                       val x = c.host\n    \
                       val n = c.count()\n    \
                       val u = go2(x)\n\
                   }\n";
        let df = KotlinTypes.extract_dataflow("f.kt", src);

        // capitalized callee = ctor call = `new` node with the type name.
        let cfg = dnode(&df, "new", "Cfg").id.clone();
        // named args land in df_field AND keep their source slot in df_arg.
        let h_read = df.nodes.iter().find(|n| n.kind == "var_read" && n.var == "h").unwrap().id.clone();
        assert!(has_field(&df, &cfg, "host", &h_read), "{:?}", df.fields);
        assert!(df.fields.iter().any(|(i, f, _)| i == &cfg && f == "port"), "{:?}", df.fields);
        assert!(has_arg(&df, &cfg, 0, &h_read), "{:?}", df.args);
        // the named-arg label is NOT a var_read (it's a label, not a value).
        assert!(
            !df.nodes.iter().any(|n| n.kind == "var_read" && n.var == "host"),
            "named-arg label leaked as a read: {:?}", df.nodes
        );
        // `.host` outside a call is a member read carrying the name.
        let member = dnode(&df, "member", "host");
        assert!(df.edges.iter().any(|e| e.to == member.id), "member has a base edge");
        // navigation callee: c.count() flows the receiver in at slot -1.
        assert!(
            df.args.iter().any(|(_, p, a)| *p == -1
                && df.nodes.iter().any(|n| &n.id == a && n.kind == "var_read" && n.var == "c")),
            "{:?}", df.args
        );
        // lowercase callee stays a call with slot-0 arg.
        let go2 = df.nodes.iter().filter(|n| n.kind == "call_res").count();
        assert!(go2 >= 1, "go2(x) should stay call_res: {:?}", df.nodes);
    }

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

    /// Shared gate for the lambda lift: the `closure` value node sits at the
    /// call's expected arg slot, its `var` names the lifted scope, and that
    /// scope holds a positional `param` plus a `ret` fed by the body.
    fn assert_lambda_lifted(df: &DataflowFacts, lam_slot: i64, param_var: &str) {
        let clo = df.nodes.iter().find(|n| n.kind == "closure").expect("closure node");
        let lam_sym = clo.var.clone();
        assert!(lam_sym.contains("::closure::"), "closure var carries the lifted sym: {clo:?}");
        // the closure VALUE lives in the enclosing fn, not its own scope.
        assert_ne!(clo.fn_sym, lam_sym, "{clo:?}");
        assert!(
            df.args.iter().any(|(_, p, a)| *p == lam_slot && a == &clo.id),
            "closure at arg slot {lam_slot}: {:?}", df.args
        );
        let param = df.nodes.iter()
            .find(|n| n.kind == "param" && n.var == param_var && n.fn_sym == lam_sym)
            .unwrap_or_else(|| panic!("param {param_var} under {lam_sym}: {:?}", df.nodes));
        assert!(
            df.param_pos.iter().any(|(i, p)| i == &param.id && *p == 0),
            "lambda param at slot 0: {:?}", df.param_pos
        );
        let ret = df.nodes.iter()
            .find(|n| n.kind == "ret" && n.fn_sym == lam_sym)
            .unwrap_or_else(|| panic!("ret under {lam_sym}: {:?}", df.nodes));
        // body value reaches the ret node (param -> binop -> ret here).
        assert!(df.edges.iter().any(|e| e.to == ret.id), "{:?}", df.edges);
    }

    #[test]
    fn rust_inline_closure_lifts_as_own_scope() {
        let src = "fn go(xs: Vec<i32>) {\n    let out = xs.map(|x| x + 1);\n}\n";
        let df = RustTypes.extract_dataflow("f.rs", src);
        assert_lambda_lifted(&df, 0, "x");
        // capture still resolves: the shared scope links an outer read.
        let src2 = "fn go(k: i32, xs: Vec<i32>) {\n    let out = xs.map(|x| x + k);\n}\n";
        let df2 = RustTypes.extract_dataflow("f.rs", src2);
        let k_param = df2.nodes.iter().find(|n| n.kind == "param" && n.var == "k").unwrap();
        let k_read = df2.nodes.iter().find(|n| n.kind == "var_read" && n.var == "k").unwrap();
        assert!(
            df2.edges.iter().any(|e| e.from == k_param.id && e.to == k_read.id),
            "capture edge: {:?}", df2.edges
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
    fn kotlin_trailing_lambda_lifts_with_implicit_it() {
        // trailing lambda with no parameter list: implicit `it` at slot 0.
        let src = "fun go(xs: List<Int>) {\n    val out = xs.map { it + 1 }\n}\n";
        let df = KotlinTypes.extract_dataflow("f.kt", src);
        assert_lambda_lifted(&df, 0, "it");
        // declared parameter form binds by name; trailing lambda still slots
        // after the parenthesized args (fold's accumulator lambda at slot 1).
        let src2 = "fun go(xs: List<Int>) {\n    val out = xs.fold(0) { acc, x -> acc + x }\n}\n";
        let df2 = KotlinTypes.extract_dataflow("f.kt", src2);
        let clo = df2.nodes.iter().find(|n| n.kind == "closure").expect("closure node");
        assert!(
            df2.args.iter().any(|(_, p, a)| *p == 1 && a == &clo.id),
            "trailing lambda after one paren arg sits at slot 1: {:?}", df2.args
        );
        let lam_sym = clo.var.clone();
        let pos_of = |v: &str| df2.nodes.iter()
            .find(|n| n.kind == "param" && n.var == v && n.fn_sym == lam_sym)
            .and_then(|n| df2.param_pos.iter().find(|(i, _)| i == &n.id).map(|(_, p)| *p));
        assert_eq!(pos_of("acc"), Some(0), "{:?}", df2.nodes);
        assert_eq!(pos_of("x"), Some(1), "{:?}", df2.nodes);
    }

    #[test]
    fn go_fields_embeds_and_generic_constraints() {
        let src = "\
package app

type Pricing interface {
	Price() int
}

type Store struct{}

type Repo[T Entity] struct {
	Store
	*Pricing
	cache Cache
	items []Item
}

type Color int

const (
	Red Color = iota
)
";
        let got = go_edges_from(go_parse(src).unwrap().root_node(), src.as_bytes());
        assert!(has(&got, "Repo", "Store", "impl"), "{got:?}"); // embedded (no field name)
        assert!(has(&got, "Repo", "Pricing", "impl"), "{got:?}"); // embedded via pointer
        assert!(has(&got, "Repo", "Cache", "field"), "{got:?}");
        assert!(has(&got, "Repo", "Item", "field"), "{got:?}");
        assert!(has(&got, "Repo", "Entity", "generic"), "{got:?}");
        // declared type param T is not itself a ref, and builtin `int` is noise.
        assert!(!got.iter().any(|e| e.to == "T"), "{got:?}");
        assert!(!got.iter().any(|e| e.to == "int"), "{got:?}");
    }

    #[test]
    fn go_entities_cover_struct_interface_alias_function_method() {
        let src = "\
package app

type Store struct {
	Host string
}

type Pricing interface {
	Price() int
}

type ID = string

func Resolve(name string, count int) (Store, error) { return Store{}, nil }

func (s *Store) Name() string { return s.Host }
";
        let facts = GoTypes.extract("app/store.go", src);
        let by = |name: &str| facts.entities.iter().find(|e| e.name == name)
            .unwrap_or_else(|| panic!("missing {name}: {:?}", facts.entities));
        assert_eq!(by("Store").kind, EntityKind::Struct);
        assert_eq!(by("Pricing").kind, EntityKind::Interface);
        assert_eq!(by("ID").kind, EntityKind::Alias);
        let resolve = by("Resolve");
        assert_eq!(resolve.kind, EntityKind::Function);
        let ty = resolve.ty.as_ref().unwrap();
        assert_eq!(ty.params[0], vec![]); // `string` is a builtin, no ref
        assert_eq!(ty.params[1], vec![]); // `int` is a builtin, no ref
        // multi-value return: both result types union into one flat `ret` list.
        assert!(ty.ret.contains(&TypeRef::Named("Store".into())), "{ty:?}");
        assert!(!ty.ret.contains(&TypeRef::Named("error".into())), "error is builtin noise: {ty:?}");

        let name_method = facts.entities.iter().find(|e| e.name == "Name" && e.kind == EntityKind::Method)
            .unwrap_or_else(|| panic!("missing Name method: {:?}", facts.entities));
        // receiver `*Store` strips the pointer; parent joins Store's OWN sym.
        assert_eq!(name_method.parent.as_deref(), Some(mint_sym("app/store.go", EntityKind::Struct, "Store", None).as_str()));
        assert_eq!(name_method.sym, mint_sym("app/store.go", EntityKind::Method, "Name", Some("Store")));
    }

    #[test]
    fn go_dataflow_param_call_member_and_composite_literal() {
        let src = "\
package app

func build(host string) Widget {
	w := Widget{Host: host, Port: 1}
	n := w.Host
	Log(n)
	return w
}
";
        let df = GoTypes.extract_dataflow("f.go", src);
        // param seeded at slot 0.
        let host_param = dnode(&df, "param", "host");
        assert_eq!(df.param_pos.iter().find(|(i, _)| i == &host_param.id).map(|(_, p)| *p), Some(0));
        // composite literal: `new` node carrying the type name, keyed field flows.
        let widget = dnode(&df, "new", "Widget").id.clone();
        let host_read = df.nodes.iter().find(|n| n.kind == "var_read" && n.var == "host").unwrap().id.clone();
        assert!(has_field(&df, &widget, "Host", &host_read), "{:?}", df.fields);
        assert!(df.fields.iter().any(|(i, f, _)| i == &widget && f == "Port"), "{:?}", df.fields);
        // `.Host` outside a call is a member read carrying the field name.
        let member = dnode(&df, "member", "Host");
        assert!(df.edges.iter().any(|e| e.to == member.id), "{:?}", df.edges);
        // `Log(n)`: plain call stays call_res with a slot-0 arg.
        let n_read = df.nodes.iter().find(|n| n.kind == "var_read" && n.var == "n").unwrap().id.clone();
        let call = dnode(&df, "call_res", "").id.clone();
        assert!(has_arg(&df, &call, 0, &n_read), "{:?}", df.args);
        // `return w`: one ret node fed by the read of `w`.
        let ret = df.nodes.iter().find(|n| n.kind == "ret").expect("ret node");
        assert!(df.edges.iter().any(|e| e.to == ret.id), "{:?}", df.edges);
    }

    #[test]
    fn go_for_range_loop_span_and_nest() {
        let src = "\
package app

func sum(xs []int) int {
	total := 0
	for _, x := range xs {
		total = total + Compute(x)
	}
	return total
}
";
        let df = GoTypes.extract_dataflow("f.go", src);
        assert_eq!(df.loops.len(), 1, "{:?}", df.loops);
        assert_eq!(df.loops[0].var, "x");
        let call = df.nodes.iter().find(|n| n.kind == "call_res").expect("Compute call");
        assert!(df.nests.iter().any(|n| n.call_id == call.id), "{:?}", df.nests);
    }

    #[test]
    fn go_func_literal_lifts_as_own_scope() {
        let src = "\
package app

func process() {
	apply(func(x int) int { return x + 1 })
}
";
        let df = GoTypes.extract_dataflow("f.go", src);
        assert_lambda_lifted(&df, 0, "x");
    }

    #[test]
    fn go_doc_comment_block_and_deprecated_tag() {
        let src = r#"
package app

// Store holds pricing data.
//
// Deprecated: use Repo instead.
type Store struct{}

// Name returns the display name.
func (s *Store) Name() string { return "" }
"#;
        let facts = GoTypes.extract("app/store.go", src);
        let store_sym = mint_sym("app/store.go", EntityKind::Struct, "Store", None);
        let doc = facts.docs.iter().find(|d| d.sym == store_sym)
            .unwrap_or_else(|| panic!("missing Store doc: {:?}", facts.docs));
        assert!(doc.text.starts_with("Store holds pricing data."), "{:?}", doc.text);
        assert!(doc.tags.iter().any(|t| t.tag == "deprecated" && t.text == "use Repo instead."), "{:?}", doc.tags);
        let method_sym = mint_sym("app/store.go", EntityKind::Method, "Name", Some("Store"));
        assert!(facts.docs.iter().any(|d| d.sym == method_sym), "{:?}", facts.docs);
    }
    // ── Python ───────────────────────────────────────────────────────────────

    #[test]
    fn python_entities_class_method_function_module() {
        let src = "\"\"\"Module doc.\"\"\"\n\n\nclass Repo:\n    def fetch(self, id: int) -> Report:\n        return Report()\n\n\ndef helper(n: int) -> int:\n    return n\n";
        let es = PyTypes.extract("app.py", src).entities;
        let module = es.iter().find(|e| e.kind == EntityKind::Module).expect("module entity");
        assert_eq!(module.line, 1);
        let repo = es.iter().find(|e| e.name == "Repo").expect("class entity");
        assert_eq!(repo.kind, EntityKind::Class);
        assert!(repo.parent.is_none());
        let fetch = es.iter().find(|e| e.name == "fetch").expect("method entity");
        assert_eq!(fetch.kind, EntityKind::Method);
        assert_eq!(fetch.parent.as_deref(), Some(repo.sym.as_str()));
        // self dropped: one param slot only for `id`, which is a builtin (no ref).
        let ty = fetch.ty.as_ref().unwrap();
        assert_eq!(ty.params.len(), 1, "{ty:?}");
        assert!(ty.params[0].is_empty(), "int is a builtin, no ref: {ty:?}");
        assert_eq!(ty.ret, vec![TypeRef::Named("Report".into())]);
        let helper = es.iter().find(|e| e.name == "helper").expect("function entity");
        assert_eq!(helper.kind, EntityKind::Function);
        assert!(helper.parent.is_none());
    }

    #[test]
    fn python_edges_bases_fields_params_returns_and_subscript_inner() {
        let src = "\
from typing import Optional


class Base:
    pass


class Widget(Base):
    label: Optional[str]

    def render(self, item: Optional[Widget]) -> Optional[Report]:
        note: Widget = item
        return note
";
        let facts = PyTypes.extract("app.py", src);
        let got = &facts.edges;
        assert!(has(got, "Widget", "Base", "impl"), "{got:?}");
        // Optional[str] -> "str" is noise-filtered (builtin), so no field edge
        // to str, but "Optional" itself must never appear as a ref either.
        assert!(!got.iter().any(|e| e.to == "Optional"), "{got:?}");
        assert!(has(got, "render", "Widget", "param"), "{got:?}");
        assert!(has(got, "render", "Report", "returns"), "{got:?}");
        assert!(has(got, "render", "Widget", "uses"), "{got:?}");
    }

    #[test]
    fn python_calls_ctor_and_attribute_callee() {
        let src = "\
class Widget:
    def render(self):
        return self.helper()


def make(store):
    w = Widget()
    return store.save(w)
";
        let facts = PyTypes.extract_calls("app.py", src);
        let render_def = facts.defs.iter().find(|d| d.name == "render").expect("render def");
        assert_eq!(render_def.kind, CallKind::Method);
        let make_def = facts.defs.iter().find(|d| d.name == "make").expect("make def");
        assert_eq!(make_def.kind, CallKind::Free);
        // attribute call: bare trailing name only.
        assert!(facts.sites.iter().any(|s| s.callee == "helper"), "{:?}", facts.sites);
        assert!(facts.sites.iter().any(|s| s.callee == "save"), "{:?}", facts.sites);
        // capitalized bare call is present as a call site too (ctor df_node is
        // a dataflow-layer concept, checked separately below).
        assert!(facts.sites.iter().any(|s| s.callee == "Widget"), "{:?}", facts.sites);
    }

    #[test]
    fn python_dataflow_ctor_kwarg_lambda_and_comprehension_loop_span() {
        let src = "\
def build(xs):
    item = Widget(label=\"x\")
    doubled = [n * 2 for n in xs]
    fn = lambda value: value + 1
    return fn(item)
";
        let df = PyTypes.extract_dataflow("app.py", src);
        // capitalized call mints a `new` node carrying the type name.
        let ctor = df.nodes.iter().find(|n| n.kind == "new" && n.var == "Widget").expect("ctor node");
        // keyword argument also lands in df_field under its name.
        assert!(df.fields.iter().any(|(id, name, _)| id == &ctor.id && name == "label"), "{:?}", df.fields);
        // list comprehension records a loop span with its loop variable.
        assert!(df.loops.iter().any(|l| l.var == "n"), "{:?}", df.loops);
        // lambda lifts as its own closure scope with a param node.
        let closure = df.nodes.iter().find(|n| n.kind == "closure").expect("closure node");
        let lam_sym = closure.var.clone();
        assert!(
            df.nodes.iter().any(|n| n.kind == "param" && n.fn_sym == lam_sym && n.var == "value"),
            "{:?}", df.nodes
        );
    }

    #[test]
    fn python_docstring_and_sphinx_tags() {
        let src = "\
def compute(count):
    \"\"\"Compute a thing.

    :param count: how many
    :returns: the result
    \"\"\"
    return count
";
        let docs = PyTypes.extract("app.py", src).docs;
        let doc = docs.iter().find(|d| d.text.starts_with("Compute a thing")).expect("docstring");
        let param_tag = doc.tags.iter().find(|t| t.tag == "param").expect("param tag");
        assert_eq!(param_tag.arg, "count");
        assert_eq!(param_tag.text, "how many");
        assert!(doc.tags.iter().any(|t| t.tag == "returns"), "{:?}", doc.tags);
    }

    // ── template_parts ──────────────────────────────────────────────────────

    #[test]
    fn template_parts_static_then_expr_then_static() {
        let src = "const route = `GET /users/${userId}/posts`;\n";
        let parts = ts_template_parts("route.ts", src);
        assert_eq!(parts.len(), 3, "{:?}", parts);
        assert_eq!((parts[0].idx, parts[0].kind, parts[0].text.as_str()),
                   (0, "static", "GET /users/"));
        assert_eq!((parts[1].idx, parts[1].kind, parts[1].text.as_str()),
                   (1, "expr", "userId"));
        assert_eq!((parts[2].idx, parts[2].kind, parts[2].text.as_str()),
                   (2, "static", "/posts"));
        // one occurrence: every piece shares the same node id.
        assert_eq!(parts[0].node, parts[1].node);
        assert_eq!(parts[1].node, parts[2].node);
    }

    #[test]
    fn template_parts_adjacent_statics_and_expr_only() {
        // `${a}${b}`: quasis/expressions strictly alternate (quasis.len() ==
        // expressions.len() + 1), so back-to-back interpolations with no
        // literal text between them still produce an (empty) static row
        // between them — idx never skips a slot.
        let src = "const both = `${a}${b}`;\nconst justExpr = `${onlyExpr}`;\n";
        let both = ts_template_parts("both.ts", src);
        let first_node = both[0].node.clone();
        let first_occurrence: Vec<_> = both.iter().filter(|p| p.node == first_node).collect();
        assert_eq!(first_occurrence.len(), 5, "{:?}", both);
        assert_eq!(
            first_occurrence.iter().map(|p| (p.idx, p.kind, p.text.as_str())).collect::<Vec<_>>(),
            vec![(0, "static", ""), (1, "expr", "a"), (2, "static", ""), (3, "expr", "b"), (4, "static", "")],
        );
        // second template: expr-only occurrence still opens and closes with
        // (empty) static chunks around the single interpolation.
        let second_node = both.iter().map(|p| p.node.clone()).find(|n| *n != first_node).expect("second node");
        let second_occurrence: Vec<_> = both.iter().filter(|p| p.node == second_node).collect();
        assert_eq!(
            second_occurrence.iter().map(|p| (p.idx, p.kind, p.text.as_str())).collect::<Vec<_>>(),
            vec![(0, "static", ""), (1, "expr", "onlyExpr"), (2, "static", "")],
        );
    }

    #[test]
    fn template_parts_empty_template_yields_one_static_row() {
        let src = "const blank = ``;\n";
        let parts = ts_template_parts("blank.ts", src);
        assert_eq!(parts.len(), 1, "{:?}", parts);
        assert_eq!((parts[0].idx, parts[0].kind, parts[0].text.as_str()), (0, "static", ""));
    }

    #[test]
    fn template_parts_backtick_escapes_stay_verbatim() {
        // raw (not cooked): \n stays as the two source characters backslash+n,
        // and an escaped backtick/dollar stays escaped, exactly as written.
        let src = r#"const s = `line one\nline two \` and \${notAnExpr}`;
"#;
        let parts = ts_template_parts("esc.ts", src);
        assert_eq!(parts.len(), 1, "{:?}", parts);
        assert_eq!(parts[0].kind, "static");
        assert_eq!(parts[0].text, r"line one\nline two \` and \${notAnExpr}");
    }

    #[test]
    fn template_parts_nested_template_mints_its_own_node() {
        // the outer's interpolation slot is itself a template literal; it gets
        // its own independent node/idx sequence, while the outer's `expr`
        // piece for that slot carries the nested template's full source text.
        let src = "const s = `outer ${`inner ${value}`}`;\n";
        let parts = ts_template_parts("nested.ts", src);
        let nodes: std::collections::HashSet<String> = parts.iter().map(|p| p.node.clone()).collect();
        assert_eq!(nodes.len(), 2, "{:?}", parts);

        let outer_node = parts.iter().find(|p| p.text == "outer ").expect("outer static").node.clone();
        let outer: Vec<_> = parts.iter().filter(|p| p.node == outer_node).collect();
        // one interpolation slot -> 2 quasis (leading "outer ", trailing "")
        // plus the 1 expr piece in between.
        assert_eq!(
            outer.iter().map(|p| (p.idx, p.kind, p.text.as_str())).collect::<Vec<_>>(),
            vec![(0, "static", "outer "), (1, "expr", "`inner ${value}`"), (2, "static", "")],
        );

        let inner_node = parts.iter().find(|p| p.text == "value").expect("inner expr").node.clone();
        assert_ne!(inner_node, outer_node);
        let inner: Vec<_> = parts.iter().filter(|p| p.node == inner_node).collect();
        assert_eq!(
            inner.iter().map(|p| (p.idx, p.kind, p.text.as_str())).collect::<Vec<_>>(),
            vec![(0, "static", "inner "), (1, "expr", "value"), (2, "static", "")],
        );
    }

    #[test]
    fn template_parts_tagged_template_uses_quasi_pieces() {
        // `` styled.div`color: ${c}` ``: the tag isn't part of the split, only
        // the quasi (the backtick-delimited literal) is.
        let src = "const box = styled.div`color: ${c};`;\n";
        let parts = ts_template_parts("tagged.ts", src);
        assert_eq!(
            parts.iter().map(|p| (p.idx, p.kind, p.text.as_str())).collect::<Vec<_>>(),
            vec![(0, "static", "color: "), (1, "expr", "c"), (2, "static", ";")],
        );
    }

    // --- string-values arc (df_lit + const_value + concat) ---

    #[test]
    fn ts_string_literal_const_mints_entity_and_value() {
        let src = "const home = '/home';\n";
        let facts = TsTypes.extract("f.ts", src);
        let ent = facts.entities.iter().find(|e| e.name == "home").expect("const entity");
        assert_eq!(ent.kind, EntityKind::Const);
        let row = facts.consts.iter().find(|c| c.sym == ent.sym).expect("const_value row");
        assert_eq!(row.field, "");
        assert_eq!(row.text, "/home");
        assert_eq!(row.kind, "lit");
    }

    #[test]
    fn ts_object_literal_const_dotted_field_paths() {
        let src = "const routes = { home: '/home', nested: { a: '/a' } };\n";
        let facts = TsTypes.extract("f.ts", src);
        let ent = facts.entities.iter().find(|e| e.name == "routes").expect("const entity");
        let by_field = |field: &str| facts.consts.iter().find(|c| c.sym == ent.sym && c.field == field);
        assert_eq!(by_field("home").expect("home row").text, "/home");
        assert_eq!(by_field("nested.a").expect("nested.a row").text, "/a");
    }

    #[test]
    fn ts_template_const_keeps_holes_and_no_entity_without_strings() {
        let src = "const greeting = `hi ${name}`;\nconst count = 3;\n";
        let facts = TsTypes.extract("f.ts", src);
        let ent = facts.entities.iter().find(|e| e.name == "greeting").expect("template const entity");
        let row = facts.consts.iter().find(|c| c.sym == ent.sym).expect("const_value row");
        assert_eq!(row.kind, "template");
        assert_eq!(row.text, "`hi ${name}`");
        // a numeric const gains neither an entity nor a const_value row.
        assert!(!facts.entities.iter().any(|e| e.name == "count"), "{:?}", facts.entities);
    }

    #[test]
    fn ts_string_enum_members_key_off_the_enum_sym() {
        let src = "enum Routes { Home = '/home', About = '/about' }\n";
        let facts = TsTypes.extract("f.ts", src);
        let enum_ent = facts.entities.iter().find(|e| e.name == "Routes").expect("enum entity");
        assert_eq!(enum_ent.kind, EntityKind::Enum);
        let home = facts.consts.iter().find(|c| c.field == "Home").expect("Home row");
        assert_eq!(home.sym, enum_ent.sym);
        assert_eq!(home.text, "/home");
        let about = facts.consts.iter().find(|c| c.field == "About").expect("About row");
        assert_eq!(about.sym, enum_ent.sym);
    }

    #[test]
    fn ts_let_var_string_init_excluded_but_as_const_included() {
        let src = "let mutablePath = '/mut';\nconst pinned = '/pin' as const;\n";
        let facts = TsTypes.extract("f.ts", src);
        assert!(!facts.entities.iter().any(|e| e.name == "mutablePath"), "{:?}", facts.entities);
        assert!(!facts.consts.iter().any(|c| c.text == "/mut"), "{:?}", facts.consts);
        assert_eq!(facts.const_mutable_skips, 1);
        let pinned = facts.entities.iter().find(|e| e.name == "pinned").expect("as-const entity");
        assert!(facts.consts.iter().any(|c| c.sym == pinned.sym && c.text == "/pin"));
    }

    #[test]
    fn ts_object_spread_property_counted_not_followed() {
        let src = "const base = { a: '/a' };\nconst merged = { ...base, b: '/b' };\n";
        let facts = TsTypes.extract("f.ts", src);
        let merged = facts.entities.iter().find(|e| e.name == "merged").expect("merged entity");
        // "b" still lands; the spread contributes no field (nothing named ".." here).
        assert!(facts.consts.iter().any(|c| c.sym == merged.sym && c.field == "b" && c.text == "/b"));
        assert_eq!(facts.const_spread_skips, 1);
    }

    #[test]
    fn ts_arrow_fn_const_unaffected_by_const_value_pass() {
        // arrow-fn consts stay Function entities (ts_var_fn_entity's job); the
        // const-value pass must not also mint a Const entity for them.
        let src = "const handler = (x: number) => x + 1;\n";
        let facts = TsTypes.extract("f.ts", src);
        let ents: Vec<&TypeEntity> = facts.entities.iter().filter(|e| e.name == "handler").collect();
        assert_eq!(ents.len(), 1, "{:?}", facts.entities);
        assert_eq!(ents[0].kind, EntityKind::Function);
        assert!(!facts.consts.iter().any(|c| c.sym == ents[0].sym));
    }

    // --- const_string_member retirement: evidence-diff gap fix ---
    // (plans/2026-07-10-string-values-const-value.md follow-up) —
    // const_string_member walked EVERY const declarator with no scope
    // restriction; const_value's original module-level-only loop missed a
    // lookup table declared inside a function body. TsNestedConstWalker
    // closes that gap; these two tests are the evidence.

    #[test]
    fn ts_const_inside_function_body_is_found_and_scoped() {
        let src = "\
function makeTable() {\n    \
    const INNER_TABLE = { x: '/inner/x' };\n    \
    return INNER_TABLE;\n\
}\n";
        let facts = TsTypes.extract("f.ts", src);
        let ent = facts.entities.iter().find(|e| e.name == "INNER_TABLE").expect("nested const entity");
        assert_eq!(ent.kind, EntityKind::Const);
        assert!(ent.sym.contains("makeTable"), "sym should carry the enclosing scope: {}", ent.sym);
        let row = facts.consts.iter().find(|c| c.sym == ent.sym).expect("const_value row");
        assert_eq!(row.field, "x");
        assert_eq!(row.text, "/inner/x");
    }

    #[test]
    fn ts_same_named_const_in_two_functions_does_not_collide() {
        let src = "\
function a() {\n    \
    const TABLE = { k: '/a' };\n    \
    return TABLE;\n\
}\n\
function b() {\n    \
    const TABLE = { k: '/b' };\n    \
    return TABLE;\n\
}\n";
        let facts = TsTypes.extract("f.ts", src);
        let ents: Vec<&TypeEntity> = facts.entities.iter().filter(|e| e.name == "TABLE").collect();
        assert_eq!(ents.len(), 2, "{:?}", facts.entities);
        assert_ne!(ents[0].sym, ents[1].sym);
        let text_for = |sym: &str| facts.consts.iter().find(|c| c.sym == sym && c.field == "k").map(|c| c.text.as_str());
        let texts: Vec<&str> = ents.iter().map(|e| text_for(&e.sym).unwrap()).collect();
        assert!(texts.contains(&"/a") && texts.contains(&"/b"), "{:?}", texts);
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

    #[test]
    fn rust_const_str_mints_entity_and_df_lit() {
        let src = "const HOME: &str = \"/home\";\nfn go() { let _ = HOME; }\n";
        let facts = RustTypes.extract("f.rs", src);
        let ent = facts.entities.iter().find(|e| e.name == "HOME").expect("const entity");
        assert_eq!(ent.kind, EntityKind::Const);
        let row = facts.consts.iter().find(|c| c.sym == ent.sym).expect("const_value row");
        assert_eq!(row.text, "/home");
        assert_eq!(row.kind, "lit");

        let df = RustTypes.extract_dataflow("f.rs", "fn go() { let x = \"/home\"; }\n");
        assert!(df.lits.iter().any(|(_, text, kind)| text == "/home" && *kind == "lit"), "{:?}", df.lits);
    }

    #[test]
    fn rust_bundle_matches_independent_extractors_and_honors_mask() {
        let src = r#"
            /// Increment a value.
            pub fn inc(input: i64) -> i64 {
                let next = input + 1;
                next
            }
        "#;
        let all = RustTypes.extract_bundle("f.rs", src, AnalysisMask::ALL);
        assert_eq!(all.types, Some(RustTypes.extract("f.rs", src)));
        assert_eq!(all.calls, Some(RustTypes.extract_calls("f.rs", src)));
        assert_eq!(all.dataflow, Some(RustTypes.extract_dataflow("f.rs", src)));

        let types_only = RustTypes.extract_bundle(
            "f.rs",
            src,
            AnalysisMask { types: true, calls: false, dataflow: false },
        );
        assert!(types_only.types.is_some());
        assert!(types_only.calls.is_none());
        assert!(types_only.dataflow.is_none());
    }
}
