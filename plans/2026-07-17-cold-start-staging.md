# Cold-start staging: extract one family-shard per tick, not the whole corpus at once

Status: IMPLEMENTED 2026-07-18 (branch `cold-stage`). Riding the existing `src/jobq/`
queue as greenlit; the D1–D6 proposals below were accepted as the decisions taken.
Code: `src/engine/cold_stage.rs` (node table + seed/run/complete/resume), the staging
gate in `src/engine/tick.rs::tick_report`, `JobKind::ColdExtract` + `JobRow::cold_extract`
in `src/jobq/mod.rs`, and the daemon wiring in `src/daemon.rs` (single-flight cold
worker, `ServedRoot::run_cold_node`, `cold_start_pending` on status/ping, `poll_idle`
guard). Tests: `tests/it/cold_stage.rs`.

## Decisions taken (D1–D6)

- **D1 — enable/default:** staged path taken ONLY when `Engine::poll_loop` is set (the
  daemon). One-shot `--no-daemon` keeps the inline cold tick and never touches
  `_cold_node`. `DL_NO_COLD_STAGE=1` disables staging even under the daemon.
- **D2 — shard count/domain:** `cold_shard_count` carries `ceil(files/200)` capped at
  16 (`DL_COLD_SHARD_FILES` overrides the 200), behind a per-family `shardable_cold()`
  gate. Hash-by-`blake3(path)` slicing is the Shape-B follow-up.
- **D3 — hot subset inline:** deferred (pure staging). Hot-priority is a follow-up.
- **D4 — which families shard:** in this arc EVERY used family runs WHOLESALE as one
  node (`n_shards = 1`; `shardable_cold()` is false for all). Per-file sharding of an
  individual family is deferred: the type/call/dataflow resolvers run a corpus-global
  name→def barrier and the `extract:<family>` skip digest is per-rev (not per-shard),
  so a per-file slice cannot be made digest-consistent without new infra — and a wrong
  slice would poison the digest skip and break the inline-equivalence contract. This is
  exactly the plan's sanctioned "a wholesale family is just a family with N_SHARDS=1".
  Cross-shard call-edge resolution is therefore a non-issue this arc; the completion
  Tick is the single correctness guarantee (it re-runs the normal blank-slate path,
  whose per-family digest skips make the already-extracted families cheap). `node`
  (CST) and `spine` are NOT staged (they lack a pre-walk skip digest and would re-run
  on the completion tick anyway; `spine` also needs `node` first) — they run on the
  completion tick, same as inline.
- **D5 — query semantics during cold start:** serve partial. `cold_start_pending` is
  exposed on the status + ping RPCs; `poll_idle` returns not-idle (await-settle blocks)
  while cold-pending; `dl daemon jobs` lists the `cold_extract` rows for free.
- **D6 — relation to J3:** `ColdExtract` is a separate `JobKind` from the reserved
  `DeriveStratum` (J3); both ride the same node-table shape.

Written 2026-07-17.

Question from the user: on a cold db, one tick extracts every used family over the
whole corpus in a single synchronous body. Can that be staged/queued across ticks
instead, so a blank-slate boot never does all the work in one lock-held pass?

Short answer: yes, and most of the machinery already exists. The durable job queue
(`src/jobq/`, arc J1) and the per-component completion marker (`_derived_complete`)
are the two hard parts, and both are built. What is missing is (a) a persisted
`(family, shard)` node table that records which slices of the corpus have been
extracted, and (b) a blank-slate branch in `tick_report` that SEEDS that table +
the queue instead of running every family inline. This plan specifies both and
leaves the policy knobs to the user.

## Table of contents
1. Current cold-tick flow (what runs inline today)
2. What the intensity caps already bound
3. Prior art (build-vs-buy survey)
4. Two staging shapes
5. Type signatures
6. Pseudo-code
7. Instance lifetimes
8. Storage layout, read/write sequence, uniqueness
9. Interaction with `_derived_complete`, crash recovery, the poll loop
10. Open user decisions

---

## 1. Current cold-tick flow

`Engine::tick_report` (`src/engine/tick.rs:170`, ARCH `engine/40-tick`) runs one
reactive tick. On a blank-slate db every used family runs its full-corpus refresh
inline, in this order (line numbers are the current tree):

