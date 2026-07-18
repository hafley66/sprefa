# Storage diet for the per-root SQLite db (2026-07-18)

Planning-only arc. No engine code changes in this document. Executes the
measured, incident-driven slice of the relational storage-key audit
(plans/2026-07-15-relational-storage-key-audit.md), triggered by the
2026-07-18 kill-respawn read-storm incident (docs/failure-modes.md classes 16
and 17).

## 0. Build-vs-buy verdict (standing law)

The choice is **vanilla SQLite with a better schema**. No new storage
technology is proposed, so no candidate-by-candidate library analysis is owed.
The measurements below show the bloat is not compression-shaped (row entropy is
low but the cost is duplicated B-trees and one-use interned strings, not
payload size); zstd row compression, sqlite-zstd, a duckdb sidecar, or a custom
pager would each add a dependency and an opacity cost to attack a problem that
a schema change removes outright. The db is already 60% index bytes; the fix is
fewer and narrower B-trees, which is pure SQLite DDL. If a later step ever
proposes bespoke storage, that step owns the written candidate analysis; this
arc does not reach for it.

## 1. Measurements

Live db `~/.local/state/sprefa/roots/fbabddda40d22347/db.sqlite`, read-only
`dbstat` and `sqlite3` queries on 2026-07-18. Corpus 7.3MB / 712 files.

| metric | value |
|---|---|
| db bytes (live, pre-VACUUM) | 937.6 MB |
| db bytes (post-VACUUM, per incident) | 895 MB |
| corpus bytes | 7.3 MB |
| ratio (VACUUMed) | ~123x (CodeQL band 5-20x; this repo's scip index 21MB, ~3x) |
| total indexes | 758 |
| all autoindex bytes (full-row PK duplicates) | 232.6 MB (25% of db) |
| all secondary (named) index bytes | 329.6 MB (35% of db) |
| index bytes total | **562 MB = 60% of db** |
| dataflow/flow family total (tables + all indexes) | 311.9 MB (33% of db) |
| scip + named_call family total | 107.0 MB (11%) |

`_strings` interning table:

| segment | rows | content+norm text | note |
|---|---|---|---|
| `_strings` table on disk | 1,348,693 | 130.2 MB | avg content 43.1 chars, avg norm 34.2 |
| `_strings_norm_idx` | 1,348,693 | 66.7 MB | index on the `norm` column |
| coordinate composites, non-rev (`file:line:col:kind`) | 502,192 | 44.7 MB | unique by construction, zero sharing |
| rev-salted coordinates (`<40-hex-sha>\x01file:line:col:kind`) | 555,034 | 45.1 MB | second population, salt_rev at extract/mod.rs:1110 |
| coordinate total | 1,057,226 (78% of rows) | 89.8 MB (86% of text) | the interning tax buys nothing here |
| genuine interning candidates ("other": entity syms, identifiers, paths) | 291,467 (22%) | 14.6 MB | where sharing actually happens |

Top per-table consumers (table data | autoindex), MB:

| table | data | autoindex | secondary indexes |
|---|---|---|---|
| rel_df_node_rev | 17.1 | 17.1 | idx on id, fn, rev, var |
| rel_scip_occurrence | 13.7 | 13.7 | (none) |
| rel_named_call_site | 13.3 | 13.2 | (one call site fans to N rows: 353,406 rows / 3,615 callers) |
| rel_df_node | 12.8 | 12.8 | idx on file, fn, id, kind, line, var (6) |
| rel_scip_binding | 11.3 | 11.3 | (none) |
| rel_df_node_repo_rev | 9.8 | 9.8 | idx on id, repo, rev |
| rel_flow_edge | 8.6 | 8.6 | (none) |
| rel_df_edge_src_kind | 8.3 | 8.3 | (none) |
| rel_df_node_in_fn | 7.3 | 7.3 | idx on fn_sym, kind, node |
| rel_df_node_repo | 6.4 | 6.4 | idx on id, repo |

Schema facts that drive the cost:

- df_node identity is an interned coordinate string. `push_node`
  (src/graph/typegraph.rs) mints `id = "{file}:{line}:{col}:{kind}"`; the
  dataflow persister (src/engine/extract/dataflow.rs:88-96) routes that string
  through `sink.sym(...)`, so df_node.id is an INTEGER handle into `_strings`.
  The columns are already integers; the bloat is the 1.06M one-use coordinate
  strings sitting in `_strings` with a content copy, a norm copy, and a
  norm-index entry.
- Every `rel_*` table is a rowid table with an explicit composite PRIMARY KEY
  over its full column set. Example: `rel_df_node PRIMARY KEY (id, kind, var,
  fn, file, line)`. SQLite backs that PK with a UNIQUE autoindex covering all
  columns, so the autoindex duplicates the entire row. That is why autoindex
  is roughly equal to table size across the board.
- Secondary indexes are created by `create_auto_indexes`
  (src/engine/declare.rs:242) over the set `auto_indexes` returns
  (src/engine/strata.rs:295): one index per `(rel, col)` where a variable in
  that column position is a join key (appears in >=2 body atoms) across the
  union of every served program. On the sprefa root every std/ and examples/
  program is served, so every df_node column is a join key somewhere and each
  gets its own index. 758 indexes result; the tiny rels cost one page (4096 B)
  each, the df family costs the multi-MB entries above.
- The `_strings.norm` column has exactly one reader: the `string(id, text,
  norm)` rel projection at src/engine/extract/mod.rs:447. No `.dl` program
  filters or joins on `string`'s third argument (the only norm-shaped joins,
  examples/route-norm.dl, compute their own rel via `replace_re`). The
  `norm(x)` dl builtin is the deterministic scalar `sprf_norm` (src/db.rs:423,
  registered in src/lower.rs:21), computed at query time, not read from the
  column. The 66.7MB `_strings_norm_idx` is therefore never used to satisfy a
  query.

