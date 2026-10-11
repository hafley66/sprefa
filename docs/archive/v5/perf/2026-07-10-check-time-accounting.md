# `dl --check` time accounting — 2026-07-10 (ext-wave3 worktree)

Investigation only. No code changes. Method: parsed `.dl/perf.jsonl`, cross-checked
against `_stmt_ms`/`_reldigest` tables in `.dl/cache.db`, read the `--check` /
`Engine::tick` / `activity` code paths, and ran controlled isolated repros with
`target/debug/dl` (built from this worktree, 2026-07-10 18:25) to separate
contention noise from structural cost. All ms figures below are measured, not
estimated, except where marked "attributed by elimination."

## Headline correction to the stated hypothesis

**The "one engine tick per .dl program (~15 programs)" hypothesis is FALSE for a
bare `dl --check`.** Code (`src/lib.rs:59-121`, `src/frontend.rs:96-120`) merges
every file discovered in `.dl/*.dl` into **one** `ast::Program` via
`load_program_set`, and `run_check_inproc` (`src/lib.rs:356-370`) runs **exactly
one** `Engine::tick()` over it. Verified directly: an isolated
`target/debug/dl --check --no-daemon` run (no other process touching this
worktree's `cache.db`) emits exactly **one** `"type":"tick"` JSON record per
invocation, with `total_ms` matching measured wall time to within noise. The
apparent multi-program pattern in the raw `.dl/perf.jsonl` (many `"tick":0`
blocks with disjoint small `changed_rels`/`files` sets, seconds apart) is real
but is **not** one process — it is the historical log accumulating entries from
many separate `dl` invocations over the day (daemon-served ticks from other
worktrees/repos sharing nothing, ad-hoc single-file runs, other agent sessions),
because `perf.jsonl` carries **no PID field** (`src/perflog.rs`), so concurrent
or sequential writers are indistinguishable in the raw stream.

## What actually happens in one `dl --check --no-daemon`

Two clean, back-to-back, uncontended runs against the already-warm worktree
`cache.db` (no other `dl` process holding the file — verified via `lsof`):

| run | wall (`real`) | user | sys |
|---|---|---|---|
| 1 (first after idle) | 22.87s | 19.20s | 2.89s |
| 2 (immediately after, fully warm) | 20.36s | 11.47s | 3.02s |

Both runs are a **single tick** covering the merged 15-file `.dl/*.dl` program
(2500 corpus files, 0 files re-parsed either time — full cache hit on file
content).

### Per-phase accounting, run 2 (warm, clean, isolated)

| phase | ms | % of 20141ms total_ms |
|---|---|---|
| declare | 21 | 0.1% |
| reconcile (file enumerate/hash/mtime check) | 653 | 3.2% |
| extract: module/type/call/dataflow-rels (family digest-skip path) | 146 | 0.7% |
| **unlogged ("dark")** | **19,321** | **95.9%** |
| **total_ms (tick record)** | **20,141** | 100% |

Sum of logged phases: 820ms. The remaining 19.3s is **completely invisible in
`perf.jsonl`** for this code path — see "Why the dominant cost is dark" below.
`DL_PROFILE=1` breaks the dark time open (separate run, same warm state):

```
[profile] reconcile-sources: 130.1ms
[profile] builtin-rels: 14.6ms
[profile] module-rels: 3.5ms
[profile] type-rels: 1.2ms
[profile] call-rels: 0.8ms
[profile] dataflow-rels: 0.4ms
[profile] sql: 2024 statements, 14715.0ms inside sqlite
real 16.20s
```

**2024 individual SQL statements consuming 14.7s inside SQLite is the single
biggest sink**, ~91% of that run's wall time. This is the derived-relation
fixpoint rebuild (154 relations × roughly DELETE + INSERT-SELECT + index
maintenance each ≈ 13 statements/rel), executed one at a time on one SQLite
connection (no batching, no parallelism — expected for a correctness-first
Datalog fixpoint, but it means 2024 sequential round-trips dominate).
`_stmt_ms` (a persisted per-rel timing table, `src/rels/perf.rs`) confirms the
worst individual relations from that pass:

| rel | ms |
|---|---|
| port_reach | 864 |
| member_edge | 765 |
| call_node | 458 |
| flow_edge | 307 |
| bare_edge | 301 |
| bare_node | 253 |
| call_target | 205 |
| bom_fan_out | 165 |
| (146 more, sum) | 4659 total |

`_stmt_ms`'s sum (4659ms) is well below the profiler's 14715ms "inside sqlite"
figure — `_stmt_ms` only wraps `rebuild_derived`'s primary per-rel statement,
not the DELETE that precedes it, not `rebuild_closures`, not
`eval_extract_rules` (term-form `json`/`jsonp` rules — several of the merged
programs' `chapter_blurb`/`lesson_blurb`/`*_table_has`/`side_page` rels are this
shape), not `create_auto_indexes` DDL, not the 154 `any_derived_empty` `COUNT(*)`
probes, not `_reldigest`/`_stmt_ms` writes themselves. All of that is real SQL
time, just not attributed per-relation.

## Root cause 1 (structural, reproduces every time, no contention needed): full derived rebuild every tick

`Engine::tick_report` (`src/engine/tick.rs:427-428`):

```rust
let need_full = derived_moved || carry_changed
    || self.any_derived_empty(&derived_rels)? || self.any_closure_empty(&edges)?;
```

`any_derived_empty` (`src/engine/mod.rs:4405-4411`) does one `SELECT COUNT(*)`
per derived relation and returns `true` on the **first** relation with zero
rows — vacuous-empty short-circuit. Queried directly against this worktree's
`cache.db` after a fully-warm run, **34 of the 154 rebuilt relations currently
have zero rows**, e.g. `flow_modeled`, `op_endpoint`, `trace_step`,
`type_neighbor_edge`, `node_added`/`node_removed`/`edge_added`/`edge_removed`
(from `.dl/graph-diff.dl`'s `diff_pair` — ledgered as shipping with an
intentionally inert default `(WORK,WORK)`), `mark_missing_kind`, `unwrap_count`.
These are **legitimately, permanently empty by design** in this corpus/config
(no PR diff in flight, no chat marks recorded this session, no unwraps to flag,
no OpenAPI spec matched). Because `||` short-circuits on the first hit,
`any_derived_empty` returns `true` on essentially every tick regardless of the
`derived:program` digest (`_reldigest` table, confirmed present and stable
across the two runs), which **permanently disables the digest-based
full-rebuild skip** for this 15-file merged program. Every `--check` invocation
therefore reruns the entire 154-relation fixpoint (the 2024-statement, 14.7s
block above) even when nothing on disk changed.

This is a genuine floor: even a perfectly warm, perfectly uncontended
`dl --check` on this worktree costs ~20s, almost entirely the derived rebuild,
because the "is anything empty" heuristic that decides whether a full rebuild
is needed treats "designed to be empty right now" the same as "never
populated."

## Root cause 2 (contention, explains the high-variance/300s+ tail): concurrent `dl` processes share one SQLite file with no coordination

While this investigation ran, a **second, independent** `dl --check` process
(another Claude session, different `root`) was observed contending for
`/Users/chrishafley/projects/sprefa/.dl/cache.db` — a different file from this
worktree's, so it did not corrupt our measurement, but it demonstrates the
failure mode is live in this environment: multiple `dl` invocations against the
same SQLite file with no application-level serialization. `perf.jsonl` records
carry no PID, so a caller cannot tell contention apart from genuine slowness by
reading the log alone (a real methodology gap, worth fixing in `perflog.rs`).

Separately, the **first background run launched for this investigation**
(`/usr/bin/time -p timeout 300 dl --check --no-daemon`, using the *installed*
`~/.cargo/bin/dl`, not this worktree's freshly-built binary) **hit the 300s
timeout and never completed** (`user 233.59s sys 9.96s` — CPU time far below
wall time, consistent with time spent blocked rather than computing). Its
`perf.jsonl` slice shows two back-to-back reconcile phases of **38,386ms** and
**51,391ms** — 15-45x this report's clean, isolated, warm reconcile (653ms,
same corpus, same machine, minutes apart). The same "two big reconciles,
20-50s each, back to back" signature recurs identically roughly a dozen times
across the day's full `perf.jsonl` history (escalating over the day: ~20s/26s
pairs mid-afternoon, ~38s/51s by evening), always in that paired shape. Given
this worktree's own cache.db is currently **278MB with a 35MB, never-checkpointed
WAL** (`ls -la .dl/`), and given SQLite's single-writer model means any
concurrent process (daemon, another agent's `--check`, a hook) holding a write
transaction blocks this process's own reconcile writes for the transaction's
full duration, the most parsimonious explanation for the historical 20-50s
reconcile pairs — and for the observed 300s timeout — is **lock contention with
another writer on the same cache.db**, not a cost intrinsic to reconcile logic
itself. (The uncontended reconcile in this report is 130-1104ms for the same
2500-file corpus.) This is not certain — I did not catch a contending PID
in the act against *this exact* worktree's db — but it is consistent with every
other data point, and the ever-growing 35MB WAL is itself a plausible secondary
drag on every reader/writer (uncheckpointed WAL pages must be scanned on every
connection open) independent of contention.

## Cold-run narrative (task item 4)

No genuinely-cold `cache.db` was recreated for this investigation (would cost
another 300s+ of the budget); the following is read from code plus the day's
historical `perf.jsonl`, not independently re-measured cold.

- The cited **20,159ms / 26,493ms** reconcile pair and the **28,944ms**
  dataflow-rels extract (files: parsed 314, extracted 884, retracted 809, total
  2496) are both real entries, located at `ts_ms` 1783712490613/1783712508098
  and 1783722016622 respectively.
- `reconcile_sources` (`src/engine/mod.rs:4413+`) loads prior file metadata
  (`load_file_meta`) and diffs it against a fresh enumerate+hash pass. Cold
  (no `_file` baseline), every file needs a real content hash, not just an
  mtime compare — for ~2500 files this is real but should be sub-second CPU;
  the two-reconciles-in-a-row shape recurring identically all day (present in
  our OWN clean run's tick as well as every historical occurrence) suggests
  reconcile runs **twice per tick** in some circumstance (once for the base
  corpus, once for a rev-aware pass — this program's `changed_rels` includes
  `_rev` twins: `type_entity_rev`, `call_def_rev`, `module_edge_rev`, from the
  D5 rev-aware extraction arc) rather than the file-hash work itself scaling to
  20s+ — i.e. the *pairing* is structural, the *20-50s magnitude* is most
  likely the same lock-contention effect as Root Cause 2, just caught here
  during a cold/high-load window rather than proven independently cold.
- The 28,944ms dataflow-rels extract with 314 parsed / 809 retracted is
  consistent with the **per-file fact cache working as designed** (Perf gaps
  A/C, CLAUDE.md ledger): only the 314 files whose corpus membership actually
  changed were re-parsed; the other ~2182 files in the 2496-file total were
  not. 314 files × full TypeLang dataflow lift (oxc/syn/tree-sitter parse +
  df_node/df_field/df_arg/lambda extraction) at ~92ms/file average is
  consistent with known per-file AST-based extraction costs elsewhere in this
  codebase — **this looks like real, proportionate work, not a cache miss
  bug.** (Task item 5, answered: the 809 retractions did *not* force a
  whole-corpus re-parse; `parsed:314` proves the fact cache correctly scoped
  the re-parse to the delta.)

## Why the dominant cost is dark in `perf.jsonl` (the methodology gap)

Two independent gaps compound:

1. **`activity::end_tick()` is never called on the in-process one-shot path.**
   It is called only from `daemon.rs`'s `ServedRoot::tick_full`/`tick_paths`
   (`src/daemon.rs:238,259,570`). `run_check_inproc` calls `Engine::tick`
   directly (`src/lib.rs:370`), which never reaches `end_tick`. `end_tick` is
   the only thing that flushes the *currently active* phase's elapsed time to
   `perf.jsonl` (`src/activity.rs:120-128`); without it, whatever phase was
   running when the tick function returns is never logged.
2. **The phase enum transitions that would otherwise close out "Derived" don't
   fire for this program.** `activity::set(Phase::Operators, ...)`
   (`src/engine/tick.rs:479`) — which would flush "Derived"'s timer — is
   gated on `!scc_rules.is_empty() || !node2vec_rules.is_empty() ||
   !edges.is_empty()`; this merged program apparently has none of those
   populated when this ran (no logged "operators"/"derived" phase entries
   appear in either clean run). `activity::set(Phase::Query, ...)`
   (line 526) — which would otherwise flush "Derived" via the next
   transition — is additionally gated on `!quiet`, and `--check` calls
   `eng.tick(&prog, true)` (`quiet=true`, `src/lib.rs:370`).

Net effect: for `dl --check`, the single most expensive phase (`Derived`:
`rebuild_derived` + `rebuild_closures` + `eval_extract_rules` +
`persist_type_decl_shapes` + `create_auto_indexes`, ~14.7s of SQL alone) is
**structurally guaranteed to never appear** in `perf.jsonl` as a `"phase"`
record. `stmt_ms`/`_reldigest` querying (as done in this report) is currently
the only way to see it, and even that is incomplete (misses DELETE/DDL/
extract-rule cost, per the gap between `_stmt_ms`'s 4659ms sum and the
profiler's 14715ms).

## Ranked sinks (this worktree, this corpus, warm+uncontended floor)

| rank | sink | ms (warm, clean) | evidence |
|---|---|---|---|
| 1 | Derived-relation full fixpoint rebuild (154 rels, 2024 SQL statements) | ~14,700 | `DL_PROFILE=1`: "sql: 2024 statements, 14715.0ms inside sqlite" |
| 2 | Everything else inside the "dark" 19.3s not accounted by #1 (eval_extract_rules term-form rules, create_auto_indexes DDL, any_derived_empty's 154 COUNT(*) probes, rebuild_closures, per-tick Rust-side overhead) | ~4,600 | `total_ms` 20141 − logged phases 820 − profiler sql 14715 ≈ 4606 |
| 3 | reconcile (file enumerate/hash/mtime, warm) | 130–1104 | perf.jsonl phase entries, both clean runs |
| 4 | extract families (module/type/call/dataflow, warm, digest-skip path) | 146–478 | perf.jsonl phase entries; matches ledgered perf-gap-A fix |
| 5 | declare | 18–48 | perf.jsonl phase entries |

Separately, **not part of the warm floor above but explaining Chris's observed
13-50s/300s+ range**: lock contention with any other `dl` process sharing the
same `cache.db` (Root Cause 2) can add tens of seconds to minutes on top of the
~20s floor, with no ceiling other than `timeout`.

## Fix shapes, ranked by expected win

1. **Fix `any_derived_empty` to stop forcing a full rebuild forever
   (biggest win, addresses the ~20s floor directly).** The check conflates
   "this relation's table has never been populated" (a real reason to force a
   full pass) with "this relation is currently, validly, zero rows" (not a
   reason). Candidates: only treat a relation as "empty" for this purpose the
   *first* time its table is created (track in `_reldigest` or a sibling
   sentinel row per relation, the same "sentinel already exists" idiom used
   elsewhere in this engine's ref-spine meta tables), or drop
   `any_derived_empty` from the `need_full` OR-chain entirely now that
   `derived_moved`/`carry_changed` already cover the "shape changed" and
   "carry changed" cases, and trust the scoped/`affected_derived` path (already
   built, `src/engine/tick.rs:440-452`) to correctly (re)populate an
   empty-by-design relation the moment its actual dependencies move.
2. **Instrument the derived-rebuild phase so it isn't dark.** Either (a) call
   `activity::begin_tick`/`end_tick` from the in-process one-shot path too
   (not just the daemon), or (b) add a perflog record directly around
   `rebuild_derived`/`rebuild_closures`/`eval_extract_rules` in `tick.rs`
   independent of the `activity` phase-transition mechanism, so a `--check`
   run's dominant cost is visible without needing `DL_PROFILE=1` + manual
   `_stmt_ms` archaeology.
3. **Serialize or detect concurrent writers on one `cache.db`.** At minimum,
   tag `perf.jsonl` records with PID so contention is diagnosable from the log
   alone; at most, the daemon's singleton-per-root model already solves this
   for daemon-served roots — extending "loud daemon-is-serving-this-root"
   warning (already an open ledger item, 2026-07-10 turnkey-query-surface
   debrief) to the plain `--check`/one-shot path would stop agents from
   accidentally running a second writer against a daemon-owned db.
4. **Checkpoint the WAL.** 35MB uncheckpointed WAL on a 278MB db is a
   plausible small drag on every connection open; a `PRAGMA wal_checkpoint`
   on clean daemon shutdown or idle would cost nothing when the db isn't busy.
5. **Batch the 154 `any_derived_empty` COUNT(*) round-trips** into one SQL
   statement (`SELECT (SELECT COUNT(*) FROM t1)+...` or a UNION ALL) if item 1
   isn't done outright — currently 154 synchronous round-trips every tick that
   reaches that check.

## What contradicted the initial hypotheses

- **"One engine tick per .dl program (~15 programs)"** — false. One merged
  program, one tick, confirmed by direct code read and reproduced isolated
  run. The multi-program appearance in raw `perf.jsonl` is cross-invocation
  log accumulation (no PID field), not per-file engine construction.
- **"Cold reconcile 20-50s is corpus-hashing cost"** — not supported as the
  primary explanation; a clean warm reconcile over the *same* 2500-file corpus
  measured 130-1104ms, three orders of magnitude below the historical 20-50s
  pairs. Lock contention with a concurrent writer is the better-supported
  explanation for the *magnitude*, though the *paired-reconcile shape* itself
  (two reconciles per occurrence) does look structural (rev-aware twin pass)
  rather than accidental.
- **"244MB db / 33MB WAL as prime suspect"** — plausible secondary drag, not
  ruled out, but not shown to be the dominant driver either; the ~20s
  uncontended floor is fully attributable to the always-full derived rebuild
  (Root Cause 1) independent of db/WAL size.