| Phase | Call | Cost on cold db |
|---|---|---|
| declare | `ensure_meta` + `declare_all` | cheap |
| reconcile | `reconcile_sources` (tick.rs:416) | walks + stats the WHOLE tree, extracts every source rule's facts |
| builtin | `refresh_builtin_rels` (tick.rs:442) | repo/rev/content/file projections |
| pre-extract | scip `RelKind::refresh` (tick.rs:485) | loads the entire SCIP index |
| prime | `prime_analysis_bundles` (tick.rs:495) | parses every Rust file once for type/call/df |
| extract pre-node | `extract_families_pre_node` (tick.rs:496) | module, type, call, dataflow, doc — each `refresh()` re-reads the whole corpus |
| node | `refresh_node_rels` (tick.rs:510) | full CST walk, writes `_strings`/`_where_bytes` |
| extract post-node | `extract_families_post_node` (tick.rs:516) | spine, corpus-gated |
| other RelKinds | git/analysis/propose/embed (tick.rs:547) | each full refresh |
| derived | `rebuild_derived` under `need_full` (tick.rs:745) | blank-slate = `full_reason="blank-slate"` (tick.rs:702), wipes + fixpoints every derived rel |

Every extract family is gated only by `.used(prog)` (`ExtractFamily::used`); a family
that is used runs its whole-corpus refresh with no per-tick budget. On a blank slate
`prior_der_digest.is_none()` so `need_full` is unconditionally true and the derived
layer rebuilds in full. The result is exactly the user's description: one tick, every
used family, whole corpus, all inside the engine mutex.

The digest skips that make WARM ticks cheap (perf gap A: `extract:<family>` input
digest for type/call/dataflow/doc; the `src:`/`drv:` per-rel rule-shape digests) do
nothing on a blank slate because there is no prior digest to match.

## 2. What the intensity caps already bound

`apply_daemon_budget` (`src/daemon.rs:1062`) and `apply_process_budget`
(`src/daemon.rs:1027`) already cap how HARD the cold tick hits the machine, but not
how LONG it holds the lock or how much it does per tick:

- rayon global pool = `daemon_thread_count` = `max(2, cores/4)` (daemon.rs:1006) — the "width 2" floor.
- macOS QoS `UTILITY` on the calling thread, inherited by rayon workers (daemon.rs:1034).
- `setpriority` nice `+10` on every unix (daemon.rs:1040).
- `setiopolicy_np(DISK, PROCESS, THROTTLE)` — bulk db writes land in the background I/O tier (daemon.rs:1058).
- daemon-only: `PRIO_DARWIN_BG` resource tier (daemon.rs:1082) — leftover CPU, throttled disk, deprioritized network.
- `BulkRebuildIo::enter` (tick.rs:751) drops fsync + autocheckpoint for the derived rebuild span.

These bound CPU share and disk tier. They do NOT bound wall time of a single tick,
peak memory of a single family's row set, or lock-hold duration. Staging is the
orthogonal lever: cap the WORK PER TICK so each tick is short, releasable, and
resumable, and let the caps govern the tempo of the many small ticks. The two
compose — staging turns "one 72s lock-held tick" into "N short ticks, each already
throttled".

## 3. Prior art (build-vs-buy survey)

Standing law: no bespoke queue/scheduler without a written candidate analysis first.
Two layers need deciding separately: the DURABLE QUEUE, and the STAGING POLICY.

### 3a. The durable queue is already resolved in-house

`src/jobq/mod.rs` (arc J1) is a durable SQLite job queue that was ALREADY built
against a candidate survey — its own doc comments cite `effectum` (startup
`expires_at` recovery) and `apalis` (`reenqueue_orphaned` live interval) as the prior
art it borrowed from (jobq/mod.rs:44-56). It has:

- `_job(key PRIMARY KEY, kind, root, arg, priority, state, dirty, cancelled, attempts, run_at, enqueued_at, started_at, finished_at, last_error)` + index `_job_ready(state, run_at, priority)` (jobq/mod.rs:67).
- `claim` = `SELECT ... WHERE state='pending' AND cancelled=0 AND run_at<=now ORDER BY priority DESC, run_at ASC LIMIT 1` then `UPDATE ... state='running'` (jobq/mod.rs:309). Single-writer SQLite makes this atomic without `SKIP LOCKED`.
- `finish` -> done / repending (coalesce or backoff) / parked (jobq/mod.rs:336).
- `sweep` = lease reclaim (`LEASE_SECS=900`) + done-row GC, set-ops no per-row loop (jobq/mod.rs:442).
- `reset_running_on_boot` -> every `running` row back to `pending` (jobq/mod.rs:425): dead-PROCESS recovery.
- coalescing via `key` UPSERT with `arg` union (jobq/mod.rs:240).
- `JobKind { Tick, SinkDrain }` today; the doc explicitly names **`DeriveStratum` (per-stratum derive jobs) as J3**, a planned follow-up (jobq/mod.rs:24, 124).

The external-crate re-survey (rechecked for this plan) confirms not pulling a new
crate: `apalis`+`apalis-sqlite` (MIT) is the closest off-the-shelf SQLite queue but
drags `tokio`+`sqlx` and models jobs as opaque async tasks, not a `(family, shard)`
digest node; `sqlxmq`/`underway` are Postgres-only; `pgmq` is a PG extension;
`aide-de-camp-sqlite`'s own docs warn SQLite lacks the row-locking its design wants.
Every crate assumes multi-worker `SELECT ... FOR UPDATE SKIP LOCKED` contention we do
not have as a single writer. Verdict: the pattern (claimed-at cursor + orphan reaper)
transfers, the dependency does not — and we already implemented the pattern in
`jobq`. Cold-start staging should ride `jobq`, adding a `JobKind`, not a queue.

### 3b. The staging POLICY — what to borrow

| System | Mechanism | Persistence | Borrow / reject |
|---|---|---|---|
| rust-analyzer | `parallel_prime_caches`: after VFS load, eagerly force expensive salsa queries on a bounded pool, interruptible by edits; opened files prioritized | none (persistent cache is open issue #4712) | BORROW the shape (prioritize hot subset, interruptible background sweep on width-2 pool). REJECT the no-persistence posture. |
| Salsa | demand-pull; nothing computes until a query pulls it. "Cold" = "nothing memoized". Durability tiers = revalidation-skip. Per-query LRU eviction. | in-RAM only | REJECT pull-as-driver (a datalog fixpoint is eager to closure, not lazily pullable). BORROW durability tiers as STAGE ORDER (vendored/stdlib families extracted once, workspace first). |
| Bazel Skyframe / Buck2 DICE | `(family, shard)` = a `SkyFunction`/action keyed by input digest; bottom-up dirty/clean marking; a dirty node is resurrected without re-running if inputs re-digest equal; the persisted node graph IS the resume cursor | in-RAM server (SIGKILL -> cold); action cache is the durable resume point | STRONGEST match. BORROW the node-graph-as-cursor, re-hosted in SQLite so `kill -9` resumes. Our `src:`/`drv:` digests already give clean-with-same-value. |
| differential dataflow | cold ingestion in coarse frontier-delimited BATCHES ("rounds of ten cost little more than one"); never row-by-row | trace batches | BORROW coarse shard batching (a few large shards, per-shard watermark). Aligns with our own N+1 ban. |
| apalis / effectum | `claimed_at` + `attempts` + orphan reaper schema | SQLite/PG | ALREADY borrowed into `jobq`. |

Synthesis: this is **Skyframe's persisted dirty/clean node graph, re-hosted in SQLite
instead of a RAM server, staged like rust-analyzer's `prime_caches`, tiered by Salsa
durability, riding the `jobq` queue we already vetted against apalis/effectum**. No
new async runtime, no new queue crate.

Sources: RA PR #21828 (prime-caches width), RA #4712 (persistent cache), Durable
Incrementality blog (2023-07-24), salsa RFC0004 (LRU), Bazel Skyframe reference,
Buck2 DICE docs, differential-dataflow ch5.3, apalis-sql / apalis-sqlite, the
SKIP-LOCKED job-queue pattern writeups.

## 4. Two staging shapes

### Shape A — family-per-tick queue (coarse, ~10 nodes)

One node per used family. A blank-slate tick seeds one `ColdExtract{family}` job per
used family into `jobq`, then returns having extracted NOTHING (or only the hot
subset, see decision D3). Each subsequent poll cycle claims one job, runs that ONE
family's whole-corpus refresh, marks it done. Derived rebuild waits until every
family node is done (a gate).

- Pros: tiny table (~10 rows), trivial cursor, no sharding logic, reuses each
  family's existing whole-corpus `refresh()` unchanged.
- Cons: a single family (call graph over a 600-file corpus) is still one big
  lock-held tick — it lowers the count of heavy ticks from "all families" to "one
  family", but the heaviest family is unshardable at this granularity. Memory peak
  per tick is one family's full row set.

### Shape B — file-shard slices (fine, family x shard nodes)

Node = `(family, shard)` where a shard is a deterministic slice of the file set
(e.g. `blake3(path) % N_SHARDS`, or contiguous runs of the `_file` rowid order that
`reconcile_sources` already sorts deterministically). A blank-slate tick seeds
`family_count * N_SHARDS` nodes. Each job runs one family over one shard's files.

- Pros: bounded work AND bounded memory per tick regardless of corpus size; the
  heavy families shard evenly; differential's coarse-batch guidance (a few large
  shards, not per-file) keeps us clear of the N+1 ban.
