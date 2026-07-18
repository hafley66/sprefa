# Failure modes: the classes that bit us, their rails, and the gaps

The canonical catalog of the mistakes agents (human and AI) keep re-making in
this repo. Every guard that exists was built after a bite; this doc exists so
the classes stop being rediscovered one incident at a time. Every claim cites
a file:line or commit hash; where no receipt exists the entry says
"unverified" instead of inventing one.

Rail-status rubric used throughout: **enforced** = a check that fails (it-test,
exit-2 `--check`, engine bail, runtime cap/watchdog). **half** = advisory only
(warn-tier rail, runtime eprintln, doc convention) or enforced at the fixed
sites but not against new code. **missing** = nothing.

## 1. Per-row writes / N+1

- WHAT IT LOOKS LIKE: an `execute`/`prepare`/`insert` inside a `for` loop, or a
  loop calling a fn whose own body does SQL — one statement per row instead of
  one statement per set.
- HOW IT BIT US: per-row INSERTs in refresh paths were defect 1 of the exe-swap
  write storm (docs/rca-exe-swap-write-storm.md:45, fix 4d0d24bf). Deltaflow had
  4 per-change write loops; batched to chunked multi-row INSERT/DELETE under the
  800-param ceiling inside one BEGIN IMMEDIATE (2bda577c, CLAUDE.md:64).
- THE LAW: "N+1: never a per-row write. Collect the set, call `Db::insert_rows`
  once. The tick counter screams if you don't." (CLAUDE.md:74)
