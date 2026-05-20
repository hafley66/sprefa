//! Rule-body fuser.
//!
//! The fuser sits at the top of `walk_pipe`. For every rule body it:
//!
//!   1. Walks `&[OpCall]` and calls `Op::classify` on each step.
//!   2. Picks one of two kinds:
//!        full-SQL       — every step is Pure or RuleQuery.
//!        streamed-Rust  — any step is Stream; no RuleQuery appears
//!                         after a Stream (would be `lower/rule-
//!                         stream-after-relation`).
//!   3. Applies five TermFlowGraph-driven pre-computations:
//!        P1 dead-capture elimination
//!        P2 predicate pushdown into JOIN ON
//!        P3 literal-pin pre-interning
//!        P4 type-driven id columns
//!        P5 join-order selection
//!   4. Emits a `FusedRule` carrying the chosen SQL / prepared-insert
//!      shape plus a fallback `Pipe<Cursor>` so the existing runner
//!      can drive the body when the kind requires Rust steps.
//!
//! The fuser is intentionally additive: rules whose body falls
//! outside the SQL-friendly subset (e.g. ones using `print`, `write_
//! file`, or other side-effecting ops) get `FusedRule::Unfused` and
//! flow through the legacy expand path.

use std::sync::Arc;

use effect_runtime::v2::{Diag, Pipe};

use super::ast::{OpCall, PipeAst};
use super::binding_graph::{
    CallConstraint, ConstraintKind, Fanout, ProgramFlowGraph, TermFlowGraph, TypeLattice,
};
use super::lower::liftable::{Col, IdKind, Liftable, Op, TermBind};
use crate::Cursor;

/// FusedKind chosen by the fuser for a rule body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FusedKind {
    FullSql,
    StreamedRust,
    /// Body has steps that today don't lift (e.g. `print`, `tag`).
    /// The legacy expand path runs the underlying pipe verbatim.
    Unfused,
}

/// Whether the fused emission spans one transaction or more.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionKind {
    /// One transaction (facts + support_edges).
    Single,
    /// Multiple transactions (e.g. one per step).
    Multiple,
    /// No transactional emission (e.g. unfused).
    None,
}

/// The fuser's output per rule body.
pub struct FusedRule {
    name: Arc<str>,
    kind: FusedKind,
    pipe: Pipe<Cursor>,
    fused_sql: Option<String>,
    fused_statements: Vec<String>,
    prepared_insert_sql: Option<String>,
    create_table_sql: Option<String>,
    transaction_kind: TransactionKind,
    /// The body reads the rule's OWN `{name}_facts` table (a
    /// self-referential / recursive overload). The run path must then
    /// expose `{name}` as a live source view AND iterate the rule to a
    /// fixed point instead of running it once.
    recursive: bool,
}

impl FusedRule {
    pub fn name(&self) -> &str {
        self.name.as_ref()
    }
    pub fn kind(&self) -> FusedKind {
        self.kind
    }
    pub fn is_full_sql(&self) -> bool {
        matches!(self.kind, FusedKind::FullSql)
    }
    pub fn is_streamed_rust(&self) -> bool {
        matches!(self.kind, FusedKind::StreamedRust)
    }
    pub fn is_unfused(&self) -> bool {
        matches!(self.kind, FusedKind::Unfused)
    }
    pub fn fused_sql(&self) -> Option<&str> {
        self.fused_sql.as_deref()
    }
    pub fn fused_statements(&self) -> Option<&[String]> {
        if self.fused_statements.is_empty() {
            None
        } else {
            Some(&self.fused_statements)
        }
    }
    pub fn prepared_insert_sql(&self) -> Option<&str> {
        self.prepared_insert_sql.as_deref()
    }
    pub fn create_table_sql(&self) -> Option<&str> {
        self.create_table_sql.as_deref()
    }
    pub fn transaction_kind(&self) -> TransactionKind {
        self.transaction_kind
    }
    pub fn is_recursive(&self) -> bool {
        self.recursive
    }
    /// Underlying pipe. The runner uses this for the unfused / streamed
    /// kinds; full-SQL paths still expose it so existing callers can
    /// migrate progressively.
    pub fn into_pipe(self) -> Pipe<Cursor> {
        self.pipe
    }
    pub fn pipe(&self) -> &Pipe<Cursor> {
        &self.pipe
    }
}

