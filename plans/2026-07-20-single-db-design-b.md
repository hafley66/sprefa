# Single-db design B

Branch v11, HEAD 538e7f78. Read-only pass over `git show HEAD:` blobs.
Transcribed from the design agent's report (it had no write tool). Design A is
`plans/2026-07-20-single-db-design-a.md`; the reconciled decision supersedes both.

**Scale caveat added after the fact**: this design was briefed on 3 roots. The
user's actual target is 500+ repos. Section 6's concurrency-1 answer does not
survive that, and section 12 records what breaks.

## 0. One measured fact in the brief is contradicted by the source

The brief claimed `_strings` ids are per-database dictionary offsets. `src/spine.rs:52-57`:

```rust
pub fn of(text: &str) -> Self {
    if text.is_empty() { return Self::EMPTY; }
    Self(hash64(text.as_bytes()))
}
```

`src/engine/meta.rs:274-283` documents storage as "id = StringId::sqlite() (the
i64 bit-pattern of the content-derived u64 hash) as an INTEGER PRIMARY KEY".
`src/lower.rs:36-38` (`sym_lit`) computes the id at COMPILE TIME from literal text
with no table lookup. `src/db.rs` registers `sprf_sym_intern` returning
`StringId::of(&text).sqlite()` with no sequence and no offset.

Ids are content-addressed and globally stable across every database file.
Consequences:

- `_strings` union across dbs is naively valid; `INSERT OR IGNORE` merges with
  zero remapping.
- Cross-db ATTACH+join would have worked. Zero `ATTACH` occurrences are a choice.
- The only per-db thing is which subset is materialized, plus the process-local
  `Db::persisted_strings` memo (`STRING_CACHE_CAP = 4_000_000`), whose doc comment
  already reasons about "root B could skip an insert root A made into a DIFFERENT
  database file". Under one db that hazard disappears and the memo gets strictly
  more effective.

## 1. Type signatures

```rust
// ---------- src/daemon/home.rs ----------

/// THE database. No key, no per-root dir.
pub fn corpus_db_path() -> PathBuf;
// body: daemon_home().join("db.sqlite")

/// Retained ONLY as a migration source enumerator.
pub(crate) fn legacy_root_dbs() -> Vec<(String, PathBuf)>;
// body: read_dir(daemon_home().join("roots")); for each dir holding db.sqlite,
//       push (dir_name, path). Sorted by dir name for deterministic order.


// ---------- src/engine/relmap.rs (new) ----------

/// Everything that decides whether two declarations can share one table.
pub struct RelShape {
    pub cols: Vec<(String, Type, bool)>,  // (name, type, interned)
    pub key: Option<Vec<String>>,
    pub merge: Option<MergeFn>,
    pub without_rowid: bool,
}

impl RelShape {
    pub fn of(decl: &RelDecl) -> RelShape;
    // No sorting: column ORDER is part of the shape, because
    // src/engine/declare.rs:337 compares `have != want` POSITIONALLY and drops
    // the table on any positional difference.

    pub fn fingerprint(&self) -> String;
    // Canonical newline-joined encoding ("name:TYPE:interned" per col, then
    // "key=a,b", "merge=MaxBy(x)", "wor=1") -> blake3 -> first 16 hex.
    // Stable across runs: reads only decl content, never a path, timestamp,
    // or load order.
}

/// Which program file, inside which root, declared a rel.
pub struct Modpath(String);

impl Modpath {
    pub fn of(repo_slug: &str, program_file: &Path, root: &Path) -> Modpath;
    // strip_prefix(root); drop a leading ".dl/" segment; drop the ".dl"
    // extension; MAIN_SEPARATOR and '-' -> '_'; lowercase; keep [a-z0-9_].
    // root=/Users/c/projects/smashy, file=.dl/lint.dl -> "smashy__lint"
    // Builtin decls (src/engine/decls.rs) get Modpath("__builtin").

    pub fn builtin() -> Modpath;
}

/// One row of `_rel_table`: a pinned decision, never recomputed once written.
pub struct RelTableRow {
    pub rel: String,
    pub modpath: Modpath,
    pub shape_hash: String,
    pub physical: String,   // "rel_todo" or "rel_todo__smashy__lint"
    pub owner: bool,        // holds the unsuffixed name
    pub first_seen: i64,
    pub last_seen: i64,
}

/// The ONLY place that decides a physical table name.
pub struct RelMap {
    rows: HashMap<(String, Modpath), RelTableRow>,
    owner_of: HashMap<String, String>,   // rel -> shape_hash owning the bare name
}

impl RelMap {
    pub fn load(db: &Db) -> Result<RelMap>;
    // ONE `SELECT ... FROM _rel_table` -> build both maps. Never a per-rel point
    // read (N+1 is a blocking defect).

    pub fn resolve(&mut self, decls: &[(RelDecl, Modpath)], now: i64)
        -> (HashMap<String, String>, Vec<RelTableRow>);
    // for (decl, modpath) in decls:
    //   shape = RelShape::of(decl).fingerprint()
    //   pinned row with matching shape_hash -> touch last_seen, reuse, continue
    //   else consult owner_of[decl.name]:
    //     None                      -> first declarer wins the bare name
    //     Some(s) if s == shape      -> shape-identical: SHARE the table, no suffix
    //     Some(_)                    -> real collision: rel_<name>__<modpath>
    //   push to dirty

    pub fn persist(&self, db: &Db, dirty: &[RelTableRow]) -> Result<()>;
    // ONE chunked multi-row INSERT ... ON CONFLICT(rel, modpath) DO UPDATE SET
    // last_seen = excluded.last_seen. Never per row.

    /// Drop candidates for `dl daemon gc`: stale and not the owner's table.
    pub fn stale(&self, cutoff: i64) -> Vec<&RelTableRow>;
}

/// Rewrite every rel NAME in one program BEFORE the engine sees it. This is why
/// `lower::tbl` stays a pure format! and its ~60 call sites are untouched.
pub fn apply_renames(prog: &mut Program, renames: &HashMap<String, String>);
// RelDecl -> rename name. Rule -> rename head.rel and every Pos/Neg body atom,
// recursing into nested body items (agg subqueries, extraction ops that name a
// rel). Query -> rename q.rel. Names NOT in `renames` are untouched, so a rel
// sharing the owner's table keeps its written name and cross-repo joins work.


// ---------- src/engine/mod.rs ----------

impl Engine {
    pub(crate) fn repo_id(&self) -> i64;
    // StringId::of(&self.self_slug()).sqlite(); self_slug() is
    // src/engine/mod.rs:796, the --root directory basename.

    pub(crate) fn repo_scope(&self, rel: &str) -> String;
    // if rel_has_repo(rel) { format!(" WHERE \"repo\" = {}", self.repo_id()) }
    // else { String::new() }   // zero-column `true` has no repo

    pub(crate) fn wipe_rel_sql(&self, rel: &str) -> String;
    // format!("DELETE FROM {}{}", tbl(rel), self.repo_scope(rel))
}
```

