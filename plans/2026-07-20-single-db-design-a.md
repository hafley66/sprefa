# Single-database design (design A)

Branch v11, HEAD 538e7f78. Read-only pass over `git show HEAD:` blobs. Every claim cites file:line.

Transcribed from the design agent's report (it had no write tool). Design B and
`plans/2026-07-20-single-db-inventory.md` are separate documents; the reconciled
decision supersedes all three.

## 0. Three corrections to the brief

### 0.1 `_strings` ids are global content hashes, not per-file dictionary offsets

The brief (and the session that wrote it) claimed an interned id is a dictionary
offset local to its database file. False. `src/spine.rs:52-57`:

```rust
pub fn of(text: &str) -> Self {
    if text.is_empty() { return Self::EMPTY; }
    Self(hash64(text.as_bytes()))
}
```

`src/spine.rs:63-65` reinterprets that `u64` as the `i64` cell value.
`src/engine/meta.rs:280-283` declares `_strings(id INTEGER PRIMARY KEY, content TEXT NOT NULL)`.
Id 4711 decodes to the same string in every database, because both sides computed
it from the same bytes with the same function. `src/lower.rs:36-38` (`sym_lit`)
hashes a text literal AT COMPILE TIME with the same call, which only works
because the mapping is a pure function of the text.

Consequences: cross-database ATTACH+join was never meaningless, only unimplemented
(the zero-`ATTACH` finding stands). Migration by union is mechanically sound:
`INSERT OR IGNORE INTO main._strings SELECT * FROM other._strings` is correct and
idempotent. Cold rebuild is still recommended, for the reasons in section 7.

### 0.2 There are ~30 unscoped whole-table wipes, not 8

Beyond the 8 named in `derive.rs` and `declare.rs`:

| File:line | Note |
|---|---|
| `src/db.rs:1254` | `reload_rel`, the generic wipe-and-reload behind every `refresh_rel` caller. Scoping this one covers the bulk of `extract/`. |
| `src/engine/meta.rs:1443` | |
| `src/engine/term_extract.rs:102` | |
| `src/engine/rpc.rs:361` | |
| `src/engine/derive.rs:2102`, `2103` | node/edge table pair |
| `src/engine/derive.rs:2353` | closure head |
| `src/engine/derive.rs:2466` | scc head |
| `src/engine/extract/mod.rs:556,560,567,574,1277,1284,1291,1298` | module/type edge tables |
| `src/engine/cold_stage.rs:496` | |
| `src/storage/call.rs:358`, `561-566`, `799` | |

### 0.3 `repo` cannot carry the scoping

`repo` is a data attribute (which repository a fact is ABOUT). The wipe needs an
ownership attribute (which program set PRODUCED the row). They diverge exactly
where it matters: `src/engine/repo.rs:229-242` fans one rule across every
configured repo under `scan("*")`, so a cross-repo join yields rows with no
single `repo` value. Conflating the two repeats the class of mistake that
produced the per-directory split.

## 1. Reading of the directive

> "i dont want multiple dbs i want 1, rels should be tabled by their name from
> home or some shit as a meta, if collision then the next tables with those
> names get a new modpath"

The obvious reading (collision = name reuse across programs) defeats the purpose:
`sprefa/.dl` and `smashy/.dl` both declaring `call_def` would split into two
tables, and a `scan("*")` query would have to union identical shapes. The value of
one database is that `SELECT DISTINCT repo` returns more than one row and a single
query answers across all of them.

**Adopted reading: collision is a SHAPE disagreement, not a name reuse.** Two
programs declaring `call_def(repo, sym, kind, file, line, end)` have agreed, and
share `rel_call_def` with rows separated by an ownership column. Two programs
declaring `metric(name, value)` and `metric(name, value, unit)` have collided, and
the second gets `rel_<modpath>__metric`.

This is forced by existing code. `src/engine/declare.rs:325-360` implements a drift
check: when the declared column set, primary key set, or `WITHOUT ROWID` mode
disagrees with the cached table, it drops the view, drops the table, and deletes
the `_reldigest` and `_derived_complete` rows. With three roots in one namespace
and no collision rule, two programs disagreeing on `metric`'s shape would make
each root's tick drop and recreate the other's table forever, thrashing
`sqlite_master` pages into the WAL. The collision rule exists to stop the drift
check firing on a legitimate disagreement, and has no work to do when shapes agree.

## 2. Type signatures

### 2.1 Rel identity and the meta layer

