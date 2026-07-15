# Delta reactivity and fact ownership

## Context

The bundled Rust analysis change in commit `2b10fbd6` proved that parsing was
duplicated, but it also exposed the coarsest remaining invalidation boundary.
A one-file dataflow edit can parse one file and still perform corpus-wide work.
`refresh_dataflow_rels` loads the whole extraction file set and returns cached
facts for every file (`src/engine/extract/dataflow.rs:11-35`), constructs every
public and rev-twin row vector (`dataflow.rs:37-175`), flushes all encountered
opaque IDs (`dataflow.rs:177-181`), and replaces fifteen relations
(`dataflow.rs:183-204`). `Engine::refresh_rel` encodes every row and calls the
whole-table reload path (`src/engine/mod.rs:1118-1125`).

Existing logs show the resulting scale. A 379-file run spent 1,266 ms in
reconciliation and 5,706 ms in `dataflow-rels`; a 650-file singleton run spent
1,137 ms in reconciliation and 7,511 ms in `dataflow-rels`. Warm counterparts
still spent 759-830 ms in that family. The exact 6.95-second extraction A/B was
a debug integration binary rather than the installed release, but bundled and
separate parsing had the same incremental wall time. The architecture, not the
two eliminated parses, determines the scaling.

The current system is reactively scheduled, not delta-preserving end to end:

| Stage | Invalidation identity | Current work unit |
|---|---|---|
| watcher | path | path |
| reconcile | path + content identity | path |
| parse cache | `(repo, path, content_hash)` | file |
| extraction family | `(family, rev)` digest | family |
| dataflow projection | cached facts for all files | corpus |
| source-relation write | relation name | whole relation |
| derived scheduling | dependency-reachable relation | affected relation |
| derived evaluation | rule/stratum | often the full affected relation |

Reactivity is therefore determined by the coarsest boundary in the chain. A
path-scoped watcher does not make a family refresher path-scoped, and an
affected-relation graph does not make the rule inside that relation incremental.
Rayon, Tokio, queueing, mmap, string interning, and stratification affect
concurrency, scheduling, transport, representation, and order respectively;
none creates deltas automatically.

This arc makes the pipeline preserve inserts and retractions:

```text
PathDelta -> FileFactDelta -> SourceRelationDelta
          -> DerivedRelationDelta -> EffectDelta
```

## Performance constitution

The standard is not "faster than the current whole-table reload." The standard
is the discipline of Biome/Oxc-class batch analyzers: compact representations,
one ownership pass, arena lifetimes aligned with one file or bounded batch, and
no allocation or database work for unaffected input. Performance is a design
constraint reviewed with correctness, not a cleanup phase after features land.

The hot-path laws are:

1. **Pay once.** Inventory once per coordinate, read once per changed content,
   parse once per language, project every requested family from that parse,
   compute each identity once, and encode each changed row once.
2. **No negative work.** An unrelated file must cause zero parsing, projection,
   interning, encoding, SQL writes, index rebuilds, and derived-rule execution.
3. **Bound lifetimes.** Source bytes, parser arenas, visitors, dedup scratch, and
   staging buffers die with one file or a byte-capped batch. Only compact facts
   and indexes survive a generation.
4. **Compact by construction.** Use typed structs, enums, integer/fixed-width
   IDs, slices, and column-oriented batches. `Vec<Vec<Value>>`, repeated owned
   `String`, and generic row boxes are compatibility boundaries, not internal
   representations for a hot family.
5. **Stream cold work.** A cold scan may touch every file but may not retain
   every AST, every fact bundle, every encoded row, and SQLite's reload copy at
   once. Cold construction uses bounded batches and a single transaction/stage.
6. **Budget every cache.** Parser reuse, string dictionaries, SQLite pages,
   mmap windows, and fact indexes have explicit byte
   budgets, eviction/compaction policy, and owner. "The OS will reclaim it" is
   not a memory policy.
7. **Measure charged and mapped memory separately.** On macOS record peak RSS
   and peak footprint; also report SQLite page cache, mapped pages, Rust heap,
   arenas, staging, and dictionary bytes. A mapping is not free merely because
   it is file-backed.
8. **Fallbacks are observable debt.** Any partition or relation rebuild emits
   the reason, owners, rows, bytes, allocations, SQL statements, wall time, and
   peak working-set delta. Silent coarse fallback is a correctness bug in the
   performance model.

Initial one-process budgets, tightened rather than relaxed as representation
work lands:

| Workload | Steady/unchanged | One-file edit | Cold build |
|---|---:|---:|---:|
| current 107-file fixture | <= 64 MiB | <= 96 MiB | <= 160 MiB |
| 3,000-file polyglot fixture | <= 96 MiB | <= 128 MiB | <= 256 MiB |

For one-file edits the stronger invariant is `peak footprint <= steady + 32
MiB`; the table includes executable/shared/mapping noise for end-to-end RSS.
The current release measured 44 MiB warm, 495 MiB maximum RSS and roughly 240
MiB peak footprint for a one-shot existing-file edit, so it fails the edit
budget before latency is considered. The database was only about 81 MiB: the
remaining peak is overlapping extraction facts, row vectors, dedup sets,
encoding copies, symbol staging, SQLite reload/index work, and mapped/cache
pages. The delta design must remove the overlap, not merely tune the allocator.

Every performance-sensitive change carries an allocation/work ledger:

```text
files read / parsed / projected
source bytes and parser-arena peak
fact-bundle rows and bytes
scratch set capacity and bytes
rows encoded / inserted / retracted / untouched
new dictionary entries and dictionary bytes
SQLite statements, page-cache bytes, WAL bytes, index rebuilds
Rust heap peak, process footprint, process RSS
phase wall/user time
```

The primary regression rail varies unrelated corpus size while holding the
changed file and its emitted delta constant. Latency, allocations, staged bytes,
SQL work, and incremental footprint must be flat. Throughput-only benchmarks
cannot waive this scaling rail.

## Decisions

1. Make ownership explicit at the extraction boundary. The stable logical slot
   is `OwnerKey(repo_id, resolved_coordinate_id, normalized_repo_relative_path,
   family_id, extractor_schema_version)`. `ContentId` is separate: an edit
   changes content for the same owner, deletion finds the old owner without
   reading bytes, and rename is one removal plus one insertion. Movable refs
   resolve to an immutable OID when claimed.
2. Factor storage into canonical fact rows plus owner-to-fact edges. Payload
   columns are stored once even when byte-identical files, repositories, or
   revisions contribute the same public fact. Ownership edges carry only IDs.
