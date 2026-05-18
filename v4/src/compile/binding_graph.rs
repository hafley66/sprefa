//! Compile-time binding graph analysis over `Vec<PipeAst>`.
//!
//! Two surfaces live here:
//!
//!   `analyze_program` — diag-only, lightweight predicate pass. Emits
//!     `lang/use-before-bind` and `lang/term-self-cycle`. Stable for
//!     LSP wiring.
//!
//!   `build_graph` — produces a `ProgramFlowGraph` per program with
//!     one `TermFlowGraph` per rule body. Each capture in a body
//!     carries its binding site, read sites, source, value
//!     lattice, constraint set, fanout, and crosses-scope flag. The
//!     fuser (step 5 of the rules-as-tables patch series) consumes the
//!     graph to apply five pre-computations:
//!       P1 dead-capture elimination
//!       P2 predicate pushdown
//!       P3 literal-pin folding
//!       P4 type-driven id columns
//!       P5 join-order selection
//!
//! Scope rule: a `{ block }` sub-pipe inherits the outer bound set;
//! binders inside the block are LOCAL to the block (don't leak outward).
//! Mirrors v3's `binding_graph::analyze_pipe` (see chat_log /
//! sprefa-archive bd memory `binding-graph-v3`).
//!
//! Bareword detection (CAPS / CAPS?) lives here too so the rules don't
//! diverge from `walk.rs::classify_slot`. The analyzer is a *separate*
//! pass — it does not lower; it only reads spans + names off the AST.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use effect_runtime::v2::Diag;

use crate::compile::ast::{OpCall, PipeAst};
use crate::compile::lower::op_def::default_plain_dsl_parse;
use crate::compile::lower::registry::Registry;
use crate::compile::lower::LowerCtx;

/// Top-level entry. Returns one diag vec per program (multiple pipes,
/// each scoped independently — top-level sibling pipes do not share a
/// binding scope).
pub fn analyze_program(program: &[PipeAst], reg: &Registry) -> Vec<Diag> {
    let mut diags = Vec::new();
    let rule_decls = collect_rule_decls(program);
    for pipe in program {
        let mut bound: HashSet<Arc<str>> = HashSet::new();
        let mut declared: HashMap<Arc<str>, bool> = HashMap::new();
        analyze_pipe(
            pipe,
            reg,
            &rule_decls,
            &mut bound,
            &mut declared,
            &mut diags,
        );
    }
    diags
}

#[derive(Clone, Debug)]
struct RuleDecl {
    cols: Vec<Arc<str>>,
    has_body: bool,
}

fn collect_rule_decls(program: &[PipeAst]) -> HashMap<Arc<str>, RuleDecl> {
    let mut out = HashMap::new();
    for pipe in program {
        for op in &pipe.steps {
            if op.name.as_ref() != "rule" {
                continue;
            }
            let Some(first) = op.args.first() else {
                continue;
            };
            let raw_name = first.raw.trim();
            let Some(name) = raw_name.strip_prefix(':') else {
                continue;
            };
            if !is_rule_name(name) {
                continue;
            }
            let mut cols = Vec::new();
            for arg in op.args.iter().skip(1) {
                let raw = arg.raw.trim();
                // Cols can be authored as:
                //   `:NAME`  (atom form — :A in `rule(:r, :A, :B)`)
                //   `NAME?`  (binder form — A? in `rule(:r, A?)`)
                //   `NAME`   (bare ident — also accepted)
                // Strip the leading `:` (atom prefix) and any trailing
                // `?` / `!` suffix before checking the caps-ident
                // shape so the three forms collapse to one normalized
                // column name.
                let raw = raw.strip_prefix(':').unwrap_or(raw);
                let raw = raw
                    .strip_suffix('?')
                    .or_else(|| raw.strip_suffix('!'))
                    .unwrap_or(raw);
                if is_ident(raw) {
                    cols.push(Arc::<str>::from(raw));
                }
            }
            out.insert(
                Arc::<str>::from(name),
                RuleDecl {
                    cols,
                    has_body: op.block.is_some(),
                },
            );
        }
    }
    out
}

fn analyze_pipe(
    pipe: &PipeAst,
    reg: &Registry,
    rule_decls: &HashMap<Arc<str>, RuleDecl>,
    bound: &mut HashSet<Arc<str>>,
    // name -> read since its last `?` decl in this frame. A second `?`
    // decl of a name still marked `false` (unread) is `lang/dup-decl`.
    declared: &mut HashMap<Arc<str>, bool>,
    diags: &mut Vec<Diag>,
) {
    for op in &pipe.steps {
        let (reads, mut binds, decls) = collect_term_refs(op, reg, rule_decls);
        if op.predicate && reg.get(&op.name).is_none() {
            if let Some(decl) = rule_decls.get(&op.name) {
                binds.extend(rule_query_literal_binds(op, decl));
            }
        }

        for r in &reads {
            if !bound.contains(r) {
                let msg = if bound.is_empty() {
                    format!("`{}` used before bound (no captures bound at this step)", r)
                } else {
                    let mut bn: Vec<&str> = bound.iter().map(|s| s.as_ref()).collect();
                    bn.sort();
                    format!("`{}` used before bound (in scope: {})", r, bn.join(", "))
                };
                diags.push(
                    Diag::error("lang/use-before-bind", msg).with_span(op.span.lo, op.span.hi),
                );
            }
        }

        // A read of a previously-`?`-declared name satisfies it; a
        // later re-`?` of that name is then a fresh, legal decl.
        for r in &reads {
            if let Some(seen) = declared.get_mut(r) {
                *seen = true;
            }
        }
        // Same-scope redeclare: a `?` decl of a name already declared in
        // this frame with no intervening read since => `lang/dup-decl`.
        for d in &decls {
            if let Some(false) = declared.get(d) {
                diags.push(
                    Diag::error(
                        "lang/dup-decl",
                        format!("`{}` redeclared in the same scope without an intervening read", d),
                    )
                    .with_span(op.span.lo, op.span.hi),
                );
            }
            declared.insert(d.clone(), false);
        }

        // Self-cycle: the step reads X *and* binds X without an
        // intervening binder of X earlier in the pipe. Symptom of
        // writing back into the same capture you sourced from in one
        // step — almost never what you want.
        for b in &binds {
            if reads.contains(b) && !bound.contains(b) {
                diags.push(
                    Diag::error(
                        "lang/term-self-cycle",
                        format!(
                            "`{}` is both read and bound in the same step \
                             with no prior binder",
                            b
                        ),
                    )
                    .with_span(op.span.lo, op.span.hi),
                );
            }
        }

        let bare_rule_apply = !op.predicate
            && !op.force
            && !op.apply
            && reg.get(&op.name).is_none()
            && rule_decls.contains_key(&op.name);
        if op.apply || bare_rule_apply {
            if let Some(decl) = rule_decls.get(&op.name) {
                if decl.has_body {
                    binds.extend(decl.cols.iter().cloned());
                }
            }
        }

        for b in binds {
            bound.insert(b);
        }

        // {block} — inherits outer bound set. Binds inside don't leak.
        if let Some(block) = &op.block {
            let mut local = bound.clone();
            let mut local_decls = declared.clone();
            analyze_pipe(
                block,
                reg,
                rule_decls,
                &mut local,
                &mut local_decls,
                diags,
            );
        }
    }
}