## 2. Instance lifetimes

| Type | Owner | Created | Dropped | Count |
|---|---|---|---|---|
| `Db` (`src/db.rs`) | `Engine` | `db::open(Some(corpus_db_path()))` in `ServedRoot::open` (`src/daemon/root.rs:643`) | shutdown or `drop_root` | one per served root, all on the same file (section 9) |
| `Engine` | `ServedRoot` | `src/daemon/root.rs:644` | with its `ServedRoot` | one per served root, unchanged |
| `RelMap` | `Engine` | once in `Engine::new` after `ensure_meta_tables`, refreshed on hot-reload | with `Engine` | one per `Engine`; all read/write the same `_rel_table` rows |
| `Db::persisted_strings` | `Db` | `db::open` | with `Db` | now safely shared-content across roots; the per-file caveat in its doc can go |
| `RelShape` / `Modpath` / `RelTableRow` | values | during `resolve` | end of resolve | transient |
| apalis `Jobs` store | `Daemon` | `run_daemon` (`src/daemon/mod.rs:474-481`) on its OWN `<home>/jobs.sqlite` | shutdown | one, unchanged, deliberately NOT moved into the corpus db |

`RelMap` is the only new stateful type. Its authority lives in `_rel_table`, not
in the process, so concurrent `Engine`s converge through the table's PK rather
than through shared memory.

## 3. Storage layout

### 3.1 Files

```
~/.local/state/sprefa/
  db.sqlite          THE database (today: the config-view db, daemon/mod.rs:500-502)
  db.sqlite-wal
  db.sqlite-shm
  jobs.sqlite        apalis queue, separate file, unchanged
  daemon.sock
  daemon.pid
  roots.json         still the registered-root list; no longer carries a db key
  roots/             DELETED after migration
```

The config view already opens `<home>/db.sqlite` (`src/daemon/mod.rs:500-502`), so
this collapses the root dbs INTO that file rather than inventing a fourth path.
The config view and every root become the same connection target, and `scan("*")`
(built, unused) finally has something to fan across.

### 3.2 The meta table