```rust
// src/engine/relmap.rs (new module)

/// The shape a declaration commits to. Two decls collide iff names match and
/// signatures differ. Covers everything declare.rs:325-360 compares, plus the
/// interned flag (a text column and a sym column share a storage class but
/// differ in decode semantics).
pub struct ShapeSignature {
    /// (column name, Col::sql() storage class, Col::interned()) in DECLARED
    /// order. Order matters: declare.rs:395-401 builds the default PRIMARY KEY
    /// in declared order, and derive.rs:493's mirror INSERT selects by a
    /// positional cols_csv.
    pub columns: Vec<(String, &'static str, bool)>,
    /// `key(...)` narrowing, SORTED (declare.rs:355 already compares the PK as
    /// a sorted set, since ON CONFLICT matches a constraint order-free).
    pub key_sorted: Option<Vec<String>>,
    pub merge: Option<String>,   // `merge(MaxBy(col))` canonical text
    pub port: Option<String>,    // `@in(rpc)` / `@out(rpc)`
}

pub fn signature_of(decl: &RelDecl) -> ShapeSignature;
// Read decl.cols into (name, sql(), interned()). Clone+sort decl.key.
// Render merge via col_and_cmp(). Render port.
// Deliberately EXCLUDES pk_never_null: wants_without_rowid (declare.rs:200-207)
// is a pure function of cols + key + pk_never_null, and pk_never_null is only
// set by Rust-authored builtin decls, seeded once and never colliding with a
// .dl decl. Flagged in section 10 as uncertain.

/// Where a declaration came from. Stable across runs and machines, because it
/// is built from the repo slug and the ROOT-RELATIVE program path, never an
/// absolute path or a hash of one.
pub struct DeclOrigin {
    pub owner_slug: String,        // e.g. "sprefa"
    pub program_rel_path: String,  // e.g. ".dl/flow-panel.dl"
}

pub fn modpath_of(origin: &DeclOrigin) -> String;
// Join slug and relative path with '_'. Strip leading ".dl/", strip trailing
// ".dl". Lowercase. Non [a-z0-9] -> '_'. Collapse runs. Trim. Truncate to 40
// bytes at a char boundary.
// "sprefa" + ".dl/flow-panel.dl" -> "sprefa_flow_panel".
// PURE: no counter, no timestamp, no absolute-path hash. Determinism is why it
// avoids key_of (daemon/home.rs:91), which hashes the absolute canonical path
// and differs per machine and per worktree.

pub enum TableAllocation {
    Existing  { table: String },                    // this (name, sig) owns a table
    Bare      { table: String },                    // first claimant: rel_<name>
    Qualified { table: String, modpath: String },   // bare name held by another sig
}

impl Engine {
    /// The ONLY function permitted to compute a rel's table name from this
    /// point forward. lower::tbl (src/lower.rs:6) becomes a private seed-path
    /// fallback.
    pub(crate) fn allocate_table(&mut self, decl: &RelDecl, origin: &DeclOrigin)
        -> Result<TableAllocation>;
    // sig_hash = blake3-16hex of canonical ShapeSignature rendering.
    // Look up self.relmap (bulk-loaded ONCE per tick; a per-decl query on a
    // 200-decl program is an N+1 and a blocking defect).
    //   exact (rel_name, sig_hash) match      -> Existing
    //   no row for rel_name at all            -> Bare, stage an INSERT
    //   rows exist, none with this sig_hash   -> Qualified, stage an INSERT
    // The EXISTING table is never dropped, renamed, or migrated. First claimant
    // keeps the bare name permanently; the _relmap row makes that survive a
    // restart. Staged inserts flush in ONE batched insert_rows.

    pub(crate) fn table_for(&self, rel_name: &str, origin: &DeclOrigin)
        -> Result<String>;
    // 0 rows -> error naming the rel.
    // 1 row  -> that table (the common case; no collision ever happened).
    // n rows -> prefer owner_modpath == modpath_of(origin); else the bare-named
    //   one; else error listing every candidate table and its declaring
    //   program. A query from a third program against an ambiguous name must
    //   fail loudly rather than silently pick a table.
}
```

### 2.2 Ownership scoping

```rust
/// The interned id of the program set that produced a row. NOT `repo`.
/// Physically an INTEGER column `__owner` holding StringId::of(slug).sqlite().
/// Precedent: `__src`, a universal storage column already appended to every rel
/// table and excluded from the decl's column set (declare.rs:373-375 emits it,
/// declare.rs:298-300 skips it when reading PRAGMA table_info).
pub struct OwnerId(pub i64);

impl Engine {
    pub(crate) fn owner_id(&self) -> OwnerId;
    // StringId::of(&self.self_slug()).sqlite(), computed once at construction.

    /// Replaces every `DELETE FROM {tbl(rel)}`.
    pub(crate) fn wipe_owned(&self, rel: &str) -> Result<()>;
    // DELETE FROM {table_for(rel)} WHERE __owner = ?1
}
```

**Why an implicit column, against two alternatives.**

*Explicit and threaded* (a real declared `owner` column authors write): rejected.
Changes the arity of every rule in every `.dl` file, puts a storage concern in the
user's surface language, and `src/engine/derive.rs:2455-2461` hard-asserts `scc`
heads have exactly two columns, so an extra column breaks the graph operators.

*Session-scoped connection variable*: rejected on two grounds. `src/db.rs:1410-1425`
(`open_read_only`) creates a separate connection for the daemon's lock-free read
path and `src/daemon/root.rs` holds a `ReadView` per served root, so the variable
must be set identically on every connection and a missed one silently returns the
wrong root's rows. It also makes every generated SELECT depend on hidden
connection state, defeating the `dl daemon health` idiom of reading the database
from a cold third process (`src/cli/health.rs:1-21`).