- THE RAIL: half. Runtime: the `[n+1] '<key>' ran Nx this tick` counter screams
  on stderr (src/db.rs:503), with a guard test that chunked inserts stay under
  it (src/db.rs:782). Static twin: examples/n-plus-one.dl (loop-SQL hunt; "rows
  are receipts, not verdicts", examples/n-plus-one.dl:11-12) and warn-tier
  `.dl/static-n1.dl` with a prev-rev oracle (792cc902).
- SAY THIS TO AN AGENT: Collect the full row set and call `Db::insert_rows` once
  — never issue SQL inside a per-row loop; the `[n+1]` counter at src/db.rs:503
  will name you.

## 2. Nondeterministic extraction order

- WHAT IT LOOKS LIKE: file-set SELECTs with no ORDER BY, cache hits emitted
  before misses, first-wins dedup on a lossy key — two rebuilds of an identical
  corpus emit different rows.
- HOW IT BIT US: root defect 6 of the write storm. After defects 1–5 were fixed
  an exe-swap boot still wrote 4.4GB with `+0 -0 source facts` on the tick line
  (docs/rca-exe-swap-write-storm.md:52-54); the fingerprint was
  `+0 -0 source facts, derived rebuilt | 22193ms, trigger=full, reason=-`
  (docs/rca-exe-swap-write-storm.md:84). Found via flow_edge counts differing
  run to run on the same corpus: 235,427 vs 235,439
  (docs/rca-exe-swap-write-storm.md:96). Fix 80617b6b.
- THE LAW: identical corpus + identical binary = identical rows; "the design
  only works when re-extraction is deterministic; that invariant was assumed,
  never stated, and never tested" (docs/rca-exe-swap-write-storm.md:116-118) —
  and "jitter" is rejected as an unproven claim: exact equality or a named
  moving rel (docs/rca-exe-swap-write-storm.md:97-99).
- THE RAIL: enforced. `extraction_is_deterministic_across_identical_rebuilds`
  (tests/it/extraction_determinism.rs:134, a45c34d9) compares every rel's rows
  digest across two rebuilds; warn-tier `.dl/unordered-select.dl` and
  `.dl/lossy-dedup.dl` hunt new code forms, proven against pre-fix revs by
  scripts/rails-oracle.sh (792cc902). Residual: the lossy df_node id
  (`file:line:col`, no repo) still exists — ordering made the winner stable,
  not principled (docs/rca-exe-swap-write-storm.md:147-148).
- SAY THIS TO AN AGENT: Put ORDER BY on every file-set query and emit cached
  facts in input order — if two identical rebuilds differ, it is your bug,
  never "jitter".

## 3. Full-layer rebuild when scoped would do

- WHAT IT LOOKS LIKE: any change — a one-rule program edit, one durably-empty
  derived rel — triggers a whole-layer wipe: every derived table DELETEd and
  re-INSERTed byte-identical.
- HOW IT BIT US: every program edit used to wipe and rewrite every derived
  table; post-fix receipt on a warm src/rels corpus: edit tick 1 derived row vs
  7,312 forced-full (CLAUDE.md:55). Second arm: `any_derived_empty` treated a
  legitimately empty derived rel (34/154 on a real db) as "must full-rebuild",
  forcing a full derived rebuild on EVERY tick (tests/it/tick_digest.rs:4-8).
- THE LAW: attribute motion to the heads that moved — "the full-layer wipe
  downgrades to the scoped rebuild seeded with them" (CLAUDE.md:55); a rebuild
  you cannot attribute is a rebuild you pay for in GBs.
- THE RAIL: half. Program-edit arm enforced by tests/it/derived_scope.rs
  (proven fail-pre-fix; the `DL_STMT_TRACE=1` probe at
  tests/it/derived_scope.rs:6-9 makes "was this rel's table touched" directly
  observable) and the `_derived_complete` marker tests in
  tests/it/tick_digest.rs. MISSING for the residual: UNATTRIBUTABLE full
  rebuilds (blank slate, carry, edge-list change, crash-recovery) still rewrite
  byte-identical tables in SQL — the digest-before-write content skip is not
  landed (CLAUDE.md:55, docs/rca-exe-swap-write-storm.md:149-151).
- SAY THIS TO AN AGENT: Before wiping a derived table, attribute the change to
  the moved rule heads and rebuild only that subgraph; if you cannot attribute
  it, say so in the tick log.

## 4. Unguarded from-scratch recompute on reactive rules

- WHAT IT LOOKS LIKE: a fn that re-derives a relation/embedding from scratch
  (a global op like `embed_graph`) wired to a reactive rule, with no
  input-unchanged early-out.
- HOW IT BIT US: an unguarded recompute "re-runs on every git-checkout re-tick
  under the daemon lock" (CLAUDE.md:78); node2vec walks the whole graph, so the
  cost is a full re-embed per re-tick (examples/recompute-guard.dl:6-9). No
  standalone GB receipt — the class was railed from the storm family before it
  produced its own storm; specific numbers unverified.
- THE LAW: "a fn that re-derives a relation/embedding FROM SCRATCH ... must
  early-out when its input is unchanged — a `load_rel_digest` digest skip ... —
  or carry a `// @recompute unguarded: <reason>` waiver" (CLAUDE.md:78).
- THE RAIL: enforced. `dl examples/recompute-guard.dl --check` exits 2 on a
  recompute fn with neither guard nor waiver (diag `unguarded-recompute`,
  examples/recompute-guard.dl:76); `--lsp` squiggles the call line while
  editing.
- SAY THIS TO AN AGENT: Any fn that re-derives from scratch on a reactive rule
  gets a `load_rel_digest` early-out or a `// @recompute unguarded: <reason>`
  waiver — examples/recompute-guard.dl `--check` exits 2 otherwise.

## 5. Crash-window half-written derived state

- WHAT IT LOOKS LIKE: a long pass that wipes everything up front and marks
  completion once at the end — a SIGKILL anywhere in the window leaves the
  whole layer reading incomplete.
- HOW IT BIT US: defect 4 of the write storm: a kill during `rebuild_derived`
  left EVERY derived rel wiped and unmarked, the next boot full-rebuilt
  everything, re-choked, got re-killed — self-perpetuating (7f4d9c58 commit
  message; docs/rca-exe-swap-write-storm.md:48). The user's force-kill culture
  armed the next boot to storm again (docs/rca-exe-swap-write-storm.md:18-19).
- THE LAW: a kill mid-pass may cost one component, never the layer — bracket
  every long pass unmark → wipe → run → mark per component
  (docs/rca-exe-swap-write-storm.md:137-138).
- THE RAIL: enforced. Per-component unmark/wipe/run/mark plus deferred
  source-digest saves and the bulk-rebuild I/O guard (7f4d9c58, 6afd2cf3,
  5cf4be15); live `kill -9` mid-derived receipt 2026-07-17: clean recovery, no
  storm, follow-up ticks <1s, component scoping pinned by it-tests
  (CLAUDE.md:53).
- SAY THIS TO AN AGENT: Structure every long pass as per-component
  unmark → wipe → run → mark so a SIGKILL costs one component, never the whole
  layer.

## 6. Daemon restart / exe-swap re-extract storms

- WHAT IT LOOKS LIKE: `cargo install` followed by a daemon boot writes
  gigabytes, pins CPU for a minute-plus, and `dl daemon status` says "not
  running" while the storm is mid-pass.
- HOW IT BIT US: the worst one on record. Every post-install boot wrote 4.4 to
  6.1GB and pinned CPU 60–90s, for weeks, intermittently
  (docs/rca-exe-swap-write-storm.md:8-17). Six distinct defects over three
  sessions (defect table docs/rca-exe-swap-write-storm.md:43-50); the 2s poll
  full-tick storm (7af0e319) and the per-process exe-identity cache forcing
  full rebuilds every tick of the first post-install daemon (c351ed90) were the
  same family (CLAUDE.md:52). Double-swap receipt 2026-07-17: 6.1GB/72.9s cpu →
  110.9MB/8.5s; steady state after settle 0.0MB per 60s window, rss 18MB
  (docs/rca-exe-swap-write-storm.md:104-111).
- THE LAW: "Self-diagnosis before execution: ... Never make the user ask 'why
  is it slow' — the system answers that itself" (CLAUDE.md:25-29) and "A change
  that can beachball the machine is a blocking defect, not a follow-up"
  (CLAUDE.md:30-32).
- THE RAIL: enforced. Machine budgets in `apply_daemon_budget` (CPU QoS/nice,
  IOPOL_THROTTLE, thread cap — CLAUDE.md:30-32); `DL_MAX_WALL_SECS` watchdog,
  default 300s, exit 124 naming phase/root (e7d29829, CLAUDE.md:54);
  `dl daemon why` reads the on-disk trail after any exit including SIGKILL
  (why.jsonl, 1105fe9d; docs/rca-exe-swap-write-storm.md:132-134); per-root
  perf.jsonl carries `full_reason`/`changed_rels`, `reason=` rides the `[tick]`
  line (docs/rca-exe-swap-write-storm.md:135-136); the four storm code forms
  are banned by warn-tier rails with a prev-rev oracle (792cc902). The
  per-tick (rel, rows-written) ledger the RCA asked for
  (docs/rca-exe-swap-write-storm.md:152) has since landed:
  src/rels/write_ledger.rs, flushed at src/engine/tick.rs:582.
- SAY THIS TO AN AGENT: After any install or restart, read `dl daemon why` and
  the tick line's `reason=` before concluding anything — and never force-kill
  the daemon without capturing the on-disk trail first.

## 7. Quiet-tick write-budget violations

- WHAT IT LOOKS LIKE: the daemon is idle — no edits, no checkouts — yet
  `_write_ledger` shows rows landing for the served root tick after tick.
- HOW IT BIT US: this was the storm's steady-state shape (classes 1–6 all fed
  it); post-fix the receipt is 0.0MB written per 60s quiet window
  (docs/rca-exe-swap-write-storm.md:111). The budget itself is written down —
  "on a QUIET tick ... `_write_ledger` for this program's root MUST show zero
  rows" (examples/chaos-soak.dl:23-31) — but documented, never enforced: no
  check fails when a quiet tick writes.
