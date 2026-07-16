use anyhow::{bail, Context, Result};
use rayon::prelude::*;
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use crate::ast::*;
use crate::ingest;
use crate::lower::{lower_query, lower_rule, tbl};
use crate::scc;
use crate::spine;

// The effect runtime moved to crate::effect (engine breakdown Stage 5).
// Re-export the names external call sites (daemon, tests) and the rest of
// engine.rs reach via `engine::`, so their paths keep resolving.
use crate::effect::async_bound_vars;
pub use crate::effect::{async_effect_arity, shell_templates, EffectExec, ShellEffectExec};

// Built-in graph/CST/spine/daemon extractor methods (bucket E) live in a child
// module to shrink this file; they're still `impl Engine` methods called as
// `self.refresh_*` from the tick orchestrator (engine breakdown Stage 4).
mod declare;
#[cfg(test)]
mod deltaflow;
mod derive;
mod extract;
pub(crate) mod family;
mod gen;
#[cfg(test)]
mod generation;
mod lang_tables;
mod lens;
mod meta;
#[cfg(test)]
mod ownership;
mod path_reconcile;
pub(crate) mod pipeline;
pub(crate) mod query;
mod reconcile;
mod repo;
mod rpc;
mod source_prepare;
#[cfg(test)]
mod staged_delta;
mod symbols;
pub(crate) use query::emit_query_json;
pub(crate) use repo::git_batch_read;
mod decls;
pub(crate) use decls::*;
pub use decls::{
    all_builtin_decls, builtin_enum_brands, builtin_enum_variants, builtin_rel_names, fn_docs,
    op_docs, undocumented_builtins, undocumented_fns,
};
pub use lang_tables::ast_langs;
pub(crate) use lang_tables::ts_lang;
/// The structured result of one `checkout_one` sweep. `action` ∈
/// ff|branch-f|skip; `ok` = the git op succeeded (skip-dirty carries ok=true —
/// the SKIP is intentional); `detail` = the human line. Fed into both the
/// `[checkout]` log line and the `checkout_done` / `checkout_plan` rel.
#[derive(Clone, Debug)]
struct CheckoutOutcome {
    action: &'static str,
    ok: bool,
    detail: String,
}
// The mixed source+derived / extract+derived rel desugar: a pure Program ->
// Program rewrite that runs immediately before rule classification in both
// tick entry points (see `tick.rs`). Public so `crate::rels::perf` can map a
// twin rel name back to the one a program declared (D4 telemetry display).
pub mod desugar;
// The reactive tick orchestrator (`tick` / `tick_paths`) lives in a child
// module too; both stay `pub` and reach this module's privates directly
// (engine breakdown Stage 6).
mod strata;
mod tick;
pub(crate) mod type_arena;
pub(crate) mod typed_plan;
pub(crate) use strata::*;
mod scan;
pub(crate) use scan::*;
mod eval;
pub(crate) use eval::*;
pub use tick::{is_timer_rel, PathTickFallbackPolicy, PathTickOutcome, TickReport};

fn scc_node_tbl(edge: &str) -> String {
    format!("scc_node_{edge}")
}
fn scc_edge_tbl(edge: &str) -> String {
    format!("scc_edge_{edge}")
}
/// The per-`@next`-rel carry buffer: the live rel's columns plus a `tx`
/// generation column. Rows staged at `tx = cur+1` surface as the live rel at the
/// start of the next tick. See docs/research-reactive-effectful-datalog.md §8.
fn carry_tbl(rel: &str) -> String {
    format!("_carry_{rel}")
}

/// Current wall-clock time in whole seconds since the epoch, used by the `every`
/// clock. `DL_NOW_SECS` overrides it so tests can advance time deterministically
/// across ticks without sleeping.
fn now_secs() -> i64 {
    if let Ok(v) = std::env::var("DL_NOW_SECS") {
        if let Ok(n) = v.parse::<i64>() {
            return n;
        }
    }
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Brute-force top-k cosine neighbors over an L2-normalized vector pool, emitted
/// as `(a, b, score)` rows with `score = round(cosine * 1e6)` as Int. Shared by
/// the text `similar` rel and the structural `node2vec` rel — both reduce to
/// "nearest neighbors over a `Vec<(id, vec)>`", only the vectors differ.
pub(crate) fn knn_rows(pool: &[(String, Vec<f32>)], k: usize) -> Vec<Vec<Value>> {
    let mut rows: Vec<Vec<Value>> = Vec::new();
    for (i, (a, va)) in pool.iter().enumerate() {
        let mut scored: Vec<(f32, &str)> = Vec::with_capacity(pool.len().saturating_sub(1));
        for (j, (b, vb)) in pool.iter().enumerate() {
            if i == j {
                continue;
            }
            scored.push((crate::embed::cosine(va, vb), b.as_str()));
        }
        scored.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap_or(std::cmp::Ordering::Equal));
        for (sc, b) in scored.into_iter().take(k) {
            rows.push(vec![
                Value::Text(a.clone()),
                Value::Text(b.to_string()),
                Value::Int((sc * 1_000_000.0).round() as i64),
            ]);
        }
    }
    rows
}

/// Order-independent content digest of a directed edge list: XOR-fold of
/// `blake3(src "\0" dst)` over rows. The edge rel is a set (PK), so no row
/// repeats and XOR cannot cancel a pair; XOR is commutative + associative, so
/// the rel's row order across rebuilds is irrelevant. All-zero ⇒ empty graph.
/// Mirrors `source_rule_digests`' fold (engine.rs); the node2vec digest-skip
/// guard (W1) keys `_reldigest` on this so an unchanged graph skips re-embed.
fn blake3_edges(edges: &[(String, String)]) -> [u8; 32] {
    let mut acc = [0u8; 32];
    for (a, b) in edges {
        let mut buf = String::with_capacity(a.len() + b.len() + 1);
        buf.push_str(a);
        buf.push('\0');
        buf.push_str(b);
        let h = blake3::hash(buf.as_bytes());
        for (x, y) in acc.iter_mut().zip(h.as_bytes()) {
            *x ^= *y;
        }
    }
    acc
}

/// Per-tick `cmd` invocation counter (parse_file runs across rayon, hence
/// atomic; process-global like the profile stats — one engine per process in
/// real use, e2e tests get their own subprocess).
static CMD_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// The cmd budget: `--cmd-budget` (via `set_cmd_budget`) wins, else
/// `DL_CMD_BUDGET`, else unlimited. Fixed after first read.
static CMD_BUDGET: OnceLock<Option<u32>> = OnceLock::new();

/// Force the cmd budget (the `--cmd-budget` flag). Call before the first tick.
pub fn set_cmd_budget(n: u32) {
    let _ = CMD_BUDGET.set(Some(n));
}

fn cmd_budget() -> Option<u32> {
    *CMD_BUDGET.get_or_init(|| {
        std::env::var("DL_CMD_BUDGET")
            .ok()
            .and_then(|v| v.parse().ok())
    })
}

/// Per-file size cap for the walker, in bytes. `DL_MAX_FILESIZE` (e.g. 1048576),
/// else no cap (legacy behavior). Files larger than this are skipped before any
/// content read/hash, in both the WORK walk and the git-rev ls-tree listing.
static MAX_FILESIZE: OnceLock<Option<u64>> = OnceLock::new();
fn max_filesize() -> Option<u64> {
    *MAX_FILESIZE.get_or_init(|| {
        std::env::var("DL_MAX_FILESIZE")
            .ok()
            .and_then(|v| v.parse().ok())
    })
}

