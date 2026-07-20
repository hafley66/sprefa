# V6 table design — surrogate spine, one graph shape, sized for 500 repos

## Context

Measured on the live sprefa root db (`fbabddda40d22347`, 853MB, 2026-07-19),
plus `call_edge` queries over `src/**` run through dl itself
(`v6-minset.dl`, `v6-sqlleak.dl`).

Four defects, each with its receipt:

1. **Every rel table stores its data twice.** `PRIMARY KEY` covers every
   column (set semantics = the whole row is the key), so the PK autoindex is
   a full second copy. Buckets: rel tables 263.0MB, pk autoindexes 256.6MB.
2. **String ids are hash-valued i64.** `_strings.id` spans
   `-9223341090678652667 .. 9223358176835574204` for 939,842 rows that would
   be dense `0..939841`. Every id costs a full 8 bytes and defeats SQLite's
   varint encoding, in the table *and* in every index entry that references it.
3. **The dictionary is 92% re-encoded coordinates**, not vocabulary. Sampled
   300k of 939,842 rows: path 35.3%, rev-salted 33.5%, qualified scip symbol
   13.2%, `file:line:col` 10.2% — genuine short literals are **7.7% of rows
   and 2.6% of bytes**.
4. **`repo`/`rev` are smuggled into id strings.**
   `src/engine/extract/mod.rs:978` is `salt_rev(id, rev) = format!("{rev}\u{1}{id}")`;
   `extract/dataflow.rs:132-172` emits every node four times across
   `df_node` / `_rev` / `_repo` / `_repo_rev`, two of which mint a fresh
   interned string per `(node, rev)`. Cost: 163.6MB across four projections
   of one fact set, and a third of the dictionary.

The four graph families are the same shape wearing four schemas:

| family | node rel | edge rel | source |
|---|---|---|---|
| dataflow | `df_node(id, kind, var, fn, file, line)` | `df_edge(from, to)` | `engine/extract/dataflow.rs` |
| call | `call_def(repo, sym, kind, file, line, end)` | `call_edge(caller, callee, kind)` | `engine/extract/call.rs` |
| module | (implicit, file-keyed) | `module_edge(src, dst)` | `graph/modgraph/**` 2,828 lines |
| type | `type_entity(.., kind)` | `type_edge(.., kind)` | `graph/typegraph/**` 9,106 lines |

All four are: a located, kinded node + a kinded edge between two nodes. They
differ only in the per-language extractor that produces them.

## Decisions

### D1 — the surrogate spine (no hashing; ids are ids)

Normalized, third-normal-form. Every ref is a **surrogate**: either a dense
autoincrementing integer or a content hash. No value is denormalized into a
child table because it was convenient to have there.

**No `REFERENCES` clauses.** Declared foreign keys buy enforcement we do not
need and cost us insert ordering, `PRAGMA foreign_keys` per-connection state,
and migration pain. The `_id` suffix is the contract; a rail checks orphans on
demand instead. Refs are still refs — they just aren't policed by the engine.

`AUTOINCREMENT` is never used (it adds a second bookkeeping table); plain
`INTEGER PRIMARY KEY` *is* the rowid and stays dense.

```sql
CREATE TABLE repo (
  repo_id    INTEGER PRIMARY KEY,          -- u16 range in practice
  slug       TEXT NOT NULL UNIQUE,
  root       TEXT NOT NULL,
  url        TEXT NOT NULL DEFAULT ''
);

CREATE TABLE rev (
  rev_id      INTEGER PRIMARY KEY,
  repo_id     INTEGER NOT NULL,
  git_sha     BLOB    NOT NULL,            -- 20 raw bytes, never 40 hex chars
  observed_at INTEGER NOT NULL,
  UNIQUE (repo_id, git_sha)
);

CREATE TABLE file (
  file_id   INTEGER PRIMARY KEY,
  repo_id   INTEGER NOT NULL,
  path_id   INTEGER NOT NULL,              -- -> str
  UNIQUE (repo_id, path_id)
);

-- Content, addressed once by hash. A file byte-identical at ten revs, or in
-- ten repos, is ONE row. content_hash IS the surrogate here.
CREATE TABLE blob (
  blob_id      INTEGER PRIMARY KEY,
  content_hash BLOB NOT NULL UNIQUE,       -- 16 raw bytes (blake3 truncated)
  size_bytes   INTEGER NOT NULL,
  line_count   INTEGER NOT NULL
);

-- The (rev, file) -> content junction: a pure composite key, no surrogate.
-- This is the ONLY place rev meets file. Nothing else carries a rev string.
CREATE TABLE rev_file (
  rev_id   INTEGER NOT NULL,
  file_id  INTEGER NOT NULL,
  blob_id  INTEGER NOT NULL,
  PRIMARY KEY (rev_id, file_id)
) WITHOUT ROWID;

CREATE TABLE str (
  str_id   INTEGER PRIMARY KEY,            -- DENSE. Assigned sequentially.
  content  TEXT NOT NULL UNIQUE
);
```

