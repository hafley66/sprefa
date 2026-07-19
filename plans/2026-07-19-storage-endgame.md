# Storage endgame (2026-07-19)

Successor to plans/2026-07-18-storage-diet.md after its steps 1+3 (norm column
and `_strings_norm_idx` gone), step 2 (demand-aware index reconcile), and step
4a (WITHOUT ROWID on 17 vouched junctions; 22 tables WITHOUT ROWID in the live
schema) landed. This plan measures every remaining byte on this machine's real
dbs and orders every remaining lever down to a projected minimum.

Method: every db copied to `$TMPDIR` first (never opened live), all queries
read-only on the copies, every experiment `nice -n 19`. Byte numbers are
`dbstat` aggregates (`sum(pgsize)` per object) or `ls` file sizes, labeled.
Experiments that "apply" a lever (VACUUM INTO, DROP, WITHOUT ROWID copies) ran
only against the copies. Snapshot date 2026-07-18 19:24, daemon pid 4424 live
throughout.

## 1. Inventory: every sprefa-owned byte on this machine

| file | bytes | WAL | note |
|---|---:|---:|---|
| roots/fbabddda40d22347/db.sqlite (served sprefa root) | 971,554,816 | 45,352 (+1.5MB shm) | THE db; corpus 7.3MB / 712 files, ratio ~127x live |
| ~/projects/sprefa/.dl/.state/cache.db (one-shot twin) | 974,495,744 | — | regrown 2026-07-18 18:56, same day the 1.7GB fossil was deleted |
| ~/projects/smashy/.dl/.state/cache.db | 83,849,216 | — | freelist 8,270 pages = 33.9MB (40% slack) |
| roots/ba88bf9f6de53b7f/db.sqlite (smashy root) | 44,212,224 | 14,564,232 | WAL 33% of db |
| roots/38eab934bde24444/db.sqlite | 21,401,600 | 0 | NOT in roots.json — orphan root dir |
| invocations.db | 20,701,184 | — | 41,340 rows; retention sweep exists (invlog.rs:89) |
| daemon.log | 13,493,259 | — | 8MB cap enforced only at spawn (daemon.rs:2560); grows past it while daemon lives |
| roots/98dfaa7291e94fae/db.sqlite (instant root) | 9,228,288 | 4,140,632 | instant repo has no .dl/.state twin |
| ~/.local/state/sprefa/.dl/perf.jsonl | 65,669,936 | — | daemon-home perf log, NO cap or rotation (perflog.rs) |
| db.sqlite (daemon home) + jobs.sqlite | 1,265,664 + 1,454,080 | 4,136,512 + 4,787,472 | WAL ~3.3x db on both |
| repo perf.jsonl (sprefa/smashy/instant) | 3.7M + 4.3M + 1.9M | — | post-watch-filter growth is slow |
| why.jsonl + launchd logs + tray.log + log/ | ~9.4M | — | log/dl.log rotates (dl.log.1 exists); the rest do not |

Machine-wide sprefa footprint ~2.28GB. The served-root db plus its one-shot
twin is 1.95GB of it, for one 7.3MB corpus.

## 2. Byte classification, served sprefa root

`dbstat` object total 965,263,360 (file 971.5MB; the gap is freelist 1,536
pages = 6.3MB + per-page slack). Post-`VACUUM INTO`: 893,378,560. 515 tables
(483 `rel_*`, 7,564,430 rel rows), 770 `idx_*` indexes.

| category | bytes | share |
|---|---:|---:|
| explicit `idx_*` indexes (non-rev tables) | 298,823,680 | 31.0% |
| autoindex twins (`sqlite_autoindex_*`, incl. 24.0MB on `_rev` tables) | 254,894,080 | 26.4% |
| rel table data (non-rev) | 223,600,640 | 23.2% |
| `_strings` | 78,860,288 | 8.2% |
| `idx_*` on `_rev` shadow tables | 55,971,840 | 5.8% |
| `_rev` shadow table data | 36,614,144 | 3.8% |
| meta (`_prov`, `_call_raw_site`, `_where_bytes`, ledgers) | 15,925,248 | 1.6% |

By family: df/flow 340.5MB (35.3%), port (flow-panel.dl) 216.1MB (22.4%),
call rels 128.5MB, `_strings` 78.9MB, other rels 65.4MB, scip 57.0MB, map
36.9MB, `_call_*` meta 20.9MB, engine meta 11.4MB, type 7.3MB, module 1.9MB.