```sql
CREATE TABLE IF NOT EXISTS _rel_table (
  rel         TEXT    NOT NULL,
  modpath     TEXT    NOT NULL,
  shape_hash  TEXT    NOT NULL,
  physical    TEXT    NOT NULL,
  owner       INTEGER NOT NULL DEFAULT 0,
  first_seen  INTEGER NOT NULL,
  last_seen   INTEGER NOT NULL,
  PRIMARY KEY (rel, modpath)
);
CREATE INDEX IF NOT EXISTS _rel_table_physical_idx ON _rel_table(physical);
CREATE UNIQUE INDEX IF NOT EXISTS _rel_table_owner_idx
  ON _rel_table(rel) WHERE owner = 1;
```

Goes into the `CREATE TABLE IF NOT EXISTS` batch at `src/engine/meta.rs:270-310`,
beside `_program` (`:300`) and `_repo` (`:306`). The partial unique index makes
"one owner per name" a database invariant instead of a code convention.

### 3.3 The repo column on rel tables

`src/engine/declare.rs:369-419` builds the CREATE TABLE. For a rel that does not
already declare `repo` and is not zero-column, prepend
`"repo" INTEGER NOT NULL DEFAULT 0` and prepend `"repo"` to the PRIMARY KEY list.

Leading position matters: `declare.rs:397-408` emits the PK column list in
declaration order, so a leading `repo` makes `WHERE "repo" = <lit>` a prefix probe
on the PK B-tree, which is what the wipe and every scoped body atom need.

Rels that already declare `repo` (every extracted rel; confirmed by
`declare.rs:988` writing `refresh_rel("file", &["repo","rev","path","content"], ...)`)
are untouched except that `repo` moves to PK position 1.

`declare.rs:337-364` already drops and recreates a table on column-set or key-set
drift and clears `_reldigest` and `_derived_complete`. That existing migration
path handles the column addition with no new code, on the tick after upgrade.

### 3.4 The rel-keyed meta tables (undercounted by the brief)

Three tables key on rel name alone, so repo A's tick would overwrite repo B's
bookkeeping:

- `src/engine/meta.rs:212` `_reldigest (rel TEXT PRIMARY KEY, digest TEXT)`
- `src/engine/meta.rs:240` `_derived_complete (rel TEXT PRIMARY KEY)`
- `src/engine/meta.rs:255` `_stmt_ms (rel TEXT PRIMARY KEY, ms INTEGER, n INTEGER)`

Each becomes `PRIMARY KEY (repo, rel)`. `_derived_complete` matters most:
`mark_derived_complete` / `unmark_derived_complete` are the crash-safety markers
described at `derive.rs:497-503`, and a shared marker lets repo A's completed mark
vouch for repo B's never-populated rows. That is a silent-wrong-answer bug, worse
than the DELETE bug.

`_carry_meta` (used by `refresh_every`, `declare.rs:1024-1033`, key `every:N`)
needs the repo prefix in its `k` string: a one-line change at key construction.

## 4. Sequence of reads and writes

### 4.1 Program load (per root, per hot-reload)

1. `resolve_programs` discovers `<root>/.dl/*.dl` (`src/daemon/root.rs:648-652`).
2. `prepare_paths(&files)` parses and typechecks into a `Program` (`:657`).
3. NEW: per `RelDecl`, compute `Modpath::of(self_slug, declaring_file, root)`.
   This needs the parser to record which file each item came from. **Could not
   determine from source whether `Program::items` retains source-file
   attribution.** If not, that is a prerequisite (step 2 below).
4. NEW: `RelMap::load` (one SELECT), `resolve`, `persist` (one chunked upsert),
   `apply_renames(&mut prog, &renames)`.
5. `Engine::tick` proceeds against the rewritten program. `declare_rel`
   (`declare.rs:280-430`) sees only physical-unique names, so `tbl()` stays
   `format!("rel_{name}")` at `src/lower.rs:6` and its ~60 call sites do not move.

### 4.2 Tick

Reads unchanged in shape. Writes change at:

- Source refresh: `refresh_rel("repo"|"rev"|"content"|"file", ...)`
  (`declare.rs:985-988`). These already carry `repo`; `refresh_rel` must become a
  SCOPED reconcile rather than a wholesale replace. **Body not read**; if it
  wholesale-deletes it is another unscoped site.
- Derived rebuild: every full wipe routes through `wipe_rel_sql`.
- `_write_ledger` (`meta.rs:260`) gains a repo column so per-root write
  attribution stays readable. Observability, not correctness.

### 4.3 The wipe sites, one by one