*Implicit column, engine-injected*: PICKED. Cost, stated plainly: 8 bytes per row
on every `rel_*` table, and `__owner` enters the composite PRIMARY KEY of every
table without a `key(...)` narrowing, so the `WITHOUT ROWID` classifier at
`declare.rs:200-207` (capped at 2..=4 columns) stops selecting tables that
currently qualify. On a 986MB corpus that is a measurable regression. It buys the
only scoping correct under `scan("*")` that survives a cold third-process read and
needs no `.dl` surface change. Mitigation in section 6.

### 2.3 Program set per repo

```rust
impl Daemon {
    fn shared_db_path() -> PathBuf;  // daemon_home().join("db.sqlite")
    // Replaces daemon/home.rs:84-87 root_db_path, which is the whole bug.
}
```

Three roots load three program sets into three `Engine`s (`src/daemon/root.rs:56-70`:
`ServedRoot` holds its own `prog: Mutex<Program>` and `eng: Mutex<Engine>`). Each
runs a declare pass:

- Name unknown to `_relmap`: first claimant takes `rel_<name>`. The other roots
  later declare the same name with the same signature, get `Existing`, and point
  at the same table. Rows coexist, separated by `__owner`. A query from either
  root sees only its own rows because lowering appends the owner predicate. A
  cross-root query omits it, which is the capability one database unlocks.
- Name known, signature differs: the second root gets `rel_<modpath>__<name>`.
  The first root's table is untouched. The drift check never fires, because each
  engine compares its own decl against its own allocated table.

**Load-order dependency, stated rather than hidden**: on a cold database, whichever
root declares a conflicting name first wins the bare name. Mitigation: seed
`_relmap` from `all_builtin_decls()` (`src/engine/decls.rs:19-47`) at database
initialization, before any user program loads, so all ~200 builtin names hold
their bare tables unconditionally. User-vs-user collisions remain first-come, and
the `_relmap` row makes the outcome permanent. Residual risk, section 10.

### 2.4 Instance lifetimes

| Type | Lives in | Created | Destroyed | Notes |
|---|---|---|---|---|
| `RelMap` (`HashMap<String, Vec<RelMapRow>>`) | new field on `Engine` (`src/engine/mod.rs:432`) | `Engine::open`, one bulk SELECT | with the `Engine` | Refreshed only when a declare pass allocates. Never re-queried per decl. |
| `OwnerId` | new field on `Engine` | `Engine::open` from `self_slug()` | with the `Engine` | Immutable. |
| `DeclOrigin` | per `RelDecl` | at parse, from `Rule.origin` | with the `Program` | `src/engine/repo.rs:269` already reads `rule.origin`. |
| `_relmap` table | the shared db | meta bootstrap (`src/engine/meta.rs:209-240`) | never; append-only | The only permanently append-only engine table. |
| `ServedRoot` | `Daemon::roots` (`src/daemon/mod.rs:329`) | `add_root` | `drop_root` | Unchanged except the db path. |
| single writer lease | apalis row in `<home>/jobs.sqlite` | per tick job | on completion | Section 5. |

## 3. Storage layout

### 3.1 New table

```sql
CREATE TABLE IF NOT EXISTS _relmap (
    rel_name       TEXT    NOT NULL,
    sig_hash       TEXT    NOT NULL,
    table_name     TEXT    NOT NULL,
    owner_modpath  TEXT    NOT NULL DEFAULT '',
    owner_slug     TEXT    NOT NULL DEFAULT '',
    program_path   TEXT    NOT NULL DEFAULT '',
    sig_json       TEXT    NOT NULL DEFAULT '',
    allocated_at   INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (rel_name, sig_hash)
);
CREATE UNIQUE INDEX IF NOT EXISTS _relmap_table ON _relmap(table_name);
```

`sig_json` earns its place: without it a "rel `metric` is ambiguous" error can
only print table names and the user cannot see which shape is which.
`allocated_at` is diagnostic only. The unique index on `table_name` is the
structural guard against `modpath_of` ever producing a taken name: the insert
fails loudly instead of two rels silently sharing a table.

### 3.2 Column on every `rel_*` table

`__owner INTEGER NOT NULL DEFAULT 0`, emitted beside `__src` by the CREATE TABLE
builder at `src/engine/declare.rs:368-380`. Default 0 is `StringId::EMPTY`
(`src/spine.rs:50`), which no real slug hashes to, so `__owner = 0` is detectable
as pre-migration or engine-internal.

### 3.3 Path changes

| Path | Fate |
|---|---|
| `daemon_home()/db.sqlite` | the one database |
| `daemon_home()/roots/` | deleted entirely |
| `daemon_home()/roots.json` | KEPT. Still records which roots are served and watched, a live concern independent of storage (`src/daemon/root.rs:761,780`). |
| `daemon_home()/jobs.sqlite` | unchanged (`src/daemon/mod.rs:478`), and the existing precedent for one shared SQLite file across all roots |