**The derived/source split is the headline.** 280 rels are derived
(`_derived_complete`); their tables + indexes are **547,815,424 bytes = 56.7%**
of all object bytes (tables 155.9MB, autoindex 170.9MB, `idx_*` 221.0MB).
Measured directly: dropping all 280 `rel_*` derived tables on a copy and
VACUUMing gives **386,248,704 bytes** (893.4 → 386.2, −507.1MB). Every derived
byte is recomputable by `rebuild_derived` from source facts.

`_strings` (norm column already gone): 1,160,312 rows.

| segment | rows | content bytes |
|---|---:|---:|
| coordinate composites `file:line:col:kind` | 505,627 | 24,953,663 |
| rev-salted (`WORK\x01…` / sha-salted; 2 distinct revs live) | 417,127 | 19,785,864 |
| genuine interning candidates | 237,558 | 6,359,520 |

79.5% of rows and 87.6% of content bytes are one-use coordinate keys.

**Two-worlds duplication, measured.** Joining `dbstat` object lists of the
served root db and the repo cache.db: shared-object overlap (sum of per-object
min bytes) = **930,217,984**. Root-only 28,672; cache-only 38,514,688. The
same corpus is stored twice to within 4%.

## 3. Functional dependencies, measured (the normalization section)

Probe: `det_card = COUNT(DISTINCT determinant)`, `pair_card = COUNT(DISTINCT
determinant+dependents)`; FD holds iff equal; `violations = pair_card −
det_card`. Redundancy bytes = `(rows − det_card) × avg serial bytes of
dependents` (sampled per-value SQLite serial widths). Script archived in the
session log; all probes on the copy.

| rel | determinant | dependents | rows | det_card | violations |
|---|---|---|---:|---:|---:|
| rel_df_node | id | kind,var,fn,file,line | 274,065 | 274,065 | **0** |
| rel_df_node | file,line | id | 274,065 | 64,167 | 209,898 |
| rel_df_node_rev | id,rev | kind,var,fn,file,line | 292,014 | 292,014 | **0** |
| rel_df_node_rev | id | rev | 292,014 | 292,014 | **0** (salted id ⇒ id alone is key) |
| rel_df_node_repo_rev | id | repo | 292,014 | 292,014 | **0** (repo has 1 distinct value) |
| rel_df_node_repo_rev | id | rev | 292,014 | 292,014 | **0** |
| rel_df_node_in_fn | node | fn_sym,kind | 227,935 | 227,935 | **0** |
| rel_df_edge_src_kind | from_node | from_kind | 211,595 | 185,182 | **0** |
| rel_df_edge_src_kind | from_node | fn_sym | 211,595 | 185,182 | **0** |
| rel_df_edge_src_kind | from_node,to_node | fn_sym,from_kind | 211,595 | 211,595 | **0** |
| rel_named_call_site | file,line,fn | caller | 444,452 | 444,023 | 429 |
| rel_named_call_site | file,line | fn | 444,452 | 33,678 | 410,345 |
| rel_scip_occurrence | file,line,col | symbol,end_line,end_col,role | 253,998 | 252,823 | 1,175 |
| rel_scip_binding | symbol | local_name | 180,802 | 9,115 | 57,333 |
| rel_call_node | id | callee,file,line | 144,054 | 64,765 | 79,289 |
| rel_call_def | sym | repo,kind,file,line,end | 11,768 | 10,749 | 1,019 |

Cross-rel probes (redundancy across tables, all-rows exact):

- `rel_df_node_in_fn.kind = rel_df_node.kind` for **227,935 / 227,935** rows —
  the kind column is a verbatim copy. (`fn_sym` matches `df_node.fn` for
  0 rows — different sym namespace, so `fn_sym` is genuine information.)
- `rel_df_edge_src_kind.from_kind = rel_df_node.kind` for **211,595 / 211,595**
  rows — the whole rel is `df_edge ⋈ df_node` materialized; at-rest bytes
  (36.1MB with indexes) are 100% recomputable from rels that already exist.
- `rel_df_node_repo_rev.repo` has **1 distinct value** across 292,014 rows,
  and carries 3 `idx_*` (30.4MB) plus an autoindex on a table whose `id` is
  already unique.
- `rel_df_node_rev` shares zero rows with `rel_df_node` on any column —
  because its `id` is a second, salted `_strings` population (`WORK\x01…`),
  not because the nodes differ.