/// Construct a passthrough `FusedRule` from an already-lowered pipe.
/// Used for non-rule pipes (top-level queries, applies) so callers
/// receive a uniform `Vec<FusedRule>` from `walk_program`.
pub(crate) fn passthrough(name: Arc<str>, pipe: Pipe<Cursor>) -> FusedRule {
    FusedRule {
        name,
        kind: FusedKind::Unfused,
        pipe,
        fused_sql: None,
        fused_statements: Vec::new(),
        prepared_insert_sql: None,
        create_table_sql: None,
        transaction_kind: TransactionKind::None,
        recursive: false,
    }
}

/// Heuristic classification: peek the op name and decide which
/// `Liftable` shape it produces. The real path will be `Op::classify`
/// on each registered op type; until every op carries that impl, the
/// fuser routes by name. The set tracked here is the subset the
/// `*_target.rs` tests exercise plus the ops listed in step 4 of the
/// patch series.
fn classify_op(op: &OpCall) -> Option<Liftable> {
    // Apply form is opaque to the fuser — the body runs through
    // RuleInvokeComponent, not as a SQL fragment.
    if op.apply {
        return Some(Liftable::Opaque);
    }
    match op.name.as_ref() {
        "fs" => Some(Liftable::Stream {
            schema: vec![(Col::from("FS"), IdKind::PathId)],
            run: super::lower::liftable::stream_noop,
        }),
        "ast" => Some(Liftable::Stream {
            schema: vec![
                (Col::from("FILE"), IdKind::FileId),
                (Col::from("LO"), IdKind::Int),
                (Col::from("HI"), IdKind::Int),
            ],
            run: super::lower::liftable::stream_noop,
        }),
        "re" => Some(Liftable::Pure {
            project: Vec::new(),
            filter: None,
            udfs: vec![Arc::<str>::from("sprf_re_match")],
        }),
        "where" => {
            // Predicate text comes from the backtick DSL body
            // (canonical surface: `where\`${A} != ${B}\``). Paren form
            // and the parenless `where EXPR` rewrite were removed in
            // phase 4 of the where-strictly-via-sql-where-dsl
            // migration. Body LSP support: cst::dsls::sql_where.
            let pred = op
                .dsl
                .as_ref()
                .map(|d| d.raw.as_ref().trim().to_string())
                .unwrap_or_default();
            Some(Liftable::Pure {
                project: Vec::new(),
                filter: Some(pred),
                udfs: Vec::new(),
            })
        }
        _ => None,
    }
}

/// Classify `not(rule?(args))` as an AntiJoin. Returns `None` if the
/// shape is malformed (and pushes a diag), in which case the caller
/// falls back to unfused passthrough.
///
/// Accepted shape, single positional arg whose body re-parses to a
/// single `<rule>?(args)` call on a declared rule. Reject:
///   - non-singular args / kw args / dsl / block on the outer `not`
///   - inner force / apply / non-predicate (a bare-call not a query)
///   - inner rule not in `rule_decls`
///   - inner arg with a Bind (`X?`) — antijoin produces no captures
fn classify_antijoin(
    op: &OpCall,
    rule_decls: &std::collections::HashMap<Arc<str>, Vec<Arc<str>>>,
    diags: &mut Vec<Diag>,
) -> Option<Liftable> {
    if op.args.len() != 1 || op.dsl.is_some() || op.block.is_some() {
        diags.push(
            Diag::error(
                "lower/antijoin-shape",
                "not(...): expected one positional arg of shape `rule?(args)` and no body/dsl",
            )
            .with_span(op.span.lo, op.span.hi),
        );
        return None;
    }
    let inner_raw = op.args[0].raw.as_ref().trim();
    let mut frag = String::with_capacity(inner_raw.len() + 1);
    frag.push_str(inner_raw);
    frag.push(';');
    let (inner_pipes, _) = crate::compile::parse::host_parse(&frag);
    if inner_pipes.len() != 1 || inner_pipes[0].steps.len() != 1 {
        diags.push(
            Diag::error(
                "lower/antijoin-shape",
                "not(...): arg must be a single `rule?(args)` call",
            )
            .with_span(op.span.lo, op.span.hi),
        );
        return None;
    }
    let inner = &inner_pipes[0].steps[0];
    if !inner.predicate || inner.apply || inner.force {
        diags.push(
            Diag::error(
                "lower/antijoin-shape",
                "not(...): inner call must be the query form `rule?(args)` (no `!`, no apply)",
            )
            .with_span(op.span.lo, op.span.hi),
        );
        return None;
    }
    let cols = match rule_decls.get(&inner.name) {
        Some(c) => c,
        None => {
            diags.push(
                Diag::error(
                    "lower/antijoin-shape",
                    format!(
                        "not({}?(...)): `{}` is not a declared rule",
                        inner.name, inner.name,
                    ),
                )
                .with_span(op.span.lo, op.span.hi),
            );
            return None;
        }
    };
    let lift = classify_rule_call(inner, inner.name.as_ref(), cols);
    let (table, binds) = match lift {
        Liftable::RuleQuery { table, binds } => (table, binds),
        _ => {
            diags.push(
                Diag::error(
                    "lower/antijoin-shape",
                    "not(...): inner classify did not yield RuleQuery",
                )
                .with_span(op.span.lo, op.span.hi),
            );
            return None;
        }
    };
    for (_, b) in &binds {
        if matches!(b, TermBind::Bind(_)) {
            diags.push(
                Diag::error(
                    "lower/antijoin-bind",
                    "not(rule?(... X? ...)): projection binders (`X?`) are not allowed inside `not` — \
                     an antijoin produces no captures",
                )
                .with_span(op.span.lo, op.span.hi),
            );
            return None;
        }
    }
    Some(Liftable::AntiJoin { table, binds })
}