### 3.4 What is NOT owner-scoped

- **Global, content-addressed, no change**: `_strings` (`meta.rs:280`), `_files`
  (`meta.rs:284`), `_where_bytes` (`meta.rs:290`). Keys are content hashes
  (`spine.rs:52`, `:23-28`, `:30-43`), so identical content from two roots
  produces the identical row and `INSERT OR IGNORE` is a no-op. These become
  genuinely shared, the largest single storage win of the merge.
- **Already carry `repo`, no change**: `_file`, `_prov` (`meta.rs:209-211`,
  backfilled `meta.rs:492,506`), `_stale_file` (`meta.rs:1292`).
- **Need an owner column**: `_reldigest`, `_derived_complete`, `_shapes`,
  `_stmt_ms`, `_repo`, `_program`, `_carry_meta`, `_node_embeddings`,
  `_node_emb_seen`, `_write_ledger`, `_query_log` (all in `meta.rs:209-410`).
  Each is keyed by rel name or program path and is per-root by nature; sharing
  them unscoped would let root A's `_derived_complete` marker vouch for root B's
  rows, the exact failure the completion-marker discipline at `derive.rs:472-482`
  prevents.

**This layer disagrees with section 2.** Section 2 says `__owner` is on every rel.
This layer says it is on `rel_*` plus eleven `_*` tables and nothing else. This
layer is the authority: `OwnerId` is a value the engine carries, and where it is
physically stored is a per-table decision made here.

## 4. Sequence of reads and writes

### 4.1 Declare pass, per engine, per full tick

1. Read `_relmap` in full, once, into `self.relmap`. One SELECT, no parameters.
2. For each decl in declared order: `signature_of`, in-memory lookup, decide
   `TableAllocation`, push new rows onto a staging vec. No database access inside
   this loop.
3. Flush the staging vec with one `insert_rows` (`src/db.rs:1265`). Zero rows on a
   warm tick, so zero writes.
4. Run the existing declare DDL (`declare.rs:212-410`) against the ALLOCATED name
   rather than `tbl(&d.name)`. The drift check now compares a table against the
   decl that allocated it, so it fires only on a genuine in-place shape edit by
   the same owner, the case it was written for.
5. Rebuild `_txt` views (`declare.rs:115-155`) against allocated names. The
   skip-if-unchanged check at `declare.rs:141-148` keeps this write-free warm.

### 4.2 Derived rebuild, per component

Today (`derive.rs:472-500`): unmark completion, `DELETE FROM rel_x`, inject-crash
window, refill, mark.

After: unmark completion for `(rel, owner)`, `DELETE FROM <allocated> WHERE
__owner = ?`, inject-crash window, refill with `__owner` appended, mark.

`derive.rs:493-495` becomes:
`INSERT INTO {table} ({cols_csv}, __owner) SELECT {cols_csv}, {owner_id} FROM {mirror_table}`.
The mirror is a per-tick TEMP table and needs no owner column.

The digest-before-write skip at `derive.rs:465-480` must be namespaced by owner,
or root A's unchanged digest makes root B skip a rebuild it needs. `_reldigest`
gaining an owner column handles it, and `rel_digest_fingerprint`
(`declare.rs:29-46`) needs the owner threaded into its three key strings.

### 4.3 The eight named wipe sites

| Site | Today | After |
|---|---|---|
| `derive.rs:201` | node2vec head rebuild | `wipe_owned(head)` |
| `derive.rs:484` | mirror-path component wipe | `wipe_owned(&head_rel)` |
| `derive.rs:510` | legacy per-rel component wipe | `wipe_owned(rel)` |
| `derive.rs:546` | orphan-rel reconcile wipe | `wipe_owned(rel)` |
| `derive.rs:1123` | native multi-source walk head | `wipe_owned(&head_rel)` |
| `derive.rs:1536` | native BFS head replace | `wipe_owned(&head_rel)` |
| `declare.rs:989` | `rel_true` singleton | `wipe_owned("true")`, then `declare.rs:990-993`'s `INSERT ... DEFAULT VALUES` becomes `INSERT OR IGNORE INTO {table} (__owner) VALUES (?)`. Each owner gets its own singleton, correct because `true()` is that owner's range anchor for negation-only rules. |
| `declare.rs:1013` | `rel_every` cadence | `wipe_owned("every")`. `refresh_every` reads `_carry_meta` per interval (`declare.rs:1020-1026`); with an owner column two roots on different cadences stop clobbering each other's bucket stamps. A latent bug fix, not only a scoping change. |

The section 0.2 sites collapse mostly into one: `src/db.rs:1254` `reload_rel`
gains an owner parameter, covering every `refresh_rel` caller. The rest take
`wipe_owned` directly, except `storage/call.rs` and `cold_stage.rs`, which operate
on per-root staging tables and belong in the add-an-owner-column bucket.

### 4.4 Query lowering