What the FDs prove, with arithmetic:

1. **df_node id is a candidate key and its columns are functions of the id
   string.** `id` interns `file:line:col:kind`, and the table then stores
   file, line, kind again as columns: the FD is stored twice, once as string
   bytes in `_strings` (24.95MB content for the non-rev population) and once
   as columns. The dense-dictionary decomposition (diet Direction 1a, user
   ruling A) is exactly the BCNF repair: node dim table keyed `(node_id)`,
   coordinate string minted only at the query/display boundary.
2. **The rev shadows are a 116.6MB encoding of a 2-value column.** Rev pool
   today: `_rev` tables 36.6MB + their `idx_*` 56.0MB + their autoindexes
   24.0MB. Distinct revs live: 2. Normalized shape: `rev(rev_id INTEGER
   PRIMARY KEY, sha TEXT)` (2 rows) + `(node_id, rev_id)` WITHOUT ROWID
   junctions. df_node_rev alone: 292,014 rows × ~8B ≈ 2.3MB vs today's
   56.9MB for that one family (table 17.9 + autoindex 19.9 + idx 21.1).
   Family-wide projection: 116.6MB → ~8-10MB (**−106MB**), and the 417,127
   salted `_strings` rows (19.8MB content) stop existing.
3. **rel_df_node_in_fn needs one column dropped and a narrow PK.** `kind` is
   a copy (227,935/227,935); `node` is unique. `(node PRIMARY KEY, fn_sym)`
   WITHOUT ROWID ≈ 227,935 × ~10B ≈ 2.3MB + one fn_sym index ~2MB vs today's
   30.2MB (**−26MB**, it is derived so this lands as a rule/decl change).
4. **rel_df_edge_src_kind should not exist at rest.** Fully derivable
   (**−36.1MB** at rest; becomes a view or a leaner rule joining at read).
5. **Negative results pin the limits**: scip_occurrence has no clean FD
   (1,175 position collisions — multiple symbols per span is real), so no
   dimension split there; call_node/call_def/scip_binding fan out.
   named_call_site's `(file,line,fn) → caller` misses by 429 rows —
   worth one investigation (multi-caller lines vs extraction bug) before
   any decomposition of its 66.6MB.