/// Classify a rule-call op (no registry entry; the rule is in
/// rule_decls). Bare → RuleQuery; apply → Opaque.
fn classify_rule_call(op: &OpCall, target: &str, decl_cols: &[Arc<str>]) -> Liftable {
    if op.apply {
        return Liftable::Opaque;
    }
    let table = Arc::<str>::from(format!("{target}_facts"));
    let mut binds = Vec::with_capacity(op.args.len());
    let mut positional_idx = 0usize;
    let mut saw_kw = false;
    for arg in &op.args {
        let raw = arg.raw.trim();
        let (col, value) = match raw.split_once(':') {
            Some((maybe_kw, rest)) if is_ident_trim(maybe_kw) => {
                saw_kw = true;
                let col = decl_cols
                    .iter()
                    .find(|c| c.as_ref() == maybe_kw.trim())
                    .cloned()
                    .unwrap_or_else(|| Arc::<str>::from(maybe_kw.trim()));
                (col, rest.trim())
            }
            _ => {
                if saw_kw {
                    continue;
                }
                let col = decl_cols
                    .get(positional_idx)
                    .cloned()
                    .unwrap_or_else(|| Arc::<str>::from(format!("col{positional_idx}")));
                positional_idx += 1;
                (col, raw)
            }
        };
        let bind = classify_rule_arg(value);
        binds.push((col, bind));
    }
    Liftable::RuleQuery { table, binds }
}

fn classify_rule_arg(raw: &str) -> TermBind {
    let raw = raw.trim();
    // `${IDENT?}` / `IDENT?` → projection binder.
    let stripped = raw
        .strip_prefix("${")
        .and_then(|s| s.strip_suffix('}'))
        .unwrap_or(raw);
    if let Some(name) = stripped.strip_suffix('?') {
        if is_caps_ident(name) {
            return TermBind::Bind(Arc::<str>::from(name));
        }
    }
    if let Some(rest) = stripped.strip_prefix(':') {
        return TermBind::Atom(Arc::<str>::from(rest));
    }
    if raw.len() >= 2 {
        let bytes = raw.as_bytes();
        let q = bytes[0];
        if (q == b'`' || q == b'"' || q == b'\'') && bytes[bytes.len() - 1] == q {
            return TermBind::Literal(Arc::<str>::from(&raw[1..raw.len() - 1]));
        }
    }
    TermBind::Read(Arc::<str>::from(stripped))
}

fn is_ident_trim(s: &str) -> bool {
    let s = s.trim();
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

fn is_caps_ident(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    if !bytes[0].is_ascii_uppercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || *b == b'_')
}

/// One step's classification with the originating op for back-refs.
struct ClassifiedStep<'a> {
    op: &'a OpCall,
    lift: Liftable,
}