Composite keys where the key is genuinely composite (`rev_file`, `edge_*`);
a surrogate only where something else needs to *point at* the row cheaply
(`node_id`, `str_id`) — pointing at a 4-column composite from an edge table
would denormalize those 4 columns into every edge, twice.

`str` holds **vocabulary only** — identifiers, kinds, path segments. It never
holds a coordinate, a qualified symbol, or a rev-salted id, because D2 and D3
remove the reasons those existed. Projected from the sample: 7.7% of today's
939,842 rows, ~72k entries per repo before cross-repo dedup.

### D2 — a span is a coordinate, never a string

A value that *is* a region of a file is stored as its coordinates and read
back from the blob on demand. Nothing that can be recomputed from
`(blob_id, byte_start, byte_len)` gets a dictionary entry.

```sql
-- embedded in whatever row needs it; not its own table
  blob_id     INTEGER NOT NULL REFERENCES blob(blob_id),
  byte_start  INTEGER NOT NULL,
  byte_len    INTEGER NOT NULL
```

This kills the 26.6%-of-bytes "qualified scip symbol" class and the
10.2% "file:line:col" class outright. Line/col are **derived**, never stored:
one `line_index(blob_id) -> Vec<u32>` built lazily per blob answers
byte-offset → (line, col) in a binary search.

Rejected: keeping a `text` column "for convenience". That convenience is
26.6% of the dictionary's bytes.

### D3 — `rev` is a column, `salt_rev` is deleted

`salt_rev` exists only because `df_node.id` is a string that had to stay
disjoint across revs. With `file_id` + `byte_start` as the node's identity
and `rev_id` as a column, revs are disjoint by construction. The four
`df_node*` projections collapse to one table.

Deletes: `src/engine/extract/mod.rs:978`, and the `_rev` / `_repo` /
`_repo_rev` twin families listed at `engine/decls.rs:607-727` and
`engine/family/mod.rs:315-327`.

### D4 — one graph shape, four physical tables

The four families share a Rust type and a schema template. They stay
*physically separate* so index locality holds and a family can go cold
independently (demand plan) — a single `family` discriminator column on one
giant table would defeat both.

```sql
-- template, instantiated as node_df / node_call / node_type / node_module
CREATE TABLE node_<family> (
  node_id     INTEGER PRIMARY KEY,         -- surrogate: edges point here
  rev_id      INTEGER NOT NULL,
  file_id     INTEGER NOT NULL,
  byte_start  INTEGER NOT NULL,
  byte_len    INTEGER NOT NULL,
  kind        INTEGER NOT NULL,            -- small enum ordinal, not a string
  name_id     INTEGER,                     -- -> str, NULL for anonymous nodes
  UNIQUE (rev_id, file_id, byte_start, kind)
);

-- template, instantiated as edge_df / edge_call / edge_type / edge_module.
-- Pure composite key. No surrogate: nothing points at an edge.
CREATE TABLE edge_<family> (
  src_id  INTEGER NOT NULL,
  dst_id  INTEGER NOT NULL,
  kind    INTEGER NOT NULL,
  PRIMARY KEY (src_id, dst_id, kind)
) WITHOUT ROWID;
CREATE INDEX edge_<family>_by_dst ON edge_<family>(dst_id);
```