- Cons: every family's `refresh()` must gain a "restrict to this file subset"
  parameter. Some already have the seam: `refresh_module_rels_for_paths` /
  `refresh_node_rels_delta(paths)` / the module `_for_revs`/`_for_paths` variants
  named in `extract_family.rs`. Others (spine, doc) are wholesale-rebuild only and
  need a shard-scoped variant, or opt out of sharding (run as one shard). Cross-shard
  resolution (call-graph edges whose callee is in another shard) must reconcile at
  the end — the same "resolve within a single rev" corner `refresh_module_rels`
  already documents (extract/mod.rs:715).

Recommendation to put to the user: **Shape B for the extract families that already
have a `_for_paths` seam, Shape A (whole-family, one shard) for the wholesale-rebuild
families (spine, module full).** The node table is identical for both; a wholesale
family is just a family with `N_SHARDS = 1`. This is the Skyframe node graph with a
per-family shard count.

## 5. Type signatures

```rust
// src/jobq/mod.rs — extend the existing enum, do NOT add a new queue.
pub(crate) enum JobKind {
    Tick,
    SinkDrain,
    ColdExtract,   // NEW: run one (family, shard) node's extraction
}

// JobRow constructor mirroring JobRow::tick / ::sink_drain (jobq/mod.rs:169).
impl JobRow {
    // key = format!("cold:{root_id}:{family}:{shard}") — one row per node,
    // so a re-seed UPSERT is idempotent and per-node work serializes.
    pub fn cold_extract(root_id: &str, family: &str, shard: u32) -> JobRow;
}

// src/engine/cold_stage.rs (NEW module) — the node table + policy, owned by Engine.
pub(crate) struct ColdStage;

impl Engine {
    // Is this a blank slate we should stage rather than run inline?
    // True when the corpus is non-empty AND no `_cold_node` rows are `done`
    // AND staging is enabled (see decision D1). Cheap, O(1) indexed count.
    pub(crate) fn cold_start_pending(&self, prog: &Program) -> Result<bool>;

    // Seed one _cold_node row per (used family, shard). Idempotent: an
    // INSERT OR IGNORE keyed on (family, shard). Called once on the first
    // blank-slate tick. Returns the JobRows to enqueue (caller enqueues so
    // the engine never depends on the daemon Shared).
    pub(crate) fn seed_cold_nodes(&self, prog: &Program) -> Result<Vec<crate::jobq::JobRow>>;

    // Run ONE node: dispatch to the family's shard-scoped refresh, mark the
    // row done + store its input digest. Returns whether rows moved (feeds
    // the derived-rebuild gate). Runs under the engine mutex like any tick.
    pub(crate) fn run_cold_node(&mut self, family: &str, shard: u32) -> Result<bool>;

    // Every used family's node is `done`? Gate for the first full derived
    // rebuild. O(1): COUNT(*) WHERE state != 'done' == 0.
    pub(crate) fn cold_nodes_complete(&self, prog: &Program) -> Result<bool>;
}

// The shard-scoped refresh seam each ExtractFamily must expose (some exist,
// some are new — see Shape B). `shard = None` means "whole corpus, one shard".
pub trait ExtractFamily {
    fn shardable(&self) -> bool { false }   // default: wholesale, N_SHARDS=1
    fn refresh_shard(&self, eng: &mut Engine, shard: Option<u32>, n_shards: u32)
        -> Result<RefreshOutcome>;          // default delegates to refresh()
}
```