| site | current | becomes |
|---|---|---|
| `derive.rs:201` | node2vec `node_sim` head | `wipe_rel_sql(head)`. Also `_node_embeddings` / `_node_emb_seen` are keyed by `graph` (`:207,212,305,311`); the key becomes `format!("{repo_id}:{edge}")` so two roots embedding a same-named edge rel do not share a vector cache. |
| `derive.rs:484` | mirror-path head wipe | `wipe_rel_sql(&head_rel)`. The mirror is TEMP and connection-local (`:570-580`) so needs no scoping, but `eval_component_mirror`'s live-vs-mirror EXCEPT must be repo-scoped or an unchanged rel reads as changed every tick. |
| `derive.rs:510` | component wipe loop | `wipe_rel_sql(rel)` |
| `derive.rs:546` | orphan-rel wipe | `wipe_rel_sql(rel)` |
| `derive.rs:1123` | multi-source walk head | `wipe_rel_sql(&head_rel)`. The surrounding index drop/rebuild (`:1124-1131`) drops SECONDARY indexes on a now-shared table. Correct but wasteful; rebuild should be conditional on the table having a single repo. |
| `derive.rs:1536` | closure head | same, same index caveat |
| `derive.rs:2353` | closure-rule head | `wipe_rel_sql(head)` (not in the brief) |
| `derive.rs:2466` | scc head | `wipe_rel_sql(head)` (not in the brief) |
| `declare.rs:989` | `DELETE FROM rel_true` then `INSERT OR IGNORE ... DEFAULT VALUES` | DELETE the DELETE. `true` is a zero-column singleton whose content is repo-independent; the `INSERT OR IGNORE` alone is idempotent, and dropping the DELETE removes a cross-repo hazard and a per-tick WAL write. |
| `declare.rs:1013` | `DELETE FROM rel_every` | `rel_every` gains `repo`; `wipe_rel_sql("every")`. The `before` count read at `:1005-1010` must be scoped too, or root B's rows make root A think its clock content changed. |
| `derive.rs:1712,1749,1789` | `_delta_new_{rel}` / `_delta_{rel}` semi-naive scratch | NAMES must gain the repo id (`_delta_new_{repo_id}_{rel}`) or two roots in one fixpoint collide. Not in the brief and a real hazard. |
| `derive.rs:2102-2103` | `scc_node_tbl` / `scc_edge_tbl` | same repo-qualified naming |
| `derive.rs:1824` | `DELETE FROM _stmt_ms WHERE rel IN (...)` | `AND repo = {repo_id}` once re-keyed |
| `declare.rs:347,363` | `_reldigest` / `_derived_complete` per-rel deletes | `AND repo = {repo_id}` once re-keyed |
| `meta.rs:167,170,176` | full wipes on a schema-version bump | leave GLOBAL. A schema bump invalidates every repo's bookkeeping, which is intended. |

## 5. How `repo` reaches a derived head: the pick

**A. Implicit column plus compile-time literal (PICKED).** `src/lower.rs:461`
already has the seam:

```rust
pub fn lower_rule_to(rule: &Rule, rels: &Rels, target: &str, extra: &[(String, String)]) -> Result<String>
```

`extra` appends constant `(column, value_sql)` pairs to the head. It exists for the
`@next` carry path (`target = carry_<rel>`, `extra = [("tx", "<next_tx>")]`,
documented at `lower.rs:453-460`). Passing `extra = [("repo", repo_id.to_string())]`
from `lower_rule` is a two-line change; the constant lands in the head with no body
participation. Body atoms gain `AND {alias}."repo" = {repo_id}` in `body_sql_ex`'s
wheres. Both sides are integer literals against the leading PK column.

Tradeoff: lowered SQL embeds the repo id, so a cross-process SQL cache keyed only
on rule text would be wrong. No such cache exists: `prepare_cached` (`src/db.rs:203`)
is per-`Connection` and each `Engine` owns its own, so the cache is already
repo-partitioned. Cost zero. Benefit over the alternatives: the planner sees an
integer constant so the PK-prefix probe is available, and `scan("*")` lowers to the
SAME code path with the predicate simply omitted, which neither alternative can
express.

**B. Session-scoped connection variable.** Register `sprf_repo()` as a
`SQLITE_DETERMINISTIC` scalar capturing the connection's repo id, in the style of
`sprf_sym_intern` (which already captures `pending_syms` in a closure). Rejected: a
UDF cannot be varied per statement, so `scan("*")` and any cross-repo query have no
expression, and a function in the WHERE is a constraint SQLite can use but will not
always inline as cleanly as a literal.

**C. Explicit and threaded.** Rejected outright: breaks every existing `.dl`
program, puts an engine partition key in the user's surface language, and the
user's model is that one database is an implementation fact, not a query
obligation.

Accepted asymmetry: a derived head gets its repo from the ENGINE, not its body. A
rule whose body joins two repos (only reachable under `scan("*")`) stamps both
results with the current engine's repo. That is correct for "this root derived
this", and it is why `rev` cannot substitute (one repo, many revs; the `_rev`
twins and graph-diff depend on that).