/// Per-OpCall: gather (reads, binds) over flow + args + dsl.
///
/// Args carry CAPS / CAPS? barewords (the `walk::classify_slot`
/// desugar). dsl bodies carry `${IDENT}` interp holes (per
/// `default_plain_dsl_parse`). Block sub-pipes are recursed by the
/// caller; this fn handles only the op's own slots.
fn collect_term_refs(
    op: &OpCall,
    reg: &Registry,
    rule_decls: &HashMap<Arc<str>, RuleDecl>,
) -> (Vec<Arc<str>>, Vec<Arc<str>>, Vec<Arc<str>>) {
    let mut reads: Vec<Arc<str>> = Vec::new();
    let mut binds: Vec<Arc<str>> = Vec::new();
    // `?`-form introducers only (slot `X?` / lone predicate step `X?`).
    // Drives the same-scope redeclare check; a redeclare with no
    // intervening read of the name is `lang/dup-decl`.
    let mut decls: Vec<Arc<str>> = Vec::new();

    // Lone bare word as a step = term ref (read, or bind in predicate
    // position) ONLY when it names neither a registered op nor a declared
    // rule. Mirrors `walk_op`'s positional op/rule-vs-term decision; the
    // old ALL_CAPS heuristic is gone (lint convention only).
    if op.flow.is_none()
        && op.args.is_empty()
        && op.dsl.is_none()
        && op.block.is_none()
        && !op.force
        && !op.apply
        && is_ident(op.name.as_ref())
        && reg.get(&op.name).is_none()
        && !rule_decls.contains_key(&op.name)
    {
        if op.predicate {
            binds.push(op.name.clone());
            decls.push(op.name.clone());
        } else {
            reads.push(op.name.clone());
        }
    }

    // A `?` slot only counts as a fresh introducer for the redeclare
    // check when the op is a plain step. In a predicate / apply / rule
    // query (`hits?(FS?, LO?) > hits?(FS?, OTHER_LO?)`) a repeated `?`
    // arg is a relational join binder, not a redundant redeclare.
    let track_decls = !op.predicate && !op.apply && !rule_decls.contains_key(&op.name);
    let mut slot_decls: Vec<Arc<str>> = Vec::new();
    if let Some(flow) = &op.flow {
        slot_terms(flow.raw.as_ref(), &mut reads, &mut binds, &mut slot_decls);
    }
    for arg in &op.args {
        slot_terms(arg.raw.as_ref(), &mut reads, &mut binds, &mut slot_decls);
    }
    if track_decls {
        decls.extend(slot_decls);
    }
    if op.name.as_ref() == "rev" {
        for arg in &op.args {
            if let Some(body) = static_glob_arg_body(arg.raw.as_ref()) {
                for interp in default_plain_dsl_parse(body) {
                    use crate::compile::lower::op_def::{InterpKind, InterpMode};
                    if let InterpKind::Term { mode, .. } = interp.kind {
                        match mode {
                            InterpMode::Read => reads.push(interp.name),
                            InterpMode::Bind => binds.push(interp.name),
                        }
                    }
                }
            }
        }
    }
    if op.name.as_ref() == "term" || op.name.as_ref() == "term_bind" {
        if let Some(arg) = op.args.first() {
            let raw = arg.raw.trim();
            let name = raw.strip_prefix(':').unwrap_or(raw);
            if is_ident(name) {
                if op.name.as_ref() == "term_bind" {
                    binds.push(Arc::<str>::from(name));
                } else {
                    reads.push(Arc::<str>::from(name));
                }
            }
        }
    }
    if let Some(dsl) = &op.dsl {
        // Host pipe-hole scanner: ${X} = read, ${X?} = bind. SubPipe-
        // shape interps (`${ <pipe> }`, task #10) are opaque to the
        // binding analyzer for now — the inner pipe is self-contained
        // and walk_op runs binding_graph recursively on it via its own
        // walk_pipe entry.
        use crate::compile::lower::op_def::{InterpKind, InterpMode};
        for interp in default_plain_dsl_parse(dsl.raw.as_ref()) {
            if interp.name.as_ref() == "&" {
                continue;
            }
            match interp.kind {
                InterpKind::Term { mode, .. } => match mode {
                    InterpMode::Read => reads.push(interp.name),
                    InterpMode::Bind => binds.push(interp.name),
                },
                InterpKind::SubPipe { .. } => {}
            }
        }
        // Op-declared DSL binders — `re`'s `(?P<NAME>)`, `glob`'s
        // `<NAME>`, `ast`'s `$NAME` / `$$$REST`. The op tells us what
        // its own grammar binds at runtime, so the analyzer doesn't
        // false-positive on captures introduced by sub-grammar syntax.
        if let Some(def) = reg.get(&op.name) {
            for b in def.binders_in_dsl(dsl.raw.as_ref()) {
                binds.push(b.name);
            }
        }
    }
    // Op-declared imperative cursor binds (fs sets FS, ast sets LO/HI, etc.)
    if let Some(def) = reg.get(&op.name) {
        for name in def.cursor_binds() {
            binds.push(Arc::<str>::from(*name));
        }
    }
    // If a name appears in both reads and binds for the SAME step (e.g.
    // a json body where `${UID}` is a binder and the host-style interp
    // scanner also sees it as a read), the bind wins — the op's grammar
    // is authoritative for what its body does with the name. Without
    // this, every capture in a non-template dsl false-positives as
    // term-self-cycle.
    reads.retain(|r| !binds.iter().any(|b| b == r));
    (reads, binds, decls)
}