3. Treat contributions as a precisely defined bag feeding today's public set
   semantics. Duplicate emission inside one owner collapses to one owner edge;
   different owners contribute independent edges. A public fact is inserted on
   global owner count `0 -> 1`, removed on `1 -> 0`, and untouched while the
   count stays positive. A relation that genuinely needs within-owner
   multiplicity declares an integer multiplicity explicitly. Dataflow's current
   first-seen extraction keys become explicit semantic unique keys; two owners
   emitting different payloads for one semantic key abort the generation rather
   than preserving traversal-order first-wins behavior.
4. Use family-specific typed tables and indexes. Do not introduce a generic
   EAV fact store, generic JSON row blobs, or a universal SQL interning layer.
5. Represent `OwnerId` and internal `FactId` as deterministic 16-byte BLAKE3
   prefixes computed in Rust and stored as checked SQLite BLOBs. Fact IDs
   include relation/family tags, schema versions, and canonical semantic-key
   columns. A conflict compares every canonical component and payload column
   and aborts on mismatch; tests can force collisions. These IDs are not user
   strings and never pass through `StringId`, `sym`, or `_strings`.
6. Intern only opaque handles that participate heavily in joins or must decode
   through `sym()`. Keep payload text, source bytes, variable names, kinds, and
   display strings as ordinary typed columns unless a measurement proves a
   specific column benefits from interning.
7. Intern in process and flush new dictionary entries once per batch. No SQL
   lookup per string, no `sprf_sym(text)` conversion loop on the hot delta path,
   and no dictionary rewrite or reference-count collection on every edit.
8. Compute blast radius in two stages: the relation dependency graph selects
   candidate consumers, then generated delta plans propagate only changed keys.
   Empty output deltas stop propagation immediately.
9. Compile a non-recursive rule with inputs `A, B, ...` into delta variants such
   as `DeltaA join B` and `A join DeltaB`, using old/new generation boundaries
   to avoid double counting. Recursive components use semi-naive iteration
   seeded by the generation's input delta.
10. Declare coarse fallbacks explicitly. Aggregates, opaque SQL, lattices, or
    rules without safe delta lowering may rebuild a keyed partition or whole
    relation initially, but the fallback emits its scope, rows, bytes, and time.
11. Keep SQLite as the canonical and durable relational state for this arc.
    Tokio coordinates generation-aware bounded streams, Rayon runs admitted
    file-local CPU jobs, and typed delta SQL applies ownership/source/derived
    changes. Differential Dataflow, timely, and a process-resident relational
    trace are explicit non-goals because their resident-memory model conflicts
    with the one-repo and 3,000-file budgets.
12. Standardize plumbing on bounded generation frames: compact identities or
    byte-leased batches flow through Tokio `Stream`/bounded channels; barriers
    delimit generations; SQLite stores pending intent and committed state.

Rejected alternatives: attach full payload columns to every owner (multiplies
storage); generic EAV/JSON facts (weak typing and poor indexes); intern every
string (recreates the prior dictionary and conversion nightmare); per-edit
dictionary garbage collection (write amplification and crash complexity);
delete facts by path-shaped ID conventions (incorrect across languages and
duplicate owners); full-table compare-and-swap (still corpus work); and assume
Tokio streams or any incremental framework supplies granularity without an
explicit ownership delta seam.

## Factoring model

Conceptually, each public source relation has two internal layers:

```text
canonical fact
  fact_id -> typed public columns

ownership edge
  (owner_id, fact_id)
```

Versioned `_fact_df_*` tables become the canonical fact layer after shadow
validation; compatibility views retain the public `rel_df_*` surface. Compact
`_own_df_*(owner_id, fact_id)` tables record contribution, indexed both by
`(owner_id, fact_id)` and `(fact_id, owner_id)`. A file edit runs inside one
transaction:

1. Extract the changed owner into a bounded `FactBundle` and compute fact IDs.
2. Stream/chunk the bundle into generation staging tables; a single generated
   file may not force a million-ID Rust set.
3. Compute removed and added edges with flat, indexed `LEFT JOIN ... IS NULL`
   anti-joins driven by the bounded changed-owner/staging tables.
4. Delete removed ownership edges and insert added ownership edges in batches.
5. Delete canonical facts from `removed` only when no owner edge remains.
6. Insert canonical facts from `added` only when they do not already exist.
7. Expose those canonical `-row/+row` transitions as the source relation delta.
8. Commit ownership, canonical rows, extraction digest, and generation watermark
   atomically.

This factors duplicate contribution from fact payload. Ten repositories that
contribute the same edge store one typed edge and ten small ownership edges,
not ten copies of every string column. It also makes deletion honest: removing
one repository's file cannot retract a fact still owned elsewhere.

The first implementation uses one typed canonical table and one compact owner
edge table per relation. A family-local contribution table carrying repeated
payload is rejected: the bounded SQLite experiment below found 13-18x more VM
work to recover canonical rows and worse storage once sharing became moderate.
A later physical consolidation may combine edge tables only if it still stores
`(owner_id, relation_id, fact_id)` without payload and wins a representative
page/write benchmark; that would not change the ownership semantics.

### Bounded representation experiment

The final design dive compared two isolated SQLite layouts with exactly 10,000
owner edges per case:

- **A:** one canonical payload row plus compact owner edges;
- **B:** one owner-local contribution row repeating the payload.

The fixture varied fact sharing while holding edge count fixed. SQLite page
size was 4 KiB; VM steps and SQLite allocator/page-cache peak came from
`.stats`. No `dl`, daemon, repository scan, or production database was used.

| Shared facts | A bytes | B bytes | A canonical-read VM | B canonical-read VM | A peak | B peak |
|---:|---:|---:|---:|---:|---:|---:|
| 0% | 1,511,424 | 1,335,296 | 30,010 | 400,028 | 1,235,432 | 1,609,632 |
| 10% | 1,421,312 | 1,335,296 | 27,283 | 379,121 | 1,138,872 | 1,609,632 |
| 50% | 1,175,552 | 1,335,296 | 20,011 | 323,369 | 875,736 | 1,609,632 |
| 90% | 1,040,384 | 1,335,296 | 15,799 | 291,077 | 731,040 | 1,609,632 |

Repeated-payload contributions save some pages only when sharing is low. They
cross over between 10% and 50% sharing, are 28% larger at the high-sharing
point, and require a grouped canonical projection that costs 13-18x more VM
steps and 1.3-2.2x more SQLite peak memory. The result freezes layout A. The
reproducer is `/tmp/sprefa-owner-ratios.sh`; its disposable results and DBs are
under `/tmp/sprefa-owner-ratios/`.

This experiment does not claim end-to-end RSS. The sandbox denied the macOS
`sysctl` used by `/usr/bin/time`; SQLite's own bounded allocator/pager counters
are the evidence for this layout decision. End-to-end footprint remains a
release-gate measurement after the hot path exists.