## 6. Write contention

### Current discipline

`src/db.rs:207-209` sets `journal_mode=WAL; synchronous=NORMAL`. There is NO live
`busy_timeout` pragma: `src/db.rs:335-370` installs a custom `busy_handler`
reimplementing the semantics (20ms sleep per retry, **give up at 5000ms**,
`if elapsed_ms >= 5000 { return false; }` at `src/db.rs:365`), emitting a
`sqlite-busy` verdict past `SQLITE_BUSY_WARN_MS`. The 50ms `PRAGMA busy_timeout` at
`src/db.rs:234` is a throwaway for the write-lock probe, replaced immediately.
Read connections (`open_read_only`) take 1000ms and never contend, because WAL
readers do not block on the writer.

So busy_timeout is 5000ms, implemented by hand.

### The problem

SQLite WAL allows exactly one writer. Today three roots write three files and never
contend. Under one file, a cold rebuild of the sprefa corpus (830MB) holds the write
lock far longer than 5000ms, and smashy's tick fails with SQLITE_BUSY rather than
waiting. A hard failure, not a slowdown.

### Discipline: single writer, bought not built

The daemon already runs apalis-sqlite on `<home>/jobs.sqlite`, with a `dl-engine`
queue at N workers and a `dl-cold` queue at concurrency 1 explicitly described as
single-flight (`src/daemon/mod.rs:518-537`, `src/jobq/mod.rs:1-25,119`). Infra was
bought in `plans/2026-07-18-infra-library-adoption.md` (cited at `jobq/mod.rs:1`).

Route every corpus-writing job onto a concurrency-1 apalis queue: set `dl-engine`
concurrency to 1, or add a `dl-write` queue at 1 and move tick and sink-drain job
kinds onto it (`jobq/mod.rs:142` already models a per-kind queue name). No new
dependency, one constant plus a routing entry.

Contradiction worth naming: `daemon_thread_count` (`daemon/mod.rs:530-533`) sizes
`dl-engine` off `available_parallelism`, assuming parallel root ticks are parallel
because they write different files. That assumption dies. What survives is that the
CPU-heavy phases (parsing, tree-sitter extraction, rayon fan-out) are NOT the write
phase, so the right shape is parallel EXTRACT into per-root staging plus serialized
COMMIT. That is a larger refactor; the honest interim is concurrency 1 with the
throughput cost accepted and measured.

### Library analysis

| Candidate | Verdict |
|---|---|
| apalis + apalis-sqlite (already a dependency) | **CHOSEN.** Durable, crash-recoverable (`reset_orphaned_on_boot`, `daemon/mod.rs:524`), heartbeat re-enqueue for mid-life orphans, idempotency keys for coalescing enqueue (`jobq/mod.rs:14`), per-queue concurrency, retention vacuum, on the existing tokio shell runtime. Zero new dependency, one config change. |
| `tokio::sync::Mutex` / `parking_lot::FairMutex` write token | Rejected as primary: no durability, no crash recovery, no coalescing, no visibility, all of which apalis already provides. Worth keeping as a secondary in-process guard for non-job write paths (a one-shot `dl` inside the daemon process), one line. |
| `deadpool-sqlite` / `r2d2_sqlite` with `max_size = 1` | Rejected. Pooling solves connection reuse, not write ordering, and size-1 serializes ALL access including reads, discarding the WAL read concurrency the read path depends on (`open_read_only`: "a reader opened here NEVER blocks on the writer"). |
| `sqlx` with its own pool | Rejected. Already present transitively via apalis-sqlite (`jobq/mod.rs:37` names "the sqlx pool"), but the engine is on `rusqlite 0.32.1` and moving the engine seam to sqlx rewrites `src/db.rs`, the file whose entire premise is being the single owner of the Connection. |
| `crossbeam-channel` (already a dep) plus a writer thread | Rejected. This IS writing our own queue with a thin library underneath, with apalis right there. |

### Cross-process contention

The daemon is not the only writer: `src/cli/mod.rs:424` (daemonless one-shot),
`src/hook.rs:468`, `src/lsp.rs:91`. Under one db all three contend with the daemon
and each other. Two required mitigations:

1. Raise the 5000ms give-up (`src/db.rs:365`) to an env-overridable default of
   30000ms, so a hook waiting behind a daemon commit waits rather than fails. The
   verdict at `src/db.rs:353-360` already makes the wait visible.
2. Chunk the cold rebuild so each per-repo component commits its own transaction,
   keeping the longest write-lock hold in seconds. `rebuild_derived` already
   brackets per component (`derive.rs:497-527`); the change is transaction boundary
   placement, **not traced, flagged unverified**.