/// Slow-tick log threshold in ms. A `tick_paths` slower than this prints a
/// `[tick]` line to stderr (the LSP server log), so live dogfooding catches a
/// perf regression. `DL_TICK_LOG_MS` overrides; default 250ms. 0 logs every tick.
static TICK_LOG_MS: OnceLock<f64> = OnceLock::new();
fn tick_log_ms() -> f64 {
    *TICK_LOG_MS.get_or_init(|| {
        std::env::var("DL_TICK_LOG_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(250.0)
    })
}

/// Edge-count guard for a `?` query that would evaluate a closure VIEW. The
/// view materializes the FULL reachability relation, and a LIMIT does not
/// short-circuit it (the UNION + recursive CTE run before the first row emits
/// — measured >10s for `LIMIT 5` on a 471k-edge graph, minutes unbounded).
/// Pinned queries answer through the seeded condensation walk in microseconds;
/// anything that falls through to the view on an edge rel bigger than this is
/// refused loudly instead of hanging the tick.
/// `DL_CLOSURE_QUERY_MAX_EDGES` overrides; 0 disables. Default 20k.
static CLOSURE_QUERY_MAX_EDGES: OnceLock<usize> = OnceLock::new();
fn closure_query_max_edges() -> usize {
    *CLOSURE_QUERY_MAX_EDGES.get_or_init(|| {
        std::env::var("DL_CLOSURE_QUERY_MAX_EDGES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(20_000)
    })
}

/// Stringify whatever a cell holds, regardless of its SQLite storage type.
/// A generic row reader (rel_rows, load_edges, edge_content_digest) can't
/// assume TEXT any more now that `sym`-typed columns (df_node.id and its
/// kin) store INTEGER — `row.get::<_, String>(i)` on those is a rusqlite
/// type error, which a `.filter_map(Result::ok)`/`.flatten()` reader would
/// silently drop the whole row for (the intern-key arc's first regression:
/// closure(df_edge) read zero rows because every edge row errored here).
fn cell_as_string(r: &rusqlite::Row, i: usize) -> rusqlite::Result<String> {
    Ok(match r.get_ref(i)? {
        rusqlite::types::ValueRef::Null => String::new(),
        rusqlite::types::ValueRef::Integer(n) => n.to_string(),
        rusqlite::types::ValueRef::Real(f) => f.to_string(),
        rusqlite::types::ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
        rusqlite::types::ValueRef::Blob(b) => String::from_utf8_lossy(b).into_owned(),
    })
}

/// The built-in relations of the data-model contract (docs/data-model.md). A
/// `.dl` program may not declare these names; they are registered with fixed
/// schemas and refreshed each tick from the `_file` change-detection cache, so
/// any rule can join the file set without a `scan`. Stage 1: ids are the raw
/// rev string / content hash (no interning yet; that is Stage 2).
const BUILTIN_RELS: [&str; 5] = ["repo", "rev", "content", "file", "true"];

/// Tick-audit mode: `--tick-audit` or `DL_TICK_AUDIT=1`. After each tick,
/// print every relation's row count so you can see the cardinality graph at a
/// glance — dead extractors, blown-up joins, closure explosion.
static TICK_AUDIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn set_tick_audit(on: bool) {
    TICK_AUDIT.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Configure the GLOBAL rayon pool from `DL_RAYON_THREADS` (default 2).
/// A finite default bounds the CPU the daemon's extract/hash paths can burn;
/// operators who explicitly want more parallelism can raise the override.
/// Must run before any rayon parallelism (called first thing from `cli::run`).
/// The checkout sink has its OWN narrower pool (`DL_CHECKOUT_WIDTH`), so this
/// caps the extract/hash hot paths.
pub fn init_thread_pool() {
    let n = rayon_thread_count(std::env::var("DL_RAYON_THREADS").ok().as_deref());
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(n)
        .thread_name(|i| format!("dl-{i}"))
        .build_global();
}

fn rayon_thread_count(value: Option<&str>) -> usize {
    value
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(2)
}

#[cfg(test)]
mod rayon_thread_tests {
    use super::rayon_thread_count;

    #[test]
    fn defaults_to_two_and_honors_positive_override() {
        assert_eq!(rayon_thread_count(None), 2);
        assert_eq!(rayon_thread_count(Some("")), 2);
        assert_eq!(rayon_thread_count(Some("0")), 2);
        assert_eq!(rayon_thread_count(Some("bogus")), 2);
        assert_eq!(rayon_thread_count(Some("6")), 6);
    }
}
pub fn tick_audit() -> bool {
    TICK_AUDIT.load(std::sync::atomic::Ordering::Relaxed)
        || std::env::var("DL_TICK_AUDIT").is_ok_and(|v| !v.is_empty() && v != "0")
}

/// Whether the network/mutating sinks (`repo` pulls + `checkout` sweeps) should
/// drain on a ONE-SHOT read path. The daemon's poll loop and `--watch`/`--settle`
/// (the in-process daemon twins) always drain on their cadence; this gate is
/// only consulted by `run_file_inproc` so a bare `dl prog.dl` is a pure read
/// (no 90s destructive network sweep from a `?` query). Set by `--apply` /
/// `DL_APPLY_SINKS=1`. `DL_CHECKOUT_DRY_RUN=1` implies it (a preview must
/// actually run the plan pass), so it works alone.
pub fn apply_sinks_enabled() -> bool {
    std::env::var_os("DL_APPLY_SINKS").is_some()
        || std::env::var_os("DL_CHECKOUT_DRY_RUN").is_some()
}

/// The module-graph relations (modgraph.rs). Reserved like BUILTIN_RELS, declared
/// every tick, but populated by `refresh_module_rels` only when the program
/// references one (resolution parses every file, so it is lazy). `module_edge` is
/// the 2-col convenience closure edge; `module_edge_rev` is the rev-aware form.
pub(crate) const MODULE_RELS: [&str; 10] = [
    "module_import",
    "module_edge",
    "module_edge_rev",
    "module_unresolved",
    "module_unresolved_rev",
    "crate_edge",
    "module_binding_resolved_rev",
    "module_binding_resolved",
    "module_binding_rev",
    "module_binding",
];

/// Syntax-only type graph. `kind` is edge metadata; closure(type_edge) walks
/// the first two columns. `type_edge`/`type_edge_rev` are name-keyed (the
/// historic contract) with a trailing `repo` column so two trees scanned in
/// the same engine instance that happen to share a type name (e.g. two
/// frozen prior versions of the same crate) don't collapse into one node —
/// the column is appended last specifically so it never shifts `from`/`to`
/// out of cols[0]/cols[1]. The sem-style additions are def-keyed: `type_entity`
/// is the declared-symbol table (kind, parent, location), `type_sig` is each
/// callable's arrow `[...A] => B` exploded by slot, and `type_link` is the
/// SCIP-resolved graph where endpoints are definition symbols, not bare names
/// (already repo-prefixed via type_entity's sym, so it doesn't need its own
/// repo column).
pub(crate) const TYPE_RELS: [&str; 7] = [
    "type_edge",
    "type_edge_rev",
    "type_entity",
    "type_entity_rev",
    "type_sig",
    "type_link",
    "type_link_rev",
];

/// Phase D diet-SCIP call graph. `call_def` is each callable (sym, kind, file,
/// span); `call_site` is each call occurrence (caller sym, callee text, file,
/// line); `call_edge` is the resolved closure edge; `call_edge_rev` is the
/// rev-aware source of truth (same split as type_edge / type_edge_rev).
/// `call_kind` is the per-fn read/write classification of those call sites,
/// keyed by the bare callee name (execute/query_row/etc.) so a rail can join
/// on `write` only. Symbols are `file::kind::name`, the same shape
/// `type_entity` uses, so the call and type graphs share nodes and a join
/// reaches both.
pub(crate) const CALL_RELS: [&str; 7] = [
    "call_def",
    "call_def_rev",
    "call_site",
    "call_edge",
    "call_edge_rev",
    "call_name",
    "call_kind",
];

/// Intra-procedural dataflow lift: `df_node(id, kind, var, fn, file, line)` is a
/// value-bearing program point, `df_edge(from, to)` is local value flow. A rule
/// `df_reaches(a,b) <- closure(df_edge)` walks the lifted graph on the shared SCC
/// engine. `loop_over` records each loop's span + variable for the
/// loop-invariant-call flag; `allocates` marks fns whose body builds a
/// collection; `nest(call_id, loop_id, depth, collection)` records each call's
/// enclosing loop nest, composing over `call_edge` into symbolic Big-O
/// ("depth-N over C") without resolving trip counts. `df_arg` records which
/// positional slot an argument value feeds (receiver = -1); `df_field` is named
/// value flow into a composite (struct-literal field, object-literal property,
/// Kotlin named argument). See `typegraph::DataflowFacts`.
/// `df_node`/`df_node_repo`/`df_arg`/`df_field` gain `_rev` twins (D5.4): the
/// diff-consumed df rels carry rev, with node ids salted by rev (`salt_rev`) so
/// two revs' `file:line:col` ids stay disjoint in one table. The legacy rels
/// keep raw ids (single-rev daemon sees today's behavior). `df_edge`/`loop_over`/
/// `allocates`/`nest`/`df_param` stay WORK-only (flow/perf inputs, deferred).
/// `df_lit`/`df_lit_rev` (string-values arc, item 1): one row per STRING-
/// carrying `df_node` (kind lit/template/concat) with its cooked/raw text;
/// same rev-salted-id shape as `df_field`/`df_field_rev`. See
/// `typegraph::DataflowFacts::lits`.
pub(crate) const DATAFLOW_RELS: [&str; 15] = [
    "df_node",
    "df_node_rev",
    "df_node_repo",
    "df_node_repo_rev",
    "df_edge",
    "loop_over",
    "allocates",
    "nest",
    "df_param",
    "df_arg",
    "df_arg_rev",
    "df_field",
    "df_field_rev",
    "df_lit",
    "df_lit_rev",
];

/// Document structure from non-source text (markdown today; comments and other
/// tree-sitter grammars to follow via `ingest::IngestLang`). `doc_node` is one row
/// per heading / code block / section: (file, line, kind, name, parent). The
/// `parent` column is the enclosing heading text, so a rule can walk the section
/// tree. `doc_ref` is the doc→code bridge: (file, line, sym) where a heading's
/// name matches a `type_entity` name. Populated by the `ingest` registry over
/// `_file`'s document-typed files (a source rule scanning `**/*.md` feeds `_file`,
/// same as the source langs).
pub(crate) const DOC_RELS: [&str; 2] = ["doc_node", "doc_ref"];

/// Doc comments attached to declared entities (Tier 1/2 doc gen). `doc_comment`
/// is one row per documented `type_entity`: (repo, sym, line, text), the cleaned
/// block bound to the same sym. `doc_tag` is the structured split: (repo, sym,
/// tag, arg, text) where tag is `param`/`returns`/`deprecated`/`section`/... .
/// Both are populated in `refresh_type_rels` from the one parse that already
/// builds `type_entity`, by the per-language AST locators in `typegraph`.
pub(crate) const DOC_TEXT_RELS: [&str; 2] = ["doc_comment", "doc_tag"];

/// String values folded from `const`/`as const` bindings (string-values arc,
/// item 3): `const_value(repo, sym, field, text, kind, file, line)` — one row
/// per string-valued leaf, `sym` the owning `type_entity` (the const itself,
/// or the enum for a string member), `field` a dotted key path ("" for a bare
/// const). `const_value_rev` is the rev-carrying twin (rev is a plain trailing
/// column, like `type_entity_rev` — sym never collides across revs the way a
/// line-keyed df id does, so no id-salting here). Both ride `refresh_type_rels`
/// (the same TypeFacts parse `doc_comment` rides), so a program that asks for
/// either gates the type family the same way `doc_text_rels_used` does. `line`
/// is 1-based (rustc/tsc convention), same as `type_entity.line`.
pub(crate) const CONST_VALUE_RELS: [&str; 2] = ["const_value", "const_value_rev"];

/// Every comment in every parsed file as a grammar-backed fact:
/// `comment_node(path, line, col, end_line, end_col, text, kind)`. Unlike
/// `doc_comment` (which rides the TypeLang parse and covers only the three
/// TypeLang languages' DOC comments bound to an entity), `comment_node` is its
/// OWN family: it records EVERY comment — line, block, and doc — across the
/// oxc TS/TSX front-end AND every tree-sitter grammar the `ast` op loads
/// (Rust, Kotlin, Python, Go, C, bash, ...). `line`/`col` are 1-based line,
/// 0-based byte column (the `sg`/`diag` convention); `text` is the comment body
/// with tokens stripped; `kind` ∈ line | block | doc. String-literal safe: a
/// `//` inside a string is lexed as string content, never a comment row. The
/// eslint/biome suppression grammar (`std/suppress.dl`) is pure dl over this.
pub(crate) const COMMENT_RELS: [&str; 1] = ["comment_node"];

/// Every template literal in every TS/TSX/JS/JSX/MJS/CJS file, split into its
/// ordered static/interpolated pieces:
/// `template_parts(file, line, node, idx, kind, text)`. Own family (rides the
/// oxc parse `TsTypes` already does, but is not gated behind `type`/`call`/
/// `dataflow` — a program reading only `template_parts` shouldn't pay for
/// those passes). `node` groups a template literal occurrence's pieces (the
/// byte offset of its own span start, stable across ticks for unchanged
/// content); `idx` orders them 0-based; `kind` is `static` | `expr`; `text` is
/// the static chunk verbatim (raw, unescaped) or the interpolated expression's
/// exact source text. `line` is 1-based (the `comment_node`/`sg`/`diag`
/// convention). Template-built import paths / URLs / route keys become
/// joinable: `template_parts(file, _, node, 0, "static", "GET /users/"), ...`.
/// Kotlin string templates and Rust `format!`-style macros are OUT of scope
/// (Rust has no native template-literal syntax); this family emits nothing
/// for either language rather than guessing at a shape.
pub(crate) const TEMPLATE_RELS: [&str; 1] = ["template_parts"];

/// Every runtime-computed edge marker in every TS/TSX/JS/JSX/MJS/CJS file:
/// `unresolved(file, line, reason, detail)`. Own family (rides the oxc parse,
/// not gated behind `type`/`call`/`dataflow`/`module`, matching
/// `template_parts`). Distinguishes "an edge exists but its target is
/// computed at runtime" from `module_unresolved`'s "no edge exists" (a
/// specifier that resolved to no project file at all) — this rel does NOT
/// replace `module_unresolved`, it is a separate, generic surface for the
/// runtime-computed flavor. `line` is 1-based (the `comment_node`/`sg`/`diag`
/// convention); `detail` is the computed thing's exact source text, verbatim.
/// `reason` is a closed v1 vocabulary, each bucket re-derived from an AST
/// shape another pass in this codebase already visits for a different
/// purpose: `dynamic-import` (`import(expr)` / `require(expr)` whose argument
/// isn't a plain string literal), `computed-member-call` (`obj[key]()` — the
/// call-site walk already sees this callee shape and silently drops it),
/// `spread-call-args` (`f(...args)` — the dataflow arg walk already sees a
/// spread argument and silently drops it). TS/TSX/JS/JSX/MJS/CJS only in v1;
/// Python star-imports and `sys.path` mutation stay out (already surfaced via
/// `module_unresolved` / a loud eprintln respectively) to avoid a
/// cross-family digest dependency — see `typegraph::UnresolvedRef`.
pub(crate) const UNRESOLVED_RELS: [&str; 1] = ["unresolved"];

// The git-derived families `changed` / `changed_line` / `created`, the analysis
// families `agent` / `dl_diag` / `type_shape` / `type_lgg` / catalog, the SCIP
// importer `scip_*`, the clone proposers `propose_extract` / `propose_clone`,
// and the embedding `similar` now live behind `trait RelKind` in the `rels`
// module dir (decls + gate + refresh per family, one registry the
// tick/declare/guard sites loop over).

/// Ref-spine query relations: thin views over the `_strings` / `_where_bytes`
/// meta tables. `string(id, text, norm)` resolves an interned StringId to its
/// content; `ref(id, string, file, lo, hi)` locates each interned string's byte
/// span, `id` being the `_where_bytes` id (the rewrite coordinate an `edit` keys
/// off). Join them to ask "where does <text> occur": `string(s, "Foo", _),
/// ref(_, s, f, lo, hi)`. Populated for regex/ast/sg captures and import refs.
pub(crate) const SPINE_RELS: [&str; 2] = ["string", "ref"];

/// CST-as-relation (christmas #3): every NAMED tree-sitter node of every scanned
/// file as a row. `node(id, kind, file, lo, hi, parent)` — `id`/`parent` are
/// kind-salted `_where_bytes` ids (so `ref(id, sid, _, lo, hi)` ->
/// `string(sid, text, _)` recovers each node's source bytes); `file` is the
/// content FileId, `kind` the tree-sitter node kind, `[lo, hi)` the byte span.
/// `child(parent, child)` is the 2-col edge so `anc(a,b) <- closure(child).`
/// gives ancestor/descendant with the engine's existing recursion. Populated by
/// `refresh_node_rels` over the whole tree (no query) when the rels are used.
const NODE_RELS: [&str; 2] = ["node", "child"];

/// Daemon-state query relations: thin views over the persisted `_program` /
/// `_ref` / `_rev_log` meta tables, so a dashboard can ask the warm engine what
/// it loaded and which watched refs have moved. `program(path, hash, mtime)` is
/// the loaded `.dl` file set; `head(repo, name, oid)` is the last-seen oid of
/// every watched ref (HEAD plus each program-scanned rev); `rev_advanced(repo,
/// name, old, new)` is the advance log the daemon appends when a watched ref
/// moves. Populated by `refresh_daemon_rels`; the daemon writes the underlying
/// tables via `save_program_meta` / `save_repos_meta` / `observe_ref`.
const DAEMON_RELS: [&str; 3] = ["program", "head", "rev_advanced"];

/// The clock relation. `every(secs)` is an engine-populated source rel that holds
/// the interval `N` only on the tick that crosses an `N`-second boundary (and on
/// the first tick), so a body atom `every(30)` self-throttles the rule that joins
/// it. Edge-triggered off wall-clock seconds, bucket-per-N stored in `_carry_meta`
/// (`every:N`), so the cadence is exact regardless of how often the daemon ticks.
const EVERY_RELS: [&str; 1] = ["every"];

/// The persistent clock relation. `clock(secs, bucket)` holds, on EVERY tick, the
/// current bucket `now / secs` for each `secs` period the program names — a
/// monotone integer that advances once per `secs` wall-clock seconds. Unlike the
/// edge-triggered `every` (present only on the boundary tick), `clock` is always
/// present, so a body atom `clock(300, b)` binds `b` to the live bucket and varies
/// any join — or an `@async` request digest — exactly once per period. That is the
/// dl-native cadence primitive: time as a fact you join against, no `@next`
/// counter. Reuses `now_secs`; lazy per `clock_rels_used`.
const CLOCK_RELS: [&str; 1] = ["clock"];

/// The effect-drain audit view: a thin query rel over `pending_effect`, the job
/// table @async/@stream requests land in. One row per distinct request (digest
/// `id`), carrying its template `kind`, the `head` rel it rebuilds, the job
/// `state` (queued|running|done|failed), the request `args` JSON (the hole map —
/// the call's parameters, the endpoint analog), and `req_tx` (the tx it was
/// queued at). This is the dl-native call log: `? effect_log(...)` shows the
/// drain queue live, and it doubles as the parity surface against ghcacher's
/// `call_log`. Lazy like every other built-in group; a program that never reads
/// it pays nothing (`pending_effect` is still written, just not projected).
const EFFECT_RELS: [&str; 1] = ["effect_log"];

/// The diagnostic sink. Unlike every other built-in, `diag` is engine-declared
/// but USER-WRITTEN: a rule heads it to emit an editor squiggle (`--lsp`), a
/// check finding (`--check` exit code), or a daemon-hook message. Fixed 9-col
/// schema (was a magic user-declared name whose columns the engine mapped by
/// NAME — the merged `.dl/` namespace collided when two files declared it with
/// different columns). Write only the columns you need via named args
/// (`diag(path: p, line: l, msg: m) <- ...`); the rest lower to NULL and take
/// defaults in `Engine::diags` (severity "warn", end_line = line, ints 0). Read
/// only, never populated by a refresh — `rebuild_derived` fills it from the
/// program's rules like any other derived rel.
const DIAG_RELS: [&str; 1] = ["diag"];

/// The hover-note sink. Same shape as `diag` (engine-declared, USER-WRITTEN): a
/// rule heads `hover_note(path, line, col, end_line, end_col, md)` to attach
/// markdown to a source span; the LSP hover path appends each matching row's
/// `md` to the hover it synthesizes at that position. Positions are 0-based,
/// the same convention as `diag`. Fixed 6-col schema. Read only, never
/// populated by a refresh — `rebuild_derived` fills it from the program's
/// rules like any other derived rel; a program that never heads it leaves the
/// table empty (or undeclared, tolerated by `Engine::hover_notes_at`).
const HOVER_RELS: [&str; 1] = ["hover_note"];

/// The drawable-graph SINK relations. A user HEADS these from a rule (like
/// `diag`) to emit a graph the flow panel draws with ZERO bespoke SQL:
/// `graph_node(id, label, kind, file, line, parent)` is one vertex,
/// `graph_edge(src, dst, kind)` one edge. Fixed schema so any program's graph
/// composes into the same two tables the panel's always-available "Graph"
/// preset reads (`rel_graph_node` / `rel_graph_edge`). Pre-declared (catalogued,
/// so the binding shows in `rel_catalog`) and reserved against a `rel`
/// re-declaration — head them directly, name only the columns you use (the rest
/// lower to NULL: no file/line/parent = an unplaced, unnested node). Read only,
/// never populated by a refresh — `rebuild_derived` fills them from the
/// program's rules like any other derived rel, so an unheaded program leaves
/// them empty (and the preset shows the "nothing to draw" hint).
const GRAPH_RELS: [&str; 2] = ["graph_node", "graph_edge"];

/// The harness-hook event log. `hook_event(kind, session, seq, json)` accumulates
/// one row per coding-agent hook invocation (`dl --hook`): kind = the harness
/// event name (UserPromptSubmit / PostToolUse / ...), session = the event's
/// session id, seq = an ingest-time monotone millis stamp (orders events within a
/// session), json = the raw event JSON. Rows are written out-of-tick by the
/// `hook_event` RPC / the in-process feed, never by a refresh; a program extracts
/// fields with the term-form `json`/`jsonp` predicates, mirroring how
/// `mcp_request` carries raw JSON. Lazy per `hook_rels_used`.
const HOOK_RELS: [&str; 1] = ["hook_event"];

/// The diagnostic-mute set. `diag_mute(code)` holds one row per diagnostic code
/// the editor session has silenced. Engine-owned and WRITABLE, but only through
/// `toggle_diag_mute` (the LSP `dl.toggleDiagCode` command), never a rule head —
/// so it mirrors `hook_event`'s out-of-tick write shape, not `diag`'s
/// rule-headed one. Rows persist in the db, so a mute survives a daemon restart.
/// Read at the LSP publish seam to drop muted `diag` rows before they reach the
/// editor; `--check` / `--parse-only` read `diag` directly and are UNAFFECTED
/// (mute is an editor affordance, not a CI gate — see the lsp.rs module doc).
const MUTE_RELS: [&str; 1] = ["diag_mute"];

/// The demand / overlay SINK relations. A user HEADS these from a rule (like
/// `diag` / `repo`), and the rows drive engine behavior the name is bound to:
/// `scip_want` → SCIP index demand, `rev_cmp_want` → git ancestry demand,
/// `def_target` → LSP go-to-definition, `effect_cmd` → per-kind effect-template
/// overlay, `checkout` → git checkout sweep (clone-missing + fetch +
/// fast-forward the default branch to origin, the ghcacher keep-current half).
/// Pre-declared builtins (so the binding shows in `rel_catalog` /
/// `dl docs relations`) and reserved against a `rel` re-declaration — head them
/// directly, do not `rel`-declare them, exactly like `diag`. This is what makes
/// them first-class instead of magic: the engine reading them by name is reading
/// a catalogued builtin, not an undocumented convention. See docs/reference/
/// magic-rels.md and the `.dl/magic-rel-audit.dl` rail.
const DEMAND_RELS: [&str; 5] = [
    "scip_want",
    "rev_cmp_want",
    "def_target",
    "effect_cmd",
    "checkout",
];

/// The derived-shape SINK relation. A user HEADS `type_decl_row(shape, pos, col,
/// ty)` from a rule (like `diag` / `graph_node`) to DERIVE a relation schema from
/// data — column names + base types computed by rules rather than written by
/// hand. The engine consumes it across a one-tick phase delay: at the end of a
/// tick its rows persist to the `_shapes` meta table; on the NEXT tick's declare,
/// a `rel name: shape.` decl whose shape has no syntax `type name(...)` decl
/// resolves its columns from the persisted rows (a `shape-pending` info diag until
/// then). Syntax shapes win on a name clash (`shape-shadowed` warn). Pre-declared
/// (catalogued, group "types") and reserved against a `rel` re-declaration — head
/// it directly, like diag. Derived-only: it must be filled by a derived rule (a
/// term-extract rule feeding it must route through its own rel first, the repo
/// mixed-kind law).
const TYPE_DECL_RELS: [&str; 1] = ["type_decl_row"];

/// The trailing identifier of a SCIP symbol descriptor: `... Foo#` -> "Foo",
/// `... bar().` -> "bar". Used to key the SCIP override by plain type name.
pub(crate) fn scip_descriptor_name(symbol: &str) -> Option<String> {
    let bytes = symbol.as_bytes();
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    // the last maximal identifier run in the symbol string
    let mut last: Option<(usize, usize)> = None;
    let mut run_start: Option<usize> = None;
    for (idx, &b) in bytes.iter().enumerate() {
        if is_ident(b) {
            run_start.get_or_insert(idx);
        } else if let Some(s) = run_start.take() {
            last = Some((s, idx));
        }
    }
    if let Some(s) = run_start.take() {
        last = Some((s, bytes.len()));
    }
    last.map(|(s, e)| symbol[s..e].to_string())
}

fn module_manifest_path(path: &str) -> bool {
    path.ends_with("Cargo.toml")
        || path.ends_with("package.json")
        || path.ends_with("tsconfig.json")
}

/// Parse a 64-char hex string into 32 bytes. Errs on wrong length or non-hex
/// (e.g. the `''` __src default on a derived row), so the caller can skip it.
fn hex_to_32(s: &str) -> Result<[u8; 32]> {
    let b = s.as_bytes();
    if b.len() != 64 {
        bail!("not a 32-byte hex digest");
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)?;
    }
    Ok(out)
}

/// One row of the `diag` relation, normalized for the LSP. Columns are mapped
/// by NAME from the `.dl` author's `rel diag(...)` decl (order-free); only
/// path/line/msg are required, the span/severity fields default. See docs/lsp.md.
#[derive(Clone, Debug)]
pub struct DiagRow {
    pub path: String,
    pub line: i64,
    pub col: i64,
    pub end_line: i64,
    pub end_col: i64,
    pub severity: String,
    pub code: String,
    pub msg: String,
    pub hint: Option<String>,
}

/// One `?` query result, captured for the daemon RPC `query` path. Same shape
/// as `--query-json` per-row objects; the foreground path prints via `run_query`
/// instead.
#[derive(Clone, Debug)]
pub struct QueryResult {
    pub rel: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
}

/// One located reference for the `refs_lens` navigation surface (Track B). A hit
/// carries its OWN repo so the LSP can map the slug back to that repo's on-disk
/// root (the multi-repo `root.join` fix), plus a 0-based line/col range matching
/// what `resolve_span` produces. `role` labels the edge (declaration kind, `call`,
/// an import, a `type_link` kind, `caller`/`callee`, or `text`); `container` names
/// the enclosing symbol when known.
#[derive(Clone, Debug, serde::Serialize)]
pub struct RefHit {
    pub repo: String,
    pub path: String,
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub role: String,
    pub container: String,
}

/// The grouped references result for one cursor position, produced by
/// `Engine::refs_lens`. `tier` is the resolution grade (`resolved` = joined
/// through the type/call graph by name, `textual` = the ref-spine same-string
/// fallback). `symbol` is the preferred definition symbol (same-repo-then-same-
/// file wins when a name maps to several); `display_name` is the bare identifier
/// under the cursor.
#[derive(Clone, Debug, serde::Serialize)]
pub struct RefLens {
    pub tier: String,
    pub symbol: String,
    pub display_name: String,
    pub declarations: Vec<RefHit>,
    pub uses: Vec<RefHit>,
    pub containing_types: Vec<RefHit>,
    pub callers: Vec<RefHit>,
    pub callees: Vec<RefHit>,
}

/// One point-lookup hit for the "follow the user" navigation surface
/// (`Engine::locate`, Track B B4). Cheap by construction: this is a single
/// cursor -> symbol -> declaration-site resolution, never a uses/callers
/// collection or a closure walk — the panel calls it on every cursor move, so
/// it stays a point query same as `resolve_sym_hit`. `tier` mirrors
/// `RefLens.tier` minus "textual" (a grep-grade hit would center the graph on
/// nothing, so follow mode never falls that far). `role` is the edge/occurrence
/// role at the declaration site (a SCIP role for tier "compiler", the
/// type_entity/call_def kind for tier "resolved").
#[derive(Clone, Debug, serde::Serialize)]
pub struct LocateHit {
    pub tier: String,
    pub symbol: String,
    pub display_name: String,
    pub role: String,
    pub repo: String,
    pub file: String,
    pub line: u32,
}

/// One declared symbol for the nearly-free LSP surfaces (`workspace/symbol` and
/// `textDocument/documentSymbol`). Carries its OWN repo so the LSP maps the slug
/// back to that repo's on-disk root, `line` is 1-based as stored in the rels, and
/// `sym`/`parent` are the `file::kind::name` cross-graph keys the document-symbol
/// handler nests by. `container` names the enclosing symbol for the flat
/// workspace list.
#[derive(Clone, Debug)]
pub struct SymbolRow {
    pub repo: String,
    pub sym: String,
    pub name: String,
    pub kind: String,
    pub parent: String,
    pub file: String,
    pub line: i64,
    pub container: String,
}

/// One resolvable node for the call-hierarchy / type-hierarchy LSP surfaces
/// (Track B B5, `textDocument/prepareCallHierarchy` +
/// `textDocument/prepareTypeHierarchy` and their incoming/outgoing/super/sub
/// twins). Reuses the same two-tier resolution ladder as `locate`/`refs_lens`:
/// `sym` is the resolved-tier join key (`call_def.sym` / `type_entity.sym`),
/// `scip_symbol` is the compiler-tier SCIP moniker — exactly one of the two is
/// non-empty, mirroring `tier`. Each item carries its OWN repo (the multi-repo
/// URI fix `refhit_location`/`workspace_symbols` already use). `line`/
/// `end_line` are 0-based (unlike `SymbolRow`, which keeps the 1-based rel
/// convention) so the LSP handler can build a `Range` with no further
/// arithmetic — this struct exists purely to cross the engine/LSP boundary.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct HierarchyItem {
    pub tier: String,
    pub sym: String,
    pub scip_symbol: String,
    pub name: String,
    pub kind: String,
    pub repo: String,
    pub file: String,
    pub line: u32,
    pub end_line: u32,
}

/// One 1-hop call-hierarchy neighbor: the neighboring `HierarchyItem` plus the
/// call-site line(s) inside the CALLER (`from_ranges` in the LSP spec is
/// always relative to the caller, for both `incomingCalls` and
/// `outgoingCalls`). 0-based, matching `HierarchyItem.line`.
#[derive(Clone, Debug, serde::Serialize)]
pub struct HierarchyCallEdge {
    pub item: HierarchyItem,
    pub from_lines: Vec<u32>,
}

/// Carry set for `refresh_spine_rels_delta`. Accumulates the new rows produced
/// during a single tick so the incremental Some() path can replay only those rows
/// rather than projecting the full `_strings` / `_where_bytes` tables.
///
/// Incremental-load lever: the wholesale `_strings` / `_where_bytes` read in
/// `refresh_spine_rels_delta(None)` is correct but scales with total interned
/// strings, not per-tick delta. The staged per-tick vecs in
/// `insert_spine_where_bytes` are the future `Some()` source: collect the new
/// StringIds and WhereBytes there, pass them here, then flush one
/// `insert_rows` call per table (collect-then-flush, never per-row). The
/// `retracted_paths` list drives the corresponding delete from `string` / `ref`
/// before the new rows land.
pub struct SpineDelta {
    pub strings_added: Vec<spine::StringId>,
    pub spans_added: Vec<spine::WhereBytes>,
    pub retracted_paths: Vec<(spine::RepoId, String)>,
}

/// head relation -> edge relation, for every `head(..) <- closure(edge).` rule.

type Bind = HashMap<String, Value>;
/// (repo slug, path, rev) -> (content hash, mtime secs, size bytes, line count).
/// The repo slug is the third coordinate so two repos sharing a path do not
/// collide. `line count` is -1 when unknown (a git rev, or an old row from
/// before this column existed) — `file_lines` filters those out.
type FileMeta = HashMap<(String, String, String), (String, i64, i64, i64)>;

struct Reconcile {
    changed: bool,
    extracted: usize,
    retracted: usize,
    parsed: usize,
    total: usize,
}

#[derive(Default)]
struct ModuleRows {
    imports: Vec<Vec<Value>>,
    edges_rev: Vec<Vec<Value>>,
    unresolved_rev: Vec<Vec<Value>>,
    crate_edges: Vec<Vec<Value>>,
    // Ref-spine: (path, leaf text, located bytes) of each import's rewrite
    // coordinate. The text interns into `_strings` and the span into
    // `_where_bytes` (both flushed once in `insert_module_rows`), so
    // `ref(id,string,file,lo,hi)` ⋈ `string` covers the import graph, not just
    // regex/ast/sg captures. Collect-then-flush, never N+1.
    spans: Vec<(String, String, spine::WhereBytes)>,
    // module_binding_resolved_rev(file, local, source, dst, rev) rows: each aliased
    // import binding this specifier ref carries (see `ModuleRef::bindings`).
    bindings: Vec<Vec<Value>>,
    // module_binding_rev(file, local_name, source_module, imported_name, kind,
    // rev) rows: EVERY local binding this specifier ref carries, for every
    // resolution kind (see `ModuleRef::module_bindings`).
    module_bindings: Vec<Vec<Value>>,
}

impl ModuleRows {
    fn extend(&mut self, other: ModuleRows) {
        self.imports.extend(other.imports);
        self.edges_rev.extend(other.edges_rev);
        self.unresolved_rev.extend(other.unresolved_rev);
        self.crate_edges.extend(other.crate_edges);
        self.spans.extend(other.spans);
        self.bindings.extend(other.bindings);
        self.module_bindings.extend(other.module_bindings);
    }
}

/// In-memory condensation for a closure edge relation, held for the tick's query
/// phase. A src-pinned `reaches` query becomes a seeded BFS (microseconds)
/// instead of materializing the recursive-CTE view's whole component closure.
struct ClosureCache {
    cond: scc::Cond,
    names: Vec<String>,       // node id -> name
    id: HashMap<String, u32>, // name -> node id
    digest: [u8; 32],         // content digest of the edge relation's (c0,c1) rows
}

fn mtime_secs(md: &std::fs::Metadata) -> i64 {
    md.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Wall-clock seconds since the Unix epoch, for the daemon-state meta tables'
/// `*_at` columns. Engine code may use std::time freely.
fn unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Days-since-epoch (1970-01-01) -> (year, month, day). Howard Hinnant's
/// `civil_from_days` (public-domain, http://howardhinnant.github.io/date_algorithms.html),
/// hand-rolled so `_query_log.ts` needs no chrono/time dependency.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// ISO-8601 UTC "now", nanosecond fraction included so two requests logged in
/// the same wall-clock second (or even microsecond) still sort and compare
/// distinctly — the `_query_log` row has no primary key, so distinctness lives
/// in the timestamp, not a dedup key.
fn iso8601_utc_now() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs() as i64;
    let nanos = dur.subsec_nanos();
    let days = secs.div_euclid(86400);
    let sod = secs.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    let hh = sod / 3600;
    let mm = (sod % 3600) / 60;
    let ss = sod % 60;
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}.{nanos:09}Z")
}