## 6. Pseudo-code

```rust
// In Engine::tick_report, BEFORE the inline extract-family fan-out (tick.rs:475).
// fn tick_report(&mut self, prog, quiet) -> Result<TickReport> {
//     ... declare, reconcile_sources (source facts are cheap-ish and needed as
//         the file set the shards partition — keep them inline) ...
//     if self.cold_start_pending(prog)? {
//         let jobs = self.seed_cold_nodes(prog)?;   // INSERT OR IGNORE _cold_node
//         // Return the jobs to the daemon shell to enqueue (D3: optionally also
//         // run the hot-subset nodes inline here so a one-shot `dl q` on the
//         // exact opened file is not blank). Do NOT run the extract families
//         // inline; do NOT rebuild_derived yet (derived reads incomplete facts).
//         report.cold_staged = Some(jobs);
//         return Ok(report);   // short tick, lock released
//     }
//     ... normal inline path (existing) ...
// }

// The daemon worker, on claiming a ColdExtract job (daemon_shell/jobs.rs):
// fn run_cold_job(root, family, shard) {
//     let moved = { let mut eng = lock(&root.eng);
//                   eng.run_cold_node(family, shard)? };   // one family/shard
//     // run_cold_node marks _cold_node[family,shard].state='done' + input digest
//     if lock(&root.eng).cold_nodes_complete(&prog)? {
//         // last node: enqueue ONE Tick job to run the first full derived
//         // rebuild over the now-complete fact base (reuses the need_full path).
//         shared.enqueue(JobRow::tick(root_id, &[]))?;
//     } else {
//         // more nodes pending: the doorbell already rang on seed; the worker
//         // loops and claims the next. run_at=0 so they are all ready; the
//         // budget (QoS/IOPOL/BG tier) governs tempo, one node per claim.
//     }
// }

// run_cold_node dispatch:
// fn run_cold_node(&mut self, family, shard) -> Result<bool> {
//     let fam = extract_family_by_name(family);
//     let n = self.cold_shard_count(fam);           // fam.shardable() ? N_SHARDS : 1
//     let moved = fam.refresh_shard(self, if n==1 {None} else {Some(shard)}, n)?.moved();
//     let digest = self.cold_node_input_digest(family, shard)?;  // file-subset digest
//     self.mark_cold_node_done(family, shard, digest)?;          // one UPSERT
//     Ok(moved)
// }
```

## 7. Instance lifetimes

- `ColdStage` — zero-state marker (like the `ExtractFamily` impls); all state lives
  in `_cold_node`. No per-process instance to keep warm.
- `_cold_node` rows — durable in the corpus db (`roots/<key>/db.sqlite`), one row per
  `(family, shard)`, born on the first blank-slate seed, `done` after their node
  runs. GC'd only when the whole cold-start completes OR a program-shape change
  retires a family (see uniqueness). They persist across process death BY DESIGN —
  that is the resume cursor.
- `_job` rows (`ColdExtract` kind) — durable in `<home>/jobs.sqlite`, lifetime owned
  by `jobq`: `pending` -> `running` -> `done`, GC'd by `sweep` after
  `DONE_RETAIN_SECS`. `reset_running_on_boot` returns a mid-flight node to `pending`;
  `_cold_node` for that node is still not `done`, so re-running is a no-op-safe redo.
- The seeded `JobRow` vec returned from `seed_cold_nodes` — transient, lives only from
  tick return to the daemon shell's `enqueue` loop.