**Engine surface for FDs.** The decl language already has `key(...)`
(declare.rs honors it; `wants_without_rowid` requires its absence). Discovered
0-violation candidate keys should become declared `key()`s: df_node `key(id)`,
df_node_in_fn `key(node)`, df_node_repo_rev `key(id)` — each narrows the PK
B-tree, turns the autoindex from full-row to key-only, and makes the writer's
uniqueness contract enforced instead of assumed. Rail candidate (check-tier,
on demand, not per tick): for each rel above a byte floor, run the two-COUNT
probe for its declared key vs the full row; warn when data exhibits a
candidate key strictly narrower than the declared PK ("rel df_node exhibits
key (id); decl stores full-row PK — 15.9MB autoindex re-proves it"). One SQL
statement per rel per probe, read-only, and the probe itself is the receipt.

## 4. Experiments (all on copies)

| experiment | result |
|---|---|
| `VACUUM INTO`, page_size 4096 | 893,378,560 |
| `VACUUM INTO`, page_size 8192 | 895,148,032 (+1.8MB — worse) |
| `VACUUM INTO`, page_size 16384 | 908,181,504 (+14.8MB — worse) |
| drop 280 derived rels + VACUUM | 386,248,704 (−507.1MB) |
| zstd -3 of vacuumed db | 321,795,565 (36.0%) |
| zstd -19 of vacuumed db | 205,100,565 (23.0%) |
| NULL scan, 8 junction candidates (flow_edge, df_edge_src_kind, named_call_site, df_node_in_fn, port_reach, port_non_port_flow_edge, call_node, map_edge) | **0 NULLs in every PK column** |

WITHOUT ROWID rebuild of the biggest `.dl`-declared junctions (real rows,
`INSERT OR IGNORE` into a `WITHOUT ROWID` twin, `dbstat` measured):

| rel | today (table+autoindex) | WITHOUT ROWID | saving |
|---|---:|---:|---:|
| rel_named_call_site | 35,393,536 | 17,330,176 | 18.1MB |
| rel_flow_edge | 19,853,312 | 9,363,456 (8,957,952 without `__src`) | 10.5MB |
| rel_df_edge_src_kind | 20,082,688 | 9,977,856 | 10.1MB |
| rel_df_node_in_fn | 17,186,816 | 8,347,648 | 8.8MB |
| rel_port_reach | 14,938,112 | 7,098,368 | 7.8MB |

`__src` cost, measured on flow_edge: 405,504 bytes / 353,773 rows = 1.15 B/row
⇒ ~8.7MB across all 7,564,430 rel rows.

Composition of the derived-dropped floor (386.2MB), from its own `dbstat`:
`_strings` 71.2MB, explicit idx 76.3MB, autoindex 73.8MB, source tables
65.8MB, rev shadows 105.9MB (34.8 tables + 49.8 idx + 21.3 autoindex), meta
14.3MB.

## 5. Levers, candidate-by-candidate, ordered by bytes-per-effort

### L1. Disk hygiene + gc (no schema change) — ~1.13GB machine-wide

- Delete `.dl/.state/cache.db` for any root a daemon serves (sprefa 974.5MB,
  smashy 83.8MB) and the orphan `roots/38eab934bde24444` dir (21.4MB). A
  `dl daemon gc` subcommand makes this repeatable: sweep roots dirs not in
  roots.json, sweep `.dl/.state/cache.db` under registered roots.
- perf.jsonl rotation: daemon-home copy is 65.7MB with no cap. Candidates per
  the infra law: (a) `tracing-appender` rolling files — already the logging
  spine's library, `log/dl.log.1` proves the pattern in-tree; (b) bespoke
  size-capped rewrite like daemon.log's — rejected, that cap already fails
  (13.5MB > 8MB cap, enforced only at spawn); (c) logrotate/newsyslog OS
  config — viable but machine-local. Verdict: perf records become tracing
  events on a `perf` target with a rolling-file subscriber (the obs-logging
  arc already owns invlog/why/verdict migration; perf.jsonl joins it).
  daemon.log's spawn-only cap gets the same treatment.
- Risk: none to correctness; cache.db regrows on the next daemonless one-shot
  until L2 lands (the regrowth we measured: deleted morning of 07-18, 974MB
  again by 18:56).
- Gates: byte receipt (`du` before/after); a served root still answers
  queries; one-shot fallback still works (it rebuilds, slowly — the L2
  motivation).

### L2. Two-worlds unification — one db per corpus (−~975MB steady state)

Measured duplication: 930.2MB object overlap between the twins. The axum
merge (2067ab79) erased the public `--no-daemon` split; what remains is the
in-process fallback engine building `.dl/.state/cache.db` when no daemon
serves the root (lib.rs:389 run_check fallback, hook.rs:503, lsp.rs) and
tests. Candidates:

- (a) Fallback opens `roots/<key>/db.sqlite` directly. The daemon-down case
  has no contention by definition; the daemon-up-but-attach-failed case is
  the risk (two writers, one db) — but that pairing already exists for
  `.dl/cache.db` (db.rs:184 busy_timeout comment) and WAL + busy_timeout is
  the existing discipline. cache.db becomes purely historical.
- (b) Fallback runs read-only against the root db, refusing writes (stale
  answers when daemon is down, no rebuild storms).
- (c) Status quo + L1 gc (accept regrowth, sweep it).
Verdict: (a), with (b)'s read-only mode as the `--check`-from-hooks fast path
(hooks should never cold-build 900MB). Prerequisite: db-seam migration (the
Db struct owns every open path — this is one open-path edit, per the diet's
interplay (c)).
- Gates: it-suite fallback tests repointed; lock-contention test (daemon up +
  forced in-process run); byte receipt = `.dl/.state` empty for served roots.
- EXECUTED (2026-07-18, branch `two-worlds`): candidate (a) as written.
  `daemon::root_db_path(root)` (src/daemon/home.rs) is the one path fn —
  canonicalize + `key_of`, mirroring `add_root` — and the cli defaulting block
  (src/cli/mod.rs) points at it; the `.dl/.state/cache.db` default, its
  gitignore write, and the old-layout `.dl/cache.db` migration are deleted
  (cache.db historical; L1 gc owns the sweep). LSP recomputes the defaulted
  path after the client's rootUri overrides the cwd root (src/lsp.rs) so an
  editor-spawned server never grows a wrong-key db.
  Deviations from the letter of the verdict:
  - (b)-as-fast-path is keyed on `--max-wall` (the flag the deadline-wrapped
    hook surface passes), not on "invoked from a hook" generally: the git
    pre-commit `dl --check` (no `--max-wall`) keeps the full in-process
    engine, because a commit gate answering from stale rows could pass a rail
    that current bytes trip. Read-only mode = `run_check_readonly` in
    src/lib.rs (ReadDb over `rel_diag_txt`/`rel_diag_stage_txt`, same NULL
    defaults as `Engine::diags`); a blank/absent db warns and renders type
    diags only — never a cold build.
  - `dl --hook`'s in-process fallback still write-ticks (the event insert +
    re-derive IS its semantics; a read-only run could not evaluate
    block/inject for the incoming event). Its existing cold-skip already
    refuses cold builds under the deadline, and L2 makes its warm case warmer:
    the defaulted db is now the daemon-warmed root db.
  Gates landed as tests/it/check_daemon.rs `forced_inprocess_check_shares_the_
  daemon_db` (lock contention: daemon up + DL_NO_DAEMON one-shot on ONE db),
  `hook_check_fallback_is_read_only` (byte-identical db after the fast path;
  no cold build with no db), `discovery_check_cold_fallback_is_loud`
  (daemonless one-shot builds `roots/<key>/db.sqlite`, `.dl/.state/cache.db`
  never appears), and tests/it/discover.rs (defaulting + fossil-untouched).