- THE LAW: a quiet tick writes zero rows; anything else is a defect by
  definition (examples/chaos-soak.dl:23-31).
- THE RAIL: MISSING. The ledger exists (src/rels/write_ledger.rs; flushed at
  src/engine/tick.rs:582) and chaos-soak.dl names the assertion; nothing runs
  it. Proposed rail: serve examples/chaos-soak.dl under the daemon in CI
  (DL_POLL_SECS small) and fail the run if `_write_ledger` shows any row for
  the root outside the 30s heartbeat tick.
- SAY THIS TO AN AGENT: On a quiet tick write nothing — check `_write_ledger`
  for your root and treat any row there as a defect you introduced.

## 8. Lock held across blocking or lock-taking calls

- WHAT IT LOOKS LIKE: a `lock(&sr.eng)` guard alive across a call that blocks
  (sleep/wait/recv) or takes another lock — e.g. the known prog → eng nesting.
- HOW IT BIT US: no recorded deadlock incident in the sources — unverified as a
  bite; catalogued pre-bite from the inventory. The exposure is real: 107
  lock-acquisition sites, 60 of them in src/daemon.rs alone
  (docs/effect-inventory.md:266, :381), with named nesting edges prog → eng at
  src/daemon.rs:365-366, 407-408, 434-435, 595-596, 626-627 and
  program_files → eng at src/daemon.rs:522-523
  (docs/effect-inventory.md:296-299). Adjacent bite: an unguarded recompute
  re-runs "under the daemon lock" (CLAUDE.md:78) — whatever runs under a lock
  is on every other lock-holder's critical path.
- THE LAW: never hold a lock across a call that can block or take another lock
  — clone the data out, drop the guard, then call.
- THE RAIL: MISSING. The chokepoint exists: 83 of 107 acquisitions go through
  the `lock`/`rlock`/`plock` helpers (docs/effect-inventory.md:262, :381), so
  one helper-level instrument covers most of the surface. Proposed rail: a
  thread-local hold-set in the helpers (debug_assert/tracing when a second
  acquisition overlaps the first) plus a static `.dl` rail joining helper-call
  fns against blocking calls in the same body.
- SAY THIS TO AN AGENT: Never invoke anything that blocks or locks again while
  holding a guard — clone the data out, drop the guard, then call.

## 9. Unbounded channels

- WHAT IT LOOKS LIKE: `mpsc::channel()` / `unbounded_channel()` behind a
  producer (poll loop, watcher) that can outrun its consumer — the queue becomes
  an unaccounted memory and eventual-write amplifier.
- HOW IT BIT US: no recorded incident — unverified as a bite; catalogued
  pre-bite. The repo has 4 channel creation sites, 3 of them unbounded:
  src/engine/derive.rs:40, src/daemon.rs:1235, src/lib.rs:633; the one bounded
  site is src/cli/check_deadline.rs:18 (`sync_channel(1)`)
  (docs/effect-inventory.md:308-315, :382).