pub struct Engine {
    pub(crate) db: crate::db::Db,
    pub(crate) rels: Rels,
    pub(crate) root: PathBuf,
    /// True only when `root` is a placeholder, not an explicit workspace —
    /// i.e. the rootless daemon, whose `self.root` is the XDG state dir. Then a
    /// self-form scan (`.`/`""`/`self`/`WORK`) and a gen write fall back to each
    /// rule's own `.git` ancestor, so a script loaded into the daemon scans and
    /// writes the repo it lives in. False for foreground (`--root`/cwd) and LSP
    /// (`rootUri`): an explicit root always wins, and the script's location is
    /// ignored. (The LSP sandbox + `--root` override tests pin this.)
    root_implicit: bool,
    pub dropped: usize,
    /// Diag-shaped rows for the extraction type-drops counted in `dropped`, one
    /// per (file, head relation) that lost rows this tick. The stderr counter
    /// still fires; these additionally surface a file-level squiggle over LSP so
    /// an editor shows a file whose rows were dropped. Span is unknown at a row
    /// type-failure, so each lands at file-level line 1 (spec T3). Collected, then
    /// read once after the tick; never a per-row publish.
    extraction_drops: Vec<DiagRow>,
    /// Structural diagnostics from the derived-shape resolver (Phase 5):
    /// shape-pending / shape-shadowed / shape-unknown-type. Produced at declare +
    /// end-of-tick persist, cleared at tick start, and appended by `diags()` so
    /// --check and --lsp both surface them (they are not rule-derived `diag`
    /// rows). All non-error severity, so the error gate stays green.
    shape_diags: Vec<DiagRow>,
    /// Test/bench instrumentation: cumulative count of edge condensations
    /// actually rebuilt (Tarjan invocations). A reused cond does not bump it, so
    /// a bench can assert "this edit recondensed 0 graphs".
    pub recondensed: usize,
    /// Test/bench instrumentation: cumulative count of node2vec graphs actually
    /// re-embedded (the W1 digest-skip leaves it unchanged when the edge set's
    /// digest matched). A tick over an unchanged graph must not bump it.
    pub node2vec_recomputed: usize,
    /// Movable-ref resolutions (a branch, HEAD, a repointable tag), cleared each
    /// tick so they re-resolve as the ref advances.
    rev_cache: HashMap<String, String>,
    /// Immutable-rev resolutions (a full/prefix hex SHA — its object mapping
    /// can't change), kept ACROSS ticks so a pinned rev spawns git exactly once
    /// for the daemon's lifetime instead of once per tick.
    rev_sha_cache: HashMap<String, String>,
    /// (repo slug, rev, path) of every tracked source file this tick — the
    /// existence oracle for `:file`/`:path`/`:dir` type checks against off-disk
    /// revs (where the filesystem cannot answer).
    rev_index: std::collections::HashSet<(String, String, String)>,
    /// Repos registered via the turnkey config. When non-empty the `repo`
    /// relation lists these instead of the single `--root`. Reloaded by the
    /// watcher when the config file changes. (File ingestion from the extra
    /// roots is the next step; today only `--root` is scanned into `_file`.)
    repos: Vec<crate::config::RepoConfig>,
    /// Per-edge SCC condensation, kept ACROSS ticks. The query phase reused to
    /// rebuild every edge's condensation on every tick (the per-keystroke
    /// closure tax); now an edge is recondensed only when its rows actually
    /// changed (affected this tick AND its content digest moved). An unaffected
    /// edge is reused with zero work; a comment-only edit (rows unchanged) skips
    /// the Tarjan rebuild on the digest check.
    closure_cache: HashMap<String, ClosureCache>,
    /// One graph adjacency load shared by native reach walks during this tick.
    /// Cleared at tick entry so a large graph is never retained across ticks.
    adjacency_cache: std::cell::RefCell<Option<derive::AdjacencyCache>>,
    /// Emit `?` query results as JSON-lines (one object per query) instead of the
    /// human TSV block. For tools/editors consuming answers (`--query-json`).
    query_json: bool,
    /// When true, skip the `?` query-evaluation pass at the end of a tick. Used
    /// for the foreground one-shot's PRIMING tick: a data-driven scan or
    /// repo-sink reads last tick's coordinate/pull state, so a fresh run has
    /// nothing to read on tick 1. The priming tick derives the coordinates (and
    /// pulls repos) silently; the follow-up tick reads them and prints answers.
    prime_tick: bool,
    /// Test/bench instrumentation: the N+1 detector's verdict for the LAST tick
    /// (`db.tick_end()`), so a test can assert no per-row write slipped through
    /// the plural API. `None` = silent (good); `Some((stmt, count))` = a
    /// statement ran past `N1_THRESHOLD`.
    pub last_n1: Option<(String, u32)>,
    /// Test/bench instrumentation: how many files `refresh_node_rels`/
    /// `refresh_node_rels_delta` actually parsed+walked on the LAST node tick.
    /// A delta tick over one edited file sets this to 1; a full cold walk sets
    /// it to the whole corpus. The structural proof the incremental path is
    /// path-scoped (and can't silently regress to full-corpus).
    pub last_node_files_walked: std::cell::Cell<usize>,
    /// Test/bench instrumentation: cumulative count of files the type/call/
    /// dataflow extractors actually parsed (per-file cache misses). A warm
    /// no-change tick must not bump it (the `extract:*` digest skips the whole
    /// pass); an edit bumps it by the changed-file count per family, not the
    /// corpus. The structural proof of perf gap A.
    pub extract_files_parsed: std::cell::Cell<usize>,
    /// Production A/B lever for bundled type/call/dataflow extraction. False
    /// uses one language parse to prime all requested family caches; true
    /// restores the legacy independent family calls. Seeded from
    /// `DL_DISABLE_ANALYSIS_BUNDLE=1`; tests set the cell directly to avoid
    /// racing on process-global environment variables.
    pub force_separate_analysis_extractors: std::cell::Cell<bool>,
    /// Test/bench instrumentation: cumulative count of FULL-input rule
    /// re-executions inside a recursive fixpoint — every statement execution
    /// after the first pass of a naive re-run-to-delta-0 loop (each one
    /// re-derives every row from all previous passes and discards them on PK
    /// conflict; the exact waste semi-naive evaluation removes). Semi-naive
    /// components never bump it (seed rules run once; iteration statements
    /// read `_delta_*` snapshots, not the full input). Only the naive
    /// fallback shapes (aggregate/`key(...)` heads inside a recursive
    /// component, or `DL_NAIVE_FIXPOINT=1`) count. The structural proof of
    /// the semi-naive rewrite, in the `extract_files_parsed` idiom.
    pub fixpoint_full_reruns: std::cell::Cell<usize>,
    /// Force every recursive component onto the naive re-run-to-delta-0 loop
    /// — the A/B lever for the `fixpoint_full_reruns` counter tests and field
    /// bisection. Seeded from `DL_NAIVE_FIXPOINT=1` at construction; tests
    /// set the field directly (no process-global env mutation, which would
    /// race parallel tests).
    pub force_naive_fixpoint: std::cell::Cell<bool>,
    /// Test/bench instrumentation: the pre-stratum derived rels the LAST tick
    /// (full or incremental) actually rebuilt. A scoped tick (perf gap B) lists
    /// only the rels dependency-reachable from what changed; a full rebuild
    /// lists every pre-stratum rel; a no-change/comment-only tick leaves it
    /// empty. The structural proof the tick's rebuild is affected-scoped, not a
    /// full re-derivation on every edit.
    pub last_derived_rebuilt: Vec<String>,
    /// Verify-rollback journal (christmas #14). `None` = not in verify mode (gen
    /// writes go straight to disk, no capture). `Some(...)` = every gen write
    /// first stashes the target's original bytes (`None` entry = the file did not
    /// exist) so `rollback_writes` can restore the tree if a checker fails. One
    /// entry per path, first-write wins (the pre-tick state).
    gen_journal: std::cell::RefCell<Option<Vec<(String, Option<Vec<u8>>)>>>,
    /// Per-file extracted-fact caches for the type/call/dataflow refreshers,
    /// keyed by (repo, path, content hash) — a warm tick re-parses only files
    /// whose content address moved. Each refresh replaces the map with exactly
    /// the current file set's entries, so dead content evicts itself and the
    /// size stays bounded by the corpus. In-memory only (the daemon's warm
    /// ticks are the measured wall); a fresh process parses once, then the
    /// persisted `extract:*` input digest skips the whole pass while nothing
    /// moves. The cached value carries the file's derived repo id alongside
    /// the facts (both are (path, content) functions).
    type_facts_cache: extract::FactCache<crate::typegraph::TypeFacts>,
    call_facts_cache: extract::FactCache<crate::typegraph::CallFacts>,
    df_facts_cache: extract::FactCache<crate::typegraph::DataflowFacts>,
    /// Per-file comment cache for `refresh_comment_rels`, same shape as the
    /// type/call/df caches: (repo, path, content hash) -> (repo id, comments).
    comment_facts_cache: extract::FactCache<Vec<crate::cst::RawComment>>,
    /// Per-file template-parts cache for `refresh_template_rels`, same shape:
    /// (repo, path, content hash) -> (repo id, ordered template pieces).
    template_facts_cache: extract::FactCache<Vec<crate::typegraph::TemplatePart>>,
    /// Per-file unresolved-marker cache for `refresh_unresolved_rel`, same
    /// shape: (repo, path, content hash) -> (repo id, markers).
    unresolved_facts_cache: extract::FactCache<Vec<crate::typegraph::UnresolvedRef>>,
}

