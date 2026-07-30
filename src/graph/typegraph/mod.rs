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
    /// An anonymous / inner callable: a closure, arrow function, function
    /// expression, Kotlin/Go lambda literal, or Python `lambda`. Carries an
    /// arrow type like `Function`/`Method`, but has no source-visible name, so
    /// its sym is coordinate-derived (`lambda_sym`, the same `::closure::<coord>`
    /// chain the dataflow lift mints — so a df<->call join is an exact match).
    Lambda,
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
            EntityKind::Lambda => "lambda",
            EntityKind::Const => "const",
            EntityKind::Module => "module",
        }
    }
    /// Functions, methods, and lambdas carry an arrow type; everything else is a
    /// data type.
    pub fn is_callable(self) -> bool {
        matches!(
            self,
            EntityKind::Function | EntityKind::Method | EntityKind::Lambda
        )
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
    text.lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .unwrap_or("")
}

/// Strip a `/** ... */` (or `/* ... */` / `/*! ... */`) block down to its
/// prose: drop the delimiters, the leading `*` and one space on each inner line,
/// and the blank leading/trailing lines. Shared by the Kotlin (KDoc) and TS
/// (JSDoc) locators, and by the `comment_node` classifier (`crate::cst`).
pub(crate) fn clean_block_comment(raw: &str) -> String {
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

/// Split a JSDoc/KDoc block into `@tag` rows: `@tag [{type}] [name] description`.
/// `@param`/`@property`/`@throws`/type-param tags carry a leading name into `arg`;
/// others put the whole body in `text`. A leading `{type}` annotation is dropped
/// (the type graph already carries types via `type_sig`).
fn parse_jsdoc_tags(text: &str) -> Vec<DocTag> {
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
            tag,
            arg,
            text: desc,
        });
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
            out.push(DocTag {
                tag: "section".into(),
                arg: name,
                text: body.join("\n").trim().to_string(),
            });
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
    pub sym: String,  // file::function::name (free) or file::method::Parent.name
    pub name: String, // bare callable name, for callee resolution (not written)
    pub kind: CallKind,
    pub file: String,
    pub line: u32,
    pub end: u32, // body span end (1-based line), for callsite containment
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallSite {
    pub caller_sym: Option<String>, // filled by the engine's span-containment pass
    pub callee: String,             // trailing segment (bare name) for resolution
    pub callee_path: Option<String>, // full qualified path when >1 segment (e.g. sprefa_v5::cli::run)
    pub file: String,
    pub line: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallKind {
    Free,
    Method,
    /// An anonymous / inner callable's def-site kind. One vocabulary word for
    /// the callable registry: `call_def.kind` reads "lambda", matching
    /// `EntityKind::Lambda`'s tag and the `@callable <lang> lambda` markers.
    /// (The dataflow `df_node.kind = "closure"` value node is a *different*
    /// relation and concept — the closure-as-value in the enclosing scope, not
    /// the callable definition — so it keeps its own word; see
    /// docs/callable-coverage.md.)
    Lambda,
}

impl CallKind {
    pub fn tag(self) -> &'static str {
        match self {
            CallKind::Free => "function",
            CallKind::Method => "method",
            CallKind::Lambda => "lambda",
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
    pub param_pos: Vec<(NodeIdx, u32)>, // (param node index, positional index) for node-level type joins
    /// (call/new node index, position, arg node index): which argument slot a value
    /// feeds. Position is 0-based and aligns with `param_pos`/`type_sig.pos`
    /// (Rust method receivers are pos -1, mirroring the skipped `self` param).
    pub args: Vec<(NodeIdx, i64, NodeIdx)>,
    /// (new/call node index, field name, value node index): named value flow into a
    /// composite — Rust struct-literal fields, TS object-literal properties,
    /// Kotlin named arguments.
    pub fields: Vec<(NodeIdx, String, NodeIdx)>,
    /// (df_node id, text, kind∈lit|template|concat): the `df_lit` relation's
    /// payload — one row per STRING-carrying value node. `lit` rows carry the
    /// cooked literal value (numbers/bools/regex are never pushed here, only
    /// `syn::Lit::Str`/oxc `StringLiteral`); `template`/`concat` rows carry the
    /// RAW source slice (`${}` holes intact for a template, the written
    /// operands for a `+` concat — a syntactic label, not a type judgment, so
    /// a numeric `+` mints a concat row too). TS/TSX/JS populate `template`/
    /// `concat`; Rust populates `lit` only (Kotlin/Go/Python ledgered).
    pub lits: Vec<(NodeIdx, String, &'static str)>,
    /// Pending (df_node id, byte_start, byte_end, kind) rows for `template`/
    /// `concat` nodes, whose text is a source SLICE the per-node lift doesn't
    /// have handy (`ts_flow_expr` only carries the line-offset table, not the
    /// raw file text). `ts_dataflow_from` drains this into `lits` once, after
    /// the walk — the one place that already holds `content` — so no
    /// recursive function between the two needs it threaded through.
    pub lit_spans: Vec<(NodeIdx, u32, u32, &'static str)>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoopFact {
    pub file: String,
    pub start: u32,         // start line of the loop header
    pub end: u32,           // close line of the loop body (span end)
    pub var: String,        // loop variable name, "" when none (while/loop)
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
    pub call_id: NodeIdx,
    pub loop_id: String, // "{file}:{start}", joins back to loop_over by (file, start)
    pub depth: u32,      // 1 = outermost enclosing loop
    pub collection: String, // the inner loop's collection text ("" until extractors fill it)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DfNode {
    /// Dense in-memory identity: this node's index in `DataflowFacts.nodes`
    /// (== `NodeIdx`). The persisted df id is the `_df_node_dict` surrogate the
    /// write seam resolves from `(file, line, col, kind)`; this index is the
    /// transient join key that ties the node to its edges/args/fields/lits and
    /// never leaves extraction. Formerly `format!("{file}:{line}:{col}:{kind}")`.
    pub id: NodeIdx,
    pub kind: String, // param | let_bind | var_read | var_write | lit | call_res | new | member | ret | borrow | binop | unop | loop | if | match | block | closure | try | expr
    pub var: String,  // variable name when the node is var-related, else ""
    pub fn_sym: String, // enclosing def sym (file::function::name), joins call_def
    pub file: String,
    pub line: u32,
    /// The coordinate column baked into `id` (`file:line:col:kind`): syn's
    /// 0-based char column for rust/kotlin/python/go (`push_node`), the 0-based
    /// byte column within the line for ts (`ts_push` via `line_col`). Stored as
    /// a real `df_node` column so the id text (no longer interned into
    /// `_strings`) reconstructs from (file, line, col, kind) at display time.
    pub col: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DfEdge {
    pub from: NodeIdx,
    pub to: NodeIdx,
}

/// Dense in-memory node identity: the node's index in `DataflowFacts.nodes`.
/// Replaces the former `format!("{file}:{line}:{col}:{kind}")` string id — the
/// identity is now a surrogate (the position), never a concatenated composite.
/// The PERSISTED df id is the content-keyed `_df_node_dict` surrogate resolved
/// at the write seam from each node's `(file, line, col, kind)` columns; this
/// index is transient per-file plumbing that ties a node to its edges / args /
/// fields / lits and never leaves extraction. An index is language-agnostic and
/// stable across the 1-based line bump (positions do not move), so the old
/// id-rebuild + reference-remap pass is gone.
pub type NodeIdx = u32;

/// sem-style symbol id: `file::kind::name`, scoped by an optional parent for
/// methods (`file::method::Class.name`). Stable, index-free, human-readable.
pub fn mint_sym(file: &str, kind: EntityKind, name: &str, parent: Option<&str>) -> String {
    match parent {
        Some(p) => format!("{file}::{}::{p}.{name}", kind.tag()),
        None => format!("{file}::{}::{name}", kind.tag()),
    }
}

/// Deterministic sym for an anonymous callable (closure / arrow / function
/// expression / lambda literal / func literal). The enclosing callable's sym,
/// then `::closure::<coord>`, where `coord` is the language's stable node
/// coordinate — `<row>_<col>` for tree-sitter front-ends (Kotlin/Go/Python),
/// the byte offset for the oxc front-end (TS/JS), `<line>_<col>` for syn (Rust).
///
/// This is the SAME string the dataflow lift mints as a closure's `lam_sym`
/// (the `closure` value node's `var`, and the lifted body's `fn_sym`), so
/// `call_def.sym == df_node.fn_sym` for the lambda body and
/// `call_def.sym == df_node.var` for the closure value node are exact joins —
/// the whole point of registering lambdas as callables. Both the df lift and
/// the call_def emitter call this so there is one source of truth for the
/// format; the `EntityKind::Lambda` tag names the *kind* column, not the sym.
pub fn lambda_sym(enclosing_fn_sym: &str, coord: &str) -> String {
    format!("{enclosing_fn_sym}::closure::{coord}")
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
    fn extract_calls(&self, _file: &str, _content: &str) -> CallFacts {
        CallFacts::default()
    }
    /// Intra-procedural dataflow lift (see `DataflowFacts`). Default empty so the
    /// lazy `DATAFLOW_RELS` wiring is live end to end with zero rows; each
    /// front-end overrides as its extractor lands.
    fn extract_dataflow(&self, _file: &str, _content: &str) -> DataflowFacts {
        DataflowFacts::default()
    }

    /// Whether `extract_bundle` actually shares one parse across projections.
    /// The engine uses this to avoid routing languages through the bundle seam
    /// when their default implementation would still parse once per family.
    fn supports_analysis_bundle(&self) -> bool {
        false
    }

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
    pub const ALL: Self = Self {
        types: true,
        calls: true,
        dataflow: true,
    };
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

// --- per-language modules (2026-07-18 decomposition-normalization split) ----
// Pure code motion out of the former single typegraph.rs (8,965 lines): each
// module owns exactly one parser crate (rust -> syn, ts -> oxc,
// kotlin/go/python -> tree-sitter + grammar crate), matching the affinity
// measured in plans/2026-07-18-decomposition-normalization.md section 2.3.
// Every `pub use` below keeps an existing `crate::graph::typegraph::X` path
// resolving unchanged; no consumer outside this module reaches per-language
// internals (section 2.2).
mod analysis;
mod go;
mod kotlin;
mod python;
mod rust;
mod ts;

pub use analysis::{type_lgg_pairs, type_shape_hashes};
pub use kotlin::kotlin_edges;
pub use rust::edges;
pub use ts::{
    ts_comments, ts_edges, ts_template_parts, ts_unresolved_refs, TemplatePart, UnresolvedRef,
};

// --- shared cross-language test helpers (used by rust/kotlin/ts/go test
// modules) -----------------------------------------------------------
#[cfg(test)]
pub(crate) fn has(got: &[TypeEdge], from: &str, to: &str, kind: &'static str) -> bool {
    got.contains(&TypeEdge {
        from: from.into(),
        to: to.into(),
        kind,
    })
}

#[cfg(test)]
pub(crate) fn dnode<'a>(df: &'a DataflowFacts, kind: &str, var: &str) -> &'a DfNode {
    df.nodes
        .iter()
        .find(|n| n.kind == kind && n.var == var)
        .unwrap_or_else(|| panic!("no node {kind}/{var}: {:?}", df.nodes))
}

#[cfg(test)]
pub(crate) fn has_arg(df: &DataflowFacts, call: &NodeIdx, pos: i64, arg: &NodeIdx) -> bool {
    df.args
        .iter()
        .any(|(c, p, a)| c == call && *p == pos && a == arg)
}

#[cfg(test)]
pub(crate) fn has_field(df: &DataflowFacts, id: &NodeIdx, field: &str, value: &NodeIdx) -> bool {
    df.fields
        .iter()
        .any(|(i, f, v)| i == id && f == field && v == value)
}

/// Shared gate for the lambda lift: the `closure` value node sits at the
/// call's expected arg slot, its `var` names the lifted scope, and that
/// scope holds a positional `param` plus a `ret` fed by the body.
#[cfg(test)]
pub(crate) fn assert_lambda_lifted(df: &DataflowFacts, lam_slot: i64, param_var: &str) {
    let clo = df
        .nodes
        .iter()
        .find(|n| n.kind == "closure")
        .expect("closure node");
    let lam_sym = clo.var.clone();
    assert!(
        lam_sym.contains("::closure::"),
        "closure var carries the lifted sym: {clo:?}"
    );
    // the closure VALUE lives in the enclosing fn, not its own scope.
    assert_ne!(clo.fn_sym, lam_sym, "{clo:?}");
    assert!(
        df.args
            .iter()
            .any(|(_, p, a)| *p == lam_slot && a == &clo.id),
        "closure at arg slot {lam_slot}: {:?}",
        df.args
    );
    let param = df
        .nodes
        .iter()
        .find(|n| n.kind == "param" && n.var == param_var && n.fn_sym == lam_sym)
        .unwrap_or_else(|| panic!("param {param_var} under {lam_sym}: {:?}", df.nodes));
    assert!(
        df.param_pos.iter().any(|(i, p)| i == &param.id && *p == 0),
        "lambda param at slot 0: {:?}",
        df.param_pos
    );
    let ret = df
        .nodes
        .iter()
        .find(|n| n.kind == "ret" && n.fn_sym == lam_sym)
        .unwrap_or_else(|| panic!("ret under {lam_sym}: {:?}", df.nodes));
    // body value reaches the ret node (param -> binop -> ret here).
    assert!(df.edges.iter().any(|e| e.to == ret.id), "{:?}", df.edges);
}

#[cfg(test)]
mod tests {
    use super::*;

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
                    assert!(
                        syms.contains(p.as_str()),
                        "[{lang}] dangling parent {p} on {}: {es:?}",
                        e.sym
                    );
                }
            }
        };
        let find = |es: &[TypeEntity], n: &str| {
            es.iter()
                .find(|e| e.name == n)
                .unwrap_or_else(|| panic!("missing {n}: {es:?}"))
                .clone()
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
        assert_eq!(
            find(&re, "sm").parent.as_deref(),
            Some("src/lib.rs::struct::S")
        );
        assert_eq!(
            find(&re, "em").parent.as_deref(),
            Some("src/lib.rs::enum::E")
        );
        assert!(
            re.iter().any(|e| e.sym == "src/lib.rs::trait::T"),
            "trait entity: {re:?}"
        );

        // TS: interface + class; a class method parents to the class sym.
        let ts = "\
export interface I { }
export class C { m(): void {} }
";
        let te = TsTypes.extract("src/m.ts", ts).entities;
        check(&te, "ts");
        assert_eq!(find(&te, "m").parent.as_deref(), Some("src/m.ts::class::C"));
        assert!(
            te.iter().any(|e| e.sym == "src/m.ts::interface::I"),
            "interface entity: {te:?}"
        );

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
        assert!(
            ke.iter().any(|e| e.sym == "src/K.kt::class::K"),
            "class entity: {ke:?}"
        );
        assert!(
            ke.iter().any(|e| e.sym == "src/K.kt::interface::Itf"),
            "interface entity: {ke:?}"
        );
    }
}

// --- shared dataflow/edge helpers (used by every language's type-edge
// and dataflow-node walk, not just Rust's) --------------------------
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

/// Mint a node and return its id. Free helper (not a closure) so the recursive
/// `flow_expr` calls can borrow `out` without holding a second `&mut` alive. The
/// id is `file:line:col:kind`: a parent expression and its first child share a
/// start position (e.g. `a + 1` starts where `a` starts), so the kind suffix
/// disambiguates them — every lifted node is a distinct (position, kind) pair.
/// The id is the interned display + join handle only; the node's full IDENTITY
/// (see `df_node`'s decl) is the whole row (id, kind, var, fn, file, line),
/// because var/fn diverge across revs at the same coordinate. The `df_node`
/// writer dedups on that full tuple, so id alone is NOT the dedup key.
fn push_node(
    out: &mut DataflowFacts,
    file: &str,
    line: u32,
    col: u32,
    kind: &str,
    var: &str,
    fn_sym: &str,
) -> NodeIdx {
    let id = out.nodes.len() as NodeIdx;
    out.nodes.push(DfNode {
        id,
        kind: kind.into(),
        var: var.into(),
        fn_sym: fn_sym.into(),
        file: file.into(),
        line,
        col,
    });
    id
}

/// Bump every node's `line` to 1-based. A tree-sitter front-end mints nodes from
/// the raw 0-based row, then bumps the line column to 1-based (the df contract).
/// Node identity is now the node's INDEX (`NodeIdx`), which does not move when a
/// column value changes, so no id rebuild and no reference remap is needed: the
/// coordinate the write seam resolves through `_df_node_dict` reads the bumped
/// `(file, line, col, kind)` columns directly. `nests` are recomputed by the
/// caller after this.
pub(crate) fn bump_node_lines_1based(out: &mut DataflowFacts) {
    for n in &mut out.nodes {
        n.line += 1;
    }
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
        if n.kind != "call_res" && n.kind != "new" {
            continue;
        }
        // A lifted lambda's sym is `<enclosing fn>::closure::<pos>` (chained for
        // nesting), so a call inside a closure inside a loop still counts: the
        // loop's fn either matches exactly or is a `::closure::` ancestor.
        let in_fn = |l: &LoopFact| {
            l.fn_sym == n.fn_sym
                || (n.fn_sym.starts_with(&l.fn_sym)
                    && n.fn_sym[l.fn_sym.len()..].starts_with("::closure::"))
        };
        let mut enclosing: Vec<&LoopFact> = loops
            .iter()
            .filter(|l| in_fn(l) && n.line >= l.start && n.line <= l.end)
            .collect();
        enclosing.sort_by_key(|l| l.start);
        for (i, l) in enclosing.iter().enumerate() {
            out.push(NestFact {
                call_id: n.id,
                loop_id: format!("{}:{}", l.file, l.start),
                depth: (i + 1) as u32,
                collection: l.collection.clone(),
            });
        }
    }
    out
}
