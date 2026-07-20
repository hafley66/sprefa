# Rev identity as a normalization defect

Branch v11, HEAD 538e7f78. Citations are `git show HEAD:<path>` unless noted.
Authored read-only; transcribed by the coordinator. Under adversarial review.

## 0. Corrections to the brief (reviewers start here)

**Claim 1.** The brief said `"WORK"` appears at "30+ sites across 10 files" in `src/`.
Measured: **25 files** in `src/`; tree-wide `scan("WORK"` count is **535**.
Per tree: `examples/` 189, `tests/` 207, `.dl/` 99, `src/` 33, `std/` 4, `docs/` 3,
`book/` 0, `assets/` 0. Command: `git grep -o 'scan("WORK"' HEAD -- <tree> | wc -l`.

**Claim 2.** The brief cited `src/engine/meta.rs:1102` as the `LIKE 'extract:%'` prefix
site. Wrong file. `meta.rs:1097` is `DELETE FROM _reldigest WHERE rel IN ({holes})`,
a parameterized set delete with no prefix matching. The real prefix-match site is
**`src/engine/extract/mod.rs:1102-1103`**. The diagnosis was right, the attribution
was not. VERIFIED by coordinator: `git grep -n "substr(rel" HEAD -- src/` returns
exactly `src/engine/extract/mod.rs:1103`.