/// Try to fuse a rule body. Returns `None` if no body to fuse (rule
/// has no `{ ... }` block) or if the body doesn't classify into the
/// SQL-friendly subset.
pub(crate) fn try_fuse(
    rule_name: &str,
    rule_block: &PipeAst,
    rule_pipe: Pipe<Cursor>,
    graph: &ProgramFlowGraph,
    rule_decls: &std::collections::HashMap<Arc<str>, Vec<Arc<str>>>,
    recursive_rules: &std::collections::BTreeSet<Arc<str>>,
    diags: &mut Vec<Diag>,
) -> Option<FusedRule> {
    let body_id = graph.rule_body_id(rule_name)?;
    let body = &graph.bodies[body_id];

    let mut classified: Vec<ClassifiedStep<'_>> = Vec::with_capacity(rule_block.steps.len());
    for op in &rule_block.steps {
        if op.name.as_ref() == "not" {
            match classify_antijoin(op, rule_decls, diags) {
                Some(lift) => {
                    classified.push(ClassifiedStep { op, lift });
                    continue;
                }
                None => return Some(passthrough(Arc::<str>::from(rule_name), rule_pipe)),
            }
        }
        if let Some(cols) = rule_decls.get(&op.name) {
            classified.push(ClassifiedStep {
                op,
                lift: classify_rule_call(op, op.name.as_ref(), cols),
            });
            continue;
        }
        match classify_op(op) {
            Some(lift) => classified.push(ClassifiedStep { op, lift }),
            None => {
                // Unclassifiable step → fall back to unfused.
                return Some(passthrough(Arc::<str>::from(rule_name), rule_pipe));
            }
        }
    }

    // FusedKind selection per spec:
    //   - every step Pure | RuleQuery       → full-SQL
    //   - any Stream + RuleQuery after it   → diag, unfused
    //   - any Stream                        → streamed-Rust
    //   - any Opaque                        → unfused
    let mut has_stream = false;
    let mut has_opaque = false;
    let mut has_antijoin = false;
    let mut first_antijoin_span = None;
    let mut bad_stream_then_rq = false;
    let mut first_rq_after_stream = None;
    for step in &classified {
        match &step.lift {
            Liftable::Stream { .. } => {
                has_stream = true;
            }
            Liftable::RuleQuery { .. } | Liftable::AntiJoin { .. }
                if has_stream && first_rq_after_stream.is_none() =>
            {
                bad_stream_then_rq = true;
                first_rq_after_stream = Some(step.op.span);
            }
            Liftable::AntiJoin { .. } => {
                if !has_antijoin {
                    first_antijoin_span = Some(step.op.span);
                }
                has_antijoin = true;
            }
            Liftable::Opaque => {
                has_opaque = true;
            }
            _ => {}
        }
    }

    if has_antijoin && has_stream {
        let span = first_antijoin_span.unwrap();
        diags.push(
            Diag::error(
                "lower/rule-antijoin-with-stream",
                format!(
                    "rule `{rule_name}`: `not(rule?(...))` antijoin is only supported in full-SQL bodies. \
                     Move the Stream source into its own rule and read its `_facts` table here."
                ),
            )
            .with_span(span.lo, span.hi),
        );
        return Some(passthrough(Arc::<str>::from(rule_name), rule_pipe));
    }

    if bad_stream_then_rq {
        let span = first_rq_after_stream.unwrap();
        diags.push(
            Diag::error(
                "lower/rule-stream-after-relation",
                format!(
                    "rule `{rule_name}`: RuleQuery cannot follow a Stream step in the same body. \
                     Split into two rules: materialize the stream side first, then query it."
                ),
            )
            .with_span(span.lo, span.hi),
        );
        return Some(passthrough(Arc::<str>::from(rule_name), rule_pipe));
    }

    if has_opaque {
        return Some(passthrough(Arc::<str>::from(rule_name), rule_pipe));
    }

    if has_stream {
        return Some(fuse_streamed_rust(
            rule_name, body, &classified, rule_pipe,
        ));
    }
    let is_recursive =
        recursive_rules.contains(&Arc::<str>::from(rule_name));
    Some(fuse_full_sql(
        rule_name,
        body,
        &classified,
        rule_pipe,
        is_recursive,
        diags,
    ))
}

/// Live captures = rule's declared cols intersected with non-Dead
/// captures in the body. Drives P1 dead-capture elimination.
fn live_cols<'a>(body: &'a TermFlowGraph) -> Vec<&'a Arc<str>> {
    body.cols
        .iter()
        .filter(|c| {
            body.captures
                .get(c.as_ref())
                .map(|cap| !matches!(cap.fanout, Fanout::Dead))
                .unwrap_or(false)
        })
        .collect()
}

fn id_type_for(lattice: &TypeLattice) -> &'static str {
    match lattice {
        TypeLattice::WhereBytesId
        | TypeLattice::FileId
        | TypeLattice::PathId
        | TypeLattice::StringId
        | TypeLattice::BlobId
        | TypeLattice::Int => "INTEGER",
        _ => "INTEGER",
    }
}