- THE LAW: every channel is bounded; an unbounded queue under the daemon poll
  loop is a write storm waiting for a fast producer.
- THE RAIL: MISSING. Proposed rail: a static `.dl` rail flagging
  `mpsc::channel()`/`unbounded_channel()` without a `// @unbounded: <reason>`
  waiver, error severity on changed files — drive the inventory count from 3 to
  0-or-waivered.
- SAY THIS TO AN AGENT: Create every channel with an explicit bound
  (`sync_channel(N)`); an unbounded channel needs a `// @unbounded: <reason>`
  waiver.

## 10. Magic rel-name reads

- WHAT IT LOOKS LIKE: engine Rust special-casing a relation by a literal string
  — `eng.rels.get("scip_want")`, `FROM rel_effect_cmd` — where nothing in
  `rel_catalog` tells a program author the name is critical.
- HOW IT BIT US: the effects orphan mystery (67ed59fe): `rel_effect_cmd` stores
  interned INTEGER ids and both executor call sites read them via `as_str()`
  (always None), so every boot parked dynamic-template effects orphaned — 5
  re-parked every boot; post-fix receipt 6/6 effects done, 0 orphaned
  (CLAUDE.md:56). The name and its storage shape were invisible API surface:
  you learn it exists "only by reading engine source or tripping over it"
  (assets/sprefa-v5-no-magic-rels.skill.md:8-13).
- THE LAW: "Every name the engine reads by literal string must be a catalogued
  relation (in `rel_catalog`)." (assets/sprefa-v5-no-magic-rels.skill.md:15-16)
- THE RAIL: enforced. `.dl/magic-rel-audit.dl` — `dl --check` exits 2 with
  `magic-rel-unregistered`, `--lsp` squiggles the source line
  (assets/sprefa-v5-no-magic-rels.skill.md:16-18, :56-57); demand/overlay
  conventions live as catalogued builtin sinks in `demand_rel_decls()` +
  `DEMAND_RELS` (src/engine/mod.rs; assets/sprefa-v5-no-magic-rels.skill.md:33-36);
  docs/reference/magic-rels.md is generated from the catalog.
- SAY THIS TO AN AGENT: Never read a relation by literal string name in engine
  Rust unless it is catalogued in `rel_catalog` — clear a
  `magic-rel-unregistered` finding by declaring the rel, never by narrowing
  the rail.

## 11. Co-heading source+derived rules

- WHAT IT LOOKS LIKE: one relation headed by both a source rule
  (scan/match/ast/sg/json/cmd/comment) and a derived rule.
- HOW IT BIT US: `rebuild_derived` does a full `DELETE FROM rel` that would
  wipe the reconciled source rows (CLAUDE.md:77). The term-extract twin bit
  separately: `eval_extract_rules` fills the extract rows, then
  `rebuild_derived` (running after it) drops them — which is also why a
  term-extract rule cannot feed a `@next` carry directly; the
  `pr_number -> change_log` split in examples/gh-cache.dl is the canonical
  workaround (CLAUDE.md:77).
- THE LAW: "One rel = one rule kind: never head a rel with both a source rule
  ... and a derived rule. ... split into two rels and union in a third derived
  rule." (CLAUDE.md:77)
- THE RAIL: enforced. The engine bails on the mixed form
  (src/engine/tick.rs:340-342; the desugar rewrite and its own error at
  src/engine/desugar.rs:173), including the term-extract variant since the
  ghcacher-parity arc (CLAUDE.md:77); behavior pinned in
  tests/it/mixed_source_derived.rs.
- SAY THIS TO AN AGENT: Give each relation exactly one rule kind — split source
  and derived heads into two rels and union in a third; the engine bails on the
  mixed form.

## 12. Ambient config ingestion in tests

- WHAT IT LOOKS LIKE: an ad-hoc `dl` run or it-test that silently ingests
  `~/.config/sprefa/config.toml` `[[repos]]` — the corpus, and therefore every
  count you assert, depends on the operator's machine.
- HOW IT BIT US: a recurring cross-debrief pain — "every ad-hoc `dl` run
  ingests `~/.config/sprefa/config.toml` repos" (CLAUDE.md:70(a)); ambient
  config hermeticity is the top-ranked open friction item (CLAUDE.md:69). Same
  family: something during `cargo test --test it` regenerates
  docs/reference/syntax.md IN-TREE with the installed dl, dirtying the repo
  mid-suite (PLANS.md:17), and the empty-scan sharp edge narrowed a served
  snapshot 618 → 68 files (c3c587c9, CLAUDE.md:57).
- THE LAW: tests and smoke runs are hermetic — "set `SPREFA_CONFIG` for
  hermetic smoke tests" (CLAUDE.md:70(a)); the ambient user config is
  production data, never test input.