### L3. Derived storage policy — the 507MB whale

Measured: 280 derived rels = 547.8MB of objects; drop+VACUUM = 386.2MB file.
The port family alone (flow-panel.dl's `port_*` incl. `_2hop.._5hop`,
`_len1.._len4` layers) is 216.1MB — one dashboard program. Candidates:

- (a) Status quo: every derived rel materialized in the main db forever.
- (b) Derived sidecar: `ATTACH '<root>/derived.db'`; `rebuild_derived` writes
  rel tables there; main db holds source facts + meta only. The sidecar is
  disposable by contract (crash mid-write ⇒ delete + rebuild;
  `_derived_complete` already brackets exactly this). VACUUM of derived
  churn never touches the source db. At-rest main db: 386.2MB measured.
- (c) Demand-gated materialization: materialize a derived rel only while a
  served program reads it (the index reconcile's exact policy, lifted from
  indexes to rels); dropped rels rebuild on next demand. Kills the port
  family's 216MB whenever flow-panel is not being served.
- (d) Views for provably-cheap derivations: `df_edge_src_kind` (a measured
  100% join copy), the hop/len port layers (each one join over its
  predecessor). No storage, recompute per query; only where the EXPLAIN
  timing receipt stays flat.
Verdict: (b) + (c) compose (sidecar holds whatever demand says exists), (d)
per-rel where the timing receipt allows. Recursion (`port_reach`,
`closure(...)`) stays materialized — fixpoints are not view-shaped.
- Risk: recompute latency on first demand after eviction (the 109s
  comment-rels job on games/smash is the cautionary receipt — eviction must
  respect the cold-chunk seam and the write budget); query planner behavior
  across ATTACH (same-schema names need the rel→db routing to be explicit in
  the Db seam).
- Prerequisites: db-seam migration (routing), scheduler write-budget arc
  (rebuild bursts), cold-chunk extension (resume).
- Gates: byte receipt per family; query byte-equivalence on `_txt` dumps
  (the 4a receipt method); determinism oracle; crash-resume test that kills
  mid-sidecar-rebuild and proves clean recovery; timing receipts for every
  view conversion and eviction policy.

### L4. Rev normalization (FD finding 2) — −~106MB

`rev_id` dimension + `(node_id, rev_id)` junctions; delete the salted string
population (417,127 rows / 19.8MB content; salt observed as `WORK\x01` +
coordinate). Arithmetic above: 116.6MB → ~8-10MB. Most-entangled lever
(graph-diff-across-revs, pr_diff, daemon_stateful_revs it-tests; sym-keyed
type_link migration must ride the same `(rev_id, …)` scheme). Lands with or
after L5 since the salted string embeds the coordinate string.
- Gates: graph_diff_rev + pr_diff byte-identical outputs; determinism
  oracle; `_strings` holds zero `\x01`-salted rows (fail-pre-fix: 417,127
  today).

### L5. Dense dictionary node ids (FD finding 1, diet step 5, ruling A) — −~73MB