## 2. Design directions

### Direction 1: structural node identity

**Mechanism.** Stop routing the df_node coordinate string through `_strings`.
Give df_node a compact identity derived from its coordinates and reconstruct
the `file:line:col:kind` string only at the query and display boundary. Two
representation candidates (open decision A):

- (1a) a df-local dictionary: `rel_df_node` keyed on a dense integer `node_id`
  assigned in deterministic coordinate order during the flush, with
  `(file_id, line, col, kind)` as ordinary integer columns; `df_edge` /
  `flow_edge` keep their two-integer shape referencing `node_id`. The
  coordinate string is formatted on demand by a scalar over the columns.
- (1b) content-addressed `node_id = hash64(file:line:col:kind)`, single pass,
  edges reference the hash directly. Simpler writer, but a 64-bit hash over
  ~1M nodes carries a birthday collision probability near 2^-24, which the
  determinism law does not get to wave away; it needs a build-time collision
  probe.

**Type signatures first.** The public sym CONTRACT is a string. Introduce a
boundary function, not a stored column:

```
fn df_node_sym(file: &str, line: u32, col: u32, kind: NodeKind) -> String  // == today's push_node id
fn df_node_coord(sym: &str) -> Option<(FileId, u32, u32, NodeKind)>       // inverse, for query-time joins
```

`push_node` keeps returning the same `String` for in-extractor edge wiring;
only the persistence layer changes what it writes.

**Pseudo-code body.** In dataflow.rs, replace `sym(&n.id)` with the surrogate.
Under 1a: collect all nodes, sort by `(file_id, line, col, kind)`, assign
`node_id` densely, build a `coord_string -> node_id` map, then emit edges by
mapping `e.from`/`e.to` through it. Under 1b: `node_id = hash64(n.id)` inline.

**Instance lifetimes.** The coord->id map lives for one flush only (per-tick),
like the existing `SymSink`. No engine-lifetime state added.

**Storage layout.** `rel_df_node (node_id INTEGER, file INTEGER, line INTEGER,
col INTEGER, kind INTEGER, var INTEGER, fn INTEGER)` WITHOUT ROWID keyed on
`(node_id)`. The coordinate string leaves `_strings` entirely.

