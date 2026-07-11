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
pub(crate) use query::emit_query_json;
mod decls;
pub use decls::{
    all_builtin_decls, builtin_enum_brands, builtin_enum_variants, builtin_rel_names, fn_docs,
    op_docs, undocumented_builtins, undocumented_fns,
};
pub(crate) use decls::*;
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
    fn resolve_rev(&mut self, repo_root: &Path, rev: &str) -> Result<String> {
        if rev == "WORK" { return Ok("WORK".to_string()); }
        // Cache by (repo, rev): the same tag resolves to different shas per repo.
        // Immutable (hex-SHA) revs live in the cross-tick cache; movable refs in
        // the per-tick one. A hit in either means we already have it — no spawn.
        let key = format!("{}::{rev}", repo_root.display());
        if let Some(s) = self.rev_sha_cache.get(&key) { return Ok(s.clone()); }
        if let Some(s) = self.rev_cache.get(&key) { return Ok(s.clone()); }
        // Present rev: unchanged fast path — rev-parse resolves, cache, return
        // the identical sha the caller has always seen.
        if let Some(sha) = Self::rev_parse(repo_root, rev)? {
            self.cache_rev(key, rev, sha.clone());
            return Ok(sha);
        }
        // Miss: the rev isn't in this repo's object db. Offline mode throws
        // without touching the network; otherwise fetch this specific rev
        // on demand and re-resolve exactly once. rev_cache still only records
        // a successful resolution, so a fetched rev is cached like any other.
        if std::env::var_os("DL_NO_FETCH").is_some() {
            bail!("git rev-parse {rev} missing in {} and DL_NO_FETCH is set (offline; would have fetched)",
                repo_root.display());
        }
        // Escalating on-demand fetch, cheapest first, re-resolving after each
        // and stopping at the first hit. rev-parse of a NAME needs the ref to
        // exist locally and the object to be present — a bare `fetch origin
        // <rev>` only writes FETCH_HEAD (lands a full SHA's object, but creates
        // no tag/branch ref), so a tag/branch name needs a step that writes the
        // ref, and a shallow clone whose target is beyond the boundary needs
        // history deepened:
        //   1. `origin <rev>`        — full-SHA fast path (object into the odb).
        //   2. `origin tag <rev>`    — creates refs/tags/<rev> (the tag case).
        //   3. `--tags origin`       — all tag refs (name that step 2 missed).
        //   4. `--unshallow --tags`  — shallow only: deepen full history + tags.
        let mut resolved: Option<String> = None;
        let ladder: [&[&str]; 3] = [
            &["fetch", "--quiet", "origin", rev],
            &["fetch", "--quiet", "origin", "tag", rev],
            &["fetch", "--quiet", "--tags", "origin"],
        ];
        for args in ladder {
            let _ = Command::new("git").arg("-C").arg(repo_root).args(args).output()?;
            if let Some(sha) = Self::rev_parse(repo_root, rev)? { resolved = Some(sha); break; }
        }
        if resolved.is_none() && Self::is_shallow(repo_root)? {
            let _ = Command::new("git").arg("-C").arg(repo_root)
                .args(["fetch", "--quiet", "--unshallow", "--tags", "origin"]).output()?;
            resolved = Self::rev_parse(repo_root, rev)?;
        }
        match resolved {
            Some(sha) => { self.cache_rev(key, rev, sha.clone()); Ok(sha) }
            None => bail!("git rev-parse {rev} still missing in {} after fetching tags/unshallowing from origin",
                repo_root.display()),
        }
    }

    /// `git rev-parse --is-shallow-repository` == "true". A shallow clone may be
    /// missing a target object even after a tag fetch (it lives below the
    /// shallow boundary), so `resolve_rev` deepens with `--unshallow` only when
    /// this holds — `--unshallow` errors on a complete repo.
    fn is_shallow(repo_root: &Path) -> Result<bool> {
        let out = Command::new("git").arg("-C").arg(repo_root)
            .args(["rev-parse", "--is-shallow-repository"]).output()?;
        Ok(out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "true")
    }

    /// `git rev-parse <rev>` in `repo_root`: `Some(sha)` when the rev is present,
    /// `None` when it is missing — the signal `resolve_rev` uses to decide
    /// whether to fetch. Two miss shapes: an unknown ref/name (rev-parse exits
    /// non-zero), and a pinned full SHA whose object is absent (rev-parse echoes
    /// it back regardless, so a `cat-file -e` existence check is required). The
    /// returned value is the PLAIN rev-parse sha — unchanged for a present rev
    /// (an annotated tag stays the tag-object sha, not the peeled commit).
    /// Propagates only a spawn failure (git absent).
    fn rev_parse(repo_root: &Path, rev: &str) -> Result<Option<String>> {
        let out = Command::new("git").arg("-C").arg(repo_root)
            .args(["rev-parse", rev]).output()?;
        if !out.status.success() { return Ok(None); }
        let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
        // Only a hex-SHA rev echoes back from rev-parse WITHOUT proving the
        // object exists, so the existence probe is needed just there. A name
        // (tag/branch/HEAD) only rev-parses when its ref — hence object — is
        // present, so skip the second spawn for it.
        if Self::is_immutable_rev(rev) {
            let present = Command::new("git").arg("-C").arg(repo_root)
                .args(["cat-file", "-e", &sha]).output()?;
            if !present.status.success() { return Ok(None); }
        }
        Ok(Some(sha))
    }

    /// A rev whose object mapping can't change: a full or prefix hex SHA. Movable
    /// refs (branch/tag/HEAD names) are not hex. Gates both the cross-tick cache
    /// and the `cat-file` existence probe.
    fn is_immutable_rev(rev: &str) -> bool {
        rev.len() >= 7 && rev.chars().all(|c| c.is_ascii_hexdigit())
    }

    /// Record a resolution in the cross-tick cache for an immutable SHA, else the
    /// per-tick cache for a movable ref.
    fn cache_rev(&mut self, key: String, rev: &str, sha: String) {
        if Self::is_immutable_rev(rev) { self.rev_sha_cache.insert(key, sha); }
        else { self.rev_cache.insert(key, sha); }
    }

    /// Resolve a `scan` repo coordinate to `(slug, root)`. The slug is the
    /// repo's stable identity in the `_file` cache and the `repo`/`rev`/`file`
    /// relations (the third coordinate alongside path+rev). "." / "" / "self" =
    /// this engine's own repo (slug = root dir name); a config slug names that
    /// repo; otherwise an existing path (slug = its dir name). A config slug
    /// with `allow_missing = true` resolves even when its root is absent: the
    /// caller's `scan` walks a missing dir and gets zero rows.
    fn resolve_repo(&self, repo: &str) -> Result<(String, PathBuf)> {
        if repo.is_empty() || repo == "." || repo == "self" {
            return Ok((self.self_slug(), self.root.clone()));
        }
        if let Some(rc) = self.repos.iter().find(|r| r.slug == repo) {
            self.ensure_cloned_or_missing(rc)?;
            return Ok((rc.slug.clone(), rc.root.clone()));
        }
        let p = PathBuf::from(repo);
        if p.exists() {
            let slug = p.file_name().map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| repo.to_string());
            return Ok((slug, p));
        }
        bail!("unknown repo {repo:?} (expected \".\", a config slug, or an existing path)")
    }

    /// `ensure_cloned` with the `allow_missing` escape hatch. A configured repo
    /// whose root is absent AND `allow_missing = true` resolves to Ok: the
    /// engine prints one stderr line and `scan` against it returns zero rows.
    /// Without the flag, the underlying clone / missing-root error propagates.
    fn ensure_cloned_or_missing(&self, rc: &crate::config::RepoConfig) -> Result<()> {
        match Self::ensure_cloned(rc) {
            Ok(()) => Ok(()),
            Err(e) if rc.allow_missing && !rc.root.exists() => {
                eprintln!(
                    "[missing] repo {:?} (allow_missing); scan returns zero rows. details: {e}",
                    rc.slug,
                );
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Materialize a configured repo whose `root` is not yet on disk by cloning
    /// its `url` (full clone — pinned `(repo, rev)` scans stay deterministic by
    /// OID once cloned). No-op when the root already exists; an error when the
    /// root is missing and no `url` is configured.
    pub fn ensure_cloned(rc: &crate::config::RepoConfig) -> Result<()> {
        if rc.root.exists() { return Ok(()); }
        let Some(url) = rc.url.as_deref() else {
            bail!("repo {:?} root {} does not exist and no url is configured to clone it",
                  rc.slug, rc.root.display());
        };
        if let Some(parent) = rc.root.parent() { std::fs::create_dir_all(parent)?; }
        eprintln!("[clone] {} <- {url}", rc.root.display());
        let out = Command::new("git").args(["clone", url]).arg(&rc.root).output()?;
        if !out.status.success() {
            bail!("git clone {url} into {} failed: {}", rc.root.display(),
                  String::from_utf8_lossy(&out.stderr));
        }
        Ok(())
    }

    /// Expand a `scan` repo coordinate into the concrete `(slug, root)` set to
    /// scan this tick. `"*"` / `"all"` fans out over every configured repo (the
    /// config-folder query: one program, the whole repo set), cloning any that
    /// are not yet on disk; anything else resolves to a single repo. A repo
    /// marked `allow_missing = true` is included even when its root is absent;
    /// its `scan` returns zero rows.
    fn resolve_scan_repos(&self, repo: &str) -> Result<Vec<(String, PathBuf)>> {
        if repo == "*" || repo == "all" {
            if self.repos.is_empty() {
                return Ok(vec![(self.self_slug(), self.root.clone())]);
            }
            let mut out = Vec::with_capacity(self.repos.len());
            for rc in &self.repos {
                self.ensure_cloned_or_missing(rc)?;
                out.push((rc.slug.clone(), rc.root.clone()));
            }
            return Ok(out);
        }
        Ok(vec![self.resolve_repo(repo)?])
    }

    /// Resolve a source rule's scan to its concrete coordinate bindings: one for
    /// a literal-coord scan (the existing shape), or many for a data-driven scan
    /// whose `repo`/`rev` are `Term::Var`. In the variable case the rule's
    /// Pos/Neg/Cmp body atoms are compiled to a SELECT and run over the
    /// previous tick's tables (the coordinate relation is derived, and derived
    /// runs after source — so a data-driven scan reads last tick's coordinates,
    /// one-tick latency, no fixpoint rewrite). Each binding seeds `head_binds`
    /// with the variable slot's value so the rule head can reference the
    /// repo/rev each row was scanned under. Glob stays literal (variable glob is
    /// rejected): the file set varies per (repo, rev) but the pattern is fixed.
    #[tracing::instrument(skip_all, level = "debug")]
    fn resolve_scan_bindings(&mut self, rule: &Rule) -> Result<Vec<ScanBinding>> {
        let spec = scan_spec_of(rule)?;
        let glob = str_of(&spec.glob)?;
        let repo_var: Option<&str> = if let Term::Var(v) = &spec.repo { Some(v.as_str()) } else { None };
        let rev_var: Option<&str> = if let Term::Var(v) = &spec.rev { Some(v.as_str()) } else { None };
        tracing::debug!(head = %rule.head.rel, repo_var = ?repo_var, rev_var = ?rev_var,
            origin = ?rule.origin.as_ref().map(|p| p.to_string_lossy().to_string()), "scan bindings");
        if repo_var.is_none() && rev_var.is_none() {
            let repo_lit = str_of(&spec.repo)?;
            let rev_lit = str_of(&spec.rev)?;
            // A self-form scan (`scan("WORK", …)` / `.` / `""` / `self`) resolves
            // to the rule's own `.git` ancestor when `root_implicit` is set —
            // i.e. ONLY the rootless daemon, whose self.root is a placeholder.
            // Foreground (`--root`/cwd) and LSP (`rootUri`) pass an explicit root
            // that always wins; the script's location is irrelevant there.
            let repo_coord = if self.root_implicit && matches!(repo_lit.as_str(), "." | "" | "self") {
                rule.origin.as_deref()
                    .and_then(crate::repo::nearest_git)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or(repo_lit.clone())
            } else {
                repo_lit.clone()
            };
            tracing::debug!(head = %rule.head.rel, repo_coord = %repo_coord, rev_lit = %rev_lit, "resolved self-form scan");
            let mut out = Vec::new();
            for (slug, root) in self.resolve_scan_repos(&repo_coord)? {
                let rev = self.resolve_rev(&root, &rev_lit)?;
                out.push(ScanBinding { slug, root, rev, glob: glob.clone(), head_binds: vec![] });
            }
            return Ok(out);
        }
        let mut sel_vars: Vec<String> = Vec::new();
        if let Some(v) = repo_var { sel_vars.push(v.to_string()); }
        if let Some(v) = rev_var { sel_vars.push(v.to_string()); }
        let binding_atoms: Vec<BodyItem> = rule.body.iter()
            .filter(|b| matches!(b, BodyItem::Pos(_) | BodyItem::Neg(_) | BodyItem::Cmp(_)))
            .cloned().collect();
        if binding_atoms.is_empty() {
            bail!("data-driven scan needs a coordinate-providing body atom (e.g. \
                   pin(R,V)) binding the variable repo/rev in rule {}", rule.head.rel);
        }
        let sql = crate::lower::lower_gen(&sel_vars, &binding_atoms, &self.rels)?;
        // Collect the coordinate tuples fully (drop the statement borrow) before
        // the resolve loop: resolve_rev takes &mut self (rev_cache).
        let tuples: Vec<Vec<String>> = {
            let conn = self.db.conn();
            let mut s = match conn.prepare(&sql) {
                Ok(s) => s,
                Err(_) => return Ok(Vec::new()),
            };
            let ncol = sel_vars.len();
            let rows = s.query_map([], |r| {
                let mut v = Vec::with_capacity(ncol);
                for i in 0..ncol { v.push(r.get::<_, String>(i)?); }
                Ok(v)
            });
            rows.map(|iter| iter.filter_map(|x| x.ok()).collect()).unwrap_or_default()
        };
        let mut out = Vec::new();
        let repo_lit = str_of(&spec.repo).ok();
        let rev_lit = str_of(&spec.rev).ok();
        for row in tuples {
            let mut col = 0usize;
            let repo_val = if repo_var.is_some() { row[col].clone() } else { repo_lit.clone().unwrap() };
            if repo_var.is_some() { col += 1; }
            let rev_val = if rev_var.is_some() { row[col].clone() } else { rev_lit.clone().unwrap() };
            let mut head_binds: Vec<(String, String)> = Vec::new();
            if let Some(v) = repo_var { head_binds.push((v.to_string(), repo_val.clone())); }
            if let Some(v) = rev_var { head_binds.push((v.to_string(), rev_val.clone())); }
            for (slug, root) in self.resolve_scan_repos(&repo_val)? {
                let rev = self.resolve_rev(&root, &rev_val)?;
                out.push(ScanBinding { slug, root, rev, glob: glob.clone(),
                    head_binds: head_binds.clone() });
            }
        }
        Ok(out)
    }

    /// This engine's root directory (`--root`). The working dir an `@async`
    /// shell effect runs in, so `git`/`gh` commands resolve against the repo.
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
    pub fn source_paths(&self) -> Result<Vec<String>> {
        let conn = self.db.conn();
        let mut s = conn.prepare("SELECT DISTINCT path FROM _file WHERE rev = 'WORK'")?;
        let rows = s.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    }

    /// Row count of a relation's backing table (`rel_<name>`). Test/bench
    /// instrumentation; returns 0 when the table is empty, errors if absent.
    pub fn count_rows(&self, rel: &str) -> Result<i64> {
        let conn = self.db.conn();
        Ok(conn.query_row(&format!("SELECT COUNT(*) FROM {}", tbl(rel)), [], |r| r.get(0))?)
    }

    pub fn query_sql(&self, sql: &str, params: &[serde_json::Value]) -> Result<Vec<Vec<serde_json::Value>>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(sql)?;
        let col_count = stmt.column_count();
        let param_vals: Vec<rusqlite::types::Value> = params.iter().map(|v| match v {
            serde_json::Value::String(s) => rusqlite::types::Value::Text(s.clone()),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() { rusqlite::types::Value::Integer(i) }
                else if let Some(f) = n.as_f64() { rusqlite::types::Value::Real(f) }
                else { rusqlite::types::Value::Null }
            }
            serde_json::Value::Null => rusqlite::types::Value::Null,
            _ => rusqlite::types::Value::Text(v.to_string()),
        }).collect();
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = param_vals.iter()
            .map(|v| v as &dyn rusqlite::types::ToSql).collect();
        let rows_iter = stmt.query_map(param_refs.as_slice(), |row| {
            let mut vals: Vec<serde_json::Value> = Vec::new();
            for i in 0..col_count {
                let v: rusqlite::types::Value = row.get_unwrap(i);
                vals.push(match v {
                    rusqlite::types::Value::Null => serde_json::Value::Null,
                    rusqlite::types::Value::Integer(n) => serde_json::json!(n),
                    rusqlite::types::Value::Real(f) => serde_json::json!(f),
                    rusqlite::types::Value::Text(s) => serde_json::json!(s),
                    rusqlite::types::Value::Blob(b) => serde_json::json!(format!("<blob {}B>", b.len())),
                });
            }
            Ok(vals)
        })?;
        Ok(rows_iter.filter_map(|r| r.ok()).collect())
    }

    /// (file, specifier) for every `use`/`import` row in `module_import`. The
    /// specifier is the resolver's synthesized full path (brace leaves expanded),
    /// which the refactor sink uses to detect imports it cannot yet splice (a
    /// brace leaf's located span covers the leaf name, not the full path).
    pub fn module_imports(&self) -> Result<Vec<(String, String)>> {
        let conn = self.db.conn();
        let mut s = conn.prepare(&format!(
            "SELECT \"file\", \"specifier\" FROM {} WHERE \"kind\" IN ('use', 'import')",
            tbl("module_import")))?;
        let rows = s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    }

    /// (file, decl-name) for every Kotlin same-package implicit ref. These have
    /// no import text to rewrite, so `--move` can only count them loudly.
    pub fn same_package_uses(&self) -> Result<Vec<(String, String)>> {
        let conn = self.db.conn();
        let mut s = conn.prepare(&format!(
            "SELECT \"file\", \"specifier\" FROM {} WHERE \"kind\" = 'same-package'",
            tbl("module_import")))?;
        let rows = s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    }

    /// Read the `diag` relation, if declared, as normalized DiagRows. Maps each
    /// row by column NAME (recognized: path, line, col, end_line, end_col,
    /// severity, msg); missing optional columns take defaults. Returns empty if
    /// the program declares no `diag` relation. Drives LSP publishDiagnostics.
    /// `only` filters to one path (the changed file) when Some.
    pub fn diags(&self, only: Option<&str>) -> Result<Vec<DiagRow>> {
        // `diag` is a fixed-schema built-in (declare_builtins), so the columns
        // and their positions are known. A rule that names only some of them
        // (via head named args) leaves the rest NULL — read NULL-tolerant and
        // apply the same defaults the old by-name reader did (severity "warn",
        // end_line = line, ints 0, empty hint = None).
        let Some(_meta) = self.rels.get("diag") else { return Ok(Vec::new()); };
        let mut sql = format!(
            "SELECT \"path\", \"line\", \"col\", \"end_line\", \"end_col\", \
             \"severity\", \"code\", \"msg\", \"hint\" FROM {}", tbl("diag"));
        if only.is_some() { sql.push_str(" WHERE \"path\" = ?1"); }
        let mut stmt = self.db.conn().prepare(&sql)?;
        let map_row = |row: &rusqlite::Row| -> rusqlite::Result<DiagRow> {
            let text = |i: usize| row.get::<_, rusqlite::types::Value>(i)
                .map(|v| match v {
                    rusqlite::types::Value::Text(s) => s,
                    rusqlite::types::Value::Integer(n) => n.to_string(),
                    _ => String::new(),
                }).unwrap_or_default();
            // NULL (unnamed column) -> None, so a default can fill it.
            let int_opt = |i: usize| row.get::<_, Option<i64>>(i).ok().flatten();
            let line = int_opt(1).unwrap_or(0);
            let sev = text(5);
            Ok(DiagRow {
                path: text(0),
                line,
                col: int_opt(2).unwrap_or(0),
                end_line: int_opt(3).unwrap_or(line),
                end_col: int_opt(4).unwrap_or(0),
                severity: if sev.is_empty() { "warn".into() } else { sev },
                code: text(6),
                msg: text(7),
                hint: { let h = text(8); if h.is_empty() { None } else { Some(h) } },
            })
        };
        let mut out = Vec::new();
        let mut rows = match only {
            Some(p) => stmt.query(rusqlite::params![p])?,
            None => stmt.query([])?,
        };
        while let Some(row) = rows.next()? { out.push(map_row(row)?); }
        // Engine-structural shape diagnostics (Phase 5): not `diag`-rel rows, so
        // append them here — the single read seam --check / --lsp / the daemon
        // schema RPC all go through. Respect the `only` path filter.
        for d in &self.shape_diags {
            if only.map(|p| p == d.path).unwrap_or(true) { out.push(d.clone()); }
        }
        Ok(out)
    }

    /// The extraction type-drop diagnostics collected during the last tick (one
    /// per file+relation that lost rows). The LSP publish path merges these with
    /// the `diag` relation rows so a file whose rows were dropped shows a squiggle.
    /// File-level, line 1 (a row type-failure has no byte span). Cleared at the
    /// start of each tick.
    pub fn extraction_drops(&self) -> &[DiagRow] { &self.extraction_drops }

    /// Push a file-level drop diagnostic for `n` rows lost extracting `rel` from
    /// `path`. `path` is repo-relative (matches `DiagRow.path` and how publish
    /// joins it onto root). Collected, flushed once after the tick.
    fn record_extraction_drop(&mut self, path: &str, rel: &str, n: usize) {
        self.extraction_drops.push(DiagRow {
            path: path.to_string(),
            line: 1, col: 0, end_line: 1, end_col: 0,
            severity: "warn".into(),
            code: "checked-type".into(),
            msg: format!("{n} row(s) failing file/dir/path checks dropped from `{rel}`"),
            hint: None,
        });
    }


    fn ensure_meta(&self) -> Result<()> {
        // Intern-key migration (2026-07-11): `_strings.id` / `_where_bytes.string_id`
        // move from TEXT (decimal StringId::Display) to INTEGER (StringId::sqlite,
        // the i64 bit-pattern lower.rs already compiles literals to). No row-level
        // data migration: an existing TEXT-typed table is DROPPED and recreated
        // empty, then the extraction digests are cleared so the very next tick
        // refills both tables from scratch (every extract:<family> digest folds
        // exe identity already, so a new binary re-extracts regardless).
        {
            let conn = self.db.conn();
            let strings_is_text = conn
                .prepare("SELECT type FROM pragma_table_info('_strings') WHERE name = 'id'")
                .and_then(|mut s| s.query_row([], |r| r.get::<_, String>(0)))
                .map(|t| t.eq_ignore_ascii_case("text"))
                .unwrap_or(false);
            if strings_is_text {
                conn.execute_batch(
                    "DROP TABLE IF EXISTS _strings;
                     DROP TABLE IF EXISTS _where_bytes;
                     DELETE FROM _reldigest WHERE key LIKE 'extract:%';",
                )?;
            }
        }
        self.db.conn().execute_batch(
            "CREATE TABLE IF NOT EXISTS _file (repo TEXT NOT NULL DEFAULT '', path TEXT, rev TEXT, hash TEXT,
                 mtime INTEGER DEFAULT 0, size INTEGER DEFAULT 0, lines INTEGER DEFAULT -1, PRIMARY KEY (repo, path, rev));
             CREATE TABLE IF NOT EXISTS _prov (rel TEXT, repo TEXT NOT NULL DEFAULT '', path TEXT, src TEXT, PRIMARY KEY (rel, repo, path, src));
             CREATE TABLE IF NOT EXISTS _reldigest (rel TEXT PRIMARY KEY, digest TEXT);
             -- P1 (2026-07-10 --check perf defect): one row per derived rel that
             -- has completed a `rebuild_derived` pass, regardless of the row
             -- count it ended with. `any_derived_empty`'s old COUNT(*)-per-rel
             -- probe treated a legitimately-empty derived rel (an inert rail, a
             -- diff view with nothing to report) the same as never derived,
             -- forcing a full rebuild of every derived rel on EVERY tick (154
             -- rels / ~2024 statements measured on a real db). This table lets
             -- `derived_incomplete_rels` tell the two cases apart with one
             -- query instead of N COUNT(*) round trips. Never migrated away on
             -- a rel rename/removal — a stale row for a since-deleted rel is
             -- simply never looked up again.
             CREATE TABLE IF NOT EXISTS _derived_complete (rel TEXT PRIMARY KEY);
             -- Persisted derived shapes (Phase 5): one row per (shape, column) the
             -- `type_decl_row` sink produced last tick. Read at the next tick's
             -- declare to resolve a `rel name: shape.` whose shape was computed,
             -- not written by hand. Digest-guarded full replace (see
             -- persist_type_decl_shapes); the one-tick phase delay.
             CREATE TABLE IF NOT EXISTS _shapes (shape TEXT, pos INTEGER, col TEXT, type TEXT, PRIMARY KEY (shape, pos));
             -- Wall ms of each derived rel's INSERT statements from its most
             -- recent rebuild (max across the rel's rules/passes). Written
             -- batched by rebuild_derived; projected by the perf built-in
             -- stmt_ms so a .dl rail can watch its own rule cost.
             CREATE TABLE IF NOT EXISTS _stmt_ms (rel TEXT PRIMARY KEY, ms INTEGER NOT NULL);
             -- CST node path attribution (not a public rel column): maps a
             -- node id to its source path so the delta refresh can prune one
             -- file's `node` rows. The `node.file` column is a content FileId
             -- shared by byte-identical files, so it cannot key the prune.
             CREATE TABLE IF NOT EXISTS _node_path (id TEXT PRIMARY KEY, path TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS _node_path_path_idx ON _node_path(path);
             -- id = StringId::sqlite() (the i64 bit-pattern of the content-derived
             -- u64 hash) as an INTEGER PRIMARY KEY / rowid alias — single-word int
             -- compares + smaller keys instead of TEXT memcmp on every join probe.
             CREATE TABLE IF NOT EXISTS _strings (
                 id INTEGER PRIMARY KEY,
                 content TEXT NOT NULL,
                 norm TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS _files (
                 id TEXT PRIMARY KEY,
                 content_hash TEXT NOT NULL,
                 path TEXT NOT NULL DEFAULT '',
                 size INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS _where_bytes (
                 id TEXT PRIMARY KEY,
                 string_id INTEGER NOT NULL,
                 file_id TEXT NOT NULL,
                 lo INTEGER NOT NULL,
                 hi INTEGER NOT NULL,
                 repo TEXT NOT NULL DEFAULT '0',
                 rev TEXT NOT NULL DEFAULT '0',
                 path TEXT NOT NULL DEFAULT ''
             );
             CREATE TABLE IF NOT EXISTS _program (
                 path TEXT PRIMARY KEY,
                 hash TEXT NOT NULL DEFAULT '',
                 mtime INTEGER NOT NULL DEFAULT 0,
                 loaded_at INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS _repo (
                 slug TEXT PRIMARY KEY,
                 root TEXT NOT NULL DEFAULT '',
                 url TEXT NOT NULL DEFAULT '',
                 registered_at INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS _ref (
                 repo TEXT NOT NULL,
                 name TEXT NOT NULL,
                 oid TEXT NOT NULL DEFAULT '',
                 observed_at INTEGER NOT NULL DEFAULT 0,
                 PRIMARY KEY (repo, name)
             );
             CREATE TABLE IF NOT EXISTS _rev_log (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 repo TEXT NOT NULL,
                 name TEXT NOT NULL,
                 old TEXT NOT NULL DEFAULT '',
                 new TEXT NOT NULL DEFAULT '',
                 at INTEGER NOT NULL DEFAULT 0
             );
             -- Content-addressed embeddings: one vector per (StringId, backend).
             -- `sid` joins `_strings.id`; `backend` namespaces the model so two
             -- backends coexist without cross-space cosine. `vec` is comma-joined
             -- f32 TEXT (the existing plural Value::Text insert path; the
             -- sqlite-vec ANN mirror is the scale follow-on).
             CREATE TABLE IF NOT EXISTS _embeddings (
                 sid TEXT NOT NULL,
                 backend TEXT NOT NULL,
                 dim INTEGER NOT NULL,
                 vec TEXT NOT NULL,
                 PRIMARY KEY (sid, backend)
             );
             CREATE INDEX IF NOT EXISTS _embeddings_backend_idx ON _embeddings(backend);
             -- Structural node embeddings (node2vec): one vector per node, keyed
             -- by `graph` = the edge rel name a `node2vec(edge)` rule consumed, so
             -- multiple graphs coexist (the `backend` analog for the text path).
             -- `node` is the node id verbatim (a sym / file / whatever the edge
             -- rel carries). `vec` is comma-joined f32 TEXT, same as _embeddings.
             -- `edge_digest` (W2) lets the last N distinct edge-digests of a
             -- graph coexist, so branch A<->B thrash is a cache hit both ways;
             -- `_node_emb_seen` is the per-graph LRU bookkeeping that bounds it.
             CREATE TABLE IF NOT EXISTS _node_embeddings (
                 node TEXT NOT NULL,
                 graph TEXT NOT NULL,
                 edge_digest TEXT NOT NULL DEFAULT '',
                 dim INTEGER NOT NULL,
                 vec TEXT NOT NULL,
                 PRIMARY KEY (node, graph, edge_digest)
             );
             CREATE INDEX IF NOT EXISTS _node_embeddings_graph_idx ON _node_embeddings(graph);
             CREATE INDEX IF NOT EXISTS _node_embeddings_gd_idx ON _node_embeddings(graph, edge_digest);
             CREATE TABLE IF NOT EXISTS _node_emb_seen (
                 graph TEXT NOT NULL,
                 digest TEXT NOT NULL,
                 last_tick INTEGER NOT NULL,
                 PRIMARY KEY (graph, digest)
             );
             CREATE INDEX IF NOT EXISTS _strings_norm_idx ON _strings(norm);
             CREATE INDEX IF NOT EXISTS _where_bytes_string_idx ON _where_bytes(string_id);
             CREATE INDEX IF NOT EXISTS _where_bytes_file_span_idx ON _where_bytes(file_id, lo, hi);
             CREATE INDEX IF NOT EXISTS _where_bytes_path_idx ON _where_bytes(path);
             INSERT OR IGNORE INTO _strings (id, content, norm) VALUES (0, '', '');
             INSERT OR IGNORE INTO _files (id, content_hash, path, size)
                 VALUES ('0', '0000000000000000000000000000000000000000000000000000000000000000', '', 0);
             INSERT OR IGNORE INTO _where_bytes (id, string_id, file_id, lo, hi, repo, rev, path)
                 VALUES ('0', 0, '0', 0, 0, '0', '0', '');
             -- The @next carry clock: one row, k='tx', the current generation.
             -- A @next rule reads carry_<rel> WHERE tx=current and stages rows at
             -- tx=current+1; the counter advances once per tick. See
             -- docs/research-reactive-effectful-datalog.md §8.
             CREATE TABLE IF NOT EXISTS _carry_meta (k TEXT PRIMARY KEY, tx INTEGER NOT NULL DEFAULT 0);
             INSERT OR IGNORE INTO _carry_meta (k, tx) VALUES ('tx', 0);
             -- @async effect queue: one row per outstanding request. `id` =
             -- digest(kind, args_json) so the same request emitted on two ticks
             -- before it runs does not double-fire. `kind` = the response rel
             -- name; `args_json` = the bound-var object; `done` flips to 1 once
             -- the executor has run and the response row is inserted. Off-tick
             -- `drain_effects` is the only writer of `done`. See §8.
             -- `kind` = the effect/`sh` template key (== head rel in the
             -- head-response form, the `sh` decl name in the explicit body-effect
             -- form). `head_rel` = the response rel the head is rebuilt into (they
             -- differ when `gh(..) -> (..)` lands into a differently-named rel).
             -- `full_json` (D-4) is the full body solution, the head-rebuild
             -- payload: the head may mix body vars NOT in the effect args with the
             -- response outs, so the digest keys on `args_json` (the hole map) but
             -- the head is reconstructed from `full_json` ∪ outs in `drain_effects`.
             CREATE TABLE IF NOT EXISTS pending_effect (
                 id TEXT PRIMARY KEY, kind TEXT NOT NULL,
                 head_rel TEXT NOT NULL DEFAULT '', args_json TEXT NOT NULL,
                 full_json TEXT NOT NULL DEFAULT '',
                 req_tx INTEGER NOT NULL, done INTEGER NOT NULL DEFAULT 0,
                 state TEXT NOT NULL DEFAULT 'queued', idem_key TEXT,
                 batch INTEGER NOT NULL DEFAULT 0);
             -- Server query history: one row per daemon `query`/`query_sql` RPC
             -- and LSP `dl/query` request, appended by `Engine::log_query` at the
             -- handler (src/daemon.rs, src/lsp.rs). No primary key: two requests
             -- with identical text within the same nanosecond are both real
             -- events and both land. Append-only by design, no retention/GC.
             -- Projected by the built-in `query_log` relation (src/rels/querylog.rs).
             CREATE TABLE IF NOT EXISTS _query_log (
                 ts TEXT NOT NULL,
                 source TEXT NOT NULL,
                 method TEXT NOT NULL,
                 body TEXT NOT NULL DEFAULT '',
                 params TEXT NOT NULL DEFAULT '[]'
             );"
        )?;
        // tolerate a pending_effect created before the body-effect columns existed.
        // The pre-migration default for head_rel is `kind` (head-response 1:1), set
        // on read in `drain_effects` via the empty-string fallback.
        let _ = self.db.conn().execute(
            "ALTER TABLE pending_effect ADD COLUMN head_rel TEXT NOT NULL DEFAULT ''", []);
        let _ = self.db.conn().execute(
            "ALTER TABLE pending_effect ADD COLUMN full_json TEXT NOT NULL DEFAULT ''", []);
        // Phase 3 job state machine: `state` (queued|running|done|failed) is the
        // reconcile axis; `idem_key` records the `sh!` exactly-once claim. Legacy
        // rows migrate with state derived from `done` below.
        let _ = self.db.conn().execute(
            "ALTER TABLE pending_effect ADD COLUMN state TEXT NOT NULL DEFAULT 'queued'", []);
        let _ = self.db.conn().execute(
            "ALTER TABLE pending_effect ADD COLUMN idem_key TEXT", []);
        // Phase 1b.2 `collect(x)`: a batch request gathers `x` across ALL body
        // solutions and fires ONE effect whose response fans back out (line per
        // entity). `batch=1` tells the drain to split the response into N head
        // rows (run_stream) like a stream, but one-shot (marked done).
        let _ = self.db.conn().execute(
            "ALTER TABLE pending_effect ADD COLUMN batch INTEGER NOT NULL DEFAULT 0", []);
        // A db whose rows predate `state` carry the column default 'queued' even
        // when already drained (done=1); reconcile their state from `done` once.
        let _ = self.db.conn().execute(
            "UPDATE pending_effect SET state = 'done' WHERE done = 1 AND state = 'queued'", []);
        // tolerate dbs created before mtime/size existed
        let _ = self.db.conn().execute("ALTER TABLE _file ADD COLUMN mtime INTEGER DEFAULT 0", []);
        let _ = self.db.conn().execute("ALTER TABLE _file ADD COLUMN size INTEGER DEFAULT 0", []);
        // tolerate dbs created before the line-count column existed; -1 = unknown,
        // reconcile_sources' fast path forces one read+count on the next tick.
        let _ = self.db.conn().execute("ALTER TABLE _file ADD COLUMN lines INTEGER DEFAULT -1", []);
        // tolerate _where_bytes created before the path attribution column existed
        let _ = self.db.conn().execute("ALTER TABLE _where_bytes ADD COLUMN path TEXT NOT NULL DEFAULT ''", []);
        // Re-key `_file` and `_prov` on (repo, ...) for dbs that predate the repo
        // coordinate. SQLite can't ALTER a PK, so rebuild: every old row is this
        // engine's own repo (the only one ever ingested before Phase 2), so stamp
        // its slug. The next reconcile wipes+rewrites `_file` anyway; stamping the
        // real slug keeps that tick's prev/current keys matching (no false churn).
        let slug = self.self_slug();
        if !self.column_exists("_file", "repo")? {
            self.db.conn().execute_batch(&format!(
                "ALTER TABLE _file RENAME TO _file_old;
                 CREATE TABLE _file (repo TEXT NOT NULL DEFAULT '', path TEXT, rev TEXT, hash TEXT,
                     mtime INTEGER DEFAULT 0, size INTEGER DEFAULT 0, lines INTEGER DEFAULT -1, PRIMARY KEY (repo, path, rev));
                 INSERT INTO _file (repo, path, rev, hash, mtime, size)
                     SELECT '{s}', path, rev, hash, mtime, size FROM _file_old;
                 DROP TABLE _file_old;",
                s = slug.replace('\'', "''"),
            ))?;
        }
        if !self.column_exists("_prov", "repo")? {
            self.db.conn().execute_batch(&format!(
                "ALTER TABLE _prov RENAME TO _prov_old;
                 CREATE TABLE _prov (rel TEXT, repo TEXT NOT NULL DEFAULT '', path TEXT, src TEXT,
                     PRIMARY KEY (rel, repo, path, src));
                 INSERT INTO _prov (rel, repo, path, src)
                     SELECT rel, '{s}', path, src FROM _prov_old;
                 DROP TABLE _prov_old;",
                s = slug.replace('\'', "''"),
            ))?;
        }
        // _node_embeddings gained an edge_digest column (W2 vector cache). It is
        // a pure derived cache (vectors re-embed on the next tick), so an old
        // single-digest table is dropped and rebuilt empty, not data-migrated.
        if !self.column_exists("_node_embeddings", "edge_digest")? {
            self.db.conn().execute_batch(
                "DROP TABLE IF EXISTS _node_embeddings;
                 CREATE TABLE _node_embeddings (
                     node TEXT NOT NULL, graph TEXT NOT NULL,
                     edge_digest TEXT NOT NULL DEFAULT '',
                     dim INTEGER NOT NULL, vec TEXT NOT NULL,
                     PRIMARY KEY (node, graph, edge_digest));
                 CREATE INDEX IF NOT EXISTS _node_embeddings_graph_idx ON _node_embeddings(graph);
                 CREATE INDEX IF NOT EXISTS _node_embeddings_gd_idx ON _node_embeddings(graph, edge_digest);")?;
        }
        Ok(())
    }

    /// Order-independent content digest of a relation: XOR-fold of the per-row
    /// `__src` hashes in `rel_<rel>`. The table is a set (PK on user cols), so
    /// each `__src` contributes once and XOR cannot cancel a duplicate; XOR is
    /// commutative + associative, so insert order does not matter. All-zero ⇒
    /// empty relation. Same row set ⇒ same digest; different rows ⇒ different
    /// (blake3). Lets a comment-only edit (bytes move, rows don't) skip rebuild.
    /// Does `table` already have a column named `col`? Used to gate one-shot
    /// schema migrations (a fresh db gets the new schema from `CREATE TABLE IF
    /// NOT EXISTS`; an old db keeps its columns and needs the rebuild).
    fn column_exists(&self, table: &str, col: &str) -> Result<bool> {
        let n: i64 = self.db.conn().query_row(
            &format!("SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name = ?1", table.replace('\'', "''")),
            [col], |r| r.get(0))?;
        Ok(n > 0)
    }

    /// Load the persisted derived shapes from `_shapes` (Phase 5): shape name ->
    /// its columns in `pos` order. A `type` value that names a base type builds a plain
    /// column; anything else is a validated brand name (checked at persist time),
    /// so it lands a TEXT column carrying that brand. Read at the START of a tick
    /// (declare) to resolve a computed `rel name: shape.`.
    fn load_persisted_shapes(&self) -> Result<HashMap<String, Vec<Col>>> {
        let mut out: HashMap<String, Vec<Col>> = HashMap::new();
        let mut stmt = self.db.conn()
            .prepare("SELECT shape, col, type FROM _shapes ORDER BY shape, pos")?;
        let rows = stmt.query_map([], |r| Ok((
            r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
        for row in rows {
            let (shape, col, ty) = row?;
            let column = match Type::parse(&ty) {
                Some(base) => Col::plain(col, base),
                // A validated brand name (persist checked it): enum brands store
                // TEXT with the brand attached. A `<: int` brand base is not
                // reconstructed here (lands TEXT) — cosmetic, rules over the
                // derived rel type-checked at load when its cols were empty.
                None => Col::branded(&col, &ty),
            };
            out.entry(shape).or_default().push(column);
        }
        Ok(out)
    }

    /// Resolve every deferred `rel name: shape.` (shape_ref still set, cols empty)
    /// against the persisted derived shapes (Phase 5). Syntax `type name(...)`
    /// shapes already won at load (their refs are resolved before the engine sees
    /// the decl), so a ref still unresolved here is derived-only. Fills
    /// `self.rels[name]` via `declare` (which migrates a `rel_<name>` table on
    /// column drift and deletes its `_reldigest` row so it re-derives), or records
    /// a `shape-pending` info diag. A persisted shape that shares a name with a
    /// syntax shape records `shape-shadowed` (syntax won, the derived rows are
    /// ignored). Called at the top of a tick after the normal declare loop.
    fn resolve_derived_shapes(&mut self, prog: &Program) -> Result<()> {
        let deferred: Vec<(String, String)> = prog.items.iter().filter_map(|it| match it {
            Item::Rel(d) => d.shape_ref.as_ref().map(|s| (d.name.clone(), s.clone())),
            _ => None,
        }).collect();
        if deferred.is_empty() && !type_decl_row_used(prog) { return Ok(()); }
        let persisted = self.load_persisted_shapes()?;
        let builtins = builtin_rel_names();
        for (rel_name, shape) in &deferred {
            if builtins.contains(rel_name) { continue; } // exotic `rel diag: x` — leave the builtin alone
            match persisted.get(shape) {
                Some(cols) => {
                    let d = RelDecl { name: rel_name.clone(), cols: cols.clone(), ..Default::default() };
                    self.declare(&d)?;
                }
                None => self.shape_diags.push(DiagRow {
                    path: "(shapes)".into(), line: 1, col: 0, end_line: 1, end_col: 0,
                    severity: "info".into(), code: "shape-pending".into(),
                    msg: format!("rel `{rel_name}`: shape `{shape}` has no syntax `type {shape}(...)` \
                        decl and no derived rows yet — it derives from type_decl_row and becomes \
                        available on the next tick"),
                    hint: None,
                }),
            }
        }
        // A syntax `type X(...)` shadows a derived shape of the same name: syntax
        // won for any `rel _: X`, so the derived rows are unused. Warn once.
        let syntax_shapes: std::collections::HashSet<&str> = prog.items.iter()
            .filter_map(|it| if let Item::Shape(s) = it { Some(s.name.as_str()) } else { None })
            .collect();
        for shape in persisted.keys() {
            if syntax_shapes.contains(shape.as_str()) {
                self.shape_diags.push(DiagRow {
                    path: "(shapes)".into(), line: 1, col: 0, end_line: 1, end_col: 0,
                    severity: "warn".into(), code: "shape-shadowed".into(),
                    msg: format!("shape `{shape}` is declared both as a syntax `type {shape}(...)` \
                        and derived via type_decl_row; the syntax decl wins and the derived rows \
                        are ignored"),
                    hint: None,
                });
            }
        }
        Ok(())
    }

    /// Persist the `type_decl_row` sink's rows to `_shapes` (Phase 5), at the END
    /// of a tick (after the derived fixpoint filled `rel_type_decl_row`). Digest-
    /// guarded on the sink's content (a `shape:type_decl_row` key in `_reldigest`)
    /// so an unchanged sink does NOT re-persist or re-migrate every tick (the
    /// repo's recompute-guard rail). Each row's `type` must name a base type, an
    /// ambient builtin enum brand, or a program-declared brand; an unknown type
    /// records a `shape-unknown-type` warn and that whole shape is dropped from the
    /// persist (it stays pending). Full replace, batched (no per-row write).
    fn persist_type_decl_shapes(&mut self, prog: &Program) -> Result<()> {
        if !type_decl_row_used(prog) { return Ok(()); }

        // Valid ty vocabulary: base types + ambient builtin enum brands + program brands.
        let prog_brands: std::collections::HashSet<&str> = prog.items.iter()
            .filter_map(|it| if let Item::Brand(b) = it { Some(b.name.as_str()) } else { None })
            .collect();
        let ty_ok = |ty: &str| Type::parse(ty).is_some()
            || builtin_enum_variants(ty).is_some()
            || prog_brands.contains(ty);

        let mut stmt = self.db.conn()
            .prepare(&format!("SELECT shape, pos, col, type FROM {} ORDER BY shape, pos", tbl("type_decl_row")))?;
        let raw: Vec<(String, i64, String, String)> = stmt.query_map([], |r| Ok((
            r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .filter_map(|x| x.ok()).collect();
        drop(stmt);

        // Validate ty every tick (cheap, O(rows)) so the shape-unknown-type diag is
        // steady, not just on the tick the sink changed. Drop any shape carrying
        // an unknown type (loud diag), keep the rest.
        let mut bad_shapes: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (shape, _, _, ty) in &raw {
            if !ty_ok(ty) && bad_shapes.insert(shape.clone()) {
                self.shape_diags.push(DiagRow {
                    path: "(shapes)".into(), line: 1, col: 0, end_line: 1, end_col: 0,
                    severity: "warn".into(), code: "shape-unknown-type".into(),
                    msg: format!("derived shape `{shape}` names an unknown type `{ty}` — use a base \
                        type (text/int/path/file/dir/repo/rev) or a declared brand; the shape stays pending"),
                    hint: None,
                });
            }
        }
        // The WRITE is the recompute-guarded step: gate the DELETE + insert on the
        // sink's content digest so an unchanged sink does not re-migrate every tick
        // (the repo's recompute-guard rail).
        let digest = self.rel_content_digest("type_decl_row", &self.rels["type_decl_row"].clone())?;
        if self.load_rel_digest("shape:type_decl_row")? == Some(digest) { return Ok(()); }
        let rows: Vec<Vec<Value>> = raw.iter()
            .filter(|(shape, _, _, _)| !bad_shapes.contains(shape))
            .map(|(shape, pos, col, ty)| vec![
                Value::Text(shape.clone()), Value::Int(*pos),
                Value::Text(col.clone()), Value::Text(ty.clone())])
            .collect();
        self.db.conn().execute("DELETE FROM _shapes", [])?;
        self.db.insert_rows("_shapes", &["shape", "pos", "col", "type"], &rows)?;
        self.save_rel_digest("shape:type_decl_row", &digest)?;
        Ok(())
    }

    fn rel_digest(&self, rel: &str) -> Result<[u8; 32]> {
        let mut acc = [0u8; 32];
        let mut stmt = self.db.conn().prepare(&format!("SELECT __src FROM {}", tbl(rel)))?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let src: String = row.get(0).unwrap_or_default();
            if let Ok(bytes) = hex_to_32(&src) {
                for (a, b) in acc.iter_mut().zip(bytes.iter()) { *a ^= *b; }
            }
        }
        Ok(acc)
    }

    /// Order-independent content digest of a rel's LIVE table over its declared
    /// columns (not `__src`, which carry-loaded rows leave blank). Per-row blake3,
    /// XOR-folded so row order does not matter; relations are sets (PK-deduped) so
    /// no two rows are identical and the XOR never self-cancels. Used by
    /// `load_carry` to tell whether a carried rel actually moved this tick.
    fn rel_content_digest(&self, rel: &str, meta: &RelMeta) -> Result<[u8; 32]> {
        let sql = if meta.cols.is_empty() {
            format!("SELECT COUNT(*) FROM {}", tbl(rel))
        } else {
            let cl = meta.cols.iter().map(|c| format!("\"{}\"", c.name))
                .collect::<Vec<_>>().join(", ");
            format!("SELECT {cl} FROM {}", tbl(rel))
        };
        self.digest_of_query(&sql, [])
    }

    /// Whether the @next carry staged at `tx` differs from `rel`'s live rows —
    /// the non-destructive twin of `load_carry` (which applies the carry as its
    /// only mode). Used by the settle report to peek "will next tick move".
    fn carry_differs(&self, rel: &str, meta: &RelMeta, tx: i64) -> Result<bool> {
        let live = self.rel_content_digest(rel, meta)?;
        let cl = if meta.cols.is_empty() {
            "COUNT(*)".to_string()
        } else {
            meta.cols.iter().map(|c| format!("\"{}\"", c.name))
                .collect::<Vec<_>>().join(", ")
        };
        let sql = format!("SELECT {cl} FROM {} WHERE tx = ?1", carry_tbl(rel));
        let staged = self.digest_of_query(&sql, [tx])?;
        Ok(live != staged)
    }

    /// Order-independent (XOR-folded) content digest of a query's rows. Shared
    /// by `rel_content_digest` and `carry_differs`.
    fn digest_of_query(&self, sql: &str, params: impl rusqlite::Params) -> Result<[u8; 32]> {
        let mut acc = [0u8; 32];
        let mut stmt = self.db.conn().prepare(sql)?;
        let ncol = stmt.column_count();
        let mut rows = stmt.query(params)?;
        while let Some(row) = rows.next()? {
            let mut h = blake3::Hasher::new();
            for i in 0..ncol {
                match row.get::<_, rusqlite::types::Value>(i)? {
                    rusqlite::types::Value::Integer(n) => { h.update(b"i"); h.update(&n.to_le_bytes()); }
                    rusqlite::types::Value::Real(f) => { h.update(b"r"); h.update(&f.to_le_bytes()); }
                    rusqlite::types::Value::Text(s) => { h.update(b"t"); h.update(s.as_bytes()); }
                    rusqlite::types::Value::Blob(b) => { h.update(b"b"); h.update(&b); }
                    rusqlite::types::Value::Null => { h.update(b"n"); }
                }
                h.update(&[0]);
            }
            let d = h.finalize();
            for (a, b) in acc.iter_mut().zip(d.as_bytes().iter()) { *a ^= *b; }
        }
        Ok(acc)
    }

    fn load_rel_digest(&self, rel: &str) -> Result<Option<[u8; 32]>> {
        let hex: Option<String> = self.db.conn()
            .query_row("SELECT digest FROM _reldigest WHERE rel = ?1", [rel], |r| r.get(0))
            .ok();
        Ok(hex.and_then(|h| hex_to_32(&h).ok()))
    }

    fn save_rel_digest(&self, rel: &str, digest: &[u8; 32]) -> Result<()> {
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        self.db.conn().execute(
            "INSERT INTO _reldigest(rel, digest) VALUES (?1, ?2)
             ON CONFLICT(rel) DO UPDATE SET digest = excluded.digest",
            rusqlite::params![rel, hex])?;
        Ok(())
    }

    /// Digest each relation's source-rule TEXT (not row content): extraction
    /// rows are a function of (file content, rule), so an edited regex/glob/
    /// capture invalidates the per-file hash fast path in `reconcile_sources`
    /// even though no file moved. XOR-fold of per-rule blake3 over the Debug
    /// repr, so rule order within a relation is irrelevant. Stored in
    /// `_reldigest` under a `src:` key (distinct namespace from row-content
    /// digests). Returns (dirty rels, pending saves); the caller persists the
    /// saves only after re-extraction lands, so a failed tick retries.
    fn source_rule_digests(&self, source_rules: &[&Rule])
        -> Result<(HashSet<String>, Vec<(String, [u8; 32])>)>
    {
        let mut by_rel: HashMap<String, [u8; 32]> = HashMap::new();
        for r in source_rules {
            let h = blake3::hash(format!("{r:?}").as_bytes());
            let acc = by_rel.entry(r.head.rel.clone()).or_insert([0u8; 32]);
            for (a, b) in acc.iter_mut().zip(h.as_bytes()) { *a ^= b; }
        }
        let mut dirty = HashSet::new();
        let mut pending = Vec::new();
        for (rel, d) in by_rel {
            let key = format!("src:{rel}");
            if self.load_rel_digest(&key)? != Some(d) {
                dirty.insert(rel);
                pending.push((key, d));
            }
        }
        Ok((dirty, pending))
    }

    /// Drop from `changed` every relation whose freshly computed digest equals
    /// its stored digest (the file's bytes moved but the extracted rows did
    /// not). Records the new digest for the relations that really changed.
    /// This is v4's `Replay` short-circuit at relation granularity.
    fn prune_unchanged_by_digest(&self, changed: HashSet<String>) -> Result<HashSet<String>> {
        let mut out = HashSet::new();
        for rel in changed {
            let d_new = self.rel_digest(&rel)?;
            if self.load_rel_digest(&rel)? == Some(d_new) { continue; }
            self.save_rel_digest(&rel, &d_new)?;
            out.insert(rel);
        }
        Ok(out)
    }

    /// Seed `_reldigest` for every source relation, so the first delta after a
    /// cold run has a baseline to compare against. Returns the relations whose
    /// digest MOVED against the stored baseline (first-ever seeding counts as
    /// moved) — the full tick's per-rel change attribution, feeding the same
    /// `affected_derived` scoping `tick_paths` uses (perf gap B). An unchanged
    /// relation skips the save.
    fn seed_rel_digests(&self, source_rels: &[String]) -> Result<Vec<String>> {
        let mut moved = Vec::new();
        for rel in source_rels {
            let d = self.rel_digest(rel)?;
            if self.load_rel_digest(rel)? == Some(d) { continue; }
            self.save_rel_digest(rel, &d)?;
            moved.push(rel.clone());
        }
        Ok(moved)
    }

    /// P1 fix: which of `derived_rels` have NEVER completed a `rebuild_derived`
    /// pass (no `_derived_complete` marker) — the honest "must full-rebuild"
    /// signal. The old `any_derived_empty` asked "is this rel's table empty
    /// right now", which is also true for a rel a rule legitimately derived to
    /// zero rows (an inert rail, a diff view with nothing this tick), forcing
    /// a full rebuild of every derived rel on every subsequent tick. This is
    /// ONE query (load every completed marker into a set) instead of a
    /// `COUNT(*)` round trip per rel.
    fn derived_incomplete_rels(&self, derived_rels: &[String]) -> Result<Vec<String>> {
        if derived_rels.is_empty() { return Ok(Vec::new()); }
        let mut complete: HashSet<String> = HashSet::new();
        let mut stmt = self.db.conn().prepare("SELECT rel FROM _derived_complete")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            complete.insert(row.get::<_, String>(0)?);
        }
        Ok(derived_rels.iter().filter(|r| !complete.contains(r.as_str())).cloned().collect())
    }

    /// Mark every rel in `derived_rels` as having completed a rebuild pass —
    /// called once at the end of `rebuild_derived` for exactly the rels it was
    /// asked to rebuild (whatever row count they end with, including zero).
    /// `INSERT OR IGNORE` via the plural `insert_rows` seam, so this is one
    /// statement (chunked), never a per-rel write.
    fn mark_derived_complete(&self, derived_rels: &[String]) -> Result<()> {
        if derived_rels.is_empty() { return Ok(()); }
        let rows: Vec<Vec<Value>> = derived_rels.iter().map(|r| vec![Value::Text(r.clone())]).collect();
        self.db.insert_rows("_derived_complete", &["rel"], &rows)?;
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(n_rules = source_rules.len()), level = "debug")]
    fn reconcile_sources(&mut self, source_rules: &[&Rule], source_rels: &[String],
        consumed: &HashSet<String>) -> Result<Reconcile> {
        // Load prior file metadata first so enumerate can use the mtime fast-path.
        let prev = self.load_file_meta()?;

        let mut current: FileMeta = HashMap::new();
        // (rule idx, repo slug, path, rev, hash) for every enumerated file. A
        // single rule scanning `"*"` fans out to one batch of rows per config
        // repo, all carrying the same rule idx but distinct repo slugs.
        let mut rule_files: Vec<(usize, String, String, String, String, Vec<(String, String)>)> = Vec::new();
        // slug -> on-disk root for every repo touched this tick; parse_file reads
        // content from the matching root and the slug stamps `_file`/`_prov` so
        // two repos sharing a path stay distinct.
        let mut root_by_repo: HashMap<String, PathBuf> = HashMap::new();
        // Group rules by (slug, rev) so one repo×rev walks/ls-trees ONCE no
        // matter how many rules scan it (the old shape re-walked per rule —
        // rules × repos walks across a big config). Clones + rev-parse stay in
        // this serial loop (rev_cache needs &mut self); the walks parallelize.
        // Each entry carries `head_binds`: the data-driven coord values that
        // produced this (slug, rev), so the rule head can reference the repo/rev
        // variable each file was scanned under (empty for a literal-coord scan).
        let mut groups: BTreeMap<(String, String), (PathBuf, Vec<(usize, String, Vec<(String, String)>)>)> = BTreeMap::new();
        for (idx, rule) in source_rules.iter().enumerate() {
            for b in self.resolve_scan_bindings(rule)? {
                root_by_repo.insert(b.slug.clone(), b.root.clone());
                groups.entry((b.slug, b.rev)).or_insert_with(|| (b.root, Vec::new()))
                    .1.push((idx, b.glob, b.head_binds));
            }
        }
        let group_list: Vec<(&(String, String), &(PathBuf, Vec<(usize, String, Vec<(String, String)>)>))> = groups.iter().collect();
        let enumerated: Vec<Result<Vec<(String, String, i64, i64, i64)>>> = group_list.par_iter()
            .map(|((slug, rev), (repo_root, rules))| {
                let t = std::time::Instant::now();
                let mut union = globset::GlobSetBuilder::new();
                for (_, g, _) in rules { union.add(globset::Glob::new(g)?); }
                let files = enumerate_with_hash(slug, repo_root, rev, &union.build()?, &prev)?;
                if crate::db::profiling() {
                    eprintln!("[scan {slug}@{}] {} file(s) in {:.1}ms",
                        if rev == "WORK" { "WORK" } else { &rev[..rev.len().min(8)] },
                        files.len(), t.elapsed().as_secs_f64() * 1000.0);
                }
                Ok(files)
            }).collect();
        for (((slug, rev), (_, rules)), files) in group_list.iter().zip(enumerated) {
            let matchers: Vec<(usize, globset::GlobMatcher, Vec<(String, String)>)> = rules.iter()
                .map(|(idx, g, hb)| Ok((*idx, globset::Glob::new(g)?.compile_matcher(), hb.clone())))
                .collect::<Result<_>>()?;
            for (path, h, mt, sz, lines) in files? {
                current.insert((slug.clone(), path.clone(), rev.clone()), (h.clone(), mt, sz, lines));
                for (idx, m, hb) in &matchers {
                    if m.is_match(&path) {
                        rule_files.push((*idx, slug.clone(), path.clone(), rev.clone(), h.clone(), hb.clone()));
                    }
                }
            }
        }
        self.rev_index = current.keys().map(|(repo, p, r)| (repo.clone(), r.clone(), p.clone())).collect();

        // Zero-match diagnostic (v3/v4 parity): a scan rule that matched no files
        // is almost always a glob/root mismatch, which otherwise fails silently as
        // "0 rows" far downstream. Warn with the rule, glob, and where it looked so
        // the miss is self-diagnosing instead of a mystery.
        //
        // Softened for two expected-empty shapes so a helper-in-progress isn't
        // noisy: (a) POLYGLOT SIBLING — another scan heading the SAME rel matched
        // (e.g. `seen` scanned for both Rust and `{ts,tsx}`, and this repo has no
        // TS); the rel already has rows, so the empty glob is intentional
        // fan-out → silent. (b) CONSUMED — the rel feeds a downstream rule; the
        // author wired it up, an empty tick is transient → one quiet line, no
        // fix-it note. Only a genuinely dead scan (unmatched, no sibling, unread)
        // gets the loud two-line "check your glob/root" warning.
        let matched: HashSet<usize> = rule_files.iter().map(|(idx, ..)| *idx).collect();
        let rel_matched: HashSet<&str> = source_rules.iter().enumerate()
            .filter(|(idx, _)| matched.contains(idx))
            .map(|(_, r)| r.head.rel.as_str()).collect();
        for (idx, rule) in source_rules.iter().enumerate() {
            if matched.contains(&idx) { continue; }
            let rel = rule.head.rel.as_str();
            if rel_matched.contains(rel) { continue; } // (a) sibling glob matched — silent
            let Ok(spec) = scan_spec_of(rule) else { continue };
            let Term::Str(glob) = &spec.glob else { continue };
            let targets: Vec<String> = groups.iter()
                .filter(|(_, (_, rules))| rules.iter().any(|(i, _, _)| *i == idx))
                .map(|((slug, rev), (root, _))| {
                    let r = if rev == "WORK" { "WORK" } else { &rev[..rev.len().min(8)] };
                    format!("{slug}@{r} ({})", root.display())
                })
                .collect();
            let where_ = if targets.is_empty() { "no repo/rev resolved".into() } else { targets.join(", ") };
            if consumed.contains(rel) {
                // (b) consumed helper — quiet, no fix-it note.
                eprintln!("[dl] source `{rel}` matched 0 files this tick: scan(\"{glob}\") under {where_} (feeds a rule — transient if mid-edit)");
                continue;
            }
            eprintln!("[dl] source `{rel}` matched 0 files: scan(\"{glob}\") under {where_}", );
            // The glob matches paths relative to the working root (the cwd `dl`
            // ran in). The usual miss is an anchored glob (`src/…`) run from ABOVE
            // the repo, or a rev with no such path. `*` already crosses `/`, so
            // recursion is not the issue.
            eprintln!("       note: the glob matches paths relative to the working root; run `dl` from the repo (its cwd is the root — there is no --root) and check the leading path segments match");
        }

        let hash_of = |m: &FileMeta, repo: &str, p: &str, r: &str|
            m.get(&(repo.to_string(), p.to_string(), r.to_string())).map(|t| t.0.clone());

        // An edited source rule must re-extract files whose content did not
        // change. A dirty rel widens retraction to its whole file set; the new
        // digests persist only after the re-extraction lands (end of this fn).
        let (dirty_rels, pending_digests) = self.source_rule_digests(source_rules)?;

        // Retraction key is (repo, path): `_prov` prunes by that pair, so two
        // repos at the same path do not retract each other's source rows.
        let mut to_retract: HashSet<(String, String)> = HashSet::new();
        for ((repo, path, rev), (h, _, _, _)) in &current {
            if hash_of(&prev, repo, path, rev).as_ref() != Some(h) {
                to_retract.insert((repo.clone(), path.clone()));
            }
        }
        for (repo, path, _rev) in prev.keys() {
            if !current.contains_key(&(repo.clone(), path.clone(), _rev.clone())) {
                to_retract.insert((repo.clone(), path.clone()));
            }
        }
        for (idx, repo, path, _rev, _h, _hb) in &rule_files {
            if dirty_rels.contains(&source_rules[*idx].head.rel) {
                to_retract.insert((repo.clone(), path.clone()));
            }
        }

        let retract_list: Vec<(&str, &str)> = to_retract.iter()
            .map(|(repo, p)| (repo.as_str(), p.as_str())).collect();
        let retracted = self.retract_paths(&retract_list, source_rels)?;

        // Extract any file whose path was retracted, not just hash-moved ones:
        // retraction is path-grain across ALL source rels, so a clean rule
        // sharing a path with a dirty one must re-provide its rows too.
        let to_extract: Vec<(usize, String, String, String, String, Vec<(String, String)>)> = rule_files.iter()
            .filter(|(_, repo, p, r, h, _)| hash_of(&prev, repo, p, r).as_ref() != Some(h)
                || to_retract.contains(&(repo.clone(), p.clone())))
            .map(|(idx, repo, p, r, h, hb)| (*idx, repo.clone(), p.clone(), r.clone(), h.clone(), hb.clone()))
            .collect();
        let parsed = to_extract.len();

        // Parse + extract in parallel across files (CPU-bound, no DB touch),
        // then insert serially (SQLite is single-writer).
        let results: Vec<Result<(String, String, Vec<Vec<Value>>, Vec<(spine::WhereBytes, String)>, usize)>> = {
            let Engine { rels, rev_index, .. } = &*self;
            to_extract.par_iter().map(|(idx, repo, path, rev, hash, hb)| {
                let root = root_by_repo.get(repo)
                    .ok_or_else(|| anyhow::anyhow!("no root for repo {repo}"))?;
                let (rows, where_bytes, dropped) =
                    parse_file(source_rules[*idx], repo, path, rev, hash, root, rels, rev_index, hb)?;
                let rel = source_rules[*idx].head.rel.clone();
                Ok((rel, path.clone(), rows, where_bytes, dropped))
            }).collect()
        };

        let mut by_rel: HashMap<String, Vec<(String, String, Vec<Value>)>> = HashMap::new();
        let mut where_bytes: Vec<(String, String, spine::WhereBytes, Option<String>)> = Vec::new();
        for (res, (_, repo, _, _, _, _)) in results.into_iter().zip(to_extract.iter()) {
            let (rel, path, rows, wheres, dropped) = res?;
            self.dropped += dropped;
            if dropped > 0 { self.record_extraction_drop(&path, &rel, dropped); }
            where_bytes.extend(wheres.into_iter().map(|(w, t)| (repo.clone(), path.clone(), w, Some(t))));
            by_rel.entry(rel).or_default()
                .extend(rows.into_iter().map(|row| (repo.clone(), path.clone(), row)));
        }

        let mut extracted = 0usize;
        for (rel, rows) in by_rel {
            let meta = self.rels.get(&rel)
                .ok_or_else(|| anyhow::anyhow!("unknown head relation {}", rel))?.clone();
            extracted += self.insert_source_rows_for_paths(&rel, &meta, &rows)?;
        }
        self.insert_spine_where_bytes(&where_bytes)?;

        self.save_file_meta(&current, &prev)?;
        for (key, d) in &pending_digests { self.save_rel_digest(key, d)?; }
        Ok(Reconcile {
            changed: retracted > 0 || extracted > 0,
            extracted,
            retracted,
            parsed,
            total: rule_files.len(),
        })
    }

    fn retract_path(&self, repo: &str, path: &str, source_rels: &[String]) -> Result<usize> {
        self.retract_paths(&[(repo, path)], source_rels)
    }

    /// Retract every row sourced only from these `(repo, path)` pairs. Prune
    /// `_prov` for all pairs first, then run the orphan sweep once per relation
    /// (not once per pair): a row survives iff some remaining path still provides
    /// its `__src`. Turns the old O(paths x rels x table) into O(rels x table).
    /// Keying by `(repo, path)` keeps two repos sharing a path from retracting
    /// each other's source rows.
    fn retract_paths(&self, paths: &[(&str, &str)], source_rels: &[String]) -> Result<usize> {
        if paths.is_empty() { return Ok(0); }
        self.db.exec("CREATE TEMP TABLE IF NOT EXISTS _retract_path(repo TEXT, path TEXT, PRIMARY KEY (repo, path))")?;
        self.db.exec("DELETE FROM _retract_path")?;
        let path_rows: Vec<Vec<Value>> = paths.iter()
            .map(|(repo, p)| vec![Value::Text((*repo).to_string()), Value::Text((*p).to_string())]).collect();
        self.db.insert_rows("_retract_path", &["repo", "path"], &path_rows)?;
        self.db.exec(
            "DELETE FROM _prov WHERE (repo, path) IN (SELECT repo, path FROM _retract_path)")?;
        // Drop located rows attributed to these (repo, path) pairs; fresh spans
        // re-insert on reparse. Sentinel row has path '' and is never retracted.
        // Keying by (repo, path) keeps two config repos sharing a path from
        // retracting each other's located rows.
        self.db.exec(
            "DELETE FROM _where_bytes WHERE (repo, path) IN (SELECT repo, path FROM _retract_path)")?;
        let mut removed = 0usize;
        for rel in source_rels {
            let rel_lit = rel.replace('\'', "''");
            let sql = format!(
                "DELETE FROM {} WHERE __src NOT IN (SELECT src FROM _prov WHERE rel = '{rel_lit}')",
                tbl(rel),
            );
            removed += self.db.exec(&sql)?;
        }
        Ok(removed)
    }

    fn load_file_meta(&self) -> Result<FileMeta> {
        let mut stmt = self.db.conn().prepare("SELECT repo, path, rev, hash, mtime, size, lines FROM _file")?;
        let rows = stmt.query_map([], |r| Ok((
            (r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?),
            (r.get::<_, String>(3)?, r.get::<_, i64>(4)?, r.get::<_, i64>(5)?, r.get::<_, i64>(6)?),
        )))?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    }

    /// Persist the `_file` cache DIFFERENTIALLY: delete keys that vanished or
    /// changed, insert keys that changed or are new. A warm no-change tick
    /// writes zero rows (the old shape rewrote the whole table every tick —
    /// O(total files) of churn per tick across a big repo config). The spine
    /// `_files` insert rides the same delta: content rows are INSERT-only and
    /// content-addressed, so unchanged keys need no re-touch.
    fn save_file_meta(&self, current: &FileMeta, prev: &FileMeta) -> Result<()> {
        let mut delta: FileMeta = HashMap::new();
        let mut stale: Vec<Vec<Value>> = Vec::new();
        for (k, v) in current {
            if prev.get(k) != Some(v) {
                delta.insert(k.clone(), v.clone());
                if prev.contains_key(k) {
                    stale.push(vec![Value::Text(k.0.clone()), Value::Text(k.1.clone()), Value::Text(k.2.clone())]);
                }
            }
        }
        for k in prev.keys() {
            if !current.contains_key(k) {
                stale.push(vec![Value::Text(k.0.clone()), Value::Text(k.1.clone()), Value::Text(k.2.clone())]);
            }
        }
        if !stale.is_empty() {
            self.db.exec("CREATE TEMP TABLE IF NOT EXISTS _stale_file(repo TEXT, path TEXT, rev TEXT, PRIMARY KEY (repo, path, rev))")?;
            self.db.exec("DELETE FROM _stale_file")?;
            self.db.insert_rows("_stale_file", &["repo", "path", "rev"], &stale)?;
            self.db.exec("DELETE FROM _file WHERE (repo, path, rev) IN (SELECT repo, path, rev FROM _stale_file)")?;
        }
        let rows: Vec<Vec<Value>> = delta.iter().map(|((repo, path, rev), (h, mt, sz, lines))| vec![
            Value::Text(repo.clone()),
            Value::Text(path.clone()),
            Value::Text(rev.clone()),
            Value::Text(h.clone()),
            Value::Int(*mt),
            Value::Int(*sz),
            Value::Int(*lines),
        ]).collect();
        self.db.insert_rows("_file", &["repo", "path", "rev", "hash", "mtime", "size", "lines"], &rows)?;
        self.insert_spine_files(&delta)?;
        Ok(())
    }

    fn insert_spine_files(&self, current: &FileMeta) -> Result<usize> {
        let mut by_id: BTreeMap<String, (String, String, i64)> = BTreeMap::new();
        for ((_repo, path, _rev), (hash, _mt, size, _lines)) in current {
            let Some(id) = spine::FileId::from_content_address(hash, *size) else { continue };
            if id == spine::FileId::SYNTHETIC { continue; }
            let entry = by_id.entry(id.to_string()).or_insert_with(|| (hash.clone(), path.clone(), *size));
            if path < &entry.1 {
                entry.1 = path.clone();
            }
        }
        let file_rows: Vec<Vec<Value>> = by_id.into_iter()
            .map(|(id, (content_hash, path, size))| {
                vec![Value::Text(id), Value::Text(content_hash), Value::Text(path), Value::Int(size)]
            })
            .collect();
        self.db.insert_rows("_files", &["id", "content_hash", "path", "size"], &file_rows)
    }

    fn declare(&mut self, d: &RelDecl) -> Result<()> {
        // Port envelope check: a `@in(class)`/`@out(class)` rel must carry the
        // class's contract columns BY NAME (order-free, extra columns rejected
        // for now — the drain reads the envelope, nothing else). The class is
        // the contract, never a transport; binding happens at the CLI (--mcp).
        if let Some(p) = &d.port {
            let dir = match p.dir { crate::ast::PortDir::In => "@in", crate::ast::PortDir::Out => "@out" };
            let Some(env) = crate::ast::Port::envelope(&p.class, p.dir) else {
                bail!("rel {}: unknown port class {dir}({}); `rpc` is the only class today \
                       (stream/duplex are reserved)", d.name, p.class);
            };
            for (cname, cty) in env {
                match d.cols.iter().find(|c| c.name == *cname) {
                    Some(c) if c.ty == *cty => {}
                    Some(c) => bail!("rel {}: {dir}({}) needs column {cname}: {}, found {cname}: {}",
                        d.name, p.class, cty.sql().to_lowercase(), c.ty.sql().to_lowercase()),
                    None => bail!("rel {}: {dir}({}) needs column {cname}: {}",
                        d.name, p.class, cty.sql().to_lowercase()),
                }
            }
            if d.cols.len() != env.len() {
                let extra: Vec<&str> = d.cols.iter()
                    .filter(|c| !env.iter().any(|(n, _)| c.name == *n))
                    .map(|c| c.name.as_str()).collect();
                bail!("rel {}: {dir}({}) allows only the envelope columns ({}); extra: {}",
                    d.name, p.class,
                    env.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", "),
                    extra.join(", "));
            }
        }
        // Migrate a stale cached table whose column set OR primary key no longer
        // matches the decl. Two triggers:
        //   1. Column-set drift (e.g. a release added a leading column) — the
        //      next `refresh_rel` insert would fail "no column named ...".
        //   2. Key-set drift — a hot-reload that ADDS/REMOVES/CHANGES a
        //      `key(...)` qualifier leaves the old PRIMARY KEY in place (a bare
        //      `CREATE TABLE IF NOT EXISTS` keeps the cached shape). The lattice
        //      upsert then targets `ON CONFLICT(key)` against a full-row PK that
        //      does not match, and every subsequent tick fails "ON CONFLICT
        //      clause does not match any PRIMARY KEY or UNIQUE constraint",
        //      wedging the daemon. A merge-fn-only change (MaxBy(a) -> MaxBy(b))
        //      keeps the same PK column set, so it does NOT drop here — the
        //      upsert SQL is regenerated by `lower_rule` every tick.
        // Rel tables are derived (or source rows reconciled every tick), so
        // dropping loses nothing — reconcile / rebuild_derived refills.
        if !d.cols.is_empty() {
            let table = tbl(&d.name);
            let want: Vec<String> = d.cols.iter().map(|c| c.name.clone()).collect();
            // The PK the decl wants: the `key(...)` subset, else the full row.
            let want_pk: Vec<String> = match &d.key {
                Some(k) => k.clone(),
                None => want.clone(),
            };
            let (have, have_pk): (Vec<String>, Vec<String>) = {
                let conn = self.db.conn();
                let mut have = Vec::new();
                // (pk_position, column) for columns in the existing PRIMARY KEY.
                let mut pk_pos: Vec<(i64, String)> = Vec::new();
                if let Ok(mut s) = conn.prepare(&format!("PRAGMA table_info({table})")) {
                    // PRAGMA table_info columns: 1=name, 5=pk (1-based position
                    // in the primary key, 0 if the column is not part of it).
                    if let Ok(rows) = s.query_map([], |r| {
                        Ok((r.get::<_, String>(1)?, r.get::<_, i64>(5)?))
                    }) {
                        for (name, pk) in rows.flatten() {
                            if name == "__src" { continue; }
                            if pk > 0 { pk_pos.push((pk, name.clone())); }
                            have.push(name);
                        }
                    }
                }
                pk_pos.sort_by_key(|(p, _)| *p);
                (have, pk_pos.into_iter().map(|(_, c)| c).collect())
            };
            // ON CONFLICT matches a constraint by column SET, order-free — so
            // compare PK as sorted sets (a pure column reorder is not a drift).
            let pk_set = |mut v: Vec<String>| { v.sort(); v };
            let key_drift = pk_set(have_pk.clone()) != pk_set(want_pk.clone());
            if !have.is_empty() && (have != want || key_drift) {
                self.db.conn().execute(&format!("DROP TABLE IF EXISTS {table}"), [])?;
                self.db.conn().execute(&format!("DELETE FROM _reldigest WHERE rel = ?1"),
                    rusqlite::params![d.name])?;
                // P1 interaction: before the completion-marker fix, a dropped
                // derived table read back as 0 rows, and `any_derived_empty`
                // treated that as "must full-rebuild" — the (accidental) thing
                // that actually refilled `d.name` after this migration. Now
                // that a legitimately-empty derived rel does NOT force a full
                // rebuild, the stale completion marker must be invalidated
                // here explicitly, or a rel that was already marked complete
                // before the drop would read as "done" against its freshly
                // empty, never-refilled table. `_derived_complete` may have no
                // row for `d.name` (a source rel, or a derived rel that never
                // completed a pass yet) — the DELETE is then simply a no-op.
                self.db.conn().execute("DELETE FROM _derived_complete WHERE rel = ?1",
                    rusqlite::params![d.name])?;
            }
        }
        let sql = if d.cols.is_empty() {
            // Zero-column relation (the built-in `true()` singleton): one row,
            // no user columns. SQLite needs at least one column, so the table
            // carries only the universal `__src` sentinel.
            format!("CREATE TABLE IF NOT EXISTS {} (__src TEXT DEFAULT '')", tbl(&d.name))
        } else {
            let cols: Vec<String> = d.cols.iter()
                .map(|c| format!("\"{}\" {}", c.name, c.ty.sql())).collect();
            // The PRIMARY KEY drives dedup. The default (no `key(...)`) is the
            // full row, so identical rows collapse (set semantics). A `key(...)`
            // qualifier narrows the PK to that column subset = a functional
            // dependency / choice-domain (Soufflé APLAS'21): one row per key,
            // first-wins under `INSERT OR IGNORE`, or lattice-merged under a
            // `merge(...)`. Validate the key/merge columns exist and that the
            // merge col is NOT a key col (it ranks rows within a key).
            let pk: Vec<String> = if let Some(key) = &d.key {
                for k in key {
                    if !d.cols.iter().any(|c| &c.name == k) {
                        bail!("key column {k} not in rel {}", d.name);
                    }
                }
                key.iter().map(|c| format!("\"{c}\"")).collect()
            } else {
                d.cols.iter().map(|c| format!("\"{}\"", c.name)).collect()
            };
            if let Some(crate::ast::MergeFn::MaxBy(mc)) = &d.merge {
                let key = d.key.as_ref()
                    .ok_or_else(|| anyhow::anyhow!("rel {} has merge(...) without key(...)", d.name))?;
                if !d.cols.iter().any(|c| &c.name == mc) {
                    bail!("merge column {mc} not in rel {}", d.name);
                }
                if key.contains(mc) {
                    bail!("rel {}: merge column {mc} is also a key column; the merge ranks rows WITHIN a key", d.name);
                }
            }
            format!(
                "CREATE TABLE IF NOT EXISTS {} ({}, __src TEXT DEFAULT '', PRIMARY KEY ({}))",
                tbl(&d.name), cols.join(", "), pk.join(", ")
            )
        };
        self.db.conn().execute(&sql, [])?;
        self.rels.insert(d.name.clone(), RelMeta { cols: d.cols.clone(), key: d.key.clone(), merge: d.merge.clone(), port: d.port.clone() });
        Ok(())
    }

    /// Create the join-key indexes derived rules need (see auto_indexes). Skips
    /// closure heads, which are views. Idempotent (CREATE INDEX IF NOT EXISTS).
    fn create_auto_indexes(&self, derived_rules: &[&Rule], closures: &HashMap<String, String>) -> Result<()> {
        for (rel, col) in auto_indexes(derived_rules, &self.rels) {
            if closures.contains_key(&rel) { continue; }
            let ix = format!("idx_{rel}_{col}");
            self.db.conn().execute(
                &format!("CREATE INDEX IF NOT EXISTS \"{ix}\" ON {}(\"{col}\")", tbl(&rel)), [])?;
        }
        Ok(())
    }

    /// Declare every relation: closure heads become a VIEW over the condensation,
    /// everything else a base table.
    fn declare_all(&mut self, prog: &Program, closures: &HashMap<String, String>) -> Result<()> {
        // Discovery (`.dl/*.dl`) merges several files into one program, so the
        // same relation may be declared in more than one file. An identical
        // re-declaration is a no-op; a conflicting shape is an error.
        let mut seen: HashMap<String, Vec<crate::ast::Col>> = HashMap::new();
        for item in &prog.items {
            if let Item::Rel(d) = item {
                // A deferred `rel name: shape.` (shape_ref still set, cols empty)
                // is a computed shape: don't declare a zero-column table here.
                // `resolve_derived_shapes` (after builtins) fills it from _shapes
                // or records shape-pending. (Frontend already resolved syntax
                // shapes; a ref surviving to here is derived-only.)
                if d.shape_ref.is_some() { continue; }
                if let Some(prev) = seen.get(&d.name) {
                    if *prev == d.cols { continue; }
                    bail!("rel {} declared twice with different columns", d.name);
                }
                seen.insert(d.name.clone(), d.cols.clone());
                if BUILTIN_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in relation (true/repo/rev/content/file); pick another name", d.name);
                }
                if MODULE_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in module-graph relation; pick another name", d.name);
                }
                if TYPE_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in type-graph relation (type_edge / type_edge_rev / type_entity / type_entity_rev / type_sig / type_link / type_link_rev); pick another name", d.name);
                }
                if DOC_TEXT_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in doc relation (doc_comment / doc_tag); pick another name", d.name);
                }
                if CONST_VALUE_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in const-value relation (const_value / const_value_rev); pick another name", d.name);
                }
                if COMMENT_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in comment relation (comment_node); pick another name", d.name);
                }
                if TEMPLATE_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in template-literal relation (template_parts); pick another name", d.name);
                }
                if UNRESOLVED_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in unresolved-marker relation (unresolved); pick another name", d.name);
                }
                if CALL_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in call-graph relation (call_def / call_def_rev / call_site / call_edge / call_edge_rev / call_name / call_kind); pick another name", d.name);
                }
                if DATAFLOW_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in dataflow relation (df_node / df_node_rev / df_node_repo / df_node_repo_rev / df_edge / loop_over / allocates / nest / df_param / df_arg / df_arg_rev / df_field / df_field_rev / df_lit / df_lit_rev); pick another name", d.name);
                }
                if DOC_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in document relation (doc_node / doc_ref); pick another name", d.name);
                }
                if SPINE_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in ref-spine relation (string / ref); pick another name", d.name);
                }
                if NODE_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in CST relation (node / child); pick another name", d.name);
                }
                for k in crate::rels::rel_kinds() {
                    if k.rels().contains(&d.name.as_str()) {
                        bail!("{} is {}; pick another name", d.name, k.reserved_msg());
                    }
                }
                if DAEMON_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in daemon-state relation (program / head / rev_advanced); pick another name", d.name);
                }
                if EVERY_RELS.contains(&d.name.as_str()) {
                    bail!("{} is the built-in clock relation (every); pick another name", d.name);
                }
                if CLOCK_RELS.contains(&d.name.as_str()) {
                    bail!("{} is the built-in clock relation (clock); pick another name", d.name);
                }
                if HOOK_RELS.contains(&d.name.as_str()) {
                    bail!("{} is the built-in harness-hook event log (hook_event); pick another name", d.name);
                }
                if DIAG_RELS.contains(&d.name.as_str()) {
                    bail!("diag is the built-in diagnostic sink (fixed schema: path, line, col, end_line, end_col, severity, code, msg, hint); drop the `rel diag(...)` decl and write it directly — name only the columns you use, e.g. `diag(path: p, line: l, msg: m) <- ...`");
                }
                if HOVER_RELS.contains(&d.name.as_str()) {
                    bail!("hover_note is the built-in hover-note sink (fixed schema: path, line, col, end_line, end_col, md); drop the `rel hover_note(...)` decl and head it directly from a rule, like diag — name only the columns you use, e.g. `hover_note(path: p, line: l, end_line: l, end_col: c, md: text) <- ...`");
                }
                if GRAPH_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in drawable-graph sink (graph_node(id, label, kind[, file, line, parent]) / graph_edge(src, dst, kind)); drop the `rel {}(...)` decl and head it directly from a rule, like diag — name only the columns you use", d.name, d.name);
                }
                if MUTE_RELS.contains(&d.name.as_str()) {
                    bail!("diag_mute is the built-in diagnostic-mute set (code); it is written only by the LSP toggle command, never a rule — pick another name");
                }
                if DEMAND_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in demand sink (scip_want / rev_cmp_want / def_target / effect_cmd / checkout) — drop the `rel {}(...)` decl and head it directly from a rule, like diag/repo", d.name, d.name);
                }
                if CHECKOUT_OUT_RELS.contains(&d.name.as_str()) {
                    bail!("checkout_done is the built-in checkout-sweep outcome (repo, branch, action, ok, detail); it is written by the `checkout` sink, so READ it — do not `rel`-declare or head it");
                }
                if TYPE_DECL_RELS.contains(&d.name.as_str()) {
                    bail!("type_decl_row is the built-in derived-shape sink (shape, pos, col, type); drop the `rel type_decl_row(...)` decl and head it directly from a derived rule, like diag/graph_node");
                }
                match closures.get(&d.name) {
                    Some(edge) => self.declare_closure(d, edge)?,
                    None => self.declare(d)?,
                }
            }
        }
        self.declare_builtins()?;
        // Phase 5: resolve any computed `rel name: shape.` against the shapes the
        // `type_decl_row` sink persisted on a prior tick. Runs after builtins so a
        // shape rel can be declared alongside them; the one-tick phase delay makes
        // the derive -> persist -> resolve loop invisible under the daemon.
        self.resolve_derived_shapes(prog)?;
        Ok(())
    }

    /// Register and create the built-in relation tables (repo/rev/content/file).
    /// Reuses `declare`, so they get the same `rel_<name>` table shape and a
    /// `self.rels` entry, which is what lets `lower_rule` join body atoms against
    /// them. Populated by `refresh_builtin_rels`, not by source rules.
    fn declare_builtins(&mut self) -> Result<()> {
        for d in builtin_rel_decls() { self.declare(&d)?; }
        for d in module_rel_decls() { self.declare(&d)?; }
        for d in type_rel_decls() { self.declare(&d)?; }
        for d in doc_text_rel_decls() { self.declare(&d)?; }
        for d in const_value_rel_decls() { self.declare(&d)?; }
        for d in comment_rel_decls() { self.declare(&d)?; }
        for d in template_rel_decls() { self.declare(&d)?; }
        for d in unresolved_rel_decls() { self.declare(&d)?; }
        for d in call_rel_decls() { self.declare(&d)?; }
        for d in dataflow_rel_decls() { self.declare(&d)?; }
        for d in doc_rel_decls() { self.declare(&d)?; }
        for d in spine_rel_decls() { self.declare(&d)?; }
        for d in node_rel_decls() { self.declare(&d)?; }
        // Optional point/containment index: "innermost CST node covering byte C
        // in file F" is `node(_, _, F, lo, hi, _), lo <= C, C < hi` — a range
        // scan on (file, lo, hi) instead of a full `node` table scan. Mirrors
        // `_where_bytes_file_span_idx`. The closure(child) path is still the
        // pick for full-ancestry materialization (measured); this just makes
        // the LSP-common point query first-class. Idempotent.
        self.db.conn().execute(
            &format!("CREATE INDEX IF NOT EXISTS node_file_span_idx ON {}(\"file\", \"lo\", \"hi\")", tbl("node")), [])?;
        for d in crate::rels::rel_kind_decls() { self.declare(&d)?; }
        for d in daemon_rel_decls() { self.declare(&d)?; }
        for d in every_rel_decls() { self.declare(&d)?; }
        for d in clock_rel_decls() { self.declare(&d)?; }
        for d in effect_rel_decls() { self.declare(&d)?; }
        for d in hook_rel_decls() { self.declare(&d)?; }
        for d in diag_rel_decls() { self.declare(&d)?; }
        for d in hover_note_rel_decls() { self.declare(&d)?; }
        for d in graph_rel_decls() { self.declare(&d)?; }
        for d in diag_mute_rel_decls() { self.declare(&d)?; }
        for d in demand_rel_decls() { self.declare(&d)?; }
        for d in checkout_out_rel_decls() { self.declare(&d)?; }
        for d in type_decl_rel_decls() { self.declare(&d)?; }
        Ok(())
    }

    /// Rebuild the built-in relations from the `_file` cache. Wholesale wipe +
    /// repopulate (bounded by repo size, one row per tracked file). Stage 1:
    /// repo = one row from `--root`; rev.id/file.rev = the raw rev string;
    /// content.id = the content hash. No interning (Stage 2).
    #[tracing::instrument(skip_all, level = "debug")]
    fn refresh_builtin_rels(&self) -> Result<()> {
        let mut sel = self.db.conn().prepare("SELECT repo, path, rev, hash FROM _file")?;
        let files: Vec<(String, String, String, String)> = sel
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .filter_map(|x| x.ok()).collect();
        let t = |s: &str| Value::Text(s.to_string());
        // slug -> on-disk root for the repos we can name (self + config). Fills
        // the `repo` relation's root column; an ingested-by-path repo not in
        // either set gets ''.
        let mut root_of: HashMap<String, String> = HashMap::new();
        root_of.insert(self.self_slug(), self.root.to_string_lossy().to_string());
        for rc in &self.repos {
            root_of.insert(rc.slug.clone(), rc.root.to_string_lossy().to_string());
        }
        // `rev`/`content` dedup by their own key (id / hash). `rev.id` is the rev
        // string, so a `WORK` shared across repos folds to one row (the `repo`
        // column then names just the first); committed revs are unique shas.
        let mut revs: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        let mut contents: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        let mut file_rows: Vec<Vec<Value>> = Vec::new();
        let mut repo_slugs: BTreeSet<String> = BTreeSet::new();
        for (repo, path, rev, hash) in files {
            revs.entry(rev.clone()).or_insert_with(|| vec![t(&rev), t(&repo), t(&rev), Value::Int(0)]);
            contents.entry(hash.clone()).or_insert_with(|| vec![t(&hash), t(&hash)]);
            file_rows.push(vec![t(&repo), t(&rev), t(&path), t(&hash)]);
            repo_slugs.insert(repo);
        }
        // `repo` lists the configured repos when a config is loaded; otherwise
        // every repo actually ingested into `_file`, or `--root` if nothing has
        // been scanned yet. A configured repo whose root is missing is omitted
        // (the `allow_missing` flag keeps the engine alive past it; the absence
        // here is what lets a program write `!repo(S, _, _)` to surface misses).
        let repo_rows: Vec<Vec<Value>> = if !self.repos.is_empty() {
            self.repos.iter()
                .filter(|r| r.root.exists())
                .map(|r| {
                    vec![t(&r.slug), t(&r.root.to_string_lossy()),
                         t(&r.url.clone().unwrap_or_default())]
                }).collect()
        } else {
            if repo_slugs.is_empty() { repo_slugs.insert(self.self_slug()); }
            repo_slugs.iter().map(|slug| {
                let root = root_of.get(slug).cloned().unwrap_or_default();
                vec![t(slug), t(&root), t("")]
            }).collect()
        };
        let revs: Vec<Vec<Value>> = revs.into_values().collect();
        let contents: Vec<Vec<Value>> = contents.into_values().collect();
        self.refresh_rel("repo", &["slug", "root", "url"], &repo_rows)?;
        self.refresh_rel("rev", &["id", "repo", "oid", "ts"], &revs)?;
        self.refresh_rel("content", &["id", "hash"], &contents)?;
        self.refresh_rel("file", &["repo", "rev", "path", "content"], &file_rows)?;
        // The `true()` singleton: always exactly one zero-column row. The range
        // anchor for negation-only rules (`diag(...) <- true(), !rel(_,_,_).`).
        self.db.exec(&format!("DELETE FROM {}", tbl("true")))?;
        self.db.exec(&format!("INSERT OR IGNORE INTO {} DEFAULT VALUES", tbl("true")))?;
        Ok(())
    }

    /// Populate the `every` clock relation for this tick. For each interval `N`
    /// the program names, `secs=N` lands a row IFF the current wall-second is in a
    /// different `N`-bucket than the last time `N` fired (so it fires on the first
    /// tick and once per boundary crossing thereafter, exact regardless of tick
    /// cadence). The last-fired bucket per `N` is stored in `_carry_meta` under
    /// `every:N`. Wholesale wipe: the rel is ephemeral, not derived.
    /// Returns true if the rel's content changed this tick (a row landed, or rows
    /// cleared after the previous tick had some) — the incremental path uses it to
    /// re-derive rules that join `every`.
    fn refresh_every(&self, intervals: &[i64]) -> Result<bool> {
        use rusqlite::OptionalExtension;
        let before: i64 = self.db.conn().query_row(
            &format!("SELECT COUNT(*) FROM {}", tbl("every")), [], |r| r.get(0))?;
        self.db.exec(&format!("DELETE FROM {}", tbl("every")))?;
        let now = now_secs();
        let mut rows: Vec<Vec<Value>> = Vec::new();
        for &n in intervals {
            if n <= 0 { continue; }
            let bucket = now / n;
            let key = format!("every:{n}");
            let prev: Option<i64> = self.db.conn().query_row(
                "SELECT tx FROM _carry_meta WHERE k = ?1", [&key], |r| r.get(0)).optional()?;
            if prev != Some(bucket) {
                rows.push(vec![Value::Int(n)]);
                self.db.conn().execute(
                    "INSERT INTO _carry_meta (k, tx) VALUES (?1, ?2) \
                     ON CONFLICT(k) DO UPDATE SET tx = ?2",
                    rusqlite::params![key, bucket])?;
            }
        }
        let landed = rows.len();
        self.db.insert_rows(&tbl("every"), &["secs"], &rows)?;
        Ok(landed > 0 || before > 0)
    }

    /// Populate `clock(secs, bucket)` with the CURRENT bucket `now / secs` for each
    /// named period — one persistent row per period, every tick (unlike `every`'s
    /// edge-trigger). Skips the write when no bucket moved, returning whether the
    /// content changed so the incremental path re-derives rules that join it.
    fn refresh_clock(&self, periods: &[i64]) -> Result<bool> {
        let now = now_secs();
        let mut want: Vec<(i64, i64)> =
            periods.iter().filter(|&&n| n > 0).map(|&n| (n, now / n)).collect();
        want.sort();
        want.dedup();
        let have: Vec<(i64, i64)> = {
            let conn = self.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"secs\", \"bucket\" FROM {} ORDER BY \"secs\", \"bucket\"",
                tbl("clock")))?;
            let v: Vec<(i64, i64)> = s
                .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?
                .filter_map(|x| x.ok())
                .collect();
            v
        };
        if have == want { return Ok(false); }
        let rows: Vec<Vec<Value>> =
            want.into_iter().map(|(s, b)| vec![Value::Int(s), Value::Int(b)]).collect();
        self.refresh_rel("clock", &["secs", "bucket"], &rows)?;
        Ok(true)
    }



    /// Wholesale replace one engine-owned relation through the same plural write
    /// seam every built-in module/indexer uses.
    pub(crate) fn refresh_rel(&self, rel: &str, cols: &[&str], rows: &[Vec<Value>]) -> Result<usize> {
        let table = tbl(rel);
        self.db.exec(&format!("DELETE FROM {table}"))?;
        self.db.insert_rows(&table, cols, rows)
    }



    /// Append one row to the server-request history (`_query_log`), the meta
    /// table the built-in `query_log` relation projects (`src/rels/querylog.rs`).
    /// Called once per request from the daemon's `query`/`query_sql` RPC
    /// handlers and the LSP's `dl/query` handler — a single-row insert through
    /// the plural `Db::insert_rows` seam (same shape as the `pending_effect`
    /// job queue), never a raw per-row `conn()` write. `source` is which server
    /// ("daemon"/"lsp"); `method` is the RPC/request method name; `body` is the
    /// SQL text (empty for the plain `query` RPC, which carries no SQL param);
    /// `params` is the JSON array text of bound parameters ("[]" when none).
    /// Append-only, no retention: a polling reader (the flow panel's
    /// auto-refresh) querying `query_log` logs its own read as a new row too —
    /// intentional self-noise, not a bug.
    pub fn log_query(&self, source: &str, method: &str, body: &str, params: &str) -> Result<()> {
        let row = vec![vec![
            Value::Text(iso8601_utc_now()),
            Value::Text(source.to_string()),
            Value::Text(method.to_string()),
            Value::Text(body.to_string()),
            Value::Text(params.to_string()),
        ]];
        self.db.insert_rows("_query_log", &["ts", "source", "method", "body", "params"], &row)?;
        Ok(())
    }

    /// Persist the loaded `.dl` program file set into `_program` (wipe + insert,
    /// plural seam). Each row is (path, content hash, mtime); `loaded_at` stamps
    /// the flush. Diffable on restart against the new file set. The daemon calls
    /// this on cold tick and after a hot reload.
    pub fn save_program_meta(&self, files: &[PathBuf]) -> Result<()> {
        let now = unix_secs();
        let mut rows: Vec<Vec<Value>> = Vec::with_capacity(files.len());
        for f in files {
            let (hash, mtime) = match std::fs::read(f) {
                Ok(bytes) => {
                    let mt = std::fs::metadata(f).ok().map(|m| mtime_secs(&m)).unwrap_or(0);
                    (blake3::hash(&bytes).to_hex().to_string(), mt)
                }
                Err(_) => (String::new(), 0),
            };
            rows.push(vec![
                Value::Text(f.to_string_lossy().into_owned()),
                Value::Text(hash),
                Value::Int(mtime),
                Value::Int(now),
            ]);
        }
        self.db.exec("DELETE FROM _program")?;
        self.db.insert_rows("_program", &["path", "hash", "mtime", "loaded_at"], &rows)?;
        Ok(())
    }

    /// Persist the registered repo set into `_repo` (wipe + insert). `registered_at`
    /// stamps the flush; the daemon calls this when it loads or reloads config so
    /// a restart can diff the previously-registered repos against the new set.
    pub fn save_repos_meta(&self) -> Result<()> {
        let now = unix_secs();
        let rows: Vec<Vec<Value>> = self.repos.iter().map(|rc| vec![
            Value::Text(rc.slug.clone()),
            Value::Text(rc.root.to_string_lossy().into_owned()),
            Value::Text(rc.url.clone().unwrap_or_default()),
            Value::Int(now),
        ]).collect();
        self.db.exec("DELETE FROM _repo")?;
        self.db.insert_rows("_repo", &["slug", "root", "url", "registered_at"], &rows)?;
        Ok(())
    }

    /// Snapshot of the registered repo set (config + dynamically pulled). The
    /// daemon diffs this after a tick to add notify watches on newly-pulled
    /// roots, so edits in a dynamically-reached repo react.
    pub fn snapshot_repos(&self) -> Vec<crate::config::RepoConfig> {
        self.repos.clone()
    }

    /// Inject one inbound rpc request into an `@in(rpc)` port rel (the serving
    /// loop's pre-tick write). `id` is the raw JSON serialization of the
    /// request id (int or string), so it round-trips exactly. The rel must
    /// already be declared (the priming tick declares every program rel), so
    /// injection never races the schema.
    pub fn inject_rpc(&mut self, rel: &str, id: &str, method: &str, params: &str) -> Result<()> {
        if !self.rels.contains_key(rel) {
            bail!("@in(rpc) rel {rel} is not declared; run a tick before injecting");
        }
        self.db.insert_rows(&tbl(rel), &["id", "method", "params"],
            &[vec![Value::Text(id.into()), Value::Text(method.into()), Value::Text(params.into())]])?;
        Ok(())
    }

    /// Append one harness-hook event to the built-in `hook_event` rel. Rows
    /// accumulate (facts in the db, no retention sweep); the tick's content
    /// digest (`hook:hook_event`) re-derives dependents on a new row. The rel
    /// must already be declared (a priming tick declares every built-in), so the
    /// insert never races the schema.
    pub fn insert_hook_event(&mut self, kind: &str, session: &str, seq: i64, json: &str) -> Result<()> {
        if !self.rels.contains_key("hook_event") {
            bail!("hook_event rel is not declared; run a tick before feeding an event");
        }
        self.db.insert_rows(&tbl("hook_event"), &["kind", "session", "seq", "json"],
            &[vec![Value::Text(kind.into()), Value::Text(session.into()),
                   Value::Int(seq), Value::Text(json.into())]])?;
        Ok(())
    }

    /// Toggle a diagnostic code in the built-in `diag_mute` set: insert the row
    /// if absent (returns `true` = now muted), delete it if present (returns
    /// `false` = now unmuted). Persisted in the db, so a mute survives a daemon
    /// restart. Written out-of-tick, never by a refresh; the LSP publish seam
    /// reads the set to drop muted `diag` rows. `--check` never consults it.
    pub fn toggle_diag_mute(&mut self, code: &str) -> Result<bool> {
        if !self.rels.contains_key("diag_mute") {
            bail!("diag_mute rel is not declared; run a tick before toggling a mute");
        }
        let already: i64 = self.db.conn().query_row(
            &format!("SELECT COUNT(*) FROM {} WHERE \"code\" = ?1", tbl("diag_mute")),
            rusqlite::params![code], |r| r.get(0))?;
        if already > 0 {
            self.db.conn().execute(
                &format!("DELETE FROM {} WHERE \"code\" = ?1", tbl("diag_mute")),
                rusqlite::params![code])?;
            Ok(false)
        } else {
            self.db.insert_rows(&tbl("diag_mute"), &["code"],
                &[vec![Value::Text(code.into())]])?;
            Ok(true)
        }
    }

    /// The set of currently-muted diagnostic codes (the `diag_mute` rows). The
    /// LSP publish path filters `diag` rows against this before sending them.
    pub fn muted_codes(&self) -> Result<std::collections::HashSet<String>> {
        if !self.rels.contains_key("diag_mute") {
            return Ok(std::collections::HashSet::new());
        }
        let conn = self.db.conn();
        let mut stmt = conn.prepare(&format!("SELECT \"code\" FROM {}", tbl("diag_mute")))?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    }

    /// Every distinct diagnostic code currently in the `diag` relation, paired
    /// with whether it is muted. Powers the editor quick-pick behind
    /// `dl.listDiagCodes`. Codes that appear only in the mute set (muted but no
    /// live finding) are included too, so a user can un-mute a code with no
    /// current occurrences.
    pub fn diag_code_states(&self) -> Result<Vec<(String, bool)>> {
        let muted = self.muted_codes()?;
        let mut codes: std::collections::BTreeSet<String> = muted.iter().cloned().collect();
        if self.rels.contains_key("diag") {
            let conn = self.db.conn();
            let mut stmt = conn.prepare(
                &format!("SELECT DISTINCT \"code\" FROM {} WHERE \"code\" IS NOT NULL AND \"code\" != ''", tbl("diag")))?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            for c in rows.filter_map(|x| x.ok()) { codes.insert(c); }
        }
        Ok(codes.into_iter().map(|c| { let m = muted.contains(&c); (c, m) }).collect())
    }

    /// Drain an `@out(rpc)` port rel: return its rows, clear the table, and
    /// retire the answered rows from the paired `@in(rpc)` rel (drain law 1:
    /// every answered request row is consumed). Rows are produced by the
    /// fixpoint, pushed to the transport, deleted. The NEXT request's rebuild
    /// no longer rides "the out rel is empty" (P1 retired that signal —
    /// `derived_incomplete_rels` marks a legitimately-empty derived rel as
    /// complete, not "never derived"); instead `inject_rpc`'s write to the
    /// `@in(rpc)` rel is itself content-digested (a `port:` key in
    /// `_reldigest`, tick.rs) like `async:`/`hook:`, so the fresh request row
    /// is what re-derives the out rel's dependents.
    pub fn drain_rpc(&mut self, out_rel: &str, in_rel: &str) -> Result<Vec<(String, String)>> {
        if !self.rels.contains_key(out_rel) { return Ok(Vec::new()); }
        let rows: Vec<(String, String)> = {
            let conn = self.db.conn();
            let mut s = conn.prepare(&format!("SELECT id, result FROM {}", tbl(out_rel)))?;
            let rows = s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .filter_map(|x| x.ok()).collect();
            conn.execute(&format!("DELETE FROM {}", tbl(out_rel)), [])?;
            rows
        };
        let ids: Vec<String> = rows.iter().map(|(id, _)| id.clone()).collect();
        self.retire_rpc(in_rel, &ids)?;
        Ok(rows)
    }

    /// Delete the given request ids from an `@in(rpc)` rel (answered, or given
    /// up on). One batched DELETE, not per-row.
    pub fn retire_rpc(&mut self, in_rel: &str, ids: &[String]) -> Result<()> {
        if ids.is_empty() || !self.rels.contains_key(in_rel) { return Ok(()); }
        let ph = (1..=ids.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(",");
        self.db.conn().execute(
            &format!("DELETE FROM {} WHERE id IN ({ph})", tbl(in_rel)),
            rusqlite::params_from_iter(ids))?;
        Ok(())
    }

    // (cell_as_string lives at module scope, just above this impl block, so
    // every generic row reader — rel_rows, load_edges, edge_content_digest —
    // shares one stringify path across TEXT and INTEGER (sym) columns.)

    /// Read a relation's table as positional String rows (test/diagnostic).
    /// Returns empty if the relation isn't declared.
    pub fn rel_rows(&self, rel: &str, ncols: usize) -> Vec<Vec<String>> {
        if !self.rels.contains_key(rel) { return Vec::new(); }
        let conn = self.db.conn();
        let mut s = match conn.prepare(&format!("SELECT * FROM {}", tbl(rel))) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = s.query_map([], |r| {
            let mut v = Vec::with_capacity(ncols);
            for i in 0..ncols {
                // Stringify whatever the column holds: an int column read as
                // String is a rusqlite type error, which would silently drop
                // the whole row from a diagnostic read.
                v.push(cell_as_string(r, i)?);
            }
            Ok(v)
        });
        rows.map(|iter| iter.filter_map(|x| x.ok()).collect()).unwrap_or_default()
    }

    /// The query-facing `repo` relation (slug, root, url) as it stood after the
    /// last tick's `refresh_builtin_rels` — the union of config and dynamically
    /// pulled repos whose root exists. Diagnostics/tests.
    pub fn repo_relation(&self) -> Vec<(String, String, String)> {
        let conn = self.db.conn();
        let mut s = match conn.prepare("SELECT slug, root, url FROM rel_repo ORDER BY slug") {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = s.query_map([], |r|
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)));
        rows.ok()
            .map(|iter| iter.filter_map(|x| x.ok()).collect())
            .unwrap_or_default()
    }

    /// Drain `repo`-sink rules: compile each sink's body as a SELECT (the gen
    /// lowering), collect `(slug, root, url)` rows, and for each row whose
    /// github org is in the `org` allowlist, clone (if missing) + register the
    /// repo into `self.repos`. Registered repos appear in the `repo` builtin and
    /// `scan("*")` on the NEXT tick; idempotent (a slug/root already registered
    /// is skipped, so re-asserting each tick is cheap).
    ///
    /// The `org` allowlist is a hard filter: a row pulls only when
    /// `parse_github_org(url)` is `Some(org)` AND `org(name)` contains it. Rows
    /// with no parseable github org, or whose org is not listed, are skipped
    /// with a stderr line. A missing/empty `org` relation pulls nothing.
    #[tracing::instrument(skip_all, fields(n_sinks = sinks.len()), level = "debug")]
    fn run_repo_pulls(&mut self, sinks: &[&Rule]) -> Result<()> {
        if sinks.is_empty() { return Ok(()); }
        let allowlist: HashSet<String> = if self.rels.contains_key("org") {
            self.db.conn()
                .prepare(&format!("SELECT DISTINCT \"name\" FROM {}", tbl("org")))?
                .query_map([], |r| r.get::<_, String>(0))?
                .filter_map(|x| x.ok()).collect()
        } else {
            HashSet::new()
        };
        let mut pulled = false;
        for rule in sinks {
            // A GROUND FACT — `repo("slug", "/root", "url").` — has an empty body
            // and an all-literal head. Take the literals directly; the SELECT-body
            // path (lower_gen) can't express a bodiless rule, which is why a bare
            // fact used to be rejected (the author had to route through an extra
            // rel). Otherwise the head must be all variables (slug, root, url),
            // selected in order: a literal head term over a body is a filter the
            // gen lowering can't express, so reject it loudly.
            // `explicit` = this row is an author-written ground fact (not derived
            // from a body over the org corpus). An explicit repo bypasses the
            // github-org allowlist (the allowlist gates DYNAMIC pulls, not a repo
            // the author named by hand) and registers a present root without a
            // clone.
            let rows: Vec<(String, String, String, bool)> = if rule.body.is_empty() {
                let lit = |t: &Term| match t {
                    Term::Str(s) => Some(s.clone()),
                    Term::Wild => Some(String::new()),
                    _ => None,
                };
                let vals: Option<Vec<String>> = rule.head.terms.iter().map(lit).collect();
                match vals.as_deref() {
                    Some([slug]) => vec![(slug.clone(), String::new(), String::new(), true)],
                    Some([slug, root]) => vec![(slug.clone(), root.clone(), String::new(), true)],
                    Some([slug, root, url]) => vec![(slug.clone(), root.clone(), url.clone(), true)],
                    _ => {
                        eprintln!("[repo-sink] ground-fact head must be literal (slug[, root[, url]]); skipping");
                        continue;
                    }
                }
            } else {
                let vars: Vec<String> = rule.head.terms.iter().filter_map(|t| match t {
                    Term::Var(v) => Some(v.clone()),
                    _ => None,
                }).collect();
                if vars.len() != rule.head.terms.len() {
                    eprintln!("[repo-sink] head must be all variables (slug, root, url) or an all-literal ground fact; skipping");
                    continue;
                }
                let sql = crate::lower::lower_gen(&vars, &rule.body, &self.rels)?;
                self.db.conn().prepare(&sql)?
                    .query_map([], |r| Ok((r.get::<_, String>(0)?,
                                           r.get::<_, String>(1)?,
                                           r.get::<_, String>(2)?, false)))?
                    .filter_map(|x| x.ok()).collect()
            };
            for (slug, root_str, url, explicit) in rows {
                if slug.is_empty() {
                    eprintln!("[repo-sink] skip row with empty slug");
                    continue;
                }
                if self.repos.iter().any(|r| r.slug == slug
                    || (!root_str.is_empty() && r.root == PathBuf::from(&root_str)))
                {
                    continue; // already registered
                }
                let org = Self::parse_github_org(&url);
                let allowed = explicit || org.as_ref().is_some_and(|o| allowlist.contains(o));
                if !allowed {
                    eprintln!("[repo-sink] skip {slug}: org {:?} not in allowlist ({} listed)",
                        org, allowlist.len());
                    continue;
                }
                let root = if root_str.is_empty() {
                    crate::daemon::daemon_home().join("repos").join(&slug)
                } else {
                    PathBuf::from(&root_str)
                };
                let rc = crate::config::RepoConfig {
                    slug: slug.clone(), root: root.clone(),
                    url: Some(url.clone()), allow_missing: false,
                };
                match Self::ensure_cloned(&rc) {
                    Ok(()) => {
                        eprintln!("[repo-sink] pulled {slug} -> {} (org {org:?})",
                            root.display());
                        self.repos.push(rc);
                        pulled = true;
                    }
                    Err(e) => eprintln!("[repo-sink] clone {slug} failed: {e}"),
                }
            }
        }
        if pulled { self.save_repos_meta()?; }
        Ok(())
    }

    /// Drain `checkout` demand-sink rows: keep each named repo's checkout current
    /// on its default branch NON-DESTRUCTIVELY. The sink never stashes, never
    /// `reset --hard`s, and never moves HEAD off its current branch. On the
    /// default branch it fast-forwards via `merge --ff-only` only when the
    /// working tree is clean (dirty or diverged → skip, surface why). On any
    /// other branch it moves only the ref pointer (`git branch -f`); the working
    /// tree is left exactly as it is.
    ///
    /// Min seconds between fetches of the SAME repo by the checkout sink
    /// (DL_CHECKOUT_MIN_SECS, default 300). 0 disables the gate. Stops a short
    /// clock from re-fetching every repo every tick.
    fn checkout_min_secs() -> u64 {
        std::env::var("DL_CHECKOUT_MIN_SECS").ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(300)
    }

    /// Per-repo last-fetch timestamps (in-process; the daemon is one warm
    /// engine for its lifetime). Used by the min-interval gate.
    fn checkout_last_fetch() -> &'static Mutex<HashMap<String, std::time::Instant>> {
        static LAST: OnceLock<Mutex<HashMap<String, std::time::Instant>>> = OnceLock::new();
        LAST.get_or_init(|| Mutex::new(HashMap::new()))
    }

    /// A dedicated, narrow rayon pool for checkout sweeps (DL_CHECKOUT_WIDTH,
    /// default 2). `git fetch` is network+disk bound, so a 2-wide sweep is as
    /// fast as a full-core one and keeps the sink from eating the whole default
    /// pool (and every CPU core) when many repos are in view.
    fn checkout_pool() -> &'static rayon::ThreadPool {
        static POOL: OnceLock<rayon::ThreadPool> = OnceLock::new();
        POOL.get_or_init(|| {
            let n = std::env::var("DL_CHECKOUT_WIDTH").ok()
                .and_then(|s| s.parse::<usize>().ok())
                .filter(|&n| n > 0)
                .unwrap_or(2);
            rayon::ThreadPoolBuilder::new()
                .num_threads(n)
                .thread_name(|i| format!("dl-checkout-{i}"))
                .build()
                .expect("checkout thread pool")
        })
    }

    /// Drain the NETWORK/MUTATING sinks (`repo` pulls + `checkout` sweeps) AFTER
    /// the read-only fixpoint + query + gens ran in `tick_report`. This is the
    /// half of the tick that hits the network and rewrites checkouts, so it is
    /// split out from `tick_report` to keep read paths (`?` queries, `--check`,
    /// LSP, MCP) pure: a query must never trigger a 90s destructive sweep.
    ///
    /// The daemon's poll loop calls this off-tick on its cadence; one-shot CLI
    /// runs opt in via `--apply` / `DL_APPLY_SINKS=1` (so `dl prog.dl` on a
    /// gh-checkout program is a read by default and surfaces nothing new
    /// unless the operator opted in). `DL_CHECKOUT_DRY_RUN=1` previews the
    /// checkout sweep as `checkout_plan` rows without mutating anything.
    /// Returns the number of sink rows that landed (repos pulled + checkout
    /// outcomes), so a settle loop knows whether to re-tick to derive from them.
    pub fn drain_external_sinks(&mut self, prog: &Program) -> Result<usize> {
        use crate::ast::Item;
        let rules: Vec<&Rule> = prog.items.iter().filter_map(|i| match i { Item::Rule(r) => Some(r), _ => None }).collect();
        let repo_sinks: Vec<&Rule> = rules.iter().copied().filter(|r| r.is_repo_sink()).collect();
        // Validate repo-sink shape here (not in tick_report) so a read-only tick
        // does not pay it, and a malformed sink only bails when something would
        // actually try to drain it.
        for r in &repo_sinks {
            if r.is_source() {
                bail!("repo-sink rule must be derived-style (no scan/match/ast/...); \
                       its body is compiled as a SELECT over already-derived relations");
            }
        }
        // Repo pulls first: a pull clones + registers into self.repos; the new
        // repo is scannable / appears in the `repo` builtin on the NEXT tick
        // (mid-tick registration would shift the repo set under derived rules).
        let repos_before = self.repos.len();
        self.run_repo_pulls(&repo_sinks)?;
        let repos_pulled = self.repos.len().saturating_sub(repos_before);
        // Checkout sweeps after the pull: this tick's derived
        // `checkout(repo, branch, pr_heads)` rows keep each named repo's
        // checkout current (fetch + non-destructive fast-forward).
        let mut sink_rows = repos_pulled;
        if rules.iter().any(|r| r.is_checkout_sink()) {
            let outcomes_before = self.checkout_outcome_count()?;
            self.run_checkout_sweeps()?;
            sink_rows += self.checkout_outcome_count()?.saturating_sub(outcomes_before);
        }
        Ok(sink_rows)
    }

    /// Count rows in whichever checkout outcome rel currently exists (done or
    /// plan). Used by `drain_external_sinks` to surface "did this drain land
    /// new facts the next tick must derive from" without juggling schema.
    fn checkout_outcome_count(&self) -> Result<usize> {
        let conn = self.db.conn();
        for rel in ["checkout_done", "checkout_plan"] {
            if self.rels.contains_key(rel) {
                let n: i64 = conn.query_row(
                    &format!("SELECT COUNT(*) FROM {}", tbl(rel)), [], |r| r.get(0))?;
                return Ok(n as usize);
            }
        }
        Ok(0)
    }

    /// on disk (the ghcacher `checkout.rs` half). For every derived
    /// `checkout(repo, branch, pr_heads)` row we resolve the repo to a root
    /// (cloning a missing config repo via `resolve_repo`'s `ensure_cloned`), then
    /// sweep the roots in parallel — each an independent disk/network op. Runs
    /// AFTER the fixpoint + `run_repo_pulls` so a repo pulled this tick is
    /// sweepable, and so the rows reflect this tick's derivations.
    fn run_checkout_sweeps(&mut self) -> Result<()> {
        let Some(meta) = self.rels.get("checkout").cloned() else { return Ok(()); };
        if meta.cols.len() < 3 {
            bail!("checkout needs 3 columns (repo, branch, pr_heads); found {}", meta.cols.len());
        }
        let rows: Vec<(String, String, String)> = {
            let conn = self.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT DISTINCT \"{}\",\"{}\",\"{}\" FROM {} ORDER BY 1,2",
                meta.col_name(0), meta.col_name(1), meta.col_name(2), tbl("checkout")))?;
            let rs = s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?,
                                    r.get::<_, String>(2)?)))?;
            rs.filter_map(|x| x.ok()).collect()
        };
        if rows.is_empty() { return Ok(()); }
        let offline = std::env::var_os("DL_NO_FETCH").is_some();
        // Resolve (and clone-if-missing) each repo up front — resolve_repo needs
        // &self, so keep it out of the parallel section.
        let mut jobs: Vec<(String, PathBuf, String, bool)> = Vec::new();
        for (repo, branch, pr) in rows {
            let pr_heads = matches!(pr.as_str(), "1" | "true" | "yes" | "pr");
            match self.resolve_repo(&repo) {
                Ok((slug, root)) => jobs.push((slug, root, branch, pr_heads)),
                Err(e) => eprintln!("[checkout] skip {repo}: {e}"),
            }
        }
        // Min-interval gate: a `checkout` rule driven by a short clock used to
        // `git fetch` every repo on every tick that crossed the clock boundary,
        // pinning all cores every 5 minutes. Now a repo can't be fetched more
        // often than DL_CHECKOUT_MIN_SECS (default 300s) regardless of the rule.
        // Throttled repos surface a `skip` checkout_done row so a program sees
        // they were intentionally held, not silently dropped.
        let min_secs = Self::checkout_min_secs();
        let now = std::time::Instant::now();
        let mut throttled: Vec<(String, String)> = Vec::new();
        let eligible: Vec<(String, PathBuf, String, bool)> = if min_secs == 0 {
            jobs
        } else {
            let last = Self::checkout_last_fetch();
            let mut guard = last.lock().unwrap_or_else(|p| p.into_inner());
            jobs.into_iter().filter(|(slug, _, branch, _)| {
                let allowed = guard.get(slug)
                    .map(|t| now.duration_since(*t).as_secs() >= min_secs)
                    .unwrap_or(true);
                if allowed {
                    guard.insert(slug.clone(), now);
                    true
                } else {
                    throttled.push((slug.clone(), branch.clone()));
                    false
                }
            }).collect()
        };
        // Run the sweep on a DEDICATED narrow pool (DL_CHECKOUT_WIDTH, default
        // 2). `git fetch` is network+disk bound, so a 2-wide sweep is as fast as
        // a full-core one and stops the checkout sink from consuming the whole
        // rayon pool (and every CPU core) when many repos are in view.
        // DL_CHECKOUT_DRY_RUN=1 previews the sweep: each outcome reports what
        // WOULD happen (ff/branch-f/skip-dirty/skip-diverged) without running
        // `merge --ff-only` or `git branch -f`, and the rows land in
        // `checkout_plan` instead of `checkout_done`.
        let dry_run = std::env::var_os("DL_CHECKOUT_DRY_RUN").is_some();
        let mut results: Vec<(String, String, CheckoutOutcome)> = if eligible.is_empty() {
            Vec::new()
        } else {
            Self::checkout_pool().install(|| {
                eligible.par_iter()
                    .map(|(slug, root, branch, pr)| {
                        let out = Self::checkout_one(root, branch, *pr, offline, dry_run);
                        (slug.clone(), branch.clone(), out)
                    }).collect()
            })
        };
        // Surface throttled repos so the program can tell fetch-skipped from
        // git-failed (ok=1, action="skip", detail names the gate).
        for (slug, branch) in throttled {
            results.push((slug, branch, CheckoutOutcome {
                action: "skip", ok: true,
                detail: format!("min-interval {min_secs}s (DL_CHECKOUT_MIN_SECS)"),
            }));
        }
        // Log AND surface an outcome rel: stderr goes to daemon.log under the
        // daemon (invisible to a query), so the rel is how a program / live
        // query confirms the sweep fired and reacts to failures. Dry-run emits
        // `checkout_plan` (what would happen); apply emits `checkout_done`.
        let sink_rel = if dry_run { "checkout_plan" } else { "checkout_done" };
        let mut done_rows: Vec<Vec<Value>> = Vec::with_capacity(results.len());
        for (slug, branch, out) in &results {
            eprintln!("[checkout{}] {slug}: {} {}", if dry_run { " (plan)" } else { "" }, out.action, out.detail);
            done_rows.push(vec![
                Value::Text(slug.clone()),
                Value::Text(branch.clone()),
                Value::Text(out.action.to_string()),
                Value::Int(if out.ok { 1 } else { 0 }),
                Value::Text(out.detail.clone()),
            ]);
        }
        self.refresh_rel(sink_rel,
            &["repo", "branch", "action", "ok", "detail"], &done_rows)?;
        Ok(())
    }

    /// One repo's keep-current sweep, NON-DESTRUCTIVE. Static (no `&self`) so the
    /// caller can run it under rayon. Fetches origin (unless `offline`), resolves
    /// the default branch (given, else `origin/HEAD`), then:
    ///   * on that branch + clean working tree → `merge --ff-only origin/<branch>`
    ///     (the only mutation; fails loud on divergence, leaving everything as-is);
    ///   * on that branch + dirty working tree → SKIP (the operator's work — we do
    ///     not stash, overwrite, or reset anything);
    ///   * on any other branch or detached → `git branch -f <branch> origin/<branch>`
    ///     (move ONLY the ref pointer; HEAD + working tree are untouched).
    /// When `dry_run` is true the mutations are skipped and the outcome carries
    /// what WOULD have happened (driven by `merge-base --is-ancestor`), so a
    /// program/CLI can preview the sweep without touching any checkout.
    fn checkout_one(root: &Path, branch: &str, pr_heads: bool, offline: bool, dry_run: bool) -> CheckoutOutcome {
        let skip = |detail: String| CheckoutOutcome { action: "skip", ok: false, detail };
        if !root.exists() { return skip(format!("root {} missing", root.display())); }
        let git = |args: &[&str]| Command::new("git").arg("-C").arg(root).args(args).output();
        if !offline {
            let _ = git(&["fetch", "--quiet", "origin"]);
            if pr_heads {
                let _ = git(&["fetch", "--prune", "--quiet", "origin",
                              "+refs/pull/*/head:refs/remotes/pr/*/head"]);
            }
        }
        let default = if !branch.is_empty() {
            branch.to_string()
        } else {
            match git(&["symbolic-ref", "--short", "refs/remotes/origin/HEAD"]) {
                Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
                    .trim().rsplit('/').next().unwrap_or("").to_string(),
                _ => String::new(),
            }
        };
        if default.is_empty() {
            return skip("no branch given and origin/HEAD unset (run `git remote set-head origin -a`)".into());
        }
        let remote_ref = format!("origin/{default}");
        let have_remote = git(&["rev-parse", "--verify", "--quiet", &remote_ref])
            .map(|o| o.status.success()).unwrap_or(false);
        if !have_remote {
            return skip(format!("no {remote_ref}{}",
                if offline { " (DL_NO_FETCH; never fetched)" } else { " (fetch failed / wrong branch)" }));
        }
        let cur = git(&["symbolic-ref", "--short", "HEAD"]).ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        if cur.as_deref() == Some(default.as_str()) {
            // On the default branch. NEVER stash, NEVER reset --hard. The only
            // mutation is a clean fast-forward; dirty or diverged checkouts are
            // the operator's work and are left untouched.
            let dirty = git(&["status", "--porcelain"]).map(|o| !o.stdout.is_empty()).unwrap_or(false);
            if dirty {
                return CheckoutOutcome { action: "skip", ok: true,
                    detail: format!("{default}: working tree dirty; left untouched") };
            }
            if dry_run {
                // merge-base --is-ancestor HEAD <remote>: exit 0 = HEAD is an
                // ancestor of remote (ff would succeed); non-zero = diverged.
                // No mutation: the rel name (checkout_plan) carries the preview.
                let ff_ok = git(&["merge-base", "--is-ancestor", "HEAD", &remote_ref])
                    .map(|o| o.status.success()).unwrap_or(false);
                return if ff_ok {
                    CheckoutOutcome { action: "ff", ok: true,
                        detail: format!("{default} -> {remote_ref}") }
                } else {
                    CheckoutOutcome { action: "skip", ok: false,
                        detail: format!("{default}: diverged from {remote_ref}; left untouched") }
                };
            }
            match git(&["merge", "--ff-only", &remote_ref]) {
                Ok(o) if o.status.success() => CheckoutOutcome { action: "ff", ok: true,
                    detail: format!("{default} -> {remote_ref}") },
                Ok(o) => CheckoutOutcome { action: "skip", ok: false,
                    detail: format!("{default}: --ff-only {remote_ref} failed (diverged?); left untouched: {}",
                        String::from_utf8_lossy(&o.stderr).trim()) },
                Err(e) => CheckoutOutcome { action: "skip", ok: false,
                    detail: format!("{default}: merge --ff-only {remote_ref} errored; left untouched: {e}") },
            }
        } else {
            // NOT on the default branch: move ONLY the ref pointer. HEAD + the
            // working tree stay exactly where they are.
            if dry_run {
                return CheckoutOutcome { action: "branch-f", ok: true, detail: format!(
                    "{default} -> {remote_ref} (checkout left on {})",
                    cur.as_deref().unwrap_or("detached HEAD")) };
            }
            match git(&["branch", "-f", &default, &remote_ref]) {
                Ok(o) if o.status.success() => CheckoutOutcome { action: "branch-f", ok: true, detail: format!(
                    "{default} -> {remote_ref} (checkout left on {})", cur.as_deref().unwrap_or("detached HEAD")) },
                Ok(o) => CheckoutOutcome { action: "branch-f", ok: false,
                    detail: format!("branch -f {default} failed: {}", String::from_utf8_lossy(&o.stderr).trim()) },
                Err(e) => CheckoutOutcome { action: "branch-f", ok: false,
                    detail: format!("branch -f {default} errored: {e}") },
            }
        }
    }

    /// Parse the org from a github URL: `https://github.com/<org>/<repo>(.git)?`
    /// (ssh `git@github.com:<org>/<repo>` accepted). `None` for non-github hosts
    /// or a path too short to carry an org. This is the allowlist key: a pulled
    /// repo's org must appear in the `org` relation.
    fn parse_github_org(url: &str) -> Option<String> {
        let u = url.trim();
        let path = if let Some(rest) = u.strip_prefix("https://github.com/")
            .or_else(|| u.strip_prefix("http://github.com/"))
            .or_else(|| u.strip_prefix("git@github.com:"))
        {
            rest
        } else {
            return None;
        };
        let mut parts = path.split('/');
        let org = parts.next()?;
        if org.is_empty() { return None; }
        let repo = parts.next()?;
        if repo.is_empty() { return None; }
        Some(org.to_string())
    }

    /// Resolve `name` (HEAD or a ref) to an oid in `repo_root`, compare to the
    /// last-seen oid stored in `_ref`, and persist the new value. Returns
    /// `Some((old, new))` when the oid changed (old is `None` on first sight),
    /// appending one row to `_rev_log`; `None` when unchanged. The single-event
    /// write path: one ref observed per call, not an N+1 batch.
    pub fn observe_ref(&self, repo: &str, repo_root: &Path, name: &str)
        -> Result<Option<(Option<String>, String)>>
    {
        let out = Command::new("git").arg("-C").arg(repo_root)
            .args(["rev-parse", name]).output()?;
        if !out.status.success() { return Ok(None); }
        let new = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if new.is_empty() { return Ok(None); }
        let old: Option<String> = self.db.conn().query_row(
            "SELECT oid FROM _ref WHERE repo = ?1 AND name = ?2",
            rusqlite::params![repo, name], |r| r.get(0)).ok();
        if old.as_deref() == Some(new.as_str()) { return Ok(None); }
        let now = unix_secs();
        self.db.conn().execute(
            "INSERT INTO _ref(repo, name, oid, observed_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(repo, name) DO UPDATE SET oid=excluded.oid, observed_at=excluded.observed_at",
            rusqlite::params![repo, name, new, now])?;
        self.db.conn().execute(
            "INSERT INTO _rev_log(repo, name, old, new, at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![repo, name, old.clone().unwrap_or_default(), new, now])?;
        Ok(Some((old, new)))
    }

    /// Paths that differ between two revs in `repo_root`, intersected with the
    /// `_file` path index for `repo` (so a rev advance only reports files the
    /// engine actually tracks). `old` empty = first sight, every tracked path
    /// for that repo is considered changed.
    pub fn files_changed_between(&self, repo: &str, repo_root: &Path, old: &str, new: &str)
        -> Result<Vec<String>>
    {
        let mut tracked = self.db.conn().prepare(
            "SELECT DISTINCT path FROM _file WHERE repo = ?1")?;
        let tracked: HashSet<String> = tracked
            .query_map(rusqlite::params![repo], |r| r.get::<_, String>(0))?
            .filter_map(|x| x.ok()).collect();
        if old.is_empty() {
            let mut v: Vec<String> = tracked.into_iter().collect();
            v.sort();
            return Ok(v);
        }
        let out = Command::new("git").arg("-C").arg(repo_root)
            .args(["diff", "--name-only", old, new]).output()?;
        if !out.status.success() { return Ok(Vec::new()); }
        let mut changed: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|p| !p.is_empty() && tracked.contains(p))
            .collect();
        changed.sort();
        changed.dedup();
        Ok(changed)
    }




















    /// Materialize `head(a, b, score) <- node2vec(edge).`: read the 2-col edge
    /// rel, learn one structural vector per node (random walks + skip-gram), store
    /// the vectors in `_node_embeddings` keyed by the edge rel name, and fill the
    /// 3-col head with each node's top-k cosine-nearest neighbors. Excluded from
    /// `rebuild_derived` (the Node2vec body item can't lower to SQL); runs after
    /// the edge rel has materialized. The embed is the cost, so it is guarded:
    /// W1 skips an unchanged graph (digest match), W2 reuses cached vectors for
    /// any of the last N seen edge-digests (branch thrash), only a genuinely new
    /// graph pays `embed_graph`. Cap the graph or run on demand for huge edge sets.
    fn eval_node2vec_rule(&mut self, rule: &Rule) -> Result<()> {
        let edge = rule.node2vec_edge()
            .ok_or_else(|| anyhow::anyhow!("eval_node2vec_rule on a non-node2vec rule"))?;
        let head = &rule.head.rel;
        let head_meta = self.rels.get(head)
            .ok_or_else(|| anyhow::anyhow!("unknown head relation {head}"))?;
        if rule.head.terms.len() != 3 || head_meta.cols.len() != 3 {
            bail!("node2vec head '{head}' must have exactly 3 columns (a, b, score); got {}",
                  head_meta.cols.len());
        }
        // Own the head cols now: the recompute path mutates `self`
        // (`node2vec_recomputed`), which conflicts with holding `head_meta`'s
        // immutable borrow of `self.rels` to the end.
        let head_cols: Vec<String> = head_meta.cols.iter().map(|c| c.name.clone()).collect();
        let edge_meta = self.rels.get(edge)
            .ok_or_else(|| anyhow::anyhow!("node2vec edge relation '{edge}' is not declared"))?;
        if edge_meta.cols.len() < 2 {
            bail!("node2vec edge relation '{edge}' must have at least 2 columns (src, dst); got {}",
                  edge_meta.cols.len());
        }
        // Read the first two columns as the directed edge list.
        let (c0, c1) = (edge_meta.cols[0].name.clone(), edge_meta.cols[1].name.clone());
        let edges: Vec<(String, String)> = {
            let conn = self.db.conn();
            let mut s = conn.prepare(&format!("SELECT {c0}, {c1} FROM {}", tbl(edge)))?;
            let v: Vec<(String, String)> = s.query_map([], |r|
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .filter_map(|x| x.ok()).collect();
            v
        };

        // W1 digest-skip: node2vec is a GLOBAL op (walks touch the whole graph),
        // so it is not cheaply incrementalizable. Most git checkouts move file
        // content without moving the call/type edge set, so an order-independent
        // digest of the edge rows lets the common re-tick be a no-op. Same guard
        // the scc/closure operators use (recondense only when rows moved).
        let digest = blake3_edges(&edges);
        let dhex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        let dkey = format!("node2vec:{edge}");
        // Vectors for THIS exact digest (W2 keeps the last N digests per graph).
        let have_cur: i64 = self.db.conn().query_row(
            "SELECT COUNT(*) FROM _node_embeddings WHERE graph = ?1 AND edge_digest = ?2",
            rusqlite::params![edge, dhex], |r| r.get(0))?;
        let trace = std::env::var("SPREFA_N2V_TRACE").is_ok();
        // Skip when node_sim already reflects this digest and its vectors exist
        // (or the graph is legitimately empty and we recorded the all-zero digest).
        if self.load_rel_digest(&dkey)? == Some(digest)
            && (edges.is_empty() || have_cur > 0) {
            if trace { eprintln!("[node2vec] graph '{edge}': skip (digest unchanged)"); }
            return Ok(());
        }

        // node_sim is rebuilt for the new digest either way.
        self.db.exec(&format!("DELETE FROM {}", tbl(head)))?;
        if edges.is_empty() {
            // Empty graph: drop this graph's whole vector cache, record the
            // all-zero digest so a later empty tick skips.
            self.db.conn().execute("DELETE FROM _node_embeddings WHERE graph = ?1", [edge])?;
            self.db.conn().execute("DELETE FROM _node_emb_seen WHERE graph = ?1", [edge])?;
            if trace { eprintln!("[node2vec] graph '{edge}': empty (cleared)"); }
            self.save_rel_digest(&dkey, &digest)?;
            return Ok(());
        }

        let cfg = crate::embed::node2vec::N2vConfig::from_env();
        // W2 cache: reuse the stored vectors when we have already embedded this
        // exact edge set (branch A<->B thrash is a hit both ways); only a
        // genuinely new graph pays the embed.
        let pool: Vec<(String, Vec<f32>)> = if have_cur > 0 {
            if trace { eprintln!("[node2vec] graph '{edge}': cache hit (digest seen)"); }
            let conn = self.db.conn();
            let mut s = conn.prepare(
                "SELECT node, vec FROM _node_embeddings WHERE graph = ?1 AND edge_digest = ?2")?;
            let v: Vec<(String, Vec<f32>)> = s.query_map(rusqlite::params![edge, dhex], |r|
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .filter_map(|x| x.ok())
                .map(|(n, txt): (String, String)| (n, crate::embed::parse_vec(&txt)))
                .collect();
            v
        } else {
            if trace { eprintln!("[node2vec] graph '{edge}': re-embed ({} edges)", edges.len()); }
            self.node2vec_recomputed += 1;
            let pool = crate::embed::node2vec::embed_graph(&edges, &cfg);
            if pool.len() > 2000 {
                eprintln!("[node2vec] brute-force KNN over {} nodes (O(n^2)); \
                           shrink the edge rel or cap SPREFA_N2V_*", pool.len());
            }
            // Persist this digest's vectors (one flush, never N+1).
            let dim = cfg.dim as i64;
            let emb_rows: Vec<Vec<Value>> = pool.iter().map(|(node, v)| vec![
                Value::Text(node.clone()), Value::Text(edge.to_string()), Value::Text(dhex.clone()),
                Value::Int(dim), Value::Text(crate::embed::encode_vec(v))]).collect();
            self.db.insert_rows("_node_embeddings",
                &["node", "graph", "edge_digest", "dim", "vec"], &emb_rows)?;
            pool
        };

        // LRU: stamp this digest most-recently-used, prune each graph to the N
        // most-recent distinct digests (SPREFA_N2V_CACHE, default 4).
        let seq: i64 = self.db.conn().query_row(
            "SELECT COALESCE(MAX(last_tick), 0) + 1 FROM _node_emb_seen", [], |r| r.get(0))?;
        self.db.conn().execute(
            "INSERT INTO _node_emb_seen(graph, digest, last_tick) VALUES (?1, ?2, ?3)
             ON CONFLICT(graph, digest) DO UPDATE SET last_tick = excluded.last_tick",
            rusqlite::params![edge, dhex, seq])?;
        let cap: i64 = std::env::var("SPREFA_N2V_CACHE").ok()
            .and_then(|s| s.parse().ok()).filter(|n| *n >= 1).unwrap_or(4);
        self.db.conn().execute(
            "DELETE FROM _node_embeddings WHERE graph = ?1 AND edge_digest NOT IN
                 (SELECT digest FROM _node_emb_seen WHERE graph = ?1 ORDER BY last_tick DESC LIMIT ?2)",
            rusqlite::params![edge, cap])?;
        self.db.conn().execute(
            "DELETE FROM _node_emb_seen WHERE graph = ?1 AND digest NOT IN
                 (SELECT digest FROM _node_emb_seen WHERE graph = ?1 ORDER BY last_tick DESC LIMIT ?2)",
            rusqlite::params![edge, cap])?;

        // Fill the head with KNN pairs (reuses the text path's cosine top-k).
        let k: usize = std::env::var("SPREFA_NODE_SIM_K").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(8);
        let rows = knn_rows(&pool, k);
        let cols: Vec<&str> = head_cols.iter().map(|s| s.as_str()).collect();
        self.db.insert_rows(&tbl(head), &cols, &rows)?;
        self.save_rel_digest(&dkey, &digest)?;
        Ok(())
    }

    // LANG-JUNCTION(manifest-probe): the manifest filename list probed above the scanned file set; a language whose module resolver reads a manifest (Cargo.toml, package.json, go.mod) must add its name here or the resolver gets no manifest content
    /// Read the Cargo.toml / package.json / go.mod manifests above the file set,
    /// at this rev, into a map (manifest path -> contents) for the resolver's
    /// crate / package / module registries. Probes the distinct ancestor
    /// directories of the files; `read_content` errors (no such manifest) are
    /// skipped. Rev-correct (git show for a git rev, disk for WORK).
    fn collect_manifests(&self, rev: &str, files: &HashSet<String>) -> HashMap<String, String> {
        let mut dirs: HashSet<String> = HashSet::new();
        for f in files {
            let mut d = Path::new(f);
            while let Some(p) = d.parent() {
                dirs.insert(p.to_string_lossy().replace('\\', "/"));
                d = p;
            }
        }
        let mut out = HashMap::new();
        for dir in dirs {
            for name in ["Cargo.toml", "package.json", "go.mod"] {
                let rel = if dir.is_empty() { name.to_string() } else { format!("{dir}/{name}") };
                if let Ok(content) = read_content(&self.root, rev, &rel) {
                    out.insert(rel, content);
                }
            }
        }
        out
    }

    /// A closure head `rel_<head>` is a recursive-CTE view over the condensation
    /// tables of its edge relation. The view yields cross-component reach plus
    /// same-cyclic-component pairs (so a node on a cycle reaches itself).
    fn declare_closure(&mut self, d: &RelDecl, edge: &str) -> Result<()> {
        if d.cols.len() != 2 { bail!("closure head {} must have 2 columns", d.name); }
        self.rels.insert(d.name.clone(), RelMeta { cols: d.cols.clone(), ..Default::default() });
        let (nt, et, v) = (scc_node_tbl(edge), scc_edge_tbl(edge), tbl(&d.name));
        self.db.conn().execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS {nt} (name TEXT PRIMARY KEY, comp INTEGER, cyclic INTEGER);
             CREATE TABLE IF NOT EXISTS {et} (comp_src INTEGER, comp_dst INTEGER, PRIMARY KEY(comp_src, comp_dst));"
        ))?;
        // a prior run may have left rel_<head> as a view or a real table; clear both.
        self.db.conn().execute(&format!("DROP VIEW IF EXISTS {v}"), [])?;
        self.db.conn().execute(&format!("DROP TABLE IF EXISTS {v}"), [])?;
        let (c0, c1) = (&d.cols[0].name, &d.cols[1].name);
        self.db.conn().execute_batch(&format!(
            "CREATE VIEW {v} AS
             WITH RECURSIVE cr(a, b) AS (
               SELECT comp_src, comp_dst FROM {et}
               UNION
               SELECT cr.a, e.comp_dst FROM cr JOIN {et} e ON e.comp_src = cr.b
             )
             SELECT na.name AS \"{c0}\", nb.name AS \"{c1}\"
               FROM cr JOIN {nt} na ON na.comp = cr.a JOIN {nt} nb ON nb.comp = cr.b
             UNION
             SELECT na.name AS \"{c0}\", nb.name AS \"{c1}\"
               FROM {nt} na JOIN {nt} nb ON na.comp = nb.comp AND na.cyclic = 1;"
        ))?;
        Ok(())
    }

    /// The current `@next` generation (`_carry_meta.tx`). 0 on a fresh db.
    fn current_tx(&self) -> Result<i64> {
        Ok(self.db.conn().query_row(
            "SELECT tx FROM _carry_meta WHERE k = 'tx'", [], |r| r.get(0))?)
    }

    /// Advance the carry clock to `tx` (called once per tick after staging).
    fn set_tx(&self, tx: i64) -> Result<()> {
        self.db.conn().execute("UPDATE _carry_meta SET tx = ?1 WHERE k = 'tx'", [tx])?;
        Ok(())
    }

    /// Create a carry buffer table mirroring the live rel's columns plus `tx`.
    /// PK is (all rel cols, tx) so a re-tick at the same generation is idempotent.
    fn ensure_carry_table(&self, rel: &str, meta: &RelMeta) -> Result<()> {
        let cols: Vec<String> = meta.cols.iter()
            .map(|c| format!("\"{}\" {}", c.name, c.ty.sql())).collect();
        let pk: Vec<String> = meta.cols.iter().map(|c| format!("\"{}\"", c.name)).collect();
        let sql = format!(
            "CREATE TABLE IF NOT EXISTS {} ({}, tx INTEGER NOT NULL, PRIMARY KEY ({}, tx))",
            carry_tbl(rel), cols.join(", "), pk.join(", "));
        self.db.conn().execute(&sql, [])?;
        Ok(())
    }

    /// Replace the live rel with the carry rows staged for generation `tx`.
    /// Load the carry rows staged for `tx` into the live rel table. Returns whether
    /// the loaded content DIFFERS from what the live table held before — a carry
    /// rel that advances is an EDB input change, so the caller must rebuild the
    /// derived rules that read it (a derived rule over a carried-in rel was
    /// otherwise frozen at its first value, since nothing flipped `changed`).
    fn load_carry(&self, rel: &str, meta: &RelMeta, tx: i64) -> Result<bool> {
        let before = self.rel_content_digest(rel, meta)?;
        let cl = meta.cols.iter().map(|c| format!("\"{}\"", c.name))
            .collect::<Vec<_>>().join(", ");
        self.db.conn().execute(&format!("DELETE FROM {}", tbl(rel)), [])?;
        self.db.conn().execute(
            &format!("INSERT OR IGNORE INTO {dst} ({cl}) SELECT {cl} FROM {src} WHERE tx = ?1",
                dst = tbl(rel), src = carry_tbl(rel)),
            [tx])?;
        let after = self.rel_content_digest(rel, meta)?;
        Ok(before != after)
    }

    /// Stage each @next rule's body (evaluated over the converged tick-T state)
    /// into its carry buffer at `cur + 1`. One pass: the body reads only relations
    /// that are already converged this tick (including the carried-in live rel),
    /// none of which change during staging, so no fixpoint is needed.
    fn rebuild_next(&self, next_rules: &[&Rule], next_rels: &[String], cur: i64) -> Result<()> {
        let nxt = cur + 1;
        for rel in next_rels {
            self.db.conn().execute(
                &format!("DELETE FROM {} WHERE tx = ?1", carry_tbl(rel)), [nxt])?;
        }
        for r in next_rules {
            let sql = crate::lower::lower_rule_to(
                r, &self.rels, &carry_tbl(&r.head.rel),
                &[("tx".to_string(), nxt.to_string())])?;
            self.db.conn().execute(&sql, [])?;
        }
        Ok(())
    }


    /// Evaluate the TERM-form `json`/`jsonp` rules — the hybrid join+extract. A
    /// rule like `star(repo,n) <- page(repo,200,_,body), jsonp(body,"stars",n).`
    /// joins relations in SQL (binding the content var `body`), then runs the
    /// tree-sitter extractor over each joined row's bound string, fanning the
    /// extracted bindings into head rows. This is the only path that parses a
    /// value held in a relation (a response body, a column) rather than a file.
    /// Runs after sources/responses are present and before the derived fixpoint
    /// (so derived rules see the output). Returns whether any head rel changed,
    /// which the caller ORs into the rebuild gate.
    ///
    /// @recompute unguarded: re-runs each tick — its inputs (response/source rels)
    /// move off the file-source-digest path, so a digest skip here would miss a
    /// freshly-drained body. The join is bounded by the read relations (the
    /// response/page set), not the repo; the downstream rebuild is gated on the
    /// returned changed flag, so a steady state does not re-run the fixpoint.
    fn eval_extract_rules(&self, extract_rules: &[&Rule]) -> Result<bool> {
        if extract_rules.is_empty() { return Ok(false); }
        let mut heads: Vec<String> = Vec::new();
        for r in extract_rules {
            if !heads.contains(&r.head.rel) { heads.push(r.head.rel.clone()); }
        }
        let mut any_changed = false;
        for head_rel in &heads {
            let cols: Vec<String> = {
                let meta = self.rels.get(head_rel)
                    .ok_or_else(|| anyhow::anyhow!("term-extract head rel `{head_rel}` is not declared"))?;
                meta.cols.iter().map(|c| c.name.clone()).collect()
            };
            let mut rows: Vec<Vec<Value>> = Vec::new();
            for r in extract_rules.iter().filter(|r| &r.head.rel == head_rel) {
                self.extract_rule_rows(r, &mut rows)?;
            }
            // Changed iff the head row SET differs from what is stored (sorted
            // compare): only then does the downstream fixpoint need to re-run.
            // `Value` is not Ord/Eq; compare the row SETS via a string projection.
            let key = |row: &[Value]| -> Vec<String> {
                row.iter().map(|v| match v {
                    Value::Int(n) => format!("i{n}"),
                    Value::Text(s) => format!("t{s}"),
                    Value::Null => "n".to_string(),
                }).collect()
            };
            let mut before: Vec<Vec<String>> = {
                let n = cols.len();
                let sql = format!("SELECT * FROM {}", tbl(head_rel));
                let mut stmt = self.db.conn().prepare(&sql)?;
                let v = stmt.query_map([], |row| {
                    let mut r = Vec::with_capacity(n);
                    for i in 0..n {
                        r.push(match row.get::<_, rusqlite::types::Value>(i)? {
                            rusqlite::types::Value::Integer(x) => format!("i{x}"),
                            rusqlite::types::Value::Text(s) => format!("t{s}"),
                            rusqlite::types::Value::Null => "t".to_string(),
                            other => format!("{other:?}"),
                        });
                    }
                    Ok(r)
                })?.filter_map(|x| x.ok()).collect();
                v
            };
            let mut after: Vec<Vec<String>> = rows.iter().map(|r| key(r)).collect();
            before.sort();
            after.sort();
            if before != after { any_changed = true; }
            self.db.conn().execute(&format!("DELETE FROM {}", tbl(head_rel)), [])?;
            let col_refs: Vec<&str> = cols.iter().map(|s| s.as_str()).collect();
            self.db.insert_rows(&tbl(head_rel), &col_refs, &rows)?;
        }
        Ok(any_changed)
    }

    /// One term-extract rule: project the relational join to bind the content var,
    /// then fan the extractor (`run_data` for jsonp, `run_pattern` for json) over
    /// each joined row's bound string into head rows. Cmps over both join vars AND
    /// the extracted vars are post-filtered with `eval_cmp`.
    fn extract_rule_rows(&self, r: &Rule, out_rows: &mut Vec<Vec<Value>>) -> Result<()> {
        let extracts: Vec<&BodyItem> = r.body.iter().filter(|b| matches!(b,
            BodyItem::JsonP { rev: None, .. } | BodyItem::Json { rev: None, .. }
            | BodyItem::Sg { rev: None, .. })).collect();
        if extracts.len() != 1 {
            bail!("rule `{}`: a term-form json/jsonp/sg rule must have exactly one extract op \
                   (split a multi-extract rule into chained rules)", r.head.rel);
        }
        let cmps: Vec<&Constraint> = r.body.iter()
            .filter_map(|b| if let BodyItem::Cmp(c) = b { Some(c) } else { None }).collect();
        // The relational join binds the content var (and the head's join vars).
        let vars = async_bound_vars(r);
        if vars.is_empty() {
            bail!("rule `{}`: a term-extract rule needs a positive atom binding the content var", r.head.rel);
        }
        let sql = crate::lower::lower_body_projection(&r.body, &self.rels, &vars)?;
        let join_rows: Vec<Bind> = {
            let mut stmt = self.db.conn().prepare(&sql)?;
            let v = stmt.query_map([], |row| {
                let mut b: Bind = HashMap::new();
                for (i, v) in vars.iter().enumerate() {
                    let val = match row.get::<_, rusqlite::types::Value>(i)? {
                        rusqlite::types::Value::Integer(x) => Value::Int(x),
                        rusqlite::types::Value::Text(s) => Value::Text(s),
                        rusqlite::types::Value::Null => Value::Text(String::new()),
                        other => Value::Text(format!("{other:?}")),
                    };
                    b.insert(v.clone(), val);
                }
                Ok(b)
            })?.filter_map(|x| x.ok()).collect();
            v
        };
        // A term source has no extension to dispatch on (response bodies are
        // json); the synthetic name routes `run_data`/`run_pattern` to the json
        // walker. yaml/toml-in-a-string is not supported (v1).
        let synth = "_.json";
        let emit = |env: &Bind, out: &mut Vec<Vec<Value>>| -> Result<()> {
            for c in &cmps { if !eval_cmp(c, env)? { return Ok(()); } }
            let mut row = Vec::with_capacity(r.head.terms.len());
            for t in &r.head.terms { row.push(val_of(t, env)?); }
            out.push(row);
            Ok(())
        };
        match extracts[0] {
            BodyItem::JsonP { src, jpath, out, id, .. } => {
                let srcvar = var_of(src)?;
                let outvar = var_of(out)?;
                if id.is_some() {
                    bail!("rule `{}`: a term-form jsonp has no file to locate — drop the `id` arg", r.head.rel);
                }
                for jr in &join_rows {
                    let content = match jr.get(&srcvar) {
                        Some(Value::Text(s)) => s.clone(),
                        Some(Value::Int(n)) => n.to_string(),
                        _ => continue,
                    };
                    for (v, _lo, _hi) in crate::datapath::run_data(synth, &content, jpath) {
                        let mut env = jr.clone();
                        env.insert(outvar.clone(), Value::Text(v));
                        emit(&env, out_rows)?;
                    }
                }
            }
            BodyItem::Json { src, pat, .. } => {
                let srcvar = var_of(src)?;
                let (steps, _) = crate::datapath::parse_pattern(pat)
                    .map_err(|e| anyhow::anyhow!("json pattern error: {e}"))?;
                for jr in &join_rows {
                    let content = match jr.get(&srcvar) {
                        Some(Value::Text(s)) => s.clone(),
                        Some(Value::Int(n)) => n.to_string(),
                        _ => continue,
                    };
                    for m in crate::datapath::run_pattern(synth, &content, &steps) {
                        let mut env = jr.clone();
                        for (cap, text, _lo, _hi) in m { env.insert(cap, Value::Text(text)); }
                        emit(&env, out_rows)?;
                    }
                }
            }
            // Term-form `sg(:lang, src, "pat", line, col, end_line, end_col)`:
            // run the ast-grep pattern over each joined row's bound string. Metavar
            // captures bind by name (like the file form); the span outputs bind the
            // match's line/col RELATIVE to the bound string (byte 0 = start of the
            // value). No file, no located id — the caller adds the enclosing
            // region's own line to reach file coordinates.
            BodyItem::Sg { src, lang, pattern, line, col, end_line, end_col, .. } => {
                let srcvar = var_of(src)?;
                let slv = opt_var(line)?;
                let clv = opt_var(col)?;
                let ellv = opt_var(end_line)?;
                let eclv = opt_var(end_col)?;
                for jr in &join_rows {
                    let content = match jr.get(&srcvar) {
                        Some(Value::Text(s)) => s.clone(),
                        Some(Value::Int(n)) => n.to_string(),
                        _ => continue,
                    };
                    for (ln, c, eln, ec, _mlo, _mhi, caps) in crate::sg::run_sg(&content, lang, pattern)? {
                        let mut env = jr.clone();
                        if let Some(v) = &slv { env.insert(v.clone(), Value::Int(ln)); }
                        if let Some(v) = &clv { env.insert(v.clone(), Value::Int(c)); }
                        if let Some(v) = &ellv { env.insert(v.clone(), Value::Int(eln)); }
                        if let Some(v) = &eclv { env.insert(v.clone(), Value::Int(ec)); }
                        for (name, text, _lo, _hi) in caps { env.insert(name, Value::Text(text)); }
                        emit(&env, out_rows)?;
                    }
                }
            }
            _ => unreachable!("extracts filtered to JsonP/Json/Sg"),
        }
        Ok(())
    }

    /// Wipe derived tables and run the semi-naive fixpoint to convergence.
    #[tracing::instrument(skip_all, fields(n_rules = derived_rules.len(), n_rels = derived_rels.len()), level = "debug")]
    fn rebuild_derived(&self, derived_rules: &[&Rule], derived_rels: &[String]) -> Result<()> {
        // P3: instrument the whole pass directly (start/stop), rather than
        // relying on the `activity` phase-transition emitter — a quiet
        // one-shot tick (`--check`) may never make the NEXT phase transition
        // that would otherwise close out the "derived" phase record, so the
        // one-shot path silently lost this timing. Emitted unconditionally
        // (daemon and one-shot alike); the per-rel `_stmt_ms` breakdown below
        // is unchanged.
        let t_rebuild = std::time::Instant::now();
        for rel in derived_rels { self.db.conn().execute(&format!("DELETE FROM {}", tbl(rel)), [])?; }
        // Evaluate stratum by stratum: each higher stratum's negation reads
        // relations that lower strata have already finished. Within a stratum,
        // rules split into rel-level dependency components (dependencies first).
        // Only a recursive component iterates to a fixpoint; an acyclic one runs
        // each rule exactly once — the loop's extra pass existed only to observe
        // delta=0, which doubled the cost of every expensive non-recursive join.
        // Per-rel statement cost of THIS rebuild (max ms across a rel's rules
        // and passes), flushed into `_stmt_ms` once at the end so the perf
        // built-in `stmt_ms` can serve it to rails next tick.
        let mut stmt_ms: HashMap<String, i64> = HashMap::new();
        let mut timed = |rel: &str, sql: &str| -> Result<usize> {
            let t = std::time::Instant::now();
            let n = self.db.conn().execute(sql, [])?;
            let ms = t.elapsed().as_millis() as i64;
            let e = stmt_ms.entry(rel.to_string()).or_insert(0);
            if ms > *e { *e = ms; }
            Ok(n)
        };
        for group in stratify(derived_rules)? {
            for (comp_rules, recursive) in rel_components(&group, derived_rules) {
                let stmts: Vec<(&str, String)> = comp_rules.iter()
                    .map(|&ri| Ok((derived_rules[ri].head.rel.as_str(),
                                   lower_rule(derived_rules[ri], &self.rels)?)))
                    .collect::<Result<_>>()?;
                crate::activity::detail(format!(
                    "derived: {}", stmts.iter().map(|(r, _)| *r).collect::<Vec<_>>().join(", ")));
                if !recursive {
                    for (rel, sql) in &stmts { timed(rel, sql)?; }
                    continue;
                }
                // Defense twin of typecheck's `recursive-null-pad`: a NULL-padded
                // head in this component would re-insert the same row every
                // iteration (NULL != NULL never dedups), so the delta never
                // reaches 0. Bail instead of hanging.
                for &ri in &comp_rules {
                    if derived_rules[ri].head_null_pads() {
                        bail!("rule head for `{}` leaves column(s) NULL (`_` or named-arg padding) \
                               inside a recursive component — the fixpoint would not converge; \
                               bind every head column or break the cycle",
                              derived_rules[ri].head.rel);
                    }
                }
                // Semi-naive fallback shapes: an aggregate head has no single
                // new-row identity to differentiate on (COUNT/SUM/etc. must
                // see the WHOLE group every pass); a `key(...)` head (choice
                // domain, with or without `merge(...)`) narrows the PK below
                // the full row, so "new full row" and "new key" disagree —
                // the anti-join-based delta below assumes a full-row PK.
                // Both stay on the naive re-run-to-delta-0 loop (correct,
                // just not accelerated); everything else gets the delta
                // treatment. A mixed component (some rules eligible, some
                // not) falls back as a whole — simplest correct choice, and
                // these shapes are rare inside a recursive cycle in practice.
                // `DL_NAIVE_FIXPOINT=1` forces every recursive component onto
                // the naive loop — the A/B lever the `fixpoint_full_reruns`
                // counter tests (and any field bisection) flip.
                let naive_fallback = self.force_naive_fixpoint.get()
                    || comp_rules.iter().any(|&ri| {
                        let r = derived_rules[ri];
                        r.has_agg() || self.rels.get(&r.head.rel).map(|m| m.key.is_some()).unwrap_or(false)
                    });
                if naive_fallback {
                    let mut iters = 0;
                    loop {
                        let mut delta = 0usize;
                        for (rel, sql) in &stmts { delta += timed(rel, sql)?; }
                        // Every execution after pass 1 is a full-input re-run
                        // (the waste semi-naive removes) — count it.
                        if iters > 0 {
                            self.fixpoint_full_reruns.set(
                                self.fixpoint_full_reruns.get() + stmts.len());
                        }
                        iters += 1;
                        if delta == 0 { break; }
                        if iters > 100_000 { bail!("fixpoint did not converge"); }
                    }
                    continue;
                }
                self.rebuild_derived_seminaive(&comp_rules, derived_rules, &mut timed)?;
            }
        }
        self.save_stmt_ms(&stmt_ms)?;
        // P1: every rel this call rebuilt (deleted + re-derived) just completed
        // a pass, whatever row count it ended with — mark it so the NEXT
        // tick's `derived_incomplete_rels` sees it as legitimately derived,
        // not "never derived".
        self.mark_derived_complete(derived_rels)?;
        if !derived_rels.is_empty() {
            let tick = crate::activity::snapshot().tick;
            let detail = format!("{} rel(s): {}", derived_rels.len(), derived_rels.join(", "));
            crate::perflog::emit_phase(tick, "derived", t_rebuild.elapsed().as_millis() as u64, &detail);
        }
        Ok(())
    }

    /// Semi-naive evaluation of one recursive rel-component. Standard
    /// datalog delta evaluation: instead of every iteration re-running each
    /// rule's FULL join (re-deriving every row ever produced, including all
    /// prior iterations', and discarding the re-derivations on PK conflict),
    /// maintain a `_delta_<rel>` snapshot per targeted rel holding only the
    /// rows born in the PREVIOUS iteration. A rule with a recursive body atom
    /// reruns once per occurrence of that atom (`lower::recursive_occurrences`
    /// / `body_sql_ex`'s `overrides` seam): that ONE occurrence reads the
    /// delta, every other occurrence (recursive or not) reads the full
    /// accumulated relation. This computes exactly the rows NEW this
    /// iteration; naive re-runs of every prior iteration's now-redundant work
    /// are never issued.
    ///
    /// Preconditions enforced by the caller (`rebuild_derived`): every rule in
    /// `comp_rules` heads a rel with a full-row PRIMARY KEY (no `key(...)`,
    /// which would make "new full row" and "new key" disagree) and no
    /// aggregate head (an aggregate must see the whole group, not a delta
    /// slice). Rows/results are otherwise byte-identical to the naive loop:
    /// same PK, same INSERT OR IGNORE dedup, just fewer re-derivations.
    fn rebuild_derived_seminaive(
        &self,
        comp_rules: &[usize],
        derived_rules: &[&Rule],
        timed: &mut dyn FnMut(&str, &str) -> Result<usize>,
    ) -> Result<()> {
        let comp_rels: HashSet<String> = comp_rules.iter()
            .map(|&ri| derived_rules[ri].head.rel.clone()).collect();

        // Partition: a rule with zero recursive-atom occurrences is a base
        // case (seed) — its output never changes across iterations, so it
        // runs exactly once, into the real rel table. A rule with >=1
        // recursive occurrence is differentiated per occurrence each pass.
        let mut seed_ris: Vec<usize> = Vec::new();
        let mut rec_ris: Vec<(usize, Vec<(usize, String)>)> = Vec::new();
        for &ri in comp_rules {
            let occs = crate::lower::recursive_occurrences(derived_rules[ri], &comp_rels);
            if occs.is_empty() { seed_ris.push(ri); } else { rec_ris.push((ri, occs)); }
        }

        for &ri in &seed_ris {
            let rule = derived_rules[ri];
            let sql = lower_rule(rule, &self.rels)?;
            timed(&rule.head.rel, &sql)?;
        }

        if rec_ris.is_empty() { return Ok(()); } // pure base case, no cycle to iterate

        // One `_delta_<rel>`/`_delta_new_<rel>` TEMP table pair per targeted
        // rel, shaped exactly like the rel (same cols, same full-row PK) so
        // `INSERT OR IGNORE` dedups variant output the same way the real
        // table would. Dropped and recreated every call (a TEMP table
        // persists for the connection's lifetime, and a program edit can
        // change a rel's column set between ticks).
        for rel in &comp_rels {
            let meta = self.rels.get(rel)
                .ok_or_else(|| anyhow::anyhow!("unknown relation {rel}"))?;
            let col_defs: Vec<String> = meta.cols.iter()
                .map(|c| format!("\"{}\" {}", c.name, c.ty.sql())).collect();
            let col_names: Vec<String> = meta.cols.iter()
                .map(|c| format!("\"{}\"", c.name)).collect();
            for prefix in ["_delta_", "_delta_new_"] {
                self.db.conn().execute(&format!("DROP TABLE IF EXISTS {prefix}{rel}"), [])?;
                self.db.conn().execute(&format!(
                    "CREATE TEMP TABLE {prefix}{rel} ({}, PRIMARY KEY ({}))",
                    col_defs.join(", "), col_names.join(", ")), [])?;
            }
        }
        // Seed delta_0 = every row the seed rules just produced. `rel_<rel>`
        // was emptied at the top of `rebuild_derived`, so everything in it
        // right now IS new as of this component's first pass.
        for rel in &comp_rels {
            let col_names: Vec<String> = self.rels.get(rel).unwrap().cols.iter()
                .map(|c| format!("\"{}\"", c.name)).collect();
            let cols = col_names.join(", ");
            self.db.conn().execute(&format!(
                "INSERT INTO _delta_{rel} ({cols}) SELECT {cols} FROM {}", tbl(rel)), [])?;
        }

        let mut iters = 0usize;
        loop {
            for rel in &comp_rels {
                self.db.conn().execute(&format!("DELETE FROM _delta_new_{rel}"), [])?;
            }
            // Each recursive-body-atom occurrence gets its own variant
            // statement (the standard semi-naive differentiation): that
            // occurrence reads `_delta_<its rel>`, every other atom (incl.
            // other recursive atoms in the same rule) reads the full table.
            // Variants for the same head rel land in the same `_delta_new_`
            // table, deduped by its PK.
            for (ri, occs) in &rec_ris {
                let rule = derived_rules[*ri];
                let target = format!("_delta_new_{}", rule.head.rel);
                for (k, rel_name) in occs {
                    let mut overrides: HashMap<usize, String> = HashMap::new();
                    overrides.insert(*k, format!("_delta_{rel_name}"));
                    let sql = crate::lower::lower_rule_to_ex(rule, &self.rels, &target, &[], &overrides)?;
                    timed(&rule.head.rel, &sql)?;
                }
            }
            // Subtract rows already present in the real table — a variant
            // reading OTHER atoms at full strength can regenerate a
            // combination whose result was already derived in an earlier
            // iteration. What remains is truly new.
            let mut total_new = 0usize;
            for rel in &comp_rels {
                let col_names: Vec<String> = self.rels.get(rel).unwrap().cols.iter()
                    .map(|c| format!("\"{}\"", c.name)).collect();
                let cols = col_names.join(", ");
                timed(rel, &format!(
                    "DELETE FROM _delta_new_{rel} WHERE ({cols}) IN (SELECT {cols} FROM {})",
                    tbl(rel)))?;
                let n: i64 = self.db.conn().query_row(
                    &format!("SELECT COUNT(*) FROM _delta_new_{rel}"), [], |r| r.get(0))?;
                total_new += n as usize;
            }
            iters += 1;
            if total_new == 0 { break; }
            if iters > 100_000 { bail!("fixpoint did not converge"); }
            // Promote this iteration's new rows into the real relation, and
            // hand them to the NEXT iteration as its delta.
            for rel in &comp_rels {
                let col_names: Vec<String> = self.rels.get(rel).unwrap().cols.iter()
                    .map(|c| format!("\"{}\"", c.name)).collect();
                let cols = col_names.join(", ");
                self.db.conn().execute(&format!(
                    "INSERT OR IGNORE INTO {} ({cols}) SELECT {cols} FROM _delta_new_{rel}",
                    tbl(rel)), [])?;
                self.db.conn().execute(&format!("DELETE FROM _delta_{rel}"), [])?;
                self.db.conn().execute(&format!(
                    "INSERT INTO _delta_{rel} ({cols}) SELECT {cols} FROM _delta_new_{rel}"), [])?;
            }
        }
        Ok(())
    }

    /// Flush one rebuild's per-rel statement timings into `_stmt_ms` (replace
    /// the rebuilt rels' rows, leave the rest — the last known cost of a rel
    /// that did not rebuild this tick stays visible to rails).
    fn save_stmt_ms(&self, stmt_ms: &HashMap<String, i64>) -> Result<()> {
        if stmt_ms.is_empty() { return Ok(()); }
        let names: Vec<String> = stmt_ms.keys().map(|r| format!("'{}'", r.replace('\'', "''"))).collect();
        self.db.exec(&format!("DELETE FROM _stmt_ms WHERE rel IN ({})", names.join(",")))?;
        let rows: Vec<Vec<Value>> = stmt_ms.iter()
            .map(|(rel, ms)| vec![Value::Text(rel.clone()), Value::Int(*ms)]).collect();
        self.db.insert_rows("_stmt_ms", &["rel", "ms"], &rows)?;
        Ok(())
    }

    /// First closure edge (if any) whose SCC node table is empty — unlike
    /// `derived_incomplete_rels`, this is intentionally still a plain row-count
    /// probe (P1 scoped to derived rels only; closures are a separate, usually
    /// small set). Returning the offending edge name (not just a bool) lets
    /// the tick record a `full_reason` of `closure-missing:<edge>`.
    fn first_empty_closure_edge(&self, edges: &[&str]) -> Result<Option<String>> {
        for edge in edges {
            let n: i64 = self.db.conn().query_row(
                &format!("SELECT COUNT(*) FROM {}", scc_node_tbl(edge)), [], |r| r.get(0))?;
            if n == 0 { return Ok(Some(edge.to_string())); }
        }
        Ok(None)
    }

    /// True if any named rel's table is empty or absent — the cold-populate
    /// guard for corpus-gated families (spine): skip the family only when file
    /// content is unchanged AND its rels are already populated. A not-yet-
    /// created table counts as empty (cold start must run).
    fn any_rel_empty(&self, rels: &[&str]) -> Result<bool> {
        for rel in rels {
            let n: i64 = self.db.conn().query_row(
                &format!("SELECT COUNT(*) FROM {}", tbl(rel)), [], |r| r.get(0))
                .unwrap_or(0);
            if n == 0 { return Ok(true); }
        }
        Ok(false)
    }

    /// Load a 2-col edge relation, intern node names to dense u32 (transient),
    /// return adjacency + id->name. No persistent interning (see plan).
    fn load_edges(&self, edge: &str, c0: &str, c1: &str) -> Result<(Vec<Vec<u32>>, Vec<String>)> {
        let sql = format!("SELECT \"{c0}\", \"{c1}\" FROM {}", tbl(edge));
        let mut stmt = self.db.conn().prepare(&sql)?;
        let mut intern: HashMap<String, u32> = HashMap::new();
        let mut names: Vec<String> = Vec::new();
        let mut pairs: Vec<(u32, u32)> = Vec::new();
        let rows = stmt.query_map([], |r| Ok((cell_as_string(r, 0)?, cell_as_string(r, 1)?)))?;
        for row in rows.flatten() {
            let mut id = |s: String| -> u32 {
                if let Some(&i) = intern.get(&s) { return i; }
                let i = names.len() as u32; intern.insert(s.clone(), i); names.push(s); i
            };
            let a = id(row.0); let b = id(row.1);
            pairs.push((a, b));
        }
        let mut adj = vec![Vec::new(); names.len()];
        for (a, b) in pairs { adj[a as usize].push(b); }
        Ok((adj, names))
    }

    /// For each edge relation: condense, then replace its scc_node/scc_edge tables.
    /// The closure VIEW reads these; the Theta(V^2) pair table is never built.
    fn rebuild_closures(&self, edges: &[&str]) -> Result<()> {
        for edge in edges {
            let meta = self.rels.get(*edge)
                .ok_or_else(|| anyhow::anyhow!("closure edge relation {edge} not declared"))?;
            if meta.cols.len() < 2 { bail!("closure edge {edge} must have at least 2 columns"); }
            let (c0, c1) = (meta.cols[0].name.clone(), meta.cols[1].name.clone());
            let (adj, names) = self.load_edges(edge, &c0, &c1)?;
            let cond = scc::build_condensed(&adj);
            let (nt, et) = (scc_node_tbl(edge), scc_edge_tbl(edge));
            let mut node_rows: Vec<Vec<Value>> = Vec::with_capacity(names.len());
            for (id, name) in names.iter().enumerate() {
                let comp = cond.comp[id] as i64;
                let cyc = cond.cyclic[cond.comp[id] as usize] as i64;
                node_rows.push(vec![Value::Text(name.clone()), Value::Int(comp), Value::Int(cyc)]);
            }
            let mut edge_rows: Vec<Vec<Value>> = Vec::new();
            for (cu, succ) in cond.cadj.iter().enumerate() {
                for &cw in succ { edge_rows.push(vec![Value::Int(cu as i64), Value::Int(cw as i64)]); }
            }
            self.db.exec(&format!("DELETE FROM {nt}"))?;
            self.db.exec(&format!("DELETE FROM {et}"))?;
            self.db.insert_rows(&nt, &["name", "comp", "cyclic"], &node_rows)?;
            self.db.insert_rows(&et, &["comp_src", "comp_dst"], &edge_rows)?;
        }
        Ok(())
    }

    fn insert_source_rows(&self, rel: &str, meta: &RelMeta, repo: &str, path: &str, rows: &[Vec<Value>]) -> Result<usize> {
        if rows.is_empty() { return Ok(0); }
        let path_rows: Vec<(String, String, Vec<Value>)> = rows.iter().cloned()
            .map(|row| (repo.to_string(), path.to_string(), row)).collect();
        self.insert_source_rows_for_paths(rel, meta, &path_rows)
    }

    /// Insert source facts plus their `_prov` map rows. Each input is
    /// `(repo slug, path, row)`; `_prov` records `(rel, repo, path, __src)` so
    /// retraction can prune by `(repo, path)` without cross-repo collision.
    fn insert_source_rows_for_paths(&self, rel: &str, meta: &RelMeta, rows: &[(String, String, Vec<Value>)]) -> Result<usize> {
        if rows.is_empty() { return Ok(0); }
        self.insert_spine_strings(rows)?;
        let mut fact_rows: Vec<Vec<Value>> = Vec::with_capacity(rows.len());
        let mut prov_rows: Vec<Vec<Value>> = Vec::with_capacity(rows.len());
        for (repo, path, row) in rows {
            let src = row_hash(row);
            let mut fact = row.clone();
            fact.push(Value::Text(src.clone()));
            fact_rows.push(fact);
            prov_rows.push(vec![
                Value::Text(rel.to_string()),
                Value::Text(repo.to_string()),
                Value::Text(path.to_string()),
                Value::Text(src),
            ]);
        }
        let mut cols: Vec<String> = meta.cols.iter().map(|c| c.name.clone()).collect();
        cols.push("__src".to_string());
        let col_refs: Vec<&str> = cols.iter().map(|c| c.as_str()).collect();
        let table = tbl(rel);
        let inserted = self.db.insert_rows(&table, &col_refs, &fact_rows)?;
        self.db.insert_rows("_prov", &["rel", "repo", "path", "src"], &prov_rows)?;
        Ok(inserted)
    }

    /// Turnkey batched intern: every text cell across `rows` goes through one
    /// `SymSink`, flushed by `Db::flush_syms` (collision-guarded there — two
    /// different texts hashing to the same id within the flush is a loud bail).
    fn insert_spine_strings(&self, rows: &[(String, String, Vec<Value>)]) -> Result<usize> {
        let mut sink = spine::SymSink::new();
        for (_, _, row) in rows {
            for v in row {
                let Value::Text(s) = v else { continue };
                if s.is_empty() { continue; }
                sink.sym(s);
            }
        }
        self.db.flush_syms(&mut sink)
    }

    /// Batch located string occurrences into `_where_bytes`. Each row says
    /// "string S occupies bytes [lo, hi) in file F" — an INSERT-only index keyed
    /// by content-derived `WhereBytesId`, so duplicate occurrences (same string,
    /// same file, same span, reached via multiple binds) collapse to one row.
    /// `(repo, path)` is the source attribution `retract_paths` prunes by on
    /// reparse, and is folded into the row identity via `of_located` so two
    /// byte-identical files keep distinct rows — both within a repo (re-export
    /// stubs) and across two config repos sharing a path (otherwise the second
    /// row is lost on `INSERT OR IGNORE` and retraction misfires). The `repo`
    /// column holds the real slug (matching `_file`/`_prov`), not the vestigial
    /// `w.repo` u32.
    /// `text` (4th tuple slot) is the located source slice. When `Some`, it is
    /// interned into `_strings` under `StringId::of(text)` — the SAME id this
    /// WhereBytes already hashes — so EVERY located id round-trips through both
    /// `ref(id,_,_,lo,hi)` (the span) and `string(id,text,norm)` (the text).
    /// `None` is for callers that intern the text on a separate path (module
    /// spans, which call `insert_spine_strings` first).
    fn insert_spine_where_bytes(&self, wheres: &[(String, String, spine::WhereBytes, Option<String>)]) -> Result<usize> {
        if wheres.is_empty() { return Ok(0); }
        let mut by_id: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        let mut sink = spine::SymSink::new();
        for (repo, path, w, text) in wheres {
            let id = spine::WhereBytesId::of_located(*w, repo, path).to_string();
            by_id.entry(id.clone()).or_insert_with(|| vec![
                Value::Text(id),
                Value::Int(w.string.sqlite()),
                Value::Text(w.file.to_string()),
                Value::Int(w.lo as i64),
                Value::Int(w.hi as i64),
                Value::Text(repo.clone()),
                Value::Text(w.rev.to_string()),
                Value::Text(path.clone()),
            ]);
            if let Some(t) = text {
                if !t.is_empty() { sink.sym(t); }
            }
        }
        self.db.flush_syms(&mut sink)?;
        let rows: Vec<Vec<Value>> = by_id.into_values().collect();
        self.db.insert_rows("_where_bytes", &["id", "string_id", "file_id", "lo", "hi", "repo", "rev", "path"], &rows)
    }


    /// Order-independent content digest of a closure edge relation's `(c0,c1)`
    /// rows. Same edge set ⇒ same digest, regardless of row order; the edge
    /// table is a set (PK), so XOR cannot cancel a duplicate. Lets a tick that
    /// touched the edge's source file skip the recondense when the actual edges
    /// did not move (e.g. a comment edit that leaves call sites unchanged).
    fn edge_content_digest(&self, edge: &str, c0: &str, c1: &str) -> Result<[u8; 32]> {
        let mut acc = [0u8; 32];
        let sql = format!("SELECT \"{c0}\", \"{c1}\" FROM {}", tbl(edge));
        let mut stmt = self.db.conn().prepare(&sql)?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let a = cell_as_string(row, 0)?;
            let b = cell_as_string(row, 1)?;
            let mut h = blake3::Hasher::new();
            h.update(a.as_bytes());
            h.update(&[0]);
            h.update(b.as_bytes());
            for (x, y) in acc.iter_mut().zip(h.finalize().as_bytes().iter()) { *x ^= *y; }
        }
        Ok(acc)
    }

    /// Refresh `self.closure_cache` for the query phase, recondensing an edge
    /// ONLY when its rows actually changed. `dirty` is the set of edges whose
    /// source/derived relation was rebuilt this tick. An edge not in `dirty` is
    /// reused with zero work (no scan, no Tarjan). A dirty edge is digest-checked
    /// first; the Tarjan rebuild runs only if the digest moved. This replaces the
    /// old unconditional per-tick rebuild of every edge's condensation.
    fn refresh_cond_cache(&mut self, edges: &[&str], dirty: &HashSet<&str>) -> Result<()> {
        self.closure_cache.retain(|k, _| edges.iter().any(|e| *e == k.as_str()));
        for &edge in edges {
            let meta = self.rels.get(edge)
                .ok_or_else(|| anyhow::anyhow!("closure edge relation {edge} not declared"))?;
            if meta.cols.len() < 2 { continue; }
            // Unaffected edge already cached → reuse, no scan.
            if !dirty.contains(edge) && self.closure_cache.contains_key(edge) { continue; }
            let (c0, c1) = (meta.cols[0].name.clone(), meta.cols[1].name.clone());
            let digest = self.edge_content_digest(edge, &c0, &c1)?;
            // Dirty but rows unchanged (e.g. comment edit) → reuse, skip Tarjan.
            if self.closure_cache.get(edge).map(|c| c.digest) == Some(digest) { continue; }
            let (adj, names) = self.load_edges(edge, &c0, &c1)?;
            let cond = scc::build_condensed(&adj);
            self.recondensed += 1;
            let id = names.iter().enumerate().map(|(i, n)| (n.clone(), i as u32)).collect();
            self.closure_cache.insert(edge.to_string(), ClosureCache { cond, names, id, digest });
        }
        Ok(())
    }

    /// Answer `reaches(src=SEED, dst=?)` as a seeded BFS over the condensation.
    /// Same row set as the view's src-pinned slice, computed in microseconds.
    /// Seeded closure point query. `forward` = src pinned (walk out, callees);
    /// otherwise dst pinned (walk in, callers). Emits the same rows as the view's
    /// pinned slice, in microseconds.
    fn run_reaches_point(&self, q: &Query, cc: &ClosureCache, seed: &str, forward: bool) -> Result<()> {
        let meta = self.rels.get(&q.head.rel).unwrap();
        let header = |pos: usize| match &q.head.terms[pos] {
            Term::Var(v) => v.clone(),
            _ => meta.col_name(pos).to_string(),
        };
        let mut hits: Vec<&str> = Vec::new();
        if let Some(&sid) = cc.id.get(seed) {
            let walk = if forward { scc::reaches_from(&cc.cond, sid) } else { scc::reached_by(&cc.cond, sid) };
            hits = walk.iter().map(|&i| cc.names[i as usize].as_str()).collect();
            hits.sort_unstable();
        }
        let row = |h: &str| if forward {
            vec![serde_json::Value::String(seed.to_string()), serde_json::Value::String(h.to_string())]
        } else {
            vec![serde_json::Value::String(h.to_string()), serde_json::Value::String(seed.to_string())]
        };
        if self.query_json {
            let rows: Vec<Vec<serde_json::Value>> = hits.iter().map(|h| row(h)).collect();
            emit_query_json(&q.head.rel, &[header(0), header(1)], &rows);
        } else {
            println!("? {} => {}\t{}", q.head.rel, header(0), header(1));
            for h in &hits {
                if forward { println!("  {seed}\t{h}"); } else { println!("  {h}\t{seed}"); }
            }
            println!("  ({} rows)\n", hits.len());
        }
        Ok(())
    }

    /// Both endpoints pinned: an existence probe answered by the seeded walk —
    /// same row semantics as the view's doubly-pinned slice (one row when src
    /// reaches dst, zero otherwise), without evaluating the view.
    fn run_reaches_pair(&self, q: &Query, cc: &ClosureCache, src: &str, dst: &str) -> Result<()> {
        let meta = self.rels.get(&q.head.rel).unwrap();
        let header = |pos: usize| match &q.head.terms[pos] {
            Term::Var(v) => v.clone(),
            _ => meta.col_name(pos).to_string(),
        };
        let hit = match (cc.id.get(src), cc.id.get(dst)) {
            (Some(&sid), Some(&did)) => scc::reaches_from(&cc.cond, sid).contains(&did),
            _ => false,
        };
        let rows: Vec<Vec<serde_json::Value>> = if hit {
            vec![vec![serde_json::Value::String(src.to_string()),
                      serde_json::Value::String(dst.to_string())]]
        } else { Vec::new() };
        if self.query_json {
            emit_query_json(&q.head.rel, &[header(0), header(1)], &rows);
        } else {
            println!("? {} => {}\t{}", q.head.rel, header(0), header(1));
            if hit { println!("  {src}\t{dst}"); }
            println!("  ({} rows)\n", rows.len());
        }
        Ok(())
    }

    /// Evaluate a closure-seedable derived rule (one closure endpoint pinned to a
    /// literal) by a seeded BFS over the cross-tick condensation, writing its head
    /// table. This is the rule-body twin of `run_reaches_point`: same walk, but
    /// the reached set is projected through the head and inserted, not printed.
    /// Runs in the query phase (after `refresh_cond_cache`), so the condensation
    /// is ready; it reads `self.closure_cache` and never recondenses.
    fn eval_closure_seed_rule(&self, rule: &Rule, cs: &ClosureSeed) -> Result<()> {
        let head = &rule.head.rel;
        let head_meta = self.rels.get(head)
            .ok_or_else(|| anyhow::anyhow!("unknown head relation {head}"))?;
        if rule.head.terms.len() != head_meta.cols.len() {
            bail!("head {} expects {} cols, got {}", head, head_meta.cols.len(), rule.head.terms.len());
        }
        self.db.exec(&format!("DELETE FROM {}", tbl(head)))?;
        // The closure atom binds `free_var`; the pinned endpoint var (if any)
        // binds to the seed. The rule head may project these plus literals.
        let pinned_var: Option<&str> = rule.body.iter().find_map(|it| match it {
            BodyItem::Pos(a) if a.rel != *head && a.terms.len() == 2 => {
                // the closure atom: the non-free endpoint's var, if it is a var
                let other = a.terms.iter().find(|t| !matches!(t, Term::Var(v) if *v == cs.free_var));
                match other { Some(Term::Var(v)) => Some(v.as_str()), _ => None }
            }
            _ => None,
        });
        let cells: Result<Vec<Value>> = rule.head.terms.iter().map(|t| match t {
            Term::Str(s) => Ok(Value::Text(s.clone())),
            Term::Int(n) => Ok(Value::Int(*n)),
            Term::Var(v) if *v == cs.free_var => Ok(Value::Text(String::new())), // filled per-row
            Term::Var(v) if Some(v.as_str()) == pinned_var => Ok(Value::Text(cs.seed.clone())),
            Term::Var(v) => bail!("seeded closure rule head var '{v}' is neither the \
                                   free reached endpoint nor the pinned seed; only those \
                                   two (plus literals) can appear in the head"),
            Term::Wild => bail!("'_' not allowed in a seeded closure rule head"),
            Term::Interp(_) => bail!("interpolation not supported in a seeded closure rule head"),
            Term::PathLit { .. } => bail!("path literal not normalized before lowering"),
                Term::Arith { .. } => bail!("arithmetic not supported in a seeded closure rule head"),
                Term::Call { .. } => bail!("function call not supported in a seeded closure rule head"),
        }).collect();
        let template = cells?;
        let free_positions: Vec<usize> = rule.head.terms.iter().enumerate()
            .filter_map(|(i, t)| matches!(t, Term::Var(v) if *v == cs.free_var).then_some(i))
            .collect();

        let Some(cc) = self.closure_cache.get(&cs.edge) else { return Ok(()); };
        let mut rows: Vec<Vec<Value>> = Vec::new();
        if let Some(&sid) = cc.id.get(&cs.seed) {
            let walk = if cs.forward { scc::reaches_from(&cc.cond, sid) } else { scc::reached_by(&cc.cond, sid) };
            for &i in &walk {
                let name = &cc.names[i as usize];
                let mut row = template.clone();
                for &p in &free_positions { row[p] = Value::Text(name.clone()); }
                rows.push(row);
            }
        }
        let cols: Vec<&str> = head_meta.cols.iter().map(|c| c.name.as_str()).collect();
        self.db.insert_rows(&tbl(head), &cols, &rows)?; // one flush, never N+1
        Ok(())
    }

    /// Evaluate `head(rep, member) <- scc(edge).` — materialize SCC membership
    /// from `edge`'s already-computed condensation. One row per node: (component
    /// representative, node), where the representative is the lexicographically-min
    /// member name in the component (a stable, readable cluster id). Shares
    /// `closure(edge)`'s condensation cache — no second Tarjan run. Binds
    /// positionally: head col 0 = rep, col 1 = member. Runs in the query phase
    /// after `refresh_cond_cache`, so the cond is ready; the head is otherwise
    /// excluded from `rebuild_derived` (the Scc body item can't lower to SQL).
    fn eval_scc_rule(&self, rule: &Rule) -> Result<()> {
        let edge = rule.scc_edge()
            .ok_or_else(|| anyhow::anyhow!("eval_scc_rule on a non-scc rule"))?;
        let head = &rule.head.rel;
        let head_meta = self.rels.get(head)
            .ok_or_else(|| anyhow::anyhow!("unknown head relation {head}"))?;
        if rule.head.terms.len() != 2 || head_meta.cols.len() != 2 {
            bail!("scc head '{head}' must have exactly 2 columns (rep, member); got {}",
                  head_meta.cols.len());
        }
        self.db.exec(&format!("DELETE FROM {}", tbl(head)))?;
        let Some(cc) = self.closure_cache.get(edge) else {
            return Ok(()); // edge not condensed this tick (e.g. empty) -> head empty
        };
        let mut rows: Vec<Vec<Value>> = Vec::new();
        for c in 0..cc.cond.ncomp {
            let members = &cc.cond.members[c];
            if members.is_empty() { continue; }
            let rep = members.iter()
                .map(|&i| cc.names[i as usize].as_str())
                .min()
                .unwrap_or("");
            for &m in members {
                rows.push(vec![
                    Value::Text(rep.to_string()),
                    Value::Text(cc.names[m as usize].clone()),
                ]);
            }
        }
        let cols: Vec<&str> = head_meta.cols.iter().map(|c| c.name.as_str()).collect();
        self.db.insert_rows(&tbl(head), &cols, &rows)?; // one flush, never N+1
        Ok(())
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
struct GitBatch {
    // Held only so the process handle isn't dropped while the pipes live.
    _child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: std::io::BufReader<std::process::ChildStdout>,
}

impl GitBatch {
    fn open(root: &Path) -> Result<GitBatch> {
        let mut child = Command::new("git")
            .arg("-C").arg(root)
            .args(["cat-file", "--batch"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = std::io::BufReader::new(child.stdout.take().expect("piped stdout"));
        Ok(GitBatch { _child: child, stdin, stdout })
    }

    fn read(&mut self, rev: &str, path: &str) -> Result<String> {
        use std::io::{BufRead, Read, Write};
        writeln!(self.stdin, "{rev}:{path}")?;
        self.stdin.flush()?;
        // header: `<oid> <type> <size>` or `<object> missing` / `... ambiguous`
        let mut header = String::new();
        self.stdout.read_line(&mut header)?;
        let header = header.trim_end();
        let size: usize = match header.rsplit(' ').next().and_then(|s| s.parse().ok()) {
            Some(n) => n,
            None => bail!("git cat-file failed for {rev}:{path} ({header})"),
        };
        let mut buf = vec![0u8; size + 1]; // content + trailing LF
        self.stdout.read_exact(&mut buf)?;
        buf.pop();
        Ok(String::from_utf8_lossy(&buf).to_string())
    }
}

fn git_batch_read(root: &Path, rev: &str, path: &str) -> Result<String> {
    static BATCHES: OnceLock<std::sync::Mutex<HashMap<PathBuf, std::sync::Arc<std::sync::Mutex<GitBatch>>>>> = OnceLock::new();
    let map = BATCHES.get_or_init(Default::default);
    let batch = {
        let mut m = map.lock().unwrap();
        match m.entry(root.to_path_buf()) {
            std::collections::hash_map::Entry::Occupied(e) => e.get().clone(),
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(std::sync::Arc::new(std::sync::Mutex::new(GitBatch::open(root)?))).clone()
            }
        }
    };
    let mut b = batch.lock().unwrap();
    b.read(rev, path)
}

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
type TsLangCtor = fn() -> tree_sitter::Language;
static AST_LANG_TABLE: &[(&str, &[&str], TsLangCtor)] = &[
    ("rust",       &["rs"],                    || tree_sitter::Language::new(tree_sitter_rust::LANGUAGE)),
    ("c",          &[],                        || tree_sitter::Language::new(tree_sitter_c::LANGUAGE)),
    ("kotlin",     &["kt"],                    || tree_sitter::Language::new(tree_sitter_kotlin_sg::LANGUAGE)),
    ("python",     &["py"],                    || tree_sitter::Language::new(tree_sitter_python::LANGUAGE)),
    ("bash",       &["sh", "shell"],           || tree_sitter::Language::new(tree_sitter_bash::LANGUAGE)),
    ("go",         &["golang"],                || tree_sitter::Language::new(tree_sitter_go::LANGUAGE)),
    ("hcl",        &["terraform", "tf"],       || tree_sitter::Language::new(tree_sitter_hcl::LANGUAGE)),
    ("starlark",   &["bzl", "bazel"],          || tree_sitter::Language::new(tree_sitter_starlark::LANGUAGE)),
    ("jsonnet",    &[],                        || tree_sitter::Language::new(tree_sitter_jsonnet::LANGUAGE)),
    ("gotmpl",     &["gotemplate", "gohtml"],  || tree_sitter::Language::new(unsafe {
        tree_sitter_language::LanguageFn::from_raw(tree_sitter_gotmpl)
    })),
    ("dockerfile", &["docker"],                || tree_sitter::Language::new(unsafe {
        tree_sitter_language::LanguageFn::from_raw(tree_sitter_dockerfile)
    })),
];

fn ts_lang(lang: &str) -> Result<tree_sitter::Language> {
    for (canon, aliases, ctor) in AST_LANG_TABLE {
        if lang == *canon || aliases.contains(&lang) { return Ok(ctor()); }
    }
    let compiled = AST_LANG_TABLE.iter().map(|(c, ..)| *c).collect::<Vec<_>>().join(", ");
    bail!("no ast grammar for :{lang} (compiled in: {compiled})")
}

/// Canonical language names the `ast` op accepts (one per tree-sitter grammar).
/// The skill's per-op language matrix is checked set-equal against this in
/// `tests/it/lang_matrix.rs`, so a stale matrix fails CI.
pub fn ast_langs() -> Vec<&'static str> {
    AST_LANG_TABLE.iter().map(|(canon, ..)| *canon).collect()
}

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
