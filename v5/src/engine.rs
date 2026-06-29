use anyhow::{bail, Result};
use rayon::prelude::*;
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use crate::ast::*;
use crate::ingest;
use crate::lower::{lower_query, lower_rule, tbl};
use crate::modgraph::{self, ProjectCx, Resolution};
use crate::scc;
use crate::scip_import;
use crate::spine;
use crate::typegraph;

fn scc_node_tbl(edge: &str) -> String { format!("scc_node_{edge}") }
fn scc_edge_tbl(edge: &str) -> String { format!("scc_edge_{edge}") }

/// Brute-force top-k cosine neighbors over an L2-normalized vector pool, emitted
/// as `(a, b, score)` rows with `score = round(cosine * 1e6)` as Int. Shared by
/// the text `similar` rel and the structural `node2vec` rel — both reduce to
/// "nearest neighbors over a `Vec<(id, vec)>`", only the vectors differ.
fn knn_rows(pool: &[(String, Vec<f32>)], k: usize) -> Vec<Vec<Value>> {
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
pub fn tick_audit() -> bool {
    TICK_AUDIT.load(std::sync::atomic::Ordering::Relaxed)
        || std::env::var("DL_TICK_AUDIT").is_ok_and(|v| !v.is_empty() && v != "0")
}

/// The module-graph relations (modgraph.rs). Reserved like BUILTIN_RELS, declared
/// every tick, but populated by `refresh_module_rels` only when the program
/// references one (resolution parses every file, so it is lazy). `module_edge` is
/// the 2-col convenience closure edge; `module_edge_rev` is the rev-aware form.
const MODULE_RELS: [&str; 6] = [
    "module_import",
    "module_edge",
    "module_edge_rev",
    "module_unresolved",
    "module_unresolved_rev",
    "crate_edge",
];

/// Syntax-only type graph. `kind` is edge metadata; closure(type_edge) walks
/// the first two columns. `type_edge`/`type_edge_rev` are name-keyed (the
/// historic contract). The sem-style additions are def-keyed: `type_entity`
/// is the declared-symbol table (kind, parent, location), `type_sig` is each
/// callable's arrow `[...A] => B` exploded by slot, and `type_link` is the
/// SCIP-resolved graph where endpoints are definition symbols, not bare names.
const TYPE_RELS: [&str; 5] =
    ["type_edge", "type_edge_rev", "type_entity", "type_sig", "type_link"];

/// Phase D diet-SCIP call graph. `call_def` is each callable (sym, kind, file,
/// span); `call_site` is each call occurrence (caller sym, callee text, file,
/// line); `call_edge` is the resolved closure edge; `call_edge_rev` is the
/// rev-aware source of truth (same split as type_edge / type_edge_rev).
/// `call_kind` is the per-fn read/write classification of those call sites,
/// keyed by the bare callee name (execute/query_row/etc.) so a rail can join
/// on `write` only. Symbols are `file::kind::name`, the same shape
/// `type_entity` uses, so the call and type graphs share nodes and a join
/// reaches both.
const CALL_RELS: [&str; 6] = ["call_def", "call_site", "call_edge", "call_edge_rev", "call_name", "call_kind"];

/// Intra-procedural dataflow lift: `df_node(id, kind, var, fn, file, line)` is a
/// value-bearing program point, `df_edge(from, to)` is local value flow. A rule
/// `df_reaches(a,b) <- closure(df_edge)` walks the lifted graph on the shared SCC
/// engine. `loop_over` records each loop's span + variable for the
/// loop-invariant-call flag; `allocates` marks fns whose body builds a
/// collection; `nest(call_id, loop_id, depth, collection)` records each call's
/// enclosing loop nest, composing over `call_edge` into symbolic Big-O
/// ("depth-N over C") without resolving trip counts. See `typegraph::DataflowFacts`.
const DATAFLOW_RELS: [&str; 6] = ["df_node", "df_edge", "loop_over", "allocates", "nest", "df_param"];

/// Document structure from non-source text (markdown today; comments and other
/// tree-sitter grammars to follow via `ingest::IngestLang`). `doc_node` is one row
/// per heading / code block / section: (file, line, kind, name, parent). The
/// `parent` column is the enclosing heading text, so a rule can walk the section
/// tree. `doc_ref` is the doc→code bridge: (file, line, sym) where a heading's
/// name matches a `type_entity` name. Populated by the `ingest` registry over
/// `_file`'s document-typed files (a source rule scanning `**/*.md` feeds `_file`,
/// same as the source langs).
const DOC_RELS: [&str; 2] = ["doc_node", "doc_ref"];

/// Doc comments attached to declared entities (Tier 1/2 doc gen). `doc_comment`
/// is one row per documented `type_entity`: (repo, sym, line, text), the cleaned
/// block bound to the same sym. `doc_tag` is the structured split: (repo, sym,
/// tag, arg, text) where tag is `param`/`returns`/`deprecated`/`section`/... .
/// Both are populated in `refresh_type_rels` from the one parse that already
/// builds `type_entity`, by the per-language AST locators in `typegraph`.
const DOC_TEXT_RELS: [&str; 2] = ["doc_comment", "doc_tag"];

/// Compiler-backed SCIP importer. `scip_edge` is file-to-file dependency data
/// extracted from definition/reference occurrences in an existing index.scip.
/// `scip_fn_edge` is the function-level analogue (caller_fn → callee moniker),
/// resolved by RA and attributed via the reference's enclosing fn-def range.
/// `scip_callee_type` maps each method moniker to its receiver type (parsed
/// from the `impl#[Type]` segment), so dl can read both a fn's own type and its
/// callees' types from one relation — the basis for feature-envy detection.
const SCIP_RELS: [&str; 8] = ["scip_def", "scip_name", "scip_ref", "scip_edge", "scip_fn_edge", "scip_callee_type", "scip_local", "scip_impl"];

/// Worktree diff relation. `changed(path)` holds every path `git status` says
/// differs from HEAD in the self repo: modified, added, renamed (new side),
/// untracked. Lazily refreshed like the other built-in indexers; the rails use
/// case is `diag(...) <- some_hit(p, ...), changed(p).` so a check scoped to
/// what an edit session touched never fires on pre-existing repo debt.
const CHANGED_RELS: [&str; 1] = ["changed"];

/// Agent-harness relations (agent.rs). Reserved like CHANGED_RELS, declared each
/// tick, populated by `refresh_agent_rels` only when the program references one.
/// `agent_edit(harness, session, idx, path)` = every file edit in the newest
/// session per detected harness (Claude Code JSONL, opencode SQLite), paths
/// repo-relative. `agent_touch(harness, session, path)` = the latest turn's
/// edits (rows at `max(idx)` per session). Both stay keyed by (harness,
/// session): the cross-harness set is a TAGGED union, not a broadcast — a
/// consumer filters to one session, or projects the labels away to treat any
/// agent uniformly: `diag(p,...) <- changed(p), agent_touch(_, _, p).` ACP is
/// the live tier (deferred).
const AGENT_RELS: [&str; 2] = ["agent_edit", "agent_touch"];

/// Line-level worktree diff: `(path, line)` for every new-side line of every
/// hunk in `git diff -U0 HEAD`, plus every line of untracked files (which the
/// diff omits). Lets a rail scope to the touched lines, not the touched path:
/// `diag(p, l, ...) <- hit(p, l), changed_line(p, l).` instead of `changed(p)`,
/// so a touch on engine.rs surfaces only the `.conn()` calls on edited lines.
const CHANGED_LINE_RELS: [&str; 1] = ["changed_line"];

/// File-authorship relation. `created(path, name, email, ts)` is one row per
/// tracked file: the author of the commit that ADDED it (its creation), from
/// `git log --reverse --diff-filter=A --name-only`. `--reverse` orders
/// oldest-first, so the first time a path appears is its add. Renames are `R`,
/// not `A`, so a file moved with `git mv` keeps its original creation row only
/// if history is followed (it is not here — the new path looks un-created until
/// it is next added). Pairs with `changed` for "who wrote what": the literal
/// "most-similar code in test files by author X" is
/// `propose_clone(_, p, ...), created(p, _, "x@…", _), p ~ "*/tests/*"`.
const CREATED_RELS: [&str; 1] = ["created"];

/// Embedding-similarity relation. `similar(a, b, score)` is the top-k nearest
/// neighbors of each embedded interned string by cosine; `score` is an Int =
/// round(cosine * 1_000_000) in [-1_000_000, 1_000_000], so a `.dl` rule can
/// threshold (`score > 800000`) in the engine's Int-only value space. Vectors
/// are content-addressed: one row per (StringId, backend) in `_embeddings`, so
/// identical content across repos embeds once and a `similar` hit fans out to
/// every place that content lives via `ref`/`_file`. The backend is a
/// compile-time feature (src/embed/), default the deterministic `stub`. Lazy
/// like the other indexers; brute-force O(n^2) cosine over the embedded set
/// (capped by SPREFA_EMBED_MAX, default 4096), the sqlite-vec ANN path is the
/// scale follow-on. SPREFA_SIMILAR_K (default 8) sets neighbors per row.
const EMBED_RELS: [&str; 1] = ["similar"];

/// Extract-function proposals: one row `(path, lo, hi, param)` per free var of
/// each verbatim-duplicated block found in a scanned Rust file. `lo`/`hi` bound
/// the block's first occurrence (1-based lines); the param set is the inferred
/// extract-fn signature (free vars = read in the block, not bound inside it).
/// The proposer is the `propose` module; this relation is its queryable output.
const PROPOSE_EXTRACT_RELS: [&str; 1] = ["propose_extract"];

/// Multi-kernel clone-detection relation: `propose_clone(kernel, path, lo, hi,
/// param)`. Runs all 9 clone-detection kernels (verbatim, ast, tree, cfg, ddg,
/// cgraph, ngram, symbol, call) on every scanned Rust file. The `kernel` column
/// selects which detector produced the row. Symbol and call kernels need
/// index.scip; they emit no rows if the index is absent.
const PROPOSE_CLONE_RELS: [&str; 1] = ["propose_clone"];

/// Field-tree Merkle hash per type name: `type_shape(name, hash)`. Two types
/// with the same hash are field-tree-isomorphic — same arity, depth, and leaf
/// shape, regardless of names. Built from `type_edge` (field/variant only);
/// impl/generic excluded as cross-cutting. Fixpoint-iterated so recursive
/// types (`struct List { tail: Box<List> }`) converge. Lazy: populates only
/// when the program references `type_shape`, but forces `type_edge` to refresh
/// first since it reads the edge set from the DB.
const TYPE_SHAPE_RELS: [&str; 1] = ["type_shape"];

/// Anti-unification (Plotkin LGG) per type pair: `type_lgg(a, b, vars)`. For
/// every canonical pair (a < b) of distinct type names, `vars` is the count of
/// fresh variables the LGG introduces — 0 for identical (filtered), 1 for a
/// single-leaf difference, higher for more divergence. Two types with low vars
/// are "near shape-iso": they generalize to a shared structure with few holes,
/// candidates for a missing generic or shared abstraction. Lazy on type_edge.
const TYPE_LGG_RELS: [&str; 1] = ["type_lgg"];

/// Ref-spine query relations: thin views over the `_strings` / `_where_bytes`
/// meta tables. `string(id, text, norm)` resolves an interned StringId to its
/// content; `ref(id, string, file, lo, hi)` locates each interned string's byte
/// span, `id` being the `_where_bytes` id (the rewrite coordinate an `edit` keys
/// off). Join them to ask "where does <text> occur": `string(s, "Foo", _),
/// ref(_, s, f, lo, hi)`. Populated for regex/ast/sg captures and import refs.
const SPINE_RELS: [&str; 2] = ["string", "ref"];

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

fn builtin_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "true".into(), cols: vec![] },
        RelDecl { name: "repo".into(), cols: vec![c("slug", Type::Text), c("root", Type::Path), c("url", Type::Text)] },
        RelDecl { name: "rev".into(), cols: vec![c("id", Type::Text), c("repo", Type::Text), c("oid", Type::Text), c("ts", Type::Int)] },
        RelDecl { name: "content".into(), cols: vec![c("id", Type::Text), c("hash", Type::Text)] },
        RelDecl { name: "file".into(), cols: vec![c("repo", Type::Text), c("rev", Type::Text), c("path", Type::Path), c("content", Type::Text)] },
    ]
}

/// Every engine-emitted (built-in) relation declaration, in documentation order.
/// One list so `builtin_rel_names`, the self-describing `rel_catalog`, and the
/// doc-completeness check all read the SAME source. Keep in sync with
/// `declare_builtins` (which declares the same set).
pub fn all_builtin_decls() -> Vec<RelDecl> {
    builtin_rel_decls()
        .into_iter()
        .chain(module_rel_decls())
        .chain(type_rel_decls())
        .chain(doc_text_rel_decls())
        .chain(call_rel_decls())
        .chain(dataflow_rel_decls())
        .chain(doc_rel_decls())
        .chain(scip_rel_decls())
        .chain(spine_rel_decls())
        .chain(node_rel_decls())
        .chain(changed_rel_decls())
        .chain(agent_rel_decls())
        .chain(changed_line_rel_decls())
        .chain(created_rel_decls())
        .chain(embed_rel_decls())
        .chain(propose_extract_rel_decls())
        .chain(propose_clone_rel_decls())
        .chain(type_shape_rel_decls())
        .chain(type_lgg_rel_decls())
        .chain(daemon_rel_decls())
        .chain(catalog_rel_decls())
        .collect()
}

/// Names of every engine-emitted (built-in) relation. The daemon's `schema` RPC
/// flags relations against this so the count and the per-rel "emitted by the
/// engine" label agree (a user `.dl` rule's relation is not in this set).
pub fn builtin_rel_names() -> std::collections::HashSet<String> {
    all_builtin_decls().into_iter().map(|d| d.name).collect()
}

/// One-line documentation for every built-in relation, as `(name, group,
/// summary)`. THIS is the single source of relation docs: `rel_catalog` projects
/// it (joined to the decls for columns) and `examples/builtin-rels.dl` renders it
/// into the README, so a generated doc table can never drift from the engine. A
/// new built-in is forced to appear here by `undocumented_builtins` (a test fails
/// until it does), so the doc standard is mechanical, not a review checklist.
/// Summaries avoid `|` so they render inside a markdown table cell.
pub fn builtin_rel_docs() -> &'static [(&'static str, &'static str, &'static str)] {
    &[
        // core: the repo/rev/content/file spine + the always-true atom.
        ("true", "core", "zero-arity singleton; the always-succeeds atom"),
        ("repo", "core", "configured + dynamically-pulled repos whose root exists; writable as a sink — a repo(...) rule clones+registers when the github org is in `org` (hard filter); see docs/dynamic-reaching.md"),
        ("rev", "core", "git revs seen by scans"),
        ("content", "core", "content addresses"),
        ("file", "core", "scanned files, keyed by (repo, rev, path, content)"),
        // module graph: imports + resolved file-to-file edges.
        ("module_import", "module", "import statements (Rust + TS + Kotlin); Kotlin adds kind=same-package rows for bare uses of another file's column-0 decl, and an expect/actual decl fans edges to all declaring files"),
        ("module_edge", "module", "resolved file-to-file import graph (rev-deduped union)"),
        ("module_edge_rev", "module", "rev-aware module_edge"),
        ("module_unresolved", "module", "broken imports: a reference that resolved to no project file (the linter question)"),
        ("module_unresolved_rev", "module", "rev-aware module_unresolved"),
        ("crate_edge", "module", "workspace-internal Cargo dependency edges"),
        // type graph: entities, edges, signatures.
        ("type_edge", "type", "type-graph edges across Rust (syn), Kotlin (tree-sitter), TS (oxc); kind is field/variant/impl/generic — Kotlin interface supertypes are generic, class/object impl, val/var ctor params + body properties field, enum entries variant"),
        ("type_edge_rev", "type", "rev-aware type_edge (WORK-vs-HEAD type diff)"),
        ("type_entity", "type", "every declared type; sym is file::kind::name, the cross-graph join key; scip_ref overrides name resolution when a SCIP index is present"),
        ("type_sig", "type", "type signature slots (params, fields) per sym"),
        ("type_link", "type", "cross-type links not carried by type_edge (SCIP-resolved sym to sym)"),
        // doc comments attached to entities (Tier 1/2 doc gen).
        ("doc_comment", "type", "doc comment per type_entity sym: (repo, sym, line, text); AST-located per language (Rust #[doc] attrs, Kotlin KDoc sibling, TS leading /** */)"),
        ("doc_tag", "type", "structured doc tags per sym: (repo, sym, tag, arg, text); @param/@returns/@deprecated for JSDoc/KDoc, # Section headings for rustdoc"),
        // call graph: defs, sites, resolved edges, classification.
        ("call_def", "call", "every callable; sym is file::kind::name"),
        ("call_site", "call", "each call occurrence; caller is the resolved fn sym, callee the bare text; changed_line joins here for line-scoped rails"),
        ("call_edge", "call", "resolved caller-sym to callee-sym edge (single-def or SCIP override)"),
        ("call_edge_rev", "call", "rev-aware call_edge"),
        ("call_name", "call", "def sym to bare callable name; resolves a call_site callee to candidate def syms"),
        ("call_kind", "call", "per-fn read/write classification from the bare callee name (execute* -> write, query*/prepare -> read); rusqlite-shaped, collection names dropped to avoid false positives"),
        // intra-procedural dataflow + loop/alloc material for Big-O.
        ("df_node", "dataflow", "intra-procedural dataflow node (call_res/assign/...); id is file::line::kind"),
        ("df_edge", "dataflow", "intra-procedural dataflow dependency edge"),
        ("loop_over", "dataflow", "one row per loop with its span, iter var, and collection"),
        ("allocates", "dataflow", "one row per fn whose body builds a collection (Vec/HashMap/String ctor, .collect/.clone/.to_string)"),
        ("nest", "dataflow", "one row per (call, enclosing loop); depth is nesting rank (1=outermost); raw material for symbolic Big-O over call_edge"),
        ("df_param", "dataflow", "(param df_node id, positional index); index counts typed params only (self skipped) so it aligns with type_sig.pos for node-level type joins"),
        // doc-to-code bridge.
        ("doc_node", "doc", "structural nodes from non-source text (markdown headings + code blocks via tree-sitter-md: ATX/setext headings, fenced/indented blocks); parent is the enclosing heading"),
        ("doc_ref", "doc", "doc-to-code bridge: name-matches doc_node headings to type_entity symbols (exact + normalized) and scans code blocks for identifier mentions; empty unless the program also uses type relations"),
        // SCIP: compiler-backed facts from an index.scip.
        ("scip_def", "scip", "symbol defs from an existing index.scip (root or $SPREFA_SCIP_INDEX)"),
        ("scip_name", "scip", "descriptor name (last identifier run) of a moniker, computed in-engine"),
        ("scip_ref", "scip", "compiler-backed references (ref file, symbol, def file)"),
        ("scip_edge", "scip", "file-to-file SCIP dependency edges"),
        ("scip_fn_edge", "scip", "function-level call edge; caller is the innermost enclosing fn def"),
        ("scip_callee_type", "scip", "receiver type parsed from a method moniker's impl/for segment"),
        ("scip_local", "scip", "local-variable + parameter declarations attributed to their enclosing fn"),
        ("scip_impl", "scip", "interface/supertype dispatch edge from SCIP is_implementation (impl to iface)"),
        // working-tree change rails.
        ("changed", "changed", "git status --porcelain -uall vs HEAD (modified/added/renamed/untracked); empty outside git; the rails join"),
        ("changed_line", "changed", "new-side lines of git diff -U0 HEAD hunks plus every line of untracked files; pure-deletion hunks emit nothing; line-scoped rails precision"),
        // agent-harness rails (from the at-rest session store).
        ("agent_edit", "agent", "every file edit in the latest agent turn, tagged harness+session+turn idx (from the at-rest harness store)"),
        ("agent_touch", "agent", "the latest agent turn's edited files (harness, session, path)"),
        // misc derived sources.
        ("created", "created", "files added since their first appearance, with author name/email/timestamp"),
        ("similar", "embed", "content-addressed nearest-neighbor pairs from the embedding backend, with score"),
        ("propose_extract", "propose", "proposed extract-function refactor spans (path, lo, hi, param)"),
        ("propose_clone", "propose", "proposed clone/near-duplicate groups keyed by a shared kernel"),
        ("type_shape", "type-shape", "structural type-shape fingerprint per type (shape-iso experiment)"),
        ("type_lgg", "type-shape", "least-general generalization of two type shapes (shape-iso experiment)"),
        // ref spine: interned strings + located byte spans.
        ("string", "spine", "interned strings (ref spine): id, text, normalized text"),
        ("ref", "spine", "byte span per interned string; id is the rewrite coordinate — 'where does Foo occur' is string(s, Foo, _), ref(_, s, f, lo, hi)"),
        // CST nested-set nodes for ancestry/containment.
        ("node", "node", "CST nodes (nested-set spans): id, kind, file, lo, hi, parent"),
        ("child", "node", "CST parent-child edges (exactly 2 cols, so closure(child) gives ancestry)"),
        // daemon bookkeeping.
        ("program", "daemon", "dl programs the daemon tracks (path, content hash, mtime)"),
        ("head", "daemon", "git HEAD per repo (repo, ref name, oid)"),
        ("rev_advanced", "daemon", "daemon signal that a repo ref advanced (repo, name, old oid, new oid)"),
        // self: the catalog documents itself.
        ("rel_catalog", "meta", "this table: every built-in relation with its group, columns, and one-line doc"),
    ]
}

/// Built-in relation names that have no entry in `builtin_rel_docs`. The
/// doc-completeness invariant: this must be empty. A test asserts it, so adding a
/// built-in without documenting it fails CI rather than silently shipping a blank
/// row in the generated table.
pub fn undocumented_builtins() -> Vec<String> {
    let documented: std::collections::HashSet<&str> =
        builtin_rel_docs().iter().map(|(n, _, _)| *n).collect();
    let mut missing: Vec<String> = builtin_rel_names()
        .into_iter()
        .filter(|n| !documented.contains(n.as_str()))
        .collect();
    missing.sort();
    missing
}

/// The self-describing catalog relation: one row per built-in relation. Lazy like
/// every other built-in group, so a program that never reads it pays nothing.
const CATALOG_RELS: [&str; 1] = ["rel_catalog"];

fn catalog_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "rel_catalog".into(), cols: vec![
            c("name", Type::Text), c("group", Type::Text),
            c("cols", Type::Text), c("doc", Type::Text)] },
    ]
}

fn catalog_rels_used(prog: &Program) -> bool { rels_used(prog, &CATALOG_RELS) }

fn module_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "module_import".into(), cols: vec![
            c("file", Type::Path), c("rev", Type::Text), c("specifier", Type::Text), c("kind", Type::Text), c("line", Type::Int)] },
        RelDecl { name: "module_edge".into(), cols: vec![c("src", Type::Path), c("dst", Type::Path)] },
        RelDecl { name: "module_edge_rev".into(), cols: vec![c("src", Type::Path), c("dst", Type::Path), c("rev", Type::Text)] },
        RelDecl { name: "module_unresolved".into(), cols: vec![
            c("file", Type::Path), c("specifier", Type::Text), c("reason", Type::Text), c("line", Type::Int)] },
        RelDecl { name: "module_unresolved_rev".into(), cols: vec![
            c("file", Type::Path), c("rev", Type::Text), c("specifier", Type::Text), c("reason", Type::Text), c("line", Type::Int)] },
        RelDecl { name: "crate_edge".into(), cols: vec![c("src", Type::Text), c("dst", Type::Text), c("kind", Type::Text), c("rev", Type::Text)] },
    ]
}

fn type_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "type_edge".into(), cols: vec![c("from", Type::Text), c("to", Type::Text), c("kind", Type::Text)] },
        RelDecl { name: "type_edge_rev".into(), cols: vec![c("from", Type::Text), c("to", Type::Text), c("kind", Type::Text), c("rev", Type::Text)] },
        RelDecl { name: "type_entity".into(), cols: vec![
            c("repo", Type::Text), c("sym", Type::Text), c("name", Type::Text), c("kind", Type::Text),
            c("parent", Type::Text), c("file", Type::Path), c("line", Type::Int)] },
        RelDecl { name: "type_sig".into(), cols: vec![
            c("sym", Type::Text), c("slot", Type::Text), c("pos", Type::Int), c("ref", Type::Text)] },
        RelDecl { name: "type_link".into(), cols: vec![c("src", Type::Text), c("dst", Type::Text), c("kind", Type::Text)] },
    ]
}

fn doc_text_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "doc_comment".into(), cols: vec![
            c("repo", Type::Text), c("sym", Type::Text), c("line", Type::Int), c("text", Type::Text)] },
        RelDecl { name: "doc_tag".into(), cols: vec![
            c("repo", Type::Text), c("sym", Type::Text), c("tag", Type::Text),
            c("arg", Type::Text), c("text", Type::Text)] },
    ]
}

fn call_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "call_def".into(), cols: vec![
            c("repo", Type::Text), c("sym", Type::Text), c("kind", Type::Text),
            c("file", Type::Path), c("line", Type::Int), c("end", Type::Int)] },
        RelDecl { name: "call_site".into(), cols: vec![
            c("repo", Type::Text), c("caller", Type::Text), c("callee", Type::Text),
            c("file", Type::Path), c("line", Type::Int)] },
        RelDecl { name: "call_edge".into(), cols: vec![
            c("caller", Type::Text), c("callee", Type::Text), c("kind", Type::Text)] },
        RelDecl { name: "call_edge_rev".into(), cols: vec![
            c("caller", Type::Text), c("callee", Type::Text),
            c("kind", Type::Text), c("rev", Type::Text)] },
        // def sym -> bare callable name, so rules can resolve a call_site's
        // callee text to the set of candidate def syms (then filter, e.g. by
        // allocates). One row per def; a bare name may map to several syms.
        RelDecl { name: "call_name".into(), cols: vec![c("sym", Type::Text), c("name", Type::Text)] },
        // Per-fn read/write classification of the fn's call sites. `fn` is the
        // caller sym (same shape as call_site.caller); `kind` is `read` or
        // `write`, classified from the bare callee name (execute/
        // execute_batch -> write; prepare/query_row/query_map -> read). Lets a
        // rail ask "does this fn contain any write?" via `call_kind(fn, "write")`
        // without re-declaring the method-name table per program.
        RelDecl { name: "call_kind".into(), cols: vec![c("fn", Type::Text), c("kind", Type::Text)] },
    ]
}