**Expected MB.** Removes the 502,192 non-rev coordinate rows from `_strings`
(with Direction 2, also the 555,034 rev-salted). After the norm column is gone
(Direction 3), `_strings` content-only text drops from ~58MB to the ~8MB
"other" share; the table falls from 130MB toward ~15-20MB. Attributable table
saving ~95-110MB, plus it makes the df_node PK a single column, which is what
unlocks Direction 4's autoindex collapse on the largest family.

**Risk.** Medium. df_node.id is user-visible in `.dl` (`df_node(id, ...)`), so
`df_node_sym` must reproduce today's string byte-for-byte, and `df_edge` /
`flow_edge` joins must stay integer-cheap. Determinism: the id-assignment
order must be a pure function of corpus content (1a) or the hash must be
collision-checked (1b).

**What it touches.** src/graph/typegraph.rs (push_node signature unchanged,
sym helper added), src/engine/extract/dataflow.rs (persist),
src/engine/decls.rs (df_node column set), the ref-spine class-2 residual (the
lossy `file:line:col` id without repo, docs/rca-exe-swap-write-storm.md:147).
N+1 law: still one `insert_rows` per rel; the coord->id map build is
in-memory, no per-row SQL. Crash-window (class 5): the df family already
unmarks/wipes/marks per component; a column-set change keeps that bracket.
Dirty-rel scoped rebuild: df_node is a source-fact rel; its digest is over
logical rows, so a representation change is invisible to scoping as long as
the determinism digest is over the logical coordinate tuple, not the physical
id.

**Verdict.** Adopt, but as the second structural wave, gated behind the cheap
index wins. Recommend 1a (dense dictionary) over 1b (hash) so the determinism
contract needs no collision waiver.

### Direction 2: rev scoping as data

**Mechanism.** Replace the sha-prefixed rev-salted string population and the
parallel `*_rev` twin tables with a small `rev` table (`rev_id INTEGER PRIMARY
KEY, sha TEXT`) and a `rev_id` integer column on the base rel. `salt_rev`
(extract/mod.rs:1110, `"{rev}\x01{id}"`) stops minting a second 45MB string
population; the rev becomes one small integer per row.

**Expected MB.** rel_df_node_rev (34.2MB with autoindex) +
rel_df_node_repo_rev (19.7MB) are today near-duplicates of their base tables
salted by rev. Collapsing rev to a `rev_id` column on the base (or keeping the
twin but with a small-int rev and a narrow PK) removes the 555,034 rev-salted
`_strings` rows (~45MB text) and shrinks the twin autoindexes. Attributable
~30-40MB, more once Direction 1 removes the coordinate part of the salted
string.

**Risk.** Medium-high. The `_rev` twins are consumed by
graph-diff-across-revs; the salt guarantees two revs' identical
`file:line:col` stay disjoint. Any change must keep tests/it/graph_diff_rev.rs
and tests/it/pr_diff.rs green. Whether the twin TABLE can be eliminated in
favor of a `rev_id` column, versus merely narrowed, depends on whether any
query needs the base (rev-less) and rev-scoped rows in the same scan; that is
open decision B.

**What it touches.** src/engine/extract/mod.rs (salt_rev,
refresh_rel_for_revs), src/engine/family/call_def_rev.rs and call_edge_rev.rs
(same twin pattern), the sym-keyed type_link migration (which introduces its
own rev-scoped identity and should ride this `(rev_id, ...)` scheme rather
than a parallel one).

**Verdict.** Adopt the string-population removal (drop the sha-salt in favor
of `rev_id`) with high confidence; treat twin-table elimination as a scoped
follow-up gated on the rev-consuming query audit. This is the direction most
entangled with other arcs, so it lands last.

### Direction 3: drop the norm column and its index

**Mechanism.** `norm` is the ASCII-alnum-lowercase fold (src/spine.rs:286).
Its only consumer is the `string` rel projection; every dl `norm()` is the
query-time scalar. So:

- (3a) drop `_strings_norm_idx` immediately (66.7MB, no query reads it).
- (3b) drop the `norm` column from `_strings`; in the `string` rel projection
  (extract/mod.rs:447) compute the third argument as `sprf_norm(content)` at
  read time.

**Expected MB.** 3a saves 66.7MB with near-zero behavior risk. 3b saves the
norm text share of the `_strings` table (avg 34 chars of the 77-char
content+norm per row, ~44% of the table's text), roughly 46MB of table on disk
after VACUUM. Combined ~110MB.

**Risk.** Low for 3a (prove no EXPLAIN QUERY PLAN uses `_strings_norm_idx`;
the audit shows zero `WHERE norm`). Low-medium for 3b (the `string` rel's
third column must stay byte-identical, which it does because it is the same
`spine::normalize`; only the moment of computation moves from write to read).

**What it touches.** src/storage.rs:376, src/storage/call.rs (three
`_strings` DDL sites), src/engine/meta.rs:373 (the index),
src/engine/extract/mod.rs:442-447 (projection). No dl-facing change;
docs/reference/functions.md's norm description stays true.

**Verdict.** Adopt. 3a is step 1 of the whole arc (the fattest low-risk
receipt in the repo). 3b follows once the projection change is tested.

### Direction 4: autoindex elimination

**Mechanism.** The full-row composite PK on every rel produces an autoindex
that duplicates the row. Two levers:

- (4a) WITHOUT ROWID on pure junction/set tables (flow_edge, df_edge, the many
  two-and-three-column edge rels). The table becomes its own PK B-tree; the
  duplicate autoindex vanishes and no rowid is stored. flow_edge today is
  8.6MB table + 8.6MB autoindex; WITHOUT ROWID is ~8.6MB total, a 50% cut on
  that table.
- (4b) narrow the PK to the genuinely-unique subset where the writer already
  guarantees uniqueness. Once Direction 1 gives df_node a single-column
  `node_id`, its PK is `(node_id)` and its autoindex drops from full-row
  (12.8MB) to single-column (~2MB).

The determinism law (identical corpus = identical rows) plus
collect-then-flush-with-dedup means the DB UNIQUE constraint is
belt-and-suspenders that the determinism it-test already covers logically.
Dropping or narrowing it does not weaken the contract the tests pin; it
removes a B-tree that re-proves what the writer guarantees.

**Expected MB.** The autoindex pool is 232.6MB. WITHOUT ROWID on the
edge/junction tables and narrow-PK on the identity tables realistically
reclaims 120-180MB, net of the secondary-index locator growth WITHOUT ROWID
causes (a WITHOUT ROWID table's secondary indexes carry the full PK as
locator, so this lever is only a win where the PK is narrow or the table has
few secondary indexes; the classifier from the storage-key-audit plan decides
per table).

**Risk.** Medium. Set semantics must survive: a writer bug that emits
duplicates would, without the UNIQUE constraint, store duplicates silently
instead of erroring. The mitigation is that the determinism it-test digests
logical rows across two rebuilds and would catch a duplicate-emitting
regression. Lost conflict detection is the cost; quantify per table in the
receipt.

**What it touches.** The DDL generator (src/engine/declare.rs,
src/storage.rs), the storage-key-audit classifier. Crash-window and scoped
rebuild are unaffected (they operate on rows, not on PK shape).

**Verdict.** Adopt WITHOUT ROWID for measured pure junctions early
(independent of identity work); adopt narrow-PK for df_node as a consequence
of Direction 1. This is the second-largest lever after Direction 5.

### Direction 5: index audit on the secondary pool

**Mechanism.** 329.6MB of secondary indexes, more than the entire autoindex
pool, come from `create_auto_indexes` building one index per join-key column
across the union of every served program. The audit: for the rels that
dominate the pool (the df family, scip, named_call), run EXPLAIN QUERY PLAN
read-only against the live db for the hot query shapes and keep only the
indexes an actual plan chooses. Columns that are join keys in a rarely-run
served program but never selected by the planner on real data are dropped, or
the auto-index policy is made demand-aware (index a column only when a served,
active program joins it, not every catalogued program).

**Expected MB.** The single largest category. Conservatively half the pool is
redundant on real query plans (low-selectivity columns like `kind`, columns
whose rel is small, duplicate coverage of the autoindex). Estimate 150-220MB
reclaimable, the widest range in this plan and the one most in need of EXPLAIN
receipts before commitment.

**Risk.** Low for correctness (indexes never change results), medium for
latency (a dropped index the planner did want slows a query). Fully
revertable: re-run `CREATE INDEX`. Each drop needs a before/after query-timing
receipt, not only a byte receipt.

**What it touches.** src/engine/strata.rs:295 (`auto_indexes` selectivity),
src/engine/declare.rs:242 (creation). No schema-row change; purely which
B-trees exist.

**Verdict.** Adopt, and sequence it early because it is byte-huge,
correctness-safe, and trivially revertable. Pair every drop with an EXPLAIN
QUERY PLAN and a timing receipt so a latency regression is caught in the same
step.

### Direction 6 (numbers-led): the db is 60% index

The measurement that reframes the arc: 562MB of 937MB is index bytes.
Directions 4 and 5 together attack that 60%. The `__src TEXT DEFAULT ''`
universal column is empty on every rel_df_node row (245,129/245,129 blank),
consistent with the storage-key-audit's P0 to compact it; it is a minor
per-row cost here but folds into the same DDL passes. No new direction is
opened; the numbers say the index pool, not the data, is the whale, which is
why the sequence front-loads Directions 5 and 3a.

## 3. Chosen sequence (shippable steps, each revertable, each with a receipt)

Each step: apply, VACUUM, measure db bytes and the affected tables' `dbstat`
bytes, run the determinism oracle (scripts/rails-oracle.sh) for byte-identical
logical rows, record a before/after MB receipt on the sprefa corpus per the
receipts law (failure-modes.md "how a new rail gets born" step 5).

| # | step | direction | expected MB | gate / receipt |
|---|---|---|---|---|
| 1 | drop `_strings_norm_idx` | 3a | -66.7 | EXPLAIN shows no plan used it; db 937 -> ~871 |
| 2 | index audit: EXPLAIN the df/scip/named_call hot shapes, drop indexes no plan chooses; make `auto_indexes` demand-aware | 5 | -150 to -220 | per-drop EXPLAIN + timing receipt; db ~871 -> ~660-720 |
| 3 | drop `norm` column, recompute in the `string` projection | 3b | -46 | `string` rel byte-identical; db -> ~615-675 |
| 4 | WITHOUT ROWID for measured pure junction rels (flow_edge, df_edge, edge tables) | 4a | -60 to -90 | classifier + dbstat per table; determinism oracle |
| 5 | structural df_node identity (dense dictionary, 1a); df_node PK -> `(node_id)` | 1 | -95 to -110 | df_node_sym byte-identical; dataflow tests green; removes coord strings from `_strings` |
| 6 | narrow-PK autoindex collapse on the df family (rides step 5) | 4b | -40 to -60 | autoindex dbstat per table |
| 7 | rev as `rev_id` column, drop sha-salt string population | 2 | -30 to -45 | graph_diff_rev + pr_diff green; twin-table elimination deferred to a scoped follow-up |

**Projected end state (conservative arithmetic).**

937.6 - 66.7 (s1) - 180 (s2, mid) - 46 (s3) - 75 (s4) - 100 (s5) - 50 (s6)
- 37 (s7) = **~382 MB**.

382 MB / 7.3 MB = **~52x**, down from 123x (VACUUMed) / 128x (live), a 2.4x
reduction. Honest range across the estimate spread: 380-480MB, ~52-66x. This
clears "an order of magnitude larger than corpus is a defect to explain"
(class 17 law) by taking the ratio from ~123x to the ~50-65x band, but does
not reach CodeQL's 5-20x. Reaching that band requires the dataflow family
redesign (the df family is 33% of the db and structurally rich), which is a
larger arc than a storage diet and belongs to the decomposition-normalization
and ref-spine work, not here. Steps 5-7 are the down payment on it.

Sequencing rationale: steps 1-3 are byte-huge, correctness-safe or
single-consumer, and independently revertable, so they ship first and buy the
most ratio per unit risk. Steps 4-7 are the structural wave and are ordered so
that identity (5) precedes the autoindex collapse it enables (6), and rev (7)
lands last because it is the most entangled with other arcs.

## 4. Open decisions with recommendations

USER RULINGS (2026-07-18, voice): A = 1a dense dictionary ("dense, hash,
autoincrement, I don't care, just do something"). D = drop the norm column;
norm stays available as the query-time `norm()` scalar (the v0 self-join-on-
norm exploration survives unindexed; index-on-demand if a program joins it).
C = both levers (per-table drops + demand-aware policy; "index conditionally
if they're joined"). B and E stand as recommended. Posture note: prefer
integrating existing crates over building; the mod list may grow and get
whittled later.

- **A. df_node identity representation.** Dense dictionary (1a) vs content
  hash (1b). Recommend 1a: it keeps the determinism contract free of a
  collision waiver, at the cost of a coord->id map build during flush
  (in-memory, no N+1).
- **B. rev twin tables: eliminate or narrow.** Recommend narrow first
  (small-int `rev_id`, drop the sha-salt string), defer twin-table elimination
  until the rev-consuming query audit proves no scan needs base and rev-scoped
  rows together. Full elimination is the bigger prize but the higher-risk one;
  split it.
- **C. index-audit policy.** Drop-and-keep individual indexes (fast,
  per-table) vs make `auto_indexes` demand-aware (structural, prevents
  regrowth). Recommend both: the per-table drops for the immediate receipt,
  the policy change so the next served program does not silently re-inflate
  the pool.
- **D. norm column removal vs keep-with-lazy-index.** Recommend full column
  removal (3b): a single recompute-able consumer does not justify 46MB of
  stored fold plus the risk of the index regrowing.
- **E. ratio ceiling for the boot verdict line** (class 17 candidate rail).
  Recommend the boot verdict prints `db_bytes, corpus_bytes, ratio` and
  `--check` warns above a ceiling; set the ceiling at 60x initially (above
  this plan's projected ~52x, below today's 123x), tightening as steps land.
  This rail is shared surface with the obs-logging boot line.

## 5. Interplay

- **(a) decomposition-normalization plan
  (plans/2026-07-18-decomposition-normalization.md).** That arc is pure code
  motion (file splits) and explicitly names "any schema or rel change" a
  non-goal. It touches src/graph/typegraph.rs (where `push_node`/`mint_sym`
  live) and src/engine/extract/dataflow.rs (a consumer). Sequencing rule: land
  the storage diet's typegraph-touching step 5 either before that arc starts
  its typegraph split or after it completes, never interleaved, to avoid the
  modify/delete conflict that killed refactor/file-splits. The decomp arc does
  not rewrite any storage step, so there is no double work; they are
  orthogonal except for file locality on typegraph.rs and dataflow.rs.
- **(b) ref-spine debt.** The class-2 residual is the lossy df_node id
  (`file:line:col`, no repo). Direction 1 rebuilds df_node identity and is the
  natural place to carry repo/rev scoping properly. Coordinate so the
  ref-spine's principled id and this arc's structural id are the same change,
  landed once.
- **(c) no-daemon erasure arc and obs-logging arc.** Both edit db-open paths.
  The storage diet adds a schema-version bump and a migration on open.
  Sequence the diet's open-path touch after obs-logging lands (or coordinate a
  single open-path edit) so the open path is not rewritten twice. Put the
  class-17 boot verdict line (db bytes / corpus bytes / ratio) inside
  obs-logging's boot line rather than a parallel print.
- **(d) sym-keyed type_link migration.** It changes node identity for
  type_edge -> type_link and introduces rev-scoped identity. It must ride
  Direction 2's `(rev_id, ...)` scheme and Direction 1's boundary-sym
  contract, not a parallel representation. Land the identity and rev-column
  shapes here first so type_link consumes them.
- **Relational storage-key audit (plans/2026-07-15).** This diet is the
  measured execution of that audit's P0 items (autoindex/WITHOUT ROWID,
  `__src` compaction, identity narrowing). Reuse its classifier (label each
  rel entity/occurrence/junction/payload) and its family-slice migration
  discipline; this document supplies the incident receipts and the
  numbers-led priority order the audit asked for.

## 6. Test plan

**Existing tests that pin the contract (must stay green, unchanged,
byte-for-byte):**

- tests/it/extraction_determinism.rs
  (`extraction_is_deterministic_across_identical_rebuilds`, a45c34d9): digests
  every rel's rows across two rebuilds. Keep it meaningful by digesting the
  LOGICAL coordinate tuple, not the physical `node_id`, so a representation
  change does not falsely pass or fail.
- tests/it/callable_defs.rs (`lambda_and_ctor_extraction_is_byte_identical`,
  `every_lambda_sym_names_a_df_closure_scope`): the lambda_sym anti-join,
  `call_def.sym == df_node.fn_sym` for the lambda body, `::closure::`
  byte-exact. Direction 1's `df_node_sym` must reproduce these strings.
- tests/it/dataflow.rs: `closure(df_edge)` walks the lifted graph; pins
  df_node/df_edge join semantics through the identity change.
- tests/it/graph_diff_rev.rs, tests/it/pr_diff.rs,
  tests/it/daemon_stateful_revs.rs: rev-scoped identity across revs; pin
  Direction 2.
- tests/it/cold_stage.rs: staged == inline per-rel row counts.
- tests/it/derived_scope.rs, tests/it/tick_digest.rs: crash-window scoping and
  `_derived_complete` markers; pin that steps 4-7 do not disturb class-3/5
  rails.
- src/db.rs:782 guard (chunked inserts stay under the `[n+1]` counter): pin
  that the coord->id map build does not introduce per-row SQL.

**New discriminating tests (proven fail-pre-fix per the "how a new rail gets
born" pipeline):**

1. `_strings` holds no coordinate composite after Direction 1: assert zero
   rows matching `*:*:*:*` in `_strings`. Fails on today's db (1.06M such
   rows), passes after.
2. `df_node_sym` round-trips: for a sample of df_node rows,
   `df_node_coord(df_node_sym(...))` reconstructs the original coordinates,
   and the reconstructed string equals the pre-change interned string
   byte-for-byte.
3. norm-free `string` rel: after Direction 3, `string(id, text, norm)` returns
   the same third column as `sprf_norm(text)` for every row, with the column
   and index absent from the schema.
4. autoindex byte ceiling: a `dbstat` assertion that no rel's autoindex
   exceeds its table by more than a small factor (catches a full-row-PK
   regression reintroducing the duplication).
5. index-pool guard: total secondary index bytes on the sprefa corpus stay
   under a ceiling the audit sets (catches `auto_indexes` regrowth).
6. class-17 ratio rail: boot verdict prints the ratio; `--check` warns above
   the ceiling (open decision E). Proven by pointing it at today's 123x db
   (warns) and a post-diet db (quiet).

Storage and wall-time measurements are the evidence; a passing suite is not
proof of improvement (storage-key-audit verification law). Every step carries
a `dbstat` before/after and, for Direction 5, an EXPLAIN QUERY PLAN and
query-timing receipt.

## Appendix: critical files for implementation

- src/engine/extract/dataflow.rs (df_node persist; where the coordinate string is interned)
- src/graph/typegraph.rs (push_node / mint_sym / lambda_sym; the sym boundary)
- src/engine/declare.rs + src/engine/strata.rs (per-column secondary-index policy: `create_auto_indexes` / `auto_indexes`)
- src/engine/meta.rs + src/storage.rs (`_strings` DDL, norm column and index, rel table PK generation)
- src/engine/extract/mod.rs (salt_rev, `string` rel projection, rev-scoped twin refresh)
