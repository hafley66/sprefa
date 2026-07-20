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

pub(crate) use revid::{GitOid, RevId, WorktreeRev, WORK_ALIAS};

// The effect runtime moved to crate::effect (engine breakdown Stage 5).
// Re-export the names external call sites (daemon, tests) and the rest of
// engine.rs reach via `engine::`, so their paths keep resolving.
use crate::effect::async_bound_vars;
pub use crate::effect::{async_effect_arity, shell_templates, EffectExec, ShellEffectExec};

// Built-in graph/CST/spine/daemon extractor methods (bucket E) live in a child
// module to shrink this file; they're still `impl Engine` methods called as
// `self.refresh_*` from the tick orchestrator (engine breakdown Stage 4).
mod cold_stage;
mod declare;
#[cfg(test)]
mod deltaflow;
mod derive;
pub(crate) mod extract;
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
mod results;
pub(crate) mod revid;
mod rpc;
mod source_prepare;
mod source_rows;
#[cfg(test)]
mod staged_delta;
mod symbols;
mod term_extract;
mod timeutil;
pub(crate) use query::{emit_query_json, emit_query_json_rows, QueryOutputFormat};
pub(crate) use repo::git_batch_read;
pub use results::{
    DiagRow, HierarchyCallEdge, HierarchyItem, LocateHit, QueryResult, RefHit, RefLens, SpineDelta,
    SymbolRow,
};
pub(crate) use timeutil::{iso8601_utc_now, mtime_secs, unix_secs};
pub(crate) use family::{
    CALL_RELS, CLOCK_RELS, COMMENT_RELS, CONST_VALUE_RELS, DAEMON_RELS, DATAFLOW_RELS, DEMAND_RELS,
    DIAG_RELS, DIAG_STAGE_RELS, DOC_RELS, DOC_TEXT_RELS, EFFECT_RELS, EVERY_RELS, GRAPH_RELS,
    HOOK_RELS, HOVER_RELS, MODULE_RELS, MUTE_RELS, NODE_RELS, SPINE_RELS, TEMPLATE_RELS,
    TYPE_DECL_RELS, TYPE_RELS, UNRESOLVED_RELS,
};
mod decls;
pub(crate) use decls::*;
pub use decls::{
    all_builtin_decls, builtin_enum_brands, builtin_enum_variants, builtin_rel_names, fn_docs,
    op_docs, undocumented_builtins, undocumented_fns,
};
pub use lang_tables::ast_langs;
pub(crate) use lang_tables::{ts_lang, ts_lang_resolved};
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
pub(crate) fn now_secs() -> i64 {
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
fn cell_as_string(r: &crate::db::SqlRow, i: usize) -> crate::db::SqlRowResult<String> {
    use crate::db::SqlValueRef;
    Ok(match r.get_ref(i)? {
        SqlValueRef::Null => String::new(),
        SqlValueRef::Integer(n) => n.to_string(),
        SqlValueRef::Real(f) => f.to_string(),
        SqlValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
        SqlValueRef::Blob(b) => String::from_utf8_lossy(b).into_owned(),
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
pub(crate) fn hex_to_32(s: &str) -> Result<[u8; 32]> {
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
    /// Working-tree resolutions of the `WORK` alias, one per repo root. Cleared
    /// each tick alongside `rev_cache`, and by `invalidate` on a git event, so a
    /// tick spanning a commit uses ONE oid throughout. Kept apart from
    /// `rev_sha_cache` on purpose — see `cache_rev`'s hazard note.
    worktree_rev: WorktreeRev,
    /// Stored rev texts this tick's alias resolutions produced. The worktree
    /// predicate for a rev read back OUT of storage, where no `RevId` is in hand
    /// (extraction closures receive `_file.rev` text). A clean tree's
    /// `scan("HEAD", …)` matches this set too, which is correct: when the tree is
    /// clean the filesystem and the committed blob hold the same bytes.
    worktree_rev_texts: HashSet<String>,
    /// This engine's own working-tree rev, resolved at the top of every tick.
    /// The stand-in for code that used to write the literal `WORK` meaning "my
    /// own working tree" (the call/module family digests, the module refresh
    /// entry points).
    self_rev: RevId,
    /// (repo slug, rev, path) of every tracked source file this tick — the
    /// existence oracle for `:file`/`:path`/`:dir` type checks against off-disk
    /// revs (where the filesystem cannot answer).
    rev_index: std::collections::HashSet<(String, String, String)>,
    /// Repos registered via the turnkey config. When non-empty the `repo`
    /// relation lists these instead of the single `--root`. Reloaded by the
    /// watcher when the config file changes. (File ingestion from the extra
    /// roots is the next step; today only `--root` is scanned into `_file`.)
    repos: Vec<crate::config::RepoConfig>,
    /// (dropped-slug, kept-slug) dedup collisions already reported, so the
    /// once-per-engine "two slugs, one directory" line prints only the first
    /// time each pair is dropped, not on every `set_repos` reload (cold tick +
    /// each config-file change).
    logged_repo_dedup: HashSet<(String, String)>,
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
    /// Auto-index demand probes (storage-diet Direction 5, planner-honest
    /// demand): per (rel, col) join-key candidate, the rel's row-digest
    /// fingerprint the size/selectivity stats were probed at plus the verdict.
    /// Kept ACROSS ticks; a candidate is reprobed only when its rel's
    /// `_reldigest` fingerprint moves (the recompute-guard digest-skip idiom),
    /// so a quiet tick pays one bulk digest read and zero data scans.
    idx_demand_cache: std::cell::RefCell<HashMap<(String, String), declare::IdxDemandProbe>>,
    /// Persistent reactive router for the family-derive call-rel flip — the
    /// SOLE writer of every public call rel (P4, capstone cutover). Holds a
    /// per-family memo (rows + rel footprint) across ticks so `react` reruns
    /// only families whose inputs a tick touched. `None` until the first
    /// flip; retained for the engine's life (unlike `adjacency_cache`, the
    /// memo is the point).
    call_router: std::cell::RefCell<Option<family::FamilyRouter<'static>>>,
    /// How `?` query results print: the human TSV block (default), NDJSON
    /// (`--query-json`), or JSON row-arrays (`--format json`). See
    /// `QueryOutputFormat`'s doc for the shape of each.
    query_format: query::QueryOutputFormat,
    /// When true, skip the `?` query-evaluation pass at the end of a tick. Used
    /// for the foreground one-shot's PRIMING tick: a data-driven scan or
    /// repo-sink reads last tick's coordinate/pull state, so a fresh run has
    /// nothing to read on tick 1. The priming tick derives the coordinates (and
    /// pulls repos) silently; the follow-up tick reads them and prints answers.
    prime_tick: bool,
    /// This engine is ticked repeatedly by a scheduler (daemon poll loop,
    /// `--settle`, `--watch`). In that mode, bookkeeping-family motion
    /// (`stmt_ms`/`rel_count`/`query_log`) must not seed the scoped derived
    /// rebuild: the tick writes those rels' inputs itself and rebuilding their
    /// dependents re-jitters the timings they report, so the loop never
    /// converges (75GB/2.7h of diag-rail rebuilds, 2026-07-17). A one-shot
    /// tick (default false) cannot loop, and the perf rails' documented
    /// second-invocation contract depends on bookkeeping motion counting there.
    pub poll_loop: bool,
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
    /// Files read and re-hashed by the working-tree walk this process, i.e. the
    /// `_file` mtime/size fast path's MISSES. Test/bench instrumentation in the
    /// `extract_files_parsed` idiom; an unchanged corpus must not move it.
    pub file_hash_reads: std::cell::Cell<usize>,
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
    /// Test-only crash-window hook: when set to `Some(rel)`, `rebuild_derived`
    /// bails right AFTER that rel's component is unmarked + wiped but BEFORE it
    /// runs (or is re-marked), simulating a SIGKILL between the wipe and the
    /// refill. Lets a test assert that completed components stay marked+populated
    /// while the interrupted one reads incomplete+empty. Set the field directly
    /// (per-engine, no process-global env race); `None` in production.
    pub fail_rebuild_at_rel: std::cell::RefCell<Option<String>>,
    /// Test/bench instrumentation: the pre-stratum derived rels the LAST tick
    /// (full or incremental) actually rebuilt. A scoped tick (perf gap B) lists
    /// only the rels dependency-reachable from what changed; a full rebuild
    /// lists every pre-stratum rel; a no-change/comment-only tick leaves it
    /// empty. The structural proof the tick's rebuild is affected-scoped, not a
    /// full re-derivation on every edit.
    pub last_derived_rebuilt: Vec<String>,
    /// Digest-before-write instrumentation (failure-modes class 3 residual /
    /// class 7 quiet-tick budget): the subset of `last_derived_rebuilt` whose
    /// re-derivation landed on rows identical to the live table, so the
    /// unmark/wipe/refill/mark bracket was skipped and ZERO main-db writes
    /// were issued for them. Reset per tick.
    pub last_derived_skipped: Vec<String>,
    /// Verify-rollback journal (christmas #14). `None` = not in verify mode (gen
    /// writes go straight to disk, no capture). `Some(...)` = every gen write
    /// first stashes the target's original bytes (`None` entry = the file did not
    /// exist) so `rollback_writes` can restore the tree if a checker fails. One
    /// entry per path, first-write wins (the pre-tick state).
    gen_journal: std::cell::RefCell<Option<Vec<(String, Option<Vec<u8>>)>>>,
    /// Per-tick memo for `exe_identity_changed_since_last_run`. Computed on the
    /// first extraction-family lookup within a tick and cleared at tick
    /// completion so a real binary swap causes exactly one full rebuild cycle per
    /// Engine, not once per process (a process-global cache poisoned every root
    /// in a multi-root daemon and pinned `true` forever after a swap).
    exe_identity_changed: std::cell::Cell<Option<bool>>,
    /// Whether the LAST `flip_call_rels_via_router` actually moved any public
    /// call-rel row (a cold reload or a non-empty delta). `refresh_call_rels`
    /// reads it to report honest change: an exe-swap re-derive that reproduces
    /// identical rows must not mark the call rels changed — that mark cascaded
    /// the flow rails (`flow_edge`, `port_of_reach_*`) into a 2.6GB derived
    /// rebuild on every daemon respawn after a reinstall.
    pub(crate) call_flip_moved: std::cell::Cell<bool>,
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
    /// Effect kinds `drain_effects`/`drain_streams` has already warned about
    /// having no registered executor template (daemon CPU-hog fix, Part 2).
    /// Kept for the engine's life (one `Engine` per served root), never
    /// cleared, so an orphaned kind logs exactly once per root regardless of
    /// how many polls or how many distinct request ids it re-queues under.
    pub(crate) warned_orphan_effect_kinds: std::cell::RefCell<std::collections::HashSet<String>>,
    /// In-memory derived-side write ledger for this tick. Complements the
    /// source-side ledger kept inside `Db`; drained and flushed into
    /// `_write_ledger` once at tick end.
    write_ledger: std::cell::RefCell<Vec<(String, usize, String)>>,
    /// Monotonic per-engine tick sequence number, used to timestamp ledger rows.
    /// Starts at 1 on the first tick that runs.
    tick_seq: std::cell::Cell<i64>,
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
            idx_demand_cache: std::cell::RefCell::new(HashMap::new()),
            call_router: std::cell::RefCell::new(None),
            rev_cache: HashMap::new(),
            rev_sha_cache: HashMap::new(),
            worktree_rev: WorktreeRev::default(),
            worktree_rev_texts: HashSet::new(),
            self_rev: RevId::no_head(),
            rev_index: std::collections::HashSet::new(),
            repos: Vec::new(),
            logged_repo_dedup: HashSet::new(),
            query_format: query::QueryOutputFormat::Text,
            prime_tick: false,
            root_implicit: false,
            poll_loop: false,
            last_n1: None,
            last_node_files_walked: std::cell::Cell::new(0),
            extract_files_parsed: std::cell::Cell::new(0),
            file_hash_reads: std::cell::Cell::new(0),
            force_separate_analysis_extractors: std::cell::Cell::new(
                std::env::var("DL_DISABLE_ANALYSIS_BUNDLE").ok().as_deref() == Some("1"),
            ),
            fixpoint_full_reruns: std::cell::Cell::new(0),
            force_naive_fixpoint: std::cell::Cell::new(
                std::env::var("DL_NAIVE_FIXPOINT").ok().as_deref() == Some("1"),
            ),
            fail_rebuild_at_rel: std::cell::RefCell::new(None),
            last_derived_rebuilt: Vec::new(),
            last_derived_skipped: Vec::new(),
            gen_journal: std::cell::RefCell::new(None),
            exe_identity_changed: std::cell::Cell::new(None),
            call_flip_moved: std::cell::Cell::new(false),
            type_facts_cache: Default::default(),
            call_facts_cache: Default::default(),
            df_facts_cache: Default::default(),
            comment_facts_cache: Default::default(),
            template_facts_cache: Default::default(),
            unresolved_facts_cache: Default::default(),
            warned_orphan_effect_kinds: std::cell::RefCell::new(std::collections::HashSet::new()),
            write_ledger: std::cell::RefCell::new(Vec::new()),
            tick_seq: std::cell::Cell::new(0),
        }
    }

    /// Set how `?` query results print (`QueryOutputFormat::Text`/`Ndjson`/
    /// `JsonRows`); see that type's doc for the shape of each.
    pub fn set_query_format(&mut self, format: QueryOutputFormat) {
        self.query_format = format;
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

    /// Set the configured repos (from `SprfConfig`), deduplicated by canonical
    /// root path. Takes effect on the next tick via `refresh_builtin_rels`.
    pub fn set_repos(&mut self, repos: Vec<crate::config::RepoConfig>) {
        self.repos = self.dedupe_repos(repos);
    }

    /// Drop config repos whose canonical root path collides with an
    /// already-registered repo, so one directory is never scanned twice under
    /// two slugs (the tmpdir-plus-symlink case). Repo identity within one engine
    /// is the canonicalized root; the first slug at a path wins. Each dropped
    /// pair logs once per engine (naming both slugs), not on every reload.
    ///
    /// This folds config-vs-config only. The engine's own `--root` is NOT seeded
    /// into the dedup set: in an ad-hoc CLI run the root is the transient cwd,
    /// and folding a config repo that happens to equal it would silently strip
    /// that slug from `scan("*")`, the `repo` relation, and named-repo
    /// resolution (`resolve_repo`), changing CLI results, which must stay exact.
    /// The engine-root-equals-config double-scan the friction inventory measured
    /// happens on a DAEMON-SERVED engine, and is killed upstream by Rule B (a
    /// served engine ingests no ambient config at all; see `served_repos`), so
    /// there is nothing left here to fold against self.
    fn dedupe_repos(
        &mut self,
        repos: Vec<crate::config::RepoConfig>,
    ) -> Vec<crate::config::RepoConfig> {
        let canon =
            |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let mut seen: HashMap<PathBuf, String> = HashMap::new();
        let mut kept = Vec::with_capacity(repos.len());
        for rc in repos {
            let key = canon(&rc.root);
            if let Some(first_slug) = seen.get(&key) {
                let pair = (rc.slug.clone(), first_slug.clone());
                if self.logged_repo_dedup.insert(pair) {
                    let dir = key.display();
                    tracing::warn!(
                        repo = ?rc.slug,
                        other = ?first_slug,
                        dir = %dir,
                        kept = ?first_slug,
                        "[config] repo {:?} and {:?} resolve to the same directory {dir}; keeping {:?}",
                        rc.slug,
                        first_slug,
                        first_slug,
                    );
                }
                continue;
            }
            seen.insert(key, rc.slug.clone());
            kept.push(rc);
        }
        kept
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

    /// Record one derived-side write in the tick's in-memory ledger.
    /// Zero-row writes are dropped; aggregating happens at flush time.
    pub(crate) fn record_write(&self, rel: &str, rows: usize, seam: &str) {
        if rows == 0 {
            return;
        }
        self.write_ledger
            .borrow_mut()
            .push((rel.to_string(), rows, seam.to_string()));
    }

    /// Flush the tick's source + derived write ledger into `_write_ledger` in
    /// ONE batched insert, then prune rows older than 200 ticks. Called exactly
    /// once per tick after all writes have landed.
    pub(crate) fn flush_write_ledger(&self, tick: i64) -> Result<()> {
        use std::collections::BTreeMap;
        let mut combined: BTreeMap<(String, String), usize> = BTreeMap::new();
        for (rel, rows) in self.db.take_write_ledger() {
            *combined
                .entry((rel, "source".to_string()))
                .or_insert(0) += rows;
        }
        for (rel, rows, seam) in self.write_ledger.borrow_mut().drain(..) {
            *combined.entry((rel, seam)).or_insert(0) += rows;
        }
        // Retention: keep the last 200 ticks. Safe on first ticks (tick - 200
        // underflows i64 to a large negative, deleting nothing).
        self.db.exec_params(
            "_write_ledger",
            "DELETE FROM _write_ledger WHERE tick < ?1",
            &[crate::db::SqlVal::from(tick.saturating_sub(200))],
        )?;
        let rows: Vec<Vec<Value>> = combined
            .into_iter()
            .filter(|(_, rows)| *rows > 0)
            .map(|((rel, seam), rows)| {
                vec![
                    Value::Int(tick),
                    Value::Text(rel),
                    Value::Int(rows as i64),
                    Value::Text(seam),
                ]
            })
            .collect();
        if !rows.is_empty() {
            self.db
                .insert_rows("_write_ledger", &["tick", "rel", "rows", "seam"], &rows)?;
        }
        Ok(())
    }

    /// Returns whether the table's content actually moved. A rebuild that
    /// reproduces byte-identical rows (the classic case: a binary swap opened
    /// a fresh digest namespace and re-extracted an unchanged corpus) must not
    /// rewrite the table — the whole-table DELETE+insert of every big rel was
    /// measured at 4.5GB of WAL per daemon boot. Skip = digest match over the
    /// encoded rows PLUS a live COUNT(*) guard (a sweep or hand edit that
    /// changed the table behind the digest's back forces the write).
    pub(crate) fn refresh_rel(
        &self,
        rel: &str,
        cols: &[&str],
        rows: &[Vec<Value>],
    ) -> Result<bool> {
        let table = tbl(rel);
        let start = std::time::Instant::now();
        let encoded = self.encode_rel_rows(rel, cols, rows)?;
        let content_key = format!("rows:{rel}");
        let digest = rows_content_digest(cols, &encoded, &[]);
        if self.load_rel_digest(&content_key)? == Some(digest) {
            let live: i64 = self
                .db
                .query_one(rel, &format!("SELECT COUNT(*) FROM {table}"), &[], |r| Ok(r.get(0)?))
                .unwrap_or(-1);
            if live == encoded.len() as i64 {
                crate::verdict::debug_verdict(
                    "rel-refresh",
                    &format!("[write] {rel}: skipped (rows identical)"),
                    &[("rel", rel), ("outcome", "skip")],
                );
                return Ok(false);
            }
        }
        // Whole-table reload with index drop/rebuild for large rels (see
        // Db::reload_rel); DELETE + plain insert for small ones.
        let n = self
            .db
            .reload_rel(&table, cols, &encoded)
            .with_context(|| format!("refresh relation {rel}"))?;
        self.save_rel_digest(&content_key, &digest)?;
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
        let _ = n;
        Ok(true)
    }

    /// Cold-chunk append: encode `rows` exactly as `refresh_rel` would (interned
    /// columns re-symmed) and `INSERT OR IGNORE` them into `rel` WITHOUT the
    /// wholesale `DELETE` and WITHOUT saving a content digest. The append is the
    /// per-file-slice write path for the barrier-free extraction families under
    /// cold-start chunking (`cold_stage.rs`): each chunk contributes its slice's
    /// rows to an initially-empty rel; `INSERT OR IGNORE` makes a re-run of a
    /// crash-interrupted chunk idempotent and dedups the rare row a
    /// content-addressed id shares across two slices. The family's `extract:`
    /// digest is saved once at the completion gate, not here, so the completion
    /// tick's wholesale refresh skips the already-appended family.
    pub(crate) fn append_rel(&self, rel: &str, cols: &[&str], rows: &[Vec<Value>]) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let encoded = self.encode_rel_rows(rel, cols, rows)?;
        self.db
            .insert_rows(&crate::lower::tbl(rel), cols, &encoded)
            .with_context(|| format!("append relation {rel}"))?;
        Ok(())
    }
}

/// Order-independent, duplicate-sensitive digest of encoded rows: per-row
/// blake3 folded by wrapping-add (so row order — a rayon artifact — never
/// perturbs it, while a duplicated row still moves it), finalized with the
/// row count, the column list, and any scope tags (`refresh_rel_for_revs`
/// folds its rev set so a different delete scope never digest-matches).
pub(crate) fn rows_content_digest(cols: &[&str], rows: &[Vec<Value>], scope: &[&str]) -> [u8; 32] {
    let mut sum = [0u64; 4];
    for row in rows {
        let mut h = blake3::Hasher::new();
        for cell in row {
            match cell {
                Value::Null => { h.update(b"\x00"); }
                Value::Int(i) => { h.update(b"\x01"); h.update(&i.to_le_bytes()); }
                Value::Text(s) => { h.update(b"\x02"); h.update(s.as_bytes()); }
            }
            h.update(b"\x1f");
        }
        let d = h.finalize();
        for (k, s) in sum.iter_mut().enumerate() {
            *s = s.wrapping_add(u64::from_le_bytes(d.as_bytes()[k * 8..k * 8 + 8].try_into().unwrap()));
        }
    }
    let mut out = blake3::Hasher::new();
    out.update(&(rows.len() as u64).to_le_bytes());
    for c in cols { out.update(c.as_bytes()); out.update(b","); }
    for s in scope { out.update(s.as_bytes()); out.update(b";"); }
    for s in sum { out.update(&s.to_le_bytes()); }
    *out.finalize().as_bytes()
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
    rev: RevId,
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

/// Read one file's content at a stored rev.
///
/// The read path is decided from the rev text alone, which is sound because a
/// rev WITHOUT the `+` marker is byte-identical on disk and in the git object
/// (the marker is exactly the statement that they differ). A dirty worktree rev
/// has no git object to read, so it takes the filesystem; a clean one takes the
/// cheaper object read whether it was scanned as `WORK` or as an explicit rev.
/// Does this stored rev text name bytes that live only in the working tree?
/// True for the `+` marker (the tree differed from its oid) and for anything
/// that is not a rev at all, so an unexpected value degrades to the filesystem
/// read rather than a git failure.
pub(crate) fn rev_text_is_dirty_worktree(rev: &str) -> bool {
    RevId::parse(rev).map(|parsed| parsed.dirty()).unwrap_or(true)
}

pub(crate) fn read_content(root: &Path, rev: &str, path: &str) -> Result<String> {
    match GitOid::of(rev) {
        Some(oid) => git_batch_read(root, oid, path),
        // The dirty marker: these bytes exist only on disk.
        None => Ok(std::fs::read_to_string(root.join(path))?),
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
    if !rev_text_is_dirty_worktree(rev) {
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
