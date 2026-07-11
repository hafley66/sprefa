use anyhow::{bail, Result};
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
pub use crate::effect::{async_effect_arity, shell_templates, EffectExec, ShellEffectExec};
use crate::effect::async_bound_vars;

// Built-in graph/CST/spine/daemon extractor methods (bucket E) live in a child
// module to shrink this file; they're still `impl Engine` methods called as
// `self.refresh_*` from the tick orchestrator (engine breakdown Stage 4).
mod extract;
mod gen;
pub(crate) mod query;
mod symbols;
mod lens;
mod rpc;
mod declare;
mod reconcile;
mod repo;
mod meta;
mod derive;
mod lang_tables;
pub(crate) use repo::git_batch_read;
pub(crate) use query::emit_query_json;
mod decls;
pub use decls::{
    all_builtin_decls, builtin_enum_brands, builtin_enum_variants, builtin_rel_names, fn_docs,
    op_docs, undocumented_builtins, undocumented_fns,
};
pub(crate) use decls::*;
pub use lang_tables::ast_langs;
pub(crate) use lang_tables::ts_lang;
#[derive(Clone, Debug)]
struct CheckoutOutcome { action: &'static str, ok: bool, detail: String }
// The mixed source+derived / extract+derived rel desugar: a pure Program ->
// Program rewrite that runs immediately before rule classification in both
// tick entry points (see `tick.rs`). Public so `crate::rels::perf` can map a
// twin rel name back to the one a program declared (D4 telemetry display).
pub mod desugar;
// The reactive tick orchestrator (`tick` / `tick_paths`) lives in a child
// module too; both stay `pub` and reach this module's privates directly
// (engine breakdown Stage 6).
mod tick;
pub use tick::{TickReport, is_timer_rel};

fn scc_node_tbl(edge: &str) -> String { format!("scc_node_{edge}") }
fn scc_edge_tbl(edge: &str) -> String { format!("scc_edge_{edge}") }
/// The per-`@next`-rel carry buffer: the live rel's columns plus a `tx`
/// generation column. Rows staged at `tx = cur+1` surface as the live rel at the
/// start of the next tick. See docs/research-reactive-effectful-datalog.md §8.
fn carry_tbl(rel: &str) -> String { format!("_carry_{rel}") }


/// Current wall-clock time in whole seconds since the epoch, used by the `every`
/// clock. `DL_NOW_SECS` overrides it so tests can advance time deterministically
/// across ticks without sleeping.
fn now_secs() -> i64 {
    if let Ok(v) = std::env::var("DL_NOW_SECS") {
        if let Ok(n) = v.parse::<i64>() { return n; }
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
            if i == j { continue; }
            scored.push((crate::embed::cosine(va, vb), b.as_str()));
        }
        scored.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap_or(std::cmp::Ordering::Equal));
        for (sc, b) in scored.into_iter().take(k) {
            rows.push(vec![
                Value::Text(a.clone()), Value::Text(b.to_string()),
                Value::Int((sc * 1_000_000.0).round() as i64)]);
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
        buf.push_str(a); buf.push('\0'); buf.push_str(b);
        let h = blake3::hash(buf.as_bytes());
        for (x, y) in acc.iter_mut().zip(h.as_bytes()) { *x ^= *y; }
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
pub fn set_cmd_budget(n: u32) { let _ = CMD_BUDGET.set(Some(n)); }

fn cmd_budget() -> Option<u32> {
    *CMD_BUDGET.get_or_init(|| std::env::var("DL_CMD_BUDGET").ok().and_then(|v| v.parse().ok()))
}

/// Per-file size cap for the walker, in bytes. `DL_MAX_FILESIZE` (e.g. 1048576),
/// else no cap (legacy behavior). Files larger than this are skipped before any
/// content read/hash, in both the WORK walk and the git-rev ls-tree listing.
static MAX_FILESIZE: OnceLock<Option<u64>> = OnceLock::new();
fn max_filesize() -> Option<u64> {
    *MAX_FILESIZE.get_or_init(|| std::env::var("DL_MAX_FILESIZE").ok().and_then(|v| v.parse().ok()))
}

/// Slow-tick log threshold in ms. A `tick_paths` slower than this prints a
/// `[tick]` line to stderr (the LSP server log), so live dogfooding catches a
/// perf regression. `DL_TICK_LOG_MS` overrides; default 250ms. 0 logs every tick.
static TICK_LOG_MS: OnceLock<f64> = OnceLock::new();
fn tick_log_ms() -> f64 {
    *TICK_LOG_MS.get_or_init(|| std::env::var("DL_TICK_LOG_MS").ok()
        .and_then(|v| v.parse().ok()).unwrap_or(250.0))
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
    *CLOSURE_QUERY_MAX_EDGES.get_or_init(|| std::env::var("DL_CLOSURE_QUERY_MAX_EDGES").ok()
        .and_then(|v| v.parse().ok()).unwrap_or(20_000))
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
pub fn set_tick_audit(on: bool) { TICK_AUDIT.store(on, std::sync::atomic::Ordering::Relaxed); }

/// Configure the GLOBAL rayon pool from `DL_RAYON_THREADS` (unset = rayon's
/// default, one thread per core). Capping it (e.g. `DL_RAYON_THREADS=4`)
/// bounds the CPU the daemon's extract/hash paths can burn — the lever when the
/// fans spin on a many-core box. Must run before any rayon parallelism (called
/// first thing from `cli::run`). The checkout sink has its OWN narrower pool
/// (`DL_CHECKOUT_WIDTH`), so this caps the extract/hash hot paths.
pub fn init_thread_pool() {
    if let Some(n) = std::env::var("DL_RAYON_THREADS").ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|&n| n > 0)
    {
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .thread_name(|i| format!("dl-{i}"))
            .build_global();
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
pub(crate) const TYPE_RELS: [&str; 7] =
    ["type_edge", "type_edge_rev", "type_entity", "type_entity_rev", "type_sig", "type_link", "type_link_rev"];

/// Phase D diet-SCIP call graph. `call_def` is each callable (sym, kind, file,
/// span); `call_site` is each call occurrence (caller sym, callee text, file,
/// line); `call_edge` is the resolved closure edge; `call_edge_rev` is the
/// rev-aware source of truth (same split as type_edge / type_edge_rev).
/// `call_kind` is the per-fn read/write classification of those call sites,
/// keyed by the bare callee name (execute/query_row/etc.) so a rail can join
/// on `write` only. Symbols are `file::kind::name`, the same shape
/// `type_entity` uses, so the call and type graphs share nodes and a join
/// reaches both.
pub(crate) const CALL_RELS: [&str; 7] = ["call_def", "call_def_rev", "call_site", "call_edge", "call_edge_rev", "call_name", "call_kind"];

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
pub(crate) const DATAFLOW_RELS: [&str; 15] = ["df_node", "df_node_rev", "df_node_repo", "df_node_repo_rev", "df_edge", "loop_over", "allocates", "nest", "df_param", "df_arg", "df_arg_rev", "df_field", "df_field_rev", "df_lit", "df_lit_rev"];

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
const DEMAND_RELS: [&str; 5] = ["scip_want", "rev_cmp_want", "def_target", "effect_cmd", "checkout"];

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
    path.ends_with("Cargo.toml") || path.ends_with("package.json") || path.ends_with("tsconfig.json")
}

/// Parse a 64-char hex string into 32 bytes. Errs on wrong length or non-hex
/// (e.g. the `''` __src default on a derived row), so the caller can skip it.
fn hex_to_32(s: &str) -> Result<[u8; 32]> {
    let b = s.as_bytes();
    if b.len() != 64 { bail!("not a 32-byte hex digest"); }
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
fn closure_map(rules: &[&Rule]) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for r in rules {
        if let Some(edge) = r.closure_edge() { m.insert(r.head.rel.clone(), edge.to_string()); }
    }
    m
}

/// Unique edge relations across all closure heads (one condensation per graph).
/// Unique edge relations across closure heads AND scc heads. `refresh_cond_cache`
/// condenses every edge either operator needs; closure-view rebuild
/// (`first_empty_closure_edge`/`rebuild_closures`) stays closure-only (the
/// scc_node SQL tables exist only for closure edges — scc reads the in-memory
/// cond).
fn cond_edges_for<'a>(closure_edges: &[&'a str], scc_rules: &[&'a Rule]) -> Vec<&'a str> {
    let mut out: Vec<&'a str> = closure_edges.to_vec();
    for r in scc_rules {
        if let Some(e) = r.scc_edge() {
            if !out.contains(&e) { out.push(e); }
        }
    }
    out
}

fn dedup_edges(closures: &HashMap<String, String>) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    for e in closures.values() { if !out.contains(&e.as_str()) { out.push(e.as_str()); } }
    out
}

/// One digest over the whole derived layer (derived rules, closure-seed rules,
/// closure edges). Derived tables rebuild atomically, so a single moved bit
/// forces the rebuild; without this an edited derived rule or ground fact keeps
/// serving rows from a warm db (the derived twin of `source_rule_digests`).
fn derived_program_digest(derived_rules: &[&Rule], seed_rules: &[(&Rule, ClosureSeed)], edges: &[&str]) -> [u8; 32] {
    let mut acc = [0u8; 32];
    let xor = |acc: &mut [u8; 32], s: String| {
        let h = blake3::hash(s.as_bytes());
        for (a, b) in acc.iter_mut().zip(h.as_bytes()) { *a ^= b; }
    };
    for r in derived_rules { xor(&mut acc, format!("{r:?}")); }
    for (r, _) in seed_rules { xor(&mut acc, format!("seed:{r:?}")); }
    for e in edges { xor(&mut acc, format!("edge:{e}")); }
    acc
}

/// The literal a query pins head position `pos` to, via a literal head term.
/// None if that position is a free variable. A pinned src/dst on a closure head
/// seeds the transitive walk (the find-refs / blast-radius point query).
fn pinned_value(q: &Query, pos: usize) -> Option<String> {
    match &q.head.terms[pos] {
        Term::Str(s) => Some(s.clone()),
        _ => None,
    }
}

/// A derived rule that reads a closure head in a *seedable* shape: one endpoint
/// of the 2-ary closure atom is pinned to a literal, the other is free. We answer
/// it as a seeded reachability walk over the condensation (the same BFS the
/// closure point query uses), not by materializing the Theta(V^2) closure.
struct ClosureSeed {
    /// The edge relation the closure head is over (`closures[head]`).
    edge: String,
    /// The pinned literal (the seed node).
    seed: String,
    /// true = src pinned (walk out / callees); false = dst pinned (walk in).
    forward: bool,
    /// Variable name of the free endpoint, as it appears in the closure atom.
    free_var: String,
}

/// Classify a derived rule as closure-seedable. Returns `Some(seed)` iff:
///   - exactly one positive body atom references a closure head, that head is
///     2-ary, one column is pinned (a `Term::Str` there, or a `Term::Var` bound
///     by a body `Cmp { var = "lit", Eq }`), the other column is a free
///     `Term::Var`, and
///   - no other positive body atom joins on the free var (the closure is a leaf:
///     the free var occurs only in the closure atom and the head).
/// Otherwise `None` (caller decides: not-a-closure-read = fine; closure-read but
/// not seedable = a hard error in `check_stratification`).
fn closure_seed_of(rule: &Rule, closures: &HashMap<String, String>) -> Option<ClosureSeed> {
    // The single positive closure atom, if exactly one exists.
    let mut closure_atoms = rule.body.iter().filter_map(|it| match it {
        BodyItem::Pos(a) if closures.contains_key(&a.rel) => Some(a),
        _ => None,
    });
    let atom = closure_atoms.next()?;
    if closure_atoms.next().is_some() { return None; } // >1 closure read: not seedable
    if atom.terms.len() != 2 { return None; }
    let edge = closures.get(&atom.rel)?.clone();

    // An Eq body constraint `v = "lit"` (either operand order) pins var `v`.
    let lit_for = |v: &str| -> Option<String> {
        rule.body.iter().find_map(|it| match it {
            BodyItem::Cmp(c) if c.op == CmpOp::Eq => match (&c.lhs, &c.rhs) {
                (Term::Var(lv), Term::Str(s)) | (Term::Str(s), Term::Var(lv)) if lv == v => Some(s.clone()),
                _ => None,
            },
            _ => None,
        })
    };
    // Resolve each endpoint to either a literal seed or a free var name.
    enum End { Seed(String), Free(String) }
    let classify = |t: &Term| -> Option<End> {
        match t {
            Term::Str(s) => Some(End::Seed(s.clone())),
            Term::Var(v) => Some(match lit_for(v) { Some(s) => End::Seed(s), None => End::Free(v.clone()) }),
            _ => None,
        }
    };
    let (e0, e1) = (classify(&atom.terms[0])?, classify(&atom.terms[1])?);
    let (seed, forward, free_var) = match (e0, e1) {
        (End::Seed(s), End::Free(v)) => (s, true, v),
        (End::Free(v), End::Seed(s)) => (s, false, v),
        _ => return None, // both pinned or both free: not the seedable shape
    };
    // The free var must be a leaf: it may appear only in this closure atom and the
    // head, never in another positive body atom (that would be a real join we
    // cannot answer from the walk alone).
    let other_join = rule.body.iter().any(|it| match it {
        BodyItem::Pos(a) if !std::ptr::eq(a, atom) =>
            a.terms.iter().any(|t| matches!(t, Term::Var(v) if *v == free_var)),
        _ => false,
    });
    if other_join { return None; }
    Some(ClosureSeed { edge, seed, forward, free_var })
}

/// Reject a derived rule body that reads a closure head in a non-seedable shape.
/// Seedable reads (one endpoint pinned to a literal) ARE allowed — they evaluate
/// as a seeded reachability walk after the condensation is built (see
/// `eval_closure_seed_rule`). An unpinned read would require materializing the
/// full closure, which the SCC condensation exists to avoid; keep that out.
fn check_stratification(derived_rules: &[&Rule], closures: &HashMap<String, String>) -> Result<()> {
    for r in derived_rules {
        let reads_closure = r.body.iter().any(|it| matches!(it,
            BodyItem::Pos(a) | BodyItem::Neg(a) if closures.contains_key(&a.rel)));
        if !reads_closure { continue; }
        // Negated closure reads are never seedable.
        let neg_closure = r.body.iter().any(|it| matches!(it,
            BodyItem::Neg(a) if closures.contains_key(&a.rel)));
        if !neg_closure && closure_seed_of(r, closures).is_some() { continue; }
        let name = r.body.iter().find_map(|it| match it {
            BodyItem::Pos(a) | BodyItem::Neg(a) if closures.contains_key(&a.rel) => Some(a.rel.clone()),
            _ => None,
        }).unwrap_or_default();
        bail!("rule '{}' reads closure relation '{}' in its body in an unpinned shape; \
               reading a closure from a rule body is only supported when one endpoint is \
               pinned to a literal (seeded reachability), e.g. \
               `h(b) <- {}(a, b), a = \"X\".`. An unpinned read would materialize the \
               full closure; query '{}' directly instead.", r.head.rel, name, name, name);
    }
    Ok(())
}

/// Split the non-closure derived rules into seeded-closure rules (evaluated by
/// seeded BFS in the query phase) and ordinary derived rules (lowered to SQL),
/// then reject any ordinary derived rule that reads a seeded-closure head: that
/// head is filled only in the query phase, so a tier-0 rule reading it would
/// see it empty. Hoisted out of `tick` and `tick_paths`, which carried
/// identical copies of this split + validation loop (surfaced by the
/// repeated `seed_rules` / `derived_rules` locals).
fn split_seed_and_derived<'a>(
    all_derived: &[&'a Rule],
    closures: &HashMap<String, String>,
) -> Result<(Vec<(&'a Rule, ClosureSeed)>, Vec<&'a Rule>)> {
    let seed_rules: Vec<(&Rule, ClosureSeed)> = all_derived.iter().copied()
        .filter_map(|r| closure_seed_of(r, closures).map(|cs| (r, cs))).collect();
    let derived_rules: Vec<&Rule> = all_derived.iter().copied()
        .filter(|r| closure_seed_of(r, closures).is_none()).collect();
    let seed_heads: HashSet<&str> = seed_rules.iter().map(|(r, _)| r.head.rel.as_str()).collect();
    for r in &derived_rules {
        for it in &r.body {
            if let BodyItem::Pos(a) | BodyItem::Neg(a) = it {
                if seed_heads.contains(a.rel.as_str()) {
                    bail!("relation '{}' is seeded from a closure and cannot feed another \
                           derived rule ('{}') in the same tick; query it directly.",
                          a.rel, r.head.rel);
                }
            }
        }
    }
    Ok((seed_rules, derived_rules))
}

// Auto-index. A derived rule joins relations on a shared variable; that variable
// is the join key. Without an index the join is a nested scan, with one it is a
// lookup:
//
//   calls(c1,c2) <- fndef(c1, p, s, e), callsite(c2, p, l), ...
//                              \_______________/
//                          shared var p  =  the join key (path)
//
//   NO index                          index on the probe column
//   ────────                          ────────────────────────────
//   for each fndef row (F):           for each fndef row (F):
//     scan ALL callsite rows (C)        seek callsite WHERE path = p
//   cost  O(F * C)                     cost  O(F * log C)
//
// On the kernel/ subtree F=16k, C=96k, so the scan version is ~1.5e9 row touches
// (the 30s run). Indexing the path column collapses it to seeks.
//
// Heuristic: index every column a variable reaches across >= 2 body atoms (an
// equality join key), plus a negated atom's correlation column. This is the
// cheap form of Souffle's automatic index selection (Subotic, Jordan, Scholz,
// "Automatic index selection for large-scale datalog", which computes the
// minimal set of composite, ordered indexes via a min-chain-cover / Dilworth on
// the lattice of search orders). One single-column index per join key captures
// most of the win and stays trivial.
fn auto_indexes(rules: &[&Rule], rels: &Rels) -> Vec<(String, String)> {
    let mut occ: HashMap<String, Vec<(String, usize)>> = HashMap::new();
    for r in rules {
        for item in &r.body {
            let atom = match item { BodyItem::Pos(a) | BodyItem::Neg(a) => a, _ => continue };
            for (pos, t) in atom.terms.iter().enumerate() {
                if let Term::Var(v) = t { occ.entry(v.clone()).or_default().push((atom.rel.clone(), pos)); }
            }
        }
    }
    let mut out: BTreeSet<(String, String)> = BTreeSet::new();
    for places in occ.values() {
        if places.len() < 2 { continue; } // a variable in one atom is not a join key
        for (rel, pos) in places {
            if let Some(meta) = rels.get(rel) {
                if *pos < meta.cols.len() { out.insert((rel.clone(), meta.cols[*pos].name.clone())); }
            }
        }
    }
    out.into_iter().collect()
}

/// Derived relations transitively reachable from a set of changed relations in
/// the rule dependency graph. A derived rel is a pure function of its body rels,
/// so a rel whose body never (transitively) touches a changed source CANNOT have
/// changed and need not be rebuilt. Returns the affected derived heads only.
fn affected_derived(derived_rules: &[&Rule], changed: &HashSet<String>) -> HashSet<String> {
    let mut affected: HashSet<String> = changed.clone(); // seed with changed sources
    loop {
        let mut grew = false;
        for r in derived_rules {
            if affected.contains(&r.head.rel) { continue; }
            let touches = r.body.iter().any(|it| match it {
                BodyItem::Pos(a) | BodyItem::Neg(a) => affected.contains(&a.rel),
                _ => false,
            });
            if touches { affected.insert(r.head.rel.clone()); grew = true; }
        }
        if !grew { break; }
    }
    derived_rules.iter().map(|r| r.head.rel.clone())
        .filter(|h| affected.contains(h)).collect()
}

/// The two operator heads (scc + node2vec) fill in the QUERY phase, after the
/// derived fixpoint (`eval_scc_rule` / `eval_node2vec_rule` read an edge relation
/// the fixpoint already materialized). So a derived rule that reads one of those
/// heads cannot be lowered into the same fixpoint — it would read an empty table
/// and silently emit wrong rows. The fix: split the derived layer into a
/// `pre`-stratum (no transitive dependency on an operator head) and a
/// `post`-stratum (depends on one). The pre-stratum runs in the main fixpoint
/// (before the operator evals); the post-stratum runs AFTER the heads fill.
struct DerivedStrata<'a> {
    pre_rules: Vec<&'a Rule>,
    post_rules: Vec<&'a Rule>,
    pre_rels: Vec<String>,
    post_rels: Vec<String>,
}

/// Partition the derived rules/rels around the operator boundary. A rel is in the
/// post-stratum iff it transitively reads an scc/node2vec head (reuses the
/// `affected_derived` dependency walk, seeded with the operator head names).
/// Bails if an operator's own input edge depends on another operator head:
/// chaining one graph operator into another within a single tick is a cycle
/// through the query phase, which this two-stratum split cannot order.
fn partition_derived_strata<'a>(
    derived_rules: &[&'a Rule],
    derived_rels: &[String],
    scc_rules: &[&'a Rule],
    node2vec_rules: &[&'a Rule],
) -> Result<DerivedStrata<'a>> {
    let mut op_heads: HashSet<String> = HashSet::new();
    for r in scc_rules { op_heads.insert(r.head.rel.clone()); }
    for r in node2vec_rules { op_heads.insert(r.head.rel.clone()); }
    let post_heads = affected_derived(derived_rules, &op_heads);
    for r in scc_rules.iter().chain(node2vec_rules.iter()) {
        let edge = r.scc_edge().or_else(|| r.node2vec_edge())
            .expect("operator rule has an scc/node2vec edge");
        if post_heads.contains(edge) {
            bail!("operator rule '{}' reads edge relation '{}', which transitively \
                   depends on another operator head (scc/node2vec); chaining one graph \
                   operator into another within a single tick is not supported — \
                   materialize '{}' in a separate program/tick.", r.head.rel, edge, edge);
        }
    }
    let pre_rules = derived_rules.iter().copied()
        .filter(|r| !post_heads.contains(&r.head.rel)).collect();
    let post_rules = derived_rules.iter().copied()
        .filter(|r| post_heads.contains(&r.head.rel)).collect();
    let pre_rels = derived_rels.iter().filter(|r| !post_heads.contains(*r)).cloned().collect();
    let post_rels = derived_rels.iter().filter(|r| post_heads.contains(*r)).cloned().collect();
    Ok(DerivedStrata { pre_rules, post_rules, pre_rels, post_rels })
}

fn intern_rel(s: &str, id: &mut HashMap<String, u32>, name: &mut Vec<String>) -> u32 {
    if let Some(&i) = id.get(s) { return i; }
    let i = name.len() as u32; id.insert(s.to_string(), i); name.push(s.to_string()); i
}

/// stratum(C) = max over edges C->D of (stratum(D) + 1 if that edge is negative).
/// The condensed graph is a DAG, so this memoized recursion terminates.
fn comp_stratum(c: usize, succ: &[Vec<(u32, u32)>], memo: &mut [u32]) -> u32 {
    if memo[c] != u32::MAX { return memo[c]; }
    let mut s = 0u32;
    for &(d, w) in &succ[c] { s = s.max(comp_stratum(d as usize, succ, memo) + w); }
    memo[c] = s;
    s
}

/// Stratify derived rules: a rule that negates relation R lands in a stratum
/// strictly above every rule defining R, so `!R` reads a finished relation.
/// Returns rule indices grouped by stratum, ascending. Errors if a negation
/// sits inside a recursive cycle (unstratifiable; positive recursion is fine).
fn stratify(rules: &[&Rule]) -> Result<Vec<Vec<usize>>> {
    let mut id: HashMap<String, u32> = HashMap::new();
    let mut name: Vec<String> = Vec::new();
    // (head, body, stratum-forcing). A negation OR an aggregation over `body` forces
    // the head into a strictly higher stratum (it must read a finished relation).
    let mut edges: Vec<(u32, u32, bool)> = Vec::new();
    for r in rules {
        let h = intern_rel(&r.head.rel, &mut id, &mut name);
        let agg = r.has_agg();
        for item in &r.body {
            let (b, force) = match item {
                BodyItem::Pos(a) => (intern_rel(&a.rel, &mut id, &mut name), agg),
                BodyItem::Neg(a) => (intern_rel(&a.rel, &mut id, &mut name), true),
                _ => continue,
            };
            edges.push((h, b, force));
        }
    }
    let n = name.len();
    let mut adj = vec![Vec::new(); n];
    for &(h, b, _) in &edges { adj[h as usize].push(b); }
    let (comp, ncomp) = scc::tarjan(&adj);

    // A negation OR aggregation inside a recursive cycle has no stratified meaning.
    // (The typecheck path reports this as a `not-stratified` TypeDiag first; this
    // bail is defense so eval never runs an ill-defined fixpoint.)
    for &(h, b, force) in &edges {
        if force && comp[h as usize] == comp[b as usize] {
            bail!("unstratifiable: relation '{}' is aggregated or negated inside a recursive cycle", name[b as usize]);
        }
    }
    // condensed edge weight: 1 if any stratum-forcing edge (negation or aggregation)
    // crosses these components
    let mut cw: HashMap<(u32, u32), u32> = HashMap::new();
    for &(h, b, force) in &edges {
        let (cu, cv) = (comp[h as usize], comp[b as usize]);
        if cu != cv {
            let e = cw.entry((cu, cv)).or_insert(0);
            *e = (*e).max(if force { 1 } else { 0 });
        }
    }
    let mut succ = vec![Vec::new(); ncomp];
    for (&(cu, cv), &w) in &cw { succ[cu as usize].push((cv, w)); }

    let mut memo = vec![u32::MAX; ncomp];
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (ri, r) in rules.iter().enumerate() {
        let c = comp[id[&r.head.rel] as usize] as usize;
        let s = comp_stratum(c, &succ, &mut memo) as usize;
        if s >= groups.len() { groups.resize(s + 1, Vec::new()); }
        groups[s].push(ri);
    }
    Ok(groups)
}

/// Split one stratum's rules into rel-level dependency components, dependencies
/// first. Rules sharing a head rel form one node; a component is recursive when
/// its rels are mutually reachable (multi-rel SCC) or a rule reads its own head.
/// `stratify` groups by negation depth, so a stratum can hold long acyclic
/// chains — each such component needs exactly one execution pass, not a fixpoint.
fn rel_components(group: &[usize], rules: &[&Rule]) -> Vec<(Vec<usize>, bool)> {
    let mut id: HashMap<&str, u32> = HashMap::new();
    let mut nheads = 0u32;
    for &ri in group {
        id.entry(rules[ri].head.rel.as_str()).or_insert_with(|| { nheads += 1; nheads - 1 });
    }
    let mut adj = vec![Vec::new(); nheads as usize];
    let mut self_edge = vec![false; nheads as usize];
    for &ri in group {
        let h = id[rules[ri].head.rel.as_str()];
        for item in &rules[ri].body {
            let atom = match item { BodyItem::Pos(a) | BodyItem::Neg(a) => a, _ => continue };
            match id.get(atom.rel.as_str()) {
                Some(&b) if b == h => self_edge[h as usize] = true, // tarjan skips self-loops
                Some(&b) => adj[h as usize].push(b),
                None => {} // reads a lower stratum / source rel: already final
            }
        }
    }
    let (comp, ncomp) = scc::tarjan(&adj);
    // adj points head -> body dependency, and tarjan completes every SCC
    // reachable from a node before the node's own, so ascending comp id is
    // dependencies-first evaluation order.
    let mut out: Vec<(Vec<usize>, bool)> = vec![(Vec::new(), false); ncomp];
    let mut comp_size = vec![0usize; ncomp];
    for (n, &c) in comp.iter().enumerate() {
        comp_size[c as usize] += 1;
        if self_edge[n] { out[c as usize].1 = true; }
    }
    for (c, size) in comp_size.iter().enumerate() { if *size > 1 { out[c].1 = true; } }
    for &ri in group {
        out[comp[id[rules[ri].head.rel.as_str()] as usize] as usize].0.push(ri);
    }
    out
}

type Bind = HashMap<String, Value>;
/// (repo slug, path, rev) -> (content hash, mtime secs, size bytes, line count).
/// The repo slug is the third coordinate so two repos sharing a path do not
/// collide. `line count` is -1 when unknown (a git rev, or an old row from
/// before this column existed) — `file_lines` filters those out.
type FileMeta = HashMap<(String, String, String), (String, i64, i64, i64)>;

struct Reconcile { changed: bool, extracted: usize, retracted: usize, parsed: usize, total: usize }

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
    md.modified().ok()
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
            db, rels: HashMap::new(), root, dropped: 0, extraction_drops: Vec::new(), shape_diags: Vec::new(), recondensed: 0,
            node2vec_recomputed: 0,
            closure_cache: HashMap::new(),
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
            fixpoint_full_reruns: std::cell::Cell::new(0),
            force_naive_fixpoint: std::cell::Cell::new(
                std::env::var("DL_NAIVE_FIXPOINT").ok().as_deref() == Some("1")),
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
    pub fn set_query_json(&mut self, on: bool) { self.query_json = on; }

    /// Skip `?` evaluation on the next tick (the foreground priming pass).
    pub fn set_prime_tick(&mut self, on: bool) { self.prime_tick = on; }

    /// Mark `root` as a placeholder (rootless daemon). Self-form scans and gen
    /// writes then fall back to each rule's `.git` ancestor. See `root_implicit`.
    pub fn set_root_implicit(&mut self, on: bool) { self.root_implicit = on; }

    /// Set the configured repos (from `SprfConfig`). Takes effect on the next
    /// tick via `refresh_builtin_rels`.
    pub fn set_repos(&mut self, repos: Vec<crate::config::RepoConfig>) {
        self.repos = repos;
    }

    /// Resolve a declared rev to a stable commit SHA (WORK stays WORK).
    /// Cached per tick so a moving ref is re-resolved each tick.
    pub fn root(&self) -> PathBuf { self.root.clone() }

    /// Stable slug for this engine's own repo: the `--root` directory name.
    pub(crate) fn self_slug(&self) -> String {
        self.root.file_name().map(|s| s.to_string_lossy().to_string())
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

    /// Located byte spans with their interned text, for the refactor sink:
    /// `_where_bytes ⋈ _strings`, sentinel skipped. Returns (path, lo, hi, text),
    /// where (lo, hi) is the rewrite coordinate in `path`'s WORK bytes and `text`
    /// is the contiguous source at that span. With a scan-only source program the
    /// only rows are import refs (no capture spans), so this is the `--move` feed.
    pub(crate) fn refresh_rel(&self, rel: &str, cols: &[&str], rows: &[Vec<Value>]) -> Result<usize> {
        let table = tbl(rel);
        self.db.exec(&format!("DELETE FROM {table}"))?;
        self.db.insert_rows(&table, cols, rows)
    }




}

/// Locate a NAMED zone in a file's line list. Returns `(begin_idx, end_idx)`
/// 0-based LINE INDICES where `lines[begin_idx]` carries `BEGIN: <name>` and
/// `lines[end_idx]` carries the matching `END:`. The caller splices the
/// strictly-inside range `[begin_idx+1, end_idx)`. Comment-prefix-tolerant:
/// `// BEGIN: name`, `# BEGIN: name`, `/* BEGIN: name */`, `; BEGIN: name`,
/// `<!-- BEGIN: name -->`, or a bare `BEGIN: name` all match. The first END
/// after the BEGIN closes the zone (END carries no name). `None` if no pair.
fn scan_spec_of(rule: &Rule) -> Result<ScanSpec> {
    for item in &rule.body {
        if let BodyItem::Scan { repo, rev, glob, path, rev_out } = item {
            let path_var = match path {
                Term::Var(v) => v.clone(),
                Term::Wild => bail!("scan path output must be a variable, not `_` (a scan with no path is meaningless)"),
                other => bail!("expected scan path variable, got {other:?}"),
            };
            return Ok(ScanSpec {
                repo: repo.clone(), rev: rev.clone(), glob: glob.clone(),
                path_var, rev_out_var: opt_var(rev_out)?,
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
    rule.body.iter().any(|b| matches!(b,
        BodyItem::Scan { repo: Term::Var(_), .. } |
        BodyItem::Scan { rev: Term::Var(_), .. }))
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
        "struct", "enum", "trait", "class", "interface", "const",
        "module", "mod", "type", "item", "macro", "function",
        "fn", "method", "alias", "def",
    ];
    let words: Vec<&str> = s.split_whitespace().collect();
    if words.is_empty() { return String::new(); }
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
    if start >= end { return String::new(); }
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
                if c.is_ascii_alphanumeric() || c == b'_' { i += 1; } else { break; }
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
fn check_type(ty: Type, v: &Value, repo: &str, rev: &str, root: &Path, rev_index: &HashSet<(String, String, String)>) -> bool {
    let p = match v { Value::Text(s) => s, Value::Int(_) => return ty == Type::Int || ty == Type::Text, Value::Null => return true };
    if rev != "WORK" {
        return match ty {
            Type::File | Type::Path => rev_index.contains(&(repo.to_string(), rev.to_string(), p.clone())),
            Type::Dir => rev_index.iter().any(|(rp, r, pp)| rp == repo && r == rev && pp.starts_with(&format!("{p}/"))),
            // repo/rev are coordinate values, not filesystem paths: no check here.
            Type::Text | Type::Int | Type::Repo | Type::Rev | Type::Sym => true,
        };
    }
    let full = root.join(p);
    match ty {
        Type::File => full.is_file(),
        Type::Dir => full.is_dir(),
        Type::Path => full.exists(),
        Type::Text | Type::Int | Type::Repo | Type::Rev | Type::Sym => true,
    }
}

/// Drop duplicate `RefHit`s (same repo/path/range/role), preserving first-seen
/// order. The refs-lens buckets fan out over per-sym queries, so the same
/// location can surface more than once (two syms sharing a caller, a symbol used
/// twice on one line).
fn dedup_hits(hits: &mut Vec<RefHit>) {
    let mut seen: HashSet<(String, String, u32, u32, u32, u32, String)> = HashSet::new();
    hits.retain(|h| seen.insert((h.repo.clone(), h.path.clone(), h.line, h.col,
        h.end_line, h.end_col, h.role.clone())));
}

/// UTF-8 byte offset -> (0-based line, 0-based char column) in `content`. The
/// 0-based line matches what `resolve_span` -> `span_to_range` produces on the
/// LSP side; a past-end offset clamps to the last position.
fn byte_to_lc0(content: &str, byte: u32) -> (u32, u32) {
    let byte = (byte as usize).min(content.len());
    let mut line = 0u32;
    let mut line_start = 0usize;
    for (i, b) in content.bytes().enumerate() {
        if i >= byte { break; }
        if b == b'\n' { line += 1; line_start = i + 1; }
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
        if matches!(ch, '%' | '_' | '\\') { escaped.push('\\'); }
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
        if !out.contains(&s) { out.push(s); }
    }
    out
}

/// The module name a file path answers to in an import specifier: the file
/// stem, except `mod.rs`/`index.*`/`lib.rs`/`main.rs` answer to their parent
/// directory's name. Used by `definition_targets` to pair a specifier segment
/// with a `module_edge` dst.
fn module_stem(path: &str) -> &str {
    let file = path.rsplit('/').next().unwrap_or(path);
    let stem = file.split('.').next().unwrap_or(file);
    if matches!(stem, "mod" | "index" | "lib" | "main") {
        path.rsplit('/').nth(1).unwrap_or(stem)
    } else {
        stem
    }
}

/// Enumerate (path, hash, mtime, size) for one repo×rev against the UNION of
/// that group's rule globs — one walk / one `ls-tree` per repo×rev per tick,
/// however many rules scan it. Free function (no `&self`) so groups enumerate
/// in parallel across repos. For WORK, stat each file and reuse the stored hash
/// when mtime+size are unchanged (the fast-path), reading+hashing only changed
/// files. A git rev uses the blob OID from `ls-tree`, so unchanged blobs are
/// detected without fetching content. The walk skips `.git` explicitly:
/// `hidden(false)` un-hides it, and crawling the object store made big-repo
/// scans pathological. A directory below the root that itself owns a `.git`
/// entry (dir or file — a submodule worktree's is a file) is a foreign repo
/// and is pruned the same way: the `git ls-tree` arm below already excludes
/// submodules for free (gitlink entries are type `commit`, not `blob`), so
/// this closes the WORK-arm asymmetry. Depth 0 is `repo_root` itself and is
/// never pruned by this check (it owns the `.git` we're walking FROM).
/// Once-per-full-scan corpus sanity: total files/bytes, the top-3 dirs by
/// file count, and a loud line if any single dir carries more than
/// `DIR_SHARE_WARN_PCT`% of the corpus (e.g. a vendored/generated tree the
/// scan glob should have excluded). Called once from the WORK arm of
/// `enumerate_with_hash` per repo, never per-file — corpus-sanity is a
/// scan-level verdict, not a hot-loop one.
fn emit_corpus_scan_verdict(repo: &str, files: &[(String, String, i64, i64, i64)]) {
    if files.is_empty() { return; }
    let total_files = files.len();
    let total_bytes: i64 = files.iter().map(|(_, _, _, sz, _)| *sz).sum();
    let mut per_dir: HashMap<&str, usize> = HashMap::new();
    for (rel, _, _, _, _) in files {
        let dir = rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        *per_dir.entry(dir).or_insert(0) += 1;
    }
    let mut dirs: Vec<(&str, usize)> = per_dir.into_iter().collect();
    dirs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let top3: Vec<String> = dirs.iter().take(3)
        .map(|(dir, n)| format!("{}:{n}", if dir.is_empty() { "." } else { dir }))
        .collect();
    let msg = format!(
        "[corpus] {repo}: {total_files} files, {total_bytes} bytes, top dirs: {}",
        top3.join(", ")
    );
    crate::verdict::verdict(
        "corpus-scan", &msg,
        &[("repo", repo), ("files", &total_files.to_string()),
          ("bytes", &total_bytes.to_string()), ("top_dirs", &top3.join(","))],
    );
    if let Some((dir, n)) = dirs.first() {
        let pct = (*n as u64 * 100) / total_files as u64;
        if pct as u32 > crate::verdict::DIR_SHARE_WARN_PCT {
            let dir_label = if dir.is_empty() { "." } else { dir };
            let warn_msg = format!(
                "[corpus] {repo}: WARNING dir {dir_label} carries {pct}% of {total_files} files (over {}%)",
                crate::verdict::DIR_SHARE_WARN_PCT
            );
            crate::verdict::verdict(
                "corpus-scan", &warn_msg,
                &[("repo", repo), ("dir", dir_label), ("pct", &pct.to_string()), ("outcome", "dir-share-warn")],
            );
        }
    }
}

/// Count lines the way `wc -l` semantics-adjacent editors expect: an empty
/// file is 0 lines; a file with content but no trailing newline still counts
/// its last (unterminated) line. Counts `\n` bytes and adds one more unless
/// the file already ends on a newline — no lossy String allocation, works on
/// raw bytes so binary-ish files don't panic on invalid UTF-8.
fn count_lines(bytes: &[u8]) -> i64 {
    if bytes.is_empty() { return 0; }
    let newlines = bytes.iter().filter(|&&b| b == b'\n').count() as i64;
    if bytes.last() == Some(&b'\n') { newlines } else { newlines + 1 }
}

fn enumerate_with_hash(repo: &str, repo_root: &Path, rev: &str, union: &globset::GlobSet, prev: &FileMeta) -> Result<Vec<(String, String, i64, i64, i64)>> {
    let max_size = max_filesize();
    if rev == "WORK" {
        let mut files: Vec<(PathBuf, String, i64, i64)> = Vec::new();
        let mut walk = ignore::WalkBuilder::new(repo_root);
        walk.hidden(false).filter_entry(|e| {
            if e.file_name() == ".git" { return false; }
            // One extra stat per walked DIRECTORY; file entries skip the check.
            if e.depth() >= 1
                && e.file_type().is_some_and(|ft| ft.is_dir())
                && e.path().join(".git").exists() { return false; }
            true
        });
        // The walker crate caps oversized files itself (skips them before we ever
        // hash), so a single minified/vendored blob can't blow RSS. Opt-in via
        // `DL_MAX_FILESIZE` (bytes); unset = no cap (legacy behavior).
        if let Some(cap) = max_size { walk.max_filesize(Some(cap)); }
        let walk = walk.build();
        for entry in walk.flatten() {
            if !entry.path().is_file() { continue; }
            let rel = match entry.path().strip_prefix(repo_root) { Ok(r) => r, Err(_) => continue };
            let rel = rel.to_string_lossy().replace('\\', "/");
            if !union.is_match(&rel) { continue; }
            let (mt, sz) = entry.metadata().ok().map(|m| (mtime_secs(&m), m.len() as i64)).unwrap_or((0, 0));
            files.push((entry.path().to_path_buf(), rel, mt, sz));
        }
        // reuse stored hash + line count when mtime+size match; otherwise
        // read+hash+count (parallel). A stored line count of -1 (unknown: an
        // old row from before this column existed) still forces one read on
        // an otherwise-unchanged file, purely to count lines — the hash is
        // NOT recomputed, so this is a one-time cost per file, not a repeat.
        let mut out: Vec<(String, String, i64, i64, i64)> = files.par_iter().map(|(abs, rel, mt, sz)| {
            if let Some((h, pmt, psz, plines)) = prev.get(&(repo.to_string(), rel.clone(), "WORK".to_string())) {
                if pmt == mt && psz == sz {
                    if *plines >= 0 {
                        return (rel.clone(), h.clone(), *mt, *sz, *plines);
                    }
                    let bytes = std::fs::read(abs).unwrap_or_default();
                    return (rel.clone(), h.clone(), *mt, *sz, count_lines(&bytes));
                }
            }
            let bytes = std::fs::read(abs).unwrap_or_default();
            (rel.clone(), blake3::hash(&bytes).to_hex().to_string(), *mt, *sz, count_lines(&bytes))
        }).collect();
        out.sort();
        emit_corpus_scan_verdict(repo, &out);
        Ok(out)
    } else {
        // `git ls-tree -r -l <rev>` lines:
        // "<mode> <type> <oid> <size>\t<path>"
        let output = Command::new("git")
            .arg("-C").arg(repo_root)
            .args(["ls-tree", "-r", "-l", rev])
            .output()?;
        if !output.status.success() { return Ok(Vec::new()); }
        let text = String::from_utf8_lossy(&output.stdout);
        let mut out = Vec::new();
        for line in text.lines() {
            let Some((meta, path)) = line.split_once('\t') else { continue };
            let parts: Vec<&str> = meta.split_whitespace().collect();
            if parts.get(1) != Some(&"blob") { continue; }
            let oid = parts.get(2).copied().unwrap_or_default();
            let size = parts.get(3).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
            // Same size cap as the WORK walker, applied to blob sizes from ls-tree.
            if let Some(cap) = max_size { if size as u64 > cap { continue; } }
            // Line count is left unknown (-1) for git-rev blobs: counting them
            // would spawn a read per blob, and the file-size rail only needs
            // WORK. See `file_lines`'s doc string.
            if union.is_match(path) { out.push((path.to_string(), oid.to_string(), 0, size, -1)); }
        }
        Ok(out)
    }
}

/// Resolve a gen write/splice target against the first candidate root where it
/// already lives. Candidates are `self.root` plus each rule-origin's `.git`
/// ancestor (collected by `run_gens`), so a gen rule splicing a file scanned
/// from a loaded script's repo writes back to that repo. A new file (no candidate
/// contains it) falls back to `self.root` (the first candidate), preserving the
/// original behavior for foreground file-emits.
fn resolve_write_full(write_roots: &[PathBuf], p: &str) -> PathBuf {
    for r in write_roots {
        let f = r.join(p);
        if f.exists() { return f; }
    }
    write_roots.first().map(|r| r.join(p)).unwrap_or_else(|| PathBuf::from(p))
}

/// Parse one file for one source rule (no DB access); returns (rows, dropped).
/// Safe to call in parallel: reads file content, runs extractors, builds rows.
#[tracing::instrument(skip_all, fields(repo = repo, path = path), level = "trace")]
/// Bind the spine id of a whole-match span (captures' min lo .. max hi) and
/// intern the slice. Shared by the `ast` and `sg` arms of `parse_file`, which
/// carried identical copies; extracted as the first measured refactor of the
/// reward-validated consolidation policy (verbatim block dup → one helper).
/// Mirrors `match`'s 5th-arg id binding. No-op when the id var is absent, the
/// file has no content-addressed id, or the span is empty/invalid.
fn bind_whole_match_span(
    ext: &mut Bind,
    idv: &Option<String>,
    caps: &[(String, String, usize, usize)],
    content: &str,
    where_file: Option<spine::FileId>,
    repo: &str,
    path: &str,
    where_bytes: &mut Vec<(spine::WhereBytes, String)>,
) {
    let lo = caps.iter().map(|(_, _, lo, _)| *lo).min();
    let hi = caps.iter().map(|(_, _, _, hi)| *hi).max();
    if let (Some(lo), Some(hi)) = (lo, hi) {
        bind_span_id(ext, idv, lo, hi, content, where_file, repo, path, where_bytes);
    }
}

/// Bind `idv` to the located spine id of `[lo, hi)` in `content` and intern the
/// slice. The byte-range core of `bind_whole_match_span`; the `sg` arm calls it
/// with the TRUE match-node range (literal text included) so a `gen(:replace)`
/// keyed off the id rewrites the whole pattern, not just the captures' bbox.
#[allow(clippy::too_many_arguments)]
fn bind_span_id(
    ext: &mut Bind,
    idv: &Option<String>,
    lo: usize,
    hi: usize,
    content: &str,
    where_file: Option<spine::FileId>,
    repo: &str,
    path: &str,
    where_bytes: &mut Vec<(spine::WhereBytes, String)>,
) {
    if let Some(idv) = idv {
        if let Some(file) = where_file {
            if hi > lo && hi <= content.len() {
                let text = &content[lo..hi];
                if !text.is_empty() {
                    let wb = spine::WhereBytes {
                        string: spine::StringId::of(text), file,
                        lo: lo as u32, hi: hi as u32,
                        ..Default::default()
                    };
                    ext.insert(idv.clone(), Value::Text(
                        spine::WhereBytesId::of_located(wb, repo, path)
                            .to_string()));
                    where_bytes.push((spine::WhereBytes {
                        string: spine::StringId::of(text), file,
                        lo: lo as u32, hi: hi as u32,
                        ..Default::default()
                    }, text.to_string()));
                }
            }
        }
    }
}

/// The dl authoring note appended to every regex compile error (parse-only AND
/// the runtime scan/eval path). Points at the Rust-regex escape: the crate has
/// no look-around or backrefs, so anchor instead.
pub const DL_REGEX_NOTE: &str =
    "\nnote: regexes are Rust-flavor: no lookahead/lookbehind/backrefs; \
     anchor with $, \\b, or character classes.";

/// Compile a dl regex literal EXACTLY as the scan/eval path does — the single
/// construction point so `--parse-only` and the runtime can never drift on
/// flags — and carry the dl authoring note on any compile error. Every
/// `match`/`comment`/`=~` regex goes through here.
pub fn compile_dl_regex(pattern: &str) -> Result<Regex> {
    Regex::new(pattern).map_err(|e| anyhow::anyhow!("{e}{DL_REGEX_NOTE}"))
}

/// Compile every regex literal the program carries (`match`, `comment` open/
/// close, `=~` body constraints) through `compile_dl_regex`, turning each
/// compile failure into an error `TypeDiag`. Lets `--parse-only` reject an
/// unsupported pattern (`/(?!-)/`) without paying a scan — the runtime would
/// otherwise be the first to fail, mid-scan. `path` attributes the diags (line
/// 1, the same coarseness as the other parse-only diagnostics). Reports ALL bad
/// regexes, not the first only.
pub fn regex_literal_diags(prog: &Program, path: &str) -> Vec<TypeDiag> {
    fn push(out: &mut Vec<TypeDiag>, path: &str, pat: &str) {
        if let Err(e) = compile_dl_regex(pat) {
            out.push(TypeDiag {
                path: path.to_string(),
                span: (0, 0),
                severity: Severity::Error,
                code: "regex".to_string(),
                msg: e.to_string(),
            });
        }
    }
    let mut out = Vec::new();
    for item in &prog.items {
        let Item::Rule(r) = item else { continue };
        for b in &r.body {
            match b {
                BodyItem::Match { regex, .. } => push(&mut out, path, regex),
                BodyItem::Comment { open, close, .. } => {
                    push(&mut out, path, open);
                    if let Some(c) = close { push(&mut out, path, c); }
                }
                BodyItem::Cmp(c) if c.op == CmpOp::Match => {
                    if let Term::Str(s) = &c.rhs { push(&mut out, path, s); }
                }
                _ => {}
            }
        }
    }
    out
}

/// `match(regex, line, [id])` body op. For each input bind, scan every line of
/// `content`; for each regex capture set produce an extended bind: the line
/// number (into `line`), each named capture (into its name), and — when `id`
/// is requested (5-arg form) — a whole-match spine id plus its span. Extracted
/// from `parse_file`'s Match arm (iteration-2 god-fn split). `re_cache`
/// memoizes compiled regexes across items. `push_span`'s guard (only when the
/// file has a content-addressed id and the text is non-empty) is inlined.
fn bind_match_op(
    binds: &[Bind],
    regex: &str,
    mlv: &Option<String>,
    idv: &Option<String>,
    colv: &Option<String>,
    ecv: &Option<String>,
    content: &str,
    where_file: Option<spine::FileId>,
    re_cache: &mut HashMap<String, Regex>,
    where_bytes: &mut Vec<(spine::WhereBytes, String)>,
    repo: &str,
    path: &str,
) -> Result<Vec<Bind>> {
    if !re_cache.contains_key(regex) { re_cache.insert(regex.to_string(), compile_dl_regex(regex)?); }
    let re = &re_cache[regex];
    let names: Vec<&str> = re.capture_names().flatten().collect();
    let mut next: Vec<Bind> = Vec::new();
    let base = content.as_ptr() as usize;
    for b in binds {
        for (lineno, ln) in content.lines().enumerate() {
            let line_off = ln.as_ptr() as usize - base;
            for caps in re.captures_iter(ln) {
                let mut ext = b.clone();
                if let Some(v) = mlv { ext.insert(v.clone(), Value::Int((lineno + 1) as i64)); }
                if colv.is_some() || ecv.is_some() {
                    if let Some(m0) = caps.get(0) {
                        // Whole-match span, 0-based byte columns within the line.
                        if let Some(v) = colv { ext.insert(v.clone(), Value::Int(m0.start() as i64)); }
                        if let Some(v) = ecv { ext.insert(v.clone(), Value::Int(m0.end() as i64)); }
                    }
                }
                if let Some(idv) = idv {
                    if let Some(file) = where_file {
                        if let Some(m0) = caps.get(0) {
                            let text = m0.as_str();
                            let lo = line_off + m0.start();
                            let hi = line_off + m0.end();
                            if !text.is_empty() {
                                let wb = spine::WhereBytes {
                                    string: spine::StringId::of(text), file,
                                    lo: lo as u32, hi: hi as u32,
                                    ..Default::default()
                                };
                                ext.insert(idv.clone(), Value::Text(
                                    spine::WhereBytesId::of_located(wb, repo, path).to_string()));
                                where_bytes.push((spine::WhereBytes {
                                    string: spine::StringId::of(text), file,
                                    lo: lo as u32, hi: hi as u32,
                                    ..Default::default()
                                }, text.to_string()));
                            }
                        }
                    }
                }
                for n in &names {
                    if let Some(m) = caps.name(n) {
                        let text = m.as_str();
                        ext.insert((*n).to_string(), Value::Text(text.to_string()));
                        if let Some(file) = where_file {
                            if !text.is_empty() {
                                where_bytes.push((spine::WhereBytes {
                                    string: spine::StringId::of(text), file,
                                    lo: (line_off + m.start()) as u32, hi: (line_off + m.end()) as u32,
                                    ..Default::default()
                                }, text.to_string()));
                            }
                        }
                    }
                }
                next.push(ext);
            }
        }
    }
    Ok(next)
}

fn parse_file(
    rule: &Rule, repo: &str, path: &str, rev: &str, hash: &str,
    root: &Path, rels: &Rels, rev_index: &HashSet<(String, String, String)>,
    head_binds: &[(String, String)],
) -> Result<(Vec<Vec<Value>>, Vec<(spine::WhereBytes, String)>, usize)> {
    let spec = scan_spec_of(rule)?;
    let pathvar = spec.path_var;
    let revvar = spec.rev_out_var;
    let cmps: Vec<&Constraint> = rule.body.iter()
        .filter_map(|i| if let BodyItem::Cmp(c) = i { Some(c) } else { None }).collect();
    let content = read_content(root, rev, path).unwrap_or_default();
    // Ref-spine: locate each capture's bytes in the file content. The file id is
    // derived from the same stored content address `_files` uses (blake3 for
    // WORK, blob OID for a git rev), so located rows join `_files` for both.
    let where_file = spine::FileId::from_content_address(hash, content.len() as i64)
        .filter(|f| *f != spine::FileId::SYNTHETIC);
    let mut where_bytes: Vec<(spine::WhereBytes, String)> = Vec::new();
    let push_span = |text: &str, lo: usize, hi: usize, where_bytes: &mut Vec<(spine::WhereBytes, String)>| {
        if let Some(file) = where_file {
            if !text.is_empty() {
                // Carry the located text alongside its span so the flush interns
                // BOTH `_where_bytes` (the span) AND `_strings` (the text, under
                // the SAME StringId the WhereBytes hashes). Without the text, a
                // located id (capture span, `match`/`ast` whole-match id) resolves
                // through `ref(id,_,_,lo,hi)` but NOT `string(id,text,norm)`.
                where_bytes.push((spine::WhereBytes {
                    string: spine::StringId::of(text),
                    file,
                    lo: lo as u32,
                    hi: hi as u32,
                    ..Default::default()
                }, text.to_string()));
            }
        }
    };
    let bind_captures = |ext: &mut Bind,
                         caps: &[(String, String, usize, usize)],
                         where_bytes: &mut Vec<(spine::WhereBytes, String)>| {
        for (n, t, lo, hi) in caps {
            ext.insert(n.clone(), Value::Text(t.clone()));
            push_span(t, *lo, *hi, where_bytes);
        }
    };
    let head_meta = rels.get(&rule.head.rel)
        .ok_or_else(|| anyhow::anyhow!("unknown head relation {}", rule.head.rel))?;
    let mut re_cache: HashMap<String, Regex> = HashMap::new();

    let mut binds: Vec<Bind> = vec![{
        let mut b = Bind::new();
        b.insert(pathvar.clone(), Value::Text(path.to_string()));
        if let Some(rv) = &revvar { b.insert(rv.clone(), Value::Text(rev.to_string())); }
        // Data-driven coordinate values (the variable repo/rev this file was
        // scanned under): seed them so the rule head can reference them.
        for (k, v) in head_binds { b.insert(k.clone(), Value::Text(v.clone())); }
        b
    }];

    for item in &rule.body {
        match item {
            BodyItem::Match { regex, line, id, col, end_col, .. } => {
                let mlv = opt_var(line)?;
                let idv = id.as_ref().map(var_of).transpose()?;
                let colv = col.as_ref().map(opt_var).transpose()?.flatten();
                let ecv = end_col.as_ref().map(opt_var).transpose()?.flatten();
                binds = bind_match_op(&binds, regex, &mlv, &idv, &colv, &ecv, &content, where_file,
                                      &mut re_cache, &mut where_bytes, repo, path)?;
            }
            BodyItem::Ast { lang, query, line, end, id, .. } => {
                let alv = opt_var(line)?;
                let elv = end.as_ref().map(opt_var).transpose()?.flatten();
                // Optional 7th arg: the spine id of the WHOLE ast match span (the
                // captures' min..max byte range). Joins `ref(id, _, _, lo, hi)`
                // for the codemod anchor — the bytes this match covered — same
                // located-id shape as `match`'s 5th arg (christmas #9). The text
                // interned for both the id and the span is the literal source
                // slice over that range, so `node`/`ref`/`string` all agree.
                let idv = id.as_ref().map(var_of).transpose()?;
                let hits = run_ts(&content, lang, query)?;
                let mut next: Vec<Bind> = Vec::new();
                for b in &binds {
                    for (start, endln, caps) in &hits {
                        let mut ext = b.clone();
                        if let Some(v) = &alv { ext.insert(v.clone(), Value::Int(*start)); }
                        if let Some(ev) = &elv { ext.insert(ev.clone(), Value::Int(*endln)); }
                        // Whole-match span = the captures' min lo .. max hi. Push
                        // it (interning the contiguous source slice) and bind its
                        // located id before the per-capture spans, mirroring the
                        // `match` arm. Skipped when no captures carry a span.
                        bind_whole_match_span(&mut ext, &idv, caps, &content, where_file, repo, path, &mut where_bytes);
                        bind_captures(&mut ext, caps, &mut where_bytes);
                        next.push(ext);
                    }
                }
                binds = next;
            }
            BodyItem::Sg { lang, pattern, line, col, end_line, end_col, id, .. } => {
                let slv = opt_var(line)?;
                let clv = opt_var(col)?;
                let ellv = opt_var(end_line)?;
                let eclv = opt_var(end_col)?;
                // Optional trailing `id`: the spine id of the whole sg match span
                // (captures' min lo .. max hi), same located-id shape as `ast`/
                // `match` (christmas #9, decision 3). Resolves via `ref` AND
                // `string` (rides step 1's intern of the slice text).
                let idv = id.as_ref().map(var_of).transpose()?;
                // prefilter: a file lacking any literal token cannot match
                let lits = pattern_literals(pattern);
                if !lits.iter().all(|t| content.contains(t.as_str())) {
                    binds = Vec::new();
                    continue;
                }
                let hits = crate::sg::run_sg(&content, lang, pattern)?;
                let mut next: Vec<Bind> = Vec::new();
                for b in &binds {
                    for (ln, c, eln, ec, mlo, mhi, caps) in &hits {
                        let mut ext = b.clone();
                        if let Some(v) = &slv { ext.insert(v.clone(), Value::Int(*ln)); }
                        if let Some(v) = &clv { ext.insert(v.clone(), Value::Int(*c)); }
                        if let Some(v) = &ellv { ext.insert(v.clone(), Value::Int(*eln)); }
                        if let Some(v) = &eclv { ext.insert(v.clone(), Value::Int(*ec)); }
                        // id = the TRUE whole-match byte range (literal text incl.),
                        // so gen(:replace, ref(id)) rewrites the entire pattern.
                        bind_span_id(&mut ext, &idv, *mlo, *mhi, &content, where_file, repo, path, &mut where_bytes);
                        bind_captures(&mut ext, caps, &mut where_bytes);
                        next.push(ext);
                    }
                }
                binds = next;
            }
            BodyItem::AstYaml { lang, yaml, line, col, end_line, end_col, .. } => {
                let slv = opt_var(line)?;
                let clv = opt_var(col)?;
                let ellv = opt_var(end_line)?;
                let eclv = opt_var(end_col)?;
                // No literal-prefilter (the YAML body is structural, not a
                // plain token set like a pattern); the RuleCore matcher is
                // already cheap on a non-matching file.
                let hits = crate::sg::run_ast_yaml(&content, lang, yaml)?;
                let mut next: Vec<Bind> = Vec::new();
                for b in &binds {
                    for (ln, c, eln, ec, _mlo, _mhi, caps) in &hits {
                        let mut ext = b.clone();
                        if let Some(v) = &slv { ext.insert(v.clone(), Value::Int(*ln)); }
                        if let Some(v) = &clv { ext.insert(v.clone(), Value::Int(*c)); }
                        if let Some(v) = &ellv { ext.insert(v.clone(), Value::Int(*eln)); }
                        if let Some(v) = &eclv { ext.insert(v.clone(), Value::Int(*ec)); }
                        bind_captures(&mut ext, caps, &mut where_bytes);
                        next.push(ext);
                    }
                }
                binds = next;
            }
            BodyItem::Cmd { template, line, out, .. } => {
                let lv = opt_var(line)?;
                let ov = opt_var(out)?;
                // Budget guard: one cmd rule shells out once per matched file, so
                // a broad glob is a subprocess storm. Over budget = a loud bail
                // naming the command, never a silent truncation of the relation.
                if let Some(budget) = cmd_budget() {
                    let n = CMD_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    if n > budget {
                        bail!("cmd budget exceeded: tick needs more than {budget} `cmd` \
                               invocation(s) (next: `{template}` on {path}) — raise \
                               --cmd-budget / DL_CMD_BUDGET or narrow the scan glob");
                    }
                }
                // {file}: WORK reads the on-disk path; a git rev materializes the
                // cached content to a content-addressed temp file (reused across ticks)
                let file_arg = if rev == "WORK" {
                    root.join(path).display().to_string()
                } else {
                    let tmp = std::env::temp_dir().join(format!("dl_cmd_{hash}"));
                    if !tmp.is_file() { std::fs::write(&tmp, &content)?; }
                    tmp.display().to_string()
                };
                let cmdline = template
                    .replace("{file}", &file_arg)
                    .replace("{path}", path)
                    .replace("{root}", &root.display().to_string());
                let t_cmd = std::time::Instant::now();
                let output = Command::new("sh").arg("-c").arg(&cmdline)
                    .current_dir(root).output()?;
                if crate::db::profiling() && t_cmd.elapsed().as_millis() >= 250 {
                    eprintln!("[cmd {:.0}ms] {cmdline}", t_cmd.elapsed().as_secs_f64() * 1000.0);
                }
                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                // nonzero exit WITH stdout is the diff-tool convention (findings
                // exist); nonzero with empty stdout is a broken command, be loud
                if !output.status.success() && stdout.trim().is_empty() {
                    bail!("cmd `{cmdline}` failed (exit {:?}): {}",
                          output.status.code(),
                          String::from_utf8_lossy(&output.stderr).trim());
                }
                let mut next: Vec<Bind> = Vec::new();
                for b in &binds {
                    for (i, ln) in stdout.lines().enumerate() {
                        let mut ext = b.clone();
                        if let Some(v) = &lv { ext.insert(v.clone(), Value::Int((i + 1) as i64)); }
                        if let Some(v) = &ov { ext.insert(v.clone(), Value::Text(ln.to_string())); }
                        next.push(ext);
                    }
                }
                binds = next;
            }
            BodyItem::Comment { open, close, l0, l1, label, .. } => {
                let l0v = opt_var(l0)?;
                let l1v = opt_var(l1)?;
                let labv = opt_var(label)?;
                if !re_cache.contains_key(open) { re_cache.insert(open.clone(), compile_dl_regex(open)?); }
                if let Some(c) = close {
                    if !re_cache.contains_key(c) { re_cache.insert(c.clone(), compile_dl_regex(c)?); }
                }
                let open_re = &re_cache[open];
                let close_re = close.as_ref().map(|c| &re_cache[c]);
                let regions = crate::comment::run_comment(&content, open_re, close_re);
                let mut next: Vec<Bind> = Vec::new();
                for b in &binds {
                    for r in &regions {
                        let mut ext = b.clone();
                        if let Some(v) = &l0v { ext.insert(v.clone(), Value::Int(r.l0)); }
                        if let Some(v) = &l1v { ext.insert(v.clone(), Value::Int(r.l1)); }
                        if let Some(v) = &labv { ext.insert(v.clone(), Value::Text(r.label.clone())); }
                        if let Some((lo, hi)) = r.label_span {
                            push_span(&r.label, lo, hi, &mut where_bytes);
                        }
                        next.push(ext);
                    }
                }
                binds = next;
            }
            BodyItem::JsonP { jpath, out, id, .. } => {
                let ov = opt_var(out)?;
                // Optional trailing `id`: the spine id of the matched value's byte
                // span. For json the value span IS the whole match (christmas #9,
                // decision 3). Resolves via `ref` AND `string`.
                let idv = id.as_ref().map(var_of).transpose()?;
                let vals = crate::datapath::run_data(path, &content, jpath);
                let mut next: Vec<Bind> = Vec::new();
                for b in &binds {
                    for (v, lo, hi) in &vals {
                        let mut ext = b.clone();
                        if let Some(ov) = &ov { ext.insert(ov.clone(), Value::Text(v.clone())); }
                        if let Some(idv) = &idv {
                            if let Some(file) = where_file {
                                if !v.is_empty() {
                                    let wb = spine::WhereBytes {
                                        string: spine::StringId::of(v), file,
                                        lo: *lo as u32, hi: *hi as u32,
                                        ..Default::default()
                                    };
                                    ext.insert(idv.clone(), Value::Text(
                                        spine::WhereBytesId::of_located(wb, repo, path)
                                            .to_string()));
                                }
                            }
                        }
                        push_span(v, *lo, *hi, &mut where_bytes);
                        next.push(ext);
                    }
                }
                binds = next;
            }
            BodyItem::Json { pat, .. } => {
                // Declarative brace pattern. The body was validated at parse
                // time; re-parse to get the Step tree (cheap; pattern is tiny)
                // and walk it. Each match binds N captures by name into the
                // row, like match's named groups.
                let (steps, _) = crate::datapath::parse_pattern(pat)
                    .map_err(|e| anyhow::anyhow!("json pattern error: {e}"))?;
                let ms = crate::datapath::run_pattern(path, &content, &steps);
                let mut next: Vec<Bind> = Vec::new();
                for b in &binds {
                    for m in &ms {
                        let mut ext = b.clone();
                        for (cap, text, lo, hi) in m {
                            ext.insert(cap.clone(), Value::Text(text.clone()));
                            push_span(text, *lo, *hi, &mut where_bytes);
                        }
                        next.push(ext);
                    }
                }
                binds = next;
            }
            _ => {}
        }
    }

    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut dropped = 0usize;
    'bind: for b in binds {
        for c in &cmps {
            if !eval_cmp(c, &b)? { continue 'bind; }
        }
        let mut row = Vec::with_capacity(head_meta.cols.len());
        for (i, term) in rule.head.terms.iter().enumerate() {
            let v = match term {
                Term::Var(v) => b.get(v).cloned()
                    .ok_or_else(|| {
                        let mut msg = format!(
                            "head var `{v}` is not bound by any source op in this rule. A source rule \
                             (scan/match/ast/sg/json) binds head vars only from the source op's own \
                             captures — a join to `repo(...)`/`file(...)` in the body cannot supply it. \
                             To fan a scan over every configured repo AND capture which repo each row \
                             came from, put `{v}` in scan's repo slot: \
                             `... <- repo({v}, _, _), scan({v}, rev, glob, path, rev_out).`");
                        // sg/ast_yaml `$$$NAME` is a MULTI metavar (pattern
                        // structure), never a single-node capture, so it binds no
                        // head var. Name the fix when that is what happened.
                        let is_structural_metavar = rule.body.iter().any(|bi| match bi {
                            BodyItem::Sg { pattern, .. } => pattern.contains(&format!("$$${v}")),
                            BodyItem::AstYaml { yaml, .. } => yaml.contains(&format!("$$${v}")),
                            _ => false,
                        });
                        if is_structural_metavar {
                            msg.push_str(&format!(
                                "\nnote: $$${v} is pattern structure only; bind a single node with \
                                 ${v} or use the span outputs."));
                        }
                        anyhow::anyhow!(msg)
                    })?,
                Term::Str(s) => Value::Text(s.clone()),
                Term::Int(n) => Value::Int(*n),
                Term::Interp(parts) => interp_value(parts, &b)?,
                // A Wild head slot is head named-arg padding (a diag rule that
                // names only some columns). Emit NULL; the reader defaults it.
                Term::Wild => Value::Null,
                Term::PathLit { .. } => bail!("path literal not normalized before lowering"),
                Term::Arith { .. } => val_of(term, &b)?,
                Term::Call { .. } => val_of(term, &b)?,
            };
            // NULL (a padded column) has no type to check; the file/path checks
            // would drop it. Only type-check present values.
            if !matches!(v, Value::Null)
                && !check_type(head_meta.cols[i].ty, &v, repo, rev, root, rev_index) { dropped += 1; continue 'bind; }
            row.push(v);
        }
        rows.push(row);
    }
    Ok((rows, where_bytes, dropped))
}

fn row_hash(row: &[Value]) -> String {
    let mut s = String::new();
    for (i, v) in row.iter().enumerate() {
        if i > 0 { s.push('\u{1}'); }
        s.push_str(&v.as_str());
    }
    blake3::hash(s.as_bytes()).to_hex().to_string()
}

fn str_of(t: &Term) -> Result<String> {
    match t { Term::Str(s) => Ok(s.clone()), _ => bail!("expected string literal, got {t:?}") }
}
fn var_of(t: &Term) -> Result<String> {
    match t { Term::Var(v) => Ok(v.clone()), _ => bail!("expected variable, got {t:?}") }
}

/// Like `var_of` but accepts `Term::Wild` (`_`) — returns None so the caller
/// skips binding that output. Backs the kwarg/`_` output forms: an unmentioned
/// or `_` op output produces its row value but binds nothing.
fn opt_var(t: &Term) -> Result<Option<String>> {
    match t {
        Term::Var(v) => Ok(Some(v.clone())),
        Term::Wild => Ok(None),
        other => bail!("expected variable or `_`, got {other:?}"),
    }
}

/// Build an interpolated string from bindings: `"${ty}::${name}"` -> "Foo::bar".
fn interp_value(parts: &[InterpPart], b: &Bind) -> Result<Value> {
    let mut s = String::new();
    for p in parts {
        match p {
            InterpPart::Lit(l) => s.push_str(l),
            InterpPart::Var(v) => s.push_str(
                &b.get(v).ok_or_else(|| anyhow::anyhow!("unbound var {v} in interpolation"))?.as_str()),
        }
    }
    Ok(Value::Text(s))
}

/// text -> int the way SQLite `CAST(x AS INTEGER)` does: skip leading whitespace,
/// take an optional sign and the leading digit run, parse that; no digits -> 0.
/// Keeps the source-rule (Rust) path identical to the derived (SQL) path.
fn cast_int(s: &str) -> i64 {
    let t = s.trim_start();
    let bytes = t.as_bytes();
    let mut i = 0;
    if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'+') { i += 1; }
    let digits_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
    if i == digits_start { return 0; }
    t[..i].parse::<i64>().unwrap_or(0)
}

fn val_of(t: &Term, b: &Bind) -> Result<Value> {
    match t {
        Term::Var(v) => b.get(v).cloned().ok_or_else(|| anyhow::anyhow!(
            "unbound var {v} in constraint\nnote: to compute a new value in a SOURCE rule \
             (scan/match/ast/...), put the expression in the rule head: head(path, line+1) <- ...; \
             body binds (`ext = split(path, \".\", -1)`) work in derived-rule bodies only")),
        Term::Str(s) => Ok(Value::Text(s.clone())),
        Term::Int(n) => Ok(Value::Int(*n)),
        Term::Interp(parts) => interp_value(parts, b),
        Term::Wild => bail!("'_' in constraint"),
        Term::PathLit { .. } => bail!("path literal not normalized before lowering"),
        Term::Arith { op, lhs, rhs } => {
            let (l, r) = (val_of(lhs, b)?, val_of(rhs, b)?);
            // `+` over two text values concatenates (the source-rule twin of the
            // derived `||` lowering); every other combination stays int-only.
            if let (ArithOp::Add, Value::Text(ls), Value::Text(rs)) = (op, &l, &r) {
                return Ok(Value::Text(format!("{ls}{rs}")));
            }
            let (Value::Int(a), Value::Int(c)) = (&l, &r) else {
                if matches!(op, ArithOp::Add) {
                    bail!("cannot `+` int and text — interpolate (\"${{count}}${{name}}\") or convert with int(..)");
                }
                bail!("arithmetic needs int operands, got {l:?} {} {r:?}", op.sql());
            };
            Ok(Value::Int(match op {
                ArithOp::Add => a + c,
                ArithOp::Sub => a - c,
                ArithOp::Mul => a * c,
                ArithOp::Div => {
                    if *c == 0 { bail!("division by zero in source-rule arithmetic"); }
                    a / c
                }
                ArithOp::Mod => {
                    if *c == 0 { bail!("modulo by zero in source-rule arithmetic"); }
                    a % c
                }
            }))
        }
        Term::Call { name, args } => {
            let vals: Vec<Value> = args.iter().map(|a| val_of(a, b)).collect::<Result<_>>()?;
            let str_at = |i: usize| vals.get(i).and_then(|v| match v {
                Value::Text(s) => Some(s.as_str()), _ => None,
            }).ok_or_else(|| anyhow::anyhow!("function `{name}` arg {i} must be text"));
            let int_at = |i: usize| vals.get(i).and_then(|v| match v {
                Value::Int(n) => Some(*n), _ => None,
            }).ok_or_else(|| anyhow::anyhow!("function `{name}` arg {i} must be int"));
            match name.as_str() {
                "replace" => {
                    let (text, from, to) = (str_at(0)?, str_at(1)?, str_at(2)?);
                    Ok(Value::Text(text.replace(from, to)))
                }
                "split" => {
                    let (text, sep) = (str_at(0)?, str_at(1)?);
                    let idx = int_at(2)?;
                    if sep.is_empty() { bail!("function split: empty separator"); }
                    let parts: Vec<&str> = text.split(sep).collect();
                    let n = parts.len() as i64;
                    let i = if idx >= 0 { idx } else { idx + n };
                    if i < 0 || i >= n { bail!("function split: idx {idx} out of range ({n} parts)"); }
                    Ok(Value::Text(parts[i as usize].to_string()))
                }
                // text -> int, mirroring SQLite `CAST(.. AS INTEGER)`: leading
                // optional sign + digit run, anything else (incl. garbage) -> 0.
                "int" => Ok(Value::Int(cast_int(str_at(0)?))),
                other => bail!("unknown function `{other}` (known: split, replace, int)"),
            }
        }
    }
}

fn eval_cmp(c: &Constraint, b: &Bind) -> Result<bool> {
    let l = val_of(&c.lhs, b)?;
    let r = val_of(&c.rhs, b)?;
    // Pattern ops: lhs value tested against rhs pattern (a literal string).
    match c.op {
        CmpOp::Match => {
            let re = compile_dl_regex(&r.as_str())?;
            return Ok(re.is_match(&l.as_str()));
        }
        CmpOp::Glob => {
            let g = globset::Glob::new(&r.as_str())?.compile_matcher();
            return Ok(g.is_match(l.as_str()));
        }
        _ => {}
    }
    let ord = match (&l, &r) {
        (Value::Int(a), Value::Int(b)) => a.cmp(b),
        _ => l.as_str().cmp(&r.as_str()),
    };
    Ok(match c.op {
        CmpOp::Eq => ord.is_eq(), CmpOp::Ne => ord.is_ne(),
        CmpOp::Lt => ord.is_lt(), CmpOp::Le => ord.is_le(),
        CmpOp::Gt => ord.is_gt(), CmpOp::Ge => ord.is_ge(),
        CmpOp::Match | CmpOp::Glob => unreachable!("handled above"),
    })
}

// Vendored grammar entry points, compiled by build.rs from vendor/grammars/.
// The C signature is `const TSLanguage *tree_sitter_X(void)`; declared here as
// `*const ()` (opaque) so tree_sitter_language::LanguageFn::from_raw accepts
// it. go-template has no crate; dockerfile's only crate pins tree-sitter 0.20.
extern "C" {
    fn tree_sitter_gotmpl() -> *const ();
    fn tree_sitter_dockerfile() -> *const ();
}

// LANG-JUNCTION(ast-grammars): one table row = `ast` op support (tree-sitter constructor keyed by label); `comment_node` and the CST node/child rels also dispatch through `ts_lang`, via `cst::lang_label_for_path`
/// The tree-sitter grammar table for the `ast` op (S-expression queries):
/// `(canonical name, [extra aliases], constructor)`. Single source of truth so
/// `ts_lang` (the resolver), the bail message, and `ast_langs` (the list the
/// skill language matrix must match) can never drift. Adding a grammar here
/// without updating the skill matrix fails the matrix-honesty test. Distinct
/// from `sg`'s table: the `ast` op runs tree-sitter, `sg`/`ast_yaml` run
/// ast-grep — the language sets differ (e.g. `ast` has bash/hcl/gotmpl but no
/// tsx; `sg` has tsx/typescript/cpp but no bash). The non-capturing closures
/// coerce to `fn` pointers, so this promotes to a `&'static` slice.
/// Run a tree-sitter S-expression query over file content.
/// Returns (start_line, end_line, captures) per match; start = min capture start
/// row, end = max capture end row (the matched region's span). Each capture is
/// `(name, text, lo, hi)` where `[lo, hi)` is the node's byte range in `content`.
fn run_ts(content: &str, lang: &str, query_str: &str) -> Result<Vec<(i64, i64, Vec<(String, String, usize, usize)>)>> {
    use streaming_iterator::StreamingIterator;
    let language = ts_lang(lang)?;
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language)?;
    let tree = parser.parse(content, None).ok_or_else(|| anyhow::anyhow!("ast parse failed"))?;
    let query = tree_sitter::Query::new(&language, query_str)?;
    let names = query.capture_names();
    let src = content.as_bytes();
    let mut cursor = tree_sitter::QueryCursor::new();
    let mut out = Vec::new();
    let mut it = cursor.matches(&query, tree.root_node(), src);
    while let Some(m) = it.next() {
        let mut caps = Vec::new();
        let mut line = i64::MAX;
        let mut end = 1i64;
        for c in m.captures {
            let name = names[c.index as usize].to_string();
            let text = c.node.utf8_text(src).unwrap_or("").to_string();
            line = line.min(c.node.start_position().row as i64 + 1);
            end = end.max(c.node.end_position().row as i64 + 1);
            caps.push((name, text, c.node.start_byte(), c.node.end_byte()));
        }
        if line == i64::MAX { line = 1; }
        out.push((line, end, caps));
    }
    Ok(out)
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
             c(x) <- b(x). a(x) <- s(x). b(x) <- a(x).");
        let rules: Vec<&Rule> = prog.items.iter()
            .filter_map(|i| match i { Item::Rule(r) if !r.body.is_empty() => Some(r), _ => None })
            .collect();
        let groups = stratify(&rules).unwrap();
        assert_eq!(groups.len(), 1, "all positive: one stratum");
        let comps = rel_components(&groups[0], &rules);
        assert_eq!(comps.len(), 3);
        assert!(comps.iter().all(|(_, recursive)| !recursive), "chain is acyclic");
        let order: Vec<&str> = comps.iter()
            .map(|(ris, _)| rules[ris[0]].head.rel.as_str()).collect();
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
             lone(x) <- s(x).");
        let rules: Vec<&Rule> = prog.items.iter()
            .filter_map(|i| match i { Item::Rule(r) if !r.body.is_empty() => Some(r), _ => None })
            .collect();
        let groups = stratify(&rules).unwrap();
        assert_eq!(groups.len(), 1);
        let comps = rel_components(&groups[0], &rules);
        let by_head = |name: &str| comps.iter()
            .find(|(ris, _)| ris.iter().any(|&ri| rules[ri].head.rel == name))
            .unwrap_or_else(|| panic!("no component holds {name}"));
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
