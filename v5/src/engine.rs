use anyhow::{bail, Result};
use rayon::prelude::*;
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use crate::ast::*;
use crate::lower::{lower_query, lower_rule, tbl};
use crate::modgraph::{self, ProjectCx, Resolution};
use crate::scc;
use crate::scip_import;
use crate::spine;
use crate::typegraph;

fn scc_node_tbl(edge: &str) -> String { format!("scc_node_{edge}") }
fn scc_edge_tbl(edge: &str) -> String { format!("scc_edge_{edge}") }

/// The built-in relations of the data-model contract (docs/data-model.md). A
/// `.dl` program may not declare these names; they are registered with fixed
/// schemas and refreshed each tick from the `_file` change-detection cache, so
/// any rule can join the file set without a `scan`. Stage 1: ids are the raw
/// rev string / content hash (no interning yet; that is Stage 2).
const BUILTIN_RELS: [&str; 4] = ["repo", "rev", "content", "file"];

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

/// Syntax-only Rust type graph. `kind` is edge metadata; closure(type_edge)
/// walks the first two columns.
const TYPE_RELS: [&str; 2] = ["type_edge", "type_edge_rev"];

/// Compiler-backed SCIP importer. `scip_edge` is file-to-file dependency data
/// extracted from definition/reference occurrences in an existing index.scip.
const SCIP_RELS: [&str; 3] = ["scip_def", "scip_ref", "scip_edge"];

/// Ref-spine query relations: thin views over the `_strings` / `_where_bytes`
/// meta tables. `string(id, text, norm)` resolves an interned StringId to its
/// content; `ref(id, string, file, lo, hi)` locates each interned string's byte
/// span, `id` being the `_where_bytes` id (the rewrite coordinate an `edit` keys
/// off). Join them to ask "where does <text> occur": `string(s, "Foo", _),
/// ref(_, s, f, lo, hi)`. Populated for regex/ast/sg captures and import refs.
const SPINE_RELS: [&str; 2] = ["string", "ref"];

fn builtin_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col { name: n.to_string(), ty: t };
    vec![
        RelDecl { name: "repo".into(), cols: vec![c("id", Type::Text), c("slug", Type::Text), c("root", Type::Path)] },
        RelDecl { name: "rev".into(), cols: vec![c("id", Type::Text), c("repo", Type::Text), c("oid", Type::Text), c("ts", Type::Int)] },
        RelDecl { name: "content".into(), cols: vec![c("id", Type::Text), c("hash", Type::Text)] },
        RelDecl { name: "file".into(), cols: vec![c("repo", Type::Text), c("rev", Type::Text), c("path", Type::Path), c("content", Type::Text)] },
    ]
}

fn module_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col { name: n.to_string(), ty: t };
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
    let c = |n: &str, t: Type| Col { name: n.to_string(), ty: t };
    vec![
        RelDecl { name: "type_edge".into(), cols: vec![c("from", Type::Text), c("to", Type::Text), c("kind", Type::Text)] },
        RelDecl { name: "type_edge_rev".into(), cols: vec![c("from", Type::Text), c("to", Type::Text), c("kind", Type::Text), c("rev", Type::Text)] },
    ]
}

fn scip_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col { name: n.to_string(), ty: t };
    vec![
        RelDecl { name: "scip_def".into(), cols: vec![c("symbol", Type::Text), c("file", Type::Path)] },
        RelDecl { name: "scip_ref".into(), cols: vec![c("file", Type::Path), c("symbol", Type::Text), c("def_file", Type::Path)] },
        RelDecl { name: "scip_edge".into(), cols: vec![c("src", Type::Path), c("dst", Type::Path)] },
    ]
}

fn spine_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col { name: n.to_string(), ty: t };
    vec![
        RelDecl { name: "string".into(), cols: vec![c("id", Type::Text), c("text", Type::Text), c("norm", Type::Text)] },
        RelDecl { name: "ref".into(), cols: vec![
            c("id", Type::Text), c("string", Type::Text), c("file", Type::Text), c("lo", Type::Int), c("hi", Type::Int)] },
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
                    _ => {}
                }
            },
            Item::Query(q) => if hit(&q.head.rel) { return true; },
            Item::Rel(_) => {}
        }
    }
    false
}

fn module_rels_used(prog: &Program) -> bool { rels_used(prog, &MODULE_RELS) }

fn type_rels_used(prog: &Program) -> bool { rels_used(prog, &TYPE_RELS) }

fn scip_rels_used(prog: &Program) -> bool { rels_used(prog, &SCIP_RELS) }

fn spine_rels_used(prog: &Program) -> bool { rels_used(prog, &SPINE_RELS) }

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

/// head relation -> edge relation, for every `head(..) <- closure(edge).` rule.
fn closure_map(rules: &[&Rule]) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for r in rules {
        if let Some(edge) = r.closure_edge() { m.insert(r.head.rel.clone(), edge.to_string()); }
    }
    m
}

/// Unique edge relations across all closure heads (one condensation per graph).
fn dedup_edges(closures: &HashMap<String, String>) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    for e in closures.values() { if !out.contains(&e.as_str()) { out.push(e.as_str()); } }
    out
}