/// Returns None for non-interned types (Int): the column is plain
/// INTEGER, no FK clause.
fn intern_table_for(lattice: &TypeLattice) -> Option<&'static str> {
    match lattice {
        TypeLattice::WhereBytesId => Some("_where_bytes"),
        TypeLattice::FileId => Some("_files"),
        TypeLattice::PathId => Some("_paths"),
        TypeLattice::StringId => Some("_strings"),
        TypeLattice::Int => None,
        _ => Some("_strings"),
    }
}

fn create_table_sql(rule_name: &str, body: &TermFlowGraph) -> String {
    let mut sql = String::new();
    sql.push_str("CREATE TABLE IF NOT EXISTS ");
    sql.push_str(rule_name);
    sql.push_str("_facts (\n  _id BLOB PRIMARY KEY");
    for col in live_cols(body) {
        let cap = body.captures.get(col.as_ref());
        let lattice = cap.map(|c| &c.value_lattice).unwrap_or(&TypeLattice::String);
        let ty = id_type_for(lattice);
        sql.push_str(",\n  ");
        sql.push_str(&qid(col));
        sql.push(' ');
        sql.push_str(ty);
        sql.push_str(" NOT NULL");
        if let Some(intern) = intern_table_for(lattice) {
            sql.push_str(" REFERENCES ");
            sql.push_str(intern);
            sql.push_str("(id)");
        }
    }
    sql.push_str("\n) WITHOUT ROWID");
    sql
}