struct ScanSpec {
    repo: Term,
    rev: Term,
    glob: Term,
    path_var: String,
    /// `None` when rev_out is `_` or omitted: the scanned rev is not bound.
    rev_out_var: Option<String>,
}

/// One parsed file's CST node records plus the repo id + path + content
/// `FileId` its spans key off. Produced by `Engine::node_walk`, consumed by
/// `Engine::node_rows_from_walk` (the full and delta CST refresh share both).
struct FileNodes {
    repo: String,
    path: String,
    file: spine::FileId,
    content: String,
    nodes: Vec<crate::cst::CstNode>,
}

impl Engine {
    pub fn new(db: crate::db::Db, root: PathBuf) -> Self {
        crate::perflog::set_root(&root);
        Engine {
            db,
            rels: HashMap::new(),
            root,
            dropped: 0,
            extraction_drops: Vec::new(),
            shape_diags: Vec::new(),
            recondensed: 0,
            node2vec_recomputed: 0,
            closure_cache: HashMap::new(),
            adjacency_cache: std::cell::RefCell::new(None),
            rev_cache: HashMap::new(),
            rev_sha_cache: HashMap::new(),
            rev_index: std::collections::HashSet::new(),
            repos: Vec::new(),
            query_json: false,
            prime_tick: false,
            root_implicit: false,
            last_n1: None,
            last_node_files_walked: std::cell::Cell::new(0),
            extract_files_parsed: std::cell::Cell::new(0),
            force_separate_analysis_extractors: std::cell::Cell::new(
                std::env::var("DL_DISABLE_ANALYSIS_BUNDLE").ok().as_deref() == Some("1"),
            ),
            fixpoint_full_reruns: std::cell::Cell::new(0),
            force_naive_fixpoint: std::cell::Cell::new(
                std::env::var("DL_NAIVE_FIXPOINT").ok().as_deref() == Some("1"),
            ),
            last_derived_rebuilt: Vec::new(),
            gen_journal: std::cell::RefCell::new(None),
            type_facts_cache: Default::default(),
            call_facts_cache: Default::default(),
            df_facts_cache: Default::default(),
            comment_facts_cache: Default::default(),
            template_facts_cache: Default::default(),
            unresolved_facts_cache: Default::default(),
        }
    }