/// The literal a query pins head position `pos` to, via a literal head term or a
/// `where col = "lit"` constraint. None if that position is a free variable.
fn pinned_value(q: &Query, pos: usize) -> Option<String> {
    match &q.head.terms[pos] {
        Term::Str(s) => Some(s.clone()),
        Term::Var(v) => q.wheres.iter().find_map(|c| {
            if c.op != CmpOp::Eq { return None; }
            match (&c.lhs, &c.rhs) {
                (Term::Var(lv), Term::Str(s)) | (Term::Str(s), Term::Var(lv)) if lv == v => Some(s.clone()),
                _ => None,
            }
        }),
        _ => None,
    }
}

/// Closure heads are rebuilt *after* the derived fixpoint, so a derived rule body
/// that reads one would see stale/empty data in the same tick. Reject it (queries
/// run last and are fine; only rule bodies are stratified wrong).
fn check_stratification(derived_rules: &[&Rule], closures: &HashMap<String, String>) -> Result<()> {
    for r in derived_rules {
        for item in &r.body {
            if let BodyItem::Pos(a) | BodyItem::Neg(a) = item {
                if closures.contains_key(&a.rel) {
                    bail!("rule '{}' reads closure relation '{}' in its body; closures are \
                           rebuilt after the derived fixpoint and cannot be consumed by a rule \
                           in the same tick (queries can). Materialize '{}' into a base relation \
                           first, or query it directly.", r.head.rel, a.rel, a.rel);
                }
            }
        }
    }
    Ok(())
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
    let mut edges: Vec<(u32, u32, bool)> = Vec::new(); // (head, body, negative)
    for r in rules {
        let h = intern_rel(&r.head.rel, &mut id, &mut name);
        for item in &r.body {
            let (b, neg) = match item {
                BodyItem::Pos(a) => (intern_rel(&a.rel, &mut id, &mut name), false),
                BodyItem::Neg(a) => (intern_rel(&a.rel, &mut id, &mut name), true),
                _ => continue,
            };
            edges.push((h, b, neg));
        }
    }
    let n = name.len();
    let mut adj = vec![Vec::new(); n];
    for &(h, b, _) in &edges { adj[h as usize].push(b); }
    let (comp, ncomp) = scc::tarjan(&adj);

    // negation inside a recursive cycle has no stratified meaning
    for &(h, b, neg) in &edges {
        if neg && comp[h as usize] == comp[b as usize] {
            bail!("unstratifiable: relation '{}' is negated inside a recursive cycle", name[b as usize]);
        }
    }
    // condensed edge weight: 1 if any negative edge crosses these components
    let mut cw: HashMap<(u32, u32), u32> = HashMap::new();
    for &(h, b, neg) in &edges {
        let (cu, cv) = (comp[h as usize], comp[b as usize]);
        if cu != cv {
            let e = cw.entry((cu, cv)).or_insert(0);
            *e = (*e).max(if neg { 1 } else { 0 });
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

pub struct Engine {
    db: crate::db::Db,
    rels: Rels,
    root: PathBuf,
    pub dropped: usize,
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
}

impl Engine {
    pub fn new(db: crate::db::Db, root: PathBuf) -> Self {
        Engine {
            db, rels: HashMap::new(), root, dropped: 0, recondensed: 0,
            closure_cache: HashMap::new(),
            rev_cache: HashMap::new(),
            rev_index: std::collections::HashSet::new(),
            repos: Vec::new(),
        }
    }

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
    /// repo; otherwise an existing path (slug = its dir name). (Lazy clone of an
    /// un-cloned repo is a later phase.)
    fn resolve_repo(&self, repo: &str) -> Result<(String, PathBuf)> {
        if repo.is_empty() || repo == "." || repo == "self" {
            return Ok((self.self_slug(), self.root.clone()));
        }
        if let Some(rc) = self.repos.iter().find(|r| r.slug == repo) {
            Self::ensure_cloned(rc)?;
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
    /// are not yet on disk; anything else resolves to a single repo.
    fn resolve_scan_repos(&self, repo: &str) -> Result<Vec<(String, PathBuf)>> {
        if repo == "*" || repo == "all" {
            if self.repos.is_empty() {
                return Ok(vec![(self.self_slug(), self.root.clone())]);
            }
            let mut out = Vec::with_capacity(self.repos.len());
            for rc in &self.repos {
                Self::ensure_cloned(rc)?;
                out.push((rc.slug.clone(), rc.root.clone()));
            }
            return Ok(out);
        }
        Ok(vec![self.resolve_repo(repo)?])
    }

    /// Stable slug for this engine's own repo: the `--root` directory name.
    fn self_slug(&self) -> String {
        self.root.file_name().map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| self.root.to_string_lossy().to_string())
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

    /// Distinct WORK source paths from the `_file` cache. Feeds crate-root
    /// discovery (`rspath::crate_roots`) for the `--move` rewriter, so a crate
    /// whose root is `rust/kernel/lib.rs` (no `src/`) still yields module paths.
    pub fn source_paths(&self) -> Result<Vec<String>> {
        let conn = self.db.conn();
        let mut s = conn.prepare("SELECT DISTINCT path FROM _file WHERE rev = 'WORK'")?;
        let rows = s.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|x| x.ok()).collect())
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

    /// One reactive tick: declare, reconcile sources incrementally, rebuild
    /// derived only if a source fact changed, then run queries.
    pub fn tick(&mut self, prog: &Program, quiet: bool) -> Result<()> {
        self.rev_cache.clear();
        self.db.tick_begin();
        let rules: Vec<&Rule> = prog.items.iter().filter_map(|i| match i {
            Item::Rule(r) => Some(r), _ => None,
        }).collect();
        let closures = closure_map(&rules);
        self.declare_all(prog, &closures)?;
        self.ensure_meta()?;

        let source_rules: Vec<&Rule> = rules.iter().copied().filter(|r| r.is_source()).collect();
        // derived = neither source nor a closure rule (closures bypass lower_rule).
        let derived_rules: Vec<&Rule> = rules.iter().copied()
            .filter(|r| !r.is_source() && r.closure_edge().is_none()).collect();

        // source rels are heads of source rules; they get incremental retraction.
        let mut source_rels: Vec<String> = Vec::new();
        for r in &source_rules {
            if !source_rels.contains(&r.head.rel) { source_rels.push(r.head.rel.clone()); }
        }
        let mut derived_rels: Vec<String> = Vec::new();
        for r in &derived_rules {
            if !derived_rels.contains(&r.head.rel) { derived_rels.push(r.head.rel.clone()); }
        }
        let edges: Vec<&str> = dedup_edges(&closures);
        check_stratification(&derived_rules, &closures)?;
        self.create_auto_indexes(&derived_rules, &closures)?;

        let t_src = std::time::Instant::now();
        let recon = self.reconcile_sources(&source_rules, &source_rels)?;
        let mut changed = recon.changed;
        // Baseline each source relation's content digest so the next incremental
        // tick can skip a rebuild when bytes move but rows don't (see tick_paths).
        self.seed_rel_digests(&source_rels)?;
        // refresh built-in repo/rev/content/file from the updated _file cache,
        // before derived rules that may join them are rebuilt.
        self.refresh_builtin_rels()?;
        if module_rels_used(prog) { self.refresh_module_rels()?; }
        if type_rels_used(prog) { self.refresh_type_rels()?; }
        if scip_rels_used(prog) { changed |= self.refresh_scip_rels()?; }
        if spine_rels_used(prog) { self.refresh_spine_rels()?; }
        let src_ms = t_src.elapsed().as_secs_f64() * 1000.0;

        let t_der = std::time::Instant::now();
        let rebuilt_all = changed || self.any_derived_empty(&derived_rels)? || self.any_closure_empty(&edges)?;
        if rebuilt_all {
            self.rebuild_derived(&derived_rules, &derived_rels)?;
            self.rebuild_closures(&edges)?;
        }
        let der_ms = t_der.elapsed().as_secs_f64() * 1000.0;

        if !quiet {
            eprintln!("[tick] files {}/{} parsed, +{} -{} source facts, derived {} | source {:.1}ms, derived {:.1}ms",
                recon.parsed, recon.total, recon.extracted, recon.retracted,
                if changed { "rebuilt" } else { "unchanged" }, src_ms, der_ms);
        }
        // Full tick: every edge is potentially dirty when we rebuilt; the digest
        // check inside still skips the Tarjan for edges whose rows didn't move.
        let dirty: HashSet<&str> = if rebuilt_all { edges.iter().copied().collect() } else { HashSet::new() };
        self.refresh_cond_cache(&edges, &dirty)?;
        for item in &prog.items {
            if let Item::Query(q) = item { self.run_query(q, &closures)?; }
        }
        if self.dropped > 0 {
            eprintln!("[checked-type] dropped {} rows failing file/dir/path checks", self.dropped);
            self.dropped = 0;
        }
        self.db.tick_end();
        Ok(())
    }

    /// Reactive tick driven by a known set of changed paths (from the file
    /// watcher): reconciles only those paths, never walking or statting the
    /// tree. Only WORK source rules participate; route git-rev changes to `tick`.
    pub fn tick_paths(&mut self, prog: &Program, changed: &[PathBuf], quiet: bool) -> Result<()> {
        self.rev_cache.clear();
        self.db.tick_begin();
        let rules: Vec<&Rule> = prog.items.iter().filter_map(|i| match i { Item::Rule(r) => Some(r), _ => None }).collect();
        let closures = closure_map(&rules);
        self.declare_all(prog, &closures)?;
        self.ensure_meta()?;

        let source_rules: Vec<&Rule> = rules.iter().copied().filter(|r| r.is_source()).collect();
        let derived_rules: Vec<&Rule> = rules.iter().copied()
            .filter(|r| !r.is_source() && r.closure_edge().is_none()).collect();
        let mut source_rels: Vec<String> = Vec::new();
        for r in &source_rules { if !source_rels.contains(&r.head.rel) { source_rels.push(r.head.rel.clone()); } }
        let mut derived_rels: Vec<String> = Vec::new();
        for r in &derived_rules { if !derived_rels.contains(&r.head.rel) { derived_rels.push(r.head.rel.clone()); } }
        let edges: Vec<&str> = dedup_edges(&closures);
        check_stratification(&derived_rules, &closures)?;
        self.create_auto_indexes(&derived_rules, &closures)?;

        // WORK source rules with compiled glob matchers
        // The incremental watcher delta covers the self repo's WORK tree only
        // (changed paths under self.root); non-self repos scan via the full tick.
        let mut work_rules: Vec<(&Rule, globset::GlobMatcher)> = Vec::new();
        for r in &source_rules {
            let (repo, declared, glob, _, _) = scan_spec(r)?;
            let is_self = repo.is_empty() || repo == "." || repo == "self";
            if declared == "WORK" && is_self { work_rules.push((*r, globset::Glob::new(&glob)?.compile_matcher())); }
        }

        let prev = self.load_file_meta()?;
        let mut changed_facts = false;
        let mut changed_source_rels: HashSet<String> = HashSet::new();
        let mut module_delta_paths: HashSet<String> = HashSet::new();
        let mut module_full_work = false;
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
                retracted += self.retract_path(&slug, &rel, &source_rels)?;
                // Collect located spans across every matching rule for this file and
                // flush once after the loop (one `bump()`), not per-rule. Per-rule
                // flushing trips the N+1 screamer once enough files change.
                let mut where_rows: Vec<(String, String, spine::WhereBytes)> = Vec::new();
                for rule in &matching {
                    let (rows, where_bytes, dropped) = parse_file(rule, &slug, &rel, "WORK", &h, &self.root, &self.rels, &self.rev_index)?;
                    self.dropped += dropped;
                    let meta = self.rels.get(&rule.head.rel)
                        .ok_or_else(|| anyhow::anyhow!("unknown relation {}", rule.head.rel))?.clone();
                    extracted += self.insert_source_rows(&rule.head.rel, &meta, &slug, &rel, &rows)?;
                    where_rows.extend(where_bytes.into_iter().map(|w| (slug.clone(), rel.clone(), w)));
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
            if type_rels_used(prog) {
                self.refresh_type_rels()?;
                for t in TYPE_RELS { changed_source_rels.insert(t.to_string()); }
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
        self.refresh_cond_cache(&edges, &dirty_edges)?;
        for item in &prog.items { if let Item::Query(q) = item { self.run_query(q, &closures)?; } }
        if self.dropped > 0 { eprintln!("[checked-type] dropped {} rows", self.dropped); self.dropped = 0; }
        self.db.tick_end();
        Ok(())
    }

    fn ensure_meta(&self) -> Result<()> {
        self.db.conn().execute_batch(
            "CREATE TABLE IF NOT EXISTS _file (repo TEXT NOT NULL DEFAULT '', path TEXT, rev TEXT, hash TEXT,
                 mtime INTEGER DEFAULT 0, size INTEGER DEFAULT 0, PRIMARY KEY (repo, path, rev));
             CREATE TABLE IF NOT EXISTS _prov (rel TEXT, repo TEXT NOT NULL DEFAULT '', path TEXT, src TEXT, PRIMARY KEY (rel, repo, path, src));
             CREATE TABLE IF NOT EXISTS _reldigest (rel TEXT PRIMARY KEY, digest TEXT);
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

    fn reconcile_sources(&mut self, source_rules: &[&Rule], source_rels: &[String]) -> Result<Reconcile> {
        // Load prior file metadata first so enumerate can use the mtime fast-path.
        let prev = self.load_file_meta()?;

        let mut current: FileMeta = HashMap::new();
        // (rule idx, repo slug, path, rev, hash) for every enumerated file. A
        // single rule scanning `"*"` fans out to one batch of rows per config
        // repo, all carrying the same rule idx but distinct repo slugs.
        let mut rule_files: Vec<(usize, String, String, String, String)> = Vec::new();
        // slug -> on-disk root for every repo touched this tick; parse_file reads
        // content from the matching root and the slug stamps `_file`/`_prov` so
        // two repos sharing a path stay distinct.
        let mut root_by_repo: HashMap<String, PathBuf> = HashMap::new();
        for (idx, rule) in source_rules.iter().enumerate() {
            let (repo, declared, glob, _, _) = scan_spec(rule)?;
            for (slug, repo_root) in self.resolve_scan_repos(&repo)? {
                let rev = self.resolve_rev(&repo_root, &declared)?;
                for (path, h, mt, sz) in self.enumerate_with_hash(&slug, &repo_root, &rev, &glob, &prev)? {
                    current.insert((slug.clone(), path.clone(), rev.clone()), (h.clone(), mt, sz));
                    rule_files.push((idx, slug.clone(), path, rev.clone(), h));
                }
                root_by_repo.insert(slug, repo_root);
            }
        }
        self.rev_index = current.keys().map(|(repo, p, r)| (repo.clone(), r.clone(), p.clone())).collect();

        let hash_of = |m: &FileMeta, repo: &str, p: &str, r: &str|
            m.get(&(repo.to_string(), p.to_string(), r.to_string())).map(|t| t.0.clone());

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

        let retract_list: Vec<(&str, &str)> = to_retract.iter()
            .map(|(repo, p)| (repo.as_str(), p.as_str())).collect();
        let retracted = self.retract_paths(&retract_list, source_rels)?;

        let to_extract: Vec<(usize, String, String, String, String)> = rule_files.iter()
            .filter(|(_, repo, p, r, h)| hash_of(&prev, repo, p, r).as_ref() != Some(h))
            .map(|(idx, repo, p, r, h)| (*idx, repo.clone(), p.clone(), r.clone(), h.clone()))
            .collect();
        let parsed = to_extract.len();

        // Parse + extract in parallel across files (CPU-bound, no DB touch),
        // then insert serially (SQLite is single-writer).
        let results: Vec<Result<(String, String, Vec<Vec<Value>>, Vec<spine::WhereBytes>, usize)>> = {
            let Engine { rels, rev_index, .. } = &*self;
            to_extract.par_iter().map(|(idx, repo, path, rev, hash)| {
                let root = root_by_repo.get(repo)
                    .ok_or_else(|| anyhow::anyhow!("no root for repo {repo}"))?;
                let (rows, where_bytes, dropped) =
                    parse_file(source_rules[*idx], repo, path, rev, hash, root, rels, rev_index)?;
                let rel = source_rules[*idx].head.rel.clone();
                Ok((rel, path.clone(), rows, where_bytes, dropped))
            }).collect()
        };

        let mut by_rel: HashMap<String, Vec<(String, String, Vec<Value>)>> = HashMap::new();
        let mut where_bytes: Vec<(String, String, spine::WhereBytes)> = Vec::new();
        for (res, (_, repo, _, _, _)) in results.into_iter().zip(to_extract.iter()) {
            let (rel, path, rows, wheres, dropped) = res?;
            self.dropped += dropped;
            where_bytes.extend(wheres.into_iter().map(|w| (repo.clone(), path.clone(), w)));
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

    fn save_file_meta(&self, current: &FileMeta, _prev: &FileMeta) -> Result<()> {
        let mut rows: Vec<Vec<Value>> = Vec::with_capacity(current.len());
        for ((repo, path, rev), (h, mt, sz)) in current {
            rows.push(vec![
                Value::Text(repo.clone()),
                Value::Text(path.clone()),
                Value::Text(rev.clone()),
                Value::Text(h.clone()),
                Value::Int(*mt),
                Value::Int(*sz),
            ]);
        }
        self.db.exec("DELETE FROM _file")?;
        self.db.insert_rows("_file", &["repo", "path", "rev", "hash", "mtime", "size"], &rows)?;
        self.insert_spine_files(current)?;
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
        let cols: Vec<String> = d.cols.iter()
            .map(|c| format!("\"{}\" {}", c.name, c.ty.sql())).collect();
        let pk: Vec<String> = d.cols.iter().map(|c| format!("\"{}\"", c.name)).collect();
        let sql = format!(
            "CREATE TABLE IF NOT EXISTS {} ({}, __src TEXT DEFAULT '', PRIMARY KEY ({}))",
            tbl(&d.name), cols.join(", "), pk.join(", ")
        );
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
        for item in &prog.items {
            if let Item::Rel(d) = item {
                if BUILTIN_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in relation (repo/rev/content/file); pick another name", d.name);
                }
                if MODULE_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in module-graph relation; pick another name", d.name);
                }
                if TYPE_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in type-graph relation (type_edge / type_edge_rev); pick another name", d.name);
                }
                if SCIP_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in SCIP relation; pick another name", d.name);
                }
                if SPINE_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in ref-spine relation (string / ref); pick another name", d.name);
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
        for d in scip_rel_decls() { self.declare(&d)?; }
        for d in spine_rel_decls() { self.declare(&d)?; }
        Ok(())
    }

    /// Rebuild the built-in relations from the `_file` cache. Wholesale wipe +
    /// repopulate (bounded by repo size, one row per tracked file). Stage 1:
    /// repo = one row from `--root`; rev.id/file.rev = the raw rev string;
    /// content.id = the content hash. No interning (Stage 2).
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
        // been scanned yet.
        let repo_rows: Vec<Vec<Value>> = if !self.repos.is_empty() {
            self.repos.iter().map(|r| {
                vec![t(&r.slug), t(&r.slug), t(&r.root.to_string_lossy())]
            }).collect()
        } else {
            if repo_slugs.is_empty() { repo_slugs.insert(self.self_slug()); }
            repo_slugs.iter().map(|slug| {
                let root = root_of.get(slug).cloned().unwrap_or_default();
                vec![t(slug), t(slug), t(&root)]
            }).collect()
        };
        let revs: Vec<Vec<Value>> = revs.into_values().collect();
        let contents: Vec<Vec<Value>> = contents.into_values().collect();
        self.refresh_rel("repo", &["id", "slug", "root"], &repo_rows)?;
        self.refresh_rel("rev", &["id", "repo", "oid", "ts"], &revs)?;
        self.refresh_rel("content", &["id", "hash"], &contents)?;
        self.refresh_rel("file", &["repo", "rev", "path", "content"], &file_rows)?;
        Ok(())
    }

    /// Project the durable `_strings` / `_where_bytes` meta tables into the
    /// query-facing `string` / `ref` relations. Wholesale wipe + repopulate,
    /// skipping the zero sentinels so queries see only real interned rows.
    fn refresh_spine_rels(&self) -> Result<()> {
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

    /// Wholesale replace one engine-owned relation through the same plural write
    /// seam every built-in module/indexer uses.
    fn refresh_rel(&self, rel: &str, cols: &[&str], rows: &[Vec<Value>]) -> Result<usize> {
        let table = tbl(rel);
        self.db.exec(&format!("DELETE FROM {table}"))?;
        self.db.insert_rows(&table, cols, rows)
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
        let cx = ProjectCx::new(&root, &fileset, &manifests);
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
        let where_rows: Vec<(String, String, spine::WhereBytes)> = rows.spans.iter()
            .map(|(path, _, wb)| (slug.clone(), path.clone(), *wb)).collect();
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

    /// Rebuild the Rust type graph from the `_file` set. This is the same L3
    /// shape as module graph: read tracked Rust files, run a deterministic
    /// syntax extractor, flush one built-in relation through `refresh_rel`.
    fn refresh_type_rels(&self) -> Result<()> {
        let mut files: Vec<(String, String)> = Vec::new();
        {
            let mut sel = self.db.conn().prepare("SELECT path, rev FROM _file WHERE path LIKE '%.rs'")?;
            let rows = sel.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            for row in rows.flatten() { files.push(row); }
        }
        // Parse + extract per file in parallel (same shape as module_rows_for_rev),
        // then flatten and write once. Keeps the cold-build parse working set bounded
        // by the rayon pool, not the corpus (peak-RSS invariant). Rows carry their
        // rev so the type graph is history-aware like module_edge_rev.
        let root = self.root.clone();
        let rows: Vec<Vec<Value>> = files.par_iter().flat_map(|(path, rev)| {
            let t = |s: &str| Value::Text(s.to_string());
            let content = read_content(&root, rev, path).unwrap_or_default();
            typegraph::edges(&content)
                .into_iter()
                .map(|edge| vec![t(&edge.from), t(&edge.to), t(edge.kind), t(rev)])
                .collect::<Vec<_>>()
        }).collect();
        self.refresh_rel("type_edge_rev", &["from", "to", "kind", "rev"], &rows)?;
        self.rebuild_legacy_type_rels()?;
        Ok(())
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

    /// Import compiler-backed SCIP facts from `SPREFA_SCIP_INDEX` or
    /// `<root>/index.scip`. Missing index means empty relations, so programs can
    /// mention SCIP facts without making rust-analyzer a hard runtime dependency.
    fn refresh_scip_rels(&self) -> Result<bool> {
        let t = |s: &str| Value::Text(s.to_string());
        let Some(path) = scip_import::index_path(&self.root) else {
            self.refresh_rel("scip_def", &["symbol", "file"], &[])?;
            self.refresh_rel("scip_ref", &["file", "symbol", "def_file"], &[])?;
            self.refresh_rel("scip_edge", &["src", "dst"], &[])?;
            return Ok(true);
        };
        let rows = scip_import::load(&path)?;
        let defs: Vec<Vec<Value>> = rows.defs.iter().map(|(sym, file)| vec![t(sym), t(file)]).collect();
        let refs: Vec<Vec<Value>> = rows.refs.iter()
            .map(|(file, sym, def)| vec![t(file), t(sym), t(def)]).collect();
        let edges: Vec<Vec<Value>> = rows.edges.iter().map(|(src, dst)| vec![t(src), t(dst)]).collect();
        self.refresh_rel("scip_def", &["symbol", "file"], &defs)?;
        self.refresh_rel("scip_ref", &["file", "symbol", "def_file"], &refs)?;
        self.refresh_rel("scip_edge", &["src", "dst"], &edges)?;
        Ok(true)
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
    fn insert_spine_where_bytes(&self, wheres: &[(String, String, spine::WhereBytes)]) -> Result<usize> {
        if wheres.is_empty() { return Ok(0); }
        let mut by_id: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for (repo, path, w) in wheres {
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
        }
        let rows: Vec<Vec<Value>> = by_id.into_values().collect();
        self.db.insert_rows("_where_bytes", &["id", "string_id", "file_id", "lo", "hi", "repo", "rev", "path"], &rows)
    }

    /// Enumerate (path, hash, mtime, size) for a rev. For WORK, stat each file
    /// and reuse the stored hash when mtime+size are unchanged (the fast-path),
    /// reading+hashing only changed files. A git rev uses the blob OID from
    /// `ls-tree`, so unchanged blobs are detected without fetching content.
    fn enumerate_with_hash(&self, repo: &str, repo_root: &Path, rev: &str, glob: &str, prev: &FileMeta) -> Result<Vec<(String, String, i64, i64)>> {
        let matcher = globset::Glob::new(glob)?.compile_matcher();
        if rev == "WORK" {
            let mut files: Vec<(PathBuf, String, i64, i64)> = Vec::new();
            for entry in ignore::WalkBuilder::new(repo_root).hidden(false).build().flatten() {
                if !entry.path().is_file() { continue; }
                let rel = match entry.path().strip_prefix(repo_root) { Ok(r) => r, Err(_) => continue };
                let rel = rel.to_string_lossy().replace('\\', "/");
                if !matcher.is_match(&rel) { continue; }
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
                if matcher.is_match(path) { out.push((path.to_string(), oid.to_string(), 0, size)); }
            }
            Ok(out)
        }
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
        println!("? {} => {}\t{}", q.head.rel, header(0), header(1));
        let mut n = 0;
        if let Some(&sid) = cc.id.get(seed) {
            let walk = if forward { scc::reaches_from(&cc.cond, sid) } else { scc::reached_by(&cc.cond, sid) };
            let mut hits: Vec<&str> = walk.iter().map(|&i| cc.names[i as usize].as_str()).collect();
            hits.sort_unstable();
            for h in hits {
                if forward { println!("  {seed}\t{h}"); } else { println!("  {h}\t{seed}"); }
                n += 1;
            }
        }
        println!("  ({n} rows)\n");
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
        let (sql, headers) = lower_query(q, &self.rels)?;
        let mut stmt = self.db.conn().prepare(&sql)?;
        let ncols = stmt.column_count();
        let mut rows = stmt.query([])?;
        println!("? {} => {}", q.head.rel, if headers.is_empty() { "(count)".into() } else { headers.join("\t") });
        let mut n = 0;
        while let Some(row) = rows.next()? {
            let cells: Vec<String> = (0..ncols).map(|i| {
                match row.get::<_, rusqlite::types::Value>(i).unwrap_or(rusqlite::types::Value::Null) {
                    rusqlite::types::Value::Text(s) => s,
                    rusqlite::types::Value::Integer(n) => n.to_string(),
                    rusqlite::types::Value::Real(f) => f.to_string(),
                    _ => String::new(),
                }
            }).collect();
            println!("  {}", cells.join("\t"));
            n += 1;
        }
        println!("  ({n} rows)\n");
        Ok(())
    }
}

/// (repo, rev, glob, pathvar, revvar) of a source rule's `scan`. `repo` is the
/// repo coordinate ("." = self repo); resolve it to a root via
/// `Engine::resolve_repo_root`.
fn scan_spec(rule: &Rule) -> Result<(String, String, String, String, String)> {
    for item in &rule.body {
        if let BodyItem::Scan { repo, rev, glob, path, rev_out } = item {
            return Ok((str_of(repo)?, str_of(rev)?, str_of(glob)?, var_of(path)?, var_of(rev_out)?));
        }
    }
    bail!("source rule {} missing scan", rule.head.rel)
}

fn read_content(root: &Path, rev: &str, path: &str) -> Result<String> {
    if rev == "WORK" {
        Ok(std::fs::read_to_string(root.join(path))?)
    } else {
        let output = Command::new("git")
            .arg("-C").arg(root)
            .args(["show", &format!("{rev}:{path}")])
            .output()?;
        if !output.status.success() { bail!("git show failed for {rev}:{path}"); }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
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

/// Parse one file for one source rule (no DB access); returns (rows, dropped).
/// Safe to call in parallel: reads file content, runs extractors, builds rows.
fn parse_file(
    rule: &Rule, repo: &str, path: &str, rev: &str, hash: &str,
    root: &Path, rels: &Rels, rev_index: &HashSet<(String, String, String)>,
) -> Result<(Vec<Vec<Value>>, Vec<spine::WhereBytes>, usize)> {
    let (_, _, _, pathvar, revvar) = scan_spec(rule)?;
    let cmps: Vec<&Constraint> = rule.body.iter()
        .filter_map(|i| if let BodyItem::Cmp(c) = i { Some(c) } else { None }).collect();
    let content = read_content(root, rev, path).unwrap_or_default();
    // Ref-spine: locate each capture's bytes in the file content. The file id is
    // derived from the same stored content address `_files` uses (blake3 for
    // WORK, blob OID for a git rev), so located rows join `_files` for both.
    let where_file = spine::FileId::from_content_address(hash, content.len() as i64)
        .filter(|f| *f != spine::FileId::SYNTHETIC);
    let mut where_bytes: Vec<spine::WhereBytes> = Vec::new();
    let push_span = |text: &str, lo: usize, hi: usize, where_bytes: &mut Vec<spine::WhereBytes>| {
        if let Some(file) = where_file {
            if !text.is_empty() {
                where_bytes.push(spine::WhereBytes {
                    string: spine::StringId::of(text),
                    file,
                    lo: lo as u32,
                    hi: hi as u32,
                    ..Default::default()
                });
            }
        }
    };
    let head_meta = rels.get(&rule.head.rel)
        .ok_or_else(|| anyhow::anyhow!("unknown head relation {}", rule.head.rel))?;
    let mut re_cache: HashMap<String, Regex> = HashMap::new();

    let mut binds: Vec<Bind> = vec![{
        let mut b = Bind::new();
        b.insert(pathvar.clone(), Value::Text(path.to_string()));
        b.insert(revvar.clone(), Value::Text(rev.to_string()));
        b
    }];

    for item in &rule.body {
        match item {
            BodyItem::Match { regex, line, .. } => {
                let mlv = var_of(line)?;
                if !re_cache.contains_key(regex) { re_cache.insert(regex.clone(), Regex::new(regex)?); }
                let re = &re_cache[regex];
                let names: Vec<&str> = re.capture_names().flatten().collect();
                let mut next: Vec<Bind> = Vec::new();
                let base = content.as_ptr() as usize;
                for b in &binds {
                    for (lineno, ln) in content.lines().enumerate() {
                        let line_off = ln.as_ptr() as usize - base;
                        for caps in re.captures_iter(ln) {
                            let mut ext = b.clone();
                            ext.insert(mlv.clone(), Value::Int((lineno + 1) as i64));
                            for n in &names {
                                if let Some(m) = caps.name(n) {
                                    let text = m.as_str();
                                    ext.insert((*n).to_string(), Value::Text(text.to_string()));
                                    push_span(text, line_off + m.start(), line_off + m.end(), &mut where_bytes);
                                }
                            }
                            next.push(ext);
                        }
                    }
                }
                binds = next;
            }
            BodyItem::Ast { lang, query, line, end, .. } => {
                let alv = var_of(line)?;
                let elv = end.as_ref().map(var_of).transpose()?;
                let hits = run_ts(&content, lang, query)?;
                let mut next: Vec<Bind> = Vec::new();
                for b in &binds {
                    for (start, endln, caps) in &hits {
                        let mut ext = b.clone();
                        ext.insert(alv.clone(), Value::Int(*start));
                        if let Some(ev) = &elv { ext.insert(ev.clone(), Value::Int(*endln)); }
                        for (n, t, lo, hi) in caps {
                            ext.insert(n.clone(), Value::Text(t.clone()));
                            push_span(t, *lo, *hi, &mut where_bytes);
                        }
                        next.push(ext);
                    }
                }
                binds = next;
            }
            BodyItem::Sg { lang, pattern, line, col, end_line, end_col, .. } => {
                let slv = var_of(line)?;
                let clv = col.as_ref().map(var_of).transpose()?;
                let ellv = end_line.as_ref().map(var_of).transpose()?;
                let eclv = end_col.as_ref().map(var_of).transpose()?;
                // prefilter: a file lacking any literal token cannot match
                let lits = pattern_literals(pattern);
                if !lits.iter().all(|t| content.contains(t.as_str())) {
                    binds = Vec::new();
                    continue;
                }
                let hits = crate::sg::run_sg(&content, lang, pattern)?;
                let mut next: Vec<Bind> = Vec::new();
                for b in &binds {
                    for (ln, c, eln, ec, caps) in &hits {
                        let mut ext = b.clone();
                        ext.insert(slv.clone(), Value::Int(*ln));
                        if let Some(v) = &clv { ext.insert(v.clone(), Value::Int(*c)); }
                        if let Some(v) = &ellv { ext.insert(v.clone(), Value::Int(*eln)); }
                        if let Some(v) = &eclv { ext.insert(v.clone(), Value::Int(*ec)); }
                        for (n, t, lo, hi) in caps {
                            ext.insert(n.clone(), Value::Text(t.clone()));
                            push_span(t, *lo, *hi, &mut where_bytes);
                        }
                        next.push(ext);
                    }
                }
                binds = next;
            }
            BodyItem::Json { jpath, out, .. } => {
                let ov = var_of(out)?;
                let vals = json_extract(&content, jpath);
                let mut next: Vec<Bind> = Vec::new();
                for b in &binds {
                    for v in &vals {
                        let mut ext = b.clone();
                        ext.insert(ov.clone(), Value::Text(v.clone()));
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

fn val_of(t: &Term, b: &Bind) -> Result<Value> {
    match t {
        Term::Var(v) => b.get(v).cloned().ok_or_else(|| anyhow::anyhow!("unbound var {v} in constraint")),
        Term::Str(s) => Ok(Value::Text(s.clone())),
        Term::Int(n) => Ok(Value::Int(*n)),
        Term::Interp(parts) => interp_value(parts, b),
        Term::Wild => bail!("'_' in constraint"),
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

fn ts_lang(lang: &str) -> Result<tree_sitter::Language> {
    match lang {
        "rust" | "rs" => Ok(tree_sitter::Language::new(tree_sitter_rust::LANGUAGE)),
        "c" => Ok(tree_sitter::Language::new(tree_sitter_c::LANGUAGE)),
        other => bail!("no ast grammar for :{other} (compiled in: rust, c)"),
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

/// Extract leaf values along a dotted path; `*` matches any object key or array index.
fn json_extract(content: &str, jpath: &str) -> Vec<String> {
    let root: serde_json::Value = match serde_json::from_str(content) { Ok(v) => v, Err(_) => return vec![] };
    let mut cur: Vec<&serde_json::Value> = vec![&root];
    for seg in jpath.split('.') {
        let mut next: Vec<&serde_json::Value> = Vec::new();
        for node in cur {
            if seg == "*" {
                match node {
                    serde_json::Value::Object(m) => next.extend(m.values()),
                    serde_json::Value::Array(a) => next.extend(a.iter()),
                    _ => {}
                }
            } else if let serde_json::Value::Object(m) = node {
                if let Some(v) = m.get(seg) { next.push(v); }
            }
        }
        cur = next;
    }
    cur.iter().map(|v| match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }).collect()
}