### Canonical identity encoding

`OwnerId` and `FactId` are 16-byte prefixes of BLAKE3 over a versioned,
unambiguous preimage. The 128-bit width is deliberate: it halves index keys
versus 32-byte IDs, while the mandatory full-value collision check preserves
correctness even under the test hasher. SQLite columns use
`BLOB NOT NULL CHECK(length(id) = 16)`.

Every preimage begins with:

```text
ASCII "SPRFID"          6 bytes
encoding_version        u16 big-endian = 1
domain                  u8: 0x01 OwnerId, 0x02 FactId
reserved                u8 = 0
```

The owner body is:

```text
repo_id                 16 raw bytes
coordinate_kind         u8: 0 WORK, 1 Git SHA-1, 2 Git SHA-256
coordinate_payload      none, 20 raw bytes, or 32 raw bytes
normalized_path         PATH value encoding
family_id               u32 big-endian
extractor_schema        u32 big-endian
```

`ContentId`, generation, queue sequence, and wanted-family bits are excluded.
Each registered repository receives a persistent random 16-byte `repo_id` in
the repository registry. It is stable across path/URL edits and shadow schema
migrations within that database, unique for two roots with the same basename,
and explicitly internal: wiping the database may mint new owner IDs without
changing any public fact. Do not reuse `spine::RepoId(u32)` or a basename slug.

The fact body is:

```text
family_id               u32 big-endian
extractor_schema        u32 big-endian
relation_id             u32 big-endian
relation_schema         u32 big-endian
identity_column_count   u16 big-endian
identity columns        canonical logical values in semantic-key order
```

Canonical logical values are:

```text
NULL   0x00
false  0x01
true   0x02
INT    0x03 + signed i64 two's-complement big-endian
TEXT   0x04 + u32 big-endian byte length + exact UTF-8
BLOB   0x05 + u32 big-endian byte length + exact bytes
PATH   0x06 + u32 big-endian byte length + normalized UTF-8
```

V1 rejects NULL in identity columns. TEXT has SQLite binary-text semantics: no
Unicode normalization, case folding, trimming, or newline conversion. PATH is
strict UTF-8, repository-relative, `/`-separated, lexically collapses empty and
`.` segments, resolves `..`, and rejects root escape, absolute paths, and NUL.
It preserves case and Unicode codepoints and does not filesystem-canonicalize.
Non-UTF-8 source paths fail with an explicit unsupported-path diagnostic in V1
instead of passing through the current lossy conversion.

Interned handles are hashed from their decoded logical text, never from the
current 64-bit SQLite `StringId` cell. Current `row_hash` is not reusable: its
delimiter representation aliases NULL/empty text, integer/text spellings, and
embedded delimiters. The new encoder is a typed API, not string concatenation.

The dataflow semantic keys preserve the extractor's actual deduplication
contract and make it deterministic:

| Relation | Semantic key |
|---|---|
| `df_node`, `df_node_rev` | `id` |
| `df_node_repo` | `(id, repo)` |
| `df_node_repo_rev` | `(id, repo, rev)` |
| `df_edge` | `(from, to)` |
| `loop_over` | `(file, start)` |
| `allocates` | `fn` |
| `nest` | `(call_id, loop_id)` |
| `df_param` | `id` |
| `df_arg` | `(call, pos, arg)` |
| `df_arg_rev` | `(call, pos, arg, rev)` |
| `df_field` | `(id, field, value)` |
| `df_field_rev` | `(id, field, value, rev)` |
| `df_lit` | `id` |
| `df_lit_rev` | `(id, rev)` |

Equal ID and equal complete typed payload is idempotent. Equal ID with unequal
key or payload, or equal semantic key with unequal payload, aborts the entire
generation. A test-only `IdHasher` that always returns one 16-byte value proves
owner collision, fact collision, same-preimage idempotency, relation/schema
domain separation, and rollback of every edge and canonical change.

## Blast-radius factoring

The blast radius is not simply "every relation reachable from `df_node`." It is
the composition of three increasingly precise scopes:

1. **Owner scope:** which source facts actually entered or left when one
   `(repo, rev, file)` changed.
2. **Relation scope:** which rules can consume those relation names, taken from
   the existing dependency graph.
3. **Key scope:** which join-key partitions those rules can affect, propagated
   by delta rule variants.

For a rule `out(x, z) <- left(x, y), right(y, z)`, a change to `left` evaluates
only `DeltaLeft join Right` for the changed `y` keys. If that produces no
`DeltaOut`, consumers of `out` do not run. A simultaneous delta on both inputs
uses generation-aware old/new arrangements so the overlap is counted once.

Each lowered rule records a small impact descriptor:

```text
reads: relation + input columns
writes: head relation + key columns
partition: repo/rev/file when preserved
delta_capability: exact | partition_fallback | relation_fallback
```

This metadata is proportional to rules and dependency edges, never files times
strata. Strata continue to provide ordering; they do not own file work queues.
One changed owner produces one bounded fact delta that flows through the graph.

## String and memory discipline

The earlier SQL interning failure mode came from mixing three different things:
identity, join representation, and display content. This design separates them:

- `RepoId`, `RevId`, `FileId`, `OwnerId`, `FactId`, and opaque node handles are
  identities. Store compact typed IDs.
- Relation payload such as kind, variable, function text, field name, and
  literal text remains typed content. Do not automatically intern it.
- Raw file content never enters a persistent relational trace. SQLite/content
  storage is canonical and a byte-bounded cache may hold hot files.
- `_strings` remains an append-only decode dictionary for the limited opaque
  handles that require `sym()`. A delta flush inserts only previously unseen
  dictionary rows with one batched `INSERT OR IGNORE`.
- Dictionary reachability cleanup is an offline/thresholded compaction concern,
  not part of the keystroke transaction.

Required counters are new unique strings, dictionary references, dictionary
pages/bytes, owner-edge pages/bytes, canonical-fact pages/bytes, staged bundle
bytes, and process RSS. A change that reduces CPU but makes `_strings` or owner
tables grow per generation fails.

There is no process-resident full relational trace in this plan. SQLite holds
canonical facts, ownership, derived state, and cold content references. Tokio
channels hold bounded in-flight identities/batches only, and Rayon jobs retain
source/parser state only until their compact bundle is staged. This keeps idle
memory proportional to explicitly budgeted caches rather than corpus history.

## Executable SQLite contract