    /// Emit query results as JSON-lines instead of the human TSV block.
    pub fn set_query_json(&mut self, on: bool) {
        self.query_json = on;
    }

    /// Skip `?` evaluation on the next tick (the foreground priming pass).
    pub fn set_prime_tick(&mut self, on: bool) {
        self.prime_tick = on;
    }

    /// Mark `root` as a placeholder (rootless daemon). Self-form scans and gen
    /// writes then fall back to each rule's `.git` ancestor. See `root_implicit`.
    pub fn set_root_implicit(&mut self, on: bool) {
        self.root_implicit = on;
    }

    /// Set the configured repos (from `SprfConfig`). Takes effect on the next
    /// tick via `refresh_builtin_rels`.
    pub fn set_repos(&mut self, repos: Vec<crate::config::RepoConfig>) {
        self.repos = repos;
    }

    /// This engine's root directory (`--root`). The working dir an `@async`
    /// shell effect runs in, so `git`/`gh` commands resolve against the repo.
    pub fn root(&self) -> PathBuf {
        self.root.clone()
    }

    /// Stable slug for this engine's own repo: the `--root` directory name.
    pub(crate) fn self_slug(&self) -> String {
        self.root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| self.root.to_string_lossy().to_string())
    }

    /// slug -> on-disk root for every nameable repo (self + config). The lazy
    /// indexers (type / call / doc) read each file's content from its OWN repo
    /// root via this map, not the single `self.root`, so `type_entity`, the call
    /// graph, and `doc_node` populate for every folder in view — not just
    /// `--root`. `_file.repo` is the key; an unknown repo falls back to root.
    pub(crate) fn repo_roots(&self) -> HashMap<String, PathBuf> {
        let mut m = HashMap::new();
        m.insert(self.self_slug(), self.root.clone());
        for rc in &self.repos {
            m.insert(rc.slug.clone(), rc.root.clone());
        }
        m
    }

    pub fn run(&mut self, prog: &Program) -> Result<()> {
        self.tick(prog, false)
    }

    /// Wholesale replace one engine-owned relation through the same plural write
    /// seam every built-in module/indexer uses.
    pub(crate) fn encode_rel_rows(
        &self,
        rel: &str,
        cols: &[&str],
        rows: &[Vec<Value>],
    ) -> Result<Vec<Vec<Value>>> {
        let meta = self
            .rels
            .get(rel)
            .ok_or_else(|| anyhow::anyhow!("unknown relation {rel}"))?;
        let positions: Vec<Option<usize>> = cols
            .iter()
            .map(|name| meta.cols.iter().position(|col| col.name == *name))
            .collect();
        let mut sink = spine::SymSink::new();
        let mut encoded = Vec::with_capacity(rows.len());
        for row in rows {
            let mut out = row.clone();
            for (pos, value) in out.iter_mut().enumerate() {
                let Some(meta_pos) = positions.get(pos).and_then(|p| *p) else {
                    continue;
                };
                if !meta.cols[meta_pos].interned() {
                    continue;
                }
                if let Value::Text(text) = value {
                    *value = Value::Int(sink.sym(text).cell());
                }
            }
            encoded.push(out);
        }
        self.db.flush_syms(&mut sink)?;
        Ok(encoded)
    }

    pub(crate) fn insert_rel_rows(
        &self,
        rel: &str,
        cols: &[&str],
        rows: &[Vec<Value>],
    ) -> Result<usize> {
        let encoded = self.encode_rel_rows(rel, cols, rows)?;
        self.db.insert_rows(&tbl(rel), cols, &encoded)
    }

    pub(crate) fn refresh_rel(
        &self,
        rel: &str,
        cols: &[&str],
        rows: &[Vec<Value>],
    ) -> Result<usize> {
        let table = tbl(rel);
        let start = std::time::Instant::now();
        let encoded = self.encode_rel_rows(rel, cols, rows)?;
        // Whole-table reload with index drop/rebuild for large rels (see
        // Db::reload_rel); DELETE + plain insert for small ones.
        let n = self
            .db
            .reload_rel(&table, cols, &encoded)
            .with_context(|| format!("refresh relation {rel}"))?;
        // Per-rel write cost + the table's schema/size stats (indexes, PK, and
        // per-object dbstat bytes), so perf.jsonl carries WHY a write is slow —
        // gated inside emit_profile/rel_stats so a normal run pays nothing.
        if crate::perflog::profile_enabled() {
            let stats = self.db.rel_stats(rel).unwrap_or(serde_json::Value::Null);
            crate::perflog::emit_profile_detail(
                "write",
                rel,
                start.elapsed().as_millis() as u64,
                rows.len() as u64,
                stats,
            );
        }
        Ok(n)
    }
}