The two durable tables are deliberately split the same way `jobq` split from the
engine db (jobq/mod.rs:14): `_cold_node` is engine FACT state (which slices are
extracted) so it lives in the corpus db next to the digests it mirrors; `_job` is
CONTROL state so it lives in `jobs.sqlite`. A `ColdExtract` `_job` row is the
transient "please run node X"; the `_cold_node` row is the durable "node X is done".

## 8. Storage layout, read/write sequence, uniqueness

### Layout

```sql
-- corpus db (roots/<key>/db.sqlite), alongside _reldigest / _derived_complete
CREATE TABLE IF NOT EXISTS _cold_node (
  family      TEXT NOT NULL,
  shard       INTEGER NOT NULL,
  n_shards    INTEGER NOT NULL,
  state       TEXT NOT NULL DEFAULT 'pending',   -- 'pending' | 'done'
  input_digest BLOB,                             -- blake3 of the shard's file subset
  done_at     INTEGER,
  PRIMARY KEY (family, shard)
);
```

`_job` needs no schema change — the existing columns carry a `ColdExtract` row; `arg`
holds `{"family": "...", "shard": N}`.

### Read/write sequence (one cold start)

1. First blank-slate tick: `reconcile_sources` writes `_file` (the shard domain).
   `cold_start_pending` reads `COUNT(_file) > 0 AND COUNT(_cold_node WHERE state='done') == 0`.
2. `seed_cold_nodes`: for each used family, `INSERT OR IGNORE _cold_node(family, shard, n_shards, 'pending')` for `shard in 0..n_shards`. One batched `insert_rows` (N+1 law). Returns one `JobRow::cold_extract` per row.
3. Daemon shell enqueues each JobRow (coalesced by key; a re-seed is a no-op UPSERT).
4. Worker claims a `ColdExtract` job -> `run_cold_node` -> `refresh_shard` writes that
   family's rows for that shard -> `UPDATE _cold_node SET state='done', input_digest=?, done_at=?`.
5. `finish(Done)` on the `_job` row.
6. When `cold_nodes_complete`, the worker enqueues one `Tick` job; that tick sees
   `cold_start_pending == false` (nodes done) and runs the normal path, whose
   `need_full` (blank-slate: `prior_der_digest.is_none()`) does the first full
   `rebuild_derived` over the now-complete fact base.

### Uniqueness

- `_cold_node` PK `(family, shard)` — one node per slice; re-seed is idempotent.
- `_job` key `cold:{root}:{family}:{shard}` — one queue row per node; coalesces.
- A subsequent NORMAL tick never re-enters staging: `cold_start_pending` is false the
  moment any node is `done` AND stays false once all are done (the `_cold_node` rows
  remain `done`). A program-shape change that adds a family: `seed_cold_nodes` inserts
  only the NEW family's nodes (INSERT OR IGNORE leaves existing done rows), and the
  normal warm path (digest-driven) handles the already-extracted families. A family
  DROPPED from the program: its `_cold_node` rows are dead but harmless; a
  `declare_all`-time sweep (mirroring the `_derived_complete` cleanup at
  declare.rs:161) deletes `_cold_node` rows for families no longer used.

## 9. Interaction with `_derived_complete`, crash recovery, poll loop

### `_derived_complete` (the crash-window marker)

`rebuild_derived` marks completion per component (`mark_derived_complete` /
`unmark_derived_complete`, derive.rs:394-430); `derived_incomplete_rels`
(derive.rs:2243, reads `_derived_complete`) drives `need_full` (tick.rs:692). Staging
must NOT let the derived layer rebuild before facts are complete, or every
still-empty derived rel reads as `incomplete` and forces churn. The gate in step 6
above is exactly this: no `rebuild_derived` until `cold_nodes_complete`. Until then
the derived rels are legitimately empty and unmarked — the same state as a fresh db
before its first tick, which the existing `need_full`/blank-slate logic already
tolerates. The FIRST post-staging tick does the single full derived rebuild, and
`_derived_complete` gets marked per component then, unchanged.

### Crash recovery (SIGKILL mid-cold-start)

This is where the SQLite cursor earns its place. On `kill -9` mid-stage:

- `_cold_node`: nodes that finished are `done` and stay done. A node killed mid-run is
  still `pending` (its `state='done'` UPDATE is in the same transaction as its row
  writes — either both land or neither, so a half-extracted family is never marked
  done). Next boot re-runs only the not-done nodes.