fn static_glob_arg_body(raw: &str) -> Option<&str> {
    let raw = raw.trim();
    let body = raw.strip_prefix("glob`")?;
    body.strip_suffix('`')
}

/// Inspect a slot's raw text for a single CAPS / CAPS? bareword. Mirrors
/// `walk::classify_slot`'s ALL_CAPS arm so the two stay in lockstep.
/// Anything more complex than a single bareword falls through silently
/// — those slots don't contribute to the binding graph at this layer.
fn slot_terms(
    raw: &str,
    reads: &mut Vec<Arc<str>>,
    binds: &mut Vec<Arc<str>>,
    decls: &mut Vec<Arc<str>>,
) {
    let raw = raw.trim();
    if raw.is_empty() {
        return;
    }
    let raw = split_keyword_value(raw).unwrap_or(raw).trim();
    // Mirrors `walk::classify_slot`: `name?` = bind/decl, `name!` =
    // explicit read, bare `name` = read. ANY case (ALL_CAPS is lint
    // convention only). The only atom form is `:sym`, which fails
    // `is_ident` here and contributes nothing to the binding graph.
    if let Some(stripped) = raw.strip_suffix('?') {
        if is_ident(stripped) {
            binds.push(Arc::<str>::from(stripped));
            decls.push(Arc::<str>::from(stripped));
            return;
        }
    }
    if let Some(stripped) = raw.strip_suffix('!') {
        if is_ident(stripped) {
            reads.push(Arc::<str>::from(stripped));
            return;
        }
    }
    if is_ident(raw) {
        reads.push(Arc::<str>::from(raw));
    }
}

fn split_keyword_value(raw: &str) -> Option<&str> {
    let (key, value) = raw.split_once(':')?;
    if is_ident(key.trim()) {
        Some(value)
    } else {
        None
    }
}

fn rule_query_literal_binds(op: &OpCall, decl: &RuleDecl) -> Vec<Arc<str>> {
    let mut out = Vec::new();
    let mut positional_idx = 0usize;
    let mut saw_kw = false;
    for arg in &op.args {
        let raw = arg.raw.trim();
        let (col, value) = match split_keyword_pair(raw) {
            Some((key, value)) => {
                saw_kw = true;
                (key, value)
            }
            None => {
                if saw_kw {
                    continue;
                }
                let Some(col) = decl.cols.get(positional_idx).map(|s| s.as_ref()) else {
                    continue;
                };
                positional_idx += 1;
                (col, raw)
            }
        };
        let value = value.trim();
        if is_literal_slot(value) && decl.cols.iter().any(|c| c.as_ref() == col) {
            out.push(Arc::<str>::from(col));
        }
    }
    out
}

fn split_keyword_pair(raw: &str) -> Option<(&str, &str)> {
    let (key, value) = raw.split_once(':')?;
    let key = key.trim();
    if is_ident(key) {
        Some((key, value))
    } else {
        None
    }
}

fn is_literal_slot(raw: &str) -> bool {
    if raw == "&.value" {
        return false;
    }
    if raw.starts_with(':') {
        return true;
    }
    if raw.len() >= 2 {
        let bytes = raw.as_bytes();
        let q = bytes[0];
        if (q == b'`' || q == b'"' || q == b'\'') && bytes[bytes.len() - 1] == q {
            return true;
        }
    }
    // A bare ident is a term ref under the flip, never a literal.
    false
}

fn is_ident(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    if !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_') {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

fn is_rule_name(s: &str) -> bool {
    let mut parts = s.split('.');
    let Some(first) = parts.next() else {
        return false;
    };
    if !is_ident(first) {
        return false;
    }
    parts.all(|part| !part.is_empty() && is_ident(part))
}


// ─────────────────────────────────────────────────────────────────────
// TermFlowGraph — rule-body-scoped capture flow graph.
//
// Consumed by the fuser (step 5) to drive the five pre-computations
// (dead-cap elim, predicate pushdown, literal pin, type-driven storage,
// join-order selection). Build is per-program, scoped per-rule-body.
// `analyze_program` keeps its own (lighter) implementation so LSP-side
// diag passes don't have to allocate the full graph.
// ─────────────────────────────────────────────────────────────────────

/// Where in the binding graph a capture lives. `0` is the rule body's
/// top-level scope; nested blocks get successive ids.
pub type ScopeId = usize;
/// Index of an op in the body's step sequence.
pub type OpIdx = usize;

/// Lattice tightening from raw bytes through string interning up to
/// AST-rooted ids. Refinements run in the order
/// `Bytes ⊑ String ⊑ Tokens ⊑ Tree`, with the id-shaped refinements
/// (`*_Id`) as parallel branches used by the fuser to pick id-typed
/// SQL columns (P4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeLattice {
    Bytes,
    String,
    Tokens,
    Tree,
    /// AST-source-byte-range id; produced by ast metavar binders.
    WhereBytesId,
    /// Filesystem-entry id; produced by fs.
    FileId,
    /// Path-string id (path interned, no fs entry necessarily).
    PathId,
    /// Generic string-intern id.
    StringId,
    /// Opaque blob id.
    BlobId,
    /// Raw integer (e.g. byte offsets like LO/HI). Not a FK into an
    /// intern table.
    Int,
}