The schema below is the V1 contract, not illustrative pseudocode. Names are
versioned so shadow migration can coexist with epoch-10 tables. Owner/fact IDs
are 16 bytes; content/program/root-coordinate digests remain full 32-byte
BLAKE3 values. Generation IDs are random 16-byte UUID values. All connections
enable foreign keys. The delta path uses file-backed temp storage and an
explicit process-wide page-cache/mmap budget; it must remove the current
duplicated 512 MiB cache setup and `temp_store=MEMORY` default in `src/db.rs`.

```sql
PRAGMA foreign_keys = ON;
PRAGMA temp_store = FILE;

CREATE TABLE _repo_identity_v1 (
  repo_id BLOB PRIMARY KEY CHECK(length(repo_id)=16),
  slug TEXT NOT NULL,
  root TEXT NOT NULL,
  url TEXT NOT NULL DEFAULT '',
  UNIQUE(root)
) WITHOUT ROWID;

CREATE TABLE _family_schema_v1 (
  family TEXT PRIMARY KEY,
  schema_epoch INTEGER NOT NULL,
  extractor_schema INTEGER NOT NULL,
  state TEXT NOT NULL CHECK(state IN
    ('shadow','validating','active','retired','failed')),
  writer_min_version INTEGER NOT NULL,
  active_suffix TEXT NOT NULL,
  migration_id BLOB CHECK(migration_id IS NULL OR length(migration_id)=16),
  activated_generation INTEGER,
  updated_at_ms INTEGER NOT NULL,
  error TEXT
) WITHOUT ROWID;

CREATE TABLE _family_migration_v1 (
  migration_id BLOB PRIMARY KEY CHECK(length(migration_id)=16),
  family TEXT NOT NULL REFERENCES _family_schema_v1(family),
  from_epoch INTEGER NOT NULL,
  to_epoch INTEGER NOT NULL,
  state TEXT NOT NULL CHECK(state IN
    ('building','validating','ready','active','cleanup','failed')),
  resume_repo_id BLOB,
  resume_coordinate_id BLOB,
  resume_path TEXT,
  owners_done INTEGER NOT NULL DEFAULT 0,
  facts_done INTEGER NOT NULL DEFAULT 0,
  bytes_done INTEGER NOT NULL DEFAULT 0,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  error TEXT
) WITHOUT ROWID;
CREATE INDEX _family_migration_state_v1
  ON _family_migration_v1(state, updated_at_ms);

CREATE TABLE _root_fence_v1 (
  singleton INTEGER PRIMARY KEY CHECK(singleton=1),
  fence_token INTEGER NOT NULL,
  generation_id BLOB NOT NULL CHECK(length(generation_id)=16),
  claim_through_seq INTEGER NOT NULL,
  installed_at_ms INTEGER NOT NULL
);

CREATE TABLE _root_generation_v1 (
  singleton INTEGER PRIMARY KEY CHECK(singleton=1),
  committed_generation INTEGER NOT NULL,
  generation_id BLOB NOT NULL CHECK(length(generation_id)=16),
  scheduler_claim_seq INTEGER NOT NULL,
  fence_token INTEGER NOT NULL,
  program_digest BLOB NOT NULL CHECK(length(program_digest)=32),
  committed_at_ms INTEGER NOT NULL
);

CREATE TABLE _df_owner_v1 (
  owner_id BLOB PRIMARY KEY CHECK(length(owner_id)=16),
  repo_id BLOB NOT NULL CHECK(length(repo_id)=16),
  coordinate_id BLOB NOT NULL CHECK(length(coordinate_id)=32),
  normalized_path TEXT NOT NULL CHECK(instr(normalized_path,char(0))=0),
  extractor_schema INTEGER NOT NULL,
  content_id BLOB NOT NULL CHECK(length(content_id)=32),
  committed_generation INTEGER NOT NULL,
  UNIQUE(repo_id,coordinate_id,normalized_path,extractor_schema),
  FOREIGN KEY(repo_id) REFERENCES _repo_identity_v1(repo_id)
) WITHOUT ROWID;
CREATE INDEX _df_owner_generation_v1
  ON _df_owner_v1(committed_generation);
```

Each relation generates one typed canonical table and one edge table. This is
the concrete `df_node` instance; the other fourteen use the same ID/count and
edge columns with their declared typed payload and semantic UNIQUE key.

```sql
CREATE TABLE _fact_df_node_v1 (
  fact_id BLOB PRIMARY KEY CHECK(length(fact_id)=16),
  owner_count INTEGER NOT NULL CHECK(owner_count>=0),
  id INTEGER NOT NULL,
  kind INTEGER NOT NULL,
  var INTEGER NOT NULL,
  fn INTEGER NOT NULL,
  file INTEGER NOT NULL,
  line INTEGER NOT NULL,
  UNIQUE(id)
) WITHOUT ROWID;

CREATE TABLE _own_df_node_v1 (
  owner_id BLOB NOT NULL CHECK(length(owner_id)=16)
    REFERENCES _df_owner_v1(owner_id),
  fact_id BLOB NOT NULL CHECK(length(fact_id)=16)
    REFERENCES _fact_df_node_v1(fact_id),
  PRIMARY KEY(owner_id,fact_id)
) WITHOUT ROWID;
CREATE INDEX _own_df_node_by_fact_v1
  ON _own_df_node_v1(fact_id,owner_id);
```

The semantic-key table in the identity section generates each canonical
`UNIQUE` constraint. Payload storage follows declared SQLite representation:
interned textish columns are `INTEGER`, raw text such as `df_lit.text` is
`TEXT`, and numeric columns are `INTEGER`. A generated collision trigger on
both canonical and staging tables aborts when the same FactId or semantic key
has a different complete typed payload. `INSERT OR IGNORE` is allowed only
after that trigger has proved the ignored row identical.

Staging is durable and owner-local. Completed bundles survive restart;
incomplete bundles are discarded and re-extracted. Typed staging payload makes
collision checks and canonical insertion SQL-only and avoids a large Rust map.

