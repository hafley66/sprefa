# Multi-repo + off-disk rev coordinate scan

Goal: let a `.dl` program (1) resolve its own repo root from the **nearest `.git`**
instead of always needing `--root`, (2) query across the **config-folder repo set**
like v4, and (3) lazily crawl a **`(repo, rev)` coordinate that is not checked out**
(rev-in-objectdb today; repo-not-cloned later), running rules over the result.

The relations are already repo-shaped (`repo(id,slug,root)`, `rev(id,repo,oid,ts)`,
`file(repo,rev,path,content)` — [engine.rs:59-62](../v5/src/engine.rs#L59)). The gaps
are narrow and known:

| Gap | Site | Size |
|---|---|---|
| `_file` change-cache keyed `(path,rev)`, not `(repo,…)` | `CREATE TABLE _file` [engine.rs:765](../v5/src/engine.rs#L765); `_prov`; `retract_paths`; `load_file_meta` | **M (the real work)** |
| git calls hardcode `-C self.root` | `resolve_rev` [432](../v5/src/engine.rs#L432), `enumerate_with_hash` [1645](../v5/src/engine.rs#L1645), `read_content` [1784](../v5/src/engine.rs#L1784) | S |
| `scan` has no repo arg | 4-ary `scan(REV,GLOB,path,rev)` [parse.rs:136](../v5/src/parse.rs#L136), `scan_spec` [engine.rs:1775](../v5/src/engine.rs#L1775) | S |
| no repo-root directive / nearest-`.git` | CLI `--root` is the only source ([lib.rs:21](../v5/src/lib.rs#L21)) | S |
| no lazy clone of an un-cloned repo | — | S–M, isolated |

## What already works (do not rebuild)

A rev **not checked out** but present in `.git` is already a first-class source:
`scan("v1.2.3", "**/*.rs", p, rev)` → `git ls-tree -r -l <rev>` enumerates without a
working tree, `git show <oid>` reads blobs lazily, `FileId::from_content_address`
dedups against WORK, `check_type` resolves existence against `rev_index` not disk. The
off-disk-rev machinery is ~80% done; the missing axis is **repo**.

## Design (planning-protocol layers)

### 1. Type signatures

```rust
// new module: v5/src/repo.rs  — pure path + git, no engine/DB. Unit-testable alone.
pub enum RepoRoot { Working(PathBuf), Bare(PathBuf) }   // Bare = blobless mirror (Phase 3)
impl RepoRoot { fn git_dir(&self) -> &Path; fn has_worktree(&self) -> bool; }

/// Resolve a repo spec to a root. slug → config root; abs/rel path → that path;
/// "." | "" → `self_root`. url → Bare cache dir (Phase 3).
pub fn resolve_repo(spec: &str, repos: &[RepoConfig], self_root: &Path) -> Result<RepoRoot>;

/// Nearest ancestor of `start` that directly contains `.git`. `start` is the
/// `.dl` file's dir. Walk UP only (the above-.git submodule case is deferred).
pub fn nearest_git(start: &Path) -> Option<PathBuf>;

// NO top-level statement. The default root is the nearest-.git of the .dl file,
// resolved in the CLI (repo::nearest_git); --root overrides. Cross-repo is a
// VALUE that flows into scan, not a directive.

// repo + rev become first-class types (today both degraded to Type::Text on the
// repo/rev builtin relations). file/path/dir are already types.
pub enum Type { Text, Int, Path, File, Dir, Repo, Rev }   // + Repo, Rev

// scan gains an optional leading repo term (back-compat: 4-ary ⇒ self repo).
// The repo term is a repo-typed value: a literal slug/path, or a var bound from
// the `repo(id,slug,root)` relation (config) — so cross-repo composes by join.
BodyItem::Scan { repo: Term, rev: Term, glob: Term, path: Term, rev_out: Term }

// engine methods take a resolved root, not self.root
fn resolve_rev(&mut self, repo: &RepoRoot, rev: &str) -> Result<String>;
fn enumerate_with_hash(&self, repo: &RepoRoot, rev, glob, prev) -> Result<Vec<…>>;
fn read_content(repo: &RepoRoot, rev, path) -> Result<String>;
```

### 2. Crawl pseudo-code (`(repo,rev)=T → tuples`)

```
crawl(R, T, glob):
  root ← resolve_repo(R, repos, self_root)          # Phase 3: clone --bare --filter=blob:none if url+absent
  sha  ← git -C root.git_dir rev-parse T            # resolve_rev
  for (path, oid, size) in git -C root ls-tree -r -l sha:   # tree objects only, no checkout
      if glob.matches(path):
          fid ← FileId::from_content_address(oid)   # content-addressed ⇒ dedup across repo/rev
          emit _file(repo=R.slug, rev=sha, path, hash=oid/blake3)
  # bytes read only when a rule needs them: git -C root show oid
  #   on a blobless Bare clone this triggers the on-demand partial fetch of that one blob
```

### 3. Instance lifetimes

- `RepoRoot` — per-tick value, derived in `reconcile_sources` from each source rule's
  scan repo term; cached in a `HashMap<String, RepoRoot>` on `Engine` for the tick.
- `repos: Vec<RepoConfig>` — already on `Engine`, set via `set_repos` ([426](../v5/src/engine.rs#L426)).
- `self_root` — the `RepoSpec` directive resolves it once at program load (in `run_file`
  etc.), before `Engine::new`. Default stays CLI `--root`.

### 4. Storage layout → reads/writes → uniqueness

- `_file` (change-cache): `PRIMARY KEY(path,rev)` → **`PRIMARY KEY(repo,path,rev)`**;
  add `repo TEXT` column (migrate on open, default `'.'`). `_prov` gains `repo`.
- `rev_index`: `HashSet<(rev,path)>` → `HashSet<(repo,rev,path)>`; `check_type`
  ([1799](../v5/src/engine.rs#L1799)) joins on the triple.
- `retract_paths` / `load_file_meta` / `save` keyed by `(repo,rev,path)`.
- `refresh_builtin_rels` ([1106](../v5/src/engine.rs#L1106)) emits `file`/`repo` rows
  per configured repo, not just `--root`.
- `_files` stays content-addressed (blob OID / blake3) → cross-repo + cross-rev dedup
  is free; no schema change.
- Downstream rev-aware relations (`module_edge_rev`, `type_edge_rev`) gain a `repo`
  column so closures are repo-scoped or cross-repo by join. (Phase 2 tail.)

## Phasing — usable value as early as possible

**Phase 0 — nearest-`.git` default root (S, no re-key) — DONE 2026-06-01.**
DESIGN CORRECTION: the first cut added a `repo nearest.` top-level **statement**
(`Item::Repo(RepoSpec)`). Rejected — it is imperative config that does not compose, and
"nearest .git" is a pure function of a path's parents, i.e. a derivable fact, not a
language statement. **Reverted.** Instead: `repo.rs::nearest_git` is the resolver, and
`--root` (now `Option`) **defaults to the nearest `.git` ancestor of the program file**
(else cwd). So a `.dl` runs location-anchored with no `--root` and no new syntax.
Verified: `dl v5/examples/repo-nearest.dl` from `/tmp` scans the repo. 79/0/1 green.
The repo coordinate proper is a **typed `scan` argument** (Phase 1), defaulting to the
self repo when omitted and naming a slug/var for cross-repo — composes by join, no
keyword. **Immediately usable.**

**Phase 1 — repo coordinate threads (S, the spike, single active repo/tick).**
5-ary `scan(REPO,REV,GLOB,path,rev)` (4-ary ⇒ `"."`); repo-aware `resolve_rev` /
`enumerate_with_hash` / `read_content` take `&RepoRoot`. Still one repo ingested per
tick (no `_file` collision). Proves the coordinate end-to-end against a second config
repo + an off-disk rev. **Usable for "scan rev T of repo R" one repo at a time.**

**Phase 2 — `(repo,rev,path)` re-key (M, the real work).**
`_file`/`_prov`/`retract`/`check_type`/`file_meta` re-keyed; `refresh_builtin_rels`
per-repo; many repos in one db; cross-repo queries. Migrate `_file` in place.

**Phase 3 — lazy off-disk repo (S–M, isolated).**
`RepoRoot::Bare` + `git clone --bare --filter=blob:none <url> ~/.cache/sprefa/<hash>`;
`resolve_repo` handles urls; blob-on-demand via `git show`. The "repo not on disk" crawl.

## Parallelizable vs sequential

- **Parallel (independent files):** `repo.rs` (Phase 0/1 resolution + nearest-git, unit
  tests inline) ∥ `parse.rs`+`lex.rs` grammar (`RepoSpec` directive + 5-ary `scan`,
  parser tests) ∥ doc/example `.dl` using `repo nearest.`.
- **Sequential (all converge on engine.rs):** thread `&RepoRoot` through
  resolve_rev/enumerate/read_content/scan_spec → then the Phase-2 re-key
  (reconcile/retract/refresh/check_type/file_meta) → tests last. Parallel agents would
  conflict on engine.rs here; one writer.

## Open questions / deferred

- **`.dl` above `.git`** (orchestrator repo w/ submodules): nearest-git walks UP, so a
  `.dl` sitting above the `.git`s won't find them. Deferred: a bounded DOWNWARD search
  for child `.git` dirs (submodule roots), or an explicit `repo "sub/path".` list.
- `scan` repo term: config **slug** vs raw **path** vs **url** — slug+path in Phase 1,
  url in Phase 3.
- Cross-repo `--move` (rename in A consumed by B) — out of scope; needs Phase 2 keying.
- Per-repo glob/ignore (a repo's own `.gitignore` for off-disk revs) — `ls-tree`
  already respects committed tree; WORK uses `ignore::WalkBuilder`. Fine.

## Test targets

- `repo.rs`: `nearest_git` (ancestor with `.git`; none; nested), `resolve_repo`
  (slug/path/self).
- `parse.rs`: `repo nearest.` / `repo "slug".` directive; 4-ary and 5-ary `scan`.
- e2e: `scan` a second config repo; `scan` an off-disk tag of the self repo; a `.dl`
  run with no `--root` resolving via nearest-`.git`.
- Phase 2: two repos with colliding `src/lib.rs` both ingested, no collision; retract
  one repo's path leaves the other's intact.