/// How a capture is sourced.
///
/// Op indices are intentionally NOT carried inside the variant — the
/// canonical "where was this bound" answer lives on
/// `CaptureInfo.binding_site`. The variants only carry information
/// that's intrinsic to the source (group name, ast metavar name, rule
/// + col for projections). This keeps pattern matches on the fuser
/// side concise.
#[derive(Clone, Debug)]
pub enum CaptureSource {
    /// `(?P<NAME>…)` in a re body.
    RegexGroup { group: Arc<str> },
    /// `$NAME` / `$$$REST` in an ast pattern.
    AstMetavar { name: Arc<str> },
    /// Cursor bind by an op's imperative binders (fs sets FS, glob sets
    /// captures, etc.).
    CursorBind { name: Arc<str> },
    /// Projection out of a rule query: `r(X?)` brings col `X` of `r`
    /// into the body scope.
    RuleProj { rule: Arc<str>, col: Arc<str> },
    /// Rule-declaration parameter at the head of the rule body's scope.
    RuleParam { col: Arc<str> },
    /// Compile-time literal in scope (rare; mostly from str body).
    Literal,
}

/// What use-context the capture is read in. Drives the fuser's P2
/// predicate-pushdown decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadCtx {
    /// `where ${X} …` — pure predicate slot.
    Predicate,
    /// Inside an ast/re/glob/json body as an interpolation hole. May
    /// drive further pattern matching.
    DslInterp,
    /// Plain cursor read in an op arg.
    Argument,
    /// Read inside another rule's call/query.
    RuleCall,
    /// Anywhere else.
    Other,
}

/// Compile-time constraint attached to a capture.
#[derive(Clone, Debug)]
pub enum ConstraintKind {
    /// Equal to another capture (records the canonical name).
    Eq(Arc<str>),
    /// Pinned to a literal — `r("/etc/passwd", X?)` pins col 0.
    LiteralPin(Arc<str>),
    /// Capture is provably non-null.
    NullImpossible,
}

#[derive(Clone, Debug)]
pub struct CaptureConstraint {
    pub kind: ConstraintKind,
}

/// Capture fan-out across read sites. Drives P1 dead-cap elim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fanout {
    /// Bound but never read; the projection can drop it.
    Dead,
    /// Read exactly once.
    Once,
    /// Read more than once.
    Many,
}

/// Per-capture record inside a `TermFlowGraph`.
#[derive(Clone, Debug)]
pub struct CaptureInfo {
    pub name: Arc<str>,
    pub binding_site: (ScopeId, OpIdx),
    pub read_sites: Vec<(ScopeId, OpIdx, ReadCtx)>,
    pub source: CaptureSource,
    pub value_lattice: TypeLattice,
    pub constraints: Vec<CaptureConstraint>,
    pub fanout: Fanout,
    pub crosses_scope: bool,
}

/// One per scope inside a body. Today only one scope (the body) is
/// produced; nested `{ ... }` blocks would add more.
#[derive(Clone, Debug)]
pub struct ScopeInfo {
    pub id: ScopeId,
    pub parent: Option<ScopeId>,
}

/// One op step in the body.
#[derive(Clone, Debug)]
pub struct StepInfo {
    pub idx: OpIdx,
    pub name: Arc<str>,
    pub is_rule_query: bool,
    /// For rule-call steps, the rule name being queried/applied.
    pub rule_target: Option<Arc<str>>,
}

/// Per-call literal/equal-pin constraint at a rule call site. The
/// fuser uses these to fold literal-arg pins into SQL int comparisons
/// (P3).
#[derive(Clone, Debug)]
pub struct CallConstraint {
    pub call_idx: OpIdx,
    pub col: Arc<str>,
    pub kind: ConstraintKind,
}

/// Tiny union-find over `Arc<str>` capture names. Same name in the
/// same body shares one root regardless of which op introduced it.
#[derive(Clone, Debug, Default)]
pub struct UnionFind<T: Eq + std::hash::Hash + Clone> {
    parents: HashMap<T, T>,
}