`src/lower.rs:6` `tbl` and `:7` `txt_tbl` are called with only a rel name in hand.
Lowering should be handed a resolved `HashMap<String, String>` name-to-table map
built once per tick: one lookup structure, keeps `lower.rs` free of engine state,
and makes "which table did this query read" inspectable for `dl daemon health`.

Every generated SELECT over a `rel_*` table gains `AND __owner = <literal>` unless
explicitly cross-owner. The owner id is a compile-time integer literal, so it
costs nothing beyond the index probe, and it must be the leading column of any
index that stays useful. Section 10 flags the index consequence as unresolved.

## 5. Write contention

### 5.1 What the code does today

- `src/db.rs:210`: `PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;` on every
  writer connection.
- `src/db.rs:248-260`: a CUSTOM busy handler replacing `PRAGMA busy_timeout`.
  Deadline 5000ms, 20ms backoff, emits a `[sqlite]` verdict past
  `SQLITE_BUSY_WARN_MS`. Other timeouts: `src/db.rs:1418` 1000ms on read-only
  connections, `src/engine/generation.rs:64` 1000ms, `src/invlog.rs:80` 5000ms,
  `src/jobq/workers.rs:61` 5s.
- `src/db.rs:229-247`: a throwaway `BEGIN IMMEDIATE` probe under a 50ms timeout on
  every open, warning that another process holds the write lock. **With three
  roots on one file this fires constantly and becomes noise.** Must be gated to
  callers that are not themselves a served root.
- `src/engine/tick.rs:12-60`: `BulkRebuildIo` sets `PRAGMA synchronous=OFF;
  wal_autocheckpoint=0` for a full derived rebuild and on Drop restores them and
  runs `PRAGMA wal_checkpoint(TRUNCATE)`. Two problems with a shared WAL.
  Autocheckpointing is disabled GLOBALLY for the file while any one root
  rebuilds, so the other roots' writes pile into an uncheckpointed WAL. And
  `wal_checkpoint(TRUNCATE)` needs no other connection reading or writing, so with
  three roots it routinely returns busy and the WAL grows without bound. The
  `tick.rs:20-25` justification for `synchronous=OFF` stays true; its checkpoint
  reasoning was written for one writer.

### 5.2 The discipline: one writer, already bought

`src/jobq/mod.rs:1-40` documents the durable job store as apalis-sqlite over a
shared `<home>/jobs.sqlite` (`src/daemon/mod.rs:478`), with fetch, lock, ack,
priority, `run_at` scheduling, retry, worker registration, and orphan re-enqueue
all supplied by apalis. What the repo kept is an admission layer, and
`jobq/mod.rs:22-27` already describes the needed primitive:

> **root-serialized ColdExtract admission**: a second root's cold jobs are pushed
> HELD (`run_at` far future + a metadata flag) until the active root has no
> runnable cold work; `reconcile` promotes the next root. Replaces the old
> `active_cold_root` claim scoping (2026-07-18 incident: 4 roots cold-rebuilt
> concurrently).

The change is to widen that admission key from cold-extract work to all engine
write work. One predicate change plus the matching `reconcile` promotion arm. No
new queue, no new scheduler, no new thread.

This does not contradict the daemon architecture: `src/daemon/root.rs:77-85`
already routes ticks through `enqueue_job` onto the shared queue, and
`src/daemon/root.rs:110-160` runs `tick_full` under `lock(&self.eng)` on a
blocking thread. Ticks are already serialized per root and already dispatched
through the job queue. Serializing across roots is the same mechanism, wider key.

**Library candidates.**

| Candidate | Verdict |
|---|---|
| apalis / apalis-sqlite | **PICKED.** Already a dependency, already the queue, already implements root-serialized admission for one job kind. Extending the key is local and has existing tests (`src/jobq/tests.rs`). |
| r2d2 / deadpool pooling | Rejected. Solves the wrong problem: a pool produces MORE concurrent writers, and SQLite in WAL permits exactly one regardless of pool size, converting serialization into `SQLITE_BUSY` retries. |
| SQLite `BEGIN CONCURRENT` | Rejected. Would genuinely allow multiple writers with page-level conflict detection, but lives on a non-mainline branch and is absent from `rusqlite = "0.32.1"` bundled. Adopting it means vendoring a non-mainline SQLite. Disproportionate. |
| `sqlx` | Rejected as not applicable. Already present transitively under apalis, but supplies pooling and migrations, not cross-writer scheduling. |
| `parking_lot::Mutex` / `tokio::Semaphore` around the write span | Rejected. This is the write-our-own answer, and technically a process-local lock does not cover the one-shot CLI, hook, and LSP, which open the same file from separate processes (`src/cli/mod.rs:424`, `src/hook.rs:468`, `src/lsp.rs:91`). Only a database-mediated lease covers those. |

**Non-negotiable side change**: `BulkRebuildIo` must not disable
`wal_autocheckpoint` globally while other roots may write. Either it acquires the
single-writer lease before entering (free once admission is widened, since the
lease is held for the whole tick job) or it stops touching `wal_autocheckpoint`.
With the lease held it is safe as written and the TRUNCATE checkpoint succeeds.
This is a genuine argument for the lease being required rather than nice to have.