## 7. Migration

Because ids are content-addressed, a real migration is available and cheap:

```sql
ATTACH '<home>/roots/<key>/db.sqlite' AS src;
INSERT OR IGNORE INTO main._strings (id, content) SELECT id, content FROM src._strings;
-- per rel table present in src:
INSERT OR IGNORE INTO main.rel_X (repo, <cols>, __src)
  SELECT <repo_id_literal>, <cols>, __src FROM src.rel_X;
DETACH src;
```

`INSERT OR IGNORE` on `_strings` is exactly correct: an id collision means
identical content, by construction of `StringId::of`.

Complications: a rel table in the source may not exist in the target yet (migration
must run AFTER a program load); `WITHOUT ROWID` and PK shapes may differ if the
roots ran different binary versions; the `repo` column does not exist in source rows
and must be synthesized from `src._repo` (`meta.rs:306`) and trusted.

**Recommendation: cold rebuild, and the queued nuke item makes migration moot.**
Not only because of the queued item: every rel here is either a source rel
reconciled from the filesystem every tick or a derived rel recomputable from source
rels. `declare.rs:335-336` states it outright ("Rel tables are derived (or source
rows reconciled every tick), so dropping loses nothing"). 986MB of recomputable data
is not worth an ATTACH path used once and then rotting.

Nothing found that must be preserved. `_program` and `_repo` are re-saved every open
(`root.rs:696-697`). `_write_ledger` is telemetry. `_node_embeddings` is a cache with
a digest guard (`derive.rs:196-205`). If the node2vec vectors are worth keeping (the
only genuinely expensive-to-recompute artifact), migrate that ONE table by ATTACH
and skip everything else.

## 8. What this deletes

**Per-root db directories**

| Site | Fate |
|---|---|
| `src/daemon/home.rs:72` `root_db_dir` | migration-only, then deleted |
| `src/daemon/home.rs:84-87` `root_db_path` | callers repoint to `corpus_db_path()`: `src/cli/mod.rs:424`, `src/hook.rs:468`, `src/lsp.rs:91` |
| `src/daemon/home.rs:91-93` `key_of` | SURVIVES. `roots.json` and the `Daemon::roots` HashMap still key by it (`daemon/mod.rs:253,274,329,622`). Stops being a STORAGE key, becomes purely an in-memory registry key. Its test at `daemon/mod.rs:870-873` stays. |
| `src/daemon/mod.rs:303`, `:624` | `root_db_dir(&key).join("db.sqlite")` -> `corpus_db_path()` |
| `src/daemon/root.rs:664` | `create_dir_all(root_db_dir(k))` deleted; the home dir already exists |

**The orphan-root failure class, entirely**

| Site | Fate |
|---|---|
| `src/daemon/client.rs:288-292` | `drop --purge`'s `remove_dir_all`. `--purge` must be deleted or REDEFINED as "delete this repo's rows", now expressible as a scoped sweep across `_rel_table`. Redefining is better and is a new capability the single db unlocks. |
| `src/daemon/mod.rs:340` | same `remove_dir_all` in `drop_root` |
| `src/cli/health.rs:45,58-63` | the orphan list and the `rm -rf` hint |
| `src/cli/health.rs:68-100` `report_roots_overview` | the dirs-versus-registered diff; the registered-set report survives in reduced form, the ORPHAN branch (`:87-91`) dies |
| `src/cli/health.rs:106-119` `orphan_origin` | dead: there are no unregistered dbs |
| `src/cli/health.rs:330` `dir_bytes` | may survive for the one db; the per-root loop does not |
| `src/cli/daemon_cmd.rs:284` | help text loses "orphan roots" |
| `src/cli/health.rs:51` | the per-db report loop collapses to one `report_db`; `--root` filtering (`:47-50`) changes meaning from "pick a db" to "filter by repo column" |

**The class-14 worktree cold-check rail, entirely**

| Site | Fate |
|---|---|
| `src/hook.rs:460-489` `refuse_worktree_cold_check` | its stated incident (`:453-458`) is worktree pre-commit `dl --check` cold-building a ~600MB `roots/<key>/db.sqlite` that orphaned on worktree deletion. With one db there is no per-worktree db to mint. A worktree check writes its repo's rows into the shared db, the correct outcome. |
| `src/cli/mod.rs:471-483` | call site, `[check] {reason}` eprintln, the class-14 tracing line, the green-by-skip `exit(0)` |
| `DL_ALLOW_WORKTREE_COLD` | escape hatch and its docs |
| `src/hook.rs:449-451` `is_linked_worktree` | only caller was the rail |
| `src/hook.rs:437-445` `inproc_db_is_cold` | SURVIVES; also called from `hook_work`'s `inproc_or_skip` |