/// (repo, rev, glob, pathvar, revvar) of a source rule's `scan`. `repo` is the
/// repo coordinate ("." = self repo); resolve it to a root via
/// `Engine::resolve_repo_root`.
/// The scan atom of a source rule, with its coordinate Terms intact (a `Term::Var`
/// in `repo`/`rev` is a data-driven coordinate — see `Engine::resolve_scan_bindings`).
fn scan_spec_of(rule: &Rule) -> Result<ScanSpec> {
    for item in &rule.body {
        if let BodyItem::Scan {
            repo,
            rev,
            glob,
            path,
            rev_out,
        } = item
        {
            let path_var = match path {
                Term::Var(v) => v.clone(),
                Term::Wild => bail!("scan path output must be a variable, not `_` (a scan with no path is meaningless)"),
                other => bail!("expected scan path variable, got {other:?}"),
            };
            return Ok(ScanSpec {
                repo: repo.clone(),
                rev: rev.clone(),
                glob: glob.clone(),
                path_var,
                rev_out_var: opt_var(rev_out)?,
            });
        }
    }
    bail!("source rule {} missing scan", rule.head.rel)
}

/// One resolved scan coordinate: a (slug, root, rev, glob) plus the variable
/// bindings that produced it (so the rule head can reference the data-driven
/// repo/rev the file was scanned under). Literal scans carry empty `head_binds`.
#[derive(Clone)]
struct ScanBinding {
    slug: String,
    root: PathBuf,
    rev: String,
    glob: String,
    head_binds: Vec<(String, String)>,
}