```sql
CREATE TABLE _df_stage_owner_v1 (
  generation_id BLOB NOT NULL CHECK(length(generation_id)=16),
  owner_id BLOB NOT NULL CHECK(length(owner_id)=16),
  operation INTEGER NOT NULL CHECK(operation IN (1,2)),
  repo_id BLOB NOT NULL CHECK(length(repo_id)=16),
  coordinate_id BLOB NOT NULL CHECK(length(coordinate_id)=32),
  normalized_path TEXT NOT NULL,
  extractor_schema INTEGER NOT NULL,
  content_id BLOB CHECK(content_id IS NULL OR length(content_id)=32),
  program_digest BLOB NOT NULL CHECK(length(program_digest)=32),
  fence_token INTEGER NOT NULL,
  staged_complete INTEGER NOT NULL DEFAULT 0
    CHECK(staged_complete IN (0,1)),
  PRIMARY KEY(generation_id,owner_id)
) WITHOUT ROWID;
CREATE INDEX _df_stage_owner_ready_v1
  ON _df_stage_owner_v1(generation_id,staged_complete,owner_id);

CREATE TABLE _stage_df_node_v1 (
  generation_id BLOB NOT NULL CHECK(length(generation_id)=16),
  owner_id BLOB NOT NULL CHECK(length(owner_id)=16),
  fact_id BLOB NOT NULL CHECK(length(fact_id)=16),
  id INTEGER NOT NULL,
  kind INTEGER NOT NULL,
  var INTEGER NOT NULL,
  fn INTEGER NOT NULL,
  file INTEGER NOT NULL,
  line INTEGER NOT NULL,
  PRIMARY KEY(generation_id,owner_id,fact_id)
) WITHOUT ROWID;
CREATE INDEX _stage_df_node_fact_v1
  ON _stage_df_node_v1(generation_id,fact_id,owner_id);
```

The root transaction processes one relation at a time and reuses bounded,
file-backed TEMP tables. They contain only changed-owner edges, consolidated
fact counts, and the typed rows needed for one flat bulk apply. Per-relation
reuse lowers peak memory and removes relation tags from every scratch row.

```sql
CREATE TEMP TABLE tx_changed_owner (
  owner_id BLOB PRIMARY KEY CHECK(length(owner_id)=16),
  operation INTEGER NOT NULL CHECK(operation IN (1,2))
) WITHOUT ROWID;

CREATE TEMP TABLE tx_edge_change (
  owner_id BLOB NOT NULL,
  fact_id BLOB NOT NULL,
  diff INTEGER NOT NULL CHECK(diff IN (-1,1)),
  PRIMARY KEY(owner_id,fact_id)
) WITHOUT ROWID;
CREATE INDEX tx_edge_change_fact
  ON tx_edge_change(fact_id,owner_id,diff);

CREATE TEMP TABLE tx_fact_net (
  fact_id BLOB NOT NULL,
  net INTEGER NOT NULL,
  PRIMARY KEY(fact_id)
) WITHOUT ROWID;

-- Generated per relation; df_node is representative.
CREATE TEMP TABLE tx_fact_apply_df_node (
  fact_id BLOB PRIMARY KEY,
  old_count INTEGER NOT NULL CHECK(old_count>=0),
  new_count INTEGER NOT NULL CHECK(new_count>=0),
  id INTEGER NOT NULL,
  kind INTEGER NOT NULL,
  var INTEGER NOT NULL,
  fn INTEGER NOT NULL,
  file INTEGER NOT NULL,
  line INTEGER NOT NULL
) WITHOUT ROWID;
```

The generation transaction also writes effect intent atomically:

```sql
CREATE TABLE _effect_outbox_v1 (
  idem_key BLOB PRIMARY KEY CHECK(length(idem_key)=32),
  generation_id BLOB NOT NULL CHECK(length(generation_id)=16),
  effect_kind TEXT NOT NULL,
  delivery TEXT NOT NULL CHECK(delivery IN
    ('idempotent','at_least_once','at_most_once')),
  payload_json TEXT NOT NULL,
  state TEXT NOT NULL CHECK(state IN ('queued','running','done','poisoned')),
  lease_owner BLOB,
  lease_token INTEGER NOT NULL DEFAULT 0,
  lease_until_ms INTEGER,
  attempts INTEGER NOT NULL DEFAULT 0,
  next_attempt_ms INTEGER NOT NULL DEFAULT 0,
  last_error TEXT,
  completed_at_ms INTEGER
) WITHOUT ROWID;
CREATE INDEX _effect_outbox_ready_v1
  ON _effect_outbox_v1(state,next_attempt_ms,generation_id);
```

At cutover the legacy `rel_df_*` tables are renamed and owner-backed typed
tables become authoritative through compatibility views. The generic relation
declaration and index machinery must recognize owner-backed built-ins, create
physical indexes on `_fact_*`, and never try to migrate or index the views.
Affected `_txt` views are recreated in the same cutover transaction. This is
preferred to adding hidden internal columns to public tables, which would leak
through `SELECT *` and fight the current schema-drift logic.

The scheduler database uses the same durable schema defined in
`plans/2026-07-14-bounded-single-sweep-runtime.md`, concretized as
`root_queue_v1`, `path_intent_v1`, `generation_v1`, and immutable
`generation_path_v1`. Required keys/indexes are respectively root key;
`(root_key,coordinate_id,normalized_path)` plus `(root_key,seq)`;
generation ID plus unique `(root_key,fence_token)` and lease-state index; and
`(generation_id,coordinate_id,normalized_path)` plus `(generation_id,seq)`.
Queue payload contains IDs, sequence, wanted bits, program digest, and observed
content ID only--never source bytes, ASTs, or fact bundles.

### Exact one-generation transaction

Staging commits before the root transaction. One `BEGIN IMMEDIATE` then applies
every owner in the generation together; this is essential for rename, where an
old owner may retract the same fact the new owner inserts. Edge diffs are
aggregated before counts change, so a net-zero handoff emits no false public
retraction/insertion.

1. Require an exact `_root_fence_v1` match on generation ID, fencing token, and
   claim-through sequence. Require every staged owner complete with that token,
   the claimed program digest, and active extractor schema. A missing/mismatched
   row rolls back.
2. Clear the bounded TEMP tables. For each relation, populate removals and
   additions with flat indexed anti-joins. `tx_changed_owner` is bounded by the
   generation; persistent ownership is always a PK probe:

```sql
INSERT INTO tx_edge_change(owner_id,fact_id,diff)
SELECT e.owner_id,e.fact_id,-1
FROM tx_changed_owner AS c
CROSS JOIN _own_df_node_v1 AS e ON e.owner_id=c.owner_id
LEFT JOIN _stage_df_node_v1 AS s
  ON s.generation_id=:gen
 AND s.owner_id=e.owner_id AND s.fact_id=e.fact_id
WHERE c.operation IN (1,2) AND s.fact_id IS NULL;

INSERT INTO tx_edge_change(owner_id,fact_id,diff)
SELECT s.owner_id,s.fact_id,1
FROM tx_changed_owner AS c
CROSS JOIN _stage_df_node_v1 AS s
  ON s.generation_id=:gen AND s.owner_id=c.owner_id
LEFT JOIN _own_df_node_v1 AS e
  ON e.owner_id=s.owner_id AND e.fact_id=s.fact_id
WHERE c.operation=1 AND e.fact_id IS NULL;
```

`CROSS JOIN` is intentional SQLite join-order control: it keeps the bounded
`tx_changed_owner` table outermost. A normal inner `JOIN` was measured by the
executable plan fixture to reorder into `SCAN _own_df_node_v1`; the query-plan
rail caught it before production wiring.