- THE RAIL: half. The convention exists and the good tests pin it
  (`.env("SPREFA_CONFIG", "/dev/null")`, tests/it/derived_scope.rs:73;
  `SPREFA_CONFIG=<hermetic.toml>` in examples/chaos-soak.dl:45) — but nothing
  fails a test that forgets, and the hermeticity item is still open.
  Promotion: make hermetic the default in the it-test harness (fail when
  `SPREFA_CONFIG` is unset) and fix the in-tree syntax.md writer (PLANS.md:17).
- SAY THIS TO AN AGENT: Set `SPREFA_CONFIG=/dev/null` (or a hermetic fixture)
  in every test and smoke run you write — never let the ambient user config
  into a verification.

## 13. Stale-binary verification after stash/rebuild cycles

- WHAT IT LOOKS LIKE: you rebuild, then "verify" through a hook, daemon, or
  PATH lookup that still runs the previously installed binary — the new code
  never executes and the conclusion is fiction.
- HOW IT BIT US: freeze #1, 2026-07-17 22:18 — a worker ran `dl --check`, which
  auto-started the daemon with the STALE installed binary; boot cascade plus
  4.97G/6G swap froze the machine until the daemon was killed and the binary
  reinstalled (chat_log/20260717.3.big-wins-13-14-15-arch-expr-lab-freeze-rca.md:14).
  Earlier: a perf comparison was invalidated because "post-fix" runs used a
  stale binary (chat_log/20260626.5.refactor-reward-loop-engine-split.md:49),
  and a lint debugging session burned a cycle on three causes, one of them the
  stale binary (chat_log/20260628.0.lint-spans-edit-algebra.md:121). The hook
  runs the INSTALLED dl — after any rebuild you must
  `cargo install --path . --force` (chat_log/20260706.0.magic-rel-ban-eliminate-builtin-sinks.md:82).
- THE LAW: verify against the binary you just built — tests locate it via
  `env!("CARGO_BIN_EXE_dl")` (assets/sprefa-v5-working-conventions.skill.md:10);
  a hook or daemon run verifies the installed build, not your worktree.
- THE RAIL: half. The e2e harness pins `CARGO_BIN_EXE_dl` everywhere (e.g.
  tests/it/derived_scope.rs:15), but the hook path runs the installed dl with
  no freshness check; the daemon already reports `build_id`
  (src/cli/daemon.rs:209) and `ensure_daemon` has a build_id path
  (plans/2026-07-10-lsp-thin-client-daemon.md:141) that hooks do not consult.
  Promotion: the hook compares installed build_id against the worktree target
  and refuses (or warns) on mismatch.
- SAY THIS TO AN AGENT: Verify with the binary you just built —
  `env!("CARGO_BIN_EXE_dl")` in tests, and `cargo install --path . --force`
  before trusting any hook or daemon run.

## 14. Pre-commit hook cold-daemon hang in worktrees

- WHAT IT LOOKS LIKE: `git commit` in a throwaway worktree hangs: the
  pre-commit `dl --check` finds a blank db, auto-starts a daemon, and
  cold-starts the full extract pipeline inside a commit hook.
- HOW IT BIT US: "pre-commit `dl --check` in throwaway worktrees cold-starts a
  daemon and hangs — every delegated agent hit it" (CLAUDE.md:70(i)). The same
  shape produced freeze #1 with real numbers (class 13:
  chat_log/20260717.3.big-wins-13-14-15-arch-expr-lab-freeze-rca.md:14).
- THE LAW: a commit hook must never cold-start a daemon — blank-db roots take
  the inline path or skip.
- THE RAIL: MISSING. The fix shape is named but unbuilt: "worktree-root
  detection or a hook fast-path for blank-db roots" (CLAUDE.md:70(i)).
  Proposed rail: in the hook, if the root's db is blank (or the root is a
  linked worktree), run `dl --check --no-daemon` inline instead of daemon
  autostart.
- SAY THIS TO AN AGENT: Never let a hook cold-start the daemon — on a blank-db
  or worktree root, run `dl --check --no-daemon` inline or skip the check.

## 15. Dishonest change flags

- WHAT IT LOOKS LIKE: a refresh fn returns `Ok(true)` ("I changed rows")
  unconditionally, or bookkeeping rels count toward settle — every tick reads
  as changed and the cascade re-fires honestly on a lie.
- HOW IT BIT US: defects 2, 3, and 5 of the write storm. scip/catalog/
  type/dataflow refreshers returned `Ok(true)` unconditionally — 14 rels
  "changed" every tick (docs/rca-exe-swap-write-storm.md:46, fix 4d0d24bf);
  `refresh_call_rels` hardcoded `Ok(true)` so an exe-swap re-derive of an
  unchanged corpus cascaded the flow rails (docs/rca-exe-swap-write-storm.md:49,
  fix f48749e0); `stmt_ms`/`rel_count`/`query_log` moved every tick so the root
  never settled and the poll loop re-enqueued full ticks forever
  (docs/rca-exe-swap-write-storm.md:47, fix 4d0d24bf via
  `RelKind::bookkeeping()`; same family as the 2s poll storm, 7af0e319). The
  storm's own lesson: "The system was telling the truth about garbage"
  (docs/rca-exe-swap-write-storm.md:79) — change flags are the truth contract.