**Ancillary**

- `src/lib.rs:83` `.state/cache.db` comment referencing `daemon::root_db_path` needs rewording.
- `src/db_ratio::emit_verdict` (called at `root.rs:~718`) computes corpus-to-db ratio
  per root. With one db the denominator is shared and the per-root ratio is
  meaningless. Either drop the rail or recompute against a per-repo `dbstat` sum,
  which is real work. **Unresolved.**
- `src/db.rs` `persisted_strings` doc arguing "never a global static, or root B could
  skip an insert root A made into a DIFFERENT database file" now misleads.

## 9. Where the four layers disagree

- **Signatures versus storage.** `RelMap::resolve` returns a rename map, implying
  renaming is compile-time; storage says `_rel_table` is the authority. Resolution:
  storage wins for the NAME (a pin is never revoked while the declarer exists),
  signatures win for the SHAPE (a shape change re-pins and drops the table through
  the existing `declare.rs:337` drift path). A shape change on the OWNER row
  silently rewrites the owner's shape rather than demoting it, so a non-owner
  declarer that matched the old shape now collides and gets suffixed on the next
  load. That is a visible rename event and must produce a WARNING, not a silent
  table swap.
- **Reads/writes versus uniqueness.** A derived head is stamped with the engine's
  repo id, and the PK is `(repo, <cols>)`. Under `scan("*")`, a rule ranging over
  two repos produces rows differing only in body-derived columns, all stamped with
  the same head repo, so identical bindings collapse under the PK. That is set
  semantics and correct, but `scan("*")` results are then not per-source-repo
  attributable. Attribution would need a second implicit `src_repo` column, out of
  scope here.
- **Lifetimes versus contention.** One `Db` per served root, all on one file, with a
  one-writer discipline: coherent but wasteful (three page caches, three
  `persisted_strings` sets, three `prepare_cached` caches at capacity 256 each).
  `sqlite_memory_budget` / `reserve_process_budget` already treat the cache as a
  process-wide pool divided among connections, so it survives, but the honest
  endgame is one `Arc<Mutex<Db>>`. Not proposed because `Engine` owns `Db` by value
  and the refactor touches every `self.db` call site.

## 10. Ordering

**Step 1. Repo-key the rel-scoped meta tables.** `_reldigest`, `_derived_complete`,
`_stmt_ms` to `PRIMARY KEY (repo, rel)` (`meta.rs:212,240,255`), bump the schema
version so `meta.rs:167-176` wipes them, thread `repo_id()` through ~10 call sites.
Lands with three dbs still in place, a no-op there.
Validate: `cargo test --test it`, plus `dl --root . --check` twice; the second run
must report the derived-skip path (`derive.rs:490`), proving markers round-trip.

**Step 2. Source-file attribution on `Program` items.** Prerequisite for `Modpath`.
Empty if `prepare_paths` already retains it (undetermined). Otherwise add
`source_file: Option<PathBuf>` to `RelDecl` and populate it in the parse loop.
Validate: a unit test asserting a two-file program's decls carry distinct files.

**Step 3. `_rel_table` plus `RelMap`, inert.** Create the table, load/resolve/persist,
discard the rename map. Purely additive.
Validate: `SELECT rel, modpath, shape_hash, physical, owner FROM _rel_table ORDER BY rel`
shows one row per declared rel, all `owner = 1`, all `physical = 'rel_' || rel`. Run
twice; `first_seen` unchanged, `last_seen` moved.

**Step 4. Apply renames.** Wire `apply_renames`. Still three dbs, so no collisions
exist and the map is empty in practice. This step proves the pass is a no-op on the
identity map.
Validate: byte-compare lowered SQL for a fixed program before and after.

**Step 5. Implicit `repo` column on every rel table.** `declare.rs:369-419` prepends
column and PK position. `lower_rule` passes `extra = [("repo", repo_id)]`.
`body_sql_ex` adds the scoped predicate. The `_txt` view (`declare.rs:119-134`) must
NOT project `repo`, preserving existing query arities. The drift path does the table
migration.
Validate: `dl --root . -q 'SELECT DISTINCT repo FROM rel_call_def'` returns exactly
one row equal to `StringId::of("sprefa")`, and every existing query test passes. The
step most likely to break tests, deliberately before the merge.

**Step 6. Scope every wipe.** `wipe_rel_sql` plus the 14 sites above, plus `_delta_*`
and `scc_*` scratch-table name qualification.
Validate: a new test opening ONE in-memory db, registering two synthetic roots with
the same derived rel, ticking A then B then A, asserting B's rows survived. This is
the test that would have caught the whole bug class and it should exist BEFORE the
merge.