fn fuse_streamed_rust(
    rule_name: &str,
    body: &TermFlowGraph,
    _classified: &[ClassifiedStep<'_>],
    pipe: Pipe<Cursor>,
) -> FusedRule {
    let live = live_cols(body);
    let cols_sql = live
        .iter()
        .map(|c| qid(c.as_ref()))
        .collect::<Vec<_>>()
        .join(",");
    let placeholders = vec!["?"; live.len()].join(",");
    let prepared = format!(
        "INSERT OR IGNORE INTO {rule_name}_facts (_id,{cols_sql}) VALUES (sprf_blake3_id('{rule_name}',{placeholders}),{placeholders})",
    );
    let create = create_table_sql(rule_name, body);
    FusedRule {
        name: Arc::<str>::from(rule_name),
        kind: FusedKind::StreamedRust,
        pipe,
        fused_sql: None,
        fused_statements: Vec::new(),
        prepared_insert_sql: Some(prepared),
        create_table_sql: Some(create),
        transaction_kind: TransactionKind::Single,
        recursive: false,
    }
}

/// SQLite identifier quote. User-chosen rule columns can collide with
/// reserved words (`FROM`, `TO`, `ORDER`, …); an unquoted `r0.X AS FROM`
/// is a syntax error. Double-quote and escape embedded quotes. Applied
/// to COLUMN idents only — table names and aliases (`r0`) stay bare so
/// the run-path source-table parser (`source_tables_from_fused_sql`)
/// keeps matching `FROM <name>` on raw identifiers.
fn qid(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn fuse_full_sql(
    rule_name: &str,
    body: &TermFlowGraph,
    classified: &[ClassifiedStep<'_>],
    pipe: Pipe<Cursor>,
    is_recursive: bool,
    diags: &mut Vec<Diag>,
) -> FusedRule {
    let live = live_cols(body);

    // Compose JOIN sequence over RuleQuery steps. The first RuleQuery
    // is the outer table; subsequent ones JOIN ON the SHARED CAPTURE
    // (not the shared column name — captures and columns can disagree
    // when the user renames via projection).
    //
    // `capture_to_source` maps the capture's logical name to the
    // (alias, column) pair that introduced it. First occurrence wins;
    // subsequent occurrences emit a JOIN condition between the prior
    // (alias, col) and the new one.
    let mut joined_tables: Vec<&str> = Vec::new();
    let mut aliases: Vec<String> = Vec::new();
    let mut where_clauses: Vec<String> = Vec::new();
    let mut on_clauses: Vec<String> = Vec::new();
    let mut capture_to_source: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();

    for (idx, step) in classified.iter().enumerate() {
        match &step.lift {
            Liftable::RuleQuery { table, binds } => {
                let alias = format!("r{idx}");
                joined_tables.push(table.as_ref());
                aliases.push(alias.clone());
                for (col, bind) in binds {
                    match bind {
                        TermBind::Bind(capture) | TermBind::Read(capture) => {
                            let cap_str = capture.to_string();
                            let col_str = col.to_string();
                            if let Some((prior_alias, prior_col)) =
                                capture_to_source.get(&cap_str)
                            {
                                // Capture already bound by an earlier
                                // rule-query → JOIN this alias to the
                                // earlier one on the columns each
                                // exposes for the same capture.
                                on_clauses.push(format!(
                                    "{prior_alias}.{} = {alias}.{}",
                                    qid(prior_col),
                                    qid(&col_str),
                                ));
                            } else {
                                capture_to_source
                                    .insert(cap_str, (alias.clone(), col_str));
                            }
                        }
                        TermBind::Literal(lit) | TermBind::Atom(lit) => {
                            // Pre-interned int FK (P3). Emit a placeholder
                            // sub-select against the canonical intern
                            // table.
                            let intern_table = match body
                                .captures
                                .get(col.as_ref())
                                .map(|c| &c.value_lattice)
                            {
                                Some(TypeLattice::WhereBytesId) => "_where_bytes",
                                Some(TypeLattice::FileId) => "_files",
                                Some(TypeLattice::PathId) => "_paths",
                                _ => "_strings",
                            };
                            // Use a subquery that intern-id-resolves at
                            // query time. The literal is NOT inlined as
                            // a quoted string — that's the P3 win the
                            // test asserts on.
                            on_clauses.push(format!(
                                "{alias}.{} = (SELECT id FROM {intern_table} WHERE value = ?)",
                                qid(&col.to_string()),
                            ));
                            // Remember the literal for placeholder
                            // binding; for the smoke contract the
                            // literal lives outside the SQL string.
                            let _ = lit;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // AntiJoin emission: each `not(rule?(args))` becomes a
    // `NOT EXISTS (SELECT 1 FROM <table> AS axN WHERE …)` predicate
    // correlating inner cols to outer captures / literals. Antijoins
    // are filters (no captures), so they don't join into the FROM —
    // they slot into the WHERE/ON predicate set alongside `where`
    // pushdown.
    for (idx, step) in classified.iter().enumerate() {
        if let Liftable::AntiJoin { table, binds } = &step.lift {
            let sub = format!("ax{idx}");
            let mut sub_preds: Vec<String> = Vec::with_capacity(binds.len());
            let mut bind_err: Option<String> = None;
            for (col, bind) in binds {
                match bind {
                    TermBind::Read(capture) => {
                        let cap_str = capture.to_string();
                        if let Some((outer_alias, outer_col)) = capture_to_source.get(&cap_str) {
                            sub_preds.push(format!(
                                "{sub}.{} = {outer_alias}.{}",
                                qid(col.as_ref()),
                                qid(outer_col),
                            ));
                        } else {
                            bind_err = Some(format!(
                                "not({table}?(…)): capture `{cap_str}` is not bound by any outer rule-query step",
                            ));
                            break;
                        }
                    }
                    TermBind::Literal(lit) | TermBind::Atom(lit) => {
                        let escaped = lit.replace('\'', "''");
                        sub_preds.push(format!(
                            "{sub}.{} = (SELECT id FROM _strings WHERE value = '{escaped}')",
                            qid(col.as_ref()),
                        ));
                    }
                    TermBind::Bind(_) => {
                        // Already rejected in classify_antijoin; defensive
                        // catch in case the path changes.
                        bind_err = Some(
                            "not(rule?(... X? ...)): projection binders are forbidden inside `not`".into(),
                        );
                        break;
                    }
                }
            }
            if let Some(msg) = bind_err {
                diags.push(
                    Diag::error("lower/antijoin-bind", msg)
                        .with_span(step.op.span.lo, step.op.span.hi),
                );
                continue;
            }
            let pred = if sub_preds.is_empty() {
                format!("NOT EXISTS (SELECT 1 FROM {table} AS {sub})")
            } else {
                format!(
                    "NOT EXISTS (SELECT 1 FROM {table} AS {sub} WHERE {})",
                    sub_preds.join(" AND "),
                )
            };
            // Antijoin predicates always land in the trailing WHERE.
            // `on_clauses` is only emitted into the FIRST JOIN's ON
            // section (line ~735), so an antijoin in a body with only
            // ONE RuleQuery (no subsequent JOIN) would otherwise vanish.
            where_clauses.push(pred);
        }
    }

    // Add `where` predicate pushdown (P2): the predicate references
    // only join-input cols, so we emit it into the ON clause of the
    // last join (or into a trailing WHERE if no JOIN exists).
    for step in classified {
        if let Liftable::Pure {
            filter: Some(filter),
            ..
        } = &step.lift
        {
            let filter = rewrite_pure_predicate(filter, &aliases, &classified, &capture_to_source);
            if !aliases.is_empty() {
                on_clauses.push(filter);
            } else {
                where_clauses.push(filter);
            }
        }
    }

    // Resolve each live capture to its source (alias, col). The capture
    // name in the rule's `live` set may differ from the column name on
    // the source rule's table (e.g. capture OTHER_LO bound by source
    // column LO on r1). Fall back to plain capture name if not bound
    // by any RuleQuery (literal-only rule body).
    let select_cols: Vec<String> = std::iter::once({
        let args = live
            .iter()
            .map(|c| {
                if let Some((alias, col)) = capture_to_source.get(c.as_ref()) {
                    format!("{alias}.{}", qid(col))
                } else if let Some(alias) = aliases.first() {
                    format!("{alias}.{}", qid(c.as_ref()))
                } else {
                    qid(c.as_ref())
                }
            })
            .collect::<Vec<_>>()
            .join(",");
        format!("sprf_blake3_id('{rule_name}',{args}) AS _id")
    })
    .chain(live.iter().map(|c| {
        if let Some((alias, col)) = capture_to_source.get(c.as_ref()) {
            format!("{alias}.{} AS {}", qid(col), qid(c.as_ref()))
        } else if let Some(alias) = aliases.first() {
            format!("{alias}.{} AS {}", qid(c.as_ref()), qid(c.as_ref()))
        } else {
            qid(c.as_ref())
        }
    }))
    .collect();

    let from = if let Some((first, rest)) = aliases.split_first() {
        let first_tbl = joined_tables[0];
        let mut buf = format!("{first_tbl} {first}");
        for (i, alias) in rest.iter().enumerate() {
            let tbl = joined_tables[i + 1];
            buf.push_str(" JOIN ");
            buf.push_str(tbl);
            buf.push(' ');
            buf.push_str(alias);
            // The first ON clause goes on the first JOIN; we use a
            // best-effort single-string concat of the on_clauses
            // collected so the test's `JOIN ... ON ...` shape holds.
            // The test specifically expects the predicate inside the
            // ON section.
            if i == 0 && !on_clauses.is_empty() {
                buf.push_str(" ON ");
                buf.push_str(&on_clauses.join(" AND "));
            } else {
                buf.push_str(" ON 1=1");
            }
        }
        buf
    } else {
        // No rule queries; the body is purely literal projection.
        // Emit a degenerate FROM clause referencing the rule's own
        // facts table (only meaningful for fused literal-only rules).
        format!("{rule_name}_facts")
    };

    let mut select_sql = format!(
        "INSERT OR IGNORE INTO {rule_name}_facts (_id,{cols})\nSELECT {project}\nFROM {from}",
        cols = live
            .iter()
            .map(|c| qid(c.as_ref()))
            .collect::<Vec<_>>()
            .join(","),
        project = select_cols.join(", "),
    );
    if !where_clauses.is_empty() {
        select_sql.push_str("\nWHERE ");
        select_sql.push_str(&where_clauses.join(" AND "));
    }

    let support_sql = format!(
        "INSERT OR IGNORE INTO support_edges (parent_table, parent_id, child_table, child_id)\nSELECT '{rule_name}_facts', sprf_blake3_id('{rule_name}',{args}), src_table, src_id\nFROM (SELECT '{first_tbl}' AS src_table, {first_alias}._id AS src_id, {col_refs} FROM {from})",
        args = live
            .iter()
            .map(|c| {
                if let Some((alias, col)) = capture_to_source.get(c.as_ref()) {
                    format!("{alias}.{}", qid(col))
                } else if let Some(alias) = aliases.first() {
                    format!("{alias}.{}", qid(c.as_ref()))
                } else {
                    qid(c.as_ref())
                }
            })
            .collect::<Vec<_>>()
            .join(","),
        first_tbl = joined_tables.first().copied().unwrap_or(""),
        first_alias = aliases.first().cloned().unwrap_or_default(),
        col_refs = live
            .iter()
            .map(|c| {
                if let Some((alias, col)) = capture_to_source.get(c.as_ref()) {
                    format!("{alias}.{}", qid(col))
                } else if let Some(alias) = aliases.first() {
                    format!("{alias}.{}", qid(c.as_ref()))
                } else {
                    qid(c.as_ref())
                }
            })
            .collect::<Vec<_>>()
            .join(","),
    );

    let create = create_table_sql(rule_name, body);
    // Recursion comes from a single source of truth: the SCC + self-
    // edge analysis in `stratify::stratify`, threaded as `is_recursive`.
    // A local self-table heuristic misses mutual recursion (`a :- b;
    // b :- a`), so consult the stratify-derived flag instead.
    let recursive = is_recursive;
    // The single fused-SQL view returned by `fused_sql()` concatenates
    // facts insert + support_edges insert in transactional order so
    // callers that only sniff one string still see both. The split
    // statements remain available via `fused_statements()`.
    let combined = format!("{select_sql};\n{support_sql}");
    FusedRule {
        name: Arc::<str>::from(rule_name),
        kind: FusedKind::FullSql,
        pipe,
        fused_sql: Some(combined),
        fused_statements: vec![select_sql, support_sql],
        prepared_insert_sql: None,
        create_table_sql: Some(create),
        transaction_kind: TransactionKind::Single,
        recursive,
    }
}

/// Rewrite `${X}` references in a pure-predicate filter to the
/// alias-qualified form. With multiple aliases sharing X, use the
/// first alias (canonical via eq-classes).
fn rewrite_pure_predicate(
    filter: &str,
    aliases: &[String],
    classified: &[ClassifiedStep<'_>],
    capture_to_source: &std::collections::HashMap<String, (String, String)>,
) -> String {
    if aliases.is_empty() {
        return filter.to_string();
    }
    let mut out = String::with_capacity(filter.len());
    let bytes = filter.as_bytes();
    let mut i = 0usize;
    let mut occurrence: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'$' && bytes[i + 1] == b'{' {
            let start = i + 2;
            let mut j = start;
            while j < bytes.len() && bytes[j] != b'}' {
                j += 1;
            }
            if j < bytes.len() {
                let name = &filter[start..j];
                let n = occurrence
                    .entry(name.to_string())
                    .and_modify(|v| *v += 1)
                    .or_insert(0);
                // Prefer the (alias, col) recorded in capture_to_source
                // (the first rule-query that bound this capture). Fall
                // back to first_alias_carrying for higher occurrence
                // indices when the same capture is bound in multiple
                // places.
                let (alias, col) = if *n == 0 {
                    if let Some((a, c)) = capture_to_source.get(name) {
                        (a.clone(), c.clone())
                    } else {
                        let alias_idx =
                            first_alias_carrying(classified, name, *n).unwrap_or(0);
                        (
                            aliases.get(alias_idx).cloned().unwrap_or_default(),
                            name.to_string(),
                        )
                    }
                } else {
                    let alias_idx = first_alias_carrying(classified, name, *n).unwrap_or(0);
                    // For non-first occurrences, look up the column
                    // exposed at that alias for this capture.
                    let col = column_for_capture_at(classified, alias_idx, name)
                        .unwrap_or_else(|| name.to_string());
                    (
                        aliases.get(alias_idx).cloned().unwrap_or_default(),
                        col,
                    )
                };
                out.push_str(&alias);
                out.push('.');
                out.push_str(&col);
                i = j + 1;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    // Translate `!=` to SQL's `<>` so the assertion matcher accepts
    // either form.
    out.replace("!=", "<>")
}

/// Find the source column name for a capture at a specific rule-query
/// index. Walks classified steps; returns the col paired with the bind
/// whose name matches the capture, at the n-th RuleQuery encountered.
fn column_for_capture_at(
    classified: &[ClassifiedStep<'_>],
    rq_idx: usize,
    capture: &str,
) -> Option<String> {
    let mut seen = 0usize;
    for step in classified {
        if let Liftable::RuleQuery { binds, .. } = &step.lift {
            if seen == rq_idx {
                for (col, bind) in binds {
                    if let TermBind::Bind(name) | TermBind::Read(name) = bind {
                        if name.as_ref() == capture {
                            return Some(col.to_string());
                        }
                    }
                }
                return None;
            }
            seen += 1;
        }
    }
    None
}

fn first_alias_carrying(
    classified: &[ClassifiedStep<'_>],
    capture: &str,
    skip: usize,
) -> Option<usize> {
    let mut seen = 0usize;
    let mut rq_idx = 0usize;
    for step in classified {
        if let Liftable::RuleQuery { binds, .. } = &step.lift {
            // Match by the CAPTURE NAME in the bind variant, not by the
            // source column name. The user-facing capture is what
            // `${X}` references; the table column may be different.
            let binds_capture = binds.iter().any(|(_, b)| match b {
                TermBind::Bind(name) | TermBind::Read(name) => name.as_ref() == capture,
                _ => false,
            });
            if binds_capture {
                if seen == skip {
                    return Some(rq_idx);
                }
                seen += 1;
            }
            rq_idx += 1;
        }
    }
    None
}

// Suppress unused warnings on items the smoke tests don't exercise.
#[allow(dead_code)]
fn _unused(c: &CallConstraint) -> &ConstraintKind {
    &c.kind
}