- `_job`: `reset_running_on_boot` (jobq/mod.rs:425) returns the killed `running`
  `ColdExtract` job to `pending`; the worker re-claims it; `run_cold_node` re-runs
  the still-`pending` node. `refresh_shard` is a wholesale wipe+repopulate of that
  shard's rows (the family refreshers already are), so a redo is idempotent.
- No `_derived_complete` state was written (the gate held), so there is no
  half-derived layer to reconcile — the exact hazard the crash-window arc closed for
  the inline path, avoided here by never starting the derived rebuild until facts are
  complete.

This is strictly SAFER than today's inline cold tick, which does everything in one
transaction-spanning body where a SIGKILL leaves the deferred-digest baselines the
only recovery signal. Staging makes the recovery unit a single family-shard.

### Poll loop

`poll_idle` (daemon.rs:513) currently returns "nothing to do" when settled + no
pending effects + no dirty tick. Cold-start nodes are queue work, so the DISPATCHER
(not `poll_idle`) drives them: seeding rings `job_notify` (daemon.rs:253), a worker
wakes and drains `ColdExtract` jobs one per claim. `poll_idle` stays untouched — the
`@async`/clock cadence is orthogonal. One addition: `poll_idle` must return `false`
(not idle) while any `_cold_node` is `pending`, so `dl daemon await-settle` blocks
until cold start finishes. The tempo is governed by the existing budget (QoS/IOPOL/BG
tier + rayon width 2) — one node per claim, each node already throttled, so the whole
cold start runs in the background tier instead of one machine-seizing burst. `dl
daemon jobs` already lists queue rows, so cold-start progress is visible for free
(the rust-analyzer "Indexing n/m" signal, via the existing listing).

## 10. Open user decisions

- **D1 (enable/default):** staged cold start on by default, or behind
  `DL_COLD_STAGE=1` until proven? One-shot `dl --no-daemon` has no worker to drain the
  queue — staging only makes sense under the daemon. Proposal: staged path taken ONLY
  when `self.poll_loop` (daemon) is set; `--no-daemon` keeps the inline cold tick.
- **D2 (shard count / domain):** `N_SHARDS` fixed (e.g. 8), or scaled to corpus size
  (`ceil(file_count / TARGET_FILES_PER_SHARD)`)? And shard by `blake3(path) % N` (even)
  or by contiguous `_file` rowid runs (locality, reuses reconcile's deterministic
  order)? Proposal: `ceil(files / 200)` capped at 16, hash-sharded for evenness.
- **D3 (hot subset inline):** should the seeding tick also run the shard(s) covering
  a caller-named hot file set inline (rust-analyzer "opened files first"), so an
  interactive `dl q` on a specific file is not blank during cold start? Needs a "hot
  paths" input the tick does not have today. Proposal: defer — start with pure
  staging, add hot-subset priority (a higher `_job.priority`) as a follow-up.
- **D4 (which families shard):** confirm the family list that gets a real
  `refresh_shard` (module `_for_paths`, node `_delta`, type/call/dataflow via their
  per-file caches) vs. runs wholesale as `N_SHARDS=1` (spine, doc). Cross-shard
  edge resolution (call callee in another shard) — resolve at the completion gate
  (step 6) or per-shard-with-fixup? Proposal: wholesale for spine/doc; resolve
  cross-shard call edges in the completion Tick, not per shard.
- **D5 (query semantics during cold start):** a read RPC against a mid-cold-start db
  sees partial facts. Block (`await-settle`) or serve-partial-with-a-flag? Proposal:
  serve partial; `dl daemon jobs` / a `cold_start_pending` flag on the status RPC
  tells the client it is warming.
- **D6 (relation to J3):** the `jobq` doc already reserves `DeriveStratum` (J3) for
  per-stratum derive jobs. Cold-start staging is the fact-extraction twin. Decide
  whether `ColdExtract` and `DeriveStratum` are one arc (a general "staged work"
  JobKind carrying a phase) or two. Proposal: keep them separate kinds; both ride the
  same `_cold_node`-style node table shape.