**Step 7. Point every path at `corpus_db_path()`.** `daemon/mod.rs:303,624`,
`cli/mod.rs:424`, `hook.rs:468`, `lsp.rs:91`, `root.rs:664`. Delete `root_db_dir`
usage from `drop_root` and `client.rs:292`.
Validate: start, register all roots, `ls ~/.local/state/sprefa/` shows exactly one
`db.sqlite` and `roots/` gains no new dir. `dl -q` against each root returns rows.

**Step 8. Concurrency 1 on the writing queue.** `daemon/mod.rs:530-537`,
`jobq/mod.rs:142` routing.
Validate: touch a file in each root simultaneously, confirm zero `sqlite-busy`
verdict lines, record tick wall time as the regression baseline.

**Step 9. Delete the dead paths.** Everything in section 8. Pure deletion.
Validate: `cargo build` plus `cargo clippy --all-targets` with no new `dead_code`,
and `dl daemon health` rendering without the orphan section.

**Step 10. Nuke and cold rebuild.**
Validate: `dl daemon health` reports one db; `SELECT DISTINCT repo FROM rel_file`
returns three rows; total bytes against the 986MB baseline (expect a reduction, since
`_strings` deduplicates and shared vendor/stdlib content collapses).

## 11. Risks and guesses

**Verified in source**: every file:line above, the busy handler's 5000ms, the `extra`
seam on `lower_rule_to`, `tbl` being pure, `StringId` content-addressing, the apalis
queue and its concurrency-1 cold queue, the rel-keyed meta tables, the drift-drop path.

**Guesses, flagged:**

1. **`refresh_rel`'s body, not read.** If it wholesale-deletes it is another unscoped
   site. High confidence it needs work either way, since it is described as
   "wholesale replace one engine-owned relation" at `src/engine/mod.rs:817-819`.
2. **Whether `Program` items carry source-file attribution.** Step 2 exists because
   this is undetermined. If not, `Modpath` cannot be computed and the meta layer
   stalls on a parser change.
3. **`apply_renames` completeness.** Enumerated `RelDecl`, `Rule` head, Pos/Neg body
   atoms, and `Query`. The body-item extraction ops (`match_line`/`ast`/`match_ast`/
   `json`) may name rels in unseen shapes, and aggregate subqueries may nest. A missed
   site produces a rule reading the WRONG physical table with no error. Highest-risk
   piece of the design; wants an exhaustive `match` with no wildcard arm so the
   compiler enforces completeness.
4. **Transaction boundaries in `rebuild_derived`, not traced.** The claim that
   per-component chunking is boundary placement rather than restructure is unverified.
5. **Index drop/rebuild on shared tables** (`derive.rs:1124-1131,1537-1546`). Asserted
   wasteful. An index rebuilt while another repo's rows are present is still valid, so
   this is performance, not correctness. High confidence, untested.
6. **Ownership stability under program churn.** Renaming `.dl/lint.dl` to
   `.dl/rules.dl` changes the modpath, orphaning the old `_rel_table` row and possibly
   minting a new suffixed table beside a live one. `RelMap::stale` plus a
   `dl daemon gc` sweep is the intended cleanup; the gc policy is unnamed beyond that.
7. **Single-writer throughput cost, unmeasured.** `dl-engine` is sized off
   `available_parallelism`, so this could be a 3x-to-Nx regression on multi-root wall
   time. Step 8's validation exists to produce that number before step 9 makes the
   change hard to revert.
8. **Whether `db_ratio`'s class-17 rail can survive.** May simply have to be deleted,
   losing a rail the project deliberately built.

## 12. What 500+ repos breaks (added after the design)

This design assumed 3 roots. At the real target:

- **Concurrency 1 is fatal.** Section 6 serializes every corpus write. With 500 repos
  that is a single global write lane for the whole machine. The parallel-EXTRACT plus
  serialized-COMMIT shape named there as "a larger refactor" becomes mandatory, not
  optional.
- **Per-root `Db` instances do not scale.** Section 2 keeps one `Db` per served root,
  each with a page cache, a `persisted_strings` set, and a 256-entry `prepare_cached`.
  500 of those is the memory budget on its own. The `Arc<Mutex<Db>>` endgame named in
  section 9 becomes mandatory.
- **`(repo, <cols>)` PK cardinality.** A rel at 283k rows per repo times 500 repos is
  ~140M rows in one table. The leading-`repo` PK prefix probe is what makes that
  survivable, which raises the auto-index question design A flags as its top risk.
- **`RelMap` load.** One SELECT over `_rel_table` per `Engine` construction, times 500
  engines, each holding a full copy. Wants a shared read-only snapshot.