impl UnionFind<Arc<str>> {
    pub fn insert(&mut self, x: Arc<str>) {
        if !self.parents.contains_key(&x) {
            self.parents.insert(x.clone(), x);
        }
    }
    pub fn union(&mut self, a: Arc<str>, b: Arc<str>) {
        self.insert(a.clone());
        self.insert(b.clone());
        let ra = self.find_owned(a);
        let rb = self.find_owned(b);
        if ra != rb {
            self.parents.insert(ra, rb);
        }
    }
    /// Find the canonical root for `name`. Returns `name` itself if no
    /// entry has been registered (treat unknown names as singletons).
    pub fn find(&self, name: &str) -> Arc<str> {
        let key: Arc<str> = Arc::<str>::from(name);
        self.find_owned(key)
    }
    fn find_owned(&self, name: Arc<str>) -> Arc<str> {
        let mut cur = name;
        loop {
            match self.parents.get(&cur) {
                Some(p) if p.as_ref() != cur.as_ref() => {
                    cur = p.clone();
                }
                Some(_) => return cur,
                None => return cur,
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct TermFlowGraph {
    /// Rule name (sink table) this graph describes.
    pub rule: Arc<str>,
    /// Declared columns on the rule, in declared order.
    pub cols: Vec<Arc<str>>,
    /// Per-capture records, keyed by capture name.
    pub captures: HashMap<Arc<str>, CaptureInfo>,
    /// Equivalence classes (shared join keys).
    pub eq_classes: UnionFind<Arc<str>>,
    /// Scope tree.
    pub scopes: Vec<ScopeInfo>,
    /// Step list in source order.
    pub steps: Vec<StepInfo>,
    /// Constraints at rule call sites (literal pins, named-arg eq).
    pub call_constraints: Vec<CallConstraint>,
}

#[derive(Clone, Debug, Default)]
pub struct ProgramFlowGraph {
    pub bodies: Vec<TermFlowGraph>,
    /// Index by rule name → position in `bodies`.
    by_rule: HashMap<Arc<str>, usize>,
}

impl ProgramFlowGraph {
    pub fn rule_body_id(&self, name: &str) -> Option<usize> {
        self.by_rule.get(&Arc::<str>::from(name)).copied()
    }
}

/// Build a `ProgramFlowGraph` for the program. Diagnostics from the
/// lightweight pass (`analyze_program`) are returned alongside so
/// callers can surface both in one shot.
pub fn build_graph(
    program: &[PipeAst],
    reg: &Registry,
    _ctx: &mut LowerCtx,
) -> (ProgramFlowGraph, Vec<Diag>) {
    let mut graph = ProgramFlowGraph::default();
    let diags = analyze_program(program, reg);
    let rule_decls = collect_rule_decls(program);

    for pipe in program {
        let Some(head) = pipe.steps.first() else {
            continue;
        };
        if head.name.as_ref() != "rule" {
            continue;
        }
        let Some(name) = rule_decl_name(head) else {
            continue;
        };
        let Some(block) = head.block.as_ref() else {
            // Decl-only rule: still record cols so callers can resolve
            // rule-arg positions even when the body is missing.
            let decl = rule_decls.get(&name).cloned().unwrap_or(RuleDecl {
                cols: Vec::new(),
                has_body: false,
            });
            let body = TermFlowGraph {
                rule: name.clone(),
                cols: decl.cols.clone(),
                captures: HashMap::new(),
                eq_classes: UnionFind::default(),
                scopes: vec![ScopeInfo {
                    id: 0,
                    parent: None,
                }],
                steps: Vec::new(),
                call_constraints: Vec::new(),
            };
            let idx = graph.bodies.len();
            graph.bodies.push(body);
            graph.by_rule.insert(name, idx);
            continue;
        };
        let decl = rule_decls.get(&name).cloned().unwrap_or(RuleDecl {
            cols: Vec::new(),
            has_body: true,
        });
        let body = build_body_graph(name.clone(), &decl, block, &rule_decls);
        let idx = graph.bodies.len();
        graph.bodies.push(body);
        graph.by_rule.insert(name, idx);
    }

    (graph, diags)
}

fn rule_decl_name(op: &OpCall) -> Option<Arc<str>> {
    let raw = op.args.first()?.raw.trim();
    raw.strip_prefix(':')
        .filter(|s| is_rule_name(s))
        .map(Arc::<str>::from)
}

fn build_body_graph(
    rule: Arc<str>,
    decl: &RuleDecl,
    body: &PipeAst,
    rule_decls: &HashMap<Arc<str>, RuleDecl>,
) -> TermFlowGraph {
    let scope_id: ScopeId = 0;
    let mut captures: HashMap<Arc<str>, CaptureInfo> = HashMap::new();
    let mut eq_classes: UnionFind<Arc<str>> = UnionFind::default();
    let mut steps: Vec<StepInfo> = Vec::new();
    let mut call_constraints: Vec<CallConstraint> = Vec::new();

    // Rule-declaration parameters seed the body's scope. Their binding
    // site is the rule-decl head (synthesized op_idx = usize::MAX so
    // it doesn't collide with body op indices); fanout starts at Dead
    // and lifts as we observe reads.
    for col in &decl.cols {
        eq_classes.insert(col.clone());
        captures.insert(
            col.clone(),
            CaptureInfo {
                name: col.clone(),
                binding_site: (scope_id, usize::MAX),
                read_sites: Vec::new(),
                source: CaptureSource::RuleParam { col: col.clone() },
                value_lattice: TypeLattice::String,
                constraints: Vec::new(),
                fanout: Fanout::Dead,
                crosses_scope: false,
            },
        );
    }

    for (op_idx, op) in body.steps.iter().enumerate() {
        let is_rule_call = rule_decls.contains_key(&op.name);
        let step_info = StepInfo {
            idx: op_idx,
            name: op.name.clone(),
            is_rule_query: is_rule_query_call(op, rule_decls),
            rule_target: is_rule_call.then(|| op.name.clone()),
        };
        steps.push(step_info);

        if is_rule_call {
            // Rule call gets its own arg interpretation so RuleProj
            // source wins over CursorBind on the same hole.
            if let Some(rdecl) = rule_decls.get(&op.name) {
                record_rule_call(
                    op,
                    op_idx,
                    scope_id,
                    rdecl,
                    &mut captures,
                    &mut eq_classes,
                    &mut call_constraints,
                );
            }
            // DSL body reads still apply (rule calls don't carry one
            // today, but be defensive). Rule-call dsl is not predicate
            // context, so use Argument.
            record_dsl_reads(
                op,
                op_idx,
                scope_id,
                &mut captures,
                &mut eq_classes,
                ReadCtx::Argument,
            );
            continue;
        }

        // 1) Binders from the op's grammar (ast metavars, re groups,
        //    glob captures) and from `${X?}` host-position binders.
        record_binders(op, op_idx, scope_id, &mut captures, &mut eq_classes);

        // 2) Reads from `${X}` in dsl bodies and CAPS / `${X}` host-
        //    position reads. Includes a free-form `${X}` scan inside
        //    where-op arg text (predicate slot).
        record_reads(op, op_idx, scope_id, &mut captures, &mut eq_classes);
    }

    // Fanout pass — fill in based on collected read sites. A capture
    // that appears in the rule's declared cols is treated as read once
    // even with no in-body read site: the implicit FactWrite projection
    // at the body's tail is the read. Without this, `rule(:hits, FS?,
    // LO?) { … }` would mark FS/LO Dead and the fuser would drop them
    // from the fact-table schema.
    let in_cols = |n: &Arc<str>| decl.cols.iter().any(|c| c.as_ref() == n.as_ref());
    for (name, cap) in captures.iter_mut() {
        let reads = cap.read_sites.len();
        cap.fanout = if reads == 0 && in_cols(name) {
            Fanout::Once
        } else if reads == 0 {
            Fanout::Dead
        } else if reads == 1 {
            Fanout::Once
        } else {
            Fanout::Many
        };
    }

    TermFlowGraph {
        rule,
        cols: decl.cols.clone(),
        captures,
        eq_classes,
        scopes: vec![ScopeInfo {
            id: scope_id,
            parent: None,
        }],
        steps,
        call_constraints,
    }
}

fn is_rule_query_call(op: &OpCall, rule_decls: &HashMap<Arc<str>, RuleDecl>) -> bool {
    if !rule_decls.contains_key(&op.name) {
        return false;
    }
    !op.apply
}

fn record_binders(
    op: &OpCall,
    op_idx: OpIdx,
    scope_id: ScopeId,
    captures: &mut HashMap<Arc<str>, CaptureInfo>,
    eq_classes: &mut UnionFind<Arc<str>>,
) {
    // Op-declared imperative cursor binds (fs sets FS, etc.).
    let cursor_binds: Vec<Arc<str>> = op_cursor_binds(&op.name);
    for name in cursor_binds {
        let prov = CaptureSource::CursorBind { name: name.clone() };
        let lattice = cursor_bind_lattice(&op.name, &name);
        upsert_binder(captures, eq_classes, name, op_idx, scope_id, prov, lattice);
    }

    // DSL-body binders (ast metavar, re group, glob capture, host-
    // pipe-hole `${X?}`).
    if let Some(dsl) = &op.dsl {
        let interps = default_plain_dsl_parse(dsl.raw.as_ref());
        for interp in interps {
            if interp.name.as_ref() == "&" {
                continue;
            }
            use crate::compile::lower::op_def::{InterpKind, InterpMode};
            if let InterpKind::Term {
                mode: InterpMode::Bind,
                ..
            } = interp.kind
            {
                let name = interp.name.clone();
                let prov = ast_or_dsl_source(&op.name, &name);
                let lattice = ast_or_dsl_lattice(&op.name);
                upsert_binder(captures, eq_classes, name, op_idx, scope_id, prov, lattice);
            }
        }
    }

    // Host-position arg `${X?}` / `X?` / `${X}` (the read variants
    // are handled by record_reads).
    for slot in &op.args {
        let raw = slot.raw.trim();
        let raw = strip_keyword_value(raw).unwrap_or(raw).trim();
        let name = host_binder_name(raw);
        if let Some(name) = name {
            let prov = CaptureSource::CursorBind { name: name.clone() };
            upsert_binder(
                captures,
                eq_classes,
                name,
                op_idx,
                scope_id,
                prov,
                TypeLattice::String,
            );
        }
    }
}

fn record_reads(
    op: &OpCall,
    op_idx: OpIdx,
    scope_id: ScopeId,
    captures: &mut HashMap<Arc<str>, CaptureInfo>,
    eq_classes: &mut UnionFind<Arc<str>>,
) {
    let ctx = if op.name.as_ref() == "where" {
        ReadCtx::Predicate
    } else if op.dsl.is_some() {
        ReadCtx::DslInterp
    } else {
        ReadCtx::Argument
    };

    // Args: each slot's CAPS read or `${X}` host read.
    for slot in &op.args {
        let raw = slot.raw.trim();
        let raw_kv = strip_keyword_value(raw).unwrap_or(raw).trim();
        if let Some(name) = host_reader_name(raw_kv) {
            register_read(
                captures,
                eq_classes,
                name,
                op_idx,
                scope_id,
                ctx.clone(),
            );
            continue;
        }
        // Free-form scan: `where ${X} != ${Y}` and similar predicate
        // slots embed `${X}` inside larger expressions. Pick them up so
        // the graph records the predicate's reads.
        for name in scan_dollar_idents(raw) {
            register_read(
                captures,
                eq_classes,
                name,
                op_idx,
                scope_id,
                ctx.clone(),
            );
        }
    }

    record_dsl_reads(op, op_idx, scope_id, captures, eq_classes, ctx);
}

fn record_dsl_reads(
    op: &OpCall,
    op_idx: OpIdx,
    scope_id: ScopeId,
    captures: &mut HashMap<Arc<str>, CaptureInfo>,
    eq_classes: &mut UnionFind<Arc<str>>,
    ctx: ReadCtx,
) {
    if let Some(dsl) = &op.dsl {
        let interps = default_plain_dsl_parse(dsl.raw.as_ref());
        for interp in interps {
            if interp.name.as_ref() == "&" {
                continue;
            }
            use crate::compile::lower::op_def::{InterpKind, InterpMode};
            if let InterpKind::Term {
                mode: InterpMode::Read,
                ..
            } = interp.kind
            {
                register_read(
                    captures,
                    eq_classes,
                    interp.name.clone(),
                    op_idx,
                    scope_id,
                    ctx.clone(),
                );
            }
        }
    }
}

/// Scan raw text for `${IDENT}` reads. Used by predicate slots where
/// the read appears inside a larger expression (`where ${A} != ${B}`).
fn scan_dollar_idents(raw: &str) -> Vec<Arc<str>> {
    let bytes = raw.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        if bytes[i] == b'$' && bytes[i + 1] == b'{' {
            let start = i + 2;
            let mut j = start;
            while j < bytes.len() && bytes[j] != b'}' {
                j += 1;
            }
            if j < bytes.len() {
                let name = &raw[start..j];
                let name = name.trim_end_matches('?');
                if is_ident(name) {
                    out.push(Arc::<str>::from(name));
                }
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn record_rule_call(
    op: &OpCall,
    op_idx: OpIdx,
    scope_id: ScopeId,
    decl: &RuleDecl,
    captures: &mut HashMap<Arc<str>, CaptureInfo>,
    eq_classes: &mut UnionFind<Arc<str>>,
    call_constraints: &mut Vec<CallConstraint>,
) {
    let mut positional_idx = 0usize;
    let mut saw_kw = false;
    for arg in &op.args {
        let raw = arg.raw.trim();
        let (col_name, value) = match strip_keyword_pair(raw) {
            Some((key, value)) => {
                saw_kw = true;
                let Some(col) = decl.cols.iter().find(|c| c.as_ref() == key) else {
                    continue;
                };
                (col.clone(), value.trim())
            }
            None => {
                if saw_kw {
                    continue;
                }
                let Some(col) = decl.cols.get(positional_idx).cloned() else {
                    continue;
                };
                positional_idx += 1;
                (col, raw)
            }
        };

        // Hole `X?` / `${X?}` → projection from rule.
        if let Some(name) = host_binder_name(value) {
            let prov = CaptureSource::RuleProj {
                rule: op.name.clone(),
                col: col_name.clone(),
            };
            let lattice = TypeLattice::String;
            upsert_binder(
                captures,
                eq_classes,
                name.clone(),
                op_idx,
                scope_id,
                prov,
                lattice,
            );
            // Insert the capture into the union-find so siblings using
            // the same name across steps collapse to one root. We do
            // NOT union with the queried col_name — that would force
            // `hits(FS?, LO?)` and `hits(FS?, OTHER_LO?)` to alias LO
            // and OTHER_LO via the shared col.
            eq_classes.insert(name);
            continue;
        }

        // Grounded read `X` / `${X}` — capture reads through to the
        // column. Same eq-class rule as the binder branch: register the
        // name, don't merge it with the col.
        if let Some(name) = host_reader_name(value) {
            register_read(
                captures,
                eq_classes,
                name.clone(),
                op_idx,
                scope_id,
                ReadCtx::RuleCall,
            );
            eq_classes.insert(name);
            continue;
        }

        // Literal arg → call-site constraint.
        if is_literal_slot(value) {
            let lit = strip_literal_quotes(value);
            call_constraints.push(CallConstraint {
                call_idx: op_idx,
                col: col_name.clone(),
                kind: ConstraintKind::LiteralPin(Arc::<str>::from(lit)),
            });
        }
    }
}

fn upsert_binder(
    captures: &mut HashMap<Arc<str>, CaptureInfo>,
    eq_classes: &mut UnionFind<Arc<str>>,
    name: Arc<str>,
    op_idx: OpIdx,
    scope_id: ScopeId,
    prov: CaptureSource,
    lattice: TypeLattice,
) {
    eq_classes.insert(name.clone());
    let entry = captures.entry(name.clone()).or_insert_with(|| CaptureInfo {
        name: name.clone(),
        binding_site: (scope_id, op_idx),
        read_sites: Vec::new(),
        source: prov.clone(),
        value_lattice: lattice.clone(),
        constraints: Vec::new(),
        fanout: Fanout::Dead,
        crosses_scope: false,
    });
    // Earliest binding wins for binding_site, but lattice tightens to
    // the most specific id-shape seen across binders.
    let tightened = tighten_lattice(&entry.value_lattice, &lattice);
    entry.value_lattice = tightened;
    if matches!(entry.source, CaptureSource::RuleParam { .. }) {
        // A real binder upgrades a RuleParam placeholder.
        entry.source = prov;
        entry.binding_site = (scope_id, op_idx);
    }
}

fn register_read(
    captures: &mut HashMap<Arc<str>, CaptureInfo>,
    eq_classes: &mut UnionFind<Arc<str>>,
    name: Arc<str>,
    op_idx: OpIdx,
    scope_id: ScopeId,
    ctx: ReadCtx,
) {
    eq_classes.insert(name.clone());
    let entry = captures.entry(name.clone()).or_insert_with(|| CaptureInfo {
        name: name.clone(),
        binding_site: (scope_id, usize::MAX),
        read_sites: Vec::new(),
        source: CaptureSource::Literal,
        value_lattice: TypeLattice::String,
        constraints: Vec::new(),
        fanout: Fanout::Dead,
        crosses_scope: false,
    });
    entry.read_sites.push((scope_id, op_idx, ctx));
}

/// `Bytes ⊑ String ⊑ Tokens ⊑ Tree`. Id types win over their string-y
/// parents.
fn tighten_lattice(a: &TypeLattice, b: &TypeLattice) -> TypeLattice {
    use TypeLattice::*;
    fn rank(t: &TypeLattice) -> u8 {
        match t {
            Bytes => 0,
            String => 1,
            StringId | PathId | FileId | BlobId | Int => 2,
            Tokens | WhereBytesId => 3,
            Tree => 4,
        }
    }
    if rank(b) > rank(a) {
        b.clone()
    } else {
        a.clone()
    }
}

fn host_binder_name(raw: &str) -> Option<Arc<str>> {
    let inner = if let Some(r) = raw.strip_prefix("${").and_then(|r| r.strip_suffix('}')) {
        r
    } else {
        raw
    };
    let stripped = inner.strip_suffix('?')?;
    if !is_ident(stripped) {
        return None;
    }
    Some(Arc::<str>::from(stripped))
}

fn host_reader_name(raw: &str) -> Option<Arc<str>> {
    let inner = if let Some(r) = raw.strip_prefix("${").and_then(|r| r.strip_suffix('}')) {
        r
    } else {
        raw
    };
    if inner.ends_with('?') {
        return None;
    }
    if !is_ident(inner) {
        return None;
    }
    Some(Arc::<str>::from(inner))
}

fn strip_keyword_value(raw: &str) -> Option<&str> {
    let (key, value) = raw.split_once(':')?;
    if is_ident(key.trim()) {
        Some(value)
    } else {
        None
    }
}

fn strip_keyword_pair(raw: &str) -> Option<(&str, &str)> {
    let (key, value) = raw.split_once(':')?;
    let key = key.trim();
    if is_ident(key) {
        Some((key, value))
    } else {
        None
    }
}

fn strip_literal_quotes(raw: &str) -> &str {
    let raw = raw.trim();
    if let Some(rest) = raw.strip_prefix(':') {
        return rest;
    }
    if raw.len() >= 2 {
        let bytes = raw.as_bytes();
        let q = bytes[0];
        if (q == b'`' || q == b'"' || q == b'\'') && bytes[bytes.len() - 1] == q {
            return &raw[1..raw.len() - 1];
        }
    }
    raw
}

/// Op-declared cursor binds: hard-coded mirror of the registry. Kept
/// local so the graph builder doesn't depend on the full op registry
/// surface (only on names + arg shapes).
fn op_cursor_binds(name: &str) -> Vec<Arc<str>> {
    match name {
        "fs" => vec![Arc::<str>::from("FS")],
        "ast" => vec![
            Arc::<str>::from("FILE"),
            Arc::<str>::from("LO"),
            Arc::<str>::from("HI"),
        ],
        _ => Vec::new(),
    }
}

fn cursor_bind_lattice(op_name: &str, capture: &str) -> TypeLattice {
    match (op_name, capture) {
        ("fs", "FS") => TypeLattice::PathId,
        ("ast", "FILE") => TypeLattice::FileId,
        ("ast", "LO") | ("ast", "HI") => TypeLattice::Int,
        _ => TypeLattice::String,
    }
}

fn ast_or_dsl_source(op_name: &str, name: &Arc<str>) -> CaptureSource {
    match op_name {
        "ast" => CaptureSource::AstMetavar { name: name.clone() },
        "re" => CaptureSource::RegexGroup {
            group: name.clone(),
        },
        _ => CaptureSource::CursorBind { name: name.clone() },
    }
}

fn ast_or_dsl_lattice(op_name: &str) -> TypeLattice {
    match op_name {
        "ast" => TypeLattice::WhereBytesId,
        _ => TypeLattice::String,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::parse::host_parse;

    fn diag_codes(src: &str) -> Vec<String> {
        let (program, _parse_diags) = host_parse(src);
        let reg = crate::compile::lower::default_registry();
        analyze_program(&program, &reg)
            .iter()
            .map(|d| d.code.to_string())
            .collect()
    }

    #[test]
    fn read_before_bind_emits_diag() {
        // X is read inside the str template before any binder.
        let codes = diag_codes("rule(:r) { `hello ${X}` };");
        assert!(
            codes.iter().any(|c| c == "lang/use-before-bind"),
            "expected use-before-bind, got {:?}",
            codes
        );
    }

    #[test]
    fn bind_then_read_is_clean() {
        // X? at the rule arg position introduces X into outer scope;
        // the inner block's str then reads it cleanly.
        let codes = diag_codes("rule(:r, X?) { `hello ${X}` };");
        assert!(
            !codes.iter().any(|c| c == "lang/use-before-bind"),
            "unexpected use-before-bind, got {:?}",
            codes
        );
    }

    #[test]
    fn block_local_binds_dont_leak() {
        // X bound inside block should not satisfy a later top-level use.
        let codes = diag_codes("rule(:r) { X? } > rule(:s, X);");
        // The second `rule(:s, X)` reads X at top level; X was bound
        // only inside the first block, so it's out of scope.
        // (Note: top-level `> rule(:s, X)` becomes a CAPS read in the
        // arg slot.)
        assert!(
            codes.iter().any(|c| c == "lang/use-before-bind"),
            "expected use-before-bind, got {:?}",
            codes
        );
    }

    #[test]
    fn focal_value_interpolation_is_always_in_scope() {
        let codes = diag_codes("`alpha` > render.markdown`${&.value}`;");
        assert!(
            !codes.iter().any(|c| c == "lang/use-before-bind"),
            "unexpected use-before-bind for focal cursor value, got {:?}",
            codes
        );
    }

    #[test]
    fn dotted_bodied_rule_apply_binds_declared_cols() {
        let codes = diag_codes(
            "rule(:docs.derive, X?) { `x` > term_bind(:X) }; docs.derive() > render.markdown`${X}`;",
        );
        assert!(
            !codes.iter().any(|c| c == "lang/use-before-bind"),
            "unexpected use-before-bind for dotted rule apply, got {:?}",
            codes
        );
    }
}