`repo_id` is deliberately absent from both: it is reachable through
`rev_id` and through `file_id`, and copying it in would be denormalization
paid for on 150M rows.

`kind` is a `u8` ordinal into a compile-time enum per family, never a string
id. Today `df_node.kind` is a full 8-byte interned string id per row.

Consequence for the crate map: the per-language *extractors* (typegraph
9,106 + modgraph 2,828 = 11,934 lines, 96.4% of `src/graph/`) all produce
this one shape. They belong in a pure `sprefa-extract` crate — source text in,
nodes+edges out, no IO. `sprefa-graph` keeps the 445 lines that are actually
graph algorithms (`walk.rs` 251, `scc.rs` 180, `mod.rs` 14) plus the CSR
snapshot API. See the crate-map amendment todo below.

### D5 — junctions are `WITHOUT ROWID`, entities are not

- A table whose PK is a real surrogate (`node_id`, `str_id`) keeps its rowid:
  the PK *is* the rowid, one B-tree, zero duplication.
- A table whose PK is a composite of FKs (`rev_file`, `edge_*`) is
  `WITHOUT ROWID`: the table becomes the index instead of paying for both.

This is what removes the 256.6MB duplicate-copy bucket. Nothing in V6 gets an
all-columns PK.

### D6 — sizing for 500 repos

At sprefa density (~300k dataflow nodes, ~260k edges, ~72k vocabulary entries
per repo):

| table | 500-repo rows | id width | headroom on u32 |
|---|---|---|---|
| `repo` | 500 | u16 | — |
| `rev` (≈200 tracked per repo) | 100k | u32 | 43,000x |
| `file` | 1M | u32 | 4,300x |
| `blob` | ~3M | u32 | 1,400x |
| `str` (vocabulary only, cross-repo deduped) | 5–20M | u32 | 200x |
| `node_*` (all four families) | ~150M | u32 | 28x |
| `edge_*` (all four families) | ~130M | u32 | 33x |

**u32 everywhere**, and dense assignment means small ids varint-encode to 1–3
bytes in SQLite instead of the flat 8 that hash-valued ids force. Row width
for a dataflow node goes from 6 hashed i64 + `__src TEXT` (~50B stored, ~100B
with the duplicate PK index) to ~13–17B varint-encoded with no duplicate.

`u32` is a schema *budget*, not a Rust type: SQLite stores integers, and the
rail is a documented ceiling plus a test that asserts max ids stay under
`2^32`. Crossing it is a migration, not a corruption.

### D7 — one concrete `Store` struct, no trait, no enum-of-backends

Per the standing practicality ruling: concrete types until a second
implementation actually arrives. Backend choice is `BackendConfig` at
`open()`. The dialect match already exists inside sea-query — we do not write
it, and we do not wrap it in our own enum to write it again.

```rust
pub struct Store { /* one writer connection, backend chosen at open */ }
```

Rejected: `trait Store` with one impl (abstraction with nothing to abstract);
`enum Backend { Sqlite(..), Postgres(..) }` matched at every call site (the
"match the universe" shape — every new backend edits every method).

### D8 — the API is row/plan-shaped; 11 passthrough verbs die

Measured seam consumption (`call_edge`, callers outside `src/db.rs` +
`src/storage*`): 39 members reached externally, split as

| SQL-passthrough (caller composes SQL) | external callers |
|---|---|
| `exec_on` 37, `exec_params` 33, `exec` 18, `execute_batch_on` 13, `query_row` 6, `for_each_row` 6, `exec_in_chunks` 5, `query_in_chunks` 2, `execute_batch` 1, `exec_derived` 1, `prepare` 1 | ~123 sites |

| row/rel-shaped (kept, renamed) | external callers |
|---|---|
| `insert_rows` 28, `ReadDb.open` 18, `begin_immediate` 6, `flush_syms` 5, `pragma_i64` 5, `commit` 4, `tick_begin` 4 | — |

15 of the 39 members have exactly one external caller: inline them, do not
extract them.