3. Run generated FactId, semantic-key, and within-staging conflict checks. Insert
   new canonical rows at `owner_count=0`; canonical insertion precedes edges.
4. Aggregate once into PK-ordered `tx_fact_net`, then join it to canonical
   payload to build typed `tx_fact_apply_*`. The CHECK rejects a negative new
   count. No scalar or aggregate subquery runs per fact:

```sql
INSERT INTO tx_fact_net(fact_id,net)
SELECT fact_id,SUM(diff)
FROM tx_edge_change INDEXED BY tx_edge_change_fact
GROUP BY fact_id;

INSERT INTO tx_fact_apply_df_node
  (fact_id,old_count,new_count,id,kind,var,fn,file,line)
SELECT f.fact_id,f.owner_count,f.owner_count+n.net,
       f.id,f.kind,f.var,f.fn,f.file,f.line
FROM tx_fact_net AS n
JOIN _fact_df_node_v1 AS f ON f.fact_id=n.fact_id;
```

5. Copy zero-boundary typed rows from `tx_fact_apply_*` into generated source
   delta tables. Stream removed `(owner_id,fact_id)` keys from TEMP in
   parameter-budgeted chunks and issue PK deletes shaped as `DELETE ... WHERE
   owner_id=? AND fact_id IN (?,?,...)`. Insert additions with flat `INSERT ...
   SELECT`; unexpected missing rows or conflicts abort rather than hide behind
   `OR IGNORE`.
6. Apply counts with one flat, stage-driven bulk UPSERT:

```sql
INSERT INTO _fact_df_node_v1
  (fact_id,owner_count,id,kind,var,fn,file,line)
SELECT fact_id,new_count,id,kind,var,fn,file,line
FROM tx_fact_apply_df_node
WHERE true
ON CONFLICT(fact_id) DO UPDATE SET
  owner_count=excluded.owner_count;
```

   `WHERE true` disambiguates SQLite's `INSERT ... SELECT ... ON CONFLICT`
   parser. Collision checks already proved payload equality, so the conflict
   arm changes only the count.
7. Upsert owner state/content/generation for operation 1. For operation 2,
   remove owner state only after all fifteen edge tables contain no row for it.
   A delete stages no fact rows and never needs old source content.
8. Stream zero-count IDs from `tx_fact_apply_*` in bounded parameter chunks and
   delete them by canonical PK after capturing their negative payload. Build
   actual changed-fact counts with a flat bounded-input `LEFT JOIN` onto the
   reverse owner index and compare the two TEMP tables.
9. Propagate consolidated typed source deltas. Insert effect outbox rows. Update
   the root generation watermark with a conditional fence check and require
   exactly one changed row. Consume the generation's staging and commit.
10. Only after root commit, acknowledge scheduler intents through the claimed
   sequence. `DELETE ... WHERE seq <= :claim_seq` preserves a later upsert.

Rename stages the old owner as operation 2 and the new path owner as operation
1 in the same generation and freshly projects the new path, because dataflow
IDs and payload paths are path-dependent. All owner diffs are calculated before
any owner count changes.

Claiming in the scheduler is likewise one `BEGIN IMMEDIATE`: select a ready
root, snapshot `requested_seq` as `S`, increment its monotonic fence token to
`F`, insert generation `G`, and copy all `path_intent.seq <= S` into immutable
`generation_path`. Install `(G,F,S)` into the root fence only if `F` is newer.
Lease renewal matches generation, fence, lease owner, and nonterminal state.
If an old worker already owns the root write transaction, it completes or rolls
back; commits are deliberately non-preemptible.

Cross-database recovery reads the committed root watermark. If root commit
succeeded but scheduler acknowledgement did not, it advances `committed_seq`,
deletes only intents through `S`, and marks `G` committed idempotently. A later
path event survives. An incomplete staged owner is deleted/re-extracted; a
complete correctly fenced one may be resumed.

### Query-plan rails

Tests assert stable table/index names in `EXPLAIN QUERY PLAN`, not numeric plan
node IDs. Expected one-owner edit shapes are:

```text
removal: SCAN bounded tx_changed_owner;
         SEARCH edge USING PRIMARY KEY (owner_id=?);
         SEARCH staged fact USING PRIMARY KEY (...) LEFT-JOIN
addition: SCAN bounded tx_changed_owner;
          SEARCH staged fact USING PRIMARY KEY (...);
          SEARCH edge USING PRIMARY KEY (owner_id=?,fact_id=?) LEFT-JOIN
collision: SEARCH canonical USING PRIMARY KEY (fact_id=?)
semantic conflict: SEARCH canonical USING semantic UNIQUE index
net/apply: SCAN bounded tx_edge_change using fact-leading index;
           SCAN bounded tx_fact_apply;
           SEARCH canonical by fact_id
mutation: SEARCH owner edge/canonical USING PRIMARY KEY for parameter chunks
integrity: SCAN bounded tx_fact_apply;
           SEARCH edge using (fact_id,owner_id) LEFT-JOIN
claim: SEARCH path_intent using (root_key,seq)
outbox: SEARCH outbox using (state,next_attempt_ms,generation_id)
```

Forbidden on a one-owner edit: `SCAN _fact_*`, `SCAN _own_*`, unbounded
`SCAN path_intent`, `SUBQUERY`, `EXCEPT`, `USE TEMP B-TREE`, `DROP INDEX`, or
`CREATE INDEX`. Scans of current-generation staging/TEMP tables are allowed
because their encoded bytes are admitted by the generation budget.

The isolated query-shape pass explains the choice. With 120,000 unrelated
canonical rows and changed sets of 10/1,000/100,000, correlated `NOT EXISTS`
used 116/10,610/1,060,010 VM steps; flat `LEFT JOIN ... IS NULL` used
128/11,711/1,170,011; `EXCEPT` used 360,074/365,519/910,019, scanned globally,
and built a temp B-tree. The flat join pays roughly 10% more VM instructions
but remains delta-driven and had indistinguishable measured wall time, so V1
accepts that small local cost to standardize a no-subquery hot path. `EXCEPT`
is rejected.

For count application, the flat typed bulk UPSERT beat the indexed scalar
subquery despite about 12% more VM instructions: in-process wall at
10/1,000/100,000 rows was 0.325/0.608/19.43 ms versus
0.335/0.837/47.36 ms, with 2.96 MiB versus 3.43 MiB SQLite peak at 100,000.
A prepared per-row loop took 85.82 ms at 100,000 and is rejected as an N+1
fallback. The complete flat path with 500-key PK-delete chunks took
0.503/1.851/121.49 ms for 10/1,000/100,000 changed rows. Reproducers and plans
are `/tmp/sprefa-sqlite-delta-bench.sh` and
`/tmp/sprefa-sqlite-apply-bench.sh`; all artifacts are disposable `/tmp` data.