- THE LAW: a refresh fn returns the ORed `rows_changed` of what it actually
  wrote — "changed" is a claim you must be able to defend with a delta
  (4d0d24bf, f48749e0).
- THE RAIL: half. The fixed sites carry real deltas and settle bookkeeping is
  excluded (`RelKind::bookkeeping()`, 4d0d24bf; settle behavior tested in
  tests/it/settle.rs:1-6), but the guard against NEW dishonest writers is only
  warn-tier: `.dl/dishonest-flag.dl` with a prev-rev oracle (792cc902; 25
  unwaivered findings at HEAD). Promotion: waiver-audit the 25, then promote
  to error severity.
- SAY THIS TO AN AGENT: Return the real `rows_changed` from every refresh fn —
  never `Ok(true)` unconditionally; a change flag is a claim you must defend
  with a delta.

## 16. Kill-respawn cold-restart loop

- WHAT IT LOOKS LIKE: killing the daemon while a still-alive client (a hook, an
  agent's `dl --check`, an editor) is attached — the client autostarts a fresh
  singleton, which starts the cold rebuild from zero; every kill multiplies the
  total work instead of stopping it.
- HOW IT BIT US: 2026-07-18 morning. An external agent session ran a bash line
  invoking `dl --check` twice in ~/projects/games/smash; ambient config
  registered the sprefa root too. Six singleton generations in 7 minutes (pids
  2144, 3546, 9220, 9840, 12281 in .dl/perf.jsonl), each booting blank
  ("first-run" on every family — deferred digest saves mean a kill mid-extract
  persists nothing). The user killed it repeatedly; each kill bought a fresh
  cold start. Per-generation cost from `dl daemon why` on the killed pid 12281:
  409.6MB disk read in one 13s window, 499MB read total — ~6 generations
  ≈ 3GB read on the boot volume. Budgets held (nice 10, iothrottle, 3 threads);
  the machine beachballed anyway on sustained throttled reads.
- THE LAW: a kill is a stop order, not a restart trigger. Clients may autostart
  a daemon at most once per invocation, with backoff; cold-extract progress
  must persist at bounded intervals so generation N+1 resumes where N died,
  never from zero.
- THE RAIL: missing. No spawn backoff, no respawn-once semantics, no
  mid-cold-extract digest persistence (completion-gate saves only). The
  invocation log + spawn-attribution work (obs-logging arc) makes the loop
  visible; nothing yet prevents it. Discriminating test needed: kill the daemon
  mid-cold-extract, assert the next boot resumes (no "first-run" reason) —
  fail-pre-fix provable today.
- SAY THIS TO AN AGENT: Never wrap `dl --check` in a retry or call it twice in
  one bash line — each call can spawn a daemon; and if the user kills the
  daemon, stop invoking dl entirely until told otherwise.

## 17. Unbounded db growth, open-cost amplification

- WHAT IT LOOKS LIKE: the per-root db grows without bound relative to its
  corpus; every daemon boot, WAL recovery, and reconcile pays reads
  proportional to db size, not corpus size.
- HOW IT BIT US: same incident. The singleton's sprefa root db
  (roots/fbabddda40d22347/db.sqlite) was 979MB + 81MB WAL for a 7.3MB, 712-file
  corpus (~140x). VACUUM reclaimed only 84MB (979 → 895MB) — the bloat is live
  rows: `_strings` 1.35M rows (124MB + 63MB index), 463 rel tables each doubled
  by a unique autoindex. A second world existed: the per-root-era fossil
  ~/projects/sprefa/.dl/.state/cache.db at 1.7GB + 225MB WAL, last fed by
  one-shot runs 2026-07-17 (deleted 2026-07-18, ~2GB freed). Every respawn in
  class 16 re-read against the 1GB file — db size is the amplifier that turned
  a respawn loop into a machine seize.
- THE LAW: db bytes are a budgeted resource like CPU and I/O. A root db an
  order of magnitude larger than its corpus is a defect to explain, not a cost
  to absorb.
- THE RAIL: missing. No size accounting, no ratio ceiling, no storage diet.
  Candidate rail: a boot-time verdict line (db bytes, corpus bytes, ratio) plus
  a `--check` warning above a ratio ceiling. Diet arc candidates, measured
  (2026-07-18 quantification of CLAUDE.md debt shape 3, string-inline-
  everywhere): 89% of the 1.35M `_strings` rows (1.20M) are unique-by-
  construction coordinate composites (`file:line:col:kind` syms, avg 43 chars)
  — interning them buys zero sharing; rev-scoped syms concatenate a full
  40-char sha onto each (`<sha><path>:<line>:<col>:<kind>`, a second string
  population per rev); each string is stored as content + norm + a 63MB norm
  index (~3x bytes); autoindex duplication doubles all 463 rel tables.
  Comparison points: scip index for the same repo is 21MB (~3x source); CodeQL
  dbs run 5-20x source; this db is ~120x. Fix shapes: coordinates as integer
  columns (the v1 coordinate model), rev as a `(rev_id, sym_id)` pair, syms as
  strings only at query/display boundaries.
- SAY THIS TO AN AGENT: Before blaming CPU for a slow or thrashing daemon, `ls
  -la` the root db and its WAL — reads scale with db bytes, and a GB-scale db
  on a MB-scale corpus is the defect.

## 18. Per-rule parse amplification, tiers-not-ceiling

- WHAT IT LOOKS LIKE: the daemon pegs 250-278% CPU inside source extraction
  while every OS background tier (QoS utility, nice 10, darwin BG, IOPOL
  throttle — all verified applied) is active; stack samples show every rayon
  worker in `prepare_source_batch` → tree-sitter.
- HOW IT BIT US: 2026-07-18 midday, same incident day as classes 16+17. Source
  extraction created one job per (file, RULE): `parse_file` parsed the file
  internally, so K ast/comment rules over one file = K full tree-sitter
  parses — and the grouped work-path shape looped rules with a fresh parse
  each. ~7 served .dl programs x their rules x 712 files, re-triggered by
  binary swaps, kills mid-extract (class 16 blank slate), and every
  cargo build re-tick during development. Named by stack capture
  (`scripts/dl-trace.sh`, /tmp/dl-trace/20260718-114412), not by guessing.
- THE LAW: (a) OS background tiers are scheduling advice, not ceilings; only
  an in-process duty-cycle governor caps CPU (src/budget.rs, daemon default
  100%, `DL_MAX_CPU_PCT`). (b) A tick parses a file at most once per grammar,
  no matter how many rules match it.
- THE RAIL: enforced. Parse-counter tests pin the invariant
  (`full_batch_parses_once_per_file_per_grammar` fail-pre-fix at 6 parses for
  3, `work_path_parses_once_per_grammar_across_ast_rules` at 2 for 1,
  `non_tree_rules_do_not_parse` at 0); `tests/it/budget_cpu.rs` pins the
  governor toggle (uncapped 1.10x vs capped@60% 0.87x). Fix receipt on a
  5-rule program over 159 files: 636 → 159 parses (4x), ~35% cpu, ~30% wall.
- SAY THIS TO AN AGENT: when a daemon burns CPU, capture stacks first
  (`scripts/dl-trace.sh`) — the 2026-07-18 RCA came from one sample naming
  the exact function, after a day of plausible wrong theories.

## 19. Subscriber render storm, daemon exhaust feedback

- WHAT IT LOOKS LIKE: the machine's UI phases (WindowServer CPU, input latency
  up to 10s, swap pressure) while the daemon looks healthy — CPU governed,
  no crash, no respawn. The victim is a subscribed client (instant), not dl.