## 6. Uniqueness conditions

1. `_relmap` PK `(rel_name, sig_hash)`: same name, same shape maps to one row and
   one table by construction.
2. `UNIQUE INDEX _relmap_table ON _relmap(table_name)`: no two rel identities ever
   share a physical table.
3. At most one `_relmap` row per `rel_name` has `owner_modpath = ''`. Implement as
   `CREATE UNIQUE INDEX _relmap_bare ON _relmap(rel_name) WHERE owner_modpath = ''`.
4. Rel with no `key(...)`: `PRIMARY KEY (<declared cols>, __owner)`. Two owners
   deriving the identical tuple get two rows, correct because each owner's wipe
   must remove exactly its own.
5. Rel with `key(a, b)`: `PRIMARY KEY (a, b, __owner)`. Preserves the functional
   dependency the `key(...)` qualifier promises (`declare.rs:385-395`, citing
   Soufflé APLAS'21) WITHIN an owner, and lets two owners hold different values
   for the same key. The `merge(...)` lattice (`declare.rs:402-415`) then operates
   within an owner, the only coherent reading.
6. `wants_without_rowid` (`declare.rs:200-207`) caps at 2..=4 columns. Adding
   `__owner` pushes 4-column rels to 5 and disqualifies them. The cap must widen
   to 2..=5; the all-INTEGER test still passes since `__owner` is INTEGER.
   Without this, the merge silently un-optimizes tables the storage-diet work
   deliberately optimized.
7. `_strings` uniqueness across owners: `id INTEGER PRIMARY KEY` on a content hash
   (`meta.rs:280`, `spine.rs:52`). Three roots interning `"call_def"` write one
   row. The merge's largest win, needing no code change.
8. `__owner = 0` is reserved. `StringId::EMPTY` is the empty-string id and no real
   slug produces it, so any row carrying it after cutover is a bug worth a loud
   assertion in `dl daemon health`.

**Conditions disagree with section 3 on one point.** Section 3 says
`__owner INTEGER NOT NULL DEFAULT 0`; condition 8 says 0 is a bug. Both are
intentional: the default exists so `ALTER TABLE ADD COLUMN` is legal on an
existing table, and the assertion exists so no code path relies on it. Since
section 7 recommends cold rebuild, the default should never be exercised.

## 7. Migration

**Recommendation: cold rebuild. The separately queued nuke-and-rebuild makes this
a non-problem.**

A real migration is mechanically possible (contrary to the brief's premise, per
0.1): ATTACH each database, `INSERT OR IGNORE INTO main._strings SELECT id,
content FROM other._strings` (safe, ids are content hashes), then per rel table
`INSERT INTO main.<allocated> (<cols>, __src, __owner) SELECT <cols>, __src,
<owner_literal> FROM other.rel_<name>`.

It should not be done:

1. `_relmap` cannot be synthesized from a database file alone. Allocation needs
   the `RelDecl`, including `key`, `merge`, `port`, and interned flags.
   `PRAGMA table_info` recovers column names and storage classes, never whether a
   column is `sym`-interned versus plain TEXT, and never the `merge` function.
   A migration would have to run a declare pass anyway, at which point the
   rebuild is most of the work.
2. The 986MB is largely not worth carrying: `_strings`, `_files`, `_where_bytes`
   deduplicate across the three databases, and only a rebuild reveals the real
   post-merge size.
3. `_derived_complete` and `_reldigest` would need owner backfill and would be
   untrustworthy afterward: a marker migrated from database A vouches for rows
   rewritten during migration. The safe move is to leave them empty and let the
   first tick rebuild, which is a cold rebuild by another name.
4. A queued item already nukes and cold-rebuilds every database.

Migration is therefore: delete `~/.local/state/sprefa/roots/`, leave `roots.json`
so the three roots re-register, start the daemon, let it cold-build one
`db.sqlite`. The single-writer lease is a PREREQUISITE, because three simultaneous
cold builds against one file without it is the 2026-07-18 incident
(`jobq/mod.rs:26`) with a shared WAL added.

## 8. What this deletes

**Per-root database directories**

| Site | Fate |
|---|---|
| `src/daemon/home.rs:72-73` `root_db_dir` | deleted |
| `src/daemon/home.rs:78-87` `root_db_path` | deleted (its doc comment is the one already claiming "ONE db per corpus") |
| `src/daemon/mod.rs:303-304` | `root_db_dir(&key).join("db.sqlite")` in `add_root` -> constant shared path |
| `src/daemon/mod.rs:624-625` | same on the initial-root registration path |
| `src/daemon/root.rs:664` | `create_dir_all(root_db_dir(k))` deleted |
| `src/cli/mod.rs:424` | one-shot default db -> shared path |
| `src/hook.rs:468` | same |
| `src/lsp.rs:91` | same; the `src/lsp.rs:85-90` comment about editor-spawned servers opening a wrong-key db describes a failure class that stops existing |

**The orphan-root failure class**

| Site | Fate |
|---|---|
| `src/daemon/mod.rs:340` | `remove_dir_all(root_db_dir(&key))` in `drop_root(purge)`. No directory to purge; `purge` either goes away or becomes an owner-scoped delete across every `rel_*` table, a much cheaper operation |
| `src/daemon/client.rs:292` | daemonless twin of the same purge |
| `src/cli/daemon_cmd.rs:284` | help text mentioning orphan roots |

**The class-14 worktree cold-check rail**

| Site | Fate |
|---|---|
| `src/hook.rs:452-487` `refuse_worktree_cold_check` | deleted in full. Its doc comment states the incident: every agent worktree's pre-commit `dl --check` cold-built a ~600MB `roots/<key>/db.sqlite` that orphaned the moment the worktree was deleted (2026-07-19, three 593MB orphans in one overnight fleet wave). With one database keyed to nothing, a worktree check warms the shared database and there is no orphan to create |
| `src/hook.rs:448-450` `is_linked_worktree` | sole caller was the above |
| `src/cli/mod.rs:471-484` | the `--check` call site, its eprintln, and the green-by-skip `exit(0)` arm |
| `DL_ALLOW_WORKTREE_COLD` (`src/hook.rs:453`) | goes with it |

Caution: `refuse_worktree_cold_check` also stopped a worktree check from doing an
expensive cold build at all. After the merge the build is not wasted, but it is
still expensive and still inside a pre-commit hook. `src/hook.rs:439-446`
`inproc_db_is_cold` and the `cold_skip` gate at `src/hook.rs:495-515` remain and
still cover the deadline case, so the budget rail survives even though the orphan
rail does not. Verify rather than assume.

**The `dl daemon health` orphan probe**

| Site | Fate |
|---|---|
| `src/cli/health.rs:68-101` `report_roots_overview` | the whole per-key directory walk and registered-versus-orphan classification |
| `src/cli/health.rs:106-119` `orphan_origin` | the `_repo` / `_program` probe guessing which worktree an orphan came from |
| `src/cli/health.rs:45,58-63` | the call and the closing `rm -rf` hint |
| `src/cli/health.rs:50` | `home.join("roots").join(&rec.key).join("db.sqlite")` -> shared path, so `report_db` runs once rather than once per root |
| `src/cli/health.rs:1-8` | module doc section 1 and its orphan description |

Health gains a `_relmap` listing in exchange: each rel name, its allocated tables,
and which program claimed each, since that is the new thing a user cannot
otherwise see.

**A latent bug that disappears**: `src/engine/declare.rs:1013-1026` wipes
`rel_every` and consults `_carry_meta` per interval. With one database and
`_carry_meta` gaining an owner column, two roots naming the same cadence interval
stop sharing a bucket stamp. Today they are in separate files so it never
surfaced; without the owner column the merge would INTRODUCE it. Named here so it
is not discovered as a regression.

## 9. Ordering

Each step is independently landable against the current per-root layout, so the
merge is last and everything before it is a behavior-preserving refactor.

**Step 1: `_relmap` table and allocation, unused.** Add `src/engine/relmap.rs`.
Add the DDL to the `meta.rs:209-410` bootstrap. Seed from `all_builtin_decls()`
(`decls.rs:19-47`). Nothing calls `table_for` yet.
Validate: `cargo test --test it relmap`, asserting `modpath_of` is pure and
stable, two identical decls produce one row, a shape disagreement produces a
second row with a distinct `table_name`.

**Step 2: route table naming through `table_for`.** Replace every `tbl(...)` in
`declare.rs`, `derive.rs`, `meta.rs`, `extract/`, `rpc.rs`, `term_extract.rs`,
and lowering. On a single-root database with no collisions this is a byte-identical
no-op.
Validate: full suite green, plus a snapshot diff of
`SELECT name FROM sqlite_master ORDER BY name` against
`src/snapshots/sprefa_v5__db__tests__rel_stats_snapshot_of_schema.snap`.

**Step 3: `__owner` column, written but not read.** Add it to the CREATE TABLE
builder (`declare.rs:368-380`) and to the eleven `_*` tables. Widen
`wants_without_rowid`'s cap to 2..=5 (`declare.rs:206`). Populate on every insert.
No primary keys and no DELETE scoping yet.
Validate: rebuild, then `SELECT COUNT(*) FROM rel_call_def WHERE __owner = 0`
returns 0. Full suite green.

**Step 4: scope the wipes.** Introduce `wipe_owned`. Convert `src/db.rs:1254`
`reload_rel` FIRST, since it covers the most call sites. Then the eight named
sites, then the 0.2 sites. Add `__owner` to primary keys per conditions 4 and 5,
which trips the drift check at `declare.rs:325-360` and drops and recreates every
table on the next tick, exactly as designed.
Validate: on a single root this is still a no-op. The real test opens one
database, runs two `Engine`s with different `self_slug` values and a shared rel
name, ticks both, and asserts each sees only its own rows and that ticking A does
not change B's count.

**Step 5: widen the apalis admission key.** One predicate change plus the
`reconcile` promotion arm. Gate the `BEGIN IMMEDIATE` warn probe
(`src/db.rs:229-247`) so a served root does not warn about itself.
Validate: `cargo test --test it jobq`, plus a test registering three roots against
one file asserting no `[sqlite] busy retry` verdict during a three-root tick storm
(the verdict at `src/db.rs:252-260` is the observable).

**Step 6: flip the path.** Delete `root_db_dir` and `root_db_path`, add
`shared_db_path`, update the six call sites. Delete `refuse_worktree_cold_check`
and its call site. Rewrite `cli/health.rs` sections 1 and 2.
Validate: `rm -rf ~/.local/state/sprefa/roots`, `dl daemon start`, settle, then
`dl daemon health` shows one database and
`SELECT DISTINCT __owner FROM rel_call_def` returns three rows.

**Step 7: cold rebuild and measure.** Register all three roots, record the file
size against the 986MB baseline. The `_strings` deduplication shows up here and
nowhere else.
Validate: `dl daemon health` end to end; the class-17 database-to-corpus ratio
verdict (`src/db_ratio.rs`, surfaced in `cli/health.rs`) is the headline number.

## 10. Risks, and where this design is guessing

**Verified and confident**: `StringId` is a content hash (`spine.rs:52-57`).
`tbl` is a one-line `format!` at `lower.rs:6` with no meta layer today. The drift
check (`declare.rs:325-360`) thrashes under a shared namespace without a collision
rule. `busy_timeout` is 5000ms on writers via a custom handler (`db.rs:248-260`)
and 1000ms on readers (`db.rs:1418`). `jobs.sqlite` is already one shared file
across all roots (`daemon/mod.rs:478`) with root-serialized admission
(`jobq/mod.rs:22-27`), so the single-writer answer is bought and not built. There
are far more than 8 unscoped wipes.

**Guessing, in descending order of how much it could hurt:**

1. **Index design under `__owner`.** Every generated SELECT gains an owner
   predicate, and the auto-index demand machinery (`declare.rs:1-46`, the
   `TINY_REL_FLOOR` at `declare.rs:8`, the `IdxDemandProbe` cache at
   `engine/mod.rs:497-503`) proposes single-column indexes from join-key
   co-occurrence and has no notion of a mandatory leading predicate. Either
   `__owner` becomes the leading column of every auto-index, or the planner
   prefers the owner-less index and filters, costing a 3x scan on a table holding
   three roots. Not enough of the proposer was read to say which. Most likely
   source of a post-merge performance surprise.
2. **`storage/call.rs` bulk paths.** The wipes at `:358`, `:561-566`, `:799` and
   the `GONE`-predicate deletes at `:775-782` operate on `_call_*` staging tables
   with carefully tuned bulk shapes. The DELETE statements were read; the
   surrounding algorithm was not. Whether an owner predicate is a straightforward
   conjunct there is unknown.
3. **`pk_never_null` in the shape signature.** Excluded on the reasoning that only
   Rust-authored builtin decls set it (`ast.rs:176-200`) and those are seeded
   before any user program. If a future decl path can set it from `.dl`, two decls
   could agree on signature and disagree on `WITHOUT ROWID` mode, and the drift
   check would fire on a shared table. Including it is the safe choice and costs
   one field.
4. **Cold-database load-order determinism.** Two user programs colliding on a name
   means whoever declares first takes the bare table permanently. Seeding builtins
   first removes the important case. The residual is real, with no way to make it
   deterministic without an ordering rule the user would have to know (alphabetical
   by slug is the obvious candidate, not proposed because it silently reshuffles
   on a rename).
5. **Whether the per-component digest skip survives owner scoping.** The skip at
   `derive.rs:465-480` compares a mirror table against the live table; with three
   owners in the live table the comparison must be owner-scoped. The `_reldigest`
   owner column is the proposed fix, but `eval_component_mirror` (referenced at
   `derive.rs:485`) was not read closely enough to be sure the mirror comparison
   is owner-clean.
6. **`daemon health` sections 2 through 6.** They run once per root against
   separate files (`cli/health.rs:50-55`). Merged, the dbstat pass and per-rel
   ranking become global, arguably better, but the dupe probe (`DUPE_MIN_ROWS`,
   `DUPE_MAX_PAIRS_PER_GROUP` at `cli/health.rs:32-36`) starts comparing rels
   across owners and may report false duplicates that are the same rel from two
   roots. Needs an owner-aware EXCEPT.
7. **`BulkRebuildIo` after the lease.** The lease makes `synchronous=OFF` and
   `wal_autocheckpoint=0` safe for daemon-driven ticks. It does not obviously hold
   for a one-shot `dl` run or an LSP process that opens the shared file without
   going through the job queue (`cli/mod.rs:424`, `lsp.rs:91`). Those can be
   mid-read while a daemon tick has autocheckpointing off. Whether that is merely
   inefficient or actually harmful was not determined.