fn dataflow_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "df_node".into(), cols: vec![
            c("id", Type::Text), c("kind", Type::Text), c("var", Type::Text),
            c("fn", Type::Text), c("file", Type::Path), c("line", Type::Int)] },
        RelDecl { name: "df_edge".into(), cols: vec![c("from", Type::Text), c("to", Type::Text)] },
        // one row per loop, with its source span + loop variable. The flag rule
        // joins this against df_node/df_edge to find loop-invariant calls: a
        // call whose line falls in [start,end] taking an argument that is a
        // function param (not the loop variable).
        RelDecl { name: "loop_over".into(), cols: vec![
            c("file", Type::Path), c("start", Type::Int), c("end", Type::Int),
            c("var", Type::Text), c("collection", Type::Text), c("fn", Type::Text)] },
        // one row per fn whose body builds a collection (Vec/HashMap/String ctor
        // or .collect/.clone/.to_string). The cost signal that cuts the
        // loop-invariant-call suspect list down to recomputation candidates.
        RelDecl { name: "allocates".into(), cols: vec![c("fn", Type::Text)] },
        // one row per (call, enclosing loop) pair: `call_id` is the call_res
        // node, `loop_id` joins back to loop_over via "{file}:{start}", `depth`
        // is the loop's nesting rank (1 = outermost), `collection` is the inner
        // loop's iterated collection text ("" until extractors fill it). The
        // raw material for symbolic Big-O composed over call_edge.
        RelDecl { name: "nest".into(), cols: vec![
            c("call_id", Type::Text), c("loop_id", Type::Text),
            c("depth", Type::Int), c("collection", Type::Text)] },
        // (param df_node id, positional index) — the index counts only typed
        // params (the Rust receiver `self` is skipped), so it aligns with
        // type_sig's `pos`. Lets a query bind a specific param node to its
        // declared type at node granularity, not just per-fn.
        RelDecl { name: "df_param".into(), cols: vec![c("id", Type::Text), c("pos", Type::Int)] },
    ]
}

fn doc_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        // one row per document structural node (heading / code block / section).
        // `parent` is the enclosing heading text ("" at top level), so a rule can
        // walk the section tree. See `ingest::IngestLang`.
        RelDecl { name: "doc_node".into(), cols: vec![
            c("repo", Type::Text), c("file", Type::Path), c("line", Type::Int),
            c("kind", Type::Text), c("name", Type::Text), c("parent", Type::Text)] },
        // doc→code bridge: (file, line, sym, kind, matched_name). For each row,
        // `kind` is the doc_node kind that produced it ("heading" or
        // "code_block") and `matched_name` is the doc-side string that matched a
        // type_entity name (the heading text for `heading`, the matching
        // identifier token for `code_block`). Heading names are normalized
        // before the join (articles + trailing kind words stripped) so "The
        // Engine struct" bridges to `Engine`. Empty unless the program also uses
        // type relations.
        RelDecl { name: "doc_ref".into(), cols: vec![
            c("repo", Type::Text), c("file", Type::Path), c("line", Type::Int), c("sym", Type::Text),
            c("kind", Type::Text), c("matched_name", Type::Text)] },
    ]
}

fn scip_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "scip_def".into(), cols: vec![c("symbol", Type::Text), c("file", Type::Path)] },
        RelDecl { name: "scip_name".into(), cols: vec![c("symbol", Type::Text), c("name", Type::Text)] },
        RelDecl { name: "scip_ref".into(), cols: vec![c("file", Type::Path), c("symbol", Type::Text), c("def_file", Type::Path)] },
        RelDecl { name: "scip_edge".into(), cols: vec![c("src", Type::Path), c("dst", Type::Path)] },
        RelDecl { name: "scip_fn_edge".into(), cols: vec![c("caller", Type::Text), c("callee", Type::Text)] },
        RelDecl { name: "scip_callee_type".into(), cols: vec![c("sym", Type::Text), c("type", Type::Text)] },
        RelDecl { name: "scip_local".into(), cols: vec![c("fn", Type::Text), c("name", Type::Text)] },
        RelDecl { name: "scip_impl".into(), cols: vec![c("impl", Type::Text), c("iface", Type::Text)] },
    ]
}

fn changed_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "changed".into(), cols: vec![c("path", Type::Path)] },
    ]
}

fn agent_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "agent_edit".into(), cols: vec![
            c("harness", Type::Text), c("session", Type::Text), c("idx", Type::Int), c("path", Type::Path)] },
        RelDecl { name: "agent_touch".into(), cols: vec![
            c("harness", Type::Text), c("session", Type::Text), c("path", Type::Path)] },
    ]
}

fn changed_line_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "changed_line".into(),
                  cols: vec![c("path", Type::Path), c("line", Type::Int)] },
    ]
}

fn created_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "created".into(),
                  cols: vec![c("path", Type::Path), c("name", Type::Text),
                             c("email", Type::Text), c("ts", Type::Int)] },
    ]
}

fn embed_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "similar".into(),
                  cols: vec![c("a", Type::Text), c("b", Type::Text), c("score", Type::Int)] },
    ]
}

fn propose_extract_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "propose_extract".into(),
                  cols: vec![c("path", Type::Path), c("lo", Type::Int),
                             c("hi", Type::Int), c("param", Type::Text)] },
    ]
}

fn propose_clone_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "propose_clone".into(),
                  cols: vec![c("kernel", Type::Text), c("path", Type::Path),
                             c("lo", Type::Int), c("hi", Type::Int),
                             c("param", Type::Text)] },
    ]
}

fn type_shape_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "type_shape".into(),
                  cols: vec![c("name", Type::Text), c("hash", Type::Text)] },
    ]
}

fn type_lgg_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "type_lgg".into(),
                  cols: vec![c("a", Type::Text), c("b", Type::Text), c("vars", Type::Int)] },
    ]
}

fn spine_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "string".into(), cols: vec![c("id", Type::Text), c("text", Type::Text), c("norm", Type::Text)] },
        RelDecl { name: "ref".into(), cols: vec![
            c("id", Type::Text), c("string", Type::Text), c("file", Type::Text), c("lo", Type::Int), c("hi", Type::Int)] },
    ]
}

fn node_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "node".into(), cols: vec![
            c("id", Type::Text), c("kind", Type::Text), c("file", Type::Text),
            c("lo", Type::Int), c("hi", Type::Int), c("parent", Type::Text)] },
        // EXACTLY 2 cols: `declare_closure` requires it, so `anc(a,b) <-
        // closure(child).` works with zero new recursion code.
        RelDecl { name: "child".into(), cols: vec![c("parent", Type::Text), c("child", Type::Text)] },
    ]
}

fn daemon_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "program".into(), cols: vec![c("path", Type::Path), c("hash", Type::Text), c("mtime", Type::Int)] },
        RelDecl { name: "head".into(), cols: vec![c("repo", Type::Text), c("name", Type::Text), c("oid", Type::Text)] },
        RelDecl { name: "rev_advanced".into(), cols: vec![
            c("repo", Type::Text), c("name", Type::Text), c("old", Type::Text), c("new", Type::Text)] },
    ]
}

/// Does the program reference any relation in `rels` (body atom, closure edge,
/// or query head)? Gates lazy built-in indexers so unrelated programs pay nothing.
fn rels_used(prog: &Program, rels: &[&str]) -> bool {
    let hit = |r: &str| rels.contains(&r);
    for item in &prog.items {
        match item {
            Item::Rule(r) => for b in &r.body {
                match b {
                    BodyItem::Pos(a) | BodyItem::Neg(a) => if hit(&a.rel) { return true; },
                    BodyItem::Closure { rel } => if hit(rel) { return true; },
                    BodyItem::Scc { rel } => if hit(rel) { return true; },
                    BodyItem::Node2vec { rel } => if hit(rel) { return true; },
                    _ => {}
                }
            },
            Item::Query(q) => if hit(&q.head.rel) { return true; },
            Item::Gen(g) => for b in &g.body {
                match b {
                    BodyItem::Pos(a) | BodyItem::Neg(a) => if hit(&a.rel) { return true; },
                    BodyItem::Closure { rel } => if hit(rel) { return true; },
                    BodyItem::Scc { rel } => if hit(rel) { return true; },
                    BodyItem::Node2vec { rel } => if hit(rel) { return true; },
                    _ => {}
                }
            },
            Item::Rel(_) | Item::Anchor(_) | Item::Brand(_) => {}
        }
    }
    false
}

/// Classify a call site's bare callee name as `read` or `write`. Heuristic by
/// method name only (no receiver type); rail-side joins (`conn_fn` for the
/// .conn() rail) narrow to db-shaped sites, so a `HashMap::insert` false
/// positive in an unrelated fn never reaches a diag because that fn has no
/// `.conn()` site. The table is deliberately rusqlite-shaped: `insert`/
/// `update`/`delete`/`replace`/`commit` are dropped because they collide with
/// collection methods (`HashMap::insert`) and would pollute the table for
/// little gain — the rail's `conn_fn` join already gates on `.conn()`, so the
/// only writes that matter are the ones chained off a `.conn()` or `Db::` call,
/// which in rusqlite are `execute`/`execute_batch`/`execute_returning`.
/// `None` for anything not clearly a db read or write.
fn classify_call_kind(callee: &str) -> Option<&'static str> {
    Some(match callee {
        "execute" | "execute_batch" | "execute_returning" => "write",
        "prepare" | "prepare_cached"
        | "query_row" | "query_map" | "query_and_then" | "query_named" => "read",
        _ => return None,
    })
}

fn module_rels_used(prog: &Program) -> bool { rels_used(prog, &MODULE_RELS) }

fn type_rels_used(prog: &Program) -> bool { rels_used(prog, &TYPE_RELS) }

fn doc_text_rels_used(prog: &Program) -> bool { rels_used(prog, &DOC_TEXT_RELS) }

fn call_rels_used(prog: &Program) -> bool { rels_used(prog, &CALL_RELS) }

fn dataflow_rels_used(prog: &Program) -> bool { rels_used(prog, &DATAFLOW_RELS) }

fn doc_rels_used(prog: &Program) -> bool { rels_used(prog, &DOC_RELS) }

fn daemon_rels_used(prog: &Program) -> bool { rels_used(prog, &DAEMON_RELS) }

/// The trailing identifier of a SCIP symbol descriptor: `... Foo#` -> "Foo",
/// `... bar().` -> "bar". Used to key the SCIP override by plain type name.
fn scip_descriptor_name(symbol: &str) -> Option<String> {
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

fn scip_rels_used(prog: &Program) -> bool { rels_used(prog, &SCIP_RELS) }

fn spine_rels_used(prog: &Program) -> bool { rels_used(prog, &SPINE_RELS) }

fn node_rels_used(prog: &Program) -> bool { rels_used(prog, &NODE_RELS) }

fn changed_rels_used(prog: &Program) -> bool { rels_used(prog, &CHANGED_RELS) }
fn agent_rels_used(prog: &Program) -> bool { rels_used(prog, &AGENT_RELS) }

fn changed_line_rels_used(prog: &Program) -> bool { rels_used(prog, &CHANGED_LINE_RELS) }
fn created_rels_used(prog: &Program) -> bool { rels_used(prog, &CREATED_RELS) }
fn embed_rels_used(prog: &Program) -> bool { rels_used(prog, &EMBED_RELS) }

fn propose_extract_rels_used(prog: &Program) -> bool { rels_used(prog, &PROPOSE_EXTRACT_RELS) }

fn propose_clone_rels_used(prog: &Program) -> bool { rels_used(prog, &PROPOSE_CLONE_RELS) }

fn type_shape_rels_used(prog: &Program) -> bool { rels_used(prog, &TYPE_SHAPE_RELS) }

fn type_lgg_rels_used(prog: &Program) -> bool { rels_used(prog, &TYPE_LGG_RELS) }

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
/// (`any_closure_empty`/`rebuild_closures`) stays closure-only (the scc_node
/// SQL tables exist only for closure edges — scc reads the in-memory cond).
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

type Bind = HashMap<String, Value>;
/// (repo slug, path, rev) -> (content hash, mtime secs, size bytes). The repo
/// slug is the third coordinate so two repos sharing a path do not collide.
type FileMeta = HashMap<(String, String, String), (String, i64, i64)>;

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
}

impl ModuleRows {
    fn extend(&mut self, other: ModuleRows) {
        self.imports.extend(other.imports);
        self.edges_rev.extend(other.edges_rev);
        self.unresolved_rev.extend(other.unresolved_rev);
        self.crate_edges.extend(other.crate_edges);
        self.spans.extend(other.spans);
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

pub struct Engine {
    pub(crate) db: crate::db::Db,
    pub(crate) rels: Rels,
    root: PathBuf,
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
    /// Test/bench instrumentation: cumulative count of edge condensations
    /// actually rebuilt (Tarjan invocations). A reused cond does not bump it, so
    /// a bench can assert "this edit recondensed 0 graphs".
    pub recondensed: usize,
    rev_cache: HashMap<String, String>,
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
    /// Verify-rollback journal (christmas #14). `None` = not in verify mode (gen
    /// writes go straight to disk, no capture). `Some(...)` = every gen write
    /// first stashes the target's original bytes (`None` entry = the file did not
    /// exist) so `rollback_writes` can restore the tree if a checker fails. One
    /// entry per path, first-write wins (the pre-tick state).
    gen_journal: std::cell::RefCell<Option<Vec<(String, Option<Vec<u8>>)>>>,
}

struct ScanSpec {
    repo: Term,
    rev: Term,
    glob: Term,
    path_var: String,
    rev_out_var: String,
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
        Engine {
            db, rels: HashMap::new(), root, dropped: 0, extraction_drops: Vec::new(), recondensed: 0,
            closure_cache: HashMap::new(),
            rev_cache: HashMap::new(),
            rev_index: std::collections::HashSet::new(),
            repos: Vec::new(),
            query_json: false,
            prime_tick: false,
            root_implicit: false,
            last_n1: None,
            last_node_files_walked: std::cell::Cell::new(0),
            gen_journal: std::cell::RefCell::new(None),
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
        let key = format!("{}::{rev}", repo_root.display());
        if let Some(s) = self.rev_cache.get(&key) { return Ok(s.clone()); }
        let out = Command::new("git").arg("-C").arg(repo_root)
            .args(["rev-parse", rev]).output()?;
        if !out.status.success() { bail!("git rev-parse {rev} failed in {}", repo_root.display()); }
        let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
        self.rev_cache.insert(key, sha.clone());
        Ok(sha)
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

    /// Stable slug for this engine's own repo: the `--root` directory name.
    fn self_slug(&self) -> String {
        self.root.file_name().map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| self.root.to_string_lossy().to_string())
    }

    /// slug -> on-disk root for every nameable repo (self + config). The lazy
    /// indexers (type / call / doc) read each file's content from its OWN repo
    /// root via this map, not the single `self.root`, so `type_entity`, the call
    /// graph, and `doc_node` populate for every folder in view — not just
    /// `--root`. `_file.repo` is the key; an unknown repo falls back to root.
    fn repo_roots(&self) -> HashMap<String, PathBuf> {
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
    pub fn located_spans(&self) -> Result<Vec<(String, u32, u32, String)>> {
        let conn = self.db.conn();
        let mut s = conn.prepare(
            "SELECT w.path, w.lo, w.hi, s.content FROM _where_bytes w \
             JOIN _strings s ON s.id = w.string_id \
             WHERE w.id != '0' AND w.path != ''")?;
        let rows = s.query_map([], |r| Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)? as u32,
            r.get::<_, i64>(2)? as u32,
            r.get::<_, String>(3)?,
        )))?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    }

    /// The WORK-content `FileId` for `path`, derived from the `_file` cache's
    /// stored blake3 the same way extraction derives it. Span queries filter
    /// `_where_bytes` by this id: a git-rev span shares the path attribution but
    /// its offsets index the old blob, so only rows whose file id matches the
    /// current WORK content are positionally valid for an editor cursor.
    fn work_file_id(&self, path: &str) -> Result<Option<spine::FileId>> {
        let conn = self.db.conn();
        let mut s = conn.prepare(
            "SELECT hash, size FROM _file WHERE path = ?1 AND rev = 'WORK' LIMIT 1")?;
        let row = s.query_row(rusqlite::params![path], |r| Ok((
            r.get::<_, String>(0)?, r.get::<_, i64>(1)?,
        )));
        match row {
            Ok((hash, size)) => Ok(spine::FileId::from_content_address(&hash, size)
                .filter(|f| *f != spine::FileId::SYNTHETIC)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// The innermost located WORK span containing `byte` in `path`, with its
    /// interned string: (string_id, text, lo, hi). Innermost so a cursor inside
    /// a brace-leaf import picks the leaf over the shared head span. None when
    /// the cursor is not on any located string. Drives the LSP
    /// definition/references handlers.
    pub fn span_at(&self, path: &str, byte: u32) -> Result<Option<(String, String, u32, u32)>> {
        let Some(fid) = self.work_file_id(path)? else { return Ok(None); };
        let conn = self.db.conn();
        let mut s = conn.prepare(
            "SELECT w.string_id, s.content, w.lo, w.hi FROM _where_bytes w \
             JOIN _strings s ON s.id = w.string_id \
             WHERE w.id != '0' AND w.path = ?1 AND w.file_id = ?2 \
               AND w.lo <= ?3 AND ?3 < w.hi \
             ORDER BY (w.hi - w.lo) ASC LIMIT 1")?;
        let row = s.query_row(rusqlite::params![path, fid.to_string(), byte as i64], |r| Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)? as u32,
            r.get::<_, i64>(3)? as u32,
        )));
        match row {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Every located WORK span of `string_id`, as (path, lo, hi). The
    /// file-id-matches-WORK-content filter runs in Rust per result path because
    /// the FileId derivation (blake3 prefix) is not expressible in SQL.
    pub fn string_spans(&self, string_id: &str) -> Result<Vec<(String, u32, u32)>> {
        let candidates: Vec<(String, String, u32, u32)> = {
            let conn = self.db.conn();
            let mut s = conn.prepare(
                "SELECT path, file_id, lo, hi FROM _where_bytes \
                 WHERE string_id = ?1 AND id != '0' AND path != '' \
                 ORDER BY path, lo")?;
            let rows = s.query_map(rusqlite::params![string_id], |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)? as u32,
                r.get::<_, i64>(3)? as u32,
            )))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        let mut work_ids: HashMap<String, Option<String>> = HashMap::new();
        let mut out = Vec::new();
        for (path, fid, lo, hi) in candidates {
            let want = match work_ids.get(&path) {
                Some(w) => w.clone(),
                None => {
                    let w = self.work_file_id(&path)?.map(|f| f.to_string());
                    work_ids.insert(path.clone(), w.clone());
                    w
                }
            };
            if want.as_deref() == Some(fid.as_str()) { out.push((path, lo, hi)); }
        }
        Ok(out)
    }

    /// Definition targets for the located string `text` under a cursor in
    /// `file`. Two paths, tried in order:
    ///
    /// 1. **Phase E (rule-driven):** if the program declares a `def_target`
    ///    relation, query it by the cursor's text. The program writes rules
    ///    like `def_target(name, f, l, "fn") <- call_def(sym, _, f, l, _),
    ///    call_name(sym, name).` so go-to-def lands on real symbol definitions,
    ///    not just module edges. Returns `(file, line)` pairs.
    ///
    /// 2. **Fallback (module-edge):** the `module_edge(file, dst)` rows where a
    ///    segment of `text` names dst's module stem. Lands at line 0 (a module
    ///    edge is file-level; the spine carries no in-target symbol position).
    ///
    /// Empty when the span is not an import ref, no `def_target` match, and no
    /// module edge resolves. Drives the LSP `textDocument/definition` handler.
    pub fn definition_targets(&self, file: &str, text: &str) -> Result<Vec<(String, i64)>> {
        // Phase E: rule-driven def_target. Same pattern as `diags`: read by
        // column name so the program's column ordering/naming is flexible as
        // long as `name`, `file`, `line` are present.
        if let Some(meta) = self.rels.get("def_target") {
            let idx: HashMap<&str, usize> =
                meta.cols.iter().enumerate().map(|(i, c)| (c.name.as_str(), i)).collect();
            if let (Some(ni), Some(fi), Some(li)) = (idx.get("name"), idx.get("file"), idx.get("line")) {
                let cols: Vec<String> = meta.cols.iter().map(|c| format!("\"{}\"", c.name)).collect();
                let sql = format!(
                    "SELECT DISTINCT \"{}\", \"{}\" FROM {} WHERE \"{}\" = ?1",
                    meta.cols[*fi].name, meta.cols[*li].name, tbl("def_target"), meta.cols[*ni].name);
                let conn = self.db.conn();
                let mut s = conn.prepare(&sql)?;
                let rows = s.query_map(rusqlite::params![text], |r|
                    Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
                let out: Vec<(String, i64)> = rows.filter_map(|x| x.ok()).collect();
                let _ = cols; // (kept for symmetry with diags; unused today)
                if !out.is_empty() { return Ok(out); }
            }
        }

        // Fallback: module_edge by specifier-segment match. Line 0 (file-level).
        let dsts: Vec<String> = {
            let conn = self.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT DISTINCT \"dst\" FROM {} WHERE \"src\" = ?1", tbl("module_edge")))?;
            let rows = s.query_map(rusqlite::params![file], |r| r.get::<_, String>(0))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        let segs: HashSet<&str> = text
            .split(|c: char| c == ':' || c == '/')
            .filter(|s| !s.is_empty() && !matches!(*s, "crate" | "self" | "super" | "." | ".."))
            .collect();
        let matched: Vec<String> = dsts.iter()
            .filter(|d| segs.contains(module_stem(d)))
            .cloned().collect();
        if matched.is_empty() && dsts.len() == 1 { return Ok(dsts.into_iter().map(|f| (f, 0)).collect()); }
        Ok(matched.into_iter().map(|f| (f, 0)).collect())
    }