Node dim `rel_df_node(node_id INTEGER PRIMARY KEY, file, line, col, kind,
var, fn)` WITHOUT ROWID; edges carry `node_id`; `df_node_sym`/`df_node_coord`
boundary fns reconstruct today's string byte-for-byte. Arithmetic:
- `_strings`: remove 505,627 coordinate rows. Residual 237,558 rows at the
  measured 68 B/row on-disk average ⇒ ~16.2MB table (today 78.9MB): −55MB
  (with L4's population also gone; −39MB standalone).
- df_node family: table 14.2 + autoindex 15.9 + 6 `idx_*` 25.1 = 55.2MB →
  ~6.0MB dim (274,065 × ~22B) + ~5MB for two surviving indexes (fn, file):
  −44MB gross, −18MB net of the coordinate-string share counted above.
- Unlocks `key(id)`-narrowed PKs family-wide (diet 4b) and the ref-spine
  principled id (repo/rev-scoped) — land as ONE identity change, per the
  diet's interplay (b).
- Gates: `df_node_sym` round-trip test; `_strings` holds zero `*:*:*:*`
  coordinate rows (fail-pre-fix: 505,627 today); callable_defs byte-identical
  lambda syms; dataflow closure tests; N+1 counter silent through the flush
  map build.

### L6. `.dl`-declared junction WITHOUT ROWID — measured −47.5MB on today's top five

The 4a classifier covers Rust-vouched decls only. Measured on real rows:
named_call_site −18.1MB, flow_edge −10.5MB, df_edge_src_kind −10.1MB (moot if
L3(d) turns it into a view), df_node_in_fn −8.8MB, port_reach −7.8MB; zero
NULLs exist in any candidate today. The blocker is the class-17 NULL-in-PK
incident: `.dl` named-arg partial heads can put NULL anywhere.

Nullability design sketch (what lets flow_edge take WITHOUT ROWID safely):
- (i) **Author vouch in the decl surface**: `rel flow_edge(from: text, to:
  text) total.` (syntax open — a `total` keyword beside `key()`). Semantics:
  typecheck REJECTS any partial named-arg head for a total rel (S-tier diag,
  same tier as the S6 source+join mix), and every column is NOT NULL in DDL.
  `pk_never_null` becomes `parsed-total OR rust-vouched`. Stable across
  program-set changes; the author owns the claim; the typechecker enforces
  it program-wide instead of trusting it.
- (ii) **Insert-path guard**: for any total rel, a NULL reaching the insert
  is a loud engine error naming rel and column, never an `INSERT OR IGNORE`
  silent drop. This converts the 4a incident's failure shape from silent row
  loss to a diagnosed defect.
- (iii) **Migration assistant, not enforcement**: a check-tier probe (shares
  the FD-rail plumbing from §3) lists rels whose data has zero PK NULLs and
  no partial heads in any served program — the candidates to annotate.
  Static whole-program inference alone (auto-vouch when no partial head is
  served) is rejected: serving is dynamic, and a later-served program would
  flip storage mode drop+recreate on a 17MB table mid-tick.
Verdict: (i)+(ii) land together; (iii) generates the annotation worklist
(flow_edge, df_edge_src_kind, named_call_site, df_node_in_fn, port family,
call_node, map_edge are today's zero-NULL candidates).
- Gates: named_args it-test stays green (a partial-head rel must still parse
  and pad — just not a `total` one); fail-pre-fix test = a `total` rel with
  a partial head is a typecheck error; byte receipt per converted rel;
  determinism oracle.

### L7. Index endgame — the residual pool after L3/L4/L5

The demand-aware reconcile (diet step 2) landed, but on the served sprefa
root the full discovery set is served, so 770 `idx_*` (298.8MB) survive:
demand includes everything. Post-L3 the non-derived pool is 76.3MB explicit +
49.8MB rev-idx (L4 kills the latter). Remaining candidates:
- (a) Composite indexes per Souffle's automatic index selection
  (min-chain-cover over search orders — the strata.rs:288 comment already
  cites it): replaces N single-column indexes per rel with the minimal
  chain set. The df_node_repo_rev case (3 single-col idx totaling 30.4MB on
  a table whose id is unique) collapses to zero extra B-trees once
  `key(id)` lands.
- (b) Selectivity gate: never index a column whose distinct-count/rows ratio
  is below a floor (kind-like columns; measurable from the probes in §3).
- (c) Accept the planner's word: EXPLAIN QUERY PLAN capture per served
  program (the step-2 harness exists) and keep only chosen indexes —
  already superseded by the reconcile for existence, still the receipt
  method for (a)/(b).
Projection: post-L3/L4/L5 explicit pool ~50MB → ~20-25MB. −25 to −30MB, plus
the far larger derived-side idx pool (221MB) shrinks with L3 by policy.
- Gates: EXPLAIN + timing receipt per shape before/after (correctness-safe,
  latency-risky, trivially revertable).

### L8. Wide-table splits — scip family, bounded win

scip_occurrence (8 cols, table 13.7 + autoindex 16.2MB): the FD probe says
NO clean position→symbol dependency (1,175 violations), so no dimension
split. The remaining lever is full-row WITHOUT ROWID (kills the 16.2MB
autoindex at the cost of PK-locator growth in any future secondary index —
it has none today). Requires lifting the 4a classifier's 2..=4 column cap
for vouched, zero-secondary-index rels: scip_occurrence + scip_binding
autoindexes = 27.0MB → ~0. −27MB. Low risk (scip rels are Rust-vouched
already; scip family bulk-loads from index.scip).
- Gates: byte receipt; scip it-tests; determinism oracle.

### L9. `__src` retirement — −~8.7MB

Measured 1.15 B/row × 7.56M rows. The phantom-extract RCA (D1) already made
`__src` a hazard once (arity mismatch in the diff). Storage-key-audit P0
compaction: drop the column where no writer populates it (245,129/245,129
blank on df_node per the diet's measurement). Rides whichever DDL pass
touches each table anyway (L5/L6) — zero standalone migrations.

### L10. VACUUM / auto_vacuum policy — one-time −78MB + anti-regrowth

Measured slack: live 971.5 vs vacuumed 893.4 (8%); smashy-cache freelist 40%.
auto_vacuum=0 everywhere today; per-tick WAL is already
checkpoint-truncated (tick.rs TickPragmas). Candidates:
- (a) `auto_vacuum=FULL`: moves pages on every commit, fragments B-trees,
  taxes the hot tick path — rejected.
- (b) `auto_vacuum=INCREMENTAL` + `incremental_vacuum(N)` on the idle timer:
  bounded pauses, keeps freelist near zero, no full rewrite. Requires a
  one-time VACUUM to arm on existing dbs.
- (c) Scheduled `VACUUM INTO` + atomic swap on daemon idle: cleanest file,
  needs 2x transient disk and a swap dance under the engine lock.
- (d) Event-driven plain VACUUM after schema drift or a mass drop (the index
  reconcile and L3 evictions create exactly the 254MB-freelist moments the
  step-2 receipt showed).
Verdict: (b) as steady state + (d) after drift events; (c) only for the
one-time migration landing of L3-L6. WAL sizing: root-db WAL is healthy
(45KB); jobs.sqlite/daemon-home WAL at 3.3x db size warrants
`journal_size_limit` on those two opens (jobq's own connections) — minor
bytes, hygiene only.

### L11. Page size — rejected, measured

8K/16K both grow the file (+1.8MB / +14.8MB): row payloads here are small
int tuples; bigger pages waste leaf space. No change.

### L12. Compression — rejected for the live db, candidates analyzed

Entropy exists (zstd -19 = 23% of file) but the win overlaps the schema
levers (the redundancy zstd finds IS the autoindex/derived/rev duplication
being removed structurally). Candidates:
- (a) sqlite-zstd (row-level extension): per-table dictionaries, transparent
  reads; adds a loadable extension + dictionary lifecycle to every open
  path, taxes every read on the hot tick, and rusqlite bundling of
  third-party extensions complicates the db-seam migration. Deferred:
  revisit only if post-L3/L5 tables still show >2x zstd headroom.
- (b) ZIPVFS: proprietary license — out.
- (c) Page-size tuning: measured negative (L11).
- (d) Column encoding (dense ids, dimension tables): IS the adopted path —
  L4/L5 are compression by schema.
- (e) Whole-file zstd for archival/backup copies only (`VACUUM INTO` + zstd
  = 205MB cold artifact): fine as an ops practice, not an engine feature.

## 6. Order and stacked projection (served sprefa root)

Baseline: post-VACUUM 893.4MB (live file 971.5MB).

| step | lever | Δ (arithmetic) | running total |
|---|---|---:|---:|
| 0 | baseline, post-VACUUM | — | 893.4 |
| 1 | L3 derived → sidecar/demand (measured drop) | −507.1 | 386.2 (main db at rest) |
| 2 | L4 rev normalization (116.6→~10, minus 21.3 autoindex already inside step-1's drop? no — rev pool measured in the 386.2 floor: 105.9→~9) | −97 | 289.2 |
| 3 | L5 dense ids (`_strings` 71.2→16.2 incl. L4's salted rows; df_node family −18 net) | −73 | 216.2 |
| 4 | L7 index residual (post-L4/L5 explicit pool ~50→~22) | −28 | 188.2 |
| 5 | L8 scip WITHOUT ROWID (cap lift) | −27 | 161.2 |
| 6 | L6 remaining `.dl` junctions still materialized in main (named_call_site et al. are derived → sidecar; source-side residue) | −5 | 156.2 |
| 7 | L9 `__src` on surviving ~2.5M source rows | −3 | ~153 |

**Main-db projected minimum: ~150-165MB** (≈21x the 7.3MB corpus, inside
CodeQL's 5-20x band at its edge — from 127x live today).

Derived sidecar at steady state (all served, nothing evicted): 547.8MB today
→ L6 junction WITHOUT ROWID (−~155MB of its 170.9MB autoindex pool at the
measured ~50% ratios) → L7 on its 221MB idx pool (composite + demand ≈
−120MB) → L3(d) views for the join-copies (df_edge_src_kind 36.1, hop/len
layers ~80 if timing receipts allow) ⇒ **~150-250MB when fully hot, 0 at
rest**, and the 216MB port share exists only while flow-panel is served.

| db | today (file+WAL) | endgame |
|---|---:|---:|
| sprefa root | 971.5 + 0.05 | ~155 main + 0-250 sidecar (demand-scaled) |
| sprefa cache.db twin | 974.5 | 0 (L2) |
| smashy root + cache twin | 44.2 + 14.6 + 83.8 | ~25 one db (same levers, dominant _strings 5.8/13.2MB + autoindex ~13MB shrink proportionally) |
| instant root | 9.2 + 4.1 | ~6 |
| orphan root | 21.4 | 0 (L1) |
| daemon home logs/dbs (perf.jsonl 65.7, daemon.log 13.5, invocations 20.7, jobs+db+WAL ~11.6) | ~113 | ~35 (rotation caps; invocations retention already bounded) |
| **machine total** | **~2,280MB** | **~220MB at rest, ~470MB fully hot** |

## 7. Mandatory gates (every lever, no exceptions)

1. **Byte receipt**: `dbstat` per affected object + post-VACUUM file bytes,
   before/after, on the real corpus (4a receipt format).
2. **Query byte-equivalence**: `_txt`-view TSV dumps, ORDER BY every column,
   SHA1 equal before/after (proven method from the 4a receipt).
3. **Determinism**: `extraction_is_deterministic_across_identical_rebuilds`
   digesting logical rows (never physical ids — pin this before L5).
4. **Crash-resume**: kill mid-rebuild/mid-migration; `_derived_complete` and
   the cold-chunk seam recover without a full rebuild; new test per lever
   that moves bytes across files (L2, L3).
5. Index changes additionally carry EXPLAIN QUERY PLAN + wall-time receipts.
6. Fail-pre-fix per the failure-ledger pipeline for each new rail
   (`_strings` coordinate-row zero-count, salted-row zero-count, autoindex
   byte ceiling, FD/key mismatch probe, ratio verdict line at the diet's
   ruling-E ceiling).

## 8. Prerequisite arcs and sequencing

- **db-seam migration** (in flight): L2 (open paths) and L3 (ATTACH routing)
  land on the Db seam, after it merges. Do not fork the open path twice.
- **ref-spine**: L5's node identity carries repo/rev — one identity change
  shared with the class-2 residual, per diet interplay (b).
- **decomposition-normalization file splits**: never interleave with L5's
  typegraph.rs/dataflow.rs edits (modify/delete conflict rule).
- **scheduler write-budget**: L3 rebuild bursts ride it; eviction without a
  write budget recreates the 07-18 write-storm shape.
- **obs-logging**: the ratio verdict line and perf.jsonl rotation both land
  as tracing subscribers there (infra law: no parallel bespoke pipeline).
- Sequence: L1 (now) → L2 (after db-seam) → L6(i,ii) + L8 (independent DDL,
  early receipts) → L3 (the whale) → L5+L4 (identity wave, one change) →
  L7 → L9/L10 ride along. L11/L12 are closed as measured rejections.