- HOW IT BIT US (2026-07-18 evening, numbers): the daemon appended telemetry
  (`perf.jsonl`) inside the watched root; each append was a change event; each
  event scheduled a tick; each tick appended again — a tick every ~2s,
  ~350MB/min of no-op reconcile writes, daemon rss 19→438MB. Every tick then
  broadcast `diag_changed` to subscribers UNCONDITIONALLY (tick counter always
  advances, so every frame looked fresh). instant re-queried and re-rendered
  its webview per frame; WindowServer composited every repaint; swap hit
  3.7GB/5GB; typing lagged ~10s systemwide. A/B receipt: daemon stopped →
  instant instantly responsive. Separately measured on the same trail: every
  binary deploy invalidates every root's extraction (`extract:{exe_stamp}:…`
  digest key), so each redeploy is a full re-extract — 1.1GB written in the
  first 58s (tick 0, phase=extract), 266s worst tick on instant (12,561
  parsed / 12,432 retracted). Four deploys that day = four write storms.
- THE LAW: the daemon's own outputs are never inputs — not to its watcher
  (exhaust files inside watched roots), not to its subscribers (a no-op tick
  is not a change). Push frames must encode "something changed", not "a tick
  happened".
- THE RAIL: partial. Landed same day: (1) watcher filter — daemon-owned
  `.dl` state (perf.jsonl, why.jsonl, cache.db*) never schedules a tick,
  unit-tested (`watch_filter_tests`); (2) no-op ticks (nothing reconciled, no
  timer boundary, no digest move) skip `broadcast_diag_changed`. Gaps, open:
  no fail-pre-fix it-test for either (the unit test covers the path filter
  only); cold-extract write RATE is unbounded (the write-volume budget lever,
  scheduler arc); `exe_stamp` deploy invalidation is by design but uncosted —
  a per-family extractor version would invalidate only what changed;
  perf.jsonl grows without rotation (64MB observed in the daemon home);
  client-side amplification (instant's per-push wholesale re-render) under
  audit in instant's repo.
- SAY THIS TO AN AGENT: if the machine lags while `dl daemon why` shows a
  healthy governed daemon, count ticks per minute and ask who told the client
  to redraw. A subscribed UI multiplies every needless push by its render
  cost, and WindowServer pays the bill.

## Rail gap table

| # | class | rail status | promotion needed |
|---|-------|-------------|------------------|
| 1 | per-row writes / N+1 | half | waiver-audit the 40 HEAD static-n1 findings (792cc902), promote `.dl/static-n1.dl` to error severity |
| 2 | nondeterministic extraction | enforced | replace the lossy df_node id (`file:line:col`) with a repo-scoped id — ref-spine owns (docs/rca-exe-swap-write-storm.md:147) |
| 3 | full-layer rebuild | half | derived-layer content skip (digest-before-write) for UNATTRIBUTABLE full rebuilds (CLAUDE.md:55, docs/rca-exe-swap-write-storm.md:149-151) |
| 4 | unguarded recompute | enforced | — |
| 5 | crash-window half-written state | enforced | — |
| 6 | exe-swap / daemon-restart storm | enforced | — |
| 7 | quiet-tick write budget | missing | serve examples/chaos-soak.dl under the daemon in CI; fail on any `_write_ledger` row for the root on a quiet tick (examples/chaos-soak.dl:23-31) |
| 8 | lock across blocking calls | missing | hold-set instrumentation in the `lock`/`rlock`/`plock` helpers (83/107 sites, docs/effect-inventory.md:262) + static rail on lock-then-block in one fn body |
| 9 | unbounded channels | missing | static rail banning unbounded ctors without a `// @unbounded:` waiver; drive 3 → 0-or-waivered (docs/effect-inventory.md:308-315) |
| 10 | magic rel-name reads | enforced | — |
| 11 | co-heading source+derived | enforced | — |
| 12 | ambient config in tests | half | hermetic-by-default it-test harness (fail when `SPREFA_CONFIG` unset); fix the in-tree syntax.md regen (PLANS.md:17) |
| 13 | stale-binary verification | half | hook compares installed `build_id` (src/cli/daemon.rs:209) against the worktree target; refuse or warn on mismatch |
| 14 | pre-commit worktree hang | missing | worktree-root detection or hook fast-path for blank-db roots (CLAUDE.md:70(i)) |
| 15 | dishonest change flags | half | waiver-audit the 25 HEAD findings, promote `.dl/dishonest-flag.dl` to error severity (792cc902) |
| 16 | kill-respawn cold-restart loop | half | `--hook` is attach-only since 2026-07-18 (never autostarts; sub-second self-deadline); still missing: `--check` autostart-once + backoff, mid-cold digest persistence, kill-mid-cold resume it-test |
| 17 | unbounded db growth | half | boot verdict line (db bytes, corpus bytes, ratio) + `--check` ceiling warn; diet steps 1+3 landed (norm drop, -60% `_strings` on fixture), step 2 index audit + 4a WITHOUT ROWID open |
| 18 | per-rule parse amplification / tiers-not-ceiling | enforced | — (parse-counter tests + governor toggle test; `dl --hook` self-deadline `DL_HOOK_DEADLINE_MS` landed same day) |

## How a new rail gets born here

The observed standard, in order — no step is optional:

1. **Incident with numbers.** GB written, rows forced, seconds pinned. A vibe
   is not an incident.
2. **RCA that names the defect chain.** docs/rca-exe-swap-write-storm.md is the
   model: six defects in firing order, each fix's receipt exposing the next.
   "Jitter" and other unproven claims are rejected at review — exact equality
   or a named moving rel (docs/rca-exe-swap-write-storm.md:97-99).
3. **Discriminating test, proven fail-pre-fix.** The test must be shown to fail
   on the pre-fix code, not merely pass after. Precedents:
   tests/it/derived_scope.rs (CLAUDE.md:55), tests/it/racy_mtime.rs (f2205994),
   the loop break-value tests (aa6722ea) — all recorded "proven fail-pre-fix".
   Equivalence tests pin the invariant end-to-end (staged == inline per-rel row
   counts, tests/it/cold_stage.rs:119-120).
4. **The rail.** it-test, engine bail, exit-2 `--check`, or runtime counter —
   in that order of preference. Warn-tier `.dl` rails must carry an oracle:
   scripts/rails-oracle.sh proves each rail flags the historical pre-fix rev
   (792cc902). A rail nobody proved on old code is a hypothesis.
5. **Ledger receipt.** The fix lands only when a before/after receipt with
   numbers rides the ledger: 4.7GB → 111MB, 72.9s → 8.5s cpu (CLAUDE.md:53).
   Then — and only then — the class moves from this doc's gap table to its
   enforced list.