/// Does a rule's scan carry a variable repo or rev (a data-driven coordinate)?
/// Used by `tick_paths` to defer to the full tick (the binding relation is read
/// at reconcile time, not in the path-scoped loop).
pub fn scan_has_var_coords(rule: &Rule) -> bool {
    rule.body.iter().any(|b| {
        matches!(
            b,
            BodyItem::Scan {
                repo: Term::Var(_),
                ..
            } | BodyItem::Scan {
                rev: Term::Var(_),
                ..
            }
        )
    })
}

pub(crate) fn read_content(root: &Path, rev: &str, path: &str) -> Result<String> {
    if rev == "WORK" {
        Ok(std::fs::read_to_string(root.join(path))?)
    } else {
        git_batch_read(root, rev, path)
    }
}

/// The repo identity a file answers to, derived from the file itself: the
/// basename of its nearest ancestor `.git` directory. This makes repo a
/// property of where a file LIVES, not of an explicit `--root` — the rootless
/// model. `froot` is the on-disk root the file was scanned under, `path` its
/// relative path; when no `.git` is found (a non-git folder in view) we fall
/// back to `slug` (the scan slug, already the folder name).
fn repo_id_of(froot: &Path, path: &str, slug: &str) -> String {
    crate::repo::nearest_git(&froot.join(path))
        .and_then(|g| g.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| slug.to_string())
}