## Hardening invariants

### Generation claims, fencing, and staleness

Pending intent is mutable; claimed work is not. The scheduler copies every
claimed row through sequence `S` into immutable `generation_path` rows carrying
normalized path, observed content ID, wanted-family bits, and program digest.
Acknowledgement deletes only intent with `seq <= committed_seq`; an event with a
newer sequence survives. Full-scan promotion records `full_through_seq` and may
not consume later path events.

Every generation and worker lease has a fencing token. Before staging and again
before owner commit, WORK verifies `(normalized_path, content_id)` and the
program digest. A stale bundle is discarded, its byte lease is released, and
the newer intent remains. A branch resolves at claim time; movement afterward
is a later intent. Cancellation is cooperative before the root transaction;
once commit begins it completes or rolls back. Client disconnect drops only a
waiter, never durable work.

The scheduler and root databases cannot commit atomically together. The root
generation transaction therefore records the scheduler claim watermark. On
restart, recovery compares both sides and safely re-acknowledges a root commit
whose scheduler acknowledgement was lost. Lease expiry, retry backoff, poison
quarantine, and fencing prevent two workers from committing the same claim.

### Backpressure and fairness

Budgets are hierarchical: global, active root, stage, and item. Count limits and
byte limits both apply. A source-byte lease is acquired before reading; an
output-byte lease grows as facts are emitted and remains held until staging.
Oversized sources or fact expansions use a single-item spill/chunk lane or fail
with an explicit size diagnostic; they never bypass the budget.

Initial scheduler invariants are:

```text
active root transactions <= 1
staging roots <= 2
global staged bytes <= configured budget
hot admission-to-start p99 <= 250 ms when no commit is active
```

Cold roots yield between bounded inventory/extraction quanta. Priority plus
aging chooses the next root; an atomic commit is not preempted. Process-wide
watchers route physical-repo events to interested roots instead of multiplying
watchers by roots times configured repositories.

### SQLite durability and bounded storage

The first implementation must specify concrete typed DDL, fixed ID width and
byte order, null/text normalization, and equality identical to each public
relation's PK semantics. Canonical insertion precedes ownership insertion;
ownership deletion precedes orphan deletion. Foreign keys are enabled and
measured or equivalent integrity queries run before commit.

Owner diff, orphan deletion, and delta-join statements have `EXPLAIN QUERY
PLAN` rails proving no full scan for one-owner edits. The incremental path may
not drop/rebuild indexes. Process-wide SQLite cache, mmap, WAL, temp/staging,
and long-reader budgets are prerequisites, not later tuning; unbounded
`temp_store=MEMORY` is forbidden. Cleanup of completed generations,
superseded intents, WAL, and staging debt runs in bounded quanta.

Fallback is an operator-requested owner-aware full rebuild. A delta failure
rolls back, preserves the previous generation and intent, and becomes visible;
it may not silently switch to legacy whole-table writes and desynchronize
ownership. Hot-path fixtures assert a test-visible coarse-fallback counter is
zero.

### Schema evolution

Ownership cannot be reconstructed from existing public rows alone. Migration
uses a versioned shadow schema:

1. Record family schema epoch and reject writes from an older binary.
2. Create versioned canonical, owner, owner-state, and staging tables.
3. Re-extract owners in resumable byte-bounded batches.
4. Validate public-row equivalence, owner integrity, collision checks, and disk
   headroom including old tables, new tables, indexes, and WAL.
5. Atomically switch the active family schema marker.
6. Retain old tables through one successful restart/replay, then remove them in
   an explicit cleanup phase.

Interrupted migration, disk full, old/new daemon overlap, and extractor schema
change must resume or roll back without a mixed authoritative state.

### Effects and exact delta semantics

Effects use a transactional outbox written with the root generation. Each has
idempotency key `(root, generation, effect_identity)`, claim/lease/completed
state, bounded retries, and poison quarantine. Sinks are idempotent by key or
documented as at-least-once; relation commit never depends on successful
external dispatch.

Signed delta algebra names old and new snapshots. For a simultaneous change to
`A` and `B`, one valid non-double-counting form is:

```text
DeltaOut = (DeltaA join B_new) + (A_old join DeltaB)
```

Equal positive/negative rows consolidate before propagation. Public set
relations threshold signed multiplicity at zero/positive. Partition fallback
is permitted only when the lowered impact descriptor proves the partition key
is preserved through every input and the head; otherwise the fallback scope is
the relation.

## Implementation sequence

### Slice A: observability and ownership contract

- Add off-by-default timings for dataflow cache access, changed-file extract,
  bundle-to-row projection, symbol flush, and every relation write.
- Report input owners, changed owners, fact rows/bytes, SQLite changes, WAL
  bytes, dictionary insertions, and affected relations.
- Implement the frozen 16-byte `OwnerId`/`FactId` encoder, persistent repository
  identity, strict path normalization, explicit dataflow semantic keys, and
  forced-collision test hasher.
- Add DDL/query-plan fixture tests before routing any production extraction
  through ownership. Fix the duplicated 512 MiB SQLite cache configuration and
  remove unbounded `temp_store=MEMORY` from the delta connection profile.
- Use only deterministic tiny fixtures during implementation; do not invoke the
  daemon, repository discovery, `dl --check`, or production-corpus scans.

### Slice B: dataflow canonical facts and ownership

- Add ownership schema for all legacy and rev-twin dataflow facts, including
  loops, allocators, nesting, parameters, arguments, fields, and literals.
- Build a versioned shadow schema and resumably backfill ownership in bounded
  batches without retaining a corpus-sized owner map.
- Implement the exact generation transaction for edit/delete/rename. During
  shadow validation the existing full refresh remains production-authoritative
  for A/B comparison; after cutover the only fallback is an owner-aware rebuild.

### Slice C: delta writes and equivalence

- Route `tick_paths` to the owner-delta path while full ticks retain streaming
  cold construction.
- Emit source `-row/+row` deltas only on zero-count transitions.
- Compare every incremental result with a clean full rebuild, including file
  deletion, rename, duplicate facts, two repos with identical paths, and WORK
  plus immutable-revision twins.

### Slice D: derived blast radius

- Add rule impact descriptors and exact delta variants for projection, filter,
  union, and equijoin.
- Seed existing semi-naive recursion from the cross-generation source delta.
- Add explicit partition/relation fallback metrics for aggregates, lattices,
  negation, and unsupported shapes; convert the hottest fallbacks one at a time.