    /// Hover info for the located string `text` under a cursor. Auto-synthesizes
    /// a markdown summary by joining the type graph (`type_entity` by name) and
    /// the call graph (`call_name` -> `call_def`). No new builtin rel: the
    /// program opts in by referencing `type_entity` / `call_def` (the lazy
    /// indexers populate those tables when referenced). Returns None when no
    /// entity or callable shares the bare name.
    ///
    /// Drives the LSP `textDocument/hover` handler. One markdown block per
    /// match (a name may resolve to several entities/callables across modules).
    pub fn hover(&self, _file: &str, text: &str) -> Result<Option<String>> {
        // (kind, sym, file, line); deduped by sym so a callable that also has a
        // type_entity row (same sym shape) appears once.
        let mut entries: Vec<(String, String, String, i64)> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        // type_entity(_, name=text, kind, parent, file, line) -> one row per
        // entity whose bare name matches.
        let te = tbl("type_entity");
        let te_rows: Vec<(String, String, String, i64)> = {
            let conn = self.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"sym\", \"kind\", \"file\", \"line\" FROM {te} WHERE \"name\" = ?1"))?;
            let rows = s.query_map(rusqlite::params![text], |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?,
                r.get::<_, String>(2)?, r.get::<_, i64>(3)?)))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        for (sym, kind, file, line) in te_rows {
            if seen.insert(sym.clone()) {
                entries.push((kind, sym, file, line));
            }
        }

        // call_name(sym, text) -> call_def(sym, kind, file, line, _). The
        // intermediate join resolves the bare callee text to def syms; a bare
        // name may map to several defs (overloads, distinct modules).
        let cd = tbl("call_def");
        let cn = tbl("call_name");
        let cd_rows: Vec<(String, String, String, i64)> = {
            let conn = self.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT d.\"sym\", d.\"kind\", d.\"file\", d.\"line\" \
                 FROM {cd} d JOIN {cn} n ON n.\"sym\" = d.\"sym\" \
                 WHERE n.\"name\" = ?1"))?;
            let rows = s.query_map(rusqlite::params![text], |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?,
                r.get::<_, String>(2)?, r.get::<_, i64>(3)?)))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        for (sym, kind, file, line) in cd_rows {
            if seen.insert(sym.clone()) {
                entries.push((kind, sym, file, line));
            }
        }

        if entries.is_empty() { return Ok(None); }
        entries.sort();
        let mut md = String::new();
        for (i, (kind, sym, file, line)) in entries.into_iter().enumerate() {
            if i > 0 { md.push_str("\n\n---\n\n"); }
            md.push_str(&format!("**{kind}** `{sym}`  \n{file}:{line}"));
            // Type-profile overlay for data types: Ca, Ce, fields, variants.
            // Callable kinds (function/method) don't have field/variant edges,
            // so the overlay is data-type-only. Cheap (4 COUNT queries on an
            // indexed column); no-op when type_edge isn't populated.
            if matches!(kind.as_str(), "struct" | "enum" | "trait" | "class" | "interface" | "alias") {
                if let Some(profile) = self.type_profile_overlay(&sym) {
                    md.push_str(&format!("  \n{profile}"));
                }
            }
        }
        Ok(Some(md))
    }

    /// One-line type profile for `sym`: `Ca=N Ce=M fields=F variants=V impls=I`.
    /// `sym` is the type_entity sym; type_edge is name-keyed, so the trailing
    /// identifier of the sym (the bare name) is the join key. Returns None if
    /// type_edge is empty (program didn't opt into type rels) or all counts
    /// are zero (the type has no structural edges).
    fn type_profile_overlay(&self, sym: &str) -> Option<String> {
        let name = sym.rsplit("::").next()?;
        let edge = tbl("type_edge");
        let conn = self.db.conn();
        let q = |sql: String| -> Option<i64> {
            conn.query_row(&sql, rusqlite::params![name], |r| r.get::<_, i64>(0)).ok()
        };
        let ca = q(format!(
            "SELECT COUNT(DISTINCT \"from\") FROM {edge} WHERE \"to\" = ?1"))?;
        let ce = q(format!(
            "SELECT COUNT(DISTINCT \"to\") FROM {edge} WHERE \"from\" = ?1"))?;
        let fields = q(format!(
            "SELECT COUNT(DISTINCT \"to\") FROM {edge} WHERE \"from\" = ?1 AND \"kind\" = 'field'"))?;
        let variants = q(format!(
            "SELECT COUNT(DISTINCT \"to\") FROM {edge} WHERE \"from\" = ?1 AND \"kind\" = 'variant'"))?;
        let impls = q(format!(
            "SELECT COUNT(DISTINCT \"to\") FROM {edge} WHERE \"from\" = ?1 AND \"kind\" = 'impl'"))?;
        if ca == 0 && ce == 0 && fields == 0 && variants == 0 && impls == 0 {
            return None;
        }
        Some(format!("Ca={ca} Ce={ce} fields={fields} variants={variants} impls={impls}"))
    }

    /// Distinct WORK source paths from the `_file` cache. Feeds crate-root
    /// discovery (`rspath::crate_roots`) for the `--move` rewriter, so a crate
    /// whose root is `rust/kernel/lib.rs` (no `src/`) still yields module paths.
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
        let Some(meta) = self.rels.get("diag") else { return Ok(Vec::new()); };
        // column name -> position in the rel table
        let idx: HashMap<&str, usize> =
            meta.cols.iter().enumerate().map(|(i, c)| (c.name.as_str(), i)).collect();
        let need = |k: &str| idx.get(k).copied();
        let (pi, li, mi) = match (need("path"), need("line"), need("msg")) {
            (Some(p), Some(l), Some(m)) => (p, l, m),
            _ => bail!("diag relation must have columns: path, line, msg"),
        };
        let select: Vec<String> = meta.cols.iter().map(|c| format!("\"{}\"", c.name)).collect();
        let mut sql = format!("SELECT {} FROM {}", select.join(", "), tbl("diag"));
        if only.is_some() { sql.push_str(&format!(" WHERE \"{}\" = ?1", meta.cols[pi].name)); }
        let mut stmt = self.db.conn().prepare(&sql)?;
        let map_row = |row: &rusqlite::Row| -> rusqlite::Result<DiagRow> {
            let text = |i: usize| row.get::<_, rusqlite::types::Value>(i)
                .map(|v| match v {
                    rusqlite::types::Value::Text(s) => s,
                    rusqlite::types::Value::Integer(n) => n.to_string(),
                    _ => String::new(),
                }).unwrap_or_default();
            let int = |i: usize| row.get::<_, i64>(i).unwrap_or(0);
            let line = int(li);
            Ok(DiagRow {
                path: text(pi),
                line,
                col: need("col").map(int).unwrap_or(0),
                end_line: need("end_line").map(int).unwrap_or(line),
                end_col: need("end_col").map(int).unwrap_or(0),
                severity: need("severity").map(text).unwrap_or_else(|| "warn".into()),
                code: need("code").map(text).unwrap_or_default(),
                msg: text(mi),
                hint: need("hint").map(text).filter(|s| !s.is_empty()),
            })
        };
        let mut out = Vec::new();
        let mut rows = match only {
            Some(p) => stmt.query(rusqlite::params![p])?,
            None => stmt.query([])?,
        };
        while let Some(row) = rows.next()? { out.push(map_row(row)?); }
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

    /// One reactive tick: declare, reconcile sources incrementally, rebuild
    /// derived only if a source fact changed, then run queries.
    #[tracing::instrument(skip_all, level = "info")]
    pub fn tick(&mut self, prog: &Program, quiet: bool) -> Result<()> {
        self.rev_cache.clear();
        self.extraction_drops.clear();
        CMD_COUNT.store(0, std::sync::atomic::Ordering::Relaxed);
        self.db.tick_begin();
        let rules: Vec<&Rule> = prog.items.iter().filter_map(|i| match i {
            Item::Rule(r) => Some(r), _ => None,
        }).collect();
        let closures = closure_map(&rules);
        self.declare_all(prog, &closures)?;
        self.ensure_meta()?;

        let source_rules: Vec<&Rule> = rules.iter().copied().filter(|r| r.is_source()).collect();
        // `repo`-sink rules: head rel `repo`. Drained post-fixpoint (clone +
        // register), never inserted by reconcile/rebuild (which would wipe the
        // engine-emitted registered set). Must be derived-style: the drain
        // compiles the body as a SELECT, so a scan/match/... body is rejected.
        let repo_sinks: Vec<&Rule> = rules.iter().copied().filter(|r| r.is_repo_sink()).collect();
        for r in &repo_sinks {
            if r.is_source() {
                bail!("repo-sink rule must be derived-style (no scan/match/ast/...); \
                       its body is compiled as a SELECT over already-derived relations");
            }
        }
        // non-source, non-closure rules. A rule whose body reads a closure head in
        // the seedable shape is split out: it can't go through `lower_rule` (the
        // closure head is a VIEW that isn't populated mid-fixpoint), so it is
        // evaluated by a seeded BFS in the query phase instead. Repo-sinks are
        // excluded: they are drained, not derived.
        let all_derived: Vec<&Rule> = rules.iter().copied()
            .filter(|r| !r.is_source() && !r.is_repo_sink()
                && r.closure_edge().is_none() && r.scc_edge().is_none()
                && r.node2vec_edge().is_none()).collect();
        // `head(..) <- scc(edge).` rules: materialize (rep, member) from the
        // closure condensation in the query phase (after refresh_cond_cache).
        // Excluded from all_derived — the Scc body item can't lower to SQL, and
        // the head is filled from the already-computed Tarjan condensation.
        let scc_rules: Vec<&Rule> = rules.iter().copied()
            .filter(|r| r.scc_edge().is_some()).collect();
        // `head(..) <- node2vec(edge).` rules: same exclusion shape as scc. The
        // edge rel is an ordinary derived rel; after it materializes we read its
        // rows, learn node vectors, and fill the head with KNN pairs.
        let node2vec_rules: Vec<&Rule> = rules.iter().copied()
            .filter(|r| r.node2vec_edge().is_some()).collect();
        check_stratification(&all_derived, &closures)?;
        let (seed_rules, derived_rules) = split_seed_and_derived(&all_derived, &closures)?;

        // source rels are heads of source rules; they get incremental retraction.
        let mut source_rels: Vec<String> = Vec::new();
        for r in &source_rules {
            if !source_rels.contains(&r.head.rel) { source_rels.push(r.head.rel.clone()); }
        }
        let mut derived_rels: Vec<String> = Vec::new();
        for r in &derived_rules {
            if !derived_rels.contains(&r.head.rel) { derived_rels.push(r.head.rel.clone()); }
        }
        // A rel written by BOTH a source rule (scan/match/ast/sg/json/cmd/comment)
        // and a derived rule cannot share one table: `reconcile_sources` fills the
        // source rows incrementally (tracked in `_prov`), then `rebuild_derived`
        // does a full `DELETE FROM rel` + recompute, which silently drops every
        // scanned row. Bail loudly instead — split into two rels and union them in
        // a third derived rule (see examples/anim-self.dl's pin/fpin -> span_of).
        for rel in &source_rels {
            if derived_rels.contains(rel) {
                bail!("relation '{rel}' is written by both a source rule (scan/match/ast/...) \
                       and a derived rule; the scanned rows would be dropped on rebuild. Put \
                       the source rule and the derived rule in two separate relations and union \
                       them in a third derived rule.");
            }
        }
        let edges: Vec<&str> = dedup_edges(&closures);
        self.create_auto_indexes(&derived_rules, &closures)?;

        let t_src = std::time::Instant::now();
        // In profile mode each sub-phase reports its own wall time, so a hung or
        // crawling tick says WHERE it is spending the wait.
        let phase = |label: &'static str, t: std::time::Instant| {
            if crate::db::profiling() {
                eprintln!("[profile] {label}: {:.1}ms", t.elapsed().as_secs_f64() * 1000.0);
            }
        };
        let t = std::time::Instant::now();
        let recon = self.reconcile_sources(&source_rules, &source_rels)?;
        phase("reconcile-sources", t);
        let mut changed = recon.changed;
        // Baseline each source relation's content digest so the next incremental
        // tick can skip a rebuild when bytes move but rows don't (see tick_paths).
        self.seed_rel_digests(&source_rels)?;
        // refresh built-in repo/rev/content/file from the updated _file cache,
        // before derived rules that may join them are rebuilt.
        let t = std::time::Instant::now();
        self.refresh_builtin_rels()?;
        phase("builtin-rels", t);
        if module_rels_used(prog) {
            let t = std::time::Instant::now();
            self.refresh_module_rels()?;
            phase("module-rels", t);
        }
        if type_rels_used(prog) || type_shape_rels_used(prog) || type_lgg_rels_used(prog) || doc_text_rels_used(prog) {
            let t = std::time::Instant::now();
            self.refresh_type_rels()?;
            phase("type-rels", t);
        }
        if call_rels_used(prog) {
            let t = std::time::Instant::now();
            self.refresh_call_rels()?;
            phase("call-rels", t);
        }
        if dataflow_rels_used(prog) {
            let t = std::time::Instant::now();
            self.refresh_dataflow_rels()?;
            phase("dataflow-rels", t);
        }
        if doc_rels_used(prog) {
            let t = std::time::Instant::now();
            self.refresh_doc_rels()?;
            phase("doc-rels", t);
        }
        if scip_rels_used(prog) { changed |= self.refresh_scip_rels()?; }
        // Node rels write into `_strings`/`_where_bytes` (the spine meta tables),
        // so they must run BEFORE the spine projection or this tick's `ref`/
        // `string` would miss the node spans.
        if node_rels_used(prog) {
            let t = std::time::Instant::now();
            changed |= self.refresh_node_rels()?;
            phase("node-rels", t);
        }
        if spine_rels_used(prog) {
            let t = std::time::Instant::now();
            self.refresh_spine_rels()?;
            phase("spine-rels", t);
        }
        // The diff can move without any file content changing (a commit moves
        // HEAD under an identical worktree), so the refresh result feeds
        // `changed` directly rather than riding the reconcile delta.
        if changed_rels_used(prog) { changed |= self.refresh_changed_rel()?; }
        if agent_rels_used(prog) { changed |= self.refresh_agent_rels()?; }
        if changed_line_rels_used(prog) { changed |= self.refresh_changed_line_rel()?; }
        if created_rels_used(prog) { changed |= self.refresh_created_rel()?; }
        if embed_rels_used(prog) { changed |= self.refresh_embed_rels()?; }
        if propose_extract_rels_used(prog) { changed |= self.refresh_propose_extract_rel()?; }
        if propose_clone_rels_used(prog) { changed |= self.refresh_propose_clone_rel()?; }
        if type_shape_rels_used(prog) { changed |= self.refresh_type_shape_rel()?; }
        if type_lgg_rels_used(prog) { changed |= self.refresh_type_lgg_rel()?; }
        if daemon_rels_used(prog) { self.refresh_daemon_rels()?; }
        if catalog_rels_used(prog) { changed |= self.refresh_catalog_rel()?; }
        let src_ms = t_src.elapsed().as_secs_f64() * 1000.0;

        let t_der = std::time::Instant::now();
        let der_digest = derived_program_digest(&derived_rules, &seed_rules, &edges);
        let derived_moved = self.load_rel_digest("derived:program")? != Some(der_digest);
        let rebuilt_all = changed || derived_moved
            || self.any_derived_empty(&derived_rels)? || self.any_closure_empty(&edges)?;
        if rebuilt_all {
            self.rebuild_derived(&derived_rules, &derived_rels)?;
            self.rebuild_closures(&edges)?;
        }
        // persisted only after the rebuild lands, so a failed tick retries
        if derived_moved { self.save_rel_digest("derived:program", &der_digest)?; }
        let der_ms = t_der.elapsed().as_secs_f64() * 1000.0;

        if !quiet {
            eprintln!("[tick] files {}/{} parsed, +{} -{} source facts, derived {} | source {:.1}ms, derived {:.1}ms",
                recon.parsed, recon.total, recon.extracted, recon.retracted,
                if changed { "rebuilt" } else { "unchanged" }, src_ms, der_ms);
        }
        // Full tick: every edge is potentially dirty when we rebuilt; the digest
        // check inside still skips the Tarjan for edges whose rows didn't move.
        let dirty: HashSet<&str> = if rebuilt_all { edges.iter().copied().collect() } else { HashSet::new() };
        let cond_edges = cond_edges_for(&edges, &scc_rules);
        self.refresh_cond_cache(&cond_edges, &dirty)?;
        for (r, cs) in &seed_rules { self.eval_closure_seed_rule(r, cs)?; }
        for r in &scc_rules { self.eval_scc_rule(r)?; }
        for r in &node2vec_rules { self.eval_node2vec_rule(r)?; }
        // The priming tick skips `?` evaluation: it exists only to derive the
        // coordinates a data-driven scan / repo-sink reads on the real tick.
        if !self.prime_tick {
            for item in &prog.items {
                if let Item::Query(q) = item { self.run_query(q, &closures)?; }
            }
        }
        self.run_gens(prog, quiet)?;
        // Drain `repo`-sinks AFTER the fixpoint + gens so their bodies see this
        // tick's derived rows. A pull clones + registers into self.repos; the
        // new repo is scannable / appears in the `repo` builtin on the NEXT tick
        // (mid-tick registration would shift the repo set under derived rules).
        self.run_repo_pulls(&repo_sinks)?;
        if self.dropped > 0 {
            eprintln!("[checked-type] dropped {} rows failing file/dir/path checks", self.dropped);
            self.dropped = 0;
        }
        if tick_audit() {
            let mut counts: Vec<(String, i64)> = Vec::new();
            for rel in self.rels.keys() {
                let n: i64 = self.db.conn().query_row(
                    &format!("SELECT COUNT(*) FROM {}", tbl(rel)), [], |r| r.get(0))?;
                counts.push((rel.clone(), n));
            }
            counts.sort_by(|a, b| a.0.cmp(&b.0));
            eprintln!("[audit] {} relation(s)", counts.len());
            for (rel, n) in &counts { eprintln!("[audit]   {rel}: {n}"); }
        }
        self.last_n1 = self.db.tick_end();
        Ok(())
    }

    /// Reactive tick driven by a known set of changed paths (from the file
    /// watcher): reconciles only those paths, never walking or statting the
    /// tree. Only WORK source rules participate; route git-rev changes to `tick`.
    #[tracing::instrument(skip_all, fields(n_changed = changed.len()), level = "info")]
    pub fn tick_paths(&mut self, prog: &Program, changed: &[PathBuf], quiet: bool) -> Result<()> {
        let _tick_started = std::time::Instant::now();
        self.rev_cache.clear();
        self.extraction_drops.clear();
        CMD_COUNT.store(0, std::sync::atomic::Ordering::Relaxed);
        self.db.tick_begin();
        let rules: Vec<&Rule> = prog.items.iter().filter_map(|i| match i { Item::Rule(r) => Some(r), _ => None }).collect();
        let closures = closure_map(&rules);
        self.declare_all(prog, &closures)?;
        self.ensure_meta()?;

        // A `repo`-sink program pulls dynamically; the drain runs only on the
        // full-tick path, so an incremental tick defers to the full tick. (The
        // sink's body depends on derived relations whose churn is otherwise
        // invisible to the path-scoped reconcile.) A data-driven scan (variable
        // repo/rev) reads last tick's coordinate relation at reconcile time too,
        // so it also defers.
        if rules.iter().any(|r| r.is_repo_sink() || scan_has_var_coords(r)) {
            tracing::debug!("full-tick fallback: program has a repo-sink or data-driven scan");
            return self.tick(prog, quiet);
        }

        let source_rules: Vec<&Rule> = rules.iter().copied().filter(|r| r.is_source()).collect();
        let all_derived: Vec<&Rule> = rules.iter().copied()
            .filter(|r| !r.is_source() && r.closure_edge().is_none() && r.scc_edge().is_none()
                && r.node2vec_edge().is_none()).collect();
        let scc_rules: Vec<&Rule> = rules.iter().copied()
            .filter(|r| r.scc_edge().is_some()).collect();
        let node2vec_rules: Vec<&Rule> = rules.iter().copied()
            .filter(|r| r.node2vec_edge().is_some()).collect();
        check_stratification(&all_derived, &closures)?;
        let (seed_rules, derived_rules) = split_seed_and_derived(&all_derived, &closures)?;
        let mut source_rels: Vec<String> = Vec::new();
        for r in &source_rules { if !source_rels.contains(&r.head.rel) { source_rels.push(r.head.rel.clone()); } }
        let mut derived_rels: Vec<String> = Vec::new();
        for r in &derived_rules { if !derived_rels.contains(&r.head.rel) { derived_rels.push(r.head.rel.clone()); } }
        let edges: Vec<&str> = dedup_edges(&closures);
        self.create_auto_indexes(&derived_rules, &closures)?;

        // An edited source rule invalidates extractions everywhere, not just at
        // the delta paths: fall back to the full tick, whose reconcile
        // re-extracts the dirty relation's entire file set and persists the new
        // rule digests.
        if !self.source_rule_digests(&source_rules)?.0.is_empty() {
            tracing::debug!("full-tick fallback: source rule edited");
            return self.tick(prog, quiet);
        }
        // Same for the derived layer: an edited derived rule or ground fact
        // rebuilds everything, which is the full tick's job.
        let der_digest = derived_program_digest(&derived_rules, &seed_rules, &edges);
        if self.load_rel_digest("derived:program")? != Some(der_digest) {
            tracing::debug!("full-tick fallback: derived rule/ground-fact edited");
            return self.tick(prog, quiet);
        }
        // A changed path outside self.root (a config or dynamically-registered
        // repo's source edit) can't be reconciled by the path-scoped loop below,
        // which strips against self.root only. Fall back to the full tick so
        // every folder in view stays reactive, not just the self worktree.
        if changed.iter().any(|p| !p.starts_with(&self.root)) {
            tracing::debug!("full-tick fallback: changed path outside self.root (#6a)");
            return self.tick(prog, quiet);
        }

        // WORK source rules with compiled glob matchers
        // The incremental watcher delta covers the self repo's WORK tree only
        // (changed paths under self.root); non-self repos scan via the full tick.
        let mut work_rules: Vec<(&Rule, globset::GlobMatcher)> = Vec::new();
        for r in &source_rules {
            // Variable-coord scans defer to the full tick (guarded above), so
            // every remaining source rule has a literal scan_spec here.
            let spec = scan_spec_of(r)?;
            let (repo, declared, glob) = (str_of(&spec.repo)?, str_of(&spec.rev)?, str_of(&spec.glob)?);
            let is_self = repo.is_empty() || repo == "." || repo == "self";
            if declared == "WORK" && is_self { work_rules.push((*r, globset::Glob::new(&glob)?.compile_matcher())); }
        }

        let prev = self.load_file_meta()?;
        let mut changed_facts = false;
        let mut changed_source_rels: HashSet<String> = HashSet::new();
        let mut module_delta_paths: HashSet<String> = HashSet::new();
        let mut module_full_work = false;
        // Files whose `_file` row changed this tick (modified or deleted),
        // for the path-scoped CST `node`/`child` refresh. A file that the
        // digest prune skipped (content unchanged) is NOT added.
        let mut node_delta_paths: HashSet<String> = HashSet::new();
        let mut scip_changed = false;
        let (mut extracted, mut retracted, mut npaths) = (0usize, 0usize, 0usize);
        let mut seen: HashSet<String> = HashSet::new();
        let wants_module_rels = module_rels_used(prog);
        let wants_scip_rels = scip_rels_used(prog);
        // The watcher only watches this engine's own `--root`, so every
        // incrementally-changed file belongs to the self repo.
        let slug = self.self_slug();

        for p in changed {
            let rel = match p.strip_prefix(&self.root) { Ok(r) => r.to_string_lossy().replace('\\', "/"), Err(_) => continue };
            if !seen.insert(rel.clone()) { continue; }
            if wants_scip_rels && rel == "index.scip" { scip_changed = true; }
            let matching: Vec<&Rule> = work_rules.iter().filter(|(_, m)| m.is_match(&rel)).map(|(r, _)| *r).collect();
            if matching.is_empty() {
                if wants_module_rels && module_manifest_path(&rel) { module_full_work = true; }
                continue;
            }
            npaths += 1;
            let abs = self.root.join(&rel);
            if abs.is_file() {
                let bytes = std::fs::read(&abs).unwrap_or_default();
                let h = blake3::hash(&bytes).to_hex().to_string();
                if prev.get(&(slug.clone(), rel.clone(), "WORK".to_string())).map(|t| &t.0) == Some(&h) { continue; }
                if prev.contains_key(&(slug.clone(), rel.clone(), "WORK".to_string())) {
                    module_delta_paths.insert(rel.clone());
                } else {
                    module_full_work = true;
                }
                node_delta_paths.insert(rel.clone());
                retracted += self.retract_path(&slug, &rel, &source_rels)?;
                // Collect located spans across every matching rule for this file and
                // flush once after the loop (one `bump()`), not per-rule. Per-rule
                // flushing trips the N+1 screamer once enough files change.
                let mut where_rows: Vec<(String, String, spine::WhereBytes, Option<String>)> = Vec::new();
                for rule in &matching {
                    let (rows, where_bytes, dropped) = parse_file(rule, &slug, &rel, "WORK", &h, &self.root, &self.rels, &self.rev_index, &[])?;
                    self.dropped += dropped;
                    if dropped > 0 { self.record_extraction_drop(&rel, &rule.head.rel, dropped); }
                    let meta = self.rels.get(&rule.head.rel)
                        .ok_or_else(|| anyhow::anyhow!("unknown relation {}", rule.head.rel))?.clone();
                    extracted += self.insert_source_rows(&rule.head.rel, &meta, &slug, &rel, &rows)?;
                    where_rows.extend(where_bytes.into_iter().map(|(w, t)| (slug.clone(), rel.clone(), w, Some(t))));
                    changed_source_rels.insert(rule.head.rel.clone());
                }
                self.insert_spine_where_bytes(&where_rows)?;
                let (mt, sz) = std::fs::metadata(&abs).ok().map(|m| (mtime_secs(&m), m.len() as i64)).unwrap_or((0, 0));
                self.db.conn().execute(
                    "INSERT INTO _file(repo, path, rev, hash, mtime, size) VALUES (?1, ?2, 'WORK', ?3, ?4, ?5)
                     ON CONFLICT(repo, path, rev) DO UPDATE SET hash=excluded.hash, mtime=excluded.mtime, size=excluded.size",
                    rusqlite::params![slug, rel, h, mt, sz])?;
                changed_facts = true;
            } else {
                if prev.contains_key(&(slug.clone(), rel.clone(), "WORK".to_string())) { module_full_work = true; }
                node_delta_paths.insert(rel.clone());
                retracted += self.retract_path(&slug, &rel, &source_rels)?;
                self.db.conn().execute("DELETE FROM _file WHERE repo = ?1 AND path = ?2 AND rev = 'WORK'", [&slug, &rel])?;
                for rule in &matching { changed_source_rels.insert(rule.head.rel.clone()); }
                changed_facts = true;
            }
        }

        // A changed file's bytes moved, but did its extracted rows? Prune the
        // source rels whose content digest is unchanged (comment/format edits),
        // so they do not propagate a rebuild. v4's `Replay` at relation grain.
        let files_changed = changed_facts;
        if changed_facts {
            changed_source_rels = self.prune_unchanged_by_digest(changed_source_rels)?;
        }
        // The file set itself (path/hash/rev) is the built-in `file`/`content`/
        // `rev` relations, so any file change makes them changed inputs: refresh
        // them and mark them changed so rules that join `file` re-derive. This is
        // separate from the per-source-rel digest prune above (a comment edit
        // leaves `fn` unchanged but does change the file's content hash).
        if files_changed {
            self.refresh_builtin_rels()?;
            for b in BUILTIN_RELS { changed_source_rels.insert(b.to_string()); }
            if type_rels_used(prog) || type_shape_rels_used(prog) || type_lgg_rels_used(prog) || doc_text_rels_used(prog) {
                self.refresh_type_rels()?;
                for t in TYPE_RELS { changed_source_rels.insert(t.to_string()); }
                for t in DOC_TEXT_RELS { changed_source_rels.insert(t.to_string()); }
            }
            if call_rels_used(prog) {
                self.refresh_call_rels()?;
                for r in CALL_RELS { changed_source_rels.insert(r.to_string()); }
            }
            if dataflow_rels_used(prog) {
                self.refresh_dataflow_rels()?;
                for r in DATAFLOW_RELS { changed_source_rels.insert(r.to_string()); }
            }
            if doc_rels_used(prog) {
                self.refresh_doc_rels()?;
                for r in DOC_RELS { changed_source_rels.insert(r.to_string()); }
            }
            // Node rels write into the spine meta tables, so refresh them BEFORE
            // the spine projection (else this tick's `ref`/`string` miss node spans).
            // Path-scoped: re-walk ONLY this tick's changed files; the other
            // files' node/child rows are untouched.
            if node_rels_used(prog) && self.refresh_node_rels_delta(&node_delta_paths)? {
                for n in NODE_RELS { changed_source_rels.insert(n.to_string()); }
                changed_facts = true;
            }
            if spine_rels_used(prog) {
                self.refresh_spine_rels()?;
                for s in SPINE_RELS { changed_source_rels.insert(s.to_string()); }
            }
        }
        if wants_module_rels && (module_full_work || !module_delta_paths.is_empty()) {
            if module_full_work {
                self.refresh_module_rels_for_revs(&["WORK"])?;
            } else {
                self.refresh_module_rels_for_paths("WORK", &module_delta_paths)?;
            }
            for m in MODULE_RELS { changed_source_rels.insert(m.to_string()); }
            changed_facts = true;
        }
        if wants_scip_rels && scip_changed {
            self.refresh_scip_rels()?;
            for s in SCIP_RELS { changed_source_rels.insert(s.to_string()); }
            changed_facts = true;
        }
        // Every save can move the worktree diff, so re-read it whenever the
        // program joins `changed`; the refresh returns false (set unchanged)
        // on the common no-op, keeping the rebuild scope tight.
        if changed_rels_used(prog) && self.refresh_changed_rel()? {
            changed_source_rels.insert("changed".to_string());
            changed_facts = true;
        }
        if agent_rels_used(prog) && self.refresh_agent_rels()? {
            for a in AGENT_RELS { changed_source_rels.insert(a.to_string()); }
            changed_facts = true;
        }
        if changed_line_rels_used(prog) && self.refresh_changed_line_rel()? {
            changed_source_rels.insert("changed_line".to_string());
            changed_facts = true;
        }
        if created_rels_used(prog) && self.refresh_created_rel()? {
            changed_source_rels.insert("created".to_string());
            changed_facts = true;
        }
        if embed_rels_used(prog) && self.refresh_embed_rels()? {
            changed_source_rels.insert("similar".to_string());
            changed_facts = true;
        }
        if propose_extract_rels_used(prog) && self.refresh_propose_extract_rel()? {
            changed_source_rels.insert("propose_extract".to_string());
            changed_facts = true;
        }
        if propose_clone_rels_used(prog) && self.refresh_propose_clone_rel()? {
            changed_source_rels.insert("propose_clone".to_string());
            changed_facts = true;
        }
        if type_shape_rels_used(prog) && self.refresh_type_shape_rel()? {
            changed_source_rels.insert("type_shape".to_string());
            changed_facts = true;
        }
        if type_lgg_rels_used(prog) && self.refresh_type_lgg_rel()? {
            changed_source_rels.insert("type_lgg".to_string());
            changed_facts = true;
        }
        if daemon_rels_used(prog) { self.refresh_daemon_rels()?; }
        if catalog_rels_used(prog) && self.refresh_catalog_rel()? {
            changed_source_rels.insert("rel_catalog".to_string());
            changed_facts = true;
        }
        if changed_source_rels.is_empty() { changed_facts = false; }

        // Cold start (or empty derived/closure) needs a full rebuild; otherwise
        // rebuild only the derived rels dependency-reachable from what changed,
        // plus the closures over affected edges. Untouched chains are left intact.
        let need_full = self.any_derived_empty(&derived_rels)? || self.any_closure_empty(&edges)?;
        let mut rebuilt: Vec<String> = Vec::new();
        // Edges whose source/derived relation was rebuilt this tick; only these
        // are re-considered by the cond cache (the rest are reused untouched).
        let mut dirty_edges: HashSet<&str> = HashSet::new();
        if need_full {
            self.rebuild_derived(&derived_rules, &derived_rels)?;
            self.rebuild_closures(&edges)?;
            rebuilt = derived_rels.clone();
            dirty_edges = edges.iter().copied().collect();
        } else if changed_facts {
            let affected = affected_derived(&derived_rules, &changed_source_rels);
            let sub_rules: Vec<&Rule> = derived_rules.iter().copied()
                .filter(|r| affected.contains(&r.head.rel)).collect();
            let sub_rels: Vec<String> = derived_rels.iter()
                .filter(|r| affected.contains(*r)).cloned().collect();
            self.rebuild_derived(&sub_rules, &sub_rels)?;
            let aff_edges: Vec<&str> = edges.iter().copied()
                .filter(|e| affected.contains(*e) || changed_source_rels.contains(*e)).collect();
            self.rebuild_closures(&aff_edges)?;
            dirty_edges = aff_edges.iter().copied().collect();
            rebuilt = sub_rels;
        }

        if !quiet {
            let what = if need_full { "ALL".to_string() }
                       else if rebuilt.is_empty() { "none".to_string() }
                       else { rebuilt.join(",") };
            eprintln!("[tick] {npaths} path(s) changed, +{extracted} -{retracted} source facts, rebuilt derived: {what}");
        }
        let cond_edges = cond_edges_for(&edges, &scc_rules);
        self.refresh_cond_cache(&cond_edges, &dirty_edges)?;
        for (r, cs) in &seed_rules { self.eval_closure_seed_rule(r, cs)?; }
        for r in &scc_rules { self.eval_scc_rule(r)?; }
        for r in &node2vec_rules { self.eval_node2vec_rule(r)?; }
        for item in &prog.items { if let Item::Query(q) = item { self.run_query(q, &closures)?; } }
        self.run_gens(prog, quiet)?;
        if self.dropped > 0 { eprintln!("[checked-type] dropped {} rows", self.dropped); self.dropped = 0; }
        self.last_n1 = self.db.tick_end();
        // Surface a slow incremental tick in the LSP server log so live
        // dogfooding catches a perf regression (e.g. a CST refresh that
        // silently went full-corpus). Gated on `tick_log_ms()` (env
        // `DL_TICK_LOG_MS`, default 250ms) so a normal fast tick is silent.
        let tick_ms = _tick_started.elapsed().as_secs_f64() * 1000.0;
        if tick_ms >= tick_log_ms() {
            eprintln!("[tick] incremental tick took {tick_ms:.1}ms over {} changed path(s)", changed.len());
        }
        Ok(())
    }

    fn ensure_meta(&self) -> Result<()> {
        self.db.conn().execute_batch(
            "CREATE TABLE IF NOT EXISTS _file (repo TEXT NOT NULL DEFAULT '', path TEXT, rev TEXT, hash TEXT,
                 mtime INTEGER DEFAULT 0, size INTEGER DEFAULT 0, PRIMARY KEY (repo, path, rev));
             CREATE TABLE IF NOT EXISTS _prov (rel TEXT, repo TEXT NOT NULL DEFAULT '', path TEXT, src TEXT, PRIMARY KEY (rel, repo, path, src));
             CREATE TABLE IF NOT EXISTS _reldigest (rel TEXT PRIMARY KEY, digest TEXT);
             -- CST node path attribution (not a public rel column): maps a
             -- node id to its source path so the delta refresh can prune one
             -- file's `node` rows. The `node.file` column is a content FileId
             -- shared by byte-identical files, so it cannot key the prune.
             CREATE TABLE IF NOT EXISTS _node_path (id TEXT PRIMARY KEY, path TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS _node_path_path_idx ON _node_path(path);
             CREATE TABLE IF NOT EXISTS _strings (
                 id TEXT PRIMARY KEY,
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
                 string_id TEXT NOT NULL,
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
             CREATE TABLE IF NOT EXISTS _node_embeddings (
                 node TEXT NOT NULL,
                 graph TEXT NOT NULL,
                 dim INTEGER NOT NULL,
                 vec TEXT NOT NULL,
                 PRIMARY KEY (node, graph)
             );
             CREATE INDEX IF NOT EXISTS _node_embeddings_graph_idx ON _node_embeddings(graph);
             CREATE INDEX IF NOT EXISTS _strings_norm_idx ON _strings(norm);
             CREATE INDEX IF NOT EXISTS _where_bytes_string_idx ON _where_bytes(string_id);
             CREATE INDEX IF NOT EXISTS _where_bytes_file_span_idx ON _where_bytes(file_id, lo, hi);
             CREATE INDEX IF NOT EXISTS _where_bytes_path_idx ON _where_bytes(path);
             INSERT OR IGNORE INTO _strings (id, content, norm) VALUES ('0', '', '');
             INSERT OR IGNORE INTO _files (id, content_hash, path, size)
                 VALUES ('0', '0000000000000000000000000000000000000000000000000000000000000000', '', 0);
             INSERT OR IGNORE INTO _where_bytes (id, string_id, file_id, lo, hi, repo, rev, path)
                 VALUES ('0', '0', '0', 0, 0, '0', '0', '');"
        )?;
        // tolerate dbs created before mtime/size existed
        let _ = self.db.conn().execute("ALTER TABLE _file ADD COLUMN mtime INTEGER DEFAULT 0", []);
        let _ = self.db.conn().execute("ALTER TABLE _file ADD COLUMN size INTEGER DEFAULT 0", []);
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
                     mtime INTEGER DEFAULT 0, size INTEGER DEFAULT 0, PRIMARY KEY (repo, path, rev));
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
    /// cold run has a baseline to compare against.
    fn seed_rel_digests(&self, source_rels: &[String]) -> Result<()> {
        for rel in source_rels {
            let d = self.rel_digest(rel)?;
            self.save_rel_digest(rel, &d)?;
        }
        Ok(())
    }

    fn any_derived_empty(&self, derived_rels: &[String]) -> Result<bool> {
        for rel in derived_rels {
            let n: i64 = self.db.conn().query_row(&format!("SELECT COUNT(*) FROM {}", tbl(rel)), [], |r| r.get(0))?;
            if n == 0 { return Ok(true); }
        }
        Ok(false)
    }

    #[tracing::instrument(skip_all, fields(n_rules = source_rules.len()), level = "debug")]
    fn reconcile_sources(&mut self, source_rules: &[&Rule], source_rels: &[String]) -> Result<Reconcile> {
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
        let enumerated: Vec<Result<Vec<(String, String, i64, i64)>>> = group_list.par_iter()
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
            for (path, h, mt, sz) in files? {
                current.insert((slug.clone(), path.clone(), rev.clone()), (h.clone(), mt, sz));
                for (idx, m, hb) in &matchers {
                    if m.is_match(&path) {
                        rule_files.push((*idx, slug.clone(), path.clone(), rev.clone(), h.clone(), hb.clone()));
                    }
                }
            }
        }
        self.rev_index = current.keys().map(|(repo, p, r)| (repo.clone(), r.clone(), p.clone())).collect();

        let hash_of = |m: &FileMeta, repo: &str, p: &str, r: &str|
            m.get(&(repo.to_string(), p.to_string(), r.to_string())).map(|t| t.0.clone());

        // An edited source rule must re-extract files whose content did not
        // change. A dirty rel widens retraction to its whole file set; the new
        // digests persist only after the re-extraction lands (end of this fn).
        let (dirty_rels, pending_digests) = self.source_rule_digests(source_rules)?;

        // Retraction key is (repo, path): `_prov` prunes by that pair, so two
        // repos at the same path do not retract each other's source rows.
        let mut to_retract: HashSet<(String, String)> = HashSet::new();
        for ((repo, path, rev), (h, _, _)) in &current {
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
        let mut stmt = self.db.conn().prepare("SELECT repo, path, rev, hash, mtime, size FROM _file")?;
        let rows = stmt.query_map([], |r| Ok((
            (r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?),
            (r.get::<_, String>(3)?, r.get::<_, i64>(4)?, r.get::<_, i64>(5)?),
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
        let rows: Vec<Vec<Value>> = delta.iter().map(|((repo, path, rev), (h, mt, sz))| vec![
            Value::Text(repo.clone()),
            Value::Text(path.clone()),
            Value::Text(rev.clone()),
            Value::Text(h.clone()),
            Value::Int(*mt),
            Value::Int(*sz),
        ]).collect();
        self.db.insert_rows("_file", &["repo", "path", "rev", "hash", "mtime", "size"], &rows)?;
        self.insert_spine_files(&delta)?;
        Ok(())
    }

    fn insert_spine_files(&self, current: &FileMeta) -> Result<usize> {
        let mut by_id: BTreeMap<String, (String, String, i64)> = BTreeMap::new();
        for ((_repo, path, _rev), (hash, _mt, size)) in current {
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
        // Migrate a stale cached table whose column set no longer matches the
        // decl (e.g. a release added a leading column). Rel tables are derived
        // and rebuilt every tick, so dropping loses nothing — and it avoids
        // `CREATE TABLE IF NOT EXISTS` silently keeping the old shape, which
        // makes the next `refresh_rel` insert fail ("no column named ...").
        if !d.cols.is_empty() {
            let table = tbl(&d.name);
            let want: Vec<String> = d.cols.iter().map(|c| c.name.clone()).collect();
            let have: Vec<String> = {
                let conn = self.db.conn();
                let mut have = Vec::new();
                if let Ok(mut s) = conn.prepare(&format!("PRAGMA table_info({table})")) {
                    if let Ok(rows) = s.query_map([], |r| r.get::<_, String>(1)) {
                        for c in rows.flatten() { if c != "__src" { have.push(c); } }
                    }
                }
                have
            };
            if !have.is_empty() && have != want {
                self.db.conn().execute(&format!("DROP TABLE IF EXISTS {table}"), [])?;
                self.db.conn().execute(&format!("DELETE FROM _reldigest WHERE rel = ?1"),
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
            let pk: Vec<String> = d.cols.iter().map(|c| format!("\"{}\"", c.name)).collect();
            format!(
                "CREATE TABLE IF NOT EXISTS {} ({}, __src TEXT DEFAULT '', PRIMARY KEY ({}))",
                tbl(&d.name), cols.join(", "), pk.join(", ")
            )
        };
        self.db.conn().execute(&sql, [])?;
        self.rels.insert(d.name.clone(), RelMeta { cols: d.cols.clone() });
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
                    bail!("{} is a built-in type-graph relation (type_edge / type_edge_rev / type_entity / type_sig / type_link); pick another name", d.name);
                }
                if DOC_TEXT_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in doc relation (doc_comment / doc_tag); pick another name", d.name);
                }
                if CALL_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in call-graph relation (call_def / call_site / call_edge / call_edge_rev / call_name / call_kind); pick another name", d.name);
                }
                if DATAFLOW_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in dataflow relation (df_node / df_edge / loop_over / allocates / nest / df_param); pick another name", d.name);
                }
                if DOC_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in document relation (doc_node / doc_ref); pick another name", d.name);
                }
                if SCIP_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in SCIP relation; pick another name", d.name);
                }
                if SPINE_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in ref-spine relation (string / ref); pick another name", d.name);
                }
                if NODE_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in CST relation (node / child); pick another name", d.name);
                }
                if CHANGED_RELS.contains(&d.name.as_str()) {
                    bail!("{} is the built-in worktree-diff relation; pick another name", d.name);
                }
                if AGENT_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in agent-harness relation (agent_edit / agent_touch); pick another name", d.name);
                }
                if CHANGED_LINE_RELS.contains(&d.name.as_str()) {
                    bail!("{} is the built-in line-diff relation; pick another name", d.name);
                }
                if CREATED_RELS.contains(&d.name.as_str()) {
                    bail!("{} is the built-in file-authorship relation; pick another name", d.name);
                }
                if EMBED_RELS.contains(&d.name.as_str()) {
                    bail!("{} is the built-in embedding-similarity relation (similar); pick another name", d.name);
                }
                if PROPOSE_EXTRACT_RELS.contains(&d.name.as_str()) {
                    bail!("{} is the built-in extract-proposal relation; pick another name", d.name);
                }
                if PROPOSE_CLONE_RELS.contains(&d.name.as_str()) {
                    bail!("{} is the built-in clone-detection relation; pick another name", d.name);
                }
                if TYPE_SHAPE_RELS.contains(&d.name.as_str()) {
                    bail!("{} is the built-in type-shape relation; pick another name", d.name);
                }
                if TYPE_LGG_RELS.contains(&d.name.as_str()) {
                    bail!("{} is the built-in type-lgg relation; pick another name", d.name);
                }
                if DAEMON_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in daemon-state relation (program / head / rev_advanced); pick another name", d.name);
                }
                if CATALOG_RELS.contains(&d.name.as_str()) {
                    bail!("{} is the built-in self-describing relation catalog (rel_catalog); pick another name", d.name);
                }
                match closures.get(&d.name) {
                    Some(edge) => self.declare_closure(d, edge)?,
                    None => self.declare(d)?,
                }
            }
        }
        self.declare_builtins()?;
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
        for d in call_rel_decls() { self.declare(&d)?; }
        for d in dataflow_rel_decls() { self.declare(&d)?; }
        for d in doc_rel_decls() { self.declare(&d)?; }
        for d in scip_rel_decls() { self.declare(&d)?; }
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
        for d in changed_rel_decls() { self.declare(&d)?; }
        for d in agent_rel_decls() { self.declare(&d)?; }
        for d in changed_line_rel_decls() { self.declare(&d)?; }
        for d in created_rel_decls() { self.declare(&d)?; }
        for d in embed_rel_decls() { self.declare(&d)?; }
        for d in propose_extract_rel_decls() { self.declare(&d)?; }
        for d in propose_clone_rel_decls() { self.declare(&d)?; }
        for d in type_shape_rel_decls() { self.declare(&d)?; }
        for d in type_lgg_rel_decls() { self.declare(&d)?; }
        for d in daemon_rel_decls() { self.declare(&d)?; }
        for d in catalog_rel_decls() { self.declare(&d)?; }
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

    /// Project the durable `_strings` / `_where_bytes` meta tables into the
    /// query-facing `string` / `ref` relations. Wholesale wipe + repopulate,
    /// skipping the zero sentinels so queries see only real interned rows.
    ///
    /// `delta = None` executes the wholesale body verbatim (current behavior).
    /// `delta = Some(_)` is the incremental path: only the rows named in the
    /// delta need to be merged into `string` / `ref`. That path is not yet
    /// expanded; callers pass `None` at every existing call site. When the
    /// incremental path is wired, replace the `None` calls in the per-tick
    /// changed-file loop with `Some(&delta)` after `insert_spine_where_bytes`
    /// flushes the staged vecs -- collect-then-flush, one `insert_rows` call
    /// per table, never per-row.
    fn refresh_spine_rels_delta(&self, delta: Option<&SpineDelta>) -> Result<()> {
        // The incremental path is not yet expanded; fall through to wholesale.
        // When Some() is implemented, remove this comment and the wholesale read
        // below for the Some branch.
        let _ = delta; // future Some() will drive a targeted merge instead
        let conn = self.db.conn();
        let mut s = conn.prepare("SELECT id, content, norm FROM _strings WHERE id != '0'")?;
        let strings: Vec<Vec<Value>> = s
            .query_map([], |r| Ok(vec![
                Value::Text(r.get::<_, String>(0)?),
                Value::Text(r.get::<_, String>(1)?),
                Value::Text(r.get::<_, String>(2)?),
            ]))?
            .filter_map(|x| x.ok()).collect();
        let mut w = conn.prepare(
            "SELECT id, string_id, file_id, lo, hi FROM _where_bytes WHERE id != '0'")?;
        let refs: Vec<Vec<Value>> = w
            .query_map([], |r| Ok(vec![
                Value::Text(r.get::<_, String>(0)?),
                Value::Text(r.get::<_, String>(1)?),
                Value::Text(r.get::<_, String>(2)?),
                Value::Int(r.get::<_, i64>(3)?),
                Value::Int(r.get::<_, i64>(4)?),
            ]))?
            .filter_map(|x| x.ok()).collect();
        drop(s);
        drop(w);
        self.refresh_rel("string", &["id", "text", "norm"], &strings)?;
        self.refresh_rel("ref", &["id", "string", "file", "lo", "hi"], &refs)?;
        Ok(())
    }

    /// Thin shim so existing call sites require no signature change.
    fn refresh_spine_rels(&self) -> Result<()> {
        self.refresh_spine_rels_delta(None)
    }

    /// Wholesale replace one engine-owned relation through the same plural write
    /// seam every built-in module/indexer uses.
    fn refresh_rel(&self, rel: &str, cols: &[&str], rows: &[Vec<Value>]) -> Result<usize> {
        let table = tbl(rel);
        self.db.exec(&format!("DELETE FROM {table}"))?;
        self.db.insert_rows(&table, cols, rows)
    }

    /// Populate the self-describing `rel_catalog`: one row per built-in relation,
    /// joining the declarations (for the column list) with `builtin_rel_docs`
    /// (for group + summary). Static — independent of any scanned file — so it is
    /// the same every tick; emitted only when a program reads it. The columns are
    /// rendered as `(a, b, c)` so a generator can splice them straight into a doc
    /// table. Returns true (rows always present) to match the refresh-gate shape.
    fn refresh_catalog_rel(&self) -> Result<bool> {
        let docs: std::collections::HashMap<&str, (&str, &str)> =
            builtin_rel_docs().iter().map(|(n, g, s)| (*n, (*g, *s))).collect();
        let rows: Vec<Vec<Value>> = all_builtin_decls().iter().map(|d| {
            let cols = format!("({})",
                d.cols.iter().map(|c| c.name.clone()).collect::<Vec<_>>().join(", "));
            let (group, summary) = docs.get(d.name.as_str()).copied().unwrap_or(("", ""));
            vec![Value::Text(d.name.clone()), Value::Text(group.to_string()),
                 Value::Text(cols), Value::Text(summary.to_string())]
        }).collect();
        self.refresh_rel("rel_catalog", &["name", "group", "cols", "doc"], &rows)?;
        Ok(true)
    }

    /// Project the persisted daemon-state meta tables (`_program` / `_ref` /
    /// `_rev_log`) into the `program` / `head` / `rev_advanced` query relations.
    /// Wholesale wipe + repopulate, same shape as `refresh_spine_rels`; the
    /// underlying tables are written out of band by the daemon, so this is a
    /// pure read-back projection (cheap, bounded by the loaded program + watched
    /// ref count).
    pub fn refresh_daemon_rels(&self) -> Result<()> {
        let conn = self.db.conn();
        let mut p = conn.prepare("SELECT path, hash, mtime FROM _program")?;
        let programs: Vec<Vec<Value>> = p
            .query_map([], |r| Ok(vec![
                Value::Text(r.get::<_, String>(0)?),
                Value::Text(r.get::<_, String>(1)?),
                Value::Int(r.get::<_, i64>(2)?),
            ]))?
            .filter_map(|x| x.ok()).collect();
        let mut h = conn.prepare("SELECT repo, name, oid FROM _ref")?;
        let heads: Vec<Vec<Value>> = h
            .query_map([], |r| Ok(vec![
                Value::Text(r.get::<_, String>(0)?),
                Value::Text(r.get::<_, String>(1)?),
                Value::Text(r.get::<_, String>(2)?),
            ]))?
            .filter_map(|x| x.ok()).collect();
        let mut a = conn.prepare("SELECT repo, name, old, new FROM _rev_log ORDER BY id")?;
        let advances: Vec<Vec<Value>> = a
            .query_map([], |r| Ok(vec![
                Value::Text(r.get::<_, String>(0)?),
                Value::Text(r.get::<_, String>(1)?),
                Value::Text(r.get::<_, String>(2)?),
                Value::Text(r.get::<_, String>(3)?),
            ]))?
            .filter_map(|x| x.ok()).collect();
        drop(p);
        drop(h);
        drop(a);
        self.refresh_rel("program", &["path", "hash", "mtime"], &programs)?;
        self.refresh_rel("head", &["repo", "name", "oid"], &heads)?;
        self.refresh_rel("rev_advanced", &["repo", "name", "old", "new"], &advances)?;
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
            for i in 0..ncols { v.push(r.get::<_, String>(i)?); }
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
            // The head must be all variables (slug, root, url), selected in
            // order. A literal head term is a filter the gen lowering can't
            // express; reject it loudly so the author fixes the shape.
            let vars: Vec<String> = rule.head.terms.iter().filter_map(|t| match t {
                Term::Var(v) => Some(v.clone()),
                _ => None,
            }).collect();
            if vars.len() != rule.head.terms.len() {
                eprintln!("[repo-sink] head must be all variables (slug, root, url); skipping");
                continue;
            }
            let sql = crate::lower::lower_gen(&vars, &rule.body, &self.rels)?;
            let rows: Vec<(String, String, String)> = self.db.conn().prepare(&sql)?
                .query_map([], |r| Ok((r.get::<_, String>(0)?,
                                       r.get::<_, String>(1)?,
                                       r.get::<_, String>(2)?)))?
                .filter_map(|x| x.ok()).collect();
            for (slug, root_str, url) in rows {
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
                let allowed = org.as_ref().is_some_and(|o| allowlist.contains(o));
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

    fn module_files_by_rev(&self) -> Result<HashMap<String, Vec<(String, String)>>> {
        let mut by_rev: HashMap<String, Vec<(String, String)>> = HashMap::new();
        let conn = self.db.conn();
        let mut sel = conn.prepare("SELECT path, rev, hash FROM _file")?;
        let rows = sel.query_map([], |r| Ok((
            r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
        for row in rows.flatten() { by_rev.entry(row.1).or_default().push((row.0, row.2)); }
        Ok(by_rev)
    }

    fn module_rows_for_rev(
        &self,
        rev: &str,
        files: &[(String, String)],
        only_paths: Option<&HashSet<String>>,
        include_crate_edges: bool,
    ) -> ModuleRows {
        let t = |s: &str| Value::Text(s.to_string());
        let root = self.root.clone();
        let resolvers = modgraph::resolvers(&root);
        let fileset: HashSet<String> = files.iter().map(|(p, _)| p.clone()).collect();
        let manifests = self.collect_manifests(rev, &fileset);
        let reader = |p: &str| read_content(&root, rev, p).ok();
        let cx = ProjectCx::new(&root, &fileset, &manifests).with_reader(&reader);
        let selected: Vec<&(String, String)> = files.iter()
            .filter(|(path, _)| match only_paths {
                Some(paths) => paths.contains(path.as_str()),
                None => true,
            })
            .collect();

        let batches: Vec<ModuleRows> = selected.par_iter().map(|(path, hash)| {
            let mut rows = ModuleRows::default();
            let ext = Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("");
            if let Some(res) = resolvers.iter().find(|r| r.exts().contains(&ext)) {
                let content = read_content(&root, rev, path).unwrap_or_default();
                // Same content-addressed file id `_files`/parse_file use, so import
                // spans join `_files` for both WORK and committed revs.
                let where_file = spine::FileId::from_content_address(hash, content.len() as i64)
                    .filter(|f| *f != spine::FileId::SYNTHETIC);
                for mref in res.edges(path, &content, &cx) {
                    rows.imports.push(vec![t(path), t(rev), t(&mref.specifier), Value::Text(mref.kind.to_string()), Value::Int(mref.line as i64)]);
                    if let (Some(file), Some((lo, hi))) = (where_file, mref.span) {
                        let text = content.get(lo as usize..hi as usize).unwrap_or("");
                        if !text.is_empty() {
                            rows.spans.push((path.to_string(), text.to_string(), spine::WhereBytes {
                                string: spine::StringId::of(text), file, lo, hi, ..Default::default()
                            }));
                        }
                    }
                    match mref.target {
                        // A self-edge (e.g. `use crate::X` where X is defined in this
                        // crate root) is not a dependency; drop it so the graph and
                        // its closure have no spurious self-loops.
                        Resolution::File(dst) if &dst != path => {
                            rows.edges_rev.push(vec![t(path), t(&dst), t(rev)]);
                        }
                        Resolution::File(_) => {}
                        Resolution::Unresolved(reason) => {
                            rows.unresolved_rev.push(vec![t(path), t(rev), t(&mref.specifier), t(&reason), Value::Int(mref.line as i64)]);
                        }
                        Resolution::External(_) => {}
                    }
                }
            }
            rows
        }).collect();

        let mut out = ModuleRows::default();
        for batch in batches { out.extend(batch); }
        if include_crate_edges {
            for edge in modgraph::crate_edges(&manifests) {
                out.crate_edges.push(vec![t(&edge.src), t(&edge.dst), t(edge.kind), t(rev)]);
            }
        }
        out
    }

    fn insert_module_rows(&self, rows: &ModuleRows, include_crate_edges: bool) -> Result<()> {
        self.db.insert_rows(&tbl("module_import"), &["file", "rev", "specifier", "kind", "line"], &rows.imports)?;
        self.db.insert_rows(&tbl("module_edge_rev"), &["src", "dst", "rev"], &rows.edges_rev)?;
        self.db.insert_rows(&tbl("module_unresolved_rev"), &["file", "rev", "specifier", "reason", "line"], &rows.unresolved_rev)?;
        if include_crate_edges {
            self.db.insert_rows(&tbl("crate_edge"), &["src", "dst", "kind", "rev"], &rows.crate_edges)?;
        }
        self.insert_module_spans(rows)?;
        Ok(())
    }

    /// Intern each import ref's leaf text into `_strings` and its span into
    /// `_where_bytes`, both through their batched chokepoints, so `string ⋈ ref`
    /// covers the import graph. Called by every module-refresh path.
    fn insert_module_spans(&self, rows: &ModuleRows) -> Result<()> {
        let slug = self.self_slug();
        let string_rows: Vec<(String, String, Vec<Value>)> = rows.spans.iter()
            .map(|(path, text, _)| (slug.clone(), path.clone(), vec![Value::Text(text.clone())])).collect();
        self.insert_spine_strings(&string_rows)?;
        let where_rows: Vec<(String, String, spine::WhereBytes, Option<String>)> = rows.spans.iter()
            .map(|(path, _, wb)| (slug.clone(), path.clone(), *wb, None)).collect();
        self.insert_spine_where_bytes(&where_rows)?;
        Ok(())
    }

    fn rebuild_legacy_module_rels(&self) -> Result<()> {
        let edge = tbl("module_edge");
        let edge_rev = tbl("module_edge_rev");
        let unresolved = tbl("module_unresolved");
        let unresolved_rev = tbl("module_unresolved_rev");
        self.db.exec(&format!("DELETE FROM {edge}"))?;
        self.db.exec(&format!(
            "INSERT OR IGNORE INTO {edge} (\"src\", \"dst\") SELECT \"src\", \"dst\" FROM {edge_rev}"
        ))?;
        self.db.exec(&format!("DELETE FROM {unresolved}"))?;
        self.db.exec(&format!(
            "INSERT OR IGNORE INTO {unresolved} (\"file\", \"specifier\", \"reason\", \"line\") \
             SELECT \"file\", \"specifier\", \"reason\", \"line\" FROM {unresolved_rev}"
        ))?;
        Ok(())
    }

    /// Rebuild the module-graph relations from the `_file` set, per rev. Reads each
    /// file's content, picks the language resolver by extension, and writes one
    /// `module_import` row per reference plus `module_edge(src,dst)` /
    /// `module_edge_rev(src,dst,rev)` for resolved project files and unresolved
    /// relations for ones that should have resolved.
    /// Wholesale wipe + repopulate; gated by `module_rels_used` at the call site.
    /// Edges are resolved within a single rev (cross-rev merge is a Stage-1 corner).
    fn refresh_module_rels(&self) -> Result<()> {
        let by_rev = self.module_files_by_rev()?;
        let mut rows = ModuleRows::default();
        for (rev, files) in &by_rev {
            rows.extend(self.module_rows_for_rev(rev, files, None, true));
        }
        self.refresh_rel("module_import", &["file", "rev", "specifier", "kind", "line"], &rows.imports)?;
        self.refresh_rel("module_edge_rev", &["src", "dst", "rev"], &rows.edges_rev)?;
        self.refresh_rel("module_unresolved_rev", &["file", "rev", "specifier", "reason", "line"], &rows.unresolved_rev)?;
        self.refresh_rel("crate_edge", &["src", "dst", "kind", "rev"], &rows.crate_edges)?;
        self.insert_module_spans(&rows)?;
        self.rebuild_legacy_module_rels()?;
        Ok(())
    }

    fn refresh_module_rels_for_revs(&self, revs: &[&str]) -> Result<()> {
        if revs.is_empty() { return Ok(()); }
        self.db.exec("CREATE TEMP TABLE IF NOT EXISTS _module_refresh_rev(rev TEXT PRIMARY KEY)")?;
        self.db.exec("DELETE FROM _module_refresh_rev")?;
        let rev_rows: Vec<Vec<Value>> = revs.iter().map(|rev| vec![Value::Text((*rev).to_string())]).collect();
        self.db.insert_rows("_module_refresh_rev", &["rev"], &rev_rows)?;
        self.db.exec(&format!("DELETE FROM {} WHERE \"rev\" IN (SELECT rev FROM _module_refresh_rev)", tbl("module_import")))?;
        self.db.exec(&format!("DELETE FROM {} WHERE \"rev\" IN (SELECT rev FROM _module_refresh_rev)", tbl("module_edge_rev")))?;
        self.db.exec(&format!("DELETE FROM {} WHERE \"rev\" IN (SELECT rev FROM _module_refresh_rev)", tbl("module_unresolved_rev")))?;
        self.db.exec(&format!("DELETE FROM {} WHERE \"rev\" IN (SELECT rev FROM _module_refresh_rev)", tbl("crate_edge")))?;

        let by_rev = self.module_files_by_rev()?;
        let mut rows = ModuleRows::default();
        for rev in revs {
            if let Some(files) = by_rev.get(*rev) {
                rows.extend(self.module_rows_for_rev(rev, files, None, true));
            }
        }
        self.insert_module_rows(&rows, true)?;
        self.rebuild_legacy_module_rels()?;
        Ok(())
    }

    fn refresh_module_rels_for_paths(&self, rev: &str, paths: &HashSet<String>) -> Result<()> {
        if paths.is_empty() { return Ok(()); }
        self.db.exec("CREATE TEMP TABLE IF NOT EXISTS _module_refresh_path(path TEXT PRIMARY KEY)")?;
        self.db.exec("DELETE FROM _module_refresh_path")?;
        let path_rows: Vec<Vec<Value>> = paths.iter().map(|p| vec![Value::Text(p.clone())]).collect();
        self.db.insert_rows("_module_refresh_path", &["path"], &path_rows)?;
        self.db.exec(&format!(
            "DELETE FROM {} WHERE \"rev\" = '{rev}' AND \"file\" IN (SELECT path FROM _module_refresh_path)",
            tbl("module_import"),
        ))?;
        self.db.exec(&format!(
            "DELETE FROM {} WHERE \"rev\" = '{rev}' AND \"src\" IN (SELECT path FROM _module_refresh_path)",
            tbl("module_edge_rev"),
        ))?;
        self.db.exec(&format!(
            "DELETE FROM {} WHERE \"rev\" = '{rev}' AND \"file\" IN (SELECT path FROM _module_refresh_path)",
            tbl("module_unresolved_rev"),
        ))?;

        let by_rev = self.module_files_by_rev()?;
        let rows = by_rev.get(rev)
            .map(|files| self.module_rows_for_rev(rev, files, Some(paths), false))
            .unwrap_or_default();
        self.insert_module_rows(&rows, false)?;
        self.rebuild_legacy_module_rels()?;
        Ok(())
    }

    /// Rebuild the type graph from the `_file` set. This is the same L3
    /// shape as module graph: read tracked Rust/Kotlin/TS files, run a
    /// deterministic syntax extractor, flush one built-in relation through
    /// `refresh_rel`.
    fn refresh_type_rels(&self) -> Result<()> {
        let mut files: Vec<(String, String, String)> = Vec::new();
        {
            let mut sel = self.db.conn().prepare(
                "SELECT repo, path, rev FROM _file WHERE path LIKE '%.rs' OR path LIKE '%.kt' OR path LIKE '%.kts' \
                 OR path LIKE '%.ts' OR path LIKE '%.tsx'")?;
            let rows = sel.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
            for row in rows.flatten() { files.push(row); }
        }
        // Parse + extract per file in parallel (same shape as module_rows_for_rev),
        // then flatten and write once. Keeps the cold-build parse working set bounded
        // by the rayon pool, not the corpus (peak-RSS invariant). Rows carry their
        // rev so the type graph is history-aware like module_edge_rev.
        let root = self.root.clone();
        let roots = self.repo_roots();
        // Per-file extraction via the language registry (no extension if-chain;
        // registry order makes .kts match Kotlin before .ts would). Each file
        // yields its declared entities + edge graph; collected before resolution
        // because name->def resolution is corpus-global (a barrier). Content is
        // read from the file's OWN repo root so config-repo files index too.
        // facts carry the derived repo id (nearest `.git` of the file) so each
        // entity/edge row is attributed to the folder it lives in.
        let facts: Vec<(String, String, String, typegraph::TypeFacts)> = files.par_iter().filter_map(|(repo, path, rev)| {
            let lang = typegraph::type_langs().iter().find(|l| l.matches(path))?;
            let froot = roots.get(repo).map(|p| p.as_path()).unwrap_or(&root);
            let content = read_content(froot, rev, path).unwrap_or_default();
            let rid = repo_id_of(froot, path, repo);
            Some((rid, path.clone(), rev.clone(), lang.extract(path, &content)))
        }).collect();

        // Resolver: a name maps to its definition symbol when exactly one entity
        // in the SAME repo declares it (syntactic). Keying by repo keeps two
        // folders in view that share a name from making each other ambiguous,
        // and the resolved sym is repo-qualified (`{repo}::{sym}`) so the edge
        // relations (type_link/type_sig — no repo column) stay distinct across
        // identical-path repos. A SCIP index, when present, overrides per
        // (file, name) with the indexed def file (collision-proof).
        let mut by_name: HashMap<(&str, &str), Vec<&str>> = HashMap::new();
        let mut sym_at: HashMap<(&str, &str, &str), &str> = HashMap::new();
        for (repo, _, _, f) in &facts {
            for e in &f.entities {
                by_name.entry((repo.as_str(), e.name.as_str())).or_default().push(e.sym.as_str());
                sym_at.insert((repo.as_str(), e.file.as_str(), e.name.as_str()), e.sym.as_str());
            }
        }
        let scip = self.scip_name_defs().unwrap_or_default();
        let resolve = |repo: &str, file: &str, name: &str| -> Option<String> {
            if let Some(def_file) = scip.get(&(file.to_string(), name.to_string())) {
                if let Some(sym) = sym_at.get(&(repo, def_file.as_str(), name)) {
                    return Some(format!("{repo}::{sym}"));
                }
            }
            match by_name.get(&(repo, name)) {
                Some(v) if v.len() == 1 => Some(format!("{repo}::{}", v[0])),
                _ => None,
            }
        };

        let t = |s: &str| Value::Text(s.to_string());
        let i = |n: u32| Value::Int(n as i64);
        let mut edge_rev_rows: Vec<Vec<Value>> = Vec::new();
        let mut entity_rows: Vec<Vec<Value>> = Vec::new();
        let mut sig_rows: Vec<Vec<Value>> = Vec::new();
        let mut link_rows: Vec<Vec<Value>> = Vec::new();
        // Dedup keys carry the repo, so two folders in view that share a relative
        // path + symbol name (e.g. both have `src/index.ts`) do NOT drop each
        // other's rows — each repo's entity survives.
        let mut seen_entity: HashSet<(&str, &str)> = HashSet::new();
        let mut seen_link: HashSet<(String, String, &str)> = HashSet::new();
        let mut doc_rows: Vec<Vec<Value>> = Vec::new();
        let mut tag_rows: Vec<Vec<Value>> = Vec::new();
        let mut seen_doc: HashSet<(&str, &str)> = HashSet::new();
        for (repo, path, rev, f) in &facts {
            // historic name-keyed edges, unchanged
            for edge in &f.edges {
                edge_rev_rows.push(vec![t(&edge.from), t(&edge.to), t(edge.kind), t(rev)]);
                // SCIP-resolved graph: owner sym -> resolved target sym (or the
                // bare name when external/ambiguous, so leaf types still appear)
                let src = sym_at.get(&(repo.as_str(), path.as_str(), edge.from.as_str()))
                    .map(|s| format!("{repo}::{s}")).unwrap_or_else(|| edge.from.clone());
                let dst = resolve(repo, path, &edge.to).unwrap_or_else(|| edge.to.clone());
                if seen_link.insert((src.clone(), dst.clone(), edge.kind)) {
                    link_rows.push(vec![t(&src), t(&dst), t(edge.kind)]);
                }
            }
            for ent in &f.entities {
                // repo-qualified sym: globally unique even when two repos share a
                // relative path, so sym-keyed rels (type_sig/type_link) and the
                // cross-rel joins to call_def stay per-repo distinct.
                let qsym = format!("{repo}::{}", ent.sym);
                if seen_entity.insert((repo.as_str(), ent.sym.as_str())) {
                    let qparent = ent.parent.as_deref().map(|p| format!("{repo}::{p}")).unwrap_or_default();
                    entity_rows.push(vec![
                        t(repo), t(&qsym), t(&ent.name), t(ent.kind.tag()),
                        t(&qparent), t(&ent.file), i(ent.line),
                    ]);
                }
                // the arrow [...A] => B, one row per referenced type per slot
                if let Some(ty) = &ent.ty {
                    for (pos, slot) in ty.params.iter().enumerate() {
                        for r in slot {
                            let rf = resolve(repo, path, r.name()).unwrap_or_else(|| r.name().to_string());
                            sig_rows.push(vec![t(&qsym), t("param"), i(pos as u32), t(&rf)]);
                        }
                    }
                    for r in &ty.ret {
                        let rf = resolve(repo, path, r.name()).unwrap_or_else(|| r.name().to_string());
                        sig_rows.push(vec![t(&qsym), t("ret"), i(0), t(&rf)]);
                    }
                }
            }
            // Doc comments per entity (Tier 1) + their structured tags (Tier 2).
            // Same repo-qualified sym + first-seen dedup as the entity rows, so a
            // file present at two revs doesn't duplicate (doc_comment has no rev).
            for doc in &f.docs {
                if !seen_doc.insert((repo.as_str(), doc.sym.as_str())) { continue; }
                let qsym = format!("{repo}::{}", doc.sym);
                doc_rows.push(vec![t(repo), t(&qsym), i(doc.line), t(&doc.text)]);
                for tag in &doc.tags {
                    tag_rows.push(vec![t(repo), t(&qsym), t(&tag.tag), t(&tag.arg), t(&tag.text)]);
                }
            }
        }
        self.refresh_rel("type_edge_rev", &["from", "to", "kind", "rev"], &edge_rev_rows)?;
        self.refresh_rel("type_entity", &["repo", "sym", "name", "kind", "parent", "file", "line"], &entity_rows)?;
        self.refresh_rel("type_sig", &["sym", "slot", "pos", "ref"], &sig_rows)?;
        self.refresh_rel("type_link", &["src", "dst", "kind"], &link_rows)?;
        self.refresh_rel("doc_comment", &["repo", "sym", "line", "text"], &doc_rows)?;
        self.refresh_rel("doc_tag", &["repo", "sym", "tag", "arg", "text"], &tag_rows)?;
        self.rebuild_legacy_type_rels()?;
        Ok(())
    }

    /// Best-effort SCIP override for resolution: read `scip_ref(file, symbol,
    /// def_file)` and key it by (file, trailing-descriptor-name) -> def_file.
    /// Empty when no index.scip is present, so the syntactic path carries.
    fn scip_name_defs(&self) -> Result<HashMap<(String, String), String>> {
        let mut out = HashMap::new();
        let conn = self.db.conn();
        let Ok(mut s) = conn.prepare(&format!("SELECT file, symbol, def_file FROM {}", tbl("scip_ref"))) else {
            return Ok(out);
        };
        let rows = s.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
        })?;
        for row in rows.flatten() {
            let (file, symbol, def_file) = row;
            if let Some(name) = scip_descriptor_name(&symbol) {
                out.insert((file, name), def_file);
            }
        }
        Ok(out)
    }

    /// Rebuild the convenient rev-less `type_edge(from, to, kind)` from the
    /// rev-aware table, deduped across revs. Same shape as
    /// `rebuild_legacy_module_rels`: the `_rev` table is the source of truth,
    /// the legacy view is the simple closure target.
    fn rebuild_legacy_type_rels(&self) -> Result<()> {
        let edge = tbl("type_edge");
        let edge_rev = tbl("type_edge_rev");
        self.db.exec(&format!("DELETE FROM {edge}"))?;
        self.db.exec(&format!(
            "INSERT OR IGNORE INTO {edge} (\"from\", \"to\", \"kind\") \
             SELECT \"from\", \"to\", \"kind\" FROM {edge_rev}"
        ))?;
        Ok(())
    }

    /// CST-as-relation (christmas #3): walk every NAMED tree-sitter node of every
    /// scanned file (across all 11 `ts_lang` grammars) into `node`/`child`. Same
    /// shape as `refresh_type_rels`: parallel per-file parse (CPU-bound, no DB),
    /// then collect-then-flush (three batched writes: `_strings`/`_where_bytes`
    /// for the spine, `node`, `child`). NO per-row write — the N+1 counter stays
    /// quiet. Gated on `node_rels_used`, so a program that never asks pays
    /// nothing (the full-tree walk is ~100x type_edge; christmas #19 chunked
    /// flush is the later mitigation, NOT built here).
    ///
    /// Each node's id is a kind-salted `_where_bytes` id: `salted(of_located(
    /// raw-slice WhereBytes, repo, path), kind)`. The salt keeps a wrapper node
    /// and its sole identical-span child distinct (else innermost-containment
    /// merges them), while the underlying `_where_bytes` row carries the RAW
    /// slice's StringId, so `ref(id, sid, ..)` -> `string(sid, text, ..)` recovers
    /// the node's source bytes (riding step 1's intern fix).
    fn refresh_node_rels(&self) -> Result<bool> {
        // Whole-corpus walk: read every scanned file from `_file`.
        let files = self.node_file_set(None)?;
        let parsed = self.node_walk(&files);
        self.last_node_files_walked.set(parsed.len());
        let (node_rows, child_rows, path_by_id, str_by_id, wb_by_id) = self.node_rows_from_walk(&parsed);

        // Node ids are content-addressed and kind-salted, so each node row is
        // already unique within a tick (a node can't appear twice in one walk;
        // path folds into the id across files). Early-out by comparing the stored
        // id set to the computed one — if identical, no file's tree moved.
        let computed: std::collections::HashSet<String> = node_rows.iter()
            .filter_map(|row| if let Value::Text(s) = &row[0] { Some(s.clone()) } else { None }).collect();
        let stored: std::collections::HashSet<String> = {
            let conn = self.db.conn();
            let mut s = conn.prepare(&format!("SELECT id FROM {}", tbl("node")))?;
            let set: std::collections::HashSet<String> =
                s.query_map([], |r| r.get::<_, String>(0))?.filter_map(|x| x.ok()).collect();
            set
        };
        if stored == computed { return Ok(false); }

        // Full replace: spine first (so node ids resolve), then a whole-table
        // wipe + reinsert of node/child via refresh_rel. `_node_path` (id->path
        // attribution, not a public rel column) is rebuilt wholesale too so the
        // delta refresh can later prune one file's rows.
        self.flush_node_spine(str_by_id, wb_by_id)?;
        self.refresh_rel("node", &["id", "kind", "file", "lo", "hi", "parent"], &node_rows)?;
        self.refresh_rel("child", &["parent", "child"], &child_rows)?;
        self.db.exec("DELETE FROM _node_path")?;
        self.db.insert_rows("_node_path", &["id", "path"], &path_by_id)?;
        Ok(true)
    }

    /// Path-scoped CST refresh for the incremental tick: re-walk ONLY the changed
    /// files, prune their OLD node/child + spine rows, insert the new walk. The
    /// OTHER files' node/child rows are untouched. Mirrors
    /// `refresh_module_rels_for_paths`. The `_where_bytes`/`_strings` node spans
    /// for the changed files were already pruned by `retract_path` (keyed by
    /// (repo, path)); this re-inserts the fresh spans. Returns true if any node
    /// row changed.
    fn refresh_node_rels_delta(&self, paths: &HashSet<String>) -> Result<bool> {
        if paths.is_empty() { return Ok(false); }
        let files = self.node_file_set(Some(paths))?;
        let parsed = self.node_walk(&files);
        self.last_node_files_walked.set(parsed.len());
        let (node_rows, child_rows, path_by_id, str_by_id, wb_by_id) = self.node_rows_from_walk(&parsed);

        // Prune this tick's changed files' OLD rows: `node` rows whose id is
        // attributed to a changed path (via `_node_path`), plus the `_node_path`
        // rows themselves. `node.file` is a content FileId shared by
        // byte-identical files, so it can't key the prune; `_node_path` keys by
        // the real source path. Other files' node rows stay untouched.
        self.db.exec("CREATE TEMP TABLE IF NOT EXISTS _node_refresh_path(path TEXT PRIMARY KEY)")?;
        self.db.exec("DELETE FROM _node_refresh_path")?;
        let path_rows: Vec<Vec<Value>> = paths.iter().map(|p| vec![Value::Text(p.clone())]).collect();
        self.db.insert_rows("_node_refresh_path", &["path"], &path_rows)?;
        let node_tbl = tbl("node");
        let child_tbl = tbl("child");
        let changed_ids_sql =
            "SELECT id FROM _node_path WHERE path IN (SELECT path FROM _node_refresh_path)";
        // child edges of the changed files: their `child` endpoint id is in the
        // changed-path id set (CST is per-file, so an edge never crosses files).
        // Delete BEFORE pruning `_node_path` so the id subquery still resolves.
        self.db.exec(&format!(
            "DELETE FROM {child_tbl} WHERE \"child\" IN ({changed_ids_sql})"))?;
        self.db.exec(&format!("DELETE FROM {node_tbl} WHERE \"id\" IN ({changed_ids_sql})"))?;
        self.db.exec("DELETE FROM _node_path WHERE path IN (SELECT path FROM _node_refresh_path)")?;

        // Spine first (so the new node ids resolve through `ref`/`string`), then
        // the node rows + their `_node_path` attribution, then re-derive `child`
        // so it never references a node id that no longer exists.
        self.flush_node_spine(str_by_id, wb_by_id)?;
        let nodes_changed = !node_rows.is_empty();
        if nodes_changed {
            self.db.insert_rows(&node_tbl, &["id", "kind", "file", "lo", "hi", "parent"], &node_rows)?;
            self.db.insert_rows("_node_path", &["id", "path"], &path_by_id)?;
        }
        // Re-insert the fresh walk's child edges (the stale ones were deleted
        // above by the changed-path id set). One plural write; other files'
        // edges untouched (no whole-corpus child rebuild).
        if !child_rows.is_empty() {
            self.db.insert_rows(&child_tbl, &["parent", "child"], &child_rows)?;
        }
        Ok(nodes_changed)
    }

    /// The scanned file set for a node walk, keyed by (repo, path, rev, hash).
    /// `only` restricts to a changed-path subset (the delta refresh); `None`
    /// reads every `_file` row (the cold/full refresh).
    fn node_file_set(&self, only: Option<&HashSet<String>>) -> Result<Vec<(String, String, String, String)>> {
        let mut files: Vec<(String, String, String, String)> = Vec::new();
        let conn = self.db.conn();
        let mut sel = conn.prepare("SELECT repo, path, rev, hash FROM _file")?;
        let rows = sel.query_map([], |r| Ok((
            r.get::<_, String>(0)?, r.get::<_, String>(1)?,
            r.get::<_, String>(2)?, r.get::<_, String>(3)?)))?;
        for row in rows.flatten() {
            if let Some(set) = only { if !set.contains(&row.1) { continue; } }
            files.push(row);
        }
        Ok(files)
    }

    /// Per-file parse + tree-sitter walk in parallel (no DB touch). Each yields
    /// the file's node records plus the repo id + path + FileId its spans key off.
    fn node_walk(&self, files: &[(String, String, String, String)]) -> Vec<FileNodes> {
        let root = self.root.clone();
        let roots = self.repo_roots();
        files.par_iter().filter_map(|(repo, path, rev, hash)| {
            let label = crate::cst::lang_label_for_path(path)?;
            let lang = ts_lang(label).ok()?;
            let froot = roots.get(repo).map(|p| p.as_path()).unwrap_or(&root);
            let content = read_content(froot, rev, path).unwrap_or_default();
            let file = spine::FileId::from_content_address(hash, content.len() as i64)
                .filter(|f| *f != spine::FileId::SYNTHETIC)?;
            let nodes = crate::cst::walk_cst(&content, &lang).ok()?;
            if nodes.is_empty() { return None; }
            let rid = repo_id_of(froot, path, repo);
            Some(FileNodes { repo: rid, path: path.clone(), file, content, nodes })
        }).collect()
    }

    /// Build the node/child rel rows + the spine (`_strings`/`_where_bytes`)
    /// interns from a parsed walk. Collect-then-flush; no DB touch.
    fn node_rows_from_walk(&self, parsed: &[FileNodes])
        -> (Vec<Vec<Value>>, Vec<Vec<Value>>, Vec<Vec<Value>>, BTreeMap<String, (String, String)>, BTreeMap<String, Vec<Value>>)
    {
        let mut node_rows: Vec<Vec<Value>> = Vec::new();
        let mut child_rows: Vec<Vec<Value>> = Vec::new();
        // (node id, path) attribution rows for the `_node_path` side table.
        let mut path_by_id: Vec<Vec<Value>> = Vec::new();
        let mut str_by_id: BTreeMap<String, (String, String)> = BTreeMap::new();
        let mut wb_by_id: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for fln in parsed {
            let FileNodes { repo, path, file, content, nodes } = fln;
            // Pre-compute each node's salted id (an index-aligned Vec) so the
            // child edges reference the parent's id without recomputing.
            let mut ids: Vec<String> = Vec::with_capacity(nodes.len());
            for n in nodes {
                let slice = content.get(n.lo..n.hi).unwrap_or("");
                let raw_sid = spine::StringId::of(slice);
                let raw_wb = spine::WhereBytes { string: raw_sid, file: *file, lo: n.lo as u32, hi: n.hi as u32, ..Default::default() };
                let base = spine::WhereBytesId::of_located(raw_wb, repo, path);
                let node_id = base.salted(&n.kind).to_string();
                ids.push(node_id.clone());
                // Spine rows: the `_where_bytes` row uses the SALTED id but the
                // RAW StringId, so ref(node_id) -> string(raw_sid) = raw slice.
                if !slice.is_empty() {
                    str_by_id.entry(raw_sid.to_string())
                        .or_insert_with(|| (slice.to_string(), spine::normalize(slice)));
                    wb_by_id.entry(node_id.clone()).or_insert_with(|| vec![
                        Value::Text(node_id.clone()),
                        Value::Text(raw_sid.to_string()),
                        Value::Text(file.to_string()),
                        Value::Int(n.lo as i64),
                        Value::Int(n.hi as i64),
                        Value::Text(repo.clone()),
                        Value::Text(spine::RevId::default().to_string()),
                        Value::Text(path.clone()),
                    ]);
                }
            }
            for (ix, n) in nodes.iter().enumerate() {
                let parent_id = n.parent_ix.map(|p| ids[p].clone()).unwrap_or_default();
                node_rows.push(vec![
                    Value::Text(ids[ix].clone()),
                    Value::Text(n.kind.clone()),
                    Value::Text(file.to_string()),
                    Value::Int(n.lo as i64),
                    Value::Int(n.hi as i64),
                    Value::Text(parent_id),
                ]);
                path_by_id.push(vec![Value::Text(ids[ix].clone()), Value::Text(path.clone())]);
                if let Some(p) = n.parent_ix {
                    child_rows.push(vec![Value::Text(ids[p].clone()), Value::Text(ids[ix].clone())]);
                }
            }
        }
        (node_rows, child_rows, path_by_id, str_by_id, wb_by_id)
    }

    /// Flush the node walk's spine interns: `_strings` then `_where_bytes`
    /// (both INSERT OR IGNORE, content-addressed), so a node id resolves through
    /// `ref`/`string`. One plural write each.
    fn flush_node_spine(
        &self,
        str_by_id: BTreeMap<String, (String, String)>,
        wb_by_id: BTreeMap<String, Vec<Value>>,
    ) -> Result<()> {
        if !str_by_id.is_empty() {
            let string_rows: Vec<Vec<Value>> = str_by_id.into_iter()
                .map(|(id, (content, norm))| vec![Value::Text(id), Value::Text(content), Value::Text(norm)])
                .collect();
            self.db.insert_rows("_strings", &["id", "content", "norm"], &string_rows)?;
        }
        if !wb_by_id.is_empty() {
            let wb_rows: Vec<Vec<Value>> = wb_by_id.into_values().collect();
            self.db.insert_rows("_where_bytes", &["id", "string_id", "file_id", "lo", "hi", "repo", "rev", "path"], &wb_rows)?;
        }
        Ok(())
    }

    /// Wholesale repopulation of the Phase D call-graph relations. Same shape
    /// as `refresh_type_rels`: parallel per-file extraction via the language
    /// registry, one write per relation. Extractors return empty `CallFacts`
    /// today (the trait default), so this wires the lazy-indexer plumbing end
    /// to end with zero rows; per-language extractor bodies fill it in next.
    /// The caller-resolution second pass (span containment + bare-name resolve,
    /// the type_link path) lands with the first real extractor body; the row
    /// vecs already flow through it so the write path is exercised now.
    fn refresh_call_rels(&self) -> Result<()> {
        let mut files: Vec<(String, String, String)> = Vec::new();
        {
            let mut sel = self.db.conn().prepare(
                "SELECT repo, path, rev FROM _file WHERE path LIKE '%.rs' OR path LIKE '%.kt' OR path LIKE '%.kts' \
                 OR path LIKE '%.ts' OR path LIKE '%.tsx'")?;
            let rows = sel.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
            for row in rows.flatten() { files.push(row); }
        }

        let root = self.root.clone();
        let roots = self.repo_roots();
        let facts: Vec<(String, String, String, typegraph::CallFacts)> = files.par_iter().filter_map(|(repo, path, rev)| {
            let lang = typegraph::type_langs().iter().find(|l| l.matches(path))?;
            let froot = roots.get(repo).map(|p| p.as_path()).unwrap_or(&root);
            let content = read_content(froot, rev, path).unwrap_or_default();
            let rid = repo_id_of(froot, path, repo);
            Some((rid, path.clone(), rev.clone(), lang.extract_calls(path, &content)))
        }).collect();

        // Corpus-global def index: a barrier before any edge is emitted, same
        // shape as refresh_type_rels. by_name resolves a bare callee to a def
        // sym when exactly one callable declares it; sym_at backs the SCIP
        // override; def_by_file drives span-containment caller resolution
        // (innermost enclosing def wins, so calls inside a nested block attach
        // to the nearest fn, not the outermost).
        // Repo-scoped, same as refresh_type_rels: a callee resolves within the
        // referencing file's repo, and resolved syms are repo-qualified so the
        // sym-keyed call rels (call_edge/call_name) stay per-repo distinct.
        let mut by_name: HashMap<(&str, &str), Vec<&str>> = HashMap::new();
        let mut sym_at: HashMap<(&str, &str, &str), &str> = HashMap::new();
        let mut def_by_file: HashMap<(&str, &str), Vec<(u32, u32, &str)>> = HashMap::new();
        for (repo, _, _, f) in &facts {
            for d in &f.defs {
                by_name.entry((repo.as_str(), d.name.as_str())).or_default().push(d.sym.as_str());
                sym_at.insert((repo.as_str(), d.file.as_str(), d.name.as_str()), d.sym.as_str());
                def_by_file.entry((repo.as_str(), d.file.as_str())).or_default().push((d.line, d.end, d.sym.as_str()));
            }
        }
        let scip = self.scip_name_defs().unwrap_or_default();
        let resolve_callee = |repo: &str, file: &str, callee: &str| -> Option<String> {
            if let Some(def_file) = scip.get(&(file.to_string(), callee.to_string())) {
                if let Some(sym) = sym_at.get(&(repo, def_file.as_str(), callee)) {
                    return Some(format!("{repo}::{sym}"));
                }
            }
            match by_name.get(&(repo, callee)) {
                Some(v) if v.len() == 1 => Some(format!("{repo}::{}", v[0])),
                _ => None,
            }
        };
        let resolve_caller = |repo: &str, file: &str, line: u32| -> Option<String> {
            let mut best: Option<(u32, &str)> = None; // (span, sym); smallest containing span wins
            for &(s, e, sym) in def_by_file.get(&(repo, file)).into_iter().flatten() {
                if line >= s && line <= e {
                    let span = e - s;
                    match best {
                        Some((bs, _)) if span >= bs => {}
                        _ => best = Some((span, sym)),
                    }
                }
            }
            best.map(|(_, s)| format!("{repo}::{s}"))
        };

        let t = |s: &str| Value::Text(s.to_string());
        let i = |n: u32| Value::Int(n as i64);
        let mut def_rows: Vec<Vec<Value>> = Vec::new();
        let mut site_rows: Vec<Vec<Value>> = Vec::new();
        let mut edge_rev_rows: Vec<Vec<Value>> = Vec::new();
        let mut name_rows: Vec<Vec<Value>> = Vec::new();
        // call_kind is keyed by (caller, kind); a fn with both a read and a
        // write emits two rows. Accumulate in a set so multiple write sites in
        // the same fn collapse to one (fn, "write") row.
        let mut kind_set: HashSet<(String, &'static str)> = HashSet::new();
        let mut seen_def: HashSet<(&str, &str)> = HashSet::new();
        let mut seen_edge: HashSet<(String, String, &str)> = HashSet::new();
        for (repo, _path, rev, f) in &facts {
            for d in &f.defs {
                if seen_def.insert((repo.as_str(), d.sym.as_str())) {
                    let qsym = format!("{repo}::{}", d.sym);
                    def_rows.push(vec![t(repo), t(&qsym), t(d.kind.tag()), t(&d.file), i(d.line), i(d.end)]);
                    name_rows.push(vec![t(&qsym), t(&d.name)]);
                }
            }
            for s in &f.sites {
                // call_site is the raw graph: every site, caller resolved when a
                // def encloses it (repo-qualified), callee as written.
                let caller = resolve_caller(repo, &s.file, s.line).unwrap_or_default();
                site_rows.push(vec![t(repo), t(&caller), t(&s.callee), t(&s.file), i(s.line)]);
                // call_kind: classify the callee's bare name as read/write. The
                // fn-aggregate is the precision axis the conn-loop-reachable
                // rail needs (a fn that only reads through its .conn() does not
                // fire). Heuristic by name: $R.execute(...) is a write on any
                // receiver; the rail's conn_fn join narrows to db-shaped sites.
                if !caller.is_empty() {
                    if let Some(k) = classify_call_kind(&s.callee) {
                        kind_set.insert((caller.clone(), k));
                    }
                }
                // call_edge is the resolved graph: emit only when both endpoints
                // resolve to def syms, so closure(call_edge) walks one identity
                // space (same contract as type_link). Unresolved calls stay in
                // call_site with their bare callee.
                if let Some(callee_sym) = resolve_callee(repo, &s.file, &s.callee) {
                    if !caller.is_empty() && seen_edge.insert((caller.clone(), callee_sym.clone(), rev)) {
                        edge_rev_rows.push(vec![t(&caller), t(&callee_sym), t("call"), t(rev)]);
                    }
                }
            }
        }

        let mut kind_pairs: Vec<(String, &'static str)> = kind_set.into_iter().collect();
        kind_pairs.sort();
        let kind_rows: Vec<Vec<Value>> = kind_pairs
            .into_iter()
            .map(|(f, k)| vec![t(&f), t(k)])
            .collect();

        self.refresh_rel("call_def", &["repo", "sym", "kind", "file", "line", "end"], &def_rows)?;
        self.refresh_rel("call_site", &["repo", "caller", "callee", "file", "line"], &site_rows)?;
        self.refresh_rel("call_edge_rev", &["caller", "callee", "kind", "rev"], &edge_rev_rows)?;
        self.refresh_rel("call_name", &["sym", "name"], &name_rows)?;
        self.refresh_rel("call_kind", &["fn", "kind"], &kind_rows)?;
        self.rebuild_legacy_call_rels()?;
        Ok(())
    }

    /// Rebuild the convenient rev-less `call_edge(caller, callee, kind)` from
    /// the rev-aware table, deduped across revs. Same shape as
    /// `rebuild_legacy_type_rels`: `call_edge_rev` is the source of truth, the
    /// legacy view is the simple closure target.
    fn rebuild_legacy_call_rels(&self) -> Result<()> {
        let edge = tbl("call_edge");
        let edge_rev = tbl("call_edge_rev");
        self.db.exec(&format!("DELETE FROM {edge}"))?;
        self.db.exec(&format!(
            "INSERT OR IGNORE INTO {edge} (\"caller\", \"callee\", \"kind\") \
             SELECT \"caller\", \"callee\", \"kind\" FROM {edge_rev}"
        ))?;
        Ok(())
    }

    /// Intra-procedural dataflow lift over the corpus. Each `.rs/.kt/.kts/.ts/.tsx`
    /// file in `_file` is parsed once by the matching front-end's
    /// `extract_dataflow`; nodes and edges are corpus-deduped by id (the
    /// `file:line:col` start span is already unique across files). No resolution
    /// pass is needed — node ids and the enclosing `fn` sym are self-contained,
    /// so this is a straight extract + bulk write.
    fn refresh_dataflow_rels(&self) -> Result<()> {
        let mut files: Vec<(String, String)> = Vec::new();
        {
            let mut sel = self.db.conn().prepare(
                "SELECT path, rev FROM _file WHERE path LIKE '%.rs' OR path LIKE '%.kt' OR path LIKE '%.kts' \
                 OR path LIKE '%.ts' OR path LIKE '%.tsx'")?;
            let rows = sel.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            for row in rows.flatten() { files.push(row); }
        }

        let root = self.root.clone();
        let facts: Vec<typegraph::DataflowFacts> = files.par_iter().filter_map(|(path, rev)| {
            let lang = typegraph::type_langs().iter().find(|l| l.matches(path))?;
            let content = read_content(&root, rev, path).unwrap_or_default();
            Some(lang.extract_dataflow(path, &content))
        }).collect();

        let t = |s: &str| Value::Text(s.to_string());
        let i = |n: u32| Value::Int(n as i64);
        let mut node_rows: Vec<Vec<Value>> = Vec::new();
        let mut edge_rows: Vec<Vec<Value>> = Vec::new();
        let mut loop_rows: Vec<Vec<Value>> = Vec::new();
        let mut alloc_rows: Vec<Vec<Value>> = Vec::new();
        let mut nest_rows: Vec<Vec<Value>> = Vec::new();
        let mut param_rows: Vec<Vec<Value>> = Vec::new();
        let mut seen_param: HashSet<&str> = HashSet::new();
        let mut seen_node: HashSet<&str> = HashSet::new();
        let mut seen_edge: HashSet<(&str, &str)> = HashSet::new();
        let mut seen_loop: HashSet<(&str, u32)> = HashSet::new();
        let mut seen_nest: HashSet<(&str, &str)> = HashSet::new();
        for f in &facts {
            for n in &f.nodes {
                if seen_node.insert(n.id.as_str()) {
                    node_rows.push(vec![t(&n.id), t(&n.kind), t(&n.var), t(&n.fn_sym), t(&n.file), i(n.line)]);
                }
            }
            for e in &f.edges {
                if seen_edge.insert((e.from.as_str(), e.to.as_str())) {
                    edge_rows.push(vec![t(&e.from), t(&e.to)]);
                }
            }
            for l in &f.loops {
                if seen_loop.insert((l.file.as_str(), l.start)) {
                    loop_rows.push(vec![t(&l.file), i(l.start), i(l.end), t(&l.var), t(&l.collection), t(&l.fn_sym)]);
                }
            }
            for fn_sym in &f.allocators {
                alloc_rows.push(vec![t(fn_sym)]);
            }
            for ns in &f.nests {
                if seen_nest.insert((ns.call_id.as_str(), ns.loop_id.as_str())) {
                    nest_rows.push(vec![t(&ns.call_id), t(&ns.loop_id), i(ns.depth), t(&ns.collection)]);
                }
            }
            for (id, pos) in &f.param_pos {
                if seen_param.insert(id.as_str()) {
                    param_rows.push(vec![t(id), i(*pos)]);
                }
            }
        }

        self.refresh_rel("df_node", &["id", "kind", "var", "fn", "file", "line"], &node_rows)?;
        self.refresh_rel("df_edge", &["from", "to"], &edge_rows)?;
        self.refresh_rel("loop_over", &["file", "start", "end", "var", "collection", "fn"], &loop_rows)?;
        self.refresh_rel("allocates", &["fn"], &alloc_rows)?;
        self.refresh_rel("nest", &["call_id", "loop_id", "depth", "collection"], &nest_rows)?;
        self.refresh_rel("df_param", &["id", "pos"], &param_rows)?;
        Ok(())
    }

    /// Refresh `doc_node` from the document-grammar registry. Same shape as the
    /// source-lang refreshers but simpler: no corpus-global resolution pass, no
    /// legacy mirror -- each DocNode is self-contained. Files come from `_file`
    /// (a source rule scanning `**/*.md` feeds it, exactly as `**/*.rs` feeds
    /// the type graph); the SQL prefilter narrows to document extensions, then
    /// the registry's `matches` decides the real dispatch.
    fn refresh_doc_rels(&self) -> Result<()> {
        let mut files: Vec<(String, String, String)> = Vec::new();
        {
            let mut sel = self.db.conn().prepare(
                "SELECT repo, path, rev FROM _file WHERE path LIKE '%.md' OR path LIKE '%.markdown'")?;
            let rows = sel.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
            for row in rows.flatten() { files.push(row); }
        }
        let root = self.root.clone();
        let roots = self.repo_roots();
        let facts: Vec<(String, String, ingest::DocFacts)> = files.par_iter().filter_map(|(repo, path, rev)| {
            let lang = ingest::ingest_langs().iter().find(|l| l.matches(path))?;
            let froot = roots.get(repo).map(|p| p.as_path()).unwrap_or(&root);
            let content = read_content(froot, rev, path).unwrap_or_default();
            let rid = repo_id_of(froot, path, repo);
            Some((rid, path.clone(), lang.extract_docs(path, &content)))
        }).collect();
        let t = |s: &str| Value::Text(s.to_string());
        let i = |n: u32| Value::Int(n as i64);
        let mut rows: Vec<Vec<Value>> = Vec::new();
        let mut seen: HashSet<(String, String, u32, &str, String)> = HashSet::new();
        for (repo, path, f) in &facts {
            for n in &f.nodes {
                if seen.insert((repo.clone(), path.clone(), n.line, n.kind, n.name.clone())) {
                    rows.push(vec![t(repo), t(path), i(n.line), t(n.kind), t(&n.name), t(&n.parent)]);
                }
            }
        }
        self.refresh_rel("doc_node", &["repo", "file", "line", "kind", "name", "parent"], &rows)?;

        // doc_ref bridge: doc_node -> type_entity, three sources.
        //   (1) heading exact:    doc_node.name == type_entity.name
        //   (2) heading norm:     normalize(doc_node.name) == type_entity.name
        //       (strips leading articles + trailing kind words; lowercase).
        //   (3) code_block text:  identifiers in doc_node.text matched against
        //       type_entity.name (lowercase). The whole block counts as one
        //       doc position (the fence line); dedup collapses repeats.
        //
        // Normalization lives in `normalize_doc_name` (below). type_entity names
        // are already clean identifiers, so the symbol side only lowercases.
        // Empty if the program doesn't use type relations (type_entity table
        // exists but is unpopulated -> empty map -> no rows).
        let type_rows: Vec<(String, String)> = {
            let mut sel = self.db.prepare(
                &format!("SELECT sym, name FROM {}", tbl("type_entity")))?;
            let rows: Vec<(String, String)> = sel
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .filter_map(|x| x.ok()).collect();
            rows
        };
        // Map lowercase type name -> Vec of (sym, original_name) so multiple
        // symbols of the same name all bridge (e.g. two `Engine` in different
        // files).
        let mut by_name: HashMap<String, Vec<(String, String)>> = HashMap::new();
        for (sym, name) in &type_rows {
            by_name.entry(name.to_ascii_lowercase())
                .or_default()
                .push((sym.clone(), name.clone()));
        }
        let mut ref_rows: Vec<Vec<Value>> = Vec::new();
        let mut ref_seen: HashSet<(String, u32, String, &'static str, String)> = HashSet::new();
        let push_ref = |repo: &str, file: &str, line: u32, sym: &str, kind: &'static str,
                        matched: &str, rows: &mut Vec<Vec<Value>>,
                        seen: &mut HashSet<(String, u32, String, &'static str, String)>| {
            if seen.insert((file.to_string(), line, sym.to_string(), kind, matched.to_string())) {
                rows.push(vec![t(repo), t(file), i(line), t(sym), t(kind), t(matched)]);
            }
        };
        for (repo, path, f) in &facts {
            for n in &f.nodes {
                match n.kind {
                    "heading" => {
                        // Try exact name first, then normalized. The original
                        // heading text is recorded as matched_name so a rule
                        // can cross-reference doc_node directly.
                        let exact = by_name.get(&n.name.to_ascii_lowercase());
                        if let Some(hits) = exact {
                            for (sym, _orig) in hits {
                                push_ref(repo, path, n.line, sym, "heading", &n.name,
                                         &mut ref_rows, &mut ref_seen);
                            }
                            continue;
                        }
                        let norm = normalize_doc_name(&n.name);
                        if !norm.is_empty() {
                            if let Some(hits) = by_name.get(&norm) {
                                for (sym, _orig) in hits {
                                    push_ref(repo, path, n.line, sym, "heading", &n.name,
                                             &mut ref_rows, &mut ref_seen);
                                }
                            }
                        }
                    }
                    "code_block" => {
                        // Scan the block body for identifiers that match a type
                        // name. Each unique (sym, token) pair emits one row.
                        for tok in identifiers_in(&n.text) {
                            if let Some(hits) = by_name.get(&tok.to_ascii_lowercase()) {
                                for (sym, _orig) in hits {
                                    push_ref(repo, path, n.line, sym, "code_block", tok,
                                             &mut ref_rows, &mut ref_seen);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        self.refresh_rel("doc_ref",
            &["repo", "file", "line", "sym", "kind", "matched_name"], &ref_rows)?;
        Ok(())
    }

    /// Import compiler-backed SCIP facts from `SPREFA_SCIP_INDEX` or
    /// `<root>/index.scip`. Missing index means empty relations, so programs can
    /// mention SCIP facts without making rust-analyzer a hard runtime dependency.
    fn refresh_scip_rels(&self) -> Result<bool> {
        let t = |s: &str| Value::Text(s.to_string());
        let Some(path) = scip_import::index_path(&self.root) else {
            self.refresh_rel("scip_def", &["symbol", "file"], &[])?;
            self.refresh_rel("scip_name", &["symbol", "name"], &[])?;
            self.refresh_rel("scip_ref", &["file", "symbol", "def_file"], &[])?;
            self.refresh_rel("scip_edge", &["src", "dst"], &[])?;
            self.refresh_rel("scip_fn_edge", &["caller", "callee"], &[])?;
            self.refresh_rel("scip_callee_type", &["sym", "type"], &[])?;
            self.refresh_rel("scip_local", &["fn", "name"], &[])?;
            self.refresh_rel("scip_impl", &["impl", "iface"], &[])?;
            return Ok(true);
        };
        let rows = scip_import::load(&path)?;
        let defs: Vec<Vec<Value>> = rows.defs.iter().map(|(sym, file)| vec![t(sym), t(file)]).collect();
        // The symbol's descriptor name (last identifier run), computed where the
        // SCIP moniker grammar lives. A pure-dl `split` chain can't isolate it:
        // `…/impl#[Type]method().` needs the `[`/`]`/`#` separators that single-
        // separator split can't all honor. One row per distinct (symbol, name).
        let mut name_set: HashSet<(String, String)> = HashSet::new();
        for (sym, _) in &rows.defs {
            if let Some(name) = scip_descriptor_name(sym) {
                name_set.insert((sym.clone(), name));
            }
        }
        let names: Vec<Vec<Value>> = name_set.iter().map(|(sym, name)| vec![t(sym), t(name)]).collect();
        let refs: Vec<Vec<Value>> = rows.refs.iter()
            .map(|(file, sym, def)| vec![t(file), t(sym), t(def)]).collect();
        let edges: Vec<Vec<Value>> = rows.edges.iter().map(|(src, dst)| vec![t(src), t(dst)]).collect();
        let fn_edges: Vec<Vec<Value>> = rows.fn_edges.iter()
            .map(|(caller, callee)| vec![t(caller), t(callee)]).collect();
        let callee_types: Vec<Vec<Value>> = rows.callee_types.iter()
            .map(|(sym, ty)| vec![t(sym), t(ty)]).collect();
        let locals: Vec<Vec<Value>> = rows.locals.iter()
            .map(|(fn_, name)| vec![t(fn_), t(name)]).collect();
        let impls: Vec<Vec<Value>> = rows.impls.iter()
            .map(|(im, iface)| vec![t(im), t(iface)]).collect();
        self.refresh_rel("scip_def", &["symbol", "file"], &defs)?;
        self.refresh_rel("scip_name", &["symbol", "name"], &names)?;
        self.refresh_rel("scip_ref", &["file", "symbol", "def_file"], &refs)?;
        self.refresh_rel("scip_edge", &["src", "dst"], &edges)?;
        self.refresh_rel("scip_fn_edge", &["caller", "callee"], &fn_edges)?;
        self.refresh_rel("scip_callee_type", &["sym", "type"], &callee_types)?;
        self.refresh_rel("scip_local", &["fn", "name"], &locals)?;
        self.refresh_rel("scip_impl", &["impl", "iface"], &impls)?;
        Ok(true)
    }

    /// Refresh the `changed` relation from `git status --porcelain -uall` at the
    /// self repo's root: modified, added, renamed (new side), and untracked
    /// paths, re-anchored from the git toplevel to `--root` (paths outside the
    /// root are dropped). A non-repo root or git failure yields an empty
    /// relation, not an error. One subprocess, one batched write. Returns
    /// whether the row set differs from what was stored, so an incremental tick
    /// only propagates a rebuild when the diff actually moved.
    fn refresh_changed_rel(&self) -> Result<bool> {
        let mut paths: Vec<String> = Vec::new();
        let top = Command::new("git").arg("-C").arg(&self.root)
            .args(["rev-parse", "--show-toplevel"]).output();
        if let Ok(out) = top {
            if out.status.success() {
                let toplevel = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim().to_string());
                let status = Command::new("git").arg("-C").arg(&self.root)
                    .args(["status", "--porcelain", "-uall"]).output()?;
                if status.status.success() {
                    // canonicalize for prefix-stripping: git prints the physical
                    // toplevel (macOS /private/var) while --root may be the symlink
                    let root = std::fs::canonicalize(&self.root)
                        .unwrap_or_else(|_| self.root.clone());
                    for line in String::from_utf8_lossy(&status.stdout).lines() {
                        if line.len() < 4 { continue; }
                        let entry = &line[3..];
                        // a rename prints "old -> new"; the worktree file is the new side
                        let p = entry.rsplit(" -> ").next().unwrap_or(entry).trim_matches('"');
                        if let Ok(rel) = toplevel.join(p).strip_prefix(&root) {
                            paths.push(rel.to_string_lossy().replace('\\', "/"));
                        }
                    }
                }
            }
        }
        paths.sort();
        paths.dedup();
        let existing: Vec<String> = {
            let conn = self.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"path\" FROM {} ORDER BY \"path\"", tbl("changed")))?;
            let rows = s.query_map([], |r| r.get::<_, String>(0))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        if existing == paths { return Ok(false); }
        let rows: Vec<Vec<Value>> = paths.into_iter().map(|p| vec![Value::Text(p)]).collect();
        self.refresh_rel("changed", &["path"], &rows)?;
        Ok(true)
    }

    /// Refresh `created(path, name, email, ts)` — the author of the commit that
    /// added each tracked file. One `git log --reverse --diff-filter=A
    /// --name-only` walk: `--reverse` puts the oldest commit first, so the first
    /// time a path appears is its creation; later re-adds (delete-then-add) are
    /// ignored. The custom `--format` line carries the author fields delimited by
    /// control bytes; name-only file lines follow until the next format line.
    /// Whole-set recompute, compared to stored, early-out on no-op (history is
    /// append-only, so this is almost always a no-op after the first tick).
    fn refresh_created_rel(&self) -> Result<bool> {
        let mut created: Vec<(String, String, String, i64)> = Vec::new(); // path, name, email, ts
        let top = Command::new("git").arg("-C").arg(&self.root)
            .args(["rev-parse", "--show-toplevel"]).output();
        if let Ok(out) = top {
            if out.status.success() {
                let toplevel = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim().to_string());
                // \x01 prefixes the per-commit author line; \x1f separates fields.
                let log = Command::new("git").arg("-C").arg(&self.root)
                    .args(["log", "--reverse", "--diff-filter=A", "--name-only",
                           "--format=\x01%an\x1f%ae\x1f%at"]).output()?;
                if log.status.success() {
                    let root = std::fs::canonicalize(&self.root)
                        .unwrap_or_else(|_| self.root.clone());
                    let mut seen: HashSet<String> = HashSet::new();
                    let (mut name, mut email, mut ts) = (String::new(), String::new(), 0i64);
                    for line in String::from_utf8_lossy(&log.stdout).lines() {
                        if let Some(hdr) = line.strip_prefix('\x01') {
                            let mut it = hdr.split('\x1f');
                            name = it.next().unwrap_or("").to_string();
                            email = it.next().unwrap_or("").to_string();
                            ts = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                        } else if !line.is_empty() {
                            let p = line.trim_matches('"');
                            if let Ok(rel) = toplevel.join(p).strip_prefix(&root) {
                                let rel = rel.to_string_lossy().replace('\\', "/");
                                if seen.insert(rel.clone()) {
                                    created.push((rel, name.clone(), email.clone(), ts));
                                }
                            }
                        }
                    }
                }
            }
        }
        created.sort();
        let existing: Vec<(String, String, String, i64)> = {
            let conn = self.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"path\",\"name\",\"email\",\"ts\" FROM {} ORDER BY 1,2,3,4",
                tbl("created")))?;
            let rows = s.query_map([], |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?,
                r.get::<_, String>(2)?, r.get::<_, i64>(3)?)))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        if existing == created { return Ok(false); }
        let rows: Vec<Vec<Value>> = created.into_iter()
            .map(|(p, n, e, t)| vec![Value::Text(p), Value::Text(n), Value::Text(e), Value::Int(t)])
            .collect();
        self.refresh_rel("created", &["path", "name", "email", "ts"], &rows)?;
        Ok(true)
    }

    /// Encode every interned `_strings` row lacking a vector for the active
    /// backend (embed-once per (StringId, backend)), then materialize `similar`.
    /// Returns true if the `similar` row set could have changed (new content was
    /// embedded, or the table was empty), false on the steady-state no-op so the
    /// derived rebuild stays scoped. Brute-force O(n^2) cosine; the embedded set
    /// is capped by SPREFA_EMBED_MAX to keep a `similar` reference from hanging
    /// on a large corpus (the sqlite-vec ANN path is the scale follow-on).
    fn refresh_embed_rels(&self) -> Result<bool> {
        let embedder = crate::embed::make(None)?;
        let backend = embedder.name().to_string();
        let max: usize = std::env::var("SPREFA_EMBED_MAX").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(4096);

        // Content with no vector for THIS backend. Capped: only the first `max`
        // un-embedded strings are encoded per tick (the rest catch up next tick).
        let to_embed: Vec<(String, String)> = {
            let conn = self.db.conn();
            let mut s = conn.prepare(
                "SELECT s.id, s.content FROM _strings s
                 WHERE s.id != '0'
                   AND NOT EXISTS (SELECT 1 FROM _embeddings e
                                   WHERE e.sid = s.id AND e.backend = ?1)
                 LIMIT ?2")?;
            let v: Vec<(String, String)> = s.query_map(rusqlite::params![backend, max as i64], |r|
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .filter_map(|x| x.ok()).collect();
            v
        };
        if !to_embed.is_empty() {
            let texts: Vec<&str> = to_embed.iter().map(|(_, c)| c.as_str()).collect();
            let vecs = embedder.encode(&texts)?;
            let dim = embedder.dim() as i64;
            // collect-then-flush: one insert_rows, never per-row (the spine rule).
            let mut rows: Vec<Vec<Value>> = Vec::with_capacity(vecs.len());
            for ((sid, _), mut v) in to_embed.iter().cloned().zip(vecs) {
                crate::embed::l2_normalize(&mut v);
                rows.push(vec![
                    Value::Text(sid), Value::Text(backend.clone()),
                    Value::Int(dim), Value::Text(crate::embed::encode_vec(&v))]);
            }
            self.db.insert_rows("_embeddings", &["sid", "backend", "dim", "vec"], &rows)?;
        }

        // Steady state: no new content AND `similar` already built -> no recompute.
        let similar_rows: i64 = self.db.conn().query_row(
            &format!("SELECT count(*) FROM {}", tbl("similar")), [], |r| r.get(0))?;
        if to_embed.is_empty() && similar_rows > 0 { return Ok(false); }

        self.refresh_similar_rel(&backend, max)?;
        Ok(true)
    }

    /// Materialize `similar(a, b, score)`: top-k cosine neighbors of each
    /// embedded string for `backend`. Brute-force pairwise over the (capped)
    /// embedded pool; vectors are L2-normalized at store time so cosine is a dot
    /// product. `score` = round(cosine * 1e6) as Int.
    fn refresh_similar_rel(&self, backend: &str, max: usize) -> Result<()> {
        let k: usize = std::env::var("SPREFA_SIMILAR_K").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(8);
        let pool: Vec<(String, Vec<f32>)> = {
            let conn = self.db.conn();
            let mut s = conn.prepare("SELECT sid, vec FROM _embeddings WHERE backend = ?1 LIMIT ?2")?;
            let v: Vec<(String, Vec<f32>)> = s.query_map(rusqlite::params![backend, max as i64], |r|
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .filter_map(|x| x.ok())
                .map(|(sid, txt): (String, String)| (sid, crate::embed::parse_vec(&txt)))
                .collect();
            v
        };
        if pool.len() > 2000 {
            eprintln!("[similar] brute-force KNN over {} vectors (O(n^2)); \
                       cap with SPREFA_EMBED_MAX or wire sqlite-vec", pool.len());
        }
        let rows = knn_rows(&pool, k);
        self.refresh_rel("similar", &["a", "b", "score"], &rows)?;
        Ok(())
    }

    /// Materialize `head(a, b, score) <- node2vec(edge).`: read the 2-col edge
    /// rel, learn one structural vector per node (random walks + skip-gram), store
    /// the vectors in `_node_embeddings` keyed by the edge rel name, and fill the
    /// 3-col head with each node's top-k cosine-nearest neighbors. Excluded from
    /// `rebuild_derived` (the Node2vec body item can't lower to SQL); runs after
    /// the edge rel has materialized. Recomputes each tick (no incremental reuse
    /// yet — the embed is the cost; cap the graph or run on demand).
    fn eval_node2vec_rule(&self, rule: &Rule) -> Result<()> {
        let edge = rule.node2vec_edge()
            .ok_or_else(|| anyhow::anyhow!("eval_node2vec_rule on a non-node2vec rule"))?;
        let head = &rule.head.rel;
        let head_meta = self.rels.get(head)
            .ok_or_else(|| anyhow::anyhow!("unknown head relation {head}"))?;
        if rule.head.terms.len() != 3 || head_meta.cols.len() != 3 {
            bail!("node2vec head '{head}' must have exactly 3 columns (a, b, score); got {}",
                  head_meta.cols.len());
        }
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

        // Always clear the head + this graph's vectors so an emptied edge set
        // produces an empty head (steady-state correctness over speed).
        self.db.exec(&format!("DELETE FROM {}", tbl(head)))?;
        self.db.conn().execute("DELETE FROM _node_embeddings WHERE graph = ?1", [edge])?;
        if edges.is_empty() { return Ok(()); }

        let cfg = crate::embed::node2vec::N2vConfig::from_env();
        let pool = crate::embed::node2vec::embed_graph(&edges, &cfg);
        if pool.len() > 2000 {
            eprintln!("[node2vec] brute-force KNN over {} nodes (O(n^2)); \
                       shrink the edge rel or cap SPREFA_N2V_*", pool.len());
        }

        // Persist vectors (one flush, never N+1).
        let dim = cfg.dim as i64;
        let emb_rows: Vec<Vec<Value>> = pool.iter().map(|(node, v)| vec![
            Value::Text(node.clone()), Value::Text(edge.to_string()),
            Value::Int(dim), Value::Text(crate::embed::encode_vec(v))]).collect();
        self.db.insert_rows("_node_embeddings", &["node", "graph", "dim", "vec"], &emb_rows)?;

        // Fill the head with KNN pairs (reuses the text path's cosine top-k).
        let k: usize = std::env::var("SPREFA_NODE_SIM_K").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(8);
        let rows = knn_rows(&pool, k);
        let cols: Vec<&str> = head_meta.cols.iter().map(|c| c.name.as_str()).collect();
        self.db.insert_rows(&tbl(head), &cols, &rows)?;
        Ok(())
    }

    /// Refresh `agent_edit` / `agent_touch` from the at-rest harness stores
    /// (agent.rs). For each detected harness, read the newest session for this
    /// repo and emit one `agent_edit` row per file edit (repo-relative, tagged
    /// with harness + session + per-session turn idx); `agent_touch` is the rows
    /// at `max(idx)` per session — the latest turn, still tagged (no broadcast).
    /// Best-effort: a missing store yields no rows. Whole-set recompute compared
    /// to stored, early-out when unchanged so an incremental tick rebuilds only
    /// when the latest turn moved.
    fn refresh_agent_rels(&self) -> Result<bool> {
        let root = std::fs::canonicalize(&self.root).unwrap_or_else(|_| self.root.clone());
        let mut edits: Vec<(String, String, i64, String)> = Vec::new(); // harness, session, idx, path
        let mut touch: Vec<(String, String, String)> = Vec::new();      // harness, session, path @ max idx
        for h in crate::agent::agent_harnesses() {
            let hn = h.name().to_string();
            for sess in h.sessions_for(&root) {
                let maxidx = sess.edits.iter().map(|e| e.idx).max();
                for e in &sess.edits {
                    edits.push((hn.clone(), sess.id.clone(), e.idx, e.path.clone()));
                    if Some(e.idx) == maxidx {
                        touch.push((hn.clone(), sess.id.clone(), e.path.clone()));
                    }
                }
            }
        }
        edits.sort(); edits.dedup();
        touch.sort(); touch.dedup();
        // Early-out: compare agent_edit (the superset) against what is stored.
        let existing: Vec<(String, String, i64, String)> = {
            let conn = self.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"harness\", \"session\", \"idx\", \"path\" FROM {} ORDER BY 1,2,3,4",
                tbl("agent_edit")))?;
            let rows = s.query_map([], |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?)))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        if existing == edits { return Ok(false); }
        let edit_rows: Vec<Vec<Value>> = edits.into_iter()
            .map(|(h, s, i, p)| vec![Value::Text(h), Value::Text(s), Value::Int(i), Value::Text(p)])
            .collect();
        self.refresh_rel("agent_edit", &["harness", "session", "idx", "path"], &edit_rows)?;
        let touch_rows: Vec<Vec<Value>> = touch.into_iter()
            .map(|(h, s, p)| vec![Value::Text(h), Value::Text(s), Value::Text(p)])
            .collect();
        self.refresh_rel("agent_touch", &["harness", "session", "path"], &touch_rows)?;
        Ok(true)
    }

    /// Refresh `propose_extract(path, lo, hi, param)` — for every scanned Rust
    /// file, run the proposer (verbatim dup-block detection + lexical-scope
    /// free-var inference) and store one row per (block, param). Whole-corpus:
    /// recompute all, compare to stored, early-out if equal. Reuses `node_file_set`
    /// for the file list and `propose::extract_proposals` for the analysis.
    fn refresh_propose_extract_rel(&self) -> Result<bool> {
        let roots = self.repo_roots();
        let root = self.root.clone();
        let files = self.node_file_set(None)?;
        let mut computed: Vec<(String, i64, i64, String)> = Vec::new();
        for (repo, path, rev, _hash) in files {
            if crate::cst::lang_label_for_path(&path) != Some("rust") { continue; }
            let froot = roots.get(&repo).map(|p| p.as_path()).unwrap_or(&root);
            let content = read_content(froot, &rev, &path).unwrap_or_default();
            for prop in crate::propose::extract_proposals(&content) {
                for p in prop.params {
                    computed.push((path.clone(), prop.lo as i64, prop.hi as i64, p));
                }
            }
        }
        computed.sort(); computed.dedup();
        let stored: Vec<(String, i64, i64, String)> = {
            let conn = self.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"path\",\"lo\",\"hi\",\"param\" FROM {} ORDER BY \"path\",\"lo\",\"hi\",\"param\"",
                tbl("propose_extract")))?;
            let rows = s.query_map([], |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?, r.get::<_, String>(3)?)))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        if stored == computed { return Ok(false); }
        let rows: Vec<Vec<Value>> = computed.into_iter()
            .map(|(p, lo, hi, pm)| vec![Value::Text(p), Value::Int(lo), Value::Int(hi), Value::Text(pm)])
            .collect();
        self.refresh_rel("propose_extract", &["path", "lo", "hi", "param"], &rows)?;
        Ok(true)
    }

    /// Refresh `propose_clone(kernel, path, lo, hi, param)` — for every scanned
    /// Rust file, run all 9 clone-detection kernels and store one row per
    /// (kernel, block, param). Symbol and call-seq kernels need index.scip;
    /// they emit no rows if the index is absent.
    fn refresh_propose_clone_rel(&self) -> Result<bool> {
        let roots = self.repo_roots();
        let root = self.root.clone();
        let files = self.node_file_set(None)?;
        let scip_spans: HashMap<String, Vec<(i32, i32, String)>> =
            if let Some(idx) = scip_import::index_path(&root) {
                match scip_import::load(&idx) {
                    Ok(rows) => {
                        let mut map: HashMap<String, Vec<(i32, i32, String)>> = HashMap::new();
                        for (file, l, c, sym) in rows.occ_spans {
                            map.entry(file).or_default().push((l, c, sym));
                        }
                        map
                    }
                    Err(_) => HashMap::new(),
                }
            } else {
                HashMap::new()
            };
        let mut computed: Vec<(String, String, i64, i64, String)> = Vec::new();
        for (repo, path, rev, _hash) in files {
            if crate::cst::lang_label_for_path(&path) != Some("rust") {
                continue;
            }
            let froot = roots.get(&repo).map(|p| p.as_path()).unwrap_or(&root);
            let content = read_content(froot, &rev, &path).unwrap_or_default();
            let spans_owned = scip_spans.get(&path).cloned().unwrap_or_default();
            let spans: Vec<(i32, i32, &str)> = spans_owned
                .iter()
                .map(|(l, c, s)| (*l, *c, s.as_str()))
                .collect();
            let kernels: Vec<(&str, Vec<crate::propose::Proposal>)> = vec![
                ("verbatim", crate::propose::extract_proposals(&content)),
                ("ast", crate::propose::ast_shape_proposals(&content)),
                ("tree", crate::propose::tree_shape_proposals(&content)),
                ("cfg", crate::propose::cfg_shape_proposals(&content)),
                ("ddg", crate::propose::ddg_shape_proposals(&content)),
                ("cgraph", crate::propose::callgraph_shape_proposals(&content)),
                ("ngram", crate::propose::ngram_stat_proposals(&content)),
                ("symbol", crate::propose::symbol_shape_proposals(&content, &spans)),
                ("call", crate::propose::call_seq_proposals(&content, &spans)),
            ];
            for (kname, props) in kernels {
                for prop in props {
                    for p in &prop.params {
                        computed.push((
                            kname.to_string(),
                            path.clone(),
                            prop.lo as i64,
                            prop.hi as i64,
                            p.clone(),
                        ));
                    }
                }
            }
        }
        computed.sort();
        computed.dedup();
        let stored: Vec<(String, String, i64, i64, String)> = {
            let conn = self.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"kernel\",\"path\",\"lo\",\"hi\",\"param\" FROM {} ORDER BY \"kernel\",\"path\",\"lo\",\"hi\",\"param\"",
                tbl("propose_clone")
            ))?;
            let rows = s.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })?;
            rows.filter_map(|x| x.ok()).collect()
        };
        if stored == computed {
            return Ok(false);
        }
        let rows: Vec<Vec<Value>> = computed
            .into_iter()
            .map(|(k, p, lo, hi, pm)| {
                vec![
                    Value::Text(k),
                    Value::Text(p),
                    Value::Int(lo),
                    Value::Int(hi),
                    Value::Text(pm),
                ]
            })
            .collect();
        self.refresh_rel("propose_clone", &["kernel", "path", "lo", "hi", "param"], &rows)?;
        Ok(true)
    }

    /// Rebuild `type_shape(name, hash)` from the current `type_edge` rows.
    /// Reads the edge set out of the DB, hands it to `typegraph::type_shape_hashes`
    /// (fixpoint Merkle), and writes one row per name. Returns false if the
    /// computed set matches what's already stored.
    fn refresh_type_shape_rel(&self) -> Result<bool> {
        // Read the full edge set: (from, to, kind).
        let edges: Vec<(String, String, String)> = {
            let conn = self.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"from\",\"to\",\"kind\" FROM {}", tbl("type_edge")
            ))?;
            let rows = s.query_map([], |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            )))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        let computed: Vec<(String, String)> = typegraph::type_shape_hashes(&edges);
        let stored: Vec<(String, String)> = {
            let conn = self.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"name\",\"hash\" FROM {} ORDER BY \"name\",\"hash\"",
                tbl("type_shape")
            ))?;
            let rows = s.query_map([], |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
            )))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        if stored == computed {
            return Ok(false);
        }
        let rows: Vec<Vec<Value>> = computed
            .into_iter()
            .map(|(n, h)| vec![Value::Text(n), Value::Text(h)])
            .collect();
        self.refresh_rel("type_shape", &["name", "hash"], &rows)?;
        Ok(true)
    }

    /// Rebuild `type_lgg(a, b, vars)` from the resolved type graph.
    ///
    /// Uses `type_link` (SCIP-resolved syms) instead of `type_edge` (bare names)
    /// so the LGG can recurse into resolved local types. Unresolved types
    /// (std/external: `HashMap`, `Vec`, `String`) stay as leaves — correct,
    /// since they have no local structure to compare. Two fields that resolve
    /// to the same local type sym now produce 0 vars instead of 1, eliminating
    /// the leaf-noise that drowned the bare-name LGG.
    fn refresh_type_lgg_rel(&self) -> Result<bool> {
        let edges: Vec<(String, String, String)> = {
            let conn = self.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"src\",\"dst\",\"kind\" FROM {}", tbl("type_link")
            ))?;
            let rows = s.query_map([], |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            )))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        let computed: Vec<(String, String, i64)> = typegraph::type_lgg_pairs(&edges);
        let stored: Vec<(String, String, i64)> = {
            let conn = self.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"a\",\"b\",\"vars\" FROM {} ORDER BY \"a\",\"b\",\"vars\"",
                tbl("type_lgg")
            ))?;
            let rows = s.query_map([], |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            )))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        if stored == computed {
            return Ok(false);
        }
        let rows: Vec<Vec<Value>> = computed
            .into_iter()
            .map(|(a, b, v)| vec![Value::Text(a), Value::Text(b), Value::Int(v)])
            .collect();
        self.refresh_rel("type_lgg", &["a", "b", "vars"], &rows)?;
        Ok(true)
    }

    /// Refresh `changed_line(path, line)` from the worktree diff. Two sources:
    ///   1. `git diff -U0 HEAD` new-side hunks: parse `@@ -a,b +c,d @@`, emit
    ///      lines `c..c+d-1` for each hunk (the lines that exist in the worktree
    ///      and differ from HEAD). A pure-deletion hunk has `d=0` -> no lines.
    ///   2. Untracked files (the `??` rows of `git status --porcelain -uall`),
    ///      which `git diff HEAD` omits entirely: emit every line 1..=N.
    /// Paths are canonicalized to repo-relative exactly like `refresh_changed_rel`
    /// so a `changed_line(p, l)` row joins `call_site(_, _, p, l)` on equal text.
    fn refresh_changed_line_rel(&self) -> Result<bool> {
        let top = Command::new("git").arg("-C").arg(&self.root)
            .args(["rev-parse", "--show-toplevel"]).output();
        let mut rows: Vec<(String, i64)> = Vec::new();
        if let Ok(out) = top {
            if out.status.success() {
                let toplevel = PathBuf::from(
                    String::from_utf8_lossy(&out.stdout).trim().to_string());
                // Canonicalize for prefix-stripping: git prints the physical
                // toplevel (macOS /private/var) while --root may be the symlink.
                let root = std::fs::canonicalize(&self.root)
                    .unwrap_or_else(|_| self.root.clone());
                let canon = |p: &str| -> Option<String> {
                    toplevel.join(p).strip_prefix(&root)
                        .ok().map(|r| r.to_string_lossy().replace('\\', "/"))
                };
                // (1) tracked modifications via context-free hunks.
                let diff = Command::new("git").arg("-C").arg(&self.root)
                    .args(["diff", "-U0", "HEAD"]).output();
                if let Ok(d) = diff {
                    if d.status.success() {
                        let stdout = String::from_utf8_lossy(&d.stdout);
                        let mut cur: Option<String> = None;
                        for line in stdout.lines() {
                            if let Some(rest) = line.strip_prefix("+++ ") {
                                cur = if rest == "/dev/null" { None }
                                      else { canon(rest.strip_prefix("b/").unwrap_or(rest)) };
                            } else if line.starts_with("@@") {
                                if let (Some(path), Some((c, n))) = (&cur, Self::hunk_new_range(line)) {
                                    for ln in c..c + n {
                                        rows.push((path.clone(), ln));
                                    }
                                }
                            }
                        }
                    }
                }
                // (2) untracked files: emit every line; git diff HEAD omits them.
                let status = Command::new("git").arg("-C").arg(&self.root)
                    .args(["status", "--porcelain", "-uall"]).output()?;
                if status.status.success() {
                    for line in String::from_utf8_lossy(&status.stdout).lines() {
                        if line.len() < 4 || &line[..2] != "??" { continue; }
                        let p = line[3..].rsplit(" -> ").next().unwrap_or(&line[3..])
                            .trim_matches('"');
                        if let Some(rel) = canon(p) {
                            if let Ok(s) = std::fs::read_to_string(self.root.join(&rel)) {
                                for ln in 1..=(s.lines().count() as i64) {
                                    rows.push((rel.clone(), ln));
                                }
                            }
                        }
                    }
                }
            }
        }
        rows.sort();
        rows.dedup();
        let existing: Vec<(String, i64)> = {
            let conn = self.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"path\", \"line\" FROM {} ORDER BY \"path\", \"line\"",
                tbl("changed_line")))?;
            let rs = s.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?;
            rs.filter_map(|x| x.ok()).collect()
        };
        if existing == rows { return Ok(false); }
        let out: Vec<Vec<Value>> = rows.into_iter()
            .map(|(p, l)| vec![Value::Text(p), Value::Int(l)]).collect();
        self.refresh_rel("changed_line", &["path", "line"], &out)?;
        Ok(true)
    }

    /// Parse the new-side range `+c,d` out of an `@@ -a,b +c,d @@` hunk header.
    /// The count defaults to 1 when omitted (`@@ -a +c @@`); a deletion-only
    /// hunk carries `d=0`. Returns `(start, count)` on the new side.
    fn hunk_new_range(line: &str) -> Option<(i64, i64)> {
        let f: Vec<&str> = line.split_whitespace().collect();
        // ["@@", "-a,b", "+c,d", "@@", <optional fn label>...]
        let new = f.get(2)?.strip_prefix('+')?;
        let (c, n) = match new.split_once(',') {
            Some((c, n)) => (c.parse::<i64>().ok()?, n.parse::<i64>().ok()?),
            None => (new.parse::<i64>().ok()?, 1),
        };
        Some((c, n))
    }

    /// Read the Cargo.toml / package.json manifests above the file set, at this
    /// rev, into a map (manifest path -> contents) for the resolver's crate /
    /// package registries. Probes the distinct ancestor directories of the files;
    /// `read_content` errors (no such manifest) are skipped. Rev-correct (git show
    /// for a git rev, disk for WORK).
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
            for name in ["Cargo.toml", "package.json"] {
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
        self.rels.insert(d.name.clone(), RelMeta { cols: d.cols.clone() });
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

    /// Wipe derived tables and run the semi-naive fixpoint to convergence.
    #[tracing::instrument(skip_all, fields(n_rules = derived_rules.len(), n_rels = derived_rels.len()), level = "debug")]
    fn rebuild_derived(&self, derived_rules: &[&Rule], derived_rels: &[String]) -> Result<()> {
        for rel in derived_rels { self.db.conn().execute(&format!("DELETE FROM {}", tbl(rel)), [])?; }
        // Evaluate stratum by stratum: each runs a positive (monotone) semi-naive
        // fixpoint to convergence, so a higher stratum's negation reads relations
        // that lower strata have already finished.
        for group in stratify(derived_rules)? {
            let mut iters = 0;
            loop {
                let mut delta = 0usize;
                for &ri in &group { delta += self.db.conn().execute(&lower_rule(derived_rules[ri], &self.rels)?, [])?; }
                iters += 1;
                if delta == 0 { break; }
                if iters > 100_000 { bail!("fixpoint did not converge"); }
            }
        }
        Ok(())
    }

    fn any_closure_empty(&self, edges: &[&str]) -> Result<bool> {
        for edge in edges {
            let n: i64 = self.db.conn().query_row(
                &format!("SELECT COUNT(*) FROM {}", scc_node_tbl(edge)), [], |r| r.get(0))?;
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
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
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

    fn insert_spine_strings(&self, rows: &[(String, String, Vec<Value>)]) -> Result<usize> {
        let mut by_id: BTreeMap<String, (String, String)> = BTreeMap::new();
        for (_, _, row) in rows {
            for v in row {
                let Value::Text(s) = v else { continue };
                if s.is_empty() { continue; }
                let id = spine::StringId::of(s).to_string();
                by_id.entry(id).or_insert_with(|| (s.clone(), spine::normalize(s)));
            }
        }
        let string_rows: Vec<Vec<Value>> = by_id.into_iter()
            .map(|(id, (content, norm))| vec![Value::Text(id), Value::Text(content), Value::Text(norm)])
            .collect();
        self.db.insert_rows("_strings", &["id", "content", "norm"], &string_rows)
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
        let mut str_by_id: BTreeMap<String, (String, String)> = BTreeMap::new();
        for (repo, path, w, text) in wheres {
            let id = spine::WhereBytesId::of_located(*w, repo, path).to_string();
            by_id.entry(id.clone()).or_insert_with(|| vec![
                Value::Text(id),
                Value::Text(w.string.to_string()),
                Value::Text(w.file.to_string()),
                Value::Int(w.lo as i64),
                Value::Int(w.hi as i64),
                Value::Text(repo.clone()),
                Value::Text(w.rev.to_string()),
                Value::Text(path.clone()),
            ]);
            if let Some(t) = text {
                if !t.is_empty() {
                    str_by_id.entry(w.string.to_string())
                        .or_insert_with(|| (t.clone(), spine::normalize(t)));
                }
            }
        }
        if !str_by_id.is_empty() {
            let string_rows: Vec<Vec<Value>> = str_by_id.into_iter()
                .map(|(id, (content, norm))| vec![Value::Text(id), Value::Text(content), Value::Text(norm)])
                .collect();
            self.db.insert_rows("_strings", &["id", "content", "norm"], &string_rows)?;
        }
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
            let a: String = row.get(0).unwrap_or_default();
            let b: String = row.get(1).unwrap_or_default();
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

    fn run_query(&self, q: &Query, closures: &HashMap<String, String>) -> Result<()> {
        // Seeded Rust path on a closure head: src pinned + dst free is a forward
        // walk (callees); dst pinned + src free is a reverse walk (callers).
        // Both-pinned, both-free, or anything else falls through to the SQL view.
        if let Some(edge) = closures.get(&q.head.rel) {
            if q.head.terms.len() == 2 {
                if let Some(cc) = self.closure_cache.get(edge) {
                    match (pinned_value(q, 0), pinned_value(q, 1)) {
                        (Some(seed), None) if matches!(q.head.terms[1], Term::Var(_)) =>
                            return self.run_reaches_point(q, cc, &seed, true),
                        (None, Some(seed)) if matches!(q.head.terms[0], Term::Var(_)) =>
                            return self.run_reaches_point(q, cc, &seed, false),
                        _ => {}
                    }
                }
            }
        }
        let res = self.query_one_sql(q)?;
        if self.query_json {
            emit_query_json(&res.rel, &res.columns, &res.rows);
        } else {
            println!("? {} => {}", res.rel, if res.columns.is_empty() { "(count)".into() } else { res.columns.join("\t") });
            for cells in &res.rows {
                println!("  {}", cells.iter().map(json_cell_tsv).collect::<Vec<_>>().join("\t"));
            }
            println!("  ({} rows)\n", res.rows.len());
        }
        Ok(())
    }

    /// Run one `?` query through the SQL view path only (no closure-cache
    /// optimization). Used by the daemon RPC `query` path, which needs the
    /// rows without capturing stdout.
    fn query_one_sql(&self, q: &Query) -> Result<QueryResult> {
        let (sql, columns) = lower_query(q, &self.rels)?;
        let mut stmt = self.db.conn().prepare(&sql)?;
        let ncols = stmt.column_count();
        let mut rows = stmt.query([])?;
        let mut out: Vec<Vec<serde_json::Value>> = Vec::new();
        while let Some(row) = rows.next()? {
            let cells = (0..ncols)
                .map(|i| sqlite_to_json(row.get::<_, rusqlite::types::Value>(i).unwrap_or(rusqlite::types::Value::Null)))
                .collect();
            out.push(cells);
        }
        Ok(QueryResult { rel: q.head.rel.clone(), columns, rows: out })
    }

    /// Run every `?` query in `prog`, returning rows. Used by the daemon RPC
    /// `query` (the foreground path goes through `run_query` which prints). The
    /// closure-cache optimization is skipped; the SQL view is always used, so
    /// results are correct but a path query on a large graph pays the SQL cost.
    pub fn run_queries_capture(&self, prog: &Program) -> Result<Vec<QueryResult>> {
        let mut out = Vec::new();
        for item in &prog.items {
            if let Item::Query(q) = item { out.push(self.query_one_sql(q)?); }
        }
        Ok(out)
    }

    /// Run every `gen` item after the fixpoint: evaluate the body, render the
    /// templates, write the targets. A write is skipped when the target bytes
    /// Arm the verify-rollback journal (christmas #14): subsequent gen writes
    /// stash each target's pre-tick bytes so `rollback_writes` can undo them if a
    /// checker rejects the edit. Call before `tick`/`run`.
    pub fn begin_verify(&self) {
        *self.gen_journal.borrow_mut() = Some(Vec::new());
    }

    /// Restore every file a gen write touched since `begin_verify` to its
    /// pre-tick bytes (deleting files that did not exist before). Returns the
    /// number of paths restored and disarms the journal. Used when a verify
    /// checker fails: keep-if-pass means roll-back-if-fail.
    pub fn rollback_writes(&self) -> Result<usize> {
        let journal = self.gen_journal.borrow_mut().take().unwrap_or_default();
        let n = journal.len();
        for (path, original) in journal.into_iter().rev() {
            let full = self.root.join(&path);
            match original {
                Some(bytes) => std::fs::write(&full, &bytes)?,
                None => { let _ = std::fs::remove_file(&full); }
            }
        }
        Ok(n)
    }

    /// Disarm the journal without restoring (keep the writes). Returns how many
    /// paths were written under verify. Used when the checker passes.
    pub fn commit_writes(&self) -> usize {
        self.gen_journal.borrow_mut().take().map_or(0, |j| j.len())
    }

    /// `std::fs::write`, but when the verify journal is armed, stash the target's
    /// original bytes first (first write per path wins, so the stash is always
    /// the pre-tick state). All gen apply paths route writes through here.
    fn journaled_write(&self, full: &Path, rel: &str, bytes: &[u8]) -> Result<()> {
        if let Some(journal) = self.gen_journal.borrow_mut().as_mut() {
            if !journal.iter().any(|(p, _)| p == rel) {
                journal.push((rel.to_string(), std::fs::read(full).ok()));
            }
        }
        std::fs::write(full, bytes)?;
        Ok(())
    }

    /// already match, so a converged tick leaves the tree untouched.
    #[tracing::instrument(skip_all, level = "debug")]
    fn run_gens(&mut self, prog: &Program, quiet: bool) -> Result<()> {
        let mut written: Vec<String> = Vec::new();
        // Candidate write roots for gen targets: self.root plus the `.git`
        // ancestor of every rule's origin — but ONLY when root_implicit (rootless
        // daemon), since that's the one mode where self.root is a placeholder and
        // a loaded script must write back to the repo it scans. Foreground/LSP
        // honor the explicit self.root.
        let mut write_roots: Vec<PathBuf> = vec![self.root.clone()];
        if self.root_implicit {
            for item in &prog.items {
                if let Item::Rule(r) = item {
                    if let Some(o) = &r.origin {
                        if let Some(a) = crate::repo::nearest_git(o) {
                            if !write_roots.contains(&a) { write_roots.push(a); }
                        }
                    }
                }
            }
        }
        // Splice regions accumulate across ALL gen rules and apply per file in
        // one bottom-up write: the line coordinates come from the pre-tick file
        // content, so a second rule splicing the same file must not re-read a
        // file an earlier rule already grew.
        let mut splices = Splices::default();
        // Byte cursors (gen(:mode, ...)) accumulate the same way, separate from
        // line splices because their coordinates and apply semantics differ.
        let mut cursors = Cursors::default();
        // File targets claimed by a gen rule this tick. A second gen rule
        // rendering the SAME path would silently overwrite the first (the v0
        // last-wins trap: file emits don't concatenate across rules), so the
        // File arm bails loudly when a path is already claimed by another rule.
        // Within one rule, rows to one path concat via `groups`; the claim is
        // per rule, so that path stays legal.
        let mut claimed: HashMap<String, ()> = HashMap::new();
        for item in &prog.items {
            if let Item::Gen(g) = item {
                self.run_gen(g, &mut written, &mut splices, &mut cursors, &mut claimed, &write_roots)?;
            }
        }
        self.apply_splices(&splices, &mut written, &write_roots)?;
        self.apply_cursors(&cursors, &mut written, &write_roots)?;
        if !written.is_empty() && !quiet {
            eprintln!("[gen] wrote {}", written.join(", "));
        }
        Ok(())
    }

    fn run_gen(&mut self, g: &GenRule, written: &mut Vec<String>, splices: &mut Splices, cursors: &mut Cursors, claimed: &mut HashMap<String, ()>, write_roots: &[PathBuf]) -> Result<()> {
        // Target vars lead the SELECT so grouping reads prefix columns; the
        // ORDER BY over all vars makes render order deterministic.
        let target_vars: Vec<String> = match &g.target {
            GenTarget::File { path_tmpl } => tmpl_holes(path_tmpl),
            GenTarget::Splice { path, l0, l1 } => vec![var_of(path)?, var_of(l0)?, var_of(l1)?],
            GenTarget::Cursor { path, lo, hi, .. } => vec![var_of(path)?, var_of(lo)?, var_of(hi)?],
        };
        let mut vars = target_vars.clone();
        for v in tmpl_holes(&g.row_tmpl) {
            if !vars.contains(&v) { vars.push(v); }
        }
        let sql = crate::lower::lower_gen(&vars, &g.body, &self.rels)?;
        let rows: Vec<HashMap<String, String>> = {
            let mut stmt = self.db.conn().prepare(&sql)?;
            let mut it = stmt.query([])?;
            let mut out = Vec::new();
            while let Some(row) = it.next()? {
                let mut m = HashMap::new();
                for (i, v) in vars.iter().enumerate() {
                    use rusqlite::types::Value as V;
                    let cell = match row.get::<_, V>(i)? {
                        V::Text(s) => s,
                        V::Integer(n) => n.to_string(),
                        V::Real(f) => f.to_string(),
                        _ => String::new(),
                    };
                    m.insert(v.clone(), cell);
                }
                out.push(m);
            }
            out
        };

        match &g.target {
            GenTarget::File { path_tmpl } => {
                let mut order: Vec<String> = Vec::new();
                let mut groups: HashMap<String, Vec<String>> = HashMap::new();
                for m in &rows {
                    let path = render_tmpl(path_tmpl, m);
                    if path.starts_with('/') || path.split('/').any(|s| s == "..") {
                        bail!("gen path `{path}` escapes the root");
                    }
                    if !groups.contains_key(&path) { order.push(path.clone()); }
                    groups.entry(path).or_default().push(render_tmpl(&g.row_tmpl, m));
                }
                // A path another gen rule already wrote this tick would be
                // clobbered, not concatenated. Bail loudly (mirrors the
                // mixed-source/derived bail): union the rows into one relation
                // and emit them from a single gen rule.
                for p in &order {
                    if claimed.insert(p.clone(), ()).is_some() {
                        bail!("two gen rules write the same file `{p}`; file emits don't \
                               concatenate across rules (last would win). Union the rows into \
                               one relation and emit from a single gen rule.");
                    }
                }
                for p in order {
                    let content = format!("{}\n", groups[&p].join("\n"));
                    let full = resolve_write_full(write_roots, &p);
                    if std::fs::read(&full).ok().as_deref() == Some(content.as_bytes()) { continue; }
                    if let Some(dir) = full.parent() { std::fs::create_dir_all(dir)?; }
                    self.journaled_write(&full, &p, content.as_bytes())?;
                    written.push(p);
                }
            }
            GenTarget::Splice { .. } => {
                // Rows group by (path, l0, l1) — the `comment` op's paired
                // coordinates over WORK. Accumulate only; the file writes
                // happen once, in apply_splices, after every gen rule ran.
                for m in &rows {
                    let path = m[&vars[0]].clone();
                    let l0: i64 = m[&vars[1]].parse()
                        .map_err(|_| anyhow::anyhow!("gen splice l0 must be an int, got `{}`", m[&vars[1]]))?;
                    let l1: i64 = m[&vars[2]].parse()
                        .map_err(|_| anyhow::anyhow!("gen splice l1 must be an int, got `{}`", m[&vars[2]]))?;
                    if l1 <= l0 {
                        bail!("gen splice needs l1 > l0 (paired marker lines), got {l0}..{l1} in {path}");
                    }
                    let key = (path, l0, l1);
                    if !splices.groups.contains_key(&key) { splices.keys.push(key.clone()); }
                    splices.groups.entry(key).or_default().push(render_tmpl(&g.row_tmpl, m));
                }
            }
            GenTarget::Cursor { mode, .. } => {
                // Byte-accurate splice into [lo, hi); rows group by (path, lo,
                // hi, mode) but apply per-file across the union of regions. lo
                // and hi come from the ref spine, so a typical rule joins match
                // -> ref(id, _, path, lo, hi) -> gen(:mode, path, lo, hi, ...).
                for m in &rows {
                    let path = m[&vars[0]].clone();
                    let lo: i64 = m[&vars[1]].parse()
                        .map_err(|_| anyhow::anyhow!("gen :{} lo must be an int, got `{}`", mode.tag(), m[&vars[1]]))?;
                    let hi: i64 = m[&vars[2]].parse()
                        .map_err(|_| anyhow::anyhow!("gen :{} hi must be an int, got `{}`", mode.tag(), m[&vars[2]]))?;
                    if hi < lo {
                        bail!("gen :{} needs hi >= lo, got {lo}..{hi} in {path}", mode.tag());
                    }
                    let key = (path.clone(), lo, hi, *mode);
                    if !cursors.groups.contains_key(&key) { cursors.keys.push(key.clone()); }
                    cursors.groups.entry(key).or_default().push(render_tmpl(&g.row_tmpl, m));
                }
            }
        }
        Ok(())
    }

    /// Replace the lines strictly between each region's marker lines; the
    /// markers stay. One read-modify-write per file, regions bottom-up so
    /// earlier line numbers stay valid across the whole batch.
    ///
    /// Overlap gate (same stance as `apply_cursors`): the bottom-up apply
    /// assumes every region's line window refers to the ORIGINAL line list.
    /// Two regions whose [l0, l1) windows intersect would corrupt, so bail
    /// loudly.
    fn apply_splices(&self, splices: &Splices, written: &mut Vec<String>, write_roots: &[PathBuf]) -> Result<()> {
        let mut files: Vec<&str> = Vec::new();
        for (p, _, _) in &splices.keys { if !files.contains(&p.as_str()) { files.push(p); } }
        for p in files {
            let full = resolve_write_full(write_roots, p);
            let old = std::fs::read_to_string(&full)
                .map_err(|e| anyhow::anyhow!("gen splice target {p}: {e}"))?;
            let mut lines: Vec<String> = old.lines().map(|s| s.to_string()).collect();
            let mut regions: Vec<&(String, i64, i64)> =
                splices.keys.iter().filter(|k| k.0 == p).collect();
            // Disjointness gate: ascending by l0, require each region's l1 <=
            // the next region's l0.
            regions.sort_by_key(|k| k.1);
            for w in regions.windows(2) {
                if w[0].2 > w[1].1 {
                    bail!("gen splice regions overlap in {p}: lines [{},{}) and [{},{}) \
                          collide; merge into one rule or disjoint the windows",
                          w[0].1, w[0].2, w[1].1, w[1].2);
                }
            }
            regions.sort_by_key(|k| std::cmp::Reverse(k.1));
            for k in regions {
                let (l0, l1) = (k.1 as usize, k.2 as usize);
                if l1 > lines.len() {
                    bail!("gen splice region {l0}..{l1} out of range in {p} ({} lines)", lines.len());
                }
                lines.splice(l0..l1 - 1, splices.groups[k].iter().cloned());
            }
            let content = format!("{}\n", lines.join("\n"));
            if content != old {
                self.journaled_write(&full, p, content.as_bytes())?;
                written.push(p.to_string());
            }
        }
        Ok(())
    }

    /// Apply byte-accurate gen(:mode, ...) cursors per file. Port of v4's
    /// `WriteCursorComponent::render_batch`: group by path, sort regions
    /// right-to-left by lo so earlier offsets stay valid as the buffer grows,
    /// then dispatch on mode. :wrap inserts at both endpoints; the other three
    /// modes splice into the [lo, hi) window, at lo, at hi, or replace it
    /// outright. One read-modify-write per file. Skipped silently if the
    /// resulting bytes match the original. `write_roots` resolves the target
    /// per file (parity with `apply_splices`): under `root_implicit` a cursor
    /// rule from a loaded script writes back to that script's repo, not the
    /// placeholder root.
    ///
    /// Overlap gate: the right-to-left apply assumes every region's window
    /// refers to the ORIGINAL buffer. Two regions whose [lo, hi) windows
    /// intersect would see the second operate on already-mutated bytes (silent
    /// corruption), so bail loudly — same stance as the file-emit claimed-path
    /// bail. Regions keyed identically (same path/lo/hi/mode) already concat
    /// their rows into one payload, so this only fires across DISTINCT windows.
    fn apply_cursors(&self, cursors: &Cursors, written: &mut Vec<String>, write_roots: &[PathBuf]) -> Result<()> {
        let mut files: Vec<&str> = Vec::new();
        for (p, _, _, _) in &cursors.keys { if !files.contains(&p.as_str()) { files.push(p); } }
        for p in files {
            let full = resolve_write_full(write_roots, p);
            let original = std::fs::read(&full)
                .map_err(|e| anyhow::anyhow!("gen :mode target {p}: {e}"))?;
            let mut regions: Vec<&(String, i64, i64, SpliceMode)> =
                cursors.keys.iter().filter(|k| k.0 == p).collect();
            // Disjointness gate: ascending by lo, require each region's hi <=
            // the next region's lo. Adjacent-equal (hi == next lo) is allowed
            // (windows touch but don't overlap).
            regions.sort_by_key(|k| k.1);
            for w in regions.windows(2) {
                if w[0].2 > w[1].1 {
                    bail!("gen :{} and :{} regions overlap in {p}: [{},{}) and [{},{}) \
                          collide; merge into one rule or disjoint the windows",
                          w[0].3.tag(), w[1].3.tag(), w[0].1, w[0].2, w[1].1, w[1].2);
                }
            }
            // Right-to-left by lo so prior inserts don't shift later offsets.
            regions.sort_by_key(|k| std::cmp::Reverse(k.1));
            let mut buf: Vec<u8> = original.clone();
            for k in regions {
                let (lo, hi, mode) = (k.1 as usize, k.2 as usize, k.3);
                let lo = lo.min(buf.len());
                let hi = hi.min(buf.len()).max(lo);
                // Each row in the group becomes one insertion/splice. Rows
                // already share (path, lo, hi, mode); render their templates in
                // order and concatenate so multi-row groups behave like
                // line-splice (rows join into one payload).
                let payload: Vec<u8> = cursors.groups[k].iter()
                    .flat_map(|s| s.as_bytes().iter().copied())
                    .collect();
                match mode {
                    SpliceMode::Replace => { buf.splice(lo..hi, payload); }
                    SpliceMode::Append  => { buf.splice(hi..hi, payload); }
                    SpliceMode::Prepend => { buf.splice(lo..lo, payload); }
                    SpliceMode::Wrap    => {
                        // Insert at hi first so the lo offset stays valid, then lo.
                        buf.splice(hi..hi, payload.clone());
                        buf.splice(lo..lo, payload);
                    }
                }
            }
            if buf != original {
                self.journaled_write(&full, p, &buf)?;
                written.push(p.to_string());
            }
        }
        Ok(())
    }
}

/// Splice regions collected across every gen rule in a tick: key order is
/// first appearance, rows per key append in rule order.
#[derive(Default)]
struct Splices {
    keys: Vec<(String, i64, i64)>,
    groups: HashMap<(String, i64, i64), Vec<String>>,
}

/// Byte-cursor regions collected across every gen(:mode, ...) rule in a tick.
/// The key is (path, lo, hi, mode); rows per key join into one payload at
/// apply time. Separate from `Splices` because the coordinates and apply
/// semantics differ (byte offsets vs line numbers, four modes vs implicit
/// replace). Ported from v4's `WriteCursorComponent` accumulator.
#[derive(Default)]
struct Cursors {
    keys: Vec<(String, i64, i64, SpliceMode)>,
    groups: HashMap<(String, i64, i64, SpliceMode), Vec<String>>,
}

/// `{var}` holes in a gen template, first-appearance order, deduped. A brace
/// pair whose inside is not a plain identifier is literal text.
fn tmpl_holes(t: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = t;
    while let Some(i) = rest.find('{') {
        rest = &rest[i + 1..];
        let Some(j) = rest.find('}') else { break };
        let name = &rest[..j];
        if !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !out.iter().any(|v| v == name)
        {
            out.push(name.to_string());
        }
        rest = &rest[j + 1..];
    }
    out
}

fn render_tmpl(t: &str, vals: &HashMap<String, String>) -> String {
    let mut out = t.to_string();
    for (k, v) in vals {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

/// SQLite value -> JSON: text stays a string, integer/real become JSON numbers,
/// everything else (NULL/blob) becomes JSON null. The TSV path stringifies these
/// back via `json_cell_tsv`, so both renderers share one cell representation.
fn sqlite_to_json(v: rusqlite::types::Value) -> serde_json::Value {
    use rusqlite::types::Value as V;
    match v {
        V::Text(s) => serde_json::Value::String(s),
        V::Integer(n) => serde_json::Value::from(n),
        V::Real(f) => serde_json::Value::from(f),
        _ => serde_json::Value::Null,
    }
}

/// Render a JSON cell for the human TSV block: strings raw (no quotes), numbers
/// as their text, null empty — matching the pre-JSON behavior exactly.
fn json_cell_tsv(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// One JSON object per query (JSON-lines), so a program's multiple `?` queries
/// stream as independent records a tool can read line by line.
fn emit_query_json(rel: &str, columns: &[String], rows: &[Vec<serde_json::Value>]) {
    let obj = serde_json::json!({
        "query": rel,
        "columns": columns,
        "rows": rows,
        "count": rows.len(),
    });
    println!("{obj}");
}

/// (repo, rev, glob, pathvar, revvar) of a source rule's `scan`. `repo` is the
/// repo coordinate ("." = self repo); resolve it to a root via
/// `Engine::resolve_repo_root`.
/// The scan atom of a source rule, with its coordinate Terms intact (a `Term::Var`
/// in `repo`/`rev` is a data-driven coordinate — see `Engine::resolve_scan_bindings`).
fn scan_spec_of(rule: &Rule) -> Result<ScanSpec> {
    for item in &rule.body {
        if let BodyItem::Scan { repo, rev, glob, path, rev_out } = item {
            return Ok(ScanSpec {
                repo: repo.clone(), rev: rev.clone(), glob: glob.clone(),
                path_var: var_of(path)?, rev_out_var: var_of(rev_out)?,
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

fn read_content(root: &Path, rev: &str, path: &str) -> Result<String> {
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
    let p = match v { Value::Text(s) => s, Value::Int(_) => return ty == Type::Int || ty == Type::Text };
    if rev != "WORK" {
        return match ty {
            Type::File | Type::Path => rev_index.contains(&(repo.to_string(), rev.to_string(), p.clone())),
            Type::Dir => rev_index.iter().any(|(rp, r, pp)| rp == repo && r == rev && pp.starts_with(&format!("{p}/"))),
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
/// scans pathological.
fn enumerate_with_hash(repo: &str, repo_root: &Path, rev: &str, union: &globset::GlobSet, prev: &FileMeta) -> Result<Vec<(String, String, i64, i64)>> {
    let max_size = max_filesize();
    if rev == "WORK" {
        let mut files: Vec<(PathBuf, String, i64, i64)> = Vec::new();
        let mut walk = ignore::WalkBuilder::new(repo_root);
        walk.hidden(false).filter_entry(|e| e.file_name() != ".git");
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
        // reuse stored hash when mtime+size match; otherwise read+hash (parallel)
        let mut out: Vec<(String, String, i64, i64)> = files.par_iter().map(|(abs, rel, mt, sz)| {
            if let Some((h, pmt, psz)) = prev.get(&(repo.to_string(), rel.clone(), "WORK".to_string())) {
                if pmt == mt && psz == sz {
                    return (rel.clone(), h.clone(), *mt, *sz);
                }
            }
            let bytes = std::fs::read(abs).unwrap_or_default();
            (rel.clone(), blake3::hash(&bytes).to_hex().to_string(), *mt, *sz)
        }).collect();
        out.sort();
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
            if union.is_match(path) { out.push((path.to_string(), oid.to_string(), 0, size)); }
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
    if !re_cache.contains_key(regex) { re_cache.insert(regex.to_string(), Regex::new(regex)?); }
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
        b.insert(revvar.clone(), Value::Text(rev.to_string()));
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
                if !re_cache.contains_key(open) { re_cache.insert(open.clone(), Regex::new(open)?); }
                if let Some(c) = close {
                    if !re_cache.contains_key(c) { re_cache.insert(c.clone(), Regex::new(c)?); }
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
                    .ok_or_else(|| anyhow::anyhow!("head var {v} unbound in source rule"))?,
                Term::Str(s) => Value::Text(s.clone()),
                Term::Int(n) => Value::Int(*n),
                Term::Interp(parts) => interp_value(parts, &b)?,
                Term::Wild => bail!("'_' in head not allowed"),
                Term::PathLit { .. } => bail!("path literal not normalized before lowering"),
                Term::Arith { .. } => val_of(term, &b)?,
                Term::Call { .. } => val_of(term, &b)?,
            };
            if !check_type(head_meta.cols[i].ty, &v, repo, rev, root, rev_index) { dropped += 1; continue 'bind; }
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
        Term::Var(v) => b.get(v).cloned().ok_or_else(|| anyhow::anyhow!("unbound var {v} in constraint")),
        Term::Str(s) => Ok(Value::Text(s.clone())),
        Term::Int(n) => Ok(Value::Int(*n)),
        Term::Interp(parts) => interp_value(parts, b),
        Term::Wild => bail!("'_' in constraint"),
        Term::PathLit { .. } => bail!("path literal not normalized before lowering"),
        Term::Arith { op, lhs, rhs } => {
            let (l, r) = (val_of(lhs, b)?, val_of(rhs, b)?);
            let (Value::Int(a), Value::Int(c)) = (&l, &r) else {
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
            let re = regex::Regex::new(&r.as_str())?;
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

fn ts_lang(lang: &str) -> Result<tree_sitter::Language> {
    match lang {
        "rust" | "rs" => Ok(tree_sitter::Language::new(tree_sitter_rust::LANGUAGE)),
        "c" => Ok(tree_sitter::Language::new(tree_sitter_c::LANGUAGE)),
        "kotlin" | "kt" => Ok(tree_sitter::Language::new(tree_sitter_kotlin_sg::LANGUAGE)),
        "py" | "python" => Ok(tree_sitter::Language::new(tree_sitter_python::LANGUAGE)),
        "sh" | "bash" | "shell" => Ok(tree_sitter::Language::new(tree_sitter_bash::LANGUAGE)),
        "go" | "golang" => Ok(tree_sitter::Language::new(tree_sitter_go::LANGUAGE)),
        "hcl" | "terraform" | "tf" => Ok(tree_sitter::Language::new(tree_sitter_hcl::LANGUAGE)),
        "starlark" | "bzl" | "bazel" => Ok(tree_sitter::Language::new(tree_sitter_starlark::LANGUAGE)),
        "jsonnet" => Ok(tree_sitter::Language::new(tree_sitter_jsonnet::LANGUAGE)),
        "gotmpl" | "gotemplate" | "gohtml" => Ok(tree_sitter::Language::new(unsafe {
            tree_sitter_language::LanguageFn::from_raw(tree_sitter_gotmpl)
        })),
        "dockerfile" | "docker" => Ok(tree_sitter::Language::new(unsafe {
            tree_sitter_language::LanguageFn::from_raw(tree_sitter_dockerfile)
        })),
        other => bail!("no ast grammar for :{other} (compiled in: rust, c, kotlin, python, bash, go, hcl, starlark, jsonnet, gotmpl, dockerfile)"),
    }
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