/// Normalize a doc heading name for joining against `type_entity.name`. Strips
/// at most one leading token (an article OR a kind word) and at most one
/// trailing kind word, collapses internal whitespace, lowercases. Empty in,
/// empty out. Used only on the doc side: type names are already clean
/// identifiers, so the symbol side only lowercases.
///
///   "The Engine struct" -> "engine"   (leading article + trailing kind)
///   "A Widget"           -> "widget"  (leading article)
///   "fn do_thing"        -> "do_thing" (leading kind word)
///   "struct Engine"      -> "engine"  (leading kind word)
///   "Engine struct"      -> "engine"  (trailing kind word)
///   "Items"              -> "items"   (single word, untouched)
fn normalize_doc_name(s: &str) -> String {
    const ARTICLES: &[&str] = &["the", "a", "an"];
    const KIND: &[&str] = &[
        "struct",
        "enum",
        "trait",
        "class",
        "interface",
        "const",
        "module",
        "mod",
        "type",
        "item",
        "macro",
        "function",
        "fn",
        "method",
        "alias",
        "def",
    ];
    let words: Vec<&str> = s.split_whitespace().collect();
    if words.is_empty() {
        return String::new();
    }
    let mut start = 0;
    let mut end = words.len();
    // Strip one leading token: an article, or (if no article) a kind word.
    // Both forms appear in practice ("The Engine struct" vs "fn do_thing").
    if end - start > 1 {
        let first = words[start].to_ascii_lowercase();
        if ARTICLES.contains(&first.as_str()) || KIND.contains(&first.as_str()) {
            start += 1;
        }
    }
    // Strip one trailing kind word. Independent of the leading strip so
    // "The Engine struct" loses both the article and the trailing classifier.
    if end - start > 1 {
        let last = words[end - 1].to_ascii_lowercase();
        if KIND.contains(&last.as_str()) {
            end -= 1;
        }
    }
    if start >= end {
        return String::new();
    }
    words[start..end].join(" ").to_ascii_lowercase()
}

/// Pull identifier-shaped tokens out of arbitrary source text. An identifier is
/// `[A-Za-z_][A-Za-z0-9_]*`; everything else is a separator. Order preserved;
/// duplicates NOT removed (the caller dedups via `ref_seen`). Used by the
/// `doc_ref` bridge to find symbol mentions inside a markdown code block.
fn identifiers_in(s: &str) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let start_ok = b.is_ascii_alphabetic() || b == b'_';
        if start_ok {
            let start = i;
            i += 1;
            while i < bytes.len() {
                let c = bytes[i];
                if c.is_ascii_alphanumeric() || c == b'_' {
                    i += 1;
                } else {
                    break;
                }
            }
            out.push(&s[start..i]);
        } else {
            i += 1;
        }
    }
    out
}

/// One long-lived `git cat-file --batch` process per repo root, shared across
/// the whole run. Spawning `git show` per file made committed-rev scans
/// pathological (one fork+exec per blob); the batch protocol answers
/// `rev:path` requests over a single pipe. Requests are serialized per root —
/// the pipe is one stream — but parallel readers across repos don't contend.
fn check_type(
    ty: Type,
    v: &Value,
    repo: &str,
    rev: &str,
    root: &Path,
    rev_index: &HashSet<(String, String, String)>,
) -> bool {
    let p = match v {
        Value::Text(s) => s,
        Value::Int(_) => return ty == Type::Int || ty == Type::Text,
        Value::Null => return true,
    };
    if rev != "WORK" {
        return match ty {
            Type::File | Type::Path => {
                rev_index.contains(&(repo.to_string(), rev.to_string(), p.clone()))
            }
            Type::Dir => rev_index
                .iter()
                .any(|(rp, r, pp)| rp == repo && r == rev && pp.starts_with(&format!("{p}/"))),
            // repo/rev are coordinate values, not filesystem paths: no check here.
            Type::Text | Type::Int | Type::Repo | Type::Rev => true,
        };
    }
    let full = root.join(p);
    match ty {
        Type::File => full.is_file(),
        Type::Dir => full.is_dir(),
        Type::Path => full.exists(),
        Type::Text | Type::Int | Type::Repo | Type::Rev => true,
    }
}

/// Drop duplicate `RefHit`s (same repo/path/range/role), preserving first-seen
/// order. The refs-lens buckets fan out over per-sym queries, so the same
/// location can surface more than once (two syms sharing a caller, a symbol used
/// twice on one line).
fn dedup_hits(hits: &mut Vec<RefHit>) {
    let mut seen: HashSet<(String, String, u32, u32, u32, u32, String)> = HashSet::new();
    hits.retain(|h| {
        seen.insert((
            h.repo.clone(),
            h.path.clone(),
            h.line,
            h.col,
            h.end_line,
            h.end_col,
            h.role.clone(),
        ))
    });
}

/// UTF-8 byte offset -> (0-based line, 0-based char column) in `content`. The
/// 0-based line matches what `resolve_span` -> `span_to_range` produces on the
/// LSP side; a past-end offset clamps to the last position.
fn byte_to_lc0(content: &str, byte: u32) -> (u32, u32) {
    let byte = (byte as usize).min(content.len());
    let mut line = 0u32;
    let mut line_start = 0usize;
    for (i, b) in content.bytes().enumerate() {
        if i >= byte {
            break;
        }
        if b == b'\n' {
            line += 1;
            line_start = i + 1;
        }
    }
    let col = content[line_start..byte].chars().count() as u32;
    (line, col)
}

/// Wrap a user substring as a case-insensitive `LIKE` pattern (`%query%`),
/// escaping the LIKE metacharacters `%`, `_`, and `\` so an identifier that
/// contains `_` (common in symbol names) matches literally under `ESCAPE '\'`.
fn like_contains(query: &str) -> String {
    let mut escaped = String::with_capacity(query.len() + 2);
    escaped.push('%');
    for ch in query.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped.push('%');
    escaped
}

/// Literal identifier tokens a pattern requires (metavars stripped). Used as a
/// cheap prefilter: skip parsing a file that cannot contain a match.
fn pattern_literals(pat: &str) -> Vec<String> {
    static META: OnceLock<Regex> = OnceLock::new();
    static IDENT: OnceLock<Regex> = OnceLock::new();
    let meta = META.get_or_init(|| Regex::new(r"\$+[A-Za-z0-9_]*").unwrap());
    let ident = IDENT.get_or_init(|| Regex::new(r"[A-Za-z_][A-Za-z0-9_]*").unwrap());
    let stripped = meta.replace_all(pat, " ");
    let mut out = Vec::new();
    for m in ident.find_iter(&stripped) {
        let s = m.as_str().to_string();
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(src: &str) -> Program {
        crate::parse::parse(crate::lex::lex(src).unwrap()).unwrap()
    }

    /// One stratum can hold a long acyclic chain (stratify groups by negation
    /// depth, not SCC). rel_components must order it dependencies-first and
    /// mark every link non-recursive — each then runs exactly one pass.
    #[test]
    fn rel_components_orders_acyclic_chain_dependencies_first() {
        let prog = program(
            "rel s(x: text). rel a(x: text). rel b(x: text). rel c(x: text).\n\
             s(\"seed\").\n\
             c(x) <- b(x). a(x) <- s(x). b(x) <- a(x).",
        );
        let rules: Vec<&Rule> = prog
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Rule(r) if !r.body.is_empty() => Some(r),
                _ => None,
            })
            .collect();
        let groups = stratify(&rules).unwrap();
        assert_eq!(groups.len(), 1, "all positive: one stratum");
        let comps = rel_components(&groups[0], &rules);
        assert_eq!(comps.len(), 3);
        assert!(
            comps.iter().all(|(_, recursive)| !recursive),
            "chain is acyclic"
        );
        let order: Vec<&str> = comps
            .iter()
            .map(|(ris, _)| rules[ris[0]].head.rel.as_str())
            .collect();
        assert_eq!(order, ["a", "b", "c"], "dependencies evaluate first");
    }

    /// Self-recursion and mutual recursion both mark their component recursive
    /// (the fixpoint loop), while an independent plain rel in the same stratum
    /// stays single-pass.
    #[test]
    fn rel_components_flags_recursive_components() {
        let prog = program(
            "rel s(x: text). rel e(x: text, y: text). rel t(x: text).\n\
             rel p(x: text). rel q(x: text). rel lone(x: text).\n\
             s(\"seed\"). e(\"seed\", \"z\").\n\
             t(x) <- s(x). t(y) <- t(x), e(x, y).\n\
             p(x) <- q(x). q(x) <- p(x). q(x) <- s(x).\n\
             lone(x) <- s(x).",
        );
        let rules: Vec<&Rule> = prog
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Rule(r) if !r.body.is_empty() => Some(r),
                _ => None,
            })
            .collect();
        let groups = stratify(&rules).unwrap();
        assert_eq!(groups.len(), 1);
        let comps = rel_components(&groups[0], &rules);
        let by_head = |name: &str| {
            comps
                .iter()
                .find(|(ris, _)| ris.iter().any(|&ri| rules[ri].head.rel == name))
                .unwrap_or_else(|| panic!("no component holds {name}"))
        };
        assert!(by_head("t").1, "self-recursive t iterates");
        assert!(by_head("p").1, "mutually recursive p/q iterate");
        assert_eq!(by_head("p").0.len(), 3, "p+q rules share one component");
        assert!(!by_head("lone").1, "independent rel is single-pass");
    }

    #[test]
    fn count_lines_empty_file_is_zero() {
        assert_eq!(count_lines(b""), 0);
    }

    #[test]
    fn count_lines_no_trailing_newline_still_counts_last_line() {
        assert_eq!(count_lines(b"a\nb"), 2);
        assert_eq!(count_lines(b"only one line, no newline"), 1);
    }

    #[test]
    fn count_lines_trailing_newline_does_not_add_a_phantom_line() {
        assert_eq!(count_lines(b"a\nb\n"), 2);
        assert_eq!(count_lines(b"\n"), 1);
    }
}