### Slice E: durable generation scheduling

- Feed compact owner identities into the durable coalescing queue described in
  `plans/2026-07-14-bounded-single-sweep-runtime.md`.
- Preserve one global generation initially. Queue state contains identities and
  wanted-family bits, never content, syntax trees, or fact bundles.
- Snapshot claims into immutable generation rows, add leases/fencing and
  cross-database watermark recovery, and schedule cold roots in bounded quanta.
- Add the transactional effect outbox before routing external sinks through the
  generation coordinator.

## Rollout and operational visibility

Ownership becomes authoritative only after shadow validation. Rollout order is:

1. Counters, byte leases, and allocation/accounting rails.
2. Versioned shadow schema plus resumable bounded migration.
3. Dataflow canonical/owner dual-write behind a DB-persisted feature state.
4. Shadow-read equivalence while production reads remain legacy.
5. Crash, disk-full, restart, collision, and old-binary refusal tests.
6. Atomic read switch to owner-backed canonical tables.
7. Path-delta writes and owner-aware full-rebuild fallback.
8. Derived delta propagation and proven fallback scopes.
9. Durable scheduling, fairness quanta, and transactional effect outbox.
10. Retirement of legacy refresh only after memory/latency/database-growth soak.

Every generation reports bounded-cardinality metrics for queue wait, claims,
lease recovery, retry/poison state, stale bundles, owners changed/unchanged,
input/output leased bytes, staging/cleanup debt, canonical and ownership
transitions, delta rows before/after consolidation, fallback scope, transaction
time, WAL/checkpoint growth, and effect-outbox state. Paths appear only in
sampled debug records, never metric labels.

## Verification

Test execution is staged so implementation never needs a production corpus to
find a correctness or scaling failure:

1. **Pure identity tests:** golden encoding bytes, domain/schema separation,
   strict path normalization, interned logical-text hashing, forced OwnerId and
   FactId collisions, and same-preimage idempotency.
2. **Tiny SQLite transaction tests:** edit, delete, rename, duplicate emission,
   shared fact, final owner, semantic-key payload conflict, stale program,
   stale fence, rollback, and restart of complete/incomplete staging.
3. **Query-plan tests:** assert the named indexes above and reject canonical,
   ownership, and intent scans on a one-owner generation.
4. **Differential fixture tests:** every incremental result and emitted source
   delta equals a clean owner-aware rebuild under different Rayon schedules and
   chunk sizes.
5. **Scaling fixtures:** hold one changed owner's delta constant while unrelated
   files grow through 10, 100, 1,000, and 3,000; assert work, statements, page
   visits, staged bytes, and memory remain flat.
6. **Durability tests:** process kill at every transaction boundary, lease
   expiry/fencing race, scheduler/root acknowledgement loss, WAL reader,
   disk-full migration, old-writer refusal, outbox retry, and cleanup debt.
7. **Explicitly approved release gate:** only after the prior gates pass, run
   the installed release on an isolated representative corpus and measure wall,
   CPU, RSS, footprint, WAL, database bytes, and unchanged/edit/cold behavior.

Correctness gates:

- One-owner incremental output equals a clean full rebuild byte for byte.
- Deleting one of two owners of the same fact preserves the public fact;
  deleting the final owner retracts it exactly once.
- Rename is one owner retraction plus one owner insertion in one generation.
- A crash exposes either the previous or next ownership/canonical generation,
  never mixed state.
- Delta join variants match full rule evaluation for single-input, simultaneous,
  insert, retract, and recursive changes.
- Old content unavailable on edit/delete still retracts by stable path owner.
- Stale WORK/program results, lease expiry, and racing claim/ack cannot commit.
- Effects survive crashes before dispatch, after external success, and before
  outbox acknowledgement according to their declared delivery semantics.
- Interrupted/disk-full migration resumes or rolls back, and old binaries
  refuse to write a newer family epoch.
- Distinct payloads for one explicit dataflow semantic key abort atomically;
  traversal order cannot choose a winner.
- Non-UTF-8 or escaping owner paths fail before identity creation and never
  alias a lossy normalized path.

Scaling gates on deterministic fixtures with 10, 100, and 1,000 unrelated
files:

```text
changed owners = 1
parsed files = 1
projected files = 1
unrelated canonical rows written = 0
owner edges touched = O(changed-file facts)
source delta rows = O(changed-file fact difference)
incremental wall and staged RSS are independent of unrelated file count
```

Add the 3,000-file fixture before declaring the hot path complete. Its one-file
edit must satisfy the same work counts and the 128 MiB end-to-end RSS budget;
the cold build may scale in total work but not in simultaneously retained
source/AST/fact/encoded payload.

String/memory gates:

- `_strings` grows only for genuinely new opaque handles, not once per owner or
  generation.
- Payload strings are stored once per canonical fact, not once per ownership
  edge.
- No hot-path per-string SQL lookup and no per-edit dictionary sweep.
- Owner/fact storage bytes are reported separately; the factored representation
  must beat payload-per-owner storage on duplicate fixtures.
- A 1,000-edit churn test reaches stable RSS and database growth after content
  identities repeat.
- A heap/allocation profile must show no corpus-sized `Vec<Vec<Value>>`, second
  encoded corpus copy, or corpus-wide dedup set on the incremental path.
- Repeated failures/rollbacks and 100,000 superseding events do not grow queue,
  staging, WAL, temp files, or RSS without bound.

Performance gates:

- Existing-file body edit: under 250 ms first, then under 100 ms in release on
  the 107-file fixture.
- Dataflow edit work remains within 20% from 10 to 1,000 unrelated files.
- Unsupported fallback work is loud and attributed by rule and scope.
- SQL plan/page-visit counts stay flat across the unrelated-file scaling rail;
  wall time alone cannot hide a scan behind cache warmth.
- The production fallback remains available until exact equivalence, crash,
  database-size, and RSS rails pass.

## Staffing

- Base SHA: `2b10fbd6159b786ef008a6e3d48698821dd44c4b`.
- Luna owns bounded slices A-C: counters, typed identity helpers, dataflow
  ownership schema, tiny fixtures, delta transaction, and equivalence/scaling
  rails. Luna must not run `dl`, daemon commands, discovery, or production
  corpus benchmarks during the implementation loop.
- Terra reviews the owner/fact schema and owns slice D's delta algebra,
  generation semantics, fallback boundaries, and memory-growth audit.
- Separate worktree per slice touching engine/database code; rebase on the base
  commit and do not share modifications to dataflow or DB schema files.
- Suite budget: tiny ownership/equivalence tests under two minutes; scaling
  fixture under five minutes; production release measurement only as an
  explicitly approved final gate.