Conversion debt for "the engine never speaks SQL": **104 passthrough call
sites inside `src/engine/`**, of which **44 are in files already slated to
move into `sprefa-store`** (`meta.rs` 17, `declare.rs` 11, `staged_delta/*`
13, `cold_stage.rs` 3). The genuine conversion is **60 sites**, concentrated
in `derive.rs` (18) and `extract/*` (19) — and `derive.rs` is the fixpoint
codegen, which the crate map already routes through `LoweredPlan` → sea-query.

### D9 — a rel's table does not exist until something subscribes

DDL is an effect of first materialization, not of declaration. A `rel`
declared in a program that nobody ever subscribes to has **zero bytes** and
zero schema objects.

Two schema populations, with different rules:

| population | tables | owner | lifecycle |
|---|---|---|---|
| **spine** (fixed) | `repo`, `rev`, `file`, `blob`, `rev_file`, `str`, `node_*`, `edge_*`, `trace_dep`, `rel_state` | migrations (`rusqlite_migration`) | created at `open()`, versioned |
| **rels** (dynamic) | one table per materialized derived rel | runtime DDL | created on first materialization, dropped when evicted |

This resolves the open impedance note in `v6-deps` ("generated `rel_*` DDL vs
rusqlite_migration's append-only model"): the migration framework never sees
a generated rel table, because generated rel tables are not schema — they are
cache. Migrations own the spine only, which is fixed and hand-written.

`rel_state` is the registry the demand layer reads:

```sql
CREATE TABLE rel_state (
  rel_id       INTEGER NOT NULL,           -- dense, from the loaded program
  rev_id       INTEGER NOT NULL,
  status       INTEGER NOT NULL,           -- 0 cold, 1 materialized, 2 stale
  row_count    INTEGER NOT NULL DEFAULT 0,
  bytes        INTEGER NOT NULL DEFAULT 0,
  last_read_at INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (rel_id, rev_id)
) WITHOUT ROWID;
```

Three tiers a rel can occupy:

| tier | storage | chosen by |
|---|---|---|
| **transient** (default) | in memory, for the life of the subscription | nothing — this is what a rel is |
| **saved** | a table that survives the subscription and the process | **syntax**, in the program |
| **cold** | nothing exists | no subscription |

**The tier is syntax, not policy.** A rel is a computation; the programmer
says when a computation is worth keeping. No heuristic infers it, no
`last_read_at` threshold decides it behind the author's back.

```dl
rel port_reach(from: text, to: text) save.     % keyword form
rel port_reach(from: text, to: text) @save.    % annotation form
```

Consistent with the existing modifier slot (`key(...)`, `merge(...)`,
`@in(...)`, `@out(...)`). Which spelling ships is a human review — it is a
`sprefa-lang` change and lands with a spec update, per rule 2.

Prior art for making this the author's word rather than the system's guess:
LogicBlox `lang:derivationType` (Extensional / DerivedAndStored / Derived),
Logica `@Ground`, Soufflé `.inline`. All three put it in the program.

`rel_state.last_read_at` and `bytes` stay in the table, but as **reporting**
for `dl daemon health` ("this saved rel costs 61MB and was last read 9 days
ago") — an argument handed to the author, never an eviction the system
performs.

Cold-start consequence, stated plainly: a transient rel has no prior state
after a restart, so its first tick recomputes its cone from scratch. That is
affordable precisely because the cone is bounded by the subscription — you
never recompute the corpus, only the subscribed cut. `save` is the escape
hatch for a cone where that recompute is too slow, and the author is the one
who knows.

### D10 — multiplicity is a weight column; retraction is subtraction

A row is not present or absent, it is present **with a weight**. Insert is
`+1`, retract is `-1`, a row derived two ways carries `2`, and a row whose
weight reaches `0` is gone. That is the Z-set, and it is refcounting: the
same discipline as an `Rc`, applied to tuples instead of allocations.

```sql
-- every rel row, transient or saved
  weight INTEGER NOT NULL DEFAULT 1
```

What this buys, in order of importance:

1. **Retraction stops being a separate code path.** A delta is a set of
   `(row, ±weight)` pairs. Applying it is one upsert that adds weights and
   deletes at zero. There is no "retract" verb distinct from "insert".
2. **Recursion is handled without DRed.** A tuple derived by three rules
   carries weight 3; killing one derivation leaves weight 2 and the tuple
   correctly survives. DRed exists to answer that question by
   over-deleting then re-deriving; the weight answers it by arithmetic.
3. **Joins compose.** The weight of a joined row is the product of its input
   weights, which is why the incremental chain rule works at all. Feldera is
   this, and nothing more exotic than this.

The one sharp edge, named so it does not surprise us: a **cyclic**
derivation (a tuple that participates in deriving itself) makes naive weights
diverge. The fix is the standard one and it is structural, not extra
machinery: run the recursive group's fixpoint as a nested loop that reaches a
least fixed point before its deltas are published outward, so weights inside
a cycle are settled before anyone sees them. This is why the SCC is the unit
in `trace_dep` below.

Explicitly not adopted: salsa (resident memo graph, single process) and
differential-dataflow (resident indexed arrangements — the "you have enough
RAM" assumption that fails at 500 repos). The weights idea is borrowed; the
resident storage model is not.

**Source rows: retract by extraction scope.** Exact, no algorithm — a
whole-scope delete is a bulk `-1` that happens to zero everything at once.
```sql
DELETE FROM node_<family> WHERE rev_id = ? AND file_id = ?;
DELETE FROM edge_<family> WHERE src_id IN (SELECT node_id FROM node_<family>
                                           WHERE rev_id = ? AND file_id = ?);
```
The scope is exactly what the extractor is about to re-emit. One file changed,
one scope deleted, one scope inserted. This is already how V5's
`refresh_rel_for_paths` works and it is correct.

**Derived rows: weights carry the delta; `trace_dep` decides who runs at
all.** The two mechanisms answer different questions and neither replaces
the other:

| question | mechanism | where it lives |
|---|---|---|
| *what changed inside a rel* | weights, `±1`, products across joins | the `weight` column |
| *which rels need to run* | the persisted trace | `trace_dep`, on disk |

Without the trace, a change means asking all 255 rels to compute their
deltas — cheap per rel, but 255 wake-ups where 3 were needed, and it requires
every rel's prior state to be resident. `trace_dep` is what keeps the
dependency graph **non-resident**: 602 rows a query reads on demand.

```sql
CREATE TABLE trace_dep (
  rel_id     INTEGER NOT NULL,             -- dense; SCC id for a recursive group
  dep_rel_id INTEGER NOT NULL,
  dep_digest BLOB    NOT NULL,             -- 8 bytes, truncated blake3
  PRIMARY KEY (rel_id, dep_rel_id)
) WITHOUT ROWID;
CREATE INDEX trace_dep_by_dep ON trace_dep(dep_rel_id);
```

```
on commit of a delta to rel R:
    wake := SELECT DISTINCT rel_id FROM trace_dep WHERE dep_rel_id = R
    for each W in wake:
        if W has no subscriber:      mark stale; stop        -- lazy
        if W is transient:           apply the delta in memory
        if W is saved:               apply the weight delta to its table
        if W is an SCC member:       run the group's nested fixpoint first,
                                     publish its settled deltas outward once
```

Eager invalidation, lazy recomputation — XSB's incremental-tabling shape, and
the same split Feldera draws between *maintained incrementally* (always: the
trace and the weights update regardless) and *contents retained* (opt-in:
only `save` puts rows on disk).

Measured selectivity on the current program set: **median 3 of 255 derived
rels wake per source change**, 602 dependency edges total. Waking 3 rels
instead of 255 is the whole reason the trace is worth persisting.

### D11 — every read is a subscription; `?` is subscribe + take(1) + complete

There is one path to a value, and it goes through the demand registry. A
one-shot is not a special case; it is a subscription with a synchronous
lifecycle, exactly RxJS's cold-observable contract.

```rust
pub struct Sub { rel: RelId, at: RevId, /* refcount handle */ }

impl Demand {
    /// Standing: LSP open doc, @serve route, watcher. Lives until dropped.
    pub fn subscribe(&self, rel: RelId, at: RevId) -> Sub;

    /// One-shot: `? rel(..)`, an MCP tool call, a CLI query.
    /// subscribe -> materialize if cold -> emit once -> complete -> drop.
    pub fn take_one(&self, rel: RelId, at: RevId) -> Result<Rows>;
}
```

```
take_one(R, rev):
    sub = subscribe(R, rev)            // refcount 0 -> 1, activates cone(R)
    if rel_state(R, rev) is cold:      CREATE TABLE, compute cone, record trace_dep
    if rel_state(R, rev) is stale:     recompute the invalid part of cone, re-record
    rows = read R
    drop sub                           // refcount 1 -> 0
    if refcount == 0 and tier == view: DROP TABLE      // never existed as bytes
    if refcount == 0 and tier == materialized: keep rows, mark last_read_at
    return rows                        // "complete"
```

`Drop for Sub` is the unsubscribe — no explicit teardown call to forget, and
a panicking handler still releases its demand. A rel that goes to refcount 0
stops ticking on the next cycle; whether its **rows** survive is the tier
decision in D9, not the subscription's business.

## Read/write sequence

**Write (one tick, one file changed):**
```
1. blob:      hash content -> SELECT blob_id; INSERT if absent
2. rev_file:  UPSERT (rev_id, file_id) -> blob_id
3. if blob_id unchanged for this file: STOP (nothing downstream re-derives)
4. extract:   sprefa-extract turns (blob bytes, lang) into nodes+edges,
              coordinates only, zero DB awareness
5. str:       batch-intern the vocabulary names ONLY (one insert_rows)
6. node_*:    DELETE WHERE rev_id=? AND file_id=?; batch INSERT
7. edge_*:    same scope, batch INSERT
```
Step 3 is the change that never fires today — the `_strings` re-intern
autopsy (1,207,064 rows offered / 146 accepted) was step 5 running with no
step 3 in front of it.

**Read (a query needing text):**
```
node_* row -> (blob_id, byte_start, byte_len) -> blob content -> slice
line/col   -> line_index(blob_id) binary search, built lazily, LRU-cached
```

## Uniqueness conditions

| table | uniqueness | why |
|---|---|---|
| `repo` | `slug` | one row per configured repo |
| `rev` | `(repo_id, git_sha)` | same sha in two repos is two revs |
| `file` | `(repo_id, path_id)` | path identity is per-repo |
| `blob` | `content_hash` | identical bytes at N revs = ONE row |
| `rev_file` | `(rev_id, file_id)` | one content per file per rev |
| `str` | `content` | the dictionary invariant |
| `node_*` | `(rev_id, file_id, byte_start, kind)` | a node IS its coordinate |
| `edge_*` | `(src_id, dst_id, kind)` | parallel edges differ only by kind |

## Instance lifetimes

| type | lifetime | owner |
|---|---|---|
| `Store` | process | the writer thread |
| `StoreHandle` | process, cloned per handler | `sprefa-server` |
| `str` id cache (`HashMap<String, u32>`) | process, bounded LRU | `Store` |
| `line_index(blob_id)` | per query burst, LRU-capped | `Store` reader |
| `Csr` snapshot per `(family, rev_id)` | until tick commit invalidates | `GraphCache` |
| extractor state | one file | `sprefa-extract`, no state across files |

## Verification

- **No all-columns PK:** a schema-introspection rail asserting every table's
  PK is either a single surrogate or a `WITHOUT ROWID` composite. Today: 473
  violations.
- **Dense ids:** `SELECT MAX(str_id) FROM str` within 1.05x of `COUNT(*)`.
  Today the ratio is ~1.96e13.
- **Dictionary purity:** zero `str.content` rows matching the coordinate,
  rev-salted, or qualified-symbol patterns. Today: 92% of rows.
- **No rev in a string:** zero occurrences of `\u{1}` in `str.content`;
  `salt_rev` deleted from the tree.
- **Blob dedup:** a file byte-identical at N revs yields exactly one `blob`
  row and N `rev_file` rows.
- **Skip-on-unchanged:** re-ticking an unchanged corpus performs zero
  `node_*`/`edge_*`/`str` writes — asserted from `events.jsonl` `db_write`
  events, which count batches.
- **Size:** the importer runs against a copy of the live 853MB root db; the
  V6 layout is reported as a ratio. Target from the arithmetic above is
  4–6x smaller before VACUUM.
- **Id budget:** a test asserting every surrogate max stays under `2^32`.
- **Nothing exists until subscribed:** loading a program with 255 derived
  rels and subscribing to none creates zero rel tables — asserted by counting
  `sqlite_master` rows before and after.
- **Wake selectivity:** a source change wakes only the rels reachable through
  `trace_dep`; asserted against the measured median of 3 of 255.
- **take_one is a full lifecycle:** a one-shot on a cold corpus materializes,
  emits, completes, and leaves a view-tier rel at zero bytes.
- **Orphan-ref rail:** an on-demand query per `_id` column finding rows whose
  ref has no parent — the check that replaces declared foreign keys.

## Staffing

One agent (opus-class), worktree under `.worktrees/`, base SHA `8d7b6092`
(branch `next`). Lands as the schema half of the `sprefa-store` arc, before
`StoreHandle`. Suite budget: the eight rails above + the importer run +
`scripts/verify.sh`.

<!-- todo(decision): crate-map amendment — src/graph/ is 11,934 lines of per-language extraction and 445 lines of graph algorithms; split into sprefa-extract (pure, source->nodes+edges) and sprefa-graph (algorithms + CSR), replacing the single 12,500 ceiling -->
<!-- todo(decision): line/col fully derived from byte offsets (current call) vs a stored line column on hot node tables; measure the line_index rebuild cost on a cold LRU before committing -->
<!-- todo(decision): does `str` stay one global dictionary or shard per repo? cross-repo identifier overlap at org scale is unmeasured — the sprefa/smashy 1.5% sample was two unrelated projects and does not answer it -->
<!-- todo(perf): node_* at 150M rows for 500 repos — measure whether one table per family per corpus holds, or whether rev_id becomes a partition key -->
<!-- todo(feature): sea-query coverage for `WITHOUT ROWID` (goes through .extra()) and for the generated node_*/edge_* DDL templates -->
<!-- todo(decision): 15 seam members with exactly one external caller — inline list to be produced and reviewed before the store arc, so they are deleted rather than extracted -->
<!-- todo(decision): `scan` defaults to WORK of the enclosing repo — a .dl file lives IN a repo, so the common case should need no ceremony; other repo/rev become the explicit forms. V5 has four positional overloads (bare, "WORK", "HEAD~5", "*"), which is the confusion this removes. sprefa-lang change, human reviews before the lang arc -->
<!-- todo(decision): historical scans — keep the explicit rev form for reading past revs, and decide whether a scan over a rev RANGE is a language form or a join over v_rev_ancestor -->
<!-- todo(decision): dir.repo_id is a cached denormalization of ancestry (150k rows, derivable, rail-checkable) — keep as cache, or make repo membership a pure v_dir walk with no stored column? -->
<!-- todo(decision): rev.seq topological order requires computing topo on import; alternative is ordering by observed_at, which is free but wrong across rebases -->
<!-- todo(decision): `save` spelling — keyword in the modifier slot (`rel foo(..) save.`) vs annotation (`@save`); sprefa-lang change, human reviews before the lang arc -->
<!-- todo(decision): does `save` take an argument for scope (`save(rev)` = per-rev tables vs one table with a rev column), or is that always one table? -->
<!-- todo(perf): weight column cost — one extra INTEGER on every rel row against the deletion of every separate retraction path; measure on the df_node family -->
<!-- todo(bug): cyclic derivation weight divergence — the nested-fixpoint fix in D10 needs a red test (a self-supporting cycle whose weights must settle before publishing) before the engine arc lands recursion on weights -->
<!-- todo(perf): trace_dep at 500-repo scale — 602 edges today is per-program, not per-corpus; measure whether the trace stays rel-granular or needs (rel, rev) rows -->
<!-- todo(bug): a stale rel that nobody re-subscribes to keeps its rows forever under the current-call eviction policy; decide whether stale + unsubscribed is a drop trigger -->