**Claim 3.** The brief said the three single-db plans are "at HEAD". They are not.
`git show HEAD:plans/2026-07-20-single-db-design-a.md` fails ("exists on disk, but not
in HEAD"). They are untracked working-tree files (687 / 625 / 276 lines) written
concurrently this session. Read from disk and treated as unstable. Also: no
"(repo, rev) as one snapshot id" item exists in them; `grep -i snapshot` returns only
`design-a.md:590-592` (an insta snapshot test) and `design-b.md:625` (a shared
read-only engine snapshot). Section 8 answers the ordering question on its merits.

**Claim 4, new defect found.** `src/engine/meta.rs:203` runs
`DELETE FROM _reldigest WHERE key LIKE 'extract:%'`. The table is created at
`meta.rs:212` as `_reldigest (rel TEXT PRIMARY KEY, digest TEXT)`; `meta.rs:166`
confirms the column is `rel`. There is no `key` column. The statement sits inside
`execute_batch_on` in the `strings_is_text` migration arm (`meta.rs:187-205`), which
runs BEFORE the `CREATE TABLE IF NOT EXISTS` at 212.
**VERIFIED by coordinator**: `sqlite3 <root db> "DELETE FROM _reldigest WHERE key LIKE 'x%'"`
returns `Error: no such column: key`. The statement cannot execute. Whether
`execute_batch_on` swallows the error and whether the arm is reachable on a surviving
db are still open.

## 1. The actual defect

One shape, three appearances:

> Several atomic values are folded into one TEXT value by `format!`, and that folded
> value is then used as a key.

Proof it is a normalization defect rather than a naming one: the code is forced into
**string surgery to recover the components**.

```
src/engine/extract/mod.rs:1102-1103
    "DELETE FROM _reldigest WHERE rel LIKE '{prefix}%' \
     AND substr(rel, {}) NOT IN (SELECT rev FROM _live_rev_scope)"
```

`LIKE 'prefix%'` plus `substr(rel, N)` is a hand-rolled column projection over a key
that should have had columns. With `family` and `rev` as real columns it becomes
`DELETE FROM _extract_digest WHERE family = ?1 AND rev NOT IN (...)`, an index seek
instead of a full scan with a per-row `substr`.

Already a registered rail: `.dl/composite-key-string.dl` states the law and
grandfathers this offender at `composite_key_baseline("src/engine/extract/mod.rs", 1)`.
Its incident block records `salt_rev`: 314,892 of 939,845 `_strings` rows (33.5%,
15MB) existed only to hold one folded form, and `rel_df_node` / `rel_df_node_rev` had
zero joinable rows on `id`.

### One arc or three?

**One arc in law, three in delivery.**

*One in law*: all three are the same rail violation with the same failure mode, that a
component becomes unrecoverable without parsing and equality on the fold is not
equality on the components.

*Three in delivery*, because the coupling is asymmetric:

- **Rev identity and the digest keys are one change.** `extract_digest_key` folds the
  rev, so normalizing the key and resolving the alias touch the same 8 lines
  (`extract/mod.rs:87-89`, `1096-1105`). Splitting them writes the sweep twice.
- **`mint_sym` must NOT be attached.** A prior agent established it is not landable
  inside `{typegraph/mod.rs, ts/flow.rs, decls.rs}`: the stored repo-qualified sym is
  minted at `src/engine/extract/type_rels.rs:201` (`format!("{repo}::{}", ent.sym)`)
  plus ~200 lines of string-keyed name resolution. Independently,
  `.dl/composite-key-string.dl`'s arm-2 comment records that `::` was **measured and
  dropped** from the rail because 37 of 39 hits were the engine's own accepted sym
  convention. Attaching `mint_sym` requires first re-litigating that rail decision.

Recommendation: land rev + digest keys as Steps 1-7. Leave
`composite_key_baseline("src/graph/typegraph/mod.rs", 4)` alone. Open `mint_sym` as a
successor plan.

## 2. The `"WORK"` inventory, classified

### 2a. Control flow (must change)

| Site | Shape | Class |
|---|---|---|
| `src/engine/repo.rs:8-9` | `if rev == "WORK" { return Ok("WORK") }` | **THE ALIAS LEAK. Resolve here.** |
| `src/engine/eval.rs:612` | fs read vs temp materialization for `cmd`'s `{file}` | worktree predicate |
| `src/engine/mod.rs:1090` | `read_content`: fs vs `git_batch_read` | worktree predicate |
| `src/engine/mod.rs:1217` | type-check dispatch: `rev_index` vs fs stat | worktree predicate |
| `src/engine/scan.rs:137` | `enumerate_with_hash`: `ignore::WalkBuilder` vs git tree | worktree predicate |
| `src/engine/scan.rs:188` | `prev.get(&(repo, rel, "WORK"))` cache probe | resolve-before-compare |
| `src/engine/extract/call.rs:29` | `.filter(\|file\| file.2 == "WORK")` | worktree predicate |
| `src/engine/extract/call.rs:43,44,103,104,108,355,356` | digest + dependency digests | **cache key** |
| `src/engine/extract/call.rs:216` | `if rev == "WORK"` | worktree predicate |
| `src/engine/extract/mod.rs:743` | `if rev != "WORK"` | worktree predicate |
| `src/engine/extract/mod.rs:927` | `if with_scip && rev == "WORK"` | worktree predicate |
| `src/engine/extract/type_rels.rs:109` | `if rev == "WORK"` | worktree predicate |
| `src/engine/path_reconcile.rs:36,63` | rev-index keys | worktree predicate |
| `src/engine/path_reconcile.rs:108,112,116,121,122` | filters | resolve-before-compare |
| `src/engine/reconcile.rs:74-75,164-165` | `rev_short` display | display only |
| `src/daemon/root.rs:530` | `if s.as_str() != "WORK" && !names.contains(s)` | resolve-before-compare |
| `src/lib.rs:810` | same alias test over `BodyItem::Scan` | resolve-before-compare |
| `src/parse/ops.rs:59,81,83` | `Term::Str("WORK")` parse default | **surface default, KEEP** |
| `src/rels/extract_family.rs:217,219` | `refresh_module_rels_for_revs(&["WORK"])` | resolve-before-compare |
| `src/storage/call.rs:319` | `StringId::of("WORK").sqlite()` | resolve-before-compare |
| `src/engine/tick.rs:1395,1402` | dependency digests | cache key |
| `src/engine/lens.rs:523,921,1102` | `read_content(&root, "WORK", path)` | resolve-before-compare |

### 2b. Doc comments and embedded `.dl` snippets (do not change)

`src/ast.rs:451`, `src/engine/extract/mod.rs:822`, `src/engine/extract/scip_narrow.rs:79`,
`src/engine/decls.rs:221,502`, `src/engine/repo.rs:273`. Snippets: `src/anchor.rs:628-638`
(11), `src/verbs.rs:44-49` (6), `src/cancel.rs:119`, `src/engine/derive.rs:2553`,
`src/engine/path_reconcile.rs:236-237`, `src/engine/source_prepare.rs:602-604,662-666,698-704`.

### 2c. Test fixtures

`src/engine/extract/verdict_tests.rs` (23), `src/storage/call.rs` tests (14),
`src/engine/source_prepare.rs` (21), `src/parse/mod.rs:1033-1069` (4), plus 79 files
under `tests/`. Section 9.

### 2d. Surface syntax: SETTLED BY USER RULING

`WORK` survives as an alias. All 535 `scan("WORK", ...)` sites unchanged. No user
program migrates. `src/parse/ops.rs:59,81,83` keep emitting `Term::Str("WORK")` as the
omitted-rev default.

## 3. Layer 1 of 4: type signatures

### 3.1 The resolution seam ALREADY EXISTS

The most important finding. **`Engine::resolve_rev` at `src/engine/repo.rs:7` is
already the single funnel every scan's rev literal passes through.** Its only two
callers are `repo.rs:291` and `repo.rs:363`, the two `ScanBinding` construction paths.
Both assign the result to `ScanBinding.rev`.

```rust
// src/engine/repo.rs:7 — CURRENT
pub(crate) fn resolve_rev(&mut self, repo_root: &Path, rev: &str) -> Result<String> {
    if rev == "WORK" { return Ok("WORK".to_string()); }   // :8-9  THE LEAK
    ...
}
```

Those two lines are the entire alias leak. Everything downstream of `ScanBinding.rev`
(`_file.rev`, `rel_rev.id`, `rel_rev.oid`, every `_rev` twin's `rev`, every digest key)
receives the alias because of this early return. Non-WORK revs already resolve to a sha
via `Self::rev_parse` at `repo.rs:23`.

```rust
/// The resolved identity of a repo's working tree.
/// Lifetime: VALUE type, no state. Built per resolution, copied into
/// ScanBinding, dropped with it.
pub struct RevId { oid: String, dirty: bool, from_alias: bool }

impl RevId {
    /// Stored form: "<sha>" or "<sha>+".
    // if self.dirty { format!("{}+", self.oid) } else { self.oid.clone() }
    pub fn text(&self) -> String;

    /// THE worktree predicate. Replaces every `rev == "WORK"` classified
    /// "worktree predicate" in 2a.
    // self.from_alias
    pub fn is_worktree(&self) -> bool;

    /// The ONLY string surgery permitted in this design, allowed because the
    /// storage form is a display encoding read at exactly one place (the `rev`
    /// rel writer), never a key component.
    // let (oid, dirty) = match text.strip_suffix('+') {
    //     Some(base) => (base, true), None => (text, false) };
    // if oid.len() == 40 && oid.chars().all(|ch| ch.is_ascii_hexdigit())
    //     { Some(RevId { oid: oid.into(), dirty, from_alias: false }) } else { None }
    pub fn parse(text: &str) -> Option<RevId>;
}
```

**Design answer on `is_worktree`, not deferred.** It cannot be `rev.oid == head_sha`,
because a user may write `scan("HEAD", ...)` meaning the committed tree, which on a
clean tree resolves to the same sha while requiring the `git_batch_read` path rather
than the fs path. The two must stay distinguishable. So `from_alias` is a
**resolution-time carried boolean**, true only when `resolve_rev` was called with
`"WORK"`. Correct to carry in memory, wrong to store. Recorded as a layer disagreement
in section 5.

### 3.2 The rev cache

```rust
/// Lifetime: ONE per Engine, lives as long as the Engine. Field on Engine,
/// sibling to the existing rev_cache / rev_sha_cache (read at repo.rs:15-19).
/// Invalidated by a daemon git event.
pub(crate) struct WorktreeRev { cached: HashMap<PathBuf, (RevId, u64)> }

impl WorktreeRev {
    // hit for this tick -> return clone
    // miss -> oid = Engine::rev_parse(repo_root, "HEAD")?
    //         dirty = Self::probe_dirty(repo_root)?
    //         RevId { oid, dirty, from_alias: true }, cache at (repo_root, tick)
    pub(crate) fn resolve(&mut self, repo_root: &Path, tick: u64) -> Result<RevId>;

    // tracked: `git diff-index --quiet HEAD --`, exit code only, no stdout parse
    //   -> non-success means dirty, return early
    // untracked: `git ls-files --others --exclude-standard`, non-empty means dirty
    //   (diff-index cannot see untracked files)
    fn probe_dirty(repo_root: &Path) -> Result<bool>;

    /// Called from ServedRoot::on_git_event (src/daemon/root.rs:541).
    pub(crate) fn invalidate(&mut self);
}
```

**Non-repo repos.** `resolve` fails when `repo_root` has no HEAD. Today `resolve_rev`
returns `"WORK"` unconditionally so scanning a non-git directory works. Real behavior
change. Decision: **no HEAD resolves to the sentinel**
`RevId { oid: "0".repeat(40), dirty: true, from_alias: true }`, the all-zero oid git
itself uses for "no object". 40 hex chars, so it parses, sorts, and interns like any
other rev, and never collides with a real commit. Exercised by
`tests/it/worktree_cold_check.rs` and `src/engine/source_prepare.rs`'s non-git fixtures.

### 3.3 The digest key, normalized

```rust
/// REPLACES extract_digest_key (src/engine/extract/mod.rs:87-89).
pub(crate) struct DigestKey {
    exe: i64,      // exe_stamp() folded to i64; mod.rs:58-77 unchanged
    family: i64,   // StringId::of(family).sqlite(), FK to _strings.id
    rev: i64,      // StringId::of(rev_id.text()).sqlite(), FK to _strings.id
}
```

Nothing interpolated. Each component an interned integer. Recovering a component is a
column read.

## 4. Layer 2 of 4: storage

### 4.1 New `_extract_digest`

Current (`src/engine/meta.rs:212`): `CREATE TABLE IF NOT EXISTS _reldigest (rel TEXT PRIMARY KEY, digest TEXT);`

`_reldigest` carries at least six key namespaces folded into that one column:
`extract:` (`extract/mod.rs:826`), `src:` (`meta.rs:935`), `async:` and `hook:`
(`tick.rs:741,758`), `shape:` (`meta.rs:768`), `scip:` (`rels/scip.rs:208`).
Normalizing all of them is a much larger change than this arc justifies.
**Split the `extract:` namespace into its own table; leave the rest.**

```sql
CREATE TABLE IF NOT EXISTS _extract_digest (
    exe    INTEGER NOT NULL,
    family INTEGER NOT NULL REFERENCES _strings(id),
    rev    INTEGER NOT NULL REFERENCES _strings(id),
    digest TEXT    NOT NULL,
    PRIMARY KEY (exe, family, rev)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx__extract_digest_rev ON _extract_digest(rev);
```

- **WITHOUT ROWID is legal and unconstrained here.** `wants_without_rowid`
  (`src/engine/declare.rs:204-209`) requires `pk_never_null && key.is_none() &&
  (2..=4).contains(&cols.len()) && all INTEGER`. This table is created by
  `execute_batch_on` in `ensure_meta`, never through `declare`, so the 2..=4 cap never
  sees it. That cap DOES constrain declared rels, which is why Step 6 widens no
  declared rel's PK.
- **The `_txt` decode pattern does not apply.** `create_rel_view`
  (`declare.rs:120-134`) builds `rel_<name>_txt` views only for rels declared through
  `declare`, using the correlated subquery form. `_extract_digest` is engine-internal
  skip state no `.dl` program reads. If a debug view is wanted later, use the
  correlated subquery, never the LEFT JOIN.
- **Foreign keys may be declarative only.** `PRAGMA foreign_keys` state UNVERIFIED. If
  off, `REFERENCES` documents intent without enforcing, still strictly better than a
  fold. Step 3 validates the constraint by query rather than by pragma.

### 4.2 The gone-rev sweep after normalization

`src/engine/extract/mod.rs:1096-1105` becomes:

```rust
self.db.exec(
    "DELETE FROM _extract_digest \
     WHERE exe = ?1 \
       AND rev NOT IN (SELECT sprf_sym(rev) FROM _live_rev_scope)",
)?;
```

The `for family in [...]` loop at `extract/mod.rs:1100` disappears entirely; family was
only ever iterated to build a prefix. This is the payoff and the reviewable proof the
change is real. `_live_rev_scope` (`extract/mod.rs:1070-1072`) is `(rev TEXT PRIMARY KEY)`
from `SELECT DISTINCT rev FROM _file`. The `sprf_sym(rev)` wrapper is the established
idiom at `extract/mod.rs:1082` and `src/storage/call.rs:768`.

### 4.3 Invariant and enforcement

> **INV-1: no rev column in any table ever holds an alias.** Every value in
> `_file.rev`, `rel_rev.id`, `rel_rev.oid`, and every twin's `rev` matches
> `^[0-9a-f]{40}\+?$`.

Enforcement, because an unenforced invariant is how this happened:

1. **Structural**: `resolve_rev` returns `RevId`, not `String`. `ScanBinding.rev`
   becomes `RevId`. Alias text has no type that can reach storage; `RevId` is only
   constructible from `rev_parse` output or the zero sentinel.
2. **Runtime**: debug-assert in the `rev` rel writer rejecting non-conforming text.
3. **Rail**: a `.dl` program asserting zero rows where the pattern fails.
   UNCERTAIN whether dl's `!` negation composes with `=~`; Step 7 falls back to a Rust
   integration test.

## 5. Layer 3 of 4: sequence, and where the layers disagree

Per tick, per repo: tick counter increments; a scan rule reaches
`resolve_scan_bindings` (`repo.rs:270-300` literal, `:320-372` bound);
`resolve_rev` calls `WorktreeRev::resolve(repo_root, tick)`; cache hit returns, miss
runs `git rev-parse HEAD` then `probe_dirty`; `ScanBinding.rev = RevId`;
`enumerate_with_hash` (`scan.rs:130-137`) branches on `is_worktree()` rather than a
string compare; `_file` rows written with `text()`; extraction computes `DigestKey` and
does one indexed lookup; `sweep_gone_revs` runs the single DELETE.

**Disagreement 1: `from_alias` exists in the type layer, not the storage layer.**
`RevId` carries three fields, stored text carries two. Intentional. `from_alias`
distinguishes `scan("WORK")` from `scan("HEAD")` on a clean tree, a READ-PATH
distinction (fs vs `git_batch_read`) with no bearing on what the rows are.
**Storage is authoritative on what a row means; the type layer on how it was read.**
Storing `from_alias` would be a fourth fold.

**Disagreement 2: sequence resolves per tick, uniqueness wants per resolution.** A tick
spanning a `git commit` uses the pre-commit sha throughout. **Sequence is
authoritative.** A tick is the engine's consistency unit; re-resolving mid-tick yields
`_file` rows straddling two revs, worse than one tick stale. `on_git_event`
(`daemon/root.rs:541`) already triggers the next tick.

**Disagreement 3: `is_worktree` versus the `+` marker.** They look like the same fact
and are not. Section 6.

## 6. Layer 4 of 4: uniqueness, and what `+` may not be

`(exe, family, rev)` uniquely identifies an `_extract_digest` row. `(repo, path, rev)`
identifies a `_file` row (already the PK at `meta.rs:210`). `(id, rev)` identifies a
`df_node_rev` row (already at `decls.rs:624`).

**What `+` is**: a one-bit statement that the working tree differed from `oid` at
resolution time. A disambiguator keeping a dirty tree's rows from being mistaken for the
committed tree's.

**What `+` is FORBIDDEN to be**: `<sha>+` is not content-addressed and must never be a
cache key on its own. A dirty tree changes on every keystroke while `<sha>+` does not
move. Any cache treating `<sha>+` as sufficient identity returns stale rows.

Confirmed the real invalidation is elsewhere and survives:
`_file` stores a per-file content `hash` (`meta.rs:210`) refreshed by
`enumerate_with_hash` (`scan.rs:130-200`), which re-reads and re-hashes any file whose
`(mtime, size)` moved (`scan.rs:184-196`); `extract_input_digest` (`extract/mod.rs:831`)
folds file content hashes into the digest VALUE, so the rev is a key NAMESPACE and the
hashes are the value; `cached_facts` (`extract/mod.rs:98`) keys on
`(repo, path, content hash)` per `mod.rs:100-101`.

**Explicit prohibition.** No code may compare two `<sha>+` values and conclude the trees
are identical, skip work because a stored rev equals a computed rev ending in `+`, or
persist a `+` rev as immutable content identity in `rev_sha_cache` (`repo.rs:16`, the
CROSS-TICK cache). Dirty revs belong in the per-tick cache only. Live hazard:
`cache_rev` (`repo.rs:24`) chooses between the two caches, so Step 2 must route every
`from_alias` resolution to the per-tick cache regardless of dirtiness.

## 7. Build versus buy: reading git state

**Current state**: the repo shells out to `git` everywhere, with NO git library
dependency. `git show HEAD:Cargo.toml | grep -i 'git2\|gix'` returns only comments.
Existing shell-outs: `src/rels/git.rs:181` (`rev-parse HEAD`), `git.rs:36,110`
(`status --porcelain -uall`), `git.rs:96` (`diff -U0 HEAD`), `git.rs:194`
(`for-each-ref`), `src/engine/repo.rs:99-107` (`rev_parse`), `repo.rs:975`
(`observe_ref`), `src/rels/mod.rs:175` (`rev-parse --show-toplevel`),
`src/daemon/shell/watch.rs:272` (`rev-parse --git-dir`).

| Candidate | Verdict |
|---|---|
| **A: `git2`** (libgit2 bindings) | Mature, widely used, `Repository::head()` and `statuses()` cover both needs. Costs: links libgit2 (C dependency, vendored or system), materially larger build time and binary, historical divergences from git's own gitignore and index semantics. REJECTED on dependency weight for a job the repo already does eight other ways. |
| **B: `gix`** (gitoxide) | Pure Rust, no C dependency, actively developed, `gix-status` is fast. Costs: dozens of `gix-*` sub-crates, an API that has been moving, would be the largest dependency in the project. Strongest on technical merit, weakest on proportionality. REJECTED. |
| **C: shell out to `git`** | Costs a subprocess spawn per call. Benefits: zero new dependencies, exact semantic agreement with `changed` / `changed_line` / `git_ref` which already shell out, no divergence risk between the dirty bit and what `changed(path)` reports. **PICKED.** |

The deciding argument is semantic agreement, not spawn cost. A dirty bit from `gix`
against a `changed` rel from `git status` would let a user observe a dirty rev with an
empty `changed` relation, and reconciling that costs more than the dependency saves.

**Measured** (1,515 tracked files, three runs, warm cache, `/usr/bin/time -p`):

| Command | real |
|---|---|
| `git rev-parse HEAD` | 0.00s x3 |
| `git diff-index --quiet HEAD --` | 0.01s x3 |
| `git status --porcelain -uall` | 0.13s, 0.08s, 0.08s |

`diff-index` is ~8x cheaper than `status` because it skips untracked enumeration. It
also MISSES untracked files, so `probe_dirty` needs the `ls-files --others
--exclude-standard` second step. That term is **UNMEASURED**; expected total under
0.02s per repo per tick against 0.08s for naive `git status`.

**Why `last_changed_paths` is not sufficient.** `ServedRoot.last_changed_paths`
(`daemon/root.rs:39`, written `:205`, cleared `:159`) is a change EVENT, not a dirty
STATE: empty on a quiet tick over a dirty tree, and silent on whether a changed file
differs from HEAD. Correct as an invalidation TRIGGER for `WorktreeRev::cached`, wrong
as the dirty bit. Step 4 uses it in exactly that role.

## 8. Ordering against the single-db work

Verified in the untracked plans: `inventory.md:108-111` (the `_rev` twin is baked into
the declared rel NAME in `decls.rs`, not applied by a helper); `inventory.md:155-158`
(32 rels carry `repo`, including `rev` and `file`); `inventory.md:253` (existing
composite-tuple predicates `WHERE (repo, path) IN ...` at `meta.rs:1293`).

The cross-repo collision is real and its mechanism verified: `StringId::of` is a content
hash (`src/spine.rs:52-57`), so `"WORK"` interns to the identical i64 in every database.
Under a single-database collapse two repos' working trees would share one rev id.

**This change lands BEFORE the single-db collapse.**

1. It removes a blocker rather than adding one. The collision must be fixed before the
   collapse merges databases, or the collapse itself introduces the bug.
2. It shrinks the collapse's surface: deleting the `LIKE`/`substr` sweep removes one
   string-surgery site the collapse would otherwise carry across.
3. It is independently valuable and independently testable; every step validates with
   no single-db work existing.

**On snapshot interning**: yes, a `(repo, rev)` dimension is still worth doing and is
MORE attractive after this change. With rev as an alias the dimension was degenerate
(one `WORK` row per repo, colliding across repos). With rev as a resolved sha the pair
is genuinely two-dimensional and replaces two interned columns with one on the 13
`REV_TWINS` tables. It is a SUCCESSOR, because it touches every twin's declared column
list and `wants_without_rowid`'s 2..=4 cap means narrowing a PK from `(id, rev)` to
`(snapshot)` changes rowid-mode eligibility on tables that currently qualify. That
interacts with the collapse's storage decisions and should be decided there.

## 9. Test blast radius

**Scale**: 79 test files contain the literal `"WORK"`; 207 `scan("WORK"` under
`tests/`. Heaviest: `extract_cache.rs` (50), `dataflow.rs` (39), `graph_diff_rev.rs`
(21), `module_graph.rs` (19), `storage_diet_index.rs` (12), `call_golden.rs` (11).
Fixtures `tests/fixtures/call_golden/call_def_rev.tsv` (7 rows) and
`call_edge_rev.tsv` (3) contain literal `WORK` in a rev column.

- **Do not break (the majority)**: every `scan("WORK", ...)` in a test program is
  surface syntax, unchanged by the ruling.
- **Break: golden fixtures with a rev column.** The two `.tsv` files hold `WORK` as
  DATA. Under INV-1 these become a sha, so they need the determinism seam or become
  untestable, since a golden file cannot contain the developer's HEAD.
- **Break: tests asserting a stored rev value.** `graph_diff_rev.rs`,
  `daemon_stateful_revs.rs`, `storage_diet_index.rs`, `spine_meta.rs`.
- **Break: in-src unit tests constructing rows.** `src/storage/call.rs:951-1248`
  (14 sites, including `assert_eq!(rev_sid, StringId::of("WORK").sqlite())` at `:1248`),
  `src/engine/source_prepare.rs` (21), `src/engine/extract/verdict_tests.rs` (23).

### The false-green risk, first class

**A test suite whose result depends on working-tree cleanliness is a new false-green
class.** Without a seam, `extract_cache.rs`'s 50 assertions become sensitive to whether
the developer has uncommitted changes, with the bad failure mode: green on a clean tree
in CI, red on a dirty tree locally, or the reverse. HEAD 538e7f78 is literally the
commit that audited 1,532 tests for false greens, so shipping this without the seam
would undo part of that work in the same week.

**The seam, with precedent.** `exe_stamp` already has this exact override at
`src/engine/extract/mod.rs:63-66`:

```rust
if let Ok(s) = std::env::var("DL_EXE_STAMP") {
    return u128::from_le_bytes(blake3::hash(s.as_bytes()).as_bytes()[..16].try_into().unwrap());
}
```

commented "a distinct value stands in for a distinct binary so the digest-namespace
behavior is checkable without a real reinstall". Mirror it exactly with
`DL_REV_OVERRIDE`, parsed through `RevId::parse` with `from_alias` forced true, erroring
on anything not 40 hex chars with an optional `+`. Tests set 40 zeros (or 40 zeros `+`)
and become fully deterministic; the golden `.tsv` fixtures get the sentinel written in.
This makes cleanliness-dependence structurally impossible rather than merely unlikely.

**UNCERTAIN**: whether `tests/it/` sets env per test or shares a process. If shared,
`DL_REV_OVERRIDE` must be set once in `tests/it/main.rs` rather than per test.

## 10. Landable steps

**Step 1. Add `RevId` and `WorktreeRev`, wired to nothing.**
Files: new `src/engine/revid.rs`; `src/engine/mod.rs` (`mod revid;`).
Shape: the section 3 types plus the `DL_REV_OVERRIDE` hook. Unit tests for `parse`
round-tripping clean, dirty, the zero sentinel, and REJECTING `"WORK"`.
Validate: `cargo test --lib revid`

**Step 2. Resolve the alias at the seam.**
Files: `src/engine/repo.rs` (replace `:8-9`; `resolve_rev` returns `Result<RevId>`),
`src/engine/mod.rs` (`worktree_rev: WorktreeRev` field on `Engine`), `repo.rs:291,363`,
plus whatever `ScanBinding.rev`'s type change reaches.
Shape: WORK arm calls `self.worktree_rev.resolve(repo_root, self.tick)`; non-WORK arm
wraps its existing `rev_parse` sha as `RevId { oid, dirty: false, from_alias: false }`.
Route `from_alias` resolutions to the per-tick cache only (section 6). Add the no-HEAD
zero sentinel.
Validate: `cargo test --test it -- repo_sink resolver_repo_scope config_repos worktree_cold_check`

**Step 3. Convert worktree predicates to `is_worktree`.**
Files, exact: `eval.rs:612`, `mod.rs:1090`, `mod.rs:1217`, `scan.rs:137`,
`extract/call.rs:29,216`, `extract/mod.rs:743,927`, `extract/type_rels.rs:109`,
`path_reconcile.rs:36,63`.
Shape: each `rev == "WORK"` becomes `rev_id.is_worktree()`; `!=` becomes `!`. Widen
`rev: &str` signatures to `rev: &RevId`. Do not touch 2b.
Validate: `cargo test --test it -- cmd_op scan_kwargs path_types builtin_file_rel`

**Step 4. Invalidate on a git event.**
Files: `src/daemon/root.rs` (`on_git_event`, near `:541`).
Shape: `worktree_rev.invalidate()` at the top, before `watched_ref_names()`. Separately
fix `root.rs:530` and `src/lib.rs:810` to compare against a shared
`pub const WORK_ALIAS: &str = "WORK";`, since both correctly read PROGRAM TEXT rather
than stored data and the alias should have exactly one definition.
Validate: `cargo test --test it -- daemon daemon_stateful_revs rule_edit`

**Step 5. Normalize the digest key. THE NORMALIZATION PAYLOAD.**
Files: `src/engine/meta.rs` (add the 4.1 CREATE to `ensure_meta` near `:212`),
`src/engine/extract/mod.rs` (delete `extract_digest_key` `:87-89`, add `DigestKey`,
rewrite the extract-namespace `load_rel_digest`/`save_rel_digest` call sites, rewrite
`:1096-1105` to the single DELETE, DELETING the `for family` loop),
`src/engine/extract/call.rs:43,44,108`.
Shape: the `substr` at `:1103` and the `LIKE '{prefix}%'` at `:1102` are DELETED, not
rewritten. Also fix Claim 4: `meta.rs:203`'s `WHERE key LIKE` becomes
`DROP TABLE IF EXISTS _extract_digest`, since the arm's intent was clearing extraction
skip state and the new table is the right target. Bump `SCHEMA_EPOCH` here (section 12).
Validate: `cargo test --test it -- extract_cache extract_digest_namespace digest_skip derived_skip tick_digest`,
then `git grep -n 'substr(rel' src/ | wc -l` returning 0.

**Step 6. Delete the rail baseline row.**
Files: `.dl/composite-key-string.dl`. Delete
`composite_key_baseline("src/engine/extract/mod.rs", 1).` per the rail's own ratchet law,
and note above the block that `extract_digest_key` was normalized rather than waived.
Leave the other rows; they are the `mint_sym` arc.
Validate: `dl .dl/composite-key-string.dl --no-daemon` shows zero findings for that file.

**Step 7. Enforce INV-1.**
Files: new `.dl/rev-alias-leak.dl` (or a Rust test if negation-plus-regex does not
compose); `src/engine/decls.rs` if the writer needs the debug-assert.
Validate: `dl .dl/rev-alias-leak.dl --check`, plus a post-rebuild
`SELECT count(*) FROM rel_rev_txt WHERE oid = 'WORK'` returning 0.

**Step 8. Fixtures and the determinism seam.**
Files: `tests/it/main.rs` (set `DL_REV_OVERRIDE` suite-wide),
`tests/fixtures/call_golden/call_def_rev.tsv`, `call_edge_rev.tsv`, plus the row
constructors at `src/storage/call.rs:951-1248`, `src/engine/source_prepare.rs`,
`src/engine/extract/verdict_tests.rs`.
Validate: `cargo test --workspace --no-fail-fast`, then the check that matters: run the
suite once on a clean tree and once with an uncommitted scratch edit, confirm identical
results.

## 11. Row growth and retention

**Measured** (`~/projects/smashy-codex-gwell/.dl/cache.db`):

| Table | rows | distinct revs |
|---|---|---|
| `rel_type_entity_rev` | 3,104 | 1 |
| `rel_type_link_rev` | 2,901 | 1 |
| `rel_call_def_rev` | 2,482 | 1 |
| `rel_module_edge_rev` | 837 | 1 |
| `rel_const_value_rev` | 15 | 1 |
| `rel_df_*_rev` (5 tables) | 0 | 0 |
| **total** | **9,339** | **1** |

One rev costs ~9,339 twin rows on that repo. Today `WORK` is overwritten in place so the
count is flat forever. After the change every commit ticked at mints a new rev; 20
commits would be ~187,000 twin rows. UNCERTAIN: that repo has zero dataflow rows, and
`df_node_rev` is the largest twin on repos that do extract dataflow, so this understates
a dataflow-heavy repo by an unknown factor.

**The retention mechanism already exists and this change ACTIVATES it.**
`sweep_gone_revs` (`extract/mod.rs:1032-1105`) deletes twin rows whose rev is absent
from `_live_rev_scope` = `SELECT DISTINCT rev FROM _file` (`:1072`). Under the alias
model the sweep is inert for the working tree because `WORK` is always live. Under the
sha model, yesterday's HEAD stops being scanned the moment HEAD moves, `_file` drops it,
and the sweep collects it on the same tick. Steady state is one rev per repo plus
whatever revs user programs explicitly scan.

`REV_TWINS` (`extract/mod.rs:1032-1037`) lists 13 tables; the live db has 15
`rel_*_rev` tables. The two extra, `rel_call_def_rev` and `rel_call_edge_rev`, are swept
by `sweep_gone_call_inputs` (`extract/mod.rs:1108`, `GONE` predicate at
`storage/call.rs:768`). UNVERIFIED that this second path is rev-scoped; the constant was
read, the full query was not.

**Retention needs no new policy and lands in the same change**, because Step 5 already
rewrites the sweep. What Step 5 must NOT do is weaken it.

**`_strings` GC is the same arc.** Each new rev interns one 40-or-41-char string,
negligible alone. The twin rows referencing it are not, and `salt_rev` is the precedent:
314,892 of 939,845 `_strings` rows orphaned from a rev-folded key. The sweep deletes twin
ROWS and does not delete the orphaned `_strings` entries they referenced, so **`_strings`
GC is the missing second half of `sweep_gone_revs`.** It should FOLLOW rather than block,
since the sweep's correctness does not depend on it, but it is now on the critical path
for disk growth rather than cosmetic.

## 12. Migration

1. **Cold rebuild (already queued)** deletes every db; migration moot for anyone taking it.
2. **`SCHEMA_EPOCH` bump.** `ensure_meta` (`meta.rs:150-179`) already drops every `rel_%`
   table and clears `_reldigest` when `user_version != SCHEMA_EPOCH`. **This is the
   correct mechanism**: bump it in Step 5. Every existing db self-clears on first tick
   with the new binary, no bespoke migration code.
3. **Upgrading without a cold rebuild.** With the epoch bump: exactly one full re-extract
   on the first tick, correct rows after. Without it: a stranded `WORK` rev (never again
   in `_live_rev_scope`, so the sweep collects it correctly) plus stale
   `extract:...:WORK` keys in the old `_reldigest`, harmless since Step 5 reads a
   different table, but leaking disk forever.

**Answer: bump `SCHEMA_EPOCH` in Step 5 and write no migration code.** `exe_stamp`
already guarantees a new binary sees an empty digest namespace and does one cold rebuild,
so this costs one extra rebuild the binary change was going to cost anyway.

## 13. Risks and guesses (reviewer targets)

1. **GUESS, NOW VERIFIED BY COORDINATOR**: `meta.rs:203`'s `WHERE key LIKE` is dead or
   erroring. `sqlite3 <db> "DELETE FROM _reldigest WHERE key LIKE 'x%'"` returns
   `no such column: key`. Whether `execute_batch_on` swallows it, and whether the arm is
   reachable, remain open.
2. **GUESS**: the `.dl` negation-plus-regex rail in 4.3 compiles. dl was not run. Step 7
   has a stated fallback.
3. **UNMEASURED**: `git ls-files --others --exclude-standard` cost, the one unmeasured
   term in the dirty-probe budget.
4. **UNVERIFIED**: `PRAGMA foreign_keys` state; determines whether 4.1's `REFERENCES`
   enforce or document.
5. **UNVERIFIED**: test process and env sharing; determines where `DL_REV_OVERRIDE` is set.
6. **UNVERIFIED**: `sweep_gone_call_inputs` rev scoping.
7. **COULD NOT MEASURE**: sprefa root db row counts; `.dl/.state/cache.db` was 0 bytes
   during authoring. Growth figures come from the gwell db, which has zero dataflow rows
   and likely understates the largest twins.
8. **RISK**: the `ScanBinding.rev` type change may reach further than 2a's inventory.
   `resolve_rev`'s two callers were traced; every consumer of `ScanBinding.rev` was not.
   Bound it with `git grep -n 'ScanBinding' HEAD -- src/`.
9. **RISK**: the single-db plans were being written concurrently and may have changed.
10. **RISK**: `from_alias` is a design call, not a measurement. If `scan("WORK")` and
    `scan("HEAD")` need not stay distinguishable on a clean tree, `RevId` collapses to two
    fields and 3.1 changes. The argument for distinguishing them is that the read paths
    differ (`mod.rs:1090-1094`). ~~NO TEST PINS THIS BEHAVIOR~~ FALSE, see section 14.

## 14. Review verdicts (three reviewers, 2026-07-20)

Two mechanical reviewers checked counts, citations, and SQL. One adversarial reviewer
attacked the reasoning. Coordinator independently verified the findings marked VERIFIED.

### Confirmed sound

- **Citations**: 27 of 27 line citations and 11 of 11 surface counts MATCH.
- **`resolve_rev` has exactly two callers** (`repo.rs:291`, `:363`). Risk #8's premise that
  the seam is narrow holds AT THE SEAM.
- **Attack 4, the no-HEAD sentinel**: SOUND. Every git-shelling site that consumes a stored
  rev is gated behind `is_worktree()`, so the sentinel always takes the fs path and never
  reaches git. `files_changed_between` (`repo.rs:1044`) takes its refs from `observe_ref`'s
  fresh `rev-parse`, never from a stored rev. Residual: 31 `Command::new("git")` sites were
  targeted, not exhaustively traced.
- **Attack 8, retention**: SOUND, and it resolves risk #6 in the plan's favor. VERIFIED by
  coordinator: `reconcile_sources` runs at `tick.rs:494/497`, `sweep_gone_revs` at
  `tick.rs:556`, so `_file` is pruned before the sweep reads it and a dead rev is collected
  on the SAME tick. `save_file_meta` deletes stale `(repo, path, rev)` triples
  unconditionally, pre-existing machinery this arc does not add. `sweep_sqlite_gone_call_inputs`
  (`storage/call.rs:766`) uses the identical `rev_sid NOT IN (SELECT sprf_sym(rev) FROM
  _live_rev_scope)` idiom across all six `_call_*` tables, so risk #6 is CLOSED.
  **Retention is NOT a blocking prerequisite.**

### Corrections to this document

- **Risk #10 is factually wrong.** `tests/it/graph_diff_rev.rs::rev_pair_diff_reports_exact_added_and_removed`
  and `::rev_pair_diff_is_empty_when_work_equals_base` DO pin the `from_alias` behavior. The
  first commits `Alpha+Beta`, then overwrites the same file on disk WITHOUT committing to
  `Alpha+Gamma`, and asserts `diff_pair("HEAD","WORK")` reports Gamma added, Beta removed.
  In that scenario WORK's oid EQUALS HEAD's oid, so only the alias distinction separates
  "read the committed blob" from "read the dirty file". Computing `is_worktree()` as
  `oid == head_sha` would silently make that test assert an empty diff. The design decision
  stands and is stronger than stated.
- **`_reldigest` namespaces**: 7, not 6. `hook:` and `shape:` do NOT exist. Actual, with row
  counts: `src:` 93, `rows:` 55, `extract:` 29, `drv:` 17, `async:` 3, `scip:` 1,
  `derived:` 1. Reinforces 4.1's decision to carve out only the `extract:` namespace.
- **`_file.rev` is plain TEXT, not an interned id.** 422 rows, all `WORK`.
- **`PRAGMA foreign_keys` is OFF on the main engine connection**, set only at
  `deltaflow.rs:81` and `ownership.rs:13`. Risk #4 CLOSED: 4.1's `REFERENCES` clauses
  document intent and do not enforce on the real dbs.
- **Growth is 36x the section 11 estimate.** Measured on the sprefa root, not gwell:
  `rel_df_node_rev` 203,347, `rel_df_arg_rev` 82,894, `rel_call_edge_rev` 15,646, plus 11
  others, **~338,290 twin rows per rev**. Section 11's 9,339 came from a repo with zero
  dataflow rows. Harmless given Attack 8, but the number was wrong by a wide margin.
- **21 tables end in `_rev`, not 15, and only 13 are engine twins.** The other 6 are USER
  rels: `head_rev`, `base_rev`, `owner_at_rev`, `fn_at_rev` (`.dl/graph-diff.dl:76,79,101,117`)
  and `current_rev`, `previous_rev` (`.dl/storage-seam-map.dl:79,82`). **Any sweep keying off
  the `_rev` suffix would eat user data.** This also feeds the single-db collision rule.

### BROKEN, must be fixed before implementation

1. **Step 3 is incomplete, and the omission is SILENT.** VERIFIED by coordinator:
   `src/engine/scan.rs:188` holds a SECOND hardcoded `"WORK"`, used as the rev element of a
   `FileMeta` cache-lookup key inside the mtime/size fast path:
   `prev.get(&(repo.to_string(), rel.clone(), "WORK".to_string()))`.
   Step 3 lists only `scan.rs:137`. Change that alone and this lookup asks for `"WORK"`
   against `_file` rows now holding a sha, so it NEVER matches and every file is re-read and
   re-hashed every tick, permanently. No compile error, no test failure. Highest-cost finding
   in the review because it is the only silent one.
2. **Step 2's blast radius reaches four unlisted places.** `resolve_scan_bindings` has one
   caller, `src/engine/reconcile.rs:44`, which never contains the literal text `ScanBinding`
   (it destructures a loop variable), so risk #8's proposed bounding grep MISSES the only
   call site. Actually affected: `reconcile.rs:47` `groups: BTreeMap<(String, String), _>`
   keyed on `(slug, rev)`, which would require `RevId: Ord`; `FileMeta`
   (`mod.rs:378`, `HashMap<(String,String,String), _>`); `next_rev_index` (`reconcile.rs:121`);
   `job_index` (`reconcile.rs:282`); `self.rev_index` (`reconcile.rs:353`, read by
   `check_type` at `mod.rs:1217`); and `SourceExtractJob { rev: String }`
   (`source_prepare.rs:18-24`), a struct Step 3 never names that carries the value into the
   extract functions Step 3 DOES name.
3. **`RevId` has no stated derive list.** Given finding 2 it must satisfy `Ord`, `Hash`,
   `Eq`, `Clone` to serve as a `BTreeMap` and `HashMap`/`HashSet` key. Step 1's sketch
   specifies none.
4. **Section 12 understates the `SCHEMA_EPOCH` cost.** Mechanics are SOUND (the bump does
   drop what is claimed, and `_file` self-heals via `save_file_meta`). The cost comparison is
   not: `exe_stamp` invalidates only the `extract:` namespace inside `_reldigest`, while
   `SCHEMA_EPOCH` drops EVERY `rel_%`, `scc_node_%`, `scc_edge_%`, `_delta_%`, and `_carry_%`
   object, source and derived alike. Ordinary binary bumps do not force a full `rel_%` wipe;
   only an epoch bump does. The plan also never addresses that the daemon's three served
   roots would all cold-rebuild on the same first post-upgrade tick, UNSTAGGERED, which is
   the standing "nothing seizes the machine" law. Anyone upgrading on any later date pays
   this fresh, not only those who take the queued cold rebuild.
5. **Section 6's prose is internally inconsistent, and its hazard is incomplete.** The
   concrete design in 3.2 and Step 2 uses a separate `WorktreeRev` cache and never calls
   `cache_rev`, so section 6's warning about `cache_rev` routing describes a hazard that does
   not apply. Worse, the hazard as stated covers only the DIRTY case. If an implementer
   "simplifies" by reusing `cache_rev`, a CLEAN alias resolution yields a bare 40-hex sha
   that PASSES `is_immutable_rev` (`repo.rs:129-131`, all-hexdigit) and gets filed into the
   cross-tick `rev_sha_cache`, pinning a stale HEAD sha until process restart. That is worse
   than the dirty case, which at least fails the hexdigit test on `+`.
6. **The determinism seam's scoping is unstated, and one named test needs two distinct
   revs in one process.** `tests/it/main.rs` is a single test binary, so `set_var` is
   process-global, confirming section 9's UNCERTAIN as "yes, shared". But
   `graph_diff_rev.rs`'s `DIFF_PROG` contains `diff_pair("HEAD", "WORK")` and needs those to
   resolve differently. A single suite-wide `DL_REV_OVERRIDE` collapses them unless the
   override applies ONLY to the alias path, leaving explicit `"HEAD"` untouched. 3.1 implies
   that scoping; section 9 and Step 8 never state it. State it.
7. **Section 9 misdiagnoses its own headline example.** `extract_cache.rs` builds sandboxes
   under `std::env::temp_dir()` with NO `git init`, so it was never exposed to developer
   dirty-tree flakiness. Its actual exposure is the no-HEAD sentinel path. The change also
   adds a new cost there: every `WORK` resolution in those sandboxes now spawns a
   `git rev-parse HEAD` that fails before falling back to the sentinel, where today's code
   does zero git work for `WORK`.
8. **Section 8's "shrinks the collapse's surface" is not supported.** `design-b.md`
   section 3.4 already plans to repo-key `_reldigest` (`meta.rs:212`) to
   `PRIMARY KEY (repo, rel)` and bumps `SCHEMA_EPOCH` itself in its own Step 1. This plan
   carves `_extract_digest` out of that same table with an `(exe, family, rev)` PK and NO
   `repo` column, so design B would inherit a new sibling table that never learns it needs
   the same retrofit. Two stacked epoch bumps, and one table becomes two with only one of
   them repo-keyed. Reasons 1 and 3 of section 8 still stand; reason 2 does not.

### Also worth knowing

- **`meta.rs:203`'s dead statement changes what "success" means.** If `execute_batch_on` does
  NOT swallow errors, the `strings_is_text` migration arm has been failing on every affected
  db already. Step 5 would fix that as a side effect without diagnosing it. Resolve before
  Step 5, not during.
- **`reconcile.rs:74,164` are not safe to ignore.** Classified "display only" in 2a, but once
  rev is never literally `"WORK"`, `if rev == "WORK" { "WORK" } else { &rev[..8] }` always
  takes the else arm, so the `[scan {slug}@{rev_short}]` trace lines an operator reads lose
  the working-tree signal and print a sha prefix with no dirty marker. The standing law that
  the daemon must be able to state what it was doing treats those lines as first-class.
