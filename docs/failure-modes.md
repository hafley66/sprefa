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
- THE RAIL: mostly enforced. Program-edit arm by tests/it/derived_scope.rs
  (proven fail-pre-fix; the `DL_STMT_TRACE=1` probe at
  tests/it/derived_scope.rs:6-9 makes "was this rel's table touched" directly
  observable) and the `_derived_complete` marker tests in
  tests/it/tick_digest.rs. The digest-before-write content skip landed
  2026-07-18 for the residual: every NON-recursive derived component now
  evaluates into a TEMP mirror (its own pager, zero main-db WAL) and an
  identical rowset skips the whole unmark/wipe/refill/mark bracket, on
  attributable AND unattributable rebuilds alike — receipts in
  tests/it/derived_skip.rs (fail-pre-fix: 11,470,112 WAL bytes per no-op
  re-derive of a 160k-row rel pre-fix vs the ~2.5MB tick noise floor
  post-fix; `DL_NO_DERIVED_SKIP=1` is the parity lever; perf.jsonl
  `derived.skipped` + `Engine::last_derived_skipped` name the skips).
  Residual: RECURSIVE components (fixpoint/native-walk) still rewrite
  byte-identical rows inside an unattributable full rebuild.
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
- THE RAIL: MISSING (the CI soak is still not wired), but the derived-side
  writer class is closed: an identical re-derivation now skips the wipe+refill
  and calls no record_write, so it can no longer land `_write_ledger` rows on
  a quiet tick (digest-before-write, tests/it/derived_skip.rs — a skipped rel
  writes zero rows AND zero WAL bytes). Proposed rail unchanged: serve
  examples/chaos-soak.dl under the daemon in CI (DL_POLL_SECS small) and fail
  the run if `_write_ledger` shows any row for the root outside the 30s
  heartbeat tick. New finding while measuring (2026-07-18): `declare_all`
  DROP+CREATEs every rel's `_txt` VIEW unconditionally each tick
  (src/engine/declare.rs:4, `create_rel_view`), rewriting sqlite_master — a measured
  ~2.5MB WAL noise floor per tick even when zero rel rows move. The ledger
  never sees it (schema pages, not rows), so the CI soak should also assert
  WAL byte growth, not just ledger rows.
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
  chat_log/20260717.3.big-wins-13-14-15-arch-expr-lab-freeze-rca.md:14). The
  storage receipt landed 2026-07-19: the overnight fleet wave minted three
  593MB orphan root dbs — keys 5658fb5a59d0f252, c22f2b330d2dd1f7,
  ea3041acfc1af14c, ~1.86GB total — one per agent worktree, each cold-built by
  that worktree's pre-commit hook and each verified absent from `roots.json`.
  The worktrees were deleted; the dbs stayed under `roots/` with nothing
  pointing at them.
- THE LAW: a commit hook must never cold-start a daemon or cold-build a db —
  blank-db roots take the inline path or skip. A root that is not registered in
  `roots.json` must never have a db built for it as a side effect of a check.
- THE RAIL: enforced. `hook::refuse_worktree_cold_check(&root)` (src/hook.rs:461,
  with the `is_linked_worktree` helper at :448) returns `Some(reason)` only when
  ALL of: the root's `.git` is a FILE (linked worktree), its `roots/<key>/db.sqlite`
  is blank or absent, and the root is absent from `roots.json` (canonicalized
  compare). The `cli.check || cli.diag_json` arm calls it at src/cli/mod.rs:476 —
  before the stale-binary check and before any db work — prints `[check] {reason}`
  to stderr and exits 0: green-by-skip, never a hook-blocking exit 2. Skipped when
  an explicit `--db` was passed. `DL_ALLOW_WORKTREE_COLD=1` is the escape hatch and
  runs the real check. Fail-pre-fix test: tests/it/worktree_cold_check.rs — 3 tests
  (skip-in-worktree asserting zero bytes written under `roots/`, escape-hatch
  builds, main-checkout `.git` dir untouched).
- DETECTION SIDE: `dl daemon health` (src/cli/health.rs) reports orphan `roots/`
  dirs — directories present under `roots/` with no matching entry in
  `roots.json` — so an orphan minted before this rail, or by any other path,
  surfaces without a manual `du` sweep. See docs/daemon.md.
- SAY THIS TO AN AGENT: the rail exists — a `--check` in an unregistered linked
  worktree with a cold db skips green and builds nothing. If you need it to
  really run there, register the root (`dl daemon start` inside it), run from
  the main checkout, or set `DL_ALLOW_WORKTREE_COLD=1`. Never widen the guard
  into a path that lets a hook cold-build. Run `dl daemon health` to find
  orphan root dbs.

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
- STEP-4a NULL-IN-PK INCIDENT (2026-07-18, caught in-arc, pre-land): the diet's
  step 4a puts `WITHOUT ROWID` on pure junction rels to drop the full-row PK
  autoindex twin. The FIRST classifier judged by shape alone (no `key()`,
  all-INTEGER, 2..=4 columns) and silently broke tests/it/named_args.rs
  `named_args_in_a_rule_head_resolve`: `WITHOUT ROWID` requires every PK
  column NOT NULL, a plain rowid table's composite PK tolerates NULL (SQLite
  treats it as always-distinct there), and the `.dl` surface's named-arg
  partial-head padding (`person(name: "z").` leaves `age` NULL) can put a NULL
  in any column of any parsed rel — `INSERT OR IGNORE` then silently DROPPED
  the NULL-padded row. RCA: the classifier conflated storage shape with a
  nullability contract the parser cannot give. The rail: an explicit per-decl
  `pk_never_null` vouch on `RelDecl` (src/ast.rs), default `false` for every
  `.dl`-parsed decl, set `true` only at Rust construction sites whose insert
  path is audited to push fixed-arity non-`Option` rows (dataflow decls in
  src/engine/decls.rs, scip decls in src/rels/scip.rs, each with the push site
  named in a comment); `wants_without_rowid` (src/engine/declare.rs) checks
  the vouch FIRST. Fail-pre-fix: the named_args test IS the discriminating
  test (it failed on the shape-only classifier); the vouch gate is pinned by
  classifier unit tests beside `wants_without_rowid` and end-to-end by
  tests/it/storage_diet_without_rowid.rs (vouched builtin gets WITHOUT ROWID
  and no autoindex, an unvouched `.dl` rel of the identical shape stays a
  rowid table, plus a byte receipt vs the pre-4a DDL shape).

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
  The sg/ast_yaml residual closed 2026-07-19: those ops parsed their own
  internal ast-grep root per rule; now a per-file `SgRootCache` (embedded in
  `AstTreeCache`, sibling map — the two grammar families cannot share one tree
  object) gives one ast-grep parse per (file, grammar) per tick, pinned by
  `work_path_parses_once_per_grammar_across_sg_rules` (fail-pre-fix at 3
  parses for 1) over `SG_PARSE_COUNT`.
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

## 20. Phantom extract diff, whole-program derived rebuild

- WHAT IT LOOKS LIKE: every tick is expensive no matter how small the change
  — 271 `[derived] full wipe` lines per tick on a 6-path edit, N+1 counter
  screaming on completion marks, deploy and start costs that "everything
  plausibly explains" (swap, disk, cold extract) while the real generator
  stays silent.
- HOW IT BIT US (2026-07-18, full chain in docs/rca-phantom-extract-diff.md):
  `eval_extract_rules` compared a `SELECT *` before-snapshot (which includes
  the `__src` bookkeeping column, src/engine/declare.rs:290) against
  declared-columns-only after-rows, and encoded NULL as `"t"` on one side,
  `"n"` on the other (src/engine/reconcile.rs:487,501). Any non-empty json
  term-extract head (armed here by .dl/git-graph.dl:55-57,120-126) made
  `extract_changed` permanently true, which took the escalation arm at
  src/engine/tick.rs:879-890: unconditional `rebuild_derived` over all 271
  pre-stratum rels, every full tick — bypassing the fully-working
  `affected_derived` scoping (tick.rs:859-873, strata.rs:332-361) forever.
  Receipt: 5,521 full-wipe lines in one short session, ~7MB daemon.log in
  2 minutes, engine lock held for whole passes (feeding class 19's exhaust
  and the class-21 client freeze).
- THE LAW: a change-detector's two sides share one projection and one
  encoding, and every steady state carries a test: nothing changed ⇒
  nothing rebuilt. An escalation arm that bypasses scoping gets its own
  fail-pre-fix coverage; testing only the machinery it bypasses proves
  nothing.
- THE RAIL: enforced. Fix f9414e3c (explicit declared-column projection,
  `"n"` NULL both sides) + fail-pre-fix it-test
  `f_term_extract_steady_state_does_not_force_full_rebuild`
  (tests/it/tick_digest.rs) — failed pre-fix with
  `got ["payload","downstream"]` on an unchanged tick. Companion c3148d90
  keys the `_derived_complete` crash-rail marks per rel-set
  (`Db::insert_rows_keyed`) so the N+1 counter stops false-screaming there;
  its witness failed pre-fix with `("INSERT _derived_complete", 71)`.
- SAY THIS TO AN AGENT: when every tick full-rebuilds the derived layer,
  don't profile the rebuild — find who set the flag that forced it, and ask
  whether the two sides of that comparison could EVER be equal.

## 21. Client poll blocking its own UI thread (instant)

- WHAT IT LOOKS LIKE: the client app freezes solid the instant the daemon
  starts — immediate, deterministic, every time — and is fine the moment
  the daemon is absent. Reads as "dl kills instant"; is instant freezing
  itself.
- HOW IT BIT US (2026-07-18, all week in lesser forms): instant's
  `sprefa_ping` poll (every 4s) was a sync `#[tauri::command]` — Tauri 2
  runs those on the MAIN thread — doing blocking socket I/O with a 10s read
  timeout (src-tauri/src/sprefa_plugin/commands.rs:104), byte-per-syscall
  reads (:60). No daemon: connect fails in µs, harmless. Daemon up but slow
  (engine lock held, class 20): read blocks 10s on the main thread; 10s
  block > 4s period ⇒ continuously frozen. Same class ran the whole status
  loop on the main thread: `list_sessions` (tmux), `rogue_agent_sessions`
  (ps+lsof+tmux), `cdp_status` (blocking HTTP) every 4-8s — the baseline
  "chokes while merely open" under load.
- THE LAW: a UI-process poll never shares a thread with the UI, and a
  liveness probe's timeout is shorter than its period — otherwise the probe
  IS the outage. A daemon being slow must degrade the client's data
  freshness, never its input loop.
- THE RAIL: partial, lives in the instant repo. All 5 `sprefa_*` commands
  plus the 3 poll probes converted to async + `spawn_blocking`; ping
  timeout 10s→1s (dl's ping handler is lock-free — daemon.rs:1878 reads an
  atomic — so slow ping means wedged, and the status row should say so).
  Committed + pushed to instant main as 74d6d36 (2026-07-18). Gaps: no
  regression test pins "poll never blocks main"; ~40 remaining sync
  commands are on-demand, not polled, unconverted.
- SAY THIS TO AN AGENT: "app freezes when X starts" means find the client
  code that blocks on X, and check what thread it runs on, before touching
  X at all.

## 22. Effect-free root frozen unsettled (await-settle hang)

- WHAT IT LOOKS LIKE: `dl daemon status` shows a root "active" forever with
  the daemon idle and no job rows for it; `dl daemon await-settle` times out
  (exit 3) on that root while every other root settles in seconds. No load,
  no errors, no log lines after "watcher ready".
- HOW IT BIT US (2026-07-18, first supervised-redeploy receipt run): the
  smashy root — a pure-derived program with no `@async`/`@stream` rules —
  booted clean (cold tick 0 in 2.7s) and then never settled. The boot tick
  reports unsettled BY DESIGN (`TickReport::is_settled()` requires
  changed_rels timer-only; a boot tick changes everything), and quiescence
  is only confirmed by one more full tick that sees nothing move. But
  `poll_scan` (src/daemon/shell/timers.rs) gated enqueue on
  `sr.has_effects()` first, so an effect-free root never received that
  confirming tick: settled froze at false. Pre-dated the apalis migration
  (same gate in the bespoke queue); latent until a served root had zero
  effect rules.
- THE LAW: every gate that skips a root's tick must preserve the
  settle-confirmation obligation — a not-yet-settled root owes one full
  tick no matter what other conditions say there is nothing to do.
  `settled` may only stay false while some path still schedules the tick
  that can flip it.
- THE RAIL: enforced. `poll_scan` skips an effect-free root only once it is
  also settled; it-test `effect_free_root_settles_after_boot`
  (tests/it/daemon.rs) pins await-settle exit 0 + settled=true for a
  pure-derived program, failed pre-fix.
- SAY THIS TO AN AGENT: "await-settle hangs on one root" means find which
  scheduler gate excludes that root from polling, and check whether the
  excluded state can ever self-resolve without the thing the gate skips.

## 23. One-shot positional swallowed by the daemon's program set

- WHAT IT LOOKS LIKE: `dl some-file.dl` inside a root a daemon serves prints
  query results for rels the file never declares — another program's `?`
  blocks, with no warning — and the file's own queries never run. The output
  is well-formed, so nothing looks broken until a rel name gives it away.
- HOW IT BIT US (2026-07-18 late night): a scratch sym-format probe returned
  `.dl/call-refresh-map.dl`'s `map_count`/`focus_site` rows instead of its
  own two probe rels, from an outside-root path AND from an in-repo path;
  only an explicit isolated `--db` produced the real answer. RCA: `run_file`'s
  daemon gate (src/lib.rs) attaches whenever a daemon serves the root and
  there is at most one positional; `run_file_via_daemon` sends only
  `{"root"}` in the query RPC, and `ensure_daemon`
  (src/daemon/client.rs:54) discards its `program` argument — the positional
  file never reaches the daemon, and the response is the watched
  `.dl/*.dl` set's cached query results. The multi-file comment above the
  gate ("the daemon serves its own loaded program set, not the positionals")
  documents the split for >1 positional but the single-positional case falls
  into the same hole silently.
- THE LAW: a one-shot given a positional program either evaluates THAT
  program or refuses loudly. Substituting a different program's results is a
  wrong-answer bug, not a fallback.
- THE RAIL: missing — the fix is owned by the erase-no-daemon-split arc (one
  server code path; the daemon RPC must carry the positional program or the
  CLI must fall through to in-process when the positional is not the root's
  watched set). Interim workaround: pass an isolated `--db` (opts out to
  in-process against that file).
- SAY THIS TO AN AGENT: "dl printed rels my file does not declare" means the
  daemon answered with its watched program set — use an isolated `--db`, and
  do not trust any prior one-shot output produced without one under a live
  daemon.

## 24. TLS-parked span guard dropped during thread-local destruction

- WHAT IT LOOKS LIKE: a hard process abort (exit 101, "thread local panicked
  on drop") when a thread dies after an erroring/aborted tick with a global
  tracing subscriber installed — no assert, no test-code frame, flaky-looking
  because it needs the thread to die with a span still entered.
- HOW IT BIT US (2026-07-19, found during the reqid-midtick arc; pre-existing
  at base ca0cd8e3, verified by a stash run where the base lib suite aborted):
  activity.rs parks `tracing::span::EnteredSpan` guards in thread-local cells
  (TICK_SPAN/PHASE_SPAN — they are `!Send`, TLS is the correct home). A tick
  that errored before `end_tick` left the span entered; when the thread died,
  the TLS destructor dropped the `EnteredSpan`, whose drop calls
  `Subscriber::exit`, and tracing-subscriber's registry reaches back into its
  OWN thread-locals — already mid-destruction — and the process aborts.
- THE LAW: a guard parked in TLS must never run its real destructor during
  thread-local destruction. Abnormal thread death LEAKS the span (an unclosed
  registry slot is the same debris a SIGKILL leaves); only the normal exit
  path closes it.
- THE RAIL: enforced (9ddf1280): `ParkedSpan =
  ManuallyDrop<tracing::span::EnteredSpan>`, every normal path exits through
  `exit_span` (un-wrap then `exit()`), and the tick abort path calls
  `exit_thread_spans()` explicitly; the cancel-abort tests exercise the
  abnormal path under a live subscriber (src/jobq/tests_cancel.rs).
- SAY THIS TO AN AGENT: an exit-101 abort at thread death with tracing
  installed means a TLS destructor reached a subscriber — park `!Send` guards
  in `ManuallyDrop` and close them only on explicit paths, leaking on
  abnormal death.

## 25. Redundant write volume on unchanged content-derived data

- WHAT IT LOOKS LIKE: a daemon restart against a WARM db, with exactly ONE
  source file changed, still offers close to the entire interned working set
  to `INSERT OR IGNORE` every tick — SQLite silently discards nearly all of
  it, but the engine paid the row-construction, statement-build, and B-tree
  probe cost for every discarded row anyway. Nothing crashes and nothing
  looks wrong from the query surface; the only visible symptom is a rebuild
  writing far more disk than the database it produces.
- HOW IT BIT US (2026-07-19, measured via the new `events.jsonl` IO trail):
  `dl daemon events` recorded, on a one-file-changed warm restart, 1,207,064
  rows offered to `_strings` with only 146 accepted (99.99% waste); across
  all tables the same restart offered 1,777,188 rows and landed 555,896
  (68.7% wasted). This is a large share of why a rebuild writes 7.7GB of disk
  to produce an 893MB database. RCA: `Engine::insert_spine_strings`
  (src/engine/meta.rs:1490) iterates every `Value::Text` cell across the rows
  being written, queues each into a fresh `spine::SymSink`, and calls
  `Db::flush_syms`. `StringId` is content-derived (`StringId::of` hashes the
  text, src/spine.rs:52) — a string persisted in an earlier tick re-hashes to
  the identical id every later tick, and its `INSERT OR IGNORE` is silently
  discarded. Nothing anywhere remembered which ids were already durable, so
  every tick re-offered the entire working set from scratch. THE DETECTION
  GAP IS PART OF THE STORY: this was invisible before the IO event trail
  existed — a per-table row count alone (`rows` in `_write_ledger`) never
  distinguished "offered" from "affected", so 99.99% waste read identically
  to a healthy write in every prior receipt. `dl daemon events` recording
  offered-vs-affected per batch call is what finally surfaced it.
- THE LAW: a write path over content-derived ids must remember, per process
  per database, which ids it has already durably committed, and skip
  re-offering them — offering an id already known present is pure waste with
  zero correctness benefit, and the waste compounds every tick a working set
  stays warm.
- THE RAIL: enforced. `Db::flush_syms` (src/db.rs) now holds
  `persisted_strings: RefCell<HashSet<i64>>` on `Db` itself — one per served
  root's connection, never a global static, so root B can never skip an
  insert root A made into a DIFFERENT database file. The FULL pending set is
  still deduped and collision-checked exactly as before (two different texts
  hashing to the same id is still a loud bail) BEFORE any cache filtering
  narrows it down — filtering first would let a same-id different-text
  collision hide behind a cache hit. An id is added to the cache ONLY when
  the flush that wrote it ran with SQLite in autocommit mode at entry (no
  caller-owned transaction wrapping it): in that case `insert_rows` either
  begins+commits its own transaction (multi-chunk path) or issues one
  auto-committing statement (small-batch path), so an `Ok` return proves
  every attempted id is durably in `_strings`. A flush riding inside a
  caller-owned transaction is NOT cached — that caller can still roll back
  after `flush_syms` returns, which would silently revert the row while the
  cache kept claiming it durable; skipping the cache there just means those
  ids get re-offered next time, never wrongly skipped. The cache is capped
  (`STRING_CACHE_CAP` = 4,000,000) and clears itself whole if exceeded — a
  cleared cache only causes re-offering, never a wrongly-skipped insert.
  Tests: `flush_syms_skips_ids_already_persisted`,
  `flush_syms_does_not_cache_ids_from_a_caller_owned_transaction_that_rolls_back`,
  `flush_syms_collision_guard_fires_even_when_the_id_is_already_cached`
  (src/db.rs).
- SAY THIS TO AN AGENT: a content-derived id (hash-of-text, hash-of-coord)
  that gets re-offered to `INSERT OR IGNORE` every tick even when unchanged
  is pure waste, not a correctness issue — the fix is a process-local
  already-persisted cache on the connection-owning type, populated ONLY from
  writes provable to have committed (check autocommit state before the call,
  not after), never a global static across roots, and never skip the
  within-batch collision guard by filtering before it runs.

## 26. Composite key minted by string concatenation, stored as an id

- WHAT IT LOOKS LIKE: two logically separate values (a raw id and a
  disambiguator like a rev, or two other fields that together identify a
  row) folded into ONE string via `format!`, then that string persisted as
  an `id`/`key`/`sym` column — instead of a composite PRIMARY KEY over two
  real columns. The fold is invisible at the call site (it typechecks, it
  round-trips, nothing crashes) and only shows up as damage downstream: rows
  that should join on identity can't, and the `_strings` interning table
  carries a full extra copy of every value for each folded variant.
- HOW IT BIT US (2026-07-19/20): `salt_rev` (src/engine/extract/mod.rs:978,
  `fn salt_rev(id: &str, rev: &str) -> String { format!("{rev}\u{1}{id}") }`)
  folded a (rev, raw id) pair into one TEXT column instead of a composite
  PRIMARY KEY, written into `rel_df_node_rev.id` and four sibling `_rev`
  twins (`df_node_repo_rev`, `df_arg_rev`, `df_field_rev`, `df_lit_rev`).
  Measured damage: 314,892 of 939,845 `_strings` rows (33.5%, 15MB) existed
  only to hold that one folded form, and `rel_df_node`/`rel_df_node_rev` had
  ZERO joinable rows on `id` despite describing the same nodes. A dataflow
  rail (`df_lit`/`df_edge`) cannot see this: checked against the live root db
  before writing the rail, `df_lit` holds 13,438 `lit` rows, 40 `template`,
  22 `concat`, and 0 rows whose text contains `{`/`}` — Rust `format!` is not
  lifted into `df_lit` at all (the 62 template/concat rows are TS-only), so
  the detector had to be structural (`sg` over the Rust AST), not a
  dataflow query.
- THE LAW: a value that identifies a row by combining two or more distinct
  fields belongs in a composite PRIMARY KEY over separate columns, never in
  one TEXT column built by string concatenation — the fold is unrecoverable
  to a query planner even when it is "recoverable by eye."
- THE RAIL: half (warn-tier, advisory, tunable). `.dl/composite-key-string.dl`,
  three arms unioned: (1) a `format!` bound via `let $NAME = format!(...)`
  where `$NAME` reads as `(?i)\b(id|key|sym|handle|slug)\b`, or sitting
  inside a fn whose OWN last `::`-segment reads the same way (anchored so
  `_id`/`_key`/... must be a whole underscore-delimited segment, not a bare
  substring — unanchored, `validate_brands` and
  `install_busy_verdict_handler` false-matched on "id" inside "valid" and
  "handle" inside "handler"), with the format string interpolating 2+
  holes; (2) a control-character separator (`\u{0}`/`\u{1}`/`\x1f`) mixed
  with 2+ holes in one format string — `salt_rev` fires here, at
  src/engine/extract/mod.rs:979 (the `format!` line itself, one below the fn
  signature cited above), confirmed via a HEAD-scoped run after the fn was
  deleted from WORK by an unrelated concurrent edit mid-session. `::` was
  measured and dropped from arm 2: at full-repo scale it produced 39 hits,
  37 of them the repo's own accepted `sym` convention
  (`{repo}::{file}::{kind}::{name}`), which would have told a contributor to
  un-fix the engine's own idiom. Arm 3 (join a `RelDecl` column literally
  named `id`/`*_id` to the writer expression that fills it) is NOT
  implemented: `RelDecl` schemas are static name/type pairs with no
  structural link to the `Vec<Value>`-by-position code that fills them
  elsewhere, and dl's regex constraint takes a literal pattern, not a second
  bound text column, so there is no `contains(text, text)` operator to fall
  back on even for the weak textual-mention case. Baseline ratchet (mirrors
  `.dl/no-new-eprintln.dl`): 14 live findings on WORK, one per file, at zero
  drift. One of the 14 (src/engine/family/mod.rs:233,
  `format!("{rel}\u{1}{col_list}")` as a cache key) is a genuine NEW instance
  of the same shape that landed in a file this rail's author does not own,
  in the SAME session the rail was written — the detector fired on a live
  contributor site, not only the motivating one. Three of `src/effect.rs`'s
  hits (lines 527/612/644) are the one FALSE-POSITIVE class found: their
  folded string feeds `blake3::hash(...)` immediately, never reads back as
  a raw id, so the fold produces an opaque content digest, not an
  unsplittable composite key — a genuinely different, accepted pattern the
  message wording does not yet distinguish.
- SAY THIS TO AN AGENT: never fold two identifying fields into one string
  with `format!` and store the result as an id/key column — declare a
  composite PRIMARY KEY over the separate columns instead; a control
  character or delimiter chosen specifically so it "never occurs" in either
  field is the loudest possible tell that this is happening.

## 27. Empty input read recorded as no dependency

- WHAT IT LOOKS LIKE: an incremental engine builds a unit's dependency set by
  projecting the per-row reads it observed during a derive. A relation that was
  scanned but held ZERO rows contributes zero observed reads, so the projected
  set omits it, and omission is indistinguishable from "this unit never read
  that relation". The unit then never reruns when the relation goes empty ->
  populated, and its public output stays stale forever. Nothing errors; the
  stale value is a well-formed empty answer, so every downstream consumer agrees
  with it.
- HOW IT BIT US (2026-07-16, found 2026-07-20): `rel_footprint`
  (src/engine/family/router.rs:213) computed a call family's reactive footprint
  as `deps.iter().map(|d| d.rel).collect()` over the `DepKey`s `Ctx::scan`
  recorded. On the tick where `_call_def` was empty, the `CallDef` / `CallName`
  / `CallDefRev` families derived over it, recorded zero `DepKey`s for it, and
  memoized a footprint that did not name `_call_def`. The next tick inserted a
  fn, `_call_def` gained a row, and `react` skipped all three families because
  the changed-set did not intersect their footprints. Public `call_def`,
  `call_name` and `call_def_rev` stayed EMPTY against a tree that defined a
  function. Minimal sequence, three ticks: seed `m0.rs` defining `f0`; delete
  `m0.rs` and add an empty `m1.rs`; add `f1` to `m1.rs`. Measured on the
  pre-fix line: 0 incremental rows against 1 oracle row for `call_def`.
- THE LAW: reading a relation is a dependency on that relation whatever its
  cardinality. A dependency set derived only from observed rows must be unioned
  with the set of relations the unit declared it reads, so that the empty case
  and the never-touched case stay distinguishable.
- THE RAIL: enforced, two layers. (1) The union lives inside `rel_footprint`
  itself, the single helper that `cold` / `react` / `react_deltas` all build
  memos through, so no caller can construct a `FamilyMemo` with a
  projection-only footprint; the exact `DepKey`-count rails in
  src/engine/storage/call.rs are untouched by it. (2) Two tests, both proven
  fail-pre-fix by deleting the `rels.extend(family.input_rels()...)` line:
  `tests/it/retraction_props.rs::empty_input_rel_still_reruns_the_family_after_insert`
  is the deterministic three-tick pin (fails at
  tests/it/retraction_props.rs:526 pre-fix, 0.19s), and the T4 equivalence
  property `equivalence_and_memo_hold_at_every_step` replays the shrinking seed
  `cc 0d80eca002b18e65b3098c2eb6b2308ccd1b0ba5edb79c527e349b5751592c9e` from
  tests/proptest-regressions/retraction_props.txt on every run. The property is
  what found the defect; the deterministic test is what keeps it found, since a
  20-case random draw is not guaranteed to regenerate the shape. Residual gap:
  the union is a discipline inside one helper, not a checkable rail. No `.dl`
  rail asserts that every incremental-unit dependency set is built from declared
  inputs rather than observed reads, so a second unit type added later can
  reintroduce the same shape without tripping anything.
- SAY THIS TO AN AGENT: when you compute what a cached computation depends on,
  never derive that set purely from the rows it happened to see. An empty read
  returns the same evidence as no read at all. Union in the declared inputs, and
  keep the union inside the one constructor every caller goes through.

## 28. Clean-tree-only code path masked by an always-dirty verification tree

- WHAT IT LOOKS LIKE: a rev-resolution or git-object code path exists in the
  source, but the local tree used to "verify" is always dirty (uncommitted
  changes, a live agent worktree), so the alias that would exercise the
  clean-tree branch never resolves that way, and a passing test suite ships a
  branch nobody ran.
- HOW IT BIT US: commit a0a0ff25 made `WORK` resolve to a bare git oid on a
  clean tree, routing `read_content` (src/engine/mod.rs) through the
  git-object path for the first time. Two bugs surfaced, both masked on a
  dirty tree because the `+` dirty suffix always routes reads to the
  filesystem: (1) `git_batch_read` (src/engine/repo.rs:1220) wrote
  `<rev>:<path>` to `git cat-file --batch`, which git resolves from the REPO
  root, so a nested scan root (bench/flow, a crate subdir) failed every
  historical read; (2) a scan root outside any git work tree (a temp-dir
  corpus) has no object database, so a bare-oid rev could not resolve there
  at all. The commit's own "962 passed" gate was measured on a DIRTY tree,
  where neither branch runs. Under `DL_REV_OVERRIDE=<clean-sha>` the it-suite
  went from 500 failed to 45 — the residual 45 collapse HEAD and WORK to one
  value, a probe artifact of the override itself, not an engine fault
  (spot-verified: `module_edge_rev_keeps_head_and_work_separate` passes on a
  normal run).
- THE LAW: a rev-resolution branch that only fires on a clean tree needs its
  own clean-tree measurement, or a forced probe (`DL_REV_OVERRIDE`) — a green
  suite measured exclusively on a dirty tree proves nothing about the
  clean-tree path.
- THE RAIL: half. Fix a7350f95 — `git_batch_read` now writes `<rev>:./<path>`
  (cwd-relative, cwd pinned to `root` via `-C`), and `read_content` checks
  `git rev-parse --is-inside-work-tree` (`root_is_inside_work_tree`,
  src/engine/repo.rs:1203, cached per root) and reads from disk when there is
  no work tree. tests/it/rev_alias_leak.rs pins the alias-resolution
  behavior. The DETECTION-gap rail — a clean-rev CI condition distinct from
  the dirty working tree every local run measures — does NOT exist yet;
  nothing in CI forces `DL_REV_OVERRIDE` or a clean checkout, so a future
  clean-tree-only branch can ship un-exercised the same way.
- SAY THIS TO AN AGENT: before trusting an "N passed" gate on rev-resolution
  or git-object code, check whether the tree was dirty when it ran — the `+`
  suffix silently disables the entire git-object branch, and a passing suite
  on a dirty tree says nothing about it. Force the branch with
  `DL_REV_OVERRIDE=<clean-sha>` or a clean checkout before believing the gate.

## 29. Read-shaped CLI flag silently retargets --db to the real served root

- WHAT IT LOOKS LIKE: `dl <file> --check` / `--diag-json` / `--lsp`, run
  without an explicit `--db`, ticks against the user's REAL per-root database
  (`~/.local/state/sprefa/roots/<key>/db.sqlite`) instead of an isolated or
  in-memory db — a command that reads like "just check this file" silently
  writes into live analysis state.
- HOW IT BIT US: an agent ran `dl <file> --check` from a worktree under the
  daemon-enabled default. `want_default` (src/cli/mod.rs:421-422:
  `programs.is_empty() || (daemon_on && (cli.lsp || cli.check ||
  cli.diag_json))`) defaults `--db` to the real per-root file whenever the
  daemon is enabled and any of `--lsp`/`--check`/`--diag-json` is set — the
  empty-positional case (a genuine "attach to the served root" request) and
  the "I gave a file, just check it" case share the same default. The tick
  ran against that file and narrowed its cache: reconcile dropped source rows
  for every path the worktree's own (partial) scan did not cover, reducing an
  ~860MB analysis cache to ~0.6MB of rel tables + freelist. The damage is a
  regenerable analysis cache, not source or git data — but it is a silent
  write to the user's real db from a read-shaped command.
- THE LAW: a read-shaped, file-scoped invocation (a positional program under
  `--check`) never defaults onto the daemon's real served-root db — only the
  discovery mode (no positional, `programs.is_empty()`) may attach to it.
- THE RAIL: enforced (2026-07-21). The db-defaulting block (src/cli/mod.rs)
  no longer keys on `want_default`. A file-scoped read-shaped run (a positional
  program under `--check`/`--diag-json`/`--lsp` with no `--db`) now defaults to
  a CONCRETE `:memory:` db (not the real per-root file, and not `None`). The
  concrete `:memory:` is critical: `run_file`/`run_check` daemon
  eligibility keys on `db_path.is_none() || db_defaulted`, so `:memory:` also
  keeps the run off the daemon that would otherwise tick+write the real served
  db. Attaching to the real root is now OPT-IN: the no-positional discovery
  mode (`programs.is_empty()`), or the new `--attach` flag, which restores the
  warm-cache path on purpose. Explicit `--db` still wins. Tests:
  tests/it/hermetic_state.rs (`file_scoped_check_is_ephemeral_no_root_db`
  asserts zero per-root db is minted; `file_scoped_check_parity_daemon_vs_no_daemon`
  asserts the same verdict with a daemon up and forced in-process).
- SAY THIS TO AN AGENT: `dl <file> --check`/`--diag-json`/`--lsp` is hermetic
  by default now (ephemeral `:memory:`), so it no longer narrows the real
  cache. Pass `--attach` only when you deliberately want the warm served-root
  db. Sandbox any run with `DL_STATE_DIR=<scratch>` (now honored — see class
  30); belt-and-suspenders is `DL_STATE_DIR` + `XDG_STATE_HOME` + `HOME`.

## 30. Sandbox knob `DL_STATE_DIR` was read nowhere — every run hit the real home

- WHAT IT LOOKS LIKE: the whole agent/test fleet was instructed to isolate `dl`
  runs with `DL_STATE_DIR=<scratch>`. `DL_STATE_DIR` was not a recognized env
  var. The only state-home knob `daemon::daemon_home()` honored was
  `XDG_STATE_HOME`. Every "sandboxed" run resolved the state home to the real
  `~/.local/state/sprefa`, ticking the real per-root db and minting orphan root
  dirs, while the operator believed the run was isolated.
- HOW IT BIT US: agent-worktree and commit-hook runs carrying `DL_STATE_DIR`
  (and nothing else) wrote the shared root db. Combined with class 29, a
  worktree `--check` believing it was sandboxed rebuilt the sprefa root db to
  833MB from partial scans.
- THE LAW: there is exactly ONE env override for the state home and it is
  honored. Precedence: explicit `--db`/`--attach` (db-level) > `DL_STATE_DIR`
  (the sprefa dir itself) > `XDG_STATE_HOME` (`<XDG>/sprefa`) > platform default
  (`~/.local/state/sprefa`). Resolution lives in one function
  (`daemon::daemon_home`, src/daemon/home.rs); no caller reads the env directly
  to build a state path.
- THE RAIL: enforced (2026-07-21). `daemon_home()` now reads `DL_STATE_DIR`
  first (src/daemon/home.rs). `.dl/state-home-single-source.dl` (warning
  severity, grandfather-baseline ratchet) flags any NEW `env::var*` read of
  `XDG_STATE_HOME`/`DL_STATE_DIR` outside the resolver — the scattered-knob
  shape that reintroduces a second home. Tests: tests/it/hermetic_state.rs
  (`dl_state_dir_outranks_xdg_and_receives_the_write` proves the write lands
  under `DL_STATE_DIR` and the `XDG_STATE_HOME` fallback stays empty;
  `state_home_rail_flags_a_new_reader` proves the rail fires).
- SAY THIS TO AN AGENT: sandbox with `DL_STATE_DIR=<scratch>` — it is honored
  now. For `cargo test` (which spawns many child `dl` each with its own
  per-test `XDG_STATE_HOME`), do NOT export `DL_STATE_DIR` globally: it outranks
  every test's XDG sandbox and collapses them into one shared home, breaking
  socket/roots isolation across the suite. Let the tests self-isolate via XDG.

## 31. Unbounded daemon logs (launchd redirect + a spawn-only cap)

- WHAT IT LOOKS LIKE: `~/.local/state/sprefa/launchd-stderr.log` grows without
  bound on a long-lived supervised daemon — 440MB observed, ~494MB total
  across every unrotated file under the daemon home (`launchd-stderr.log`,
  `launchd-stdout.log`, `daemon.log`, plus the newer `events.jsonl` this
  incident's report also names). Nothing truncates, nothing caps; on a
  long-running install it grows until the disk fills.
- HOW IT BIT US (2026-07-20, incident receipt from the user's real daemon
  home): `crate::daemon::init_daemon_tracing` installed an stderr `fmt` layer
  filtered at `DL_LOG`'s default of `info` — the SAME level
  `crate::trace::dl_log_layer` already writes to the size-capped
  `<home>/log/dl.log` (`crate::trace::RollingWriter`, 4MB cap, one
  generation). Under `dl daemon install` + `dl daemon start` (the launchd-
  supervised path, `crate::supervise::plist_contents`), that stderr stream is
  `dup2`'d by launchd into `launchd-stderr.log` BEFORE this binary ever runs a
  line of Rust — every `info`-level event was silently duplicated into a file
  no in-process rotator, subscriber, or crate (`tracing-appender`,
  `file-rotate`, `flexi_logger`) can ever rotate, because rotation means
  closing the writer and opening a new one, and the writer here (fd 2) is not
  a handle this process owns. Separately, `daemon::client::spawn_detached`'s
  own `daemon.log` cap (8MB, truncate-on-oversized) only ran ONCE, at spawn
  time — a long daemon run had no periodic recheck.
- BUILD-VS-BUY RECEIPT (required before any fix per CLAUDE.md's standing
  law): researched `tracing-appender` (`RollingFileAppender`,
  `Rotation::{MINUTELY,HOURLY,DAILY,NEVER}` — rotation trigger is TIME ONLY,
  no size-based variant exists; `max_log_files` bounds retained PERIODS, not
  bytes within one un-rolled period — does not match this incident's actual
  shape, a burst inside one period, and cannot touch the launchd-redirect
  file at all since the fd is opened by launchd before this binary execs),
  `file-rotate` (same fundamental limitation for the launchd file; supports
  size-based rotation for files THIS process opens, but is a second,
  lower-adoption dependency doing exactly what this repo's own
  `trace::RollingWriter` already does for dl.log/error.log/why.jsonl — no
  incident calls for touching those), `flexi_logger` (a competing logging
  FACADE, not a `tracing_subscriber::Layer`; adopting it would violate the
  standing law that logging rides `tracing` subscribers, not a parallel
  pipeline — rejected outright), and macOS `newsyslog`/OS-level rotation
  (the textbook system fix; genuinely correct for files the OS opens directly
  IF the app either restarts periodically or reopens on signal — this repo
  ships no config file and cannot enforce the user has one installed).
  Conclusion, stated plainly: no in-process rotator crate — bought or
  hand-rolled — can touch `launchd-stdout.log`/`launchd-stderr.log`, because
  the OS opens and holds that fd before the binary runs; that half of the
  fix is either (a) an OS-level `newsyslog.d` config outside this binary
  (recommended, documented, not shipped/enforced by this repo), or (b) the
  daemon periodically truncating the SAME path IN PLACE (no rename) from a
  separate fd — safe specifically because launchd opens the redirect
  `O_APPEND`, and POSIX recomputes the append offset to the current
  end-of-file on every write, so an external truncate is immediately safe
  with no signal and no reopen. (b) is the one implemented here; it is a
  single `stat`+`set_len(0)` primitive reusing the repo's existing size-cap
  idiom (`trace::RollingWriter`, `why.rs::ROTATE_BYTES`), not a general
  rotator framework — no retention windows, no compression, no multi-writer
  coordination.
- THE LAW: every daemon-owned log file — one this process writes itself, or
  one an OS redirect writes on its behalf — needs an enforced hard byte cap.
  "The daemon does not log unboundedly" is exactly the same standing
  discipline as "nothing seizes the machine" (CPU/IO/threads capped in
  `apply_daemon_budget`): a class of resource a long-lived daemon can exhaust
  gets a ceiling before the daemon ships, not a follow-up.
- THE RAIL: enforced. `src/daemon/logcap.rs` (`sweep`, `cap_in_place`):
  truncates `launchd-stdout.log`/`launchd-stderr.log`/`daemon.log` to 0 bytes
  in place once any crosses `EXTERNAL_LOG_CAP_BYTES` (8MB, matching
  `daemon.log`'s pre-existing convention). Called once at daemon boot
  (`run_daemon`, so a prior oversized file does not wait out a cadence) and
  every 30s idle-task tick thereafter
  (`daemon_shell::timers::idle_task`) — reusing the daemon's existing
  maintenance cadence, not a new timer. `init_daemon_tracing`'s stderr filter
  now defaults to `warn` (not `info`) unless `DL_LOG` is explicitly set —
  the root-cause half of the fix: `warn`-and-up already lands in the
  size-capped `error.log` too, so nothing is lost, and an explicit ask for
  more terminal verbosity (`--foreground` debugging) still widens it exactly
  as before (`stderr_filter_spec`, unit-tested). Fail-pre-fix-shaped proof:
  `truncate_in_place_is_safe_under_a_live_o_append_writer` and
  `rename_would_orphan_an_external_o_append_writer_truncate_does_not`
  (src/daemon/logcap.rs) drive a real `O_APPEND` writer through both the
  correct (truncate) and wrong (rename) mechanisms and observe the difference
  directly; `tests/it/log_cap_sweep.rs` boots a real (foreground) daemon
  against pre-seeded oversized files at the real path helpers
  (`daemon::launchd_stdout_log_path` etc., also used by
  `supervise::plist_contents` so the two can never drift) and observes the
  boot-time sweep truncate them, plus a companion test proving an under-cap
  file survives untouched.
- RESIDUAL: this rail proves the sweep runs on the right paths inside a real
  daemon process; it does not (cannot, without a real launchd install) prove
  launchd itself actually redirects stdout/stderr into those exact files —
  that wiring is `supervise::plist_contents`, exercised by
  `supervise::tests::plist_contents_carries_the_gate2_decision`, but the
  end-to-end "launchd truly holds this fd open across the sweep's truncate"
  path is unverified in CI. The `newsyslog`/OS-level complement remains
  undocumented as a config file this repo ships; only the in-process sweep is
  shipped code today.
- SAY THIS TO AN AGENT: a daemon log file with no periodic size check is an
  unbounded-growth bug waiting for a long-lived install to trigger it — for a
  file this process writes itself, extend `trace::RollingWriter`'s pattern;
  for a file an OS redirect (launchd `StandardOutPath`/`StandardErrorPath`,
  or any inherited-fd redirect) writes on the process's behalf, no in-process
  crate can rotate it — truncate the SAME path in place (never rename) from
  inside the process, and only if you have first confirmed the writer opens
  `O_APPEND`.

## 32. Normalizing a folded composite id onto a LOSSIER decomposition

- WHAT IT LOOKS LIKE: a composite identity is stored as one folded value
  (`sym = hash64("repo::file::kind::name")`, an instance of class 26), and the
  "fix" is to normalize it into a dictionary surrogate keyed on the entity's
  separate columns. The trap: those separate columns are NOT a faithful
  decomposition of the folded id, so the surrogate SILENTLY MERGES distinct
  symbols. Every equijoin then returns a subtly wrong set, and the whole test
  suite stays green (a merge changes which rows join, not whether the query
  runs).
- HOW IT BIT US (near-miss, 2026-07-21, caught before a line shipped): the
  written sym-dict plan (`plans/2026-07-21-symbol-dict-normalization.md`) keyed
  `_sym_dict` on `UNIQUE(repo, file, kind, name, parent)` — the stored
  `rel_type_entity` columns. Measured against the live 833MB root
  (`~/.local/state/sprefa/roots/fbabddda40d22347`, `immutable=1`):
  - `rel_call_def` has NO `name`/`parent` columns at all — keying on its
    columns collapses **11,737 distinct syms → 1,028** `(repo,kind,file)`
    tuples. Every same-kind callable in a file folds into one symbol.
  - **5,454** of 6,363 `rel_type_entity` syms carry an enclosing scope in the
    folded string (`::const::addMark.fact`) but store an EMPTY `parent` column;
    the `parent` column is non-empty for only 1,918 / 7,372 rows. Worked case:
    `(file=extension.ts, kind=const, name="fact", parent="")` maps to TWO syms
    — `addMark.fact` and `addTypeSeed.fact`, two consts named `fact` in two
    different functions — that the columns cannot tell apart.
  - **6,018** closure syms carry a `coord` (`::closure::<coord>`) that exists in
    no column.
- THE LAW: replacing a folded composite id with a surrogate is behavior-
  preserving IFF the new identity reproduces the old identity's partition of
  the corpus (proof in `plans/2026-07-21-sym-dict-correctness-proof.md`, §2).
  Two non-negotiables fall out: (1) resolve the surrogate at the MINT seam from
  the exact inputs that built the folded id — never from independently-populated
  rel columns (mirror `_df_node_dict`/`resolve_coord_surrogates`, which resolve
  from the node's real `(file,line,col,kind)`, not from `rel_df_node`). (2) Gate
  the migration on a build-time BIJECTION CHECK: `count(distinct new surrogate)
  == count(distinct old id)` per rel (baselines: type_entity 6363, call_def
  11737). Equal proves the partition is preserved; unequal HALTS and dumps the
  delta tuples. A silent merge passes a green suite; only the count-equality
  gate catches it.
- THE RAIL: two enforcement points specified, neither yet code (this is the
  chapter's deferred arc). (a) The bijection gate above, run per sym-bearing rel
  during the migration. (b) A JOIN-PARITY PROBE per cross-family sym join
  (`df_node.fn_sym ↔ call_def.sym`, closure `var ↔ call_def.sym`,
  `type_edge ↔ type_entity`, `call_edge ↔ call_def`): `rowcount(new) ==
  rowcount(old)`. Because join = integer equality on the surrogate, a column
  left in the old id-space while its partner moved to the new one lands in a
  disjoint integer domain and the join returns ∅ — the probe is the only thing
  that catches that silent empty-join.
- SAY THIS TO AN AGENT: before you normalize a folded composite key onto
  "separate columns", MEASURE whether those columns reproduce the folded value
  1:1 (`count(distinct folded) == count(distinct tuple)`). If they do not, the
  columns are lossy and your surrogate will merge distinct entities silently.
  Resolve the surrogate from the id's mint-time inputs, and gate the whole
  migration on a bijection count-equality check that HALTS on any delta.

## 33. Storage id-space change bypassed by a native fast path's re-hash

- WHAT IT LOOKS LIKE: a storage normalization moves a column into a new id
  space (interned cells go from raw 8-byte `StringId` hash to a dense
  `_sym_dict` surrogate). Every SQL path follows, but a BESPOKE NATIVE FAST
  PATH that reconstructs keys from decoded text keeps hashing text to the OLD
  space. Its adjacency is loaded in the new space (raw cell = dense id) while
  its seeds/masks are keyed in the old space (`StringId::of(text)` = raw hash),
  so the two never join. The walk silently returns only its seed rows; the SQL
  fixpoint stays correct. Green suite throughout — the split is a wrong ANSWER,
  not a crash.
- HOW IT BIT US (2026-07-21, caught by round-2 adversarial review — codex
  gpt-5.6-sol + an opus reviewer independently reproduced it; the author's
  green suite and the sym-dict bijection gate both missed it): the dense
  `_sym_dict` arc converted interned rel cells to dense ids. `try_native_halt_bfs`
  (`src/engine/derive.rs`) loaded edge adjacency from `tbl(edge)` (now dense)
  but keyed its frontier seeds (derive.rs:1460) and halt mask (1482) via
  `StringId::of(decoded_text).sqlite()` = raw hash, and rendered output text with
  `_strings WHERE id = <dense>` (misses; `_strings.id` is the raw hash). Result:
  every interned-edge halt-BFS reachability closure collapsed to its seeds —
  exactly the flow-panel `port_of_reach_rec`-class rels the project ships.
  Reproduction (`edge s->a->b`, `seed p->s`, unrelated halt): native returned
  `{(p,a)}`, `DL_NO_HALT_BFS=1` SQL returned `{(p,a),(p,b)}`. INVISIBLE because
  the existing `halt_bfs` fixture's recursive rule was VACUOUS (every seed hit a
  halt or leaf on the first hop, so the base rule alone produced the expected
  rows); the twin native fast path `try_native_depth_walk` had already been
  converted (it reads its head from `tbl` in dense space), so only the untouched
  path drifted.
- THE LAW: when a column's stored id space changes, EVERY consumer that keys on
  that column must move together — including native fast paths that rebuild keys
  from decoded text. A decode-then-re-hash step is a second id-space assignment
  and must target the SAME space as the stored cells (resolve text -> hash ->
  dense via the one allocator, `Db::dense_of_hash`), or it lands in a disjoint
  integer domain and every lookup misses. Audit is by GREP for the primitive
  (`StringId::of`, raw `_strings WHERE id =`) across native walkers, not by
  trusting the suite.
- THE RAIL: `tests/it/halt_bfs.rs::native_halt_bfs_recursion_matches_sql_over_dense_edges`
  — a NON-VACUOUS recursion over a dense (interned `text`) edge graph whose
  recursive rule carries rows the base never emits, asserted row-for-row against
  the `DL_NO_HALT_BFS=1` SQL fixpoint. Fails (native drops the recursive rows)
  without the dense-key resolution; the pre-existing vacuous fixture could not.
  Fail-first verified by reverting the derive.rs fix.
- SAY THIS TO AN AGENT: after changing a column's stored id space, grep every
  native/in-memory path for where it rebuilds keys from text (`StringId::of`,
  `_strings WHERE id =`) and confirm each targets the new space. Then prove it
  with a fixture whose RECURSION is non-vacuous — a vacuous recursive fixture
  passes even when the recursive step is completely broken.

## 34. Behavior-only gates let a parallel storage world ship inside the engine

- WHAT IT LOOKS LIKE: a multi-agent build ships a runtime whose every golden is
  green — http transcripts byte-stable, retraction proven live, stress at
  baseline — while the storage plane underneath is a from-scratch parallel
  world: string-keyed tables, full-tuple PKs on rowid tables (index bytes ~48%
  of the file at 41 nodes), absolute path TEXT repeated per edge row, and the
  project's own golden-gated SQLite machinery (spine surrogates, interning,
  cascade/reconcile) imported for exactly one function (`with_txn`) plus types.
  Nothing is wrong at the surface. The engine core is v5's 39x amplification
  disease rebuilt at birth.
- HOW IT BIT US (2026-07-24, v6/dl MVP slice; caught by an owner storage
  sitrep, not by any gate): the PLAN was the defect's origin — its M2 contract
  specified the naive DDL literally (`rel_<name>(cols, PRIMARY KEY(all cols))`)
  and named the store's machinery only as prose ("attach Store",
  "ingestJsonl stays the bulk path" in one comment line). Sonnet packages
  built exactly what the contract said; the orchestrator's review checked
  style laws (banned words, rx shape, numbering), not architectural reuse;
  and every acceptance gate measured BEHAVIOR at the http surface, where ids
  are invisible by design. A parallel fact plane therefore passed every gate
  that existed. Dispatched fix: M9 (wire the actual Store; spine + interning
  as the fact plane; store extended additively where seams are missing).
- THE LAW: storage schema is CONTRACT, not implementation detail. A plan that
  touches the space plane must name the exact reused symbols (imports, table
  names, id allocators) in the contract block — prose mentions do not bind
  agents. And every epic whose package owns tables carries a SCHEMA-SHAPE
  gate (dbstat: index/table ratio ceiling, bytes/row, presence of the spine
  tables), because behavior gates cannot see storage by construction — the
  better the abstraction, the more invisible the defect.
- THE RAIL: pending with M9 — the scaled-corpus dbstat gate (index bytes <=
  15% of file for the rel plane, spine tables present, no TEXT column whose
  values repeat beyond the dictionary threshold) wired into the dl suite so
  a schema regression fails a test, not an owner's eyeball.
- SAY THIS TO AN AGENT: if your package creates tables, your prompt's contract
  must show the DDL you are REPLACING reuse with, and your gates must include
  a dbstat assertion. "All goldens green" proves the surface, never the
  storage; grep what the runtime imports from the storage engine and count
  the functions — one is a smell you can smell from orbit.

## 35. Dev server outlives its spawner (EXIT-trap cleanup dies with the shell)

- WHAT IT LOOKS LIKE: `node src/main.ts` servers from days-old runs still
  listening on their ports (7192/7373/17272 found 2026-07-27), squatting a
  stale db. Compounds with the readiness-loop blindness (review finding 7):
  a later run's `curl` readiness probe accepts ANY listener, so a squatter
  can silently grade a whole suite against the wrong server and db.
- HOW IT BIT US (2026-07-27): the diff reviewer found three orphaned main.ts
  servers from earlier agent sessions; the user killed them by hand and asked
  why they existed at all. Mechanism: every server-booting script and agent
  one-off relies on cleanup in the SPAWNER — `trap stop_server EXIT`
  (goal-endurance.sh:44) or an explicit kill at the end of the shell. A bash
  EXIT trap does not run when the shell dies from an untrapped signal
  (harness timeout SIGKILL, agent session teardown), and the node child then
  reparents to launchd and keeps listening forever. Nothing inside main.ts
  notices its parent vanished; nothing at the next boot reaps.
- THE LAW: a dev server's lifetime is enforced from INSIDE the process that
  would dangle, never only in the spawner's cleanup path — under agent
  harnesses the spawner dies uncleanly by construction, so trap-based
  cleanup is best-effort, not ownership.
- THE RAIL: proposed, two halves. (a) stdin-watch in main.ts: when stdin is
  a pipe, exit on stdin EOF — parent death closes the pipe, server exits
  (the standard LSP-server pattern; zero deps). Scripts/agents then spawn
  with stdin piped. (b) goal-endurance readiness asserts the pid it spawned
  owns the port before grading (closes finding 7's squatter blindness).
  Fail-pre-fix test owed per the pipeline before either half counts.
- SAY THIS TO AN AGENT: if you boot a server, your kill path is not enough —
  assume your own shell can be SIGKILLed between boot and cleanup. Until
  rail (a) lands, verify with lsof on your port after every run, and never
  trust a readiness probe that doesn't check WHOSE pid answered.

## 36. Non-finite number spliced unquoted into SQL text (bare `NaN` reads as a column reference)

- WHAT IT LOOKS LIKE: a commit against a derived-effect rel throws
  `SQLITE_ERROR: no such column: NaN` (or `Infinity`) out of a generic SQL
  execution wrapper, with the actual failing statement invisible -- the
  driver's error object carries no SQL text at all. The message names a
  "column" that was never declared anywhere in the program, because it isn't
  a column: it's the literal text of a JS `NaN`/`Infinity` value, string-
  spliced into a `VALUES(...)` tuple UNQUOTED (`row.join(",")`), which
  SQLite's parser reads as a bare identifier rather than a number.
- HOW IT BIT US 2026-07-27 (v6/dl ghcacher expression, F7 finding -> this
  fix): a `sh` host with more than one OUTPUT-only column, whose template
  printed one value per LINE (`printf '%s\n%s\n%s' "$status" "$tag" "$body"`,
  fixtures/ghcacher.dl:52-57's real `fetch` host, mirroring examples/
  gh-cache.dl's shape), hit `parseWhitespaceColumns` (v6/dl/src/1_hosts.ts),
  which was built for a DIFFERENT convention (one ROW per LINE, whitespace-
  split WITHIN each line). For >1 output column that read the same output
  text backwards: each of the 3 lines became its own malformed "row," and
  the middle one (built from the TAG line's own text) ran `Number()` over a
  non-numeric ETag-shaped token that landed, by the parser's positional
  mapping, in the STATUS slot -- an `int` column. `Number("<etag text>")` is
  `NaN`, and `typeof NaN === "number"` slipped past `encodeSurfaceRowByColumns`'s
  existing `typeof value !== "number"` guard (3_runtime.ts) unchanged.
  `HostRunner.runEffectOnce` (1_hosts.ts:437-525) committed all 3 malformed
  rows in one batch; `sqlTuple`'s `row.join(",")` (3_runtime.ts) spliced the
  literal text `NaN` into the INSERT statement, and the tick pipeline died
  with `SQLITE_ERROR: no such column: NaN` -- fatal by the one-subscribe
  architecture's own design (main.ts's error handler calls `process.exit(1)`),
  and the first agent to chase it (ghcacher-findings.md's F7) could not even
  see the failing statement, since `LibsqlError` carries no SQL text and
  `execute$` (3_runtime.ts) did not attach any either. Fix: `parseWhitespaceColumns`
  now reads a line-count-matches-column-count, >1-output-column response as
  ONE row (line-per-column) instead of one row per line (v6/dl/src/1_hosts.ts);
  `encodeSurfaceRowByColumns`/`encodeLiteral` (3_runtime.ts) now reject a
  non-finite number by name (rel + column) before any SQL is built;
  `execute$` (3_runtime.ts) now attaches the failing statement's own text
  (truncated) to every thrown error, not just this one.
- THE LAW: a value that is `typeof "number"` is not thereby safe to splice
  into SQL text -- `NaN`/`Infinity`/`-Infinity` all pass a bare `typeof` check
  and all stringify to non-numeric tokens. Any function that ENCODES a value
  for later string-splice into a SQL statement (not a bound parameter) must
  reject non-finite numbers explicitly, by name, at the point of encoding --
  not downstream, where the failure surfaces as an opaque parser error with
  no trace back to its source. And any generic SQL-execution wrapper shared
  across a codebase owes its callers the statement text on failure: a driver
  error with no SQL attached is a self-diagnosis gap by construction
  (CLAUDE.md's self-diagnosis law), and the first hunt for this exact
  incident spent a whole session blind because of it.
- THE RAIL: `encodeSurfaceRowByColumns` and `encodeLiteral` (v6/dl/src/
  3_runtime.ts) both reject non-finite numbers with a rel+column (or
  literal-site) named error, before any SQL text exists. `execute$`
  (v6/dl/src/3_runtime.ts) wraps every statement's failure with the
  statement's own text (`SQL_ERROR_EXCERPT_LENGTH`-truncated) and the
  original driver error as `cause`. Fail-pre-fix regression tests in
  v6/dl/tests/4_hosts.test.ts (the exact ghcacher-shaped multi-line host
  response -- reproduces the original "3 rows instead of 1, `no such column:
  NaN`" failure verbatim on the pre-fix parser) and v6/dl/tests/3_runtime.test.ts
  (the guard's message, and execute$'s SQL-in-error contract).
- SAY THIS TO AN AGENT: if you are encoding a value for a hand-built SQL
  string (not a bound parameter), `typeof value === "number"` is not enough
  -- check `Number.isFinite` too, and name the rel/column in the error. If
  you own a shared SQL-execution wrapper, attach the failing statement's
  text to every error it throws; a driver's own error object cannot be
  trusted to carry it. And if a `sh` host template prints one field per LINE
  for more than one output column, know that the generic whitespace
  parser's OTHER convention (one row per line) is not what fires by
  default -- check which one actually applies before trusting either.

## 37. Green gate that cannot fail (grader printed `fail` and exited 0)

- WHAT IT LOOKS LIKE: a battery leg prints per-check `PASS`/`fail` lines and
  the recipe exits 0 either way. Every consumer that trusts the exit code
  (`&&` chains, `just green`, coordinator battery tails) reports green over a
  red fixture. The red is visible ONLY to a reader who greps the full log.
- HOW IT BIT US 2026-07-31: v6/prolog/src/grader.pl run/1 wrapped every check
  in `forall(..., (Goal -> PASS ; print fail))` -- forall cannot fail, `go`
  succeeded, `-g go -g halt` exited 0. Fixture float_shortest_round_trip_wire
  landed RED (malformed expectation: two deltas/2 terms for one rel where the
  contract is one term carrying the full tick list) and shipped through the
  landing battery, the merge battery, and two coordinator re-runs, all
  reading exit codes or output tails. Found by a docs lane running the full
  log visibly.
- LAW: a grading loop must ACCUMULATE failures and fail its goal when the
  count is nonzero; `swipl -g` then exits 1. A checker whose exit code cannot
  go red is not a gate, whatever it prints.
- RAIL: grader.pl run/1 counts failures, prints `FAILURES N`, and fails
  (fail-pre-fix receipt 2026-07-31: exit 1 + `FAILURES 1` on the red fixture,
  exit 0 after the fixture fix). Every runner riding run/1 (conformance,
  arch, labs) inherits the rail.

## 38. Unbounded compute grind (no budget, so a cliff arrives as a hang or an OOM death)

- WHAT IT LOOKS LIKE: a toolchain invocation with no time limit. It is not a
  bug until the invocation hits a cliff, and then the cliff cannot report
  itself: the caller sees silence for as long as it is willing to wait, and
  the eventual end is a crash, an OOM kill, or a human losing patience. The
  incident always reads as "it hung", which names nothing and locates nothing.
  Its close cousin, and the reason a naive fix does not work: a timeout that
  ORPHANS. `perl -e 'alarm N; exec @ARGV'` — the house one-liner on macOS,
  which ships no coreutils `timeout` — execs the command in perl's own
  process, so SIGALRM kills that one process and every child it spawned
  survives, reparented, still consuming the machine the caller thinks it
  reclaimed.
- HOW IT BIT US, three incidents, all 2026-07-30/31:
  1. a `just devlog` run hung for 35 minutes with nothing on stdout;
  2. `3_clock_check.pl`'s simple-path enumeration ground 9m40s into 8GB and
     died as a stack overflow INSIDE the served compiler — POST /program had
     no budget, so the request held open behind a live swipl and the only
     signal at the end was a crash (ARCH row clock_check_path_blowup);
  3. 236 orphaned servers accumulated from rails whose teardown never ran.
  And the orphaning-timeout receipt, measured in v6/bench-cli/bench.sh's own
  header: at a bench cell's timeout the harness moved on to the next cell
  while the timed-out swipl kept running — `03:03 swipl` (orphaned, 3 minutes
  past its 180s cap) beside `00:01 swipl` (the cell being measured) — so every
  subsequent measurement was taken against a stolen core.
- LAW (user-set 2026-07-31): every compute invocation in the toolchain runs
  under a budget with a NAMED timeout failure. No open-ended grind anywhere. A
  resource cliff is a named refusal, never a hang, never an OOM death. A
  budget that trips on today's honest wall is a mis-set budget, so every site
  states its measured wall beside its default.
- RAIL: `v6/tools/run-capped.sh`, one helper, four entry points — `run_capped`
  (fork + setpgrp + SIGALRM -> `kill -KILL -pgid`, exit 124, the coreutils
  convention), `capped` (the same plus the named failure line), `cap_self`
  (whole-script process-group cap, for the served rails whose cost is a
  background server and the subprocesses it spawns rather than a command they
  wait on), and `capped_curl` (a request that cannot outlive its budget, so a
  poll loop's own attempt counter keeps advancing). Executed rather than
  sourced it is `run_capped` as a command, which is how an `sh` host template
  inside a .dl6 program reaches it. Sites wrapped in the landing sweep:
  the served compile door (v6/tsv2/serve/0_compile.ts, `compile_timeout`
  answered as a 400 with the swipl group killed and the running program
  untouched), atlas/self-map/devlog/files/getting-started/extraction-live/
  crawl-bench/goal-endurance/leak-soak x2/memory-soak (whole-script caps),
  roundtrip/text-door/sweep (per-leg caps), the graphviz render inside
  dataflow-atlas.dl6 (per-render cap), and both bench engine runners, whose
  orphaning one-liners this closes.
  Fail-first receipts, one per wrapped class (2026-07-31, planted budgets
  below each site's honest wall): `capped` on text_door_receipt.sh ->
  `TIMEOUT text_door_receipt.sh: the term-door vs text-door swipl receipt
  exceeded 2s`, exit 124, zero surviving swipl; `cap_self` on files.sh ->
  `TIMEOUT files.sh: whole run (files) exceeded 3s; the process group was
  killed`, exit 124, zero surviving `serve/main.ts`; the command form on a
  shell that backgrounds a child -> exit 124 and the BACKGROUNDED grandchild
  dead too (the process-group leg, which the orphaning one-liner fails);
  `capped_curl` against a socket that accepts and never answers -> curl's own
  28 in 3s, where the uncapped request was still waiting at 8s; and the bench
  runner's DNF branch on a heavy cross-join cell under a 3s budget ->
  `TSV2_DNF s3/4000 warmup timeout after 3 seconds` with the JSONL row
  recording it, which is the receipt that the new 124 reads as a timeout where
  the orphaning form's 142 used to.
  v6/tsv2/tests/serveCompileBudget.test.ts carries the compile door's, with
  its own two-part sabotage receipt.
- THE MIS-SET BUDGET IS ITS OWN FAILURE, and this sweep produced one before it
  produced a rail: a uniform 30s cap on every HTTP call in the served rails
  killed `just atlas` at `program load returned 000`, because `POST /program`
  is not a poll -- it holds the connection open for the whole ~256s compile.
  Every served script therefore carries TWO budgets, `*_LOAD_BUDGET_S` (900s)
  on the load POST and `*_HTTP_BUDGET_S` (10-60s) on the polls. Stating a
  measured wall beside every default is what catches this class; running the
  rail is what catches it when the stating does not.
- RESIDUAL, named not fixed: `scripts/dl-trace.sh` and `scripts/verify.sh`
  still carry the orphaning one-liner on the v5 side, outside this sweep's
  scope.

## 39. Nested process-group cap: `cap_self` re-groups out from under the outer kill

- WHAT IT LOOKS LIKE: an outer budget fires, prints its timeout line and exits
  124 -- and the served node engine the capped script backgrounded is still
  listening. Nothing reports it. The symptom arrives one run later as
  `EADDRINUSE` on the rail's own hardcoded port, and the quieter half is worse:
  the orphan ANSWERS on that port, so the next run's readiness probe accepts it
  and grades a whole receipt against a stale server and a stale db (class 35's
  squatter blindness, now reachable through a rail that believed it was capped).
- HOW IT BIT US (2026-07-31, atlas arc): `atlas.sh` booted the tsv2 server as a
  background child (`2c08ea62^:v6/tsv2/scripts/atlas.sh:251-255`) on the fixed
  default port 17811 (:187), with cleanup in the spawner only (`stop_server` +
  `trap stop_server EXIT`, :196-203) and a whole-script `cap_self 2400` on top
  (:166-167, whose own header names the backgrounded server as the reason the
  process-group cap is the honest one, :126-135). The run was made under a
  60-SECOND OUTER cap. The outer cap fired, the server survived holding 17811,
  and the next run of the rail died at boot on `EADDRINUSE`. The only record of
  the incident itself is the arc's session log, which named it UNFILED
  (chat_log/20260731.1.fable-parse-fix-comment-sweep-flatten-atlas-death.md:
  10,21,74 -- untracked when this entry was written); the 60-second outer cap and
  the EADDRINUSE come from there, and everything below is from the code and from
  a reproduction. `atlas.sh` itself was scrapped the same night (2c08ea62) and
  takes no fix with it, because every other served rail is shaped identically.
- THE MECHANISM, which is the inverse of what class 38's rail promises:
  `run_capped` forks a child that calls `setpgrp(0, 0)` before `exec`, then on
  SIGALRM kills that whole group (`kill("KILL", -$pid)`, v6/tools/run-capped.sh:
  51-64) -- one group, everything in it, which is exactly what makes the cap
  reach a backgrounded grandchild. `cap_self` then re-execs the calling script
  THROUGH `run_capped` (v6/tools/run-capped.sh:78-92), so the re-exec'd script
  calls `setpgrp` a second time and lands in a NEW group that the outer group's
  kill cannot address. Everything that script spawns -- the background server
  included -- inherits the inner group and survives with it. The re-entry marker
  `cap_self` exports is per LABEL (:81-83): it suppresses a second cap of the
  same name and knows nothing about an outer `run_capped`, which sets no marker
  at all.
- MEASURED, both legs, 2026-08-02 (lab reproduction in scratch, nothing added to
  the tree): a script that sources run-capped.sh, calls `cap_self 120 innerlab`,
  backgrounds `sleep 300` and then sleeps, run under `run_capped 3` -> outer
  `exit=124` and the backgrounded child is ALIVE, reparented to pid 1, sharing a
  pgid with the re-exec'd bash and killable only as that inner group
  (`ps -o pid,ppid,pgid`: `53584 53548 53548 sleep 300` beside
  `53548 1 53548 bash ./inner.sh`; `kill -9 -53548` took both). The SAME script
  with the `cap_self` line removed, same outer cap -> the backgrounded child is
  DEAD, which is class 38's own process-group receipt reproducing exactly.
  `cap_self` is the whole difference.
- THE LAW: a budget may narrow the process group it will be killed with, never
  mint a new one. A script installing a whole-script cap must first ask whether
  it is already inside one and decline to re-group if it is -- the OUTERMOST cap
  owns the group, and a nested cap that escapes it manufactures orphans out of
  the one mechanism whose entire purpose is not to. Second half, independent of
  the first: a rail that boots a server on a CONSTANT port cannot tell its own
  server from a squatter, so an orphan stays silent until a bind fails, and a
  bind failure is luckier than the alternative.
- THE RAIL: missing, two halves proposed.
  (a) group honesty in `cap_self`: set a label-INDEPENDENT marker in
  `run_capped` (pid + limit of the group that owns the run) and have `cap_self`
  return without re-exec when one is present, so the outer cap kills one group
  containing everything. Note the interaction with class 38's mis-set-budget
  lesson: under (a) a 60s outer cap correctly kills a rail whose honest wall is
  minutes, which is the right answer -- an outer budget below the inner one is a
  caller error that should be loud, not an orphan factory.
  (b) port fingerprint, and the repo already solved this ON THE TS SIDE: every
  served test used to name a constant (17521, 17531, ..., 17611) and collided as
  `EADDRINUSE` the moment two lanes ran one tree (bug
  `hostdecode_hardcoded_port_collision`); `startServed` now defaults to the
  ephemeral port 0 and callers read `served.port` back
  (v6/tsv2/tests/serveHelpers.ts:135-148), `reservePort()` supplies an address
  for receipts that need one NOT listening, and the sabotage receipt pins a
  constant back and watches the third test go red in under a millisecond
  (v6/tsv2/tests/serveLifecycle.test.ts:17-22, 49-54). The 13 server-booting
  shell scripts under v6/tsv2/scripts/ have no equivalent: each names a fixed
  default port, and two already name the SAME one -- `TSV2_EXTRACTION_PORT` and
  `TSV2_SOAK_PORT` are both 17571 (extraction-live.sh:68, memory-soak.sh:26).
  `serve/main.ts` reads `TSV2_PORT` (:18) and already prints the port it actually
  bound (`tsv2 serving on <port>`, :24), so `TSV2_PORT=0` plus reading that line
  back out of `server.log` is the mechanical port of the TS fix, and it makes
  "whose pid answers" moot rather than merely checkable.
  Fail-pre-fix test owed per the pipeline before either half counts; the
  reproduction above is its shape (assert the backgrounded pid is gone after the
  outer 124 -- red on today's code).
- SAY THIS TO AN AGENT: do not wrap a `cap_self` script in another cap and
  believe the outer one -- it kills a process group the inner script has already
  left, and the served engine goes on holding its port. If you run one under an
  outer budget anyway, check the rail's port with `lsof -i :<port>` when it
  returns, and know that the next run's `EADDRINUSE` is the FIRST notification
  you will get. When you write a new served rail, take the port from the server
  instead of naming one.

## 40. An aggregate emits no row for an empty group (the `coalesce` idiom nobody wrote down)

- WHAT IT LOOKS LIKE: a rule counts or sums per group and the answer comes back
  plausible and wrong. A group with zero members produces NO ROW from the
  aggregate, so its term leaves the formula entirely instead of entering it as 0.
  Nothing is refused, nothing is logged, and nothing in the tick log disagrees --
  the missing row is missing, so no delta and no final row can name it. The
  sharper arm: a rule that JOINS an aggregate against a threshold does not fire
  at all while the set is empty, and there the cost is the whole program rather
  than a term.
- HOW IT BIT US, twice, in two independent labs:
  1. auto-factorization (2026-07-31, finding 9,
     plans/2026-07-31-auto-factorization-verdict.md:958; worked example at
     :249-263): the first modularity draft read `und_internal_total` directly
     and returned 0.0 on the file axis where the referee (networkx) says -0.0278.
     Every group with zero internal edges dropped its own negative term, and on
     the file axis that is EVERY group, so the whole negative half of Q vanished.
     Wrong in the safe-looking direction -- the number stayed plausible. The fix
     was one rel, `und_internal_filled`, a `coalesce` against the GROUP rel
     (:255-261).
  2. csp-idioms (2026-07-30, finding W3,
     plans/2026-07-30-csp-idioms-verdict.md:97-111): a semaphore whose gate
     compares `count()` against a limit grants ZERO leases, permanently.
     `count()` over the empty set yields no row, so `latest(held_count(...))`
     matches nothing before the first grant, and because nothing is granted the
     set stays empty forever. It compiles clean through BOTH doors (`bop check`
     exit 0), runs to completion, and the tick log carries only the `acquire`
     arrivals. No diagnostic anywhere. The verdict prices the standing cost: a
     `not(held_count(_))` base case is a +1 rule tax on every aggregate compared
     to a threshold (:109-111).
  Both are language-design-review finding A11 ("count never 0") landing on real
  programs, in labs a day apart, with no fixture between them that could have
  warned either.
- THE LAW: an idiom that grading cannot see is not documented by being known.
  Empty-group absence is invisible to tick-log grading AND to final-state
  grading -- the same shape as the retention and `keep(count)` gaps this ledger
  already carries -- so the only thing that can hold it is a fixture written on
  purpose. Every construct whose failure mode is a MISSING row owes one; without
  it the next lab rediscovers the class at the price of a plausible wrong number,
  which is the most expensive kind.
- THE RAIL: missing; fixture proposed, and the spelling it would pin already
  exists. `coalesce(agg_rel(Group, Total), 0)` derived over the group rel is the
  fix both labs converged on, and `coalesce/2` is live, ruled and graded
  (`null_design = get_else_use_site_never_storage`; surface row
  v6/prolog/compile/registry.pl:69; expander v6/prolog/0_coalesce_expand.pl) --
  but not for this. v6/prolog/conformance/fixtures/7_coalesce.pl carries eight
  fixtures and every source is an EDB rel or a level view
  (`coalesce_over_derived_source` reads `heavy`, a filter, :103-122); NONE reads
  an aggregate-headed rel, which is the only shape where the absent row is
  manufactured by the aggregate itself rather than by a missing arrival. The
  fixture owed follows that file's own snake_case naming
  (`coalesce_defaults_the_absent_row`,
  `coalesce_default_returns_when_source_retracts`,
  `coalesce_over_derived_source`, `coalesce_in_edge_body_samples`):
  `coalesce_fills_an_empty_aggregate_group` in 7_coalesce.pl -- a group rel with
  three groups, an aggregate deriving rows for one, final state carrying 0 for
  the other two, and a delta leg where a populated group EMPTIES and its term
  returns to 0, which is the retraction flip :77-92 already grades for a plain
  source. The auto-factorization verdict asks for exactly this promotion and says
  the shape "today exists nowhere" (:1031-1035). Doc half: SYNTAX.md's coalesce
  paragraph states the total-read semantics and never mentions aggregates
  (v6/prolog/compile/SYNTAX.md:86-101), and the generated aggregate rows
  (:138-148) say nothing about the empty group; one sentence in each is where the
  idiom stops being folklore. If a text-door program is wanted beside the term
  fixture, v6/dl/fixtures/ is kebab-case .dl6 (clock-swr-demo.dl6, diag-rail.dl6,
  door-handwritten.dl6), so `coalesce-empty-group.dl6` -- but the graded corpus
  the sweep and both doors read is the conformance file, and that is where the
  coverage gap is. The honest fix the verdict names beyond all of it is a CHECK:
  an aggregate feeding an arithmetic expression over a group rel must have a
  filled source (auto-factorization verdict :1054-1057), which is a rail rather
  than a fixture, and is unowned.
- SAY THIS TO AN AGENT: `count`/`sum`/`min`/`max` never emit a row for a group
  with no members, so any formula where an empty group still owes a term must put
  the term back by hand -- derive over the GROUP rel and wrap the aggregate as
  `coalesce(agg(Group, Value), 0)`. And if you are comparing an aggregate against
  a threshold, the empty case is not "0 vs threshold", it is NO ROW, so the rule
  does not fire at all: that one costs a whole program rather than a term, and it
  needs a `not(agg(...))` base clause instead of a default.

## 41. A recursive rule stops after one round when its arrivals land in one batch

- WHAT IT LOOKS LIKE: a transitive-closure program returns the one-hop rows and
  silently omits every multi-hop row. No error, no refusal, no tick-log
  disagreement; the tick reports `carryPending: false` and the loop exits
  believing it converged. Feed the same edges one per tick and the answer is
  correct, which is why every recursive fixture in the suite passes.
- HOW IT BIT US (2026-08-05, exec_shootout dl6 lane): the lane could not produce
  a benchmark row for the shipping engine because the pinned 3-node chain
  checksum did not match. Reproduced outside the lane through the bench-cli
  adapters, oracle against compiled engine, same program and same schedule:

  | schedule | swipl oracle | tsv2 compiled |
  |---|---|---|
  | `[[edge(1,2), edge(2,3)]]` | `reachable: (1,2) (1,3) (2,3)` | `reachable: (1,2) (2,3)` |
  | `[[edge(1,2)], [edge(2,3)]]` | same 3 rows | same 3 rows |

  The flagship program `flagship_flow_reach_over_resolved_edges.dl6` loses
  `flow_reach(app, entry, sink, persist)` under the batched schedule, so the
  defect reaches the alpha flagship and is not an int-column artifact.
- THE DEFECT CHAIN, in firing order:
  1. `applyLevelsBeforeEdges` (1_incremental.ts:783) stages level results through
     `levelFrontierCopies(false)` (1_incremental.ts:335), which writes the
     frontier table at phase 2 and never the next-frontier table.
  2. `promoteFrontiers` (1_incremental.ts:1070) reads carry from
     `EXISTS (SELECT 1 FROM __next_frontier_<rel>)`, which is therefore always
     empty, so `carryPending` is false and `TickFold` (tickLoop.ts:47) exits.
  3. The same promote then runs `DELETE FROM __frontier_<rel>` followed by an
     insert from the empty next-frontier table, so the rows a second round would
     have joined are destroyed at tick end.
  4. `recomputeLevelsAfterEdges`, the one path that does write next-frontier
     (`levelFrontierCopies(true)`), is gated on retractions and on
     `reconcileEveryTick`, both false for a positive recursive program.
- WHY NO TEST SAW IT: every recursive fixture feeds one hop per tick
  (conformance/fixtures/4_flagship_flow.pl:34-35 is two arrival ticks), and the
  emitted third join arm reads the FULL head table, so one round per tick is
  sufficient there. The suite has no case where a single arrival batch needs two
  rounds.
- FAIL-PRE-FIX TEST OWED: a fixture whose schedule is one batch containing a
  two-hop chain, graded against the oracle's three rows.
- RAIL OWED: the drain budget work (plans/2026-08-05-fixpoint-budget.md) replaces
  the tick-count cap with a work budget over the same durable worklist tables;
  the carry write is its enabling step, so the rail rides that arc.

## 42. Receipt whose wait condition is already satisfied (asserted on a value the previous step produced)

- WHAT IT LOOKS LIKE: a shell receipt posts a second demand, waits for the row
  count it ALREADY had, sleeps a fixed few seconds, then asserts. The wait
  returns instantly, the sleep is the entire budget for the host, and the
  assertion passes only because an earlier step's rows happen to satisfy it.
- HOW IT BIT US 2026-08-07: v6/tsv2/scripts/files.sh step 4 ran
  `await_rows file "$before"` with `before` = the count step 1 had just
  asserted, so `n >= want` held on the first poll. `sleep 3` then had to cover
  a host spawning one `git rev-parse` per tracked path (316 here). On a CLEAN
  tree the leg still passed, because `files` (working-tree `git hash-object`)
  and `files_at` (the blob oid at the rev) answer the SAME (path, digest) pair
  there, so the grep matched step 1's rows and `files_at` never had to answer
  at all. The leg went red the first time it ran in a tree with edits, naming
  files_at, and the host was correct: 534 rows and both digests present with a
  longer wait.
- LAW: a wait condition must name a value the step being tested produces, not
  one already on the board; and an assertion must be unsatisfiable by the
  previous step's output. Where two producers can answer identically, assert on
  the input where they must differ.
- RAIL: files.sh step 4 waits for `before + edited` rows (`git diff
  --name-only <rev>` counts the paths whose pinned row is new, and identical
  rows dedup away everywhere else) and pins its assertion to an EDITED path
  when one exists. Fail-pre-fix receipt: red on the dirty
  `dynamic-loading` tree naming `v6/tsv2/cli/0_inventory.ts` at
  `1dc4f934`, green after, with the same server and the same host.

## 43. A tracked absolute-path symlink into the tree it lives in (node_modules ELOOP)

- INCIDENT (2026-08-08 14:39): commit `9a5889a2` tracked `v6/tsv2/node_modules`
  and `v6/dl/node_modules` as symlinks whose targets are ABSOLUTE paths into the
  main tree. Checked out in the main tree itself, each becomes a self-pointing
  symlink: `readlink` = its own path, ELOOP on read. The checkout destroyed both
  real dependency directories in the main tree, and every worktree whose deps
  linked there lost them mid-run (lane I-E hit it during a bench pass). Any
  future checkout re-breaks the tree as long as the entries exist.
- RCA: two defects in firing order. (1) A lane created the symlinks to share the
  main tree's installed deps instead of running `pnpm install` in its worktree
  — an absolute-path link into another working tree is a cross-tree tether, and
  a checkout replays it anywhere. (2) The root `.gitignore` had no `node_modules/`
  pattern (only `proofs/**/node_modules/`), so `git add -A` swept the symlink
  into the commit and review read it as a one-line file.
- FIX: `git rm --cached` both entries; root `.gitignore` gains `node_modules/`.
  Main-tree repair was `rm` the two links + `pnpm install` in both packages.
- RAIL: the `.gitignore` pattern itself — proven fail-pre-fix:
  `git check-ignore v6/tsv2/node_modules` exits 1 before the pattern (trackable)
  and 0 after (ignored), so the sweep that caused (2) cannot recur. Worktree
  briefs already carry the pnpm-install step; a lane that symlinks instead of
  installing violates the worktree-dispatch law's "working around a blocked
  command" clause.

## 44. A receipt script whose findall swallows plain failure (green gate, zero comparisons)

- INCIDENT (2026-08-09): `just text-door` printed `TEXT_DOOR compiled=0
  byte_identical=0 failures=0` and exited 0 on main. It had compared ZERO
  programs since `863fe1d5` (interning arc, 2026-08-08) grew the plan term
  from `plan/7` to `plan/8`: `text_door_receipt.pl:188` destructured 7 args,
  the unification FAILED without throwing, `grade_text_door` failed, and the
  enumerating `findall/3` dropped every entry silently. 196 fixtures' term-vs-
  text byte-comparisons vanished; the PR #57-#61 wave merged behind the empty
  green gate. Surfaced by lane catalog1's deviation report (fresh worktree
  read 0/0/0; a stash run proved it pre-existing at base).
- RCA, two defects in firing order: (1) `plan/N` grew without a consumer grep;
  the receipt script consults compile.pl from outside, so no load-time arity
  error exists to catch it. (2) The findall goal had failure statuses only for
  THROWN errors — a plainly-failing `grade_one` produced no status at all, and
  the header's dynamic-count contract ("no frozen count") made 0 a legal total.
- FIX: `plan/8` destructure; the findall goal wraps `grade_one` in if-then-else
  minting `failure(Name, grade_pipeline_failed)` on plain failure. Receipt on
  one tree: `compiled=0` before, `compiled=231 byte_identical=231 failures=0`
  after (196 grew to 231 with the revival fixtures).
- RAIL: the `grade_pipeline_failed` arm — proven fail-pre-fix by reverting the
  arity fix alone: all 231 entries land in failures and the script halts 1.
  Any future `plan/N` drift turns this gate red instead of empty.

## 45. A mechanical rename silently disables an optional-field optimization (bench 4.3x + heap abort)

- INCIDENT (found 2026-08-10, entered 2026-08-07 via `4a9b45f7`): dl6
  `grid_10000` fixpoint 1182ms → 5627ms (parent-vs-culprit rerun 1260 → 5393,
  4.28x), peak RSS 621MB → 1364MB, and `DL6_BENCH_FULL=1` aborted node's heap
  on layered/chain (~10M derived rows) — the abort also truncated FACTS.md
  through bench.sh's `>` redirect. Checksums identical throughout. Undetected
  for 3 days and ~25 landed PRs; surfaced by a manual bench rerun, named by
  the dl6-perf-bisect lane in 8 measured steps.
- RCA, three defects in firing order: (1) `4a9b45f7` (534-file snake_case
  rename) renamed the runtime reader `seam.unobservedRels` →
  `unobserved_rels` (runtime/1_incremental.ts) but missed the three lab
  drivers that SET the key (dl6/bench.ts:220, incbench.ts:19, run.ts:159);
  lab drivers run under `--experimental-transform-types`, so no typechecker
  ever saw the dead literal key. (2) The skip's fail-safe direction — an
  absent `unobserved_rels` never skips — converted the miss into silent full
  delta bookkeeping: every derived row also written to
  `__delta_/__frontier_/__new_reachable`, then a 1,069,200-row `GROUP BY`
  consolidation materialized into JS row-by-row by `boundary_delta`
  (1_incremental.ts:884), which is the RSS doubling and, at 10M rows, the
  heap abort. (3) Nothing compares bench numbers: COUNT tests gate statement
  counts never time, TickStatementLedger records with no comparator,
  dl6-bench is manual and out-of-CI — so a 4.3x cliff with identical
  checksums rode 25 green PRs.
- FIX: snake_case the key in the three lab drivers; bench.sh writes FACTS.md
  temp-then-move so a crashed run cannot truncate the bank. Receipt: grid
  5627 → 1173ms, RSS 1364 → 621MB; `DL6_BENCH_FULL=1` completes (layered
  11721ms, chain 20850ms), all three checksums byte-identical to the
  2026-08-07 bank.
- RAIL: missing — the promotion is a budgeted bench cell in the battery
  (grid fixpoint time + RSS ceilings ratcheted against banked FACTS.md; the
  bisect's 2500ms threshold proves the cell discriminates: every post-culprit
  commit measured ≥ 5393ms). Second arm: a `tsc --noEmit` gate over the dl6
  lab drivers so a seam-literal key drift is a type error instead of a
  silent default.

## 46. A gate that silently inherits the ambient locale (non-ASCII fixture unwritable)

- WHAT IT LOOKS LIKE: a swipl entry point calls `open(File, write, Stream)`
  with no `encoding(...)` option, so the stream encoding comes from the
  operator's locale. Under a UTF-8 shell everything passes; under `LC_ALL=C`
  the `encoding` flag is `text` (ASCII) and the first non-ASCII byte throws
  `io_error(write, ...)` / `'Encoding cannot represent character'`.
- HOW IT BIT US: 2026-08-11, a lane running `just text-door` reported
  `TEXT_DOOR compiled=266 byte_identical=265 failures=1` on
  `json_nfc_and_nfd_keys_stay_distinct` (the NFC/NFD fixture is the only
  non-ASCII one), while the same command in the coordinator's shell printed
  `266/266/0`. The lane stopped and reported instead of improvising, so the
  cost was one stalled lane rather than a wrong verdict. Measured that day:
  `LC_ALL=C swipl` reports `encoding=text`; UTF-8 or unset reports `utf8`.
  51 `open/3` calls across `v6/prolog/**` carried no encoding option.
- THE LAW: a gate's verdict never depends on the operator's environment.
  Encoding is declared by the program, never inherited.
- THE RAIL: enforced at the hub. `v6/prolog/compile.pl` sets
  `:- set_prolog_flag(encoding, utf8)`, which is the default for every
  `open/3` in the process; every gate entry point loads that module.
  Proven fail-pre-fix: `LC_ALL=C bash prolog/compile/scripts/text_door_receipt.sh`
  printed `265/failures=1` before the flag and `266/failures=0` after, same
  command, same shell.
- SAY THIS TO AN AGENT: Never let a locale decide whether a gate passes. If
  you write a swipl entry point that does not load `compile.pl`, set the
  encoding flag yourself.

## 47. A reactive gate that reads its verdict rel before the retraction tick lands

- WHAT IT LOOKS LIKE: `comment-budget-rail.sh` posts arrivals, waits for "no
  tick event for `COMMENT_RAIL_IDLE_MS` (default 700ms)", then reads
  `/idb/violation_run`. `violation_run` rows are minted by one host round and
  RETRACTED by a later waiver-join round. A >=700ms gap between those rounds
  reads a stale finding: the gate reports a violation the program itself
  retracts one tick later.
- HOW IT BIT US: 2026-08-15, landing chore/delete-naive-arm. The same staged
  index (identical `git write-tree` digest) graded rc=0 three times standalone
  and rc=2 three times as the pre-commit hook, flagging
  `v6/tsv2/runtime/3_subscribe.ts:1-22` WITH a `@comment-ok:` waiver on line 1.
  `COMMENT_RAIL_IDLE_MS=3000` on the identical index dropped that finding and
  surfaced the two real ones (`7_scale-floor.sh`), which then waived clean.
- THE LAW: a reactive gate reads its verdict only at fixpoint. Idle-time is a
  heuristic for fixpoint and must dominate the slowest host-round gap, or the
  serve layer must expose a real quiescence signal the gate can block on.
- THE RAIL: pending — tracked as issue `comment-rail-early-read`. Until it
  lands, a rail verdict that contradicts a visible waiver is re-measured with
  `COMMENT_RAIL_IDLE_MS=3000` before anything is reworded.
- SAY THIS TO AN AGENT: if the comment rail flags a line that carries
  `@comment-ok:`, the rail raced; re-run with a longer idle window before
  touching the comment.

## 48. A lane that does its work in the main tree and commits to local main

- WHAT IT LOOKS LIKE: `boop beep lane wait` returns rc=0, the lane's own
  worktree shows ZERO commits and a clean status, yet the lane's transcript
  says "Done. Issue closed, fix committed." The commit exists — parented on
  the coordinator's local `main`, made in `~/projects/sprefa` itself, with the
  lane's `--base-sha` ignored.
- HOW IT BIT US: 2026-08-15, `fix/list-column-raw-snapshot` (pro4). The lane
  cd'd to the main tree, built the whole deliverable there (emit_ts.pl fix +
  test + 341 regenerated modules, commit d4f6abca), and advanced local main
  past the pushed head. Its assigned worktree still sat at its base sha.
  Recovery: branch reset to the stray commit, `git reset --keep` on main,
  rebase onto the intended base, one-module sweep regen (PR #282).
- THE LAW: main-tree ownership is the coordinator's only. A lane's result is
  judged in its worktree; a clean lane worktree plus a "committed" claim means
  the commit landed somewhere it must not be. Check `git branch --contains`
  on the claimed sha before calling a lane lost.
- THE RAIL: pending — boop should refuse (or at minimum flag in `lane wait`
  output) a lane session whose commits land outside its registered worktree;
  tracked as issue `lane-main-tree-escape`.
- SAY THIS TO AN AGENT: your FIRST action is `git merge --ff-only <sha>` IN
  YOUR WORKTREE, and every commit you make must have that worktree's branch
  checked out; `pwd` before every `git commit`.

## 49. A gate that grades a stale binary because the build step is conditional

- WHAT IT LOOKS LIKE: you edit the arm, delete a whole branch of the executor
  router, re-run the gate, and it comes back GREEN with every grade
  byte-identical. Nothing in the output says the edit was never compiled.
- HOW IT BIT US: 2026-08-16, `11_change_gate.sh`, first sabotage receipt. The
  gate carried `if [ ! -x "$HARNESS" ]; then cargo build ...; fi` — the shape
  `8_git_gate.sh` uses. A prebuilt `target/debug/emit_rust_harness` from an
  earlier run satisfied the test, the build was skipped, and the run graded the
  OLD executable. The sabotage receipt "arm removed → the harness stops by
  name" would have shipped as a lie.
- THE LAW: a gate that compiles the thing it grades rebuilds it EVERY run.
  "The binary exists" is not "the binary is this source". An escape hatch for a
  caller-supplied binary is fine (`DL_RUST_HARNESS`), but the default path
  builds. A sabotage receipt that comes back green is a claim about the gate,
  not about the guard.
- THE RAIL: `11_change_gate.sh` builds unconditionally unless
  `DL_RUST_HARNESS` is set, and its header records this incident as the reason.
  `8_git_gate.sh` and `5_dep_gate.sh` still carry the conditional shape and are
  owed the same change; neither is in this lane's ownership.
- SAY THIS TO AN AGENT: before you write a sabotage receipt, prove the sabotage
  reached the binary. A green sabotage run is a gate defect until shown
  otherwise.

## 50. A Git fixture tag named `head` on a case-insensitive filesystem

- WHAT IT LOOKS LIKE: 11 of 14 tests red at once, all reporting listings from
  the wrong commit. The base revision reads correctly and the head revision
  silently resolves to the checkout tip, so every diff is computed against a
  tree nobody asked for.
- HOW IT BIT US: 2026-08-16, `tests/change_facts.rs`. The fixture tagged its
  three commits `base`, `head`, `pruned`. macOS filesystems are
  case-insensitive, so `.git/refs/tags/head` and `HEAD` collide: `git` prints
  `warning: refname 'head' is ambiguous` on stderr, which the fixture's
  `Command::output()` never surfaces, and resolves the spelling to `HEAD`.
  Renaming the tags to `at_base` / `at_head` / `at_pruned` turned 11 red tests
  green with no change to the code under test.
- THE LAW: a Git fixture never names a ref `head`, `orig_head`, `fetch_head`
  or `merge_head` in any case. The collision is filesystem-dependent, so it is
  green on Linux CI and red on a developer's Mac, or the reverse.
- THE RAIL: pending — no scanner checks fixture ref names; the two Git
  fixtures in `sprefa-engine-rs/tests` now spell theirs `at_*`.
- SAY THIS TO AN AGENT: when a Git fixture's assertions all disagree in the
  same direction, run the underlying `git` command by hand and READ ITS
  STDERR. `refname is ambiguous` is a warning, not a failure.

## 51. A message bus whose coordinator leg was never wired, discovered by its silence

- WHAT IT LOOKS LIKE: lanes finish, their result hails are appended to
  `bus.ndjson`, and the coordinator never hears anything. Everyone works
  around it (`lane wait` armed per lane) until the workaround IS the system
  and the doc says "nothing delivers boop mail" as if that were a law.
- HOW IT BIT US: 2026-08-16/17. `boop adopt` (the SessionStart hook) wrote
  the coordinator route with `kind: "lane"` hardcoded; `run_hail`
  short-circuits `kind=="lane"` with "lane supervisor delivers it". True for
  lanes, false for an adopted interactive session, which has no supervisor
  polling the mailbox. Every completion ping queued forever with
  `to_timestamp: null`. Two side defects hid it: `lane list` showed the
  coordinator dead (pane target compared against session names), and a
  supervise give-up left the opencode TUI alive after one C-c, burning a
  provider conversation with no route left to steer it.
- THE LAW: a queue with no consumer is not a notification system. When a
  delivery path exists only for one kind of receiver, adopting a new receiver
  kind must either wire its leg or refuse. And a "delivered" claim needs a
  live end-to-end receipt: a real sender, a real receiver, the message
  observed at the far end.
- THE RAIL: hafley-rs `crates/boop/tests/coordinator_ping.rs` (FAIL-PRE-FIX
  header) runs adopt → hail → capture-pane over a real tmux session; the
  claude-harness leg types the line + Enter into the pane. Live proof:
  q38 lane `chore-ping-e2e` completed and its ping arrived in the coordinator
  pane unarmed. TUI close now escalates C-c ×2 to `kill_window`.
- SAY THIS TO AN AGENT: "the mail sits there unread" is a defect with a
  file:line, not a property of the universe. Trace the delivery branch for
  YOUR receiver kind before building a polling workaround.

## 52. A pane fed a body one keystroke at a time (transport, not the model, misses the deadline)

- WHAT IT LOOKS LIKE: every lane dies `rc=1 (stalled: 30s with no harness
  activity)` with an empty worktree and no session row in the harness store,
  while a human watching the pane sees text slowly appearing. Coordinator hails
  into an interactive pane arrive concatenated, three messages fused into one.
- HOW IT BIT US: 2026-08-16 23:03-23:13 EDT, five flash4 lanes. `send_keys_literal`
  and `send_text` (hafley-rs `crates/boop-mux/src/lib.rs`) spelled the body as
  `tmux send-keys -t <pane> -l -- <body>`, which types it rune by rune. Measured
  against a live opencode TUI with the 10540-byte
  `TASKS/extract-flow-cli-dispatch.BRIEF.md`: still ingesting at 70s, first
  session row ~110s after Enter. boop's `FIRST_SIGNAL_LIMIT` is 30s
  (`crates/boop/src/supervise.rs:21`), so the watchdog killed the lane before
  the harness had read its brief. Same root cause for the hail concatenation: a
  TUI that groups a burst of typed input as one paste reads the Enter typed
  inside that window as a newline.
- THE FIRST RCA WAS WRONG, AND MEASUREMENT CAUGHT IT: the card blamed control
  mode (`ControlClient::command` writing a multi-line `send-keys` as one line;
  tmux really does answer `%error` for `hello\nworld` unquoted). But
  `git log -S'command(&["send-keys"'` returns nothing, ever: control mode was
  never on the brief path. The parser fact was true and irrelevant.
- THE LAW: text going to a pane is a PASTE, never keystrokes. `load-buffer` +
  `paste-buffer -d -p`, then the submit key as a separate send after a gap.
  `-p` brackets the paste only when the pane's application requested bracketed
  paste, so a shell pane still receives plain bytes.
- THE RAIL: three tests in hafley-rs `crates/boop-mux/src/lib.rs` against a
  scratch pane running `sh -c 'printf "\033[?2004h"; cat > file'`, so the exact
  bytes the pane received are inspectable:
  `a_multiline_body_reaches_a_pasting_pane_bracketed_and_byte_exact`,
  `a_brief_sized_body_arrives_whole`, `a_plain_pane_receives_the_body_unwrapped`.
  Fail-pre-fix with the impl reverted to `send-keys -l`: the first two RED,
  `10K of brief must land whole: 10401 of 10413 bytes in 10.101423709s`.
  Post-fix 11 passed. Live receipt: same 10540-byte brief pasted into opencode
  renders `[Pasted ~263 lines]` in ~2s and creates its session row 3s after
  Enter; three multi-line hails into a Claude Code pane land as three separate
  user messages. Landed hafley-rs PR #10.
- SAY THIS TO AN AGENT: when a lane dies with no bytes anywhere, ask what the
  TRANSPORT delivered before blaming the model or the provider. Watch the pane
  and time the arrival; a body that is still being typed is a transport defect.

## 53. A content address the object database has never seen

- WHAT IT LOOKS LIKE: a rail that hashes the files it reads works on a clean
  tree and panics the moment anyone edits a file without staging it:
  `sh host 'call_node_at': read blob bcb9ae8 ...: bcb9ae8 missing`. The digest
  is correct. The bytes exist. Only git's object store has never been told.
- HOW IT BIT US: 2026-08-21, found by sabotage-testing the dead-module rail's
  oracle gate. `dead-module-rail.dl6`'s `files` host emits
  `git hash-object` over the WORKTREE, so an unstaged edit yields a real
  content address for content that was never written to the ODB.
  `hosts.rs` `read_blob` treated a `GitBatch::read` miss as a hard stop, so
  the whole run died on one dirty file. The rail had only ever been exercised
  on committed trees, which is why months of runs never saw it.
- THE LAW: a content address names bytes, not a storage location. A reader
  that resolves one through a single store must fall back to the other places
  those bytes can live, and must RE-HASH what it finds. An unverified
  fall-back is worse than the panic: it serves content the digest does not
  name, silently.
- THE RAIL: `v6/dl/deadcode/oracle-rustc.sh` runs the rail over a fixture
  whose lib.rs is edited in place and left unstaged. Fail-pre-fix, receipt
  above: `FAIL run: ... bcb9ae809cecbca883b266a756fb51dc6ac72e39 missing`,
  gate rc=1 before `hosts.rs:380-420`, `ORACLE-RUSTC OK rustc=2 rail=3` after.

## 54. A silent fall-back to `sh` for a command a linked twin already answers

- WHAT IT LOOKS LIKE: a rail is slow and nobody can say why. Every leg looks
  ordinary. The whole-run number is the only number, so the cost gets blamed on
  whatever is easiest to believe: the extractor, the decoder, a cache that is
  missing. All three were measured innocent before the real one was found.
- HOW IT BIT US: 2026-08-21. `dead-module-rail.dl6` grew from two extract-shaped
  hosts to five; the `.adapters.json` sidecar was never updated. `execution_for_plan`
  falls back to `shell` for an unmapped host, so three of them spawned the 48MB
  `extract` binary through `sh -c`, 82 times each, 246 spawns a run. They also
  failed `is_applicative`, so the runner could not fold them into the one
  grouped call per file it exists to make. Cost about 3.3s of an 8.7s run.
- WHY IT HID: `sprefa-engine-rs` had NO tracing at all: no dependency, no span,
  nothing. An engine with no spans cannot answer "where did the wall clock go",
  so the question got answered by guessing. Three guesses, three wrong.
- THE LAW: a fall-back that is slower by two orders of magnitude is not a
  fall-back, it is a defect with a polite face. Where a linked executor exists
  for a command, reaching the shell for that command is always a missing
  registration and must say so by name.
- THE RAIL: `ShellExecutor::run` carries a `warn_span!("sh_spawn")`, so every
  spawn is countable, and `linked_twin_for` matches the first shell token
  against the linked executors and warns with the host name and the sidecar it
  is missing from. Fail-pre-fix receipt: delete the `sig_at` row from
  `dead-module-rail.adapters.json` and the run emits 82 lines of
  `host shells a command the linked ``sprefa_extract`` executor answers
  in-process; its adapter row is missing`. With all six rows present,
  `sh_spawn` count is 0 and the run is 5.27s.

## 55. A test-only call site voting a module live

- WHAT IT LOOKS LIKE: a dead-code rail agrees with rustc on every file it is
  shown, and quietly disagrees on the one class rustc is best at. The rail says
  a module is live, `cargo check` says every function in it is never used, and
  the two answers are both read as correct because nobody joins them.
- HOW IT BIT US: 2026-08-21. `dead-module-rail.dl6` subtracted cfg-guarded DEFS
  (`cfg_def`) so a test helper could not inflate a file's def count, and read
  every call SITE in the file with no such filter. A module whose only callers
  sat in another file's `#[cfg(test)] mod tests` read as used-across, so it left
  the dead bucket. rustc's `dead_code` lint flags exactly that module in a
  non-test build, so the oracle could see what the rail could not.
- WHY IT HID: the def plane and the call plane were built at different times and
  only the def plane learned about cfg. `CallCollector` is one whole-file walk
  that knew nothing about items, and the fixture crate had a cfg case on the def
  side (`test_only_defs.rs`) and none on the call side, so the label sheet was
  green with the defect in place.
- THE LAW: a filter that lands on one plane of a two-plane rail is half a filter.
  A cfg predicate decides whether the compiler builds a def AND whether it builds
  a call, so both planes subtract or neither does. The subtraction stays per
  SITE: a name called from a test and from shipped code is a shipped call, and a
  dead-code rail may under-report and must never over-report.
- THE RAIL: the extractor emits `record=test_only_call` for a callee EVERY site
  in the file names under a cfg naming `test`, and `call_from` subtracts it.
  Fail-pre-fix receipt: `v6/dl/deadcode/fixtures/deadcrate/src/called_only_from_tests.rs`,
  called only from `live_pub.rs`'s test module, read
  `FAIL called_only_from_tests.rs rail said no, label says yes` plus
  `FAIL subset rustc flagged but rail missed: called_only_from_tests.rs`.
  Site-level receipt: `mixed_call_sites.rs` is named by a shipped site and a
  test site in one caller file; dropping the `shipped` filter from
  `test_only_calls` (`v6/sprefa-extract/src/lang/rust.rs`) reports it dead.
  Unit rail: `tests/30_rust_mod_scope_owner.rs`
  `rust_test_only_callees_leave_out_every_shipped_name`.
## 56. An index that was never checked for freshness and never kept

- WHAT IT LOOKS LIKE: a resolve answers instantly and answers wrong, or answers
  correctly and costs a full index build every single time. Both come from the
  same missing coordinate, so a reader who sees one has no reason to look for
  the other.
- HOW IT BIT US: 2026-08-21. `scip_ensure::index_path` picked the newest-mtime
  index among three known locations and nothing compared it to the file set the
  caller was asking about, exactly as v5 did and with the same header saying so.
  A corpus that moved kept answering out of whatever index happened to be on
  disk. The other half is the mirror image: `ScipMode::Build` called
  `ScipSource::build` directly, which stages into a fresh temp dir and returns
  that path, so a 25-minute index over `~/projects/hafley-rs` left nothing
  behind (`find -maxdepth 3 -name '*.scip'` empty afterwards) and the next ask
  paid it again.
- WHY IT HID: mtime reads like freshness. It is not: a newer index over a
  different file set is the wrong answer, and an older index over the identical
  set is the right one. Nothing on the wire named the set an index came from,
  so no consumer could tell the two apart even in principle.
- THE LAW: freshness is digest-of-set (user decision 2026-08-21). An index is
  current when the set of (path, content digest) it was built from equals the
  set the program is asking about, and never because of a timestamp. An index
  that was built is kept where the next ask will find it.
- THE RAIL: `IndexSet` is the set and its digest; `record_index_set` writes
  `<index>.set.json` beside the index; `index_path_for_set` returns a candidate
  only when the recorded digest equals the asked one, and `SPREFA_SCIP_INDEX`
  is the one exemption because an explicitly named index is the caller
  overriding the search rather than joining it. Fail-pre-fix receipts in
  `v6/sprefa-extract/tests/scip_freshness.rs`:
  `stale_set_rebuilds_and_the_original_set_still_hits` asserts `None` for a set
  with one changed digest and a hit for the original, and
  `a_stale_index_makes_ensure_rebuild_rather_than_reuse` asserts the rebuild is
  attempted rather than the stale index reused.

## 57. A hermetic staging copy that indexed six repositories instead of one

- WHAT IT LOOKS LIKE: an indexer run that everyone accepts as slow because
  indexing is the one named exception to the 10-second law. The exception is
  what stops anyone from asking why the number is what it is.
- HOW IT BIT US: 2026-08-21. `extract --resolve --scip-build` over
  `~/projects/hafley-rs` ran 25m37s. The same indexer over the same workspace in
  place ran 11.8s: 130x, none of it the indexer. Two causes, both in the staging
  copy. `copy_sources` skipped only build-output directory NAMES, so every lane
  checkout under `.boop-worktrees/**` was copied whole: 2320 `.rs` staged where
  the workspace holds 129, and 99 `Cargo.toml` where it holds 8. Then
  `Staging::Always` staged into a FRESH temp dir every run, so rust-analyzer's
  cargo resolution found no `target/` and recompiled every build script and
  proc-macro cold each time.
- WHY IT HID: the budget did not fire. `run_capped` capped the indexer at its
  600s default and the indexer itself stayed inside it; the extra minutes were
  the copy and the cold cargo resolution, which are the caller's own work and
  carry no cap. A budget on the child process alone measures the wrong thing
  when most of the wall is spent getting the child ready to run.
- THE LAW: a directory carrying its own `.git` is a different checkout and is
  never part of this workspace. A staging dir that is thrown away is a staging
  dir that pays for its own build cache every run.
- THE RAIL: `copy_sources` skips any child directory holding a `.git` file or
  directory, and `persistent_stage` keys one stage dir per (root, indexer) under
  the OS temp dir, so its `target/` warms across runs while the corpus itself is
  never written to. `prune_unstaged` deletes staged sources the corpus dropped,
  leaving `target/` alone. Fail-pre-fix receipts in
  `v6/sprefa-extract/tests/scip_freshness.rs`:
  `a_nested_checkout_is_never_staged` builds a fixture holding both a worktree
  (`.git` file) and a submodule (`.git` dir) and asserts neither is staged;
  `a_persistent_stage_drops_a_source_the_corpus_deleted` asserts the prune and
  asserts `target/` survives it.

## 58. A host seam whose fall-back was a shell, and the 246 spawns it hid

- WHAT IT LOOKS LIKE: the runtime answers a host by handing a filled template to
  `sh -c`. Every declaration that no adapter row routes still runs, so a missing
  registration is invisible: the rail produces correct rows and simply costs
  100x more per demand. Entry 54 is that incident; this entry is the class.
- HOW IT BIT US: entry 54's fix made the fall-back LOUD (`sh_spawn`,
  `linked_twin_for`) and left it reachable. A loud defect is still a defect, and
  the shell also forbids two things the engine needs: an `sh` host cannot carry
  a structured input, and its answer must be re-parsed out of stdout, so the
  extract twin serialized 17929 facts to JSONL for the runner to parse back.
- THE LAW: a seam with a slow universal fall-back has no failing case, so it
  never teaches. Delete the fall-back and the missing registration becomes a
  named stop at construction, before a single tick runs. A host answer crosses
  the seam as ROWS, never as bytes: an executor that names its own columns needs
  no decoder, and a decoder that guesses between JSON, a grid and one field per
  line is three ways to be wrong about the same answer.
- THE RAIL: `IHostExecutor::run` answers `Vec<HostRow>` and `executor_for`
  returns `None` for anything outside `LINKED_EXECUTORS`, so `HostLiveRunner`
  construction stops with `no executor links host '<name>' (adapter '<x>');
  linked executors: ...`. `tests/live_hosts.rs`
  `an_unrouted_sh_declaration_is_a_named_stop_at_construction` asserts the stop
  AND asserts the template's `touch` marker never appears, which is the
  fail-pre-fix receipt: that same plan used to create the marker.
  `tests/consumer_integration.rs` asserts every roster name resolves and that
  `shell` does not. Grep receipt: `Command::new` in `sprefa-engine-rs/src` is
  the `dl6` CLI's `swipl`/`git`/`cargo` only.

## 59. A whole-corpus join run once per file

- WHAT IT LOOKS LIKE: a resolve that finishes instantly with no index loaded
  never finishes with one. No error, no log, no obvious hot loop: 82 files went
  past 506s and were killed. Loading MORE evidence made the work unbounded.
- HOW IT BIT US: 2026-08-21. `join_documents` reads and content-hashes EVERY
  document a SCIP index names, and the three `Resolve<CallF>` arms (rust, go,
  ts) each called it inside `resolve`, which runs per FILE. 82 files over a
  129-document index is 10578 whole-corpus reads and re-hashes. The join is
  whole-project state; its own doc comment said so and said the engine would
  cache it "when this gets hot".
- THE LAW: work whose result depends on the PROJECT belongs to the project's
  lifetime, not to the loop body that first needed it. A comment promising a
  cache later is not a cache; a `OnceLock` in the bag the arms already borrow is
  the whole fix.
- THE RAIL: `IndexBag.joined_documents: OnceLock<Vec<Option<(ContentId,
  Vec<u8>)>>>`, set through `get_or_init` by whichever arm reaches it first.
  COUNT test, `v6/sprefa-extract/tests/32_join_documents_once.rs`: a reader that
  counts its calls, 3 files, a 5-document index. Fail-pre-fix `reads = 15`,
  after `reads = 5`. The count, not the edge set, is what detects a regression
  here: the edges were always correct.
- THE SECOND WHOLE-CORPUS WALK THE JOIN WAS HIDING, now closed: `site_occurrence`
  tries every occurrence of a document against one call site and `byte_range`
  rescanned the document bytes from offset 0 for each, twice per range (start and
  end), so a file the index knows cost sites x occurrences x bytes. `sample` over
  a live resolve of hafley-rs put 2477 of 2484 stacks in `site_occurrence` ->
  `byte_range`. 5 files finished in 2.04s; 10 files did not finish in 90s.
- THE FIX: `scip::LineTable`, one memchr pass per document giving the byte offset
  of every line start plus an end sentinel. `byte_range_at` indexes it;
  `byte_range` keeps its signature and builds a table per call, so no language arm
  changed. `site_occurrence` and `scip_rows::flatten_scip_records` build one table
  per document and one per signature text.
- THE RAIL: `v6/sprefa-extract/tests/n_plus_one.rs`, `scip::line_reads()` counting
  document bytes read. Fail-pre-fix over 4000 occurrences in a 100000-byte
  document: `400000000` reads against a `116000` bound; after, it passes. Signature
  occurrences: `52000000` -> bound `34000`. End to end, hafley-rs
  `crates/*/src/*.rs`: 10 files 90s+ -> 2.2s, 65 files 150s+ -> 4.9s, 3804 rows,
  byte-identical to the pre-fix binary on the 8-file resolve both could finish.

## 60. A pathspec rooted at the repository and a manifest rooted at the workspace

- WHAT IT LOOKS LIKE: a rail that reads a cargo crate is right on the first
  target and silently empty on the second. The first target IS its own git
  repository, so its workspace root and its repository root are the same string
  and every path plane lines up by accident.
- HOW IT BIT US: 2026-08-21, `v6/dl/reach/feature-reach.dl6`. `soopy_files`
  enumerates `git ls-files` from the REPOSITORY root, so `files` answers
  repository-relative paths, while `cargo metadata` answers absolute
  `src_path`s whose only prefix in that document is `workspace_root`. The first
  draft stripped `workspace_root`, so a crate at `v6/sprefa-extract` derived
  the root `src/bin/extract.rs` against a file set carrying
  `v6/sprefa-extract/src/bin/extract.rs`: zero entry points, zero features,
  every cell unreachable, exit 0. The run that exposed it stopped earlier for
  the same reason wearing a different hat: with cwd set to the crate rather
  than the repository, `call_node_at` was handed the repository-relative
  `src/cli/check_deadline.rs` and stopped on "no repository root for digest
  read".
- THE LAW: one root per program. Pathspec, extractor read, scip project root
  and derived cargo root all key on the REPOSITORY root; a workspace root is a
  second root that gets converted, never assumed equal. The fold runs from
  `git rev-parse --show-toplevel` and the program strips a seeded `repo_root`.
- THE RAIL: `bash v6/dl/reach/feature-reach.sh --check` third plane, `nested`:
  the same fixture crate read where it lives inside sprefa, glob
  `v6/dl/reach/fixtures/reachcrate/src/*.rs`, diffed against
  `fixtures/expected.nested.tsv`. Pre-fix that plane cannot even reach a diff:
  the fold stops on the digest read above. Post-fix it carries the same 8
  matrix cells as `expected.diet.tsv` under the repository-relative prefix. The
  `diet` and `scip` planes run against a temp-dir copy that IS its own
  repository, which is exactly the shape that cannot detect this.

## 61. A runtime with no per-verb clock, and three optimizations aimed at the wrong 12%

- WHAT IT LOOKS LIKE: a fold is slow, the engine writes SQL, so the SQL gets
  tuned. Batching, grouping and memoizing all land, all are measured, and all
  three come back neutral or worse. Nobody can say what fraction of the wall
  clock SQLite even holds, so the next guess aims at the same place.
- HOW IT BIT US: 2026-08-21. `sprefa-engine-rs` had two spans in the whole
  crate and one aggregate seam tally. Three consecutive changes were measured
  and rejected on the sf_join 54k-row fold: a per-tick BEGIN/COMMIT, one arrival
  group per (rel, sign) instead of consecutive runs, and a per-(content, mask)
  extraction memo. The first per (verb, relation) table printed after the spans
  landed said SQLite was 11.9% of that fold. The other 88% was five Rust loops
  that deduplicated rows by scanning what they had already collected
  (`boundary_delta`, the arrival stage set, the arrival key probe, the keyed
  edge resolve, `text_plane::collect_values`). Indexing those five: 5441 -> 1009
  ms, byte-identical tick log.
- WHY IT HID: the seam tally counted statements and their microseconds, which
  says how much SQLite cost and never what share of the run that was. A quadratic
  loop between two statements is invisible to any counter that only wakes up
  inside the seam, and every one of the five sat between statements the tally
  did count.
- THE LAW: a measurement that cannot name the remainder is not a measurement.
  Wall clock minus instrumented time is a number the report has to print, and
  the label a statement carries comes from the IR (relation, verb), never from
  parsing the SQL text back into a guess about its purpose.
- THE RAIL: `DL_TRACE_SUMMARY=1` prints one table per fold, per (verb, relation)
  sqlite/rust/calls/rows plus the unscoped remainder, and
  `RUST_LOG=sprefa_engine_rs=trace` opens the same tree as spans.
  Fail-pre-fix receipt: `v6/sprefa-engine-rs/tests/trace_summary.rs`
  `boundary_delta_probes_once_per_row` reads 20000 probes for 20000 rows where
  the scan read 199,990,000 comparisons, and
  `summary_names_the_ir_verbs_and_the_seam_compiles_each_text_once` pins that
  every relation in the table is one the IR declares.
  Bench: `v6/sprefa-engine-rs/bench/profile.sh`, `bench/ab.sh`,
  `bench/rail-profile.sh`, `bench/file-db.sh`.

## 62. Program metadata recomputed once per tick

- WHAT IT LOOKS LIKE: a fold that costs the same whether the tick moved one row
  or ten thousand. The per-verb table shows the cost nowhere, because the work
  sits between the statements and belongs to no relation.
- HOW IT BIT US: 2026-08-21. Six fold paths asked a question whose answer is
  fixed for the life of a program, and asked it inside a loop. `recursive_heads`
  ran a substring pass over every level insert text against every relation's
  frontier name, once per tick: 44 levels x 57 rels on the dead-module rail.
  Five more walked the relations slice to find a statement's head plan, once per
  statement per round. `stage_departures` walked the whole boundary delta list
  once per relation.
- THE LAW: the shape of a program is decided when the program is built. A
  question whose answer cannot change between ticks belongs to `GenProgram`, and
  a lookup by rel name belongs to an index built at a phase entry, never inside
  the statement loop.
- THE RAIL: `v6/sprefa-engine-rs/tests/n_plus_one.rs` with
  `incremental::plan_probes()` and `incremental::frontier_probes()`.
  Fail-pre-fix over 1200 rels and 40 statements: stage_departures `720600` vs a
  `2400` bound, frontier scan `144000` vs `48000`, plan lookup `2460` vs `240`.
- WHAT IT DID NOT FIX: this class is cheap in absolute terms (schema-sized, not
  row-sized). The row-sized cost was entry 61.

## 63. Three full copies of an arrival batch before it reached the seam

- WHAT IT LOOKS LIKE: an `intern` phase costing 448ms of Rust against 25ms of
  SQL, on 6482 rows. The trace attributes it correctly and it still reads as a
  mystery, because no single line is slow.
- HOW IT BIT US: 2026-08-21. `enum_plane::intern`, `text_plane::intern` and
  `struct_plane::intern` each took `&[Arrival]` and returned `Vec<Arrival>`, so
  every tick copied the whole batch three times even when no plane changed a
  value: the enum plane is a check-only pass and returned `arrivals.to_vec()`
  unconditionally. Downstream, `stage_events` cloned every event into a per-rel
  bucket, `staged_statements` cloned the additions again, and the JSON encoders
  built a throwaway `Vec<Value>` per row to carry two leading integers.
  `boundary_delta` rendered each row's dedup key twice, once to probe and once
  to insert.
- THE LAW: a plane that may not change anything hands back what it was given.
  `Cow<'a, [Arrival]>` BY VALUE chains the planes with no lifetime knot and one
  `Owned` only where a rewrite happened. Grouping borrows; only the seam's own
  argument vector owns.
- THE RAIL: `the_boundary_read_renders_each_row_key_once` in
  `v6/sprefa-engine-rs/tests/n_plus_one.rs`, `40000` renders for `20000` rows
  pre-fix. The rest is the per-verb table, which is what the class needs:
  `bench/rail-profile.sh` `intern` rust 447838us -> 26440us, `publish
  __host_response_extract` 205359 -> 22878, `stage __host_response_extract`
  142211 -> 11607, TOTAL rust in scopes 3093662 -> 736591. RUST-GRADE stayed
  439/335 byte-clean across every step.
## 64. A program driven only by boot facts, answering silence that reads as a finding

- WHAT IT LOOKS LIKE: every seed is a plain fact in the source, the program
  compiles clean, the harness exits 0, and the reads come back empty except for
  one negated rel, which comes back FULL. The empty rels look like "nothing
  matched" and the full one looks like a real finding.
- HOW IT BIT US: 2026-08-21, `v6/dl/crosswalk/crosswalk.dl6`. `repo_rev`,
  `repo_scope` and `entry_point` were written as facts, per the brief. The fold
  answered zero `source_file`, zero `dep_edge`, zero `repo_file_count`, and
  three `entry_unreached` rows naming every declared entry point. Read at face
  value that says "the entry points are stale at this rev". What it actually
  says is that no host ever ran: a boot fact is a static table and never a tick
  delta, so a rule whose body is facts and a host demand never fires, while
  `not(reach(...))` over an empty `reach` is satisfied for every entry.
- WHY IT HID: the negation inverted the silence. Every rel that should have had
  rows had none, and the one rel whose emptiness would have been the honest
  signal was the one the negation filled. `--final-tsv` prints what a rel holds
  and never whether its host was ever demanded.
- THE LAW: a fact table is a JOIN PARTNER, never a driver. Every program that
  reads the world names one arrival rel and puts it in the body of the rule that
  raises the first host demand. `crosswalk.dl6` spells that `crosswalk_run` and
  the runner posts one row.
- THE RAIL: not built. The shape a rail would take: `analyze.pl` already walks
  each rule's body, so a host-demand rule whose body joins only boot-fact rels
  and no arrival target is statically decidable, and it can never fire. Filed as
  a compiler-lane request on the crosswalk PR rather than fixed here, because
  `v6/prolog/**` is another lane's tree.

## 65. Two hosts meant to share one pass, split by a name the registry never heard of

- WHAT IT LOOKS LIKE: two `sh` declarations carry the same template on purpose,
  so the runner's applicative grouping folds them into one process and each
  selects its own columns. One of the two names is new. The compile stops at
  `template_mismatch(unreferenced_input(digest))`, adding `{digest}` to that
  template makes it compile, and the two hosts now fill DIFFERENT commands and
  run the extractor twice over every file.
- HOW IT BIT US: 2026-08-21. `crosswalk.dl6` declared `repo_extract` (registered)
  and `repo_call_site` (new) over the same `(repo, path, digest)` inputs.
  `registry.pl:525` `host_input_roles/3` falls back to all-identity for a name
  with no `host_input_contract` row, and `1_host_expand.pl:311` requires every
  IDENTITY input to appear in the template. `repo_extract`'s registered contract
  makes `digest` freshness and exempt; the new name's does not.
- WHY IT HID: the failure is a compile stop and not a wrong answer, so it is
  loud — but the OBVIOUS fix (mention the column) is the one that silently
  doubles the work, and nothing measures that. This is ARCH.pl's already-filed
  `cold_author_defects` D1 ("host_input_contract keyed on hardcoded host NAMES")
  reaching a second caller.
- THE LAW: a host that must share a pass with another host shares its input
  contract, which today means sharing a registered NAME. `crosswalk.dl6` uses
  the registered `call_node_at` and `extract` over absolute paths rather than
  minting a repo-scoped twin.
- THE RAIL: not built. A `host_input_contract` row for the repo-scoped site
  shape is a registry request on the crosswalk PR. The measurable rail is a
  count: two hosts declared with one template must produce one `host_run` span
  per file, and `DL_TRACE_SUMMARY=1` already prints the call count that would
  catch a second pass.

## 66. A resumed agent whose worktree was gone, and a reset that landed in the coordinator's tree

INCIDENT (2026-08-21, arrivals-and-ticks lane). A subagent's isolated worktree
was cut from the wrong base and the agent stopped correctly. Its worktree was
auto-cleaned the moment it stopped. The coordinator resumed it with "run
`git reset --hard <sha>` in your worktree" - but the resumed agent no longer
HAD a worktree, its cwd had fallen back to the coordinator's own working tree,
and the reset executed there, wiping the coordinator's uncommitted work (one
file rewrite; recovered from the transcript and recommitted).

RCA. Two facts composed: (1) an isolation worktree is deleted when the agent
first completes, so a RESUMED agent silently inherits some other cwd; (2) the
instruction named an action ("reset --hard") relative to a location ("your
worktree") that no longer existed, and nothing checked the location before the
destructive command ran.

FAIL-PRE-FIX PROBE. Resume any completed worktree agent and have it print
`git rev-parse --show-toplevel`: it answers the COORDINATOR's tree, not an
agent worktree.

RAIL. Standing instruction, both directions: an agent asked to run any
destructive git command MUST first print `git rev-parse --show-toplevel` and
STOP unless the answer is the tree the coordinator named in the same message;
a coordinator resuming a completed isolation agent MUST re-state the absolute
tree path and MUST NOT name reset/checkout/clean/stash in a resume message.
Committing early and often on the coordinator branch bounds the blast radius:
the incident cost one uncommitted file, not the arc.

ENTRY: this row.

## 67. A surface removed on one branch, and two new programs written in it on the other

INCIDENT (2026-08-21, arrivals-and-ticks landing). The branch deleted `sh` and
`bind` from the parser and moved every `.dl6` it owned to the arrival form.
`origin/main` meanwhile gained two brand-new rails,
`v6/dl/rails/{no-new-eprintln,recompute-guard}-rail.dl6`, both written in the
`sh` surface with `.adapters.json` sidecars naming `soopy_files` and
`sprefa_extract`, adapter names the new roster no longer answers to. `git
merge` reported zero conflicts: neither branch touched a line the other
touched. `just v5-rails` went red on the first run after the merge, with a
compile stop from the rule index rather than from the parser.

RCA. A surface removal is a change to the LANGUAGE, and a merge only compares
FILES. Nothing in the tree relates "the parser stopped accepting `sh`" to "a
file spelling `sh` was added". The corpus sweep does not read `v6/dl/rails/**`,
and `just v5-rails` is not part of `just green-all`'s default legs, so the only
signal was a leg run by hand.

FAIL-PRE-FIX PROBE. `git merge origin/main` on the collapse branch, then
`cd v6 && just v5-rails`: the recompute rail stops with
`unsupported_construct: compiler refused rule 'surface_findings'`, and no gate
between the merge and that command says anything.

RAIL. A surface-removal arc greps the LIVE corpus for the removed keyword after
every merge from main, not only before the first commit:

    git grep -nE '^\s*(sh|bind) [a-z_]' -- 'v6/dl/**/*.dl6'

The two rails here are now in the arrival form and their sidecars are deleted.
Two hits remain and are NOT this arc's: `v6/dl/fixtures/{sg-rail,pr-size}.dl6`
are byte-identical to origin/main, absent from
`v6/prolog/compile/out/manifest.json`, named by no recipe, and already fail to
parse on main at their `?`-demand lines (15:5 and 23:6), not at `sh`. They are
tsv2-era dead corpus whose conversion needs `/clock/tick`, which the
wip/dl6-run-watch-salvage lane owns. Anything else the grep reports is live.

SECOND INSTANCE, same day, same merge direction. `origin/main` moved to
3d65add5b (PRs #405, #406, #407) and brought THREE more programs in the removed
surface: `v6/dl/fixtures/files-rev-walk.dl6` (`sh files`, `sh files_at`),
`v6/dl/prwatch/prwatch.dl6` (`bind interval`, `sh pulls`, `sh cost`), plus
three `.adapters.json` sidecars. `hosts.rs` conflicted this time, because both
sides edited `LINKED_EXECUTORS`, which is the ONLY reason the collision was
visible at all. The registry roster is what makes it visible in general: three
new executors (`GhPullsExecutor`, `TickCostExecutor`, and `files_at`'s arm of
`SoopyFilesExecutor`) reached main with no `arrival_executor/2` row, and
`executor_roster_matches_registry` is the test that would have named them.
`files-rev-walk.dl6` is converted here. `prwatch.dl6` is NOT: `bind interval`
lowers to `/clock/tick`, whose `executors/clock.rs` did not land with #407, and
the file belongs to the pr-watch-resident lane.

RAIL, SECOND HALF. A lane that adds an executor adds its `arrival_executor/2`
row in the same commit. `LINKED_EXECUTORS` is a `const` in one file precisely so
that two lanes adding executors CONFLICT rather than silently diverge; do not
split it into per-module lists.

WHAT THE RE-SPELL FOUND, and it closed the open question rather than raising
one. `bind` needs no replacement keyword: an arrival rel is demanded BY a
positive body, so a cadence is a SEED FACT the program owns. Two existing
compiler stops force that shape and both are correct.
`probe_mismatch(multiple_probes(...))` (`1_host_expand.pl:445`) says one rule
body carries at most one arrival goal, so the turn becomes its own rel that
every reader joins. `level_rule_no_positive_body` on
`__host_demand_clock__tick/3` says an arrival rel nothing demands is not a
program, it is a keyword in disguise. Six programs moved and all six compile to
a binary through `dl6 build`.

ENTRY: this row.

## Rail gap table

| # | class | rail status | promotion needed |
|---|-------|-------------|------------------|
| 1 | per-row writes / N+1 | half | waiver-audit the 40 HEAD static-n1 findings (792cc902), promote `.dl/static-n1.dl` to error severity |
| 2 | nondeterministic extraction | enforced | replace the lossy df_node id (`file:line:col`) with a repo-scoped id — ref-spine owns (docs/rca-exe-swap-write-storm.md:147) |
| 3 | full-layer rebuild | mostly enforced | digest-before-write landed for non-recursive components (tests/it/derived_skip.rs, DL_NO_DERIVED_SKIP lever); residual: recursive components still rewrite byte-identical rows on unattributable full rebuilds |
| 4 | unguarded recompute | enforced | — |
| 5 | crash-window half-written state | enforced | — |
| 6 | exe-swap / daemon-restart storm | enforced | — |
| 7 | quiet-tick write budget | missing | serve examples/chaos-soak.dl under the daemon in CI; fail on any `_write_ledger` row for the root on a quiet tick (examples/chaos-soak.dl:23-31) — and assert WAL byte growth too: `declare_all`'s unconditional per-rel VIEW DROP+CREATE rewrites sqlite_master every tick (~2.5MB/tick noise floor measured in tests/it/derived_skip.rs), invisible to the row ledger |
| 8 | lock across blocking calls | missing | hold-set instrumentation in the `lock`/`rlock`/`plock` helpers (83/107 sites, docs/effect-inventory.md:262) + static rail on lock-then-block in one fn body |
| 9 | unbounded channels | missing | static rail banning unbounded ctors without a `// @unbounded:` waiver; drive 3 → 0-or-waivered (docs/effect-inventory.md:308-315) |
| 10 | magic rel-name reads | enforced | — |
| 11 | co-heading source+derived | enforced | — |
| 12 | ambient config in tests | half | hermetic-by-default it-test harness (fail when `SPREFA_CONFIG` unset); fix the in-tree syntax.md regen (PLANS.md:17) |
| 13 | stale-binary verification | half | hook compares installed `build_id` (src/cli/daemon.rs:209) against the worktree target; refuse or warn on mismatch |
| 14 | pre-commit worktree hang / orphan root dbs | enforced | — (`refuse_worktree_cold_check` src/hook.rs:461 wired at src/cli/mod.rs:476, green-by-skip exit 0, `DL_ALLOW_WORKTREE_COLD=1` escape hatch; fail-pre-fix tests/it/worktree_cold_check.rs; detection side = `dl daemon health` orphan-roots report) |
| 15 | dishonest change flags | half | waiver-audit the 25 HEAD findings, promote `.dl/dishonest-flag.dl` to error severity (792cc902) |
| 16 | kill-respawn cold-restart loop | half | `--hook` is attach-only since 2026-07-18 (never autostarts; sub-second self-deadline); still missing: `--check` autostart-once + backoff, mid-cold digest persistence, kill-mid-cold resume it-test |
| 17 | unbounded db growth | half | boot verdict line (db bytes, corpus bytes, ratio) + `--check` ceiling warn; diet steps 1+3 landed (norm drop, -60% `_strings` on fixture); 4a WITHOUT ROWID landed on vouched Rust-authored junctions (`pk_never_null` vouch, 17 autoindex twins dropped; see the step-4a NULL-in-PK incident in this class) — `.dl`-declared junctions (flow_edge, df_edge_src_kind) still open pending a derived-nullability story; step 2 index audit open |
| 18 | per-rule parse amplification / tiers-not-ceiling | enforced | — (parse-counter tests + governor toggle test; `dl --hook` self-deadline `DL_HOOK_DEADLINE_MS` landed same day) |
| 19 | subscriber render storm / daemon exhaust feedback | half | fail-pre-fix it-tests for watch filter + broadcast gate (unit/witness only today); cold-extract write-rate budget (scheduler arc); per-family extractor versioning vs exe_stamp; perf.jsonl rotation (tracing-appender, obs arc) |
| 20 | phantom extract diff / whole-program derived rebuild | enforced | — (f9414e3c + fail-pre-fix steady-state test; c3148d90 counter keying) |
| 21 | client poll blocking its own UI thread | half | instant conversions committed + pushed (74d6d36, 2026-07-18); still open: a "poll never blocks main" regression test in instant; audit remaining ~40 sync commands there |
| 22 | effect-free root frozen unsettled | enforced | — (settle-aware poll gate + `effect_free_root_settles_after_boot`, failed pre-fix) |
| 23 | one-shot positional swallowed by daemon program set | missing | erase-no-daemon-split arc: carry the positional through the daemon RPC, or fall through to in-process (with a loud line) when the positional is not the root's watched set; fail-pre-fix it-test = one-shot a file whose rels are disjoint from `.dl/*.dl` under a live daemon and assert its own query rels come back |
| 24 | TLS-parked span guard dropped in thread-local destruction | enforced | — (ManuallyDrop park + explicit exit paths, 9ddf1280; abnormal-death coverage in src/jobq/tests_cancel.rs) |
| 26 | composite key minted by string concatenation, stored as an id | half | promote `.dl/composite-key-string.dl` to error severity once the 14-row baseline is waiver-audited; arm 3 (declared-column join) not implemented — no fact links a `RelDecl` column to its writer expression; distinguish the hash-digest false-positive class (src/effect.rs:527/612/644) from a true raw-id fold in the message wording |
| 27 | empty input read recorded as no dependency | enforced | — (union inside `rel_footprint` src/engine/family/router.rs:213, the one helper cold/react/react_deltas build memos through; deterministic pin `empty_input_rel_still_reruns_the_family_after_insert` + T4 property replaying seed `cc 0d80eca0`, both proven fail-pre-fix). Residual: no static rail asserts a dependency set is built from declared inputs rather than observed reads, so a new unit type can reintroduce the shape |
| 28 | clean-tree-only code path masked by an always-dirty verification tree | half | wire a clean-tree (or `DL_REV_OVERRIDE`) measurement into CI so rev-resolution / git-object branches actually get exercised, not just measured on a dirty tree |
| 29 | read-shaped CLI flag retargets --db to the real served root | enforced | — (file-scoped `--check`/`--diag-json`/`--lsp` defaults to `:memory:`, attach is opt-in via `--attach` or discovery mode; src/cli/mod.rs; tests/it/hermetic_state.rs) |
| 30 | state-home sandbox knob `DL_STATE_DIR` ignored | enforced | — (`daemon_home()` honors `DL_STATE_DIR` > `XDG_STATE_HOME` > default, one resolver src/daemon/home.rs; `.dl/state-home-single-source.dl` warns on a second reader; tests/it/hermetic_state.rs) |
| 31 | unbounded daemon logs (launchd redirect + spawn-only cap) | enforced | OS-level `newsyslog.d` complement remains a documented recommendation, not a shipped config |
| 32 | normalizing a folded composite id onto a lossier decomposition | missing | the sym-dict migration (next chapter) must ship the bijection count-equality gate + per-join parity probe from `plans/2026-07-21-sym-dict-correctness-proof.md`; today only R1 (df in-memory id → `NodeIdx`) landed, mint_sym/lambda_sym still fold `format!` strings |
| 35 | dev server outlives its spawner | missing | stdin-watch exit in v6 main.ts (parent death closes the pipe) + pid-owns-port assertion in goal-endurance readiness; fail-pre-fix test per the pipeline |
| 36 | non-finite number spliced unquoted into SQL text | enforced | — (encode-site guard + execute$ SQL-in-error, both v6/dl/src/3_runtime.ts; parseWhitespaceColumns line-per-column fix, v6/dl/src/1_hosts.ts; fail-pre-fix tests in v6/dl/tests/4_hosts.test.ts + tests/3_runtime.test.ts) |
| 38 | unbounded compute grind (hang or OOM instead of a named refusal) | mostly enforced | v6/tools/run-capped.sh wraps the served compile door, every v6 receipt script, the graphviz render and both bench engine runners; residual = `scripts/dl-trace.sh` and `scripts/verify.sh` still carry the orphaning `perl -e 'alarm N; exec'` on the v5 side, and no rail yet REFUSES a new unwrapped invocation (a ratchet over `swipl`/`node`/`dot`/`curl` call sites is the promotion) |
| 39 | nested `cap_self` re-groups out from under the outer kill (orphaned server squats its port) | missing | (a) label-independent group marker so `cap_self` declines to re-exec inside an existing cap (v6/tools/run-capped.sh:78-92), fail-pre-fix receipt = an outer `run_capped` 124 leaves no backgrounded child; (b) `TSV2_PORT=0` plus reading the bound port back off the server's own `tsv2 serving on <port>` line (v6/tsv2/serve/main.ts:18,24) across the 13 fixed-port shell rails in v6/tsv2/scripts/ — the mechanical port of the TS-side fix already shipped as `startServed(port = 0)` (v6/tsv2/tests/serveHelpers.ts:135-148); today two of those rails even share 17571 (extraction-live.sh:68, memory-soak.sh:26) |
| 40 | aggregate emits no row for an empty group (the `coalesce` empty-group idiom) | missing | fixture `coalesce_fills_an_empty_aggregate_group` in v6/prolog/conformance/fixtures/7_coalesce.pl (all eight existing sources are EDB rels or level views, none aggregate-headed), plus one sentence each on the coalesce paragraph and the aggregate rows of v6/prolog/compile/SYNTAX.md (:86-101, :138-148); the honest rail beyond both is a check that an aggregate feeding an arithmetic expression over a group rel has a filled source (plans/2026-07-31-auto-factorization-verdict.md:1054-1057), unowned |

| 41 | a malformed host response kills the dl server process instead of landing as a diag | missing | `encodeSurfaceRowByColumns` throws `commit: non-numeric value in rel '<r>' column '<c>'` at v6/dl/src/3_runtime.ts:186, unhandled through applyEdbTxn (:605) and the tick loop (:896), so the listener dies and every later request gets ECONNREFUSED; the load response had already answered `{"loaded":true}`, so the program looks accepted. Triggered by an sh host whose declared output column names do not match its stdout keys: parseHostOutput (v6/dl/src/1_hosts.ts:94-190) falls out of the JSON-lines branch into whitespace-column splitting and shreds every value. Fail-pre-fix receipt = an sh decl whose template emits JSON-lines under one key set while declaring another, asserting the server still answers GET /idb/:rel afterward. Rail = coerce-or-refuse at the encode site (the class-36 encode-site guard precedent, same file) plus a bridge-time check that a JSON-lines host's keys cover its output-only columns, unowned |
| 42 | interactive-harness lane never fires its on-exit hail (completion signal rides process exit; an interactive TUI idles at its prompt forever) | enforced | incident 2026-08-10: lanes codex-findings + recon-luna finished work at ~00:17 and sat silent 8h with a live pid, live tmux session, unchanged worktree; the parent monitor watched session-exit and so also stayed silent. RCA chain: (1) lane epilogue `; __rc=$?; boop hail ...` is only reachable on harness exit, (2) codex spawned interactive (`codex '<prompt>'`) which never exits, (3) coordinator liveness check used only the process-alive half of the two-check law. Fail-pre-fix: launch_command tests asserted the interactive spelling; flipped to assert the `codex exec` prefix (v6/boop/src/harness/codex.rs tests), and exec exits on completion so the epilogue is reachable. Rail = spawn composes `codex exec` + `--dangerously-bypass-approvals-and-sandbox` (lane = trusted automation, mirrors opencode `--auto`), send_midflight measured false |
| 47 | recorded-but-uncompared perf trail (bench and ledger without a comparator) | missing | budgeted bench cell in the battery (grid fixpoint time + RSS ceilings vs banked FACTS.md) + `tsc --noEmit` gate over dl6 lab drivers |

| 48 | silent provider stall kills a lane invisibly, and every respawn cold-starts a new session with the full brief | enforced (pending commit in hafley-rs) | incident 2026-08-13: DlSource lane rounds 4-6 on flash4 produced three distinct opencode session ids in one lane run, each ending MessageAbortedError, zero commits; the direct repro emitted EMPTY stdout+stderr until an external timeout kill (rc=124) while `opencode run` exits 0 on a dropped stream. RCA chain: (1) a dropped provider stream either exits 0 (caught since boop 0.0.2 by the trailing-message probe) or hangs forever with no output, (2) the supervisor had no turn watchdog, so a hung child was an invisible dead lane until a human killed the pane, (3) a respawned supervisor never read the conversation route `remember_conversation` had pinned, so every restart opened a fresh session and re-fed the whole brief. Fail-pre-fix: `a_pinned_conversation_round_trips_through_the_registry_route` (hafley-rs crates/boop/src/supervise.rs tests). Rail = STALL_LIMIT watchdog in the supervisor poll loop (landed values: FIRST_SIGNAL_LIMIT 30s with no store write at all, STALL_LIMIT 5min mid-turn, crates/boop/src/supervise.rs:22,24; kill + TurnEnd::flaked -> the existing `-s` flake-resume path), `last_activity_ms` probe over opencode's message+part tables (crates/boop/src/channel/opencode.rs), and `pinned_conversation` route read-back at lane-run start so a cold restart resumes and opens with the continue nudge instead of the brief (crates/boop/src/main.rs run_lane_supervisor) |

| 49 | lane dies and the waiter never hears (result hail rides the pane shell epilogue; the death kills the pane) | enforced (branch fix/lane-death-notification in hafley-rs, unmerged) | incident 2026-08-14: lane feature-list-value-position, pro4 provider stream went silent on turn 2 (zero bytes 56s), FIRST_SIGNAL_LIMIT C-c'd the TUI (opencode recorded MessageAbortedError 18:04:50), the flake-resume restart failed, tmux session evaporated, and `lane wait` starved forever while the registry's pid-observer already showed the lane dead — the knowledge had no consumer. RCA: the `boop hail --kind result` epilogue is only reachable if the pane's shell survives the death it is reporting. Fail-pre-fix: `a_supervisor_error_still_writes_the_lane_s_result_row` + `wait_calls_a_lane_dead_when_its_route_stops_being_live` (hafley-rs crates/boop). Rail = supervise writes the result row in-process on every exit path (supervise.rs record_result), and `lane wait` exits 3 on 5 consecutive dead-route observations with no result row (main.rs wait_for_outcome) |
| 50 | self-racing gate script (two grade.sh in one shell line clobber target/debug; the loser writes a build error as a fixture verdict) | enforced | incident 2026-08-14: opus string-arc lane chained two grade.sh invocations, the second's `cargo build` rebuilt target/debug under the first, and `emit_rust_harness: No such file or directory` landed as a per-fixture verdict in graded.tsv; re-running the write pass isolated restored the truth. Fail-first (shortened stand-in reproducing the same shared-path shape — build via rm-then-write, no lock — since two real grade.sh runs take minutes and race only probabilistically on cargo's own cache state): `runA: build done, now using .../emit_rust_harness` then `runB: build done, now using .../emit_rust_harness` then `runA: invoked harness -> built-by-runB` (run A silently read run B's rebuilt binary). Rail = mkdir-based lockdir `$here/target/.grade-sh.lock` (v6/sprefa-engine-rs/grade.sh:7-21) — mkdir is atomic and needs no companion binary, and macOS ships no flock(1) — trap-cleaned on EXIT/INT/TERM, stale-pid reclaim via `kill -0`. Post-fix receipt: two real concurrent `bash grade.sh` runs, run 2 prints `grade.sh: another run holds the lock (pid 50601); exiting` and exits rc=1 immediately (pid 50601 confirmed via `ps` as run 1's own process) while run 1 completes unaffected, `RUST-GRADE graded=428 byte-clean=320` rc=1, identical to the single-run baseline taken before and after the change |
| 51 | opencode tui lane reads as silent to the watchdog, and the stall interrupt kills the very window the resume needs | enforced (hafley-rs main 2b348b1) | incident 2026-08-14: lanes fix-grade-sh-lock + fix-boop-variant-passthrough each died 38s after dispatch, rc=1 `supervisor error: ... can't find window: 0`; the user's soopy lanes died the same way post-19:45 rebuild. RCA chain: (1) `TuiChannel` never overrode `last_activity_ms` (trait default None, channel.rs:107), so the entry-48 FIRST_SIGNAL_LIMIT 30s read every healthy tui turn as silent from turn start, (2) `close()` sends C-c and the opencode tui QUIT on it instead of absorbing it, its tmux window dying with it, (3) the flake-resume `start_turn`/`poll_turn` then hard-errored on the dead window (`can't find window`), the entry-49 in-process result row faithfully reporting a death the watchdog itself caused. Fail-pre-fix: `start_turn_respawns_a_dead_agent_window` (hafley-rs crates/boop/src/channel/tui.rs, sabotage receipt: neutering `window_is_gone` FAILED it, restore passed). Rail = `TuiChannel::last_activity_ms` reads opencode's store exactly as the run channel does, `reopen_window` respawns a dead agent window on the same session with the profile's `resume_flag` and re-holds it at index 0, and a window death mid-poll returns `TurnEnd::flaked` into the existing resume path instead of a supervisor error |
| 52 | a rel-typed column whose NAME collides with a column of its referenced type renders null (unqualified outer column inside the correlated render subquery) | enforced | incident 2026-08-14: template-bounds arc, `pair(pair(int))` rendered its second column null. RCA: `dictionary_render_expr/3` at v6/prolog/lower.pl:2752 emits the outer row's column UNQUALIFIED inside a subquery whose FROM aliases the child ref view `d`, so sqlite resolves `d."__id" = d."first"` self-referentially; `relation_render_column_expr/5` (:2723-2730) qualifies with `t.~w` and is correct. Generics-free probes: equal parent/child column names WRONG, disjoint names identical, one-of-two shadowed WRONG on that column only. Same-template nesting guarantees the collision. Fail-pre-fix repro: three-fixture file in the d73eeedb commit message (branch feature/template-bounds-parens). Rail landed in PR #256 (05c21477): `dictionary_render_expr/3` qualifies the outer row as `t.~w`, enforced by v6/prolog/conformance/fixtures/22_ref_column_collision.pl (fail-pre-fix WRONGs `colliding_ref_column_names_render_the_child_tree` + `one_colliding_ref_column_beside_a_disjoint_sibling` in the PR body) and plunit `sql_text_snapshots:ref_render_expr_qualifies_the_outer_column` + `:both_delta_reads_supply_the_render_alias` |
| 53 | half a node's identity crosses the wire, so a span-keyed consumer merges distinct nodes and reads the merge as a cycle | enforced | incident 2026-08-15 (`issues/df-span-identity-aliasing`, filed by the report_extract.dl6 rail): sprefa-extract declared df node identity as `(span.start, kind)` yet pushed every Rust value node with `len: 0` (`src/lang/rust.rs` df_push) and serialized `FlatFact::Edge` with endpoint spans and NO kind. Measured on the crate's own 33-file corpus: 22078 df node facts, 100% zero-width, 2144 spans carrying more than one kind, 2548 self-reaching nodes over 89463 reachable pairs, 430 of 462 ranked callables saturating the rail's depth-8 ladder cap in an intra-procedural graph that is a DAG. RCA chain: (1) the start-only span was justified by v5 `(line, col, kind)` parity, and v5 never shipped the span across a wire, (2) the wire's edge arm names only spans, so the kind half is unrecoverable downstream, (3) the depth ladder then needs an artificial cap to terminate and the cap reads as a measurement. Fail-pre-fix: `v6/sprefa-extract/tests/12_df_identity.rs`, three cases over the `src/dispatch.rs` shape (zero-width spans, missing `from_kind`/`to_kind`, `call_res`+`ret` merging into a 2-cycle), all three RED with the fix stashed. Rail = the Rust lift stores each value node's full syn extent and `FlatFact::Edge` carries `from_kind`/`to_kind` (skipped for cst, whose parent/child roles already separate a shared span), plus `v6/dl/dataflow/report_extract.dl6` keying every node rel on `(span, kind)` and reporting self-reaching nodes as a defect count. Receipt: self-reaching 2548 -> 0, aliased spans 2144 -> 581 (all implicit-`ret`-over-tail-expression, now separable), depth histogram 430-at-cap-8 -> a real 0..30 spread under a 32 termination guard. Still start-only: `src/lang/go.rs` and `src/lang/kotlin.rs` df_push, and the `FlatFact::DfArg`/`DfParam` aux arms |
| 54 | a test fixture names its temp directory from a clock reading, so two parallel threads draw the same path and race | enforced | incident 2026-08-16 (dl6-git-ref-ancestry arc): `v6/sprefa-engine-rs/tests/git_refs.rs` named fixtures `sprefa_git_refs_{pid}_{SystemTime::now().as_nanos()}`; the suite read 15/15, 15/15, then 13 passed 2 failed on the third whole-suite run, always inside the fixture builder with `git ["init", "-q"]: fatal: cannot copy '.../templates/description' to '.../\.git/description': File exists`. RCA: `SystemTime::now()` does not advance once per nanosecond on this machine, so two of the binary's parallel threads read one value, built one path, and ran `git init` into the same directory; the loser saw a half-written template tree. The clock reading looked like an identifier and was not one. Load-dependent, so it survives `--test-threads=1` and the first two runs -- which is exactly what the repo's measure-a-leg-three-times law is for. Fail-pre-fix: the suite itself under thread pressure, 1 red inside 12 attempts at `--test-threads=8` before the fix, 20/20 green at `--test-threads=8` after. Rail = a process-local `static FIXTURE_SEQUENCE: AtomicU64` supplies the name (`tests/git_refs.rs`), and `build()` clears any leftover at that path first, since the name now carries only this process's own pid and sequence. Remaining exposure: `tests/dep_resolve.rs:323` `checkout_root_fixture` still reads the clock; one test calls it today so it cannot collide with itself, and it will the moment a second does |
| 55 | pane fed a body one keystroke at a time (transport misses the watchdog deadline) | enforced | incident 2026-08-16: five flash4 lanes rc=1 `stalled: 30s with no harness activity`, empty worktrees, no opencode session rows. `send-keys -l` types a body rune by rune -- 10540 bytes measured still ingesting at 70s into a live opencode TUI, first session row ~110s after Enter, against a 30s `FIRST_SIGNAL_LIMIT` (hafley-rs crates/boop/src/supervise.rs:21). Second leg of the same defect: a TUI grouping a burst of typed input as one paste reads the Enter typed inside that window as a newline, fusing several coordinator hails into one message. The card's first RCA blamed control-mode `%error`, which is a real tmux parser fact and was never on the brief path (`git log -S'command(&["send-keys"'` is empty). Fail-pre-fix: `a_multiline_body_reaches_a_pasting_pane_bracketed_and_byte_exact` + `a_brief_sized_body_arrives_whole` (hafley-rs crates/boop-mux/src/lib.rs), RED with the impl reverted (`10401 of 10413 bytes in 10.101423709s`). Rail = `paste_body` sends every body through `load-buffer` + `paste-buffer -d -p` (bracketed only when the pane asked for bracketed paste) and the submit key waits `SUBMIT_GAP` 400ms after the paste; hafley-rs PR #10. Residual: the full `boop beep lane create` end-to-end spawn was not re-run under the rebuilt binary |
| 56 | a saved-state build option rewrites its own toolchain's installed files, and ships a binary macOS kills on sight | enforced | incident 2026-08-19 (dl6c arc): `qsave_program/2` with `foreign(save)` on SWI-Prolog 10.0.2 arm64-darwin ran `strip` IN PLACE over the five shared objects the compiler loads (`uri.so`, `json.so`, `crypto4pl.so`, `pcre4pl.so`, `files.so` under `/opt/homebrew/Cellar/swi-prolog/10.0.2/lib/swipl/lib/arm64-darwin/`; a second build from a different load order stripped 18 of them, the whole installed set), printing `changes being made to the file will invalidate the code signature in:` for each, and the 1083195-byte executable it produced answered `Killed: 9` rc=137 to `dl6c --version`. RCA chain: (1) `foreign(save)` copies each `.so` into the state through a strip step that writes the SOURCE file, not a copy, (2) macOS rejects a mach-o whose signature no longer matches, so every invocation of the saved state dies before `goal(main)` runs, (3) nothing in the build reads the exit code of the state it just wrote, so a build that produced a dead binary still printed success. The installed SWI was re-measured intact afterward (`crypto_data_hash(hello,H,[algorithm(sha256)])` = `2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824`, `library(pcre)`/`library(http/json)`/`library(uri)` all load). Fail-pre-fix: `v6/prolog/compile/scripts/dl6c_roundtrip.sh` runs the built executable and, with `foreign(save)` put back into `dl6c_save/1`, exits 137 at its `--version` line with zero of its twelve comparisons printed; without the option it prints twelve `ok` lines and `failures=0`. Rail = `dl6c_save/1` (`v6/prolog/dl6c.pl`) carries no `foreign(...)` option, so the state reloads the five `.so` files from the SWI installation at run time, and `v6/prolog/README.md` names SWI-Prolog 10 as the executable's one remaining run-time dependency. Second leg of the same signature story: `codesign --force --sign -` over the state answers `main executable failed strict validation`, so `install-dl6c` is rm-then-cp and the inherited adhoc signature is the one that runs |
| 57 | an emitted program document carries no shape version, so a binary built by one compiler interprets another's IR field by field | enforced | No incident yet; the rail is pre-emptive, filed with the dl6-build-single-binary arc. Exposure measured: `ProgramJson` (v6/sprefa-engine-rs/src/types.rs) had 26 fields and no version, 11 of them `#[serde(default)]`, so a field renamed or re-meant on one side deserializes to its default on the other and the program folds a silently different plan; the one version-shaped field, `incremental_safe`, was a constant `true` fossil (emit_rust.pl) nothing read. `dl6 build` makes the exposure reachable: a shipped binary outlives the tree it was compiled in. Rail = `ir_version/1` declared once in each emitter (emit_rust.pl, emit_ts.pl), stamped into the program document, and refused at boot by `GenProgram::try_from_json` (`ir_version_mismatch: program <name> was emitted at ir_version <n> and this runtime interprets <m>`) and by `IrVersionCheck.check` on the TS door (v6/tsv2/runtime/irVersion.ts, called from serve/0_compile.ts). Pinned by `v6/sprefa-engine-rs/tests/dl6_build.rs both_emitters_and_the_runtime_agree_on_ir_version`, which greps both `.pl` files and compares them to `program::IR_VERSION`; bumping one alone reds it. `incremental_safe` deleted from the emitter and both deserializers in the same change. Residual: the TS check runs at `ProgramCompiler.compile` only, so a hand-built program handed straight to `LiveEngine` states no version and is not asked for one. INCIDENT 2026-08-20 (jsonschema-rail-fix lane, HEAD 3993e44aa): the rail was reverted whole and nothing said so. `65607a8d5` dropped `ir_version/1` and both emission sites from emit_ts.pl and emit_rust.pl, and `IR_VERSION` plus `try_from_json` from program.rs, while leaving every CONSUMER standing: irVersion.ts, the dl6_build tests and build_template/main.rs still name them. Cost: the comment-budget pre-commit rail was red on an EMPTY index at HEAD (`program load returned 400 ir_version_mismatch: program main was emitted at ir_version none and this runtime interprets 1`), and `cargo check --all-targets` in sprefa-engine-rs did not compile. RCA: the STAMP had no test. `tests/irVersion.test.ts` and `dl6_build.rs` pin the CHECKER and the number's agreement, and both stayed green with no emitter stamping anything, because a grep for `ir_version(N).` over a file that has none asserts 0 == 0 only if the assertion runs at all; the file-count assertion is inside the same test that did not compile. Rail added: `plunit_tests.pl incremental_mode:both_doors_stamp_the_ir_version_the_runtimes_interpret` drives BOTH emitters over one fixture and asserts the stamp is in the emitted text, not in the source. `incremental_safe` came back with 65607a8d5 and both fields now ship. |
| 58 | an optional emitter loops, and the wall reads as a slow corpus instead of a stuck fixture | enforced (containment AND the loop) | incident 2026-08-19 (sweep-shard lane, HEAD 65607a8d5): a sequential stage-1 compile sweep measured 2m05.7s, and out/sweep.timings.tsv put 120.8s of it in two fixtures. RCA: `4_emit_jsonschema.pl` does not terminate on `recursive_enum_acyclic_tree_round_trips` or `recursive_enum_cyclic_values_store_and_render`, the two recursive-enum fixtures the relational-type-applications arc landed. `catch/3` cannot catch a goal that never returns, so before the alarm the whole corpus stopped at that fixture's position and every later fixture went unmeasured; the other 447 fixtures compile in 4.1s combined, so the sweep's whole cost WAS the loop. Rail = `bounded_emit/3` (v6/prolog/sweep.pl) puts each optional emitter (jsonschema, ts types, rust types) under its own `call_with_time_limit/2`, default 10s per the ten-second law, printing `SWEEP_EMIT_TIMEOUT <fixture> <step> <n>s` and dropping that one artifact; the fixture still compiles, still buckets, and still emits its module and schedule. Fail-pre-fix is the measurement itself: with the alarm at 60s the two rows read 60422ms and 60340ms, at 10s they read 10675ms and 10521ms, and the corpus's next-slowest optional emit is 123ms. Residual, and this is the real defect: those two fixtures have committed `out/*.schema.json` files, so the emitter USED to terminate on them and no test names the regression. The alarm buys a bounded sweep, not a working schema emitter. CLOSED 2026-08-20 (jsonschema-rail-fix lane): the loop was the emitter inlining a recursive enum. `enum_decl(tree, (leaf(...) ; branch(left: tree, right: tree)))` types the variant's own field with the enum, so `kind_schema/7`'s enum arm expanded the union at every occurrence and never bottomed out; 7_emit_ts_types.pl and 8_emit_rust_types.pl never looped because they NAME the type (`left: Tree`) instead of inlining it. Fix: `recursive_enum_row/2` in 4_emit_jsonschema.pl detects an enum reachable from itself and renders it as one `$defs` entry plus a `$ref` per occurrence, the shape the other two emitters already had; a bottoming-out enum still inlines, so no other fixture's schema.json moved. Measured: 10412ms and 10334ms (both cut off by the alarm) to 12ms and 13ms, zero SWEEP_EMIT_TIMEOUT lines. Fail-first: `plunit_tests.pl wrapper_composition:recursive_enum_column_renders_a_named_ref_and_terminates` FAILED (5.216 sec) with `throw(time_limit_exceeded)` before the fix. |
| 59 | a compiler stage keeps per-run scratch in a plain `dynamic` predicate, so turning the test battery parallel makes every stage clobber every other | enforced | incident 2026-08-20 (plunit-jobs lane, HEAD 67951ea94): `just plunit` moved from sequential to plunit's native `jobs(N)`, and the first parallel runs read 22, 25 and 18 failures against a known-red set of 8, a different set each time. RCA: `parse_dl_dcg.pl` holds `finding_fact/1`, `rel_column_order_fact/2`, `host_signature_fact/3` and `source_statement_fact/3` as `dynamic`, and `parse_dl_source/5` retractalls all four at entry, asserts into them mid-parse, and reads them back at exit. One clause store shared by every thread means two parses in flight erase each other's findings; the surviving failures were parse-shaped and scattered across `module_path_decls`, `dot_member_access`, `fact_seeding`, `json_grammar`, `rel_zero_arity` and `rel_template_and_is_clause`, none of them the tests' own fault. This is entry 54's class one layer down: 54 was a shared temp PATH, this is a shared clause STORE, and both only appear once something runs two of the thing at once. Fail-pre-fix: revert the declaration, run `just plunit` three times at `PLUNIT_JOBS=12`, read 22/25/18 failures; with it, three runs at 12 and three at 1 all read the same 8 names. Rail = `:- thread_local` on the four (`v6/prolog/compile/parse_dl_dcg.pl:30`), which gives each worker its own store and leaves the single-threaded path identical (conformance, roundtrip and text-door byte-identical output either way). Remaining exposure: `use_resolve.pl:25` `parse_count_fact/2` and `0_unsupported_messages.pl:137` `unsupported_inventory_memo/1` are still plain `dynamic`; both are keyed or idempotent so no test reads a wrong value today, and both become wrong the moment a second unit reads them. |
| 60 | a deleted module leaves a dangling load in a tool no gate loads, and the rail that does catch it is allowlisted instead of read | enforced | incident window 2026-08-12 to 2026-08-20, 8 days and 693 commits: `just self-map`, the release gate named at `v6/prolog/conformance/rulings.pl:532`, exited 1 on `FAIL  rels did not settle in 120s` at every commit from `81e1cf1bf` on. RCA: `81e1cf1bf` deleted `v6/prolog/compile/parse_dl.pl` and moved every production caller to `parse_dl_dcg.pl`; `v6/prolog/tools/self_map_facts.pl:187` still loaded the deleted path. Nothing but the `sh` host in `v6/dl/fixtures/self-map.dl6` loads that file, so no plunit unit, no conformance fixture, no lint step and no sweep stage reads it, and a whole-file deletion sweep over production callers cannot see it. The swipl one-shot exits 2 with `source_sink ... parse_dl.pl' does not exist`, `runInvocation`'s catchError settles all six projections `error` with zero rows and writes NOTHING to the server log (`v6/tsv2/serve/1_hosts.ts:765`), and the other three sources still parse, so sections 1 to 3 of ARCH-MAP.md still rendered and `write_arch_map` still wrote the file. The only visible symptoms were a settle timeout and an empty mermaid block in section 4. Measured on the unfixed tree with a per-poll instrument: `source=4 phase=11 construct=60 task=258 task_dep=144 map_document=1 write_receipt=written` from t=4s, `program_rel=0 program_edge=0` at every one of 30 polls, whole-read cksum unchanged from poll 4 to poll 30. Nothing churned and the 120s bound was never the cost. Second defect, and the one that bought the eight days: PR #373 wrote the failure text into `.github/CI-KNOWN-RED.md` as a staleness-gate row and CI has judged against that allowlist since, so the gate that did catch this went on reporting it into a file read as expected noise. Fail-pre-fix: with the one-word path reverted, `bash v6/tsv2/scripts/self-map.sh` prints `program_rel=0`, `program_edge=0` and `sm_rel error rows=0`, `sm_rel_edge error rows=0` before it dies. Rail = the load names `parse_dl_dcg.pl`, the only parser door; the settle failure prints per-rel counts and every `__host_witness` row that is not `done`; the staleness-gate row and its `allow:` line are off the known-red list. Receipt: three consecutive `just self-map` runs at 7.57s, 7.76s, 7.72s, each `SELF MAP HOLDS` with `diagrams=4 lines=692` and byte-identical output, and section 4 of v6/ARCH-MAP.md regenerating byte-identical to the copy committed 2026-08-11, the last day the rail was green. |
| 61 | two doors' FINAL readers disagree on a zero-row rel, so a Rust-door fold reads as a diff against a TS golden it actually matches | enforced (in the gate) | incident 2026-08-21 (ghcacher-rust lane, HEAD ba2daa779): all six `v6/tsv2/goldens/ghcacher_*` folded byte-identical TICK logs on the Rust door at the first attempt, and four of the six read FAIL on the FINAL line. RCA: `print_final` (`v6/sprefa-engine-rs/src/bin/emit_rust_harness.rs`) walks every key of `program.final_select` and prints `{"rel","columns","rows"}` per rel including rels whose `rows` is `[]`, while the TS golden's `3_expected.final.jsonl` is one `{"final": {rel: rows}}` object that OMITS a zero-row rel. The four differing rels were exactly the empty ones: `fresh_hit` (304 golden), `checkout_stale` (checkout golden), `env_override` (env golden), and `__host_demand_answer`, `best_rank`, `chosen_config`, `config_present`, `want_org` (config golden). No engine row was wrong and no tick line differed; the whole delta was presence-of-an-empty-array. Fail-pre-fix: drop the `select(.rows | length > 0)` filter from `fold_final` in `v6/dl/ghcacher/gate.sh` and the same four goldens go red with a one-key diff. Rail = the normalization lives in the GATE, never in the engine: `fold_final` folds the per-rel lines into the TS shape and drops empty arrays, `just ghcacher-rust` prints PASS/FAIL per golden, and the gate also diffs each copied `.dl6` against its `v6/tsv2/goldens/` original so a drifted copy fails instead of passing quietly. Receipt: three runs at 2.72s, 2.03s, 2.19s, `GHCACHER_RUST_DOOR_HOLDS goldens=6` each time. Residual: `5_expected.statements.jsonl` does NOT transfer between doors (the clock golden folds in 353 Rust-door statements against 698 TS-door ones), so no statement-count assertion is in the Rust gate; the per-golden count is printed for reading, not compared. |
| 62 | a decided design is half-built, and the unbuilt half's absence reads as a default: the marked host never landed, so the unmarked one answered every question | enforced (in the executor) | incident 2026-08-21 (soopy-rev-vs-worktree lane, HEAD da42943fc), user: "fix the soopy and extract git rev blob walk vs fs walk distinction, thought we already figured that out". `SoopyFilesExecutor::run` (v6/sprefa-engine-rs/src/hosts.rs:292 pre-fix) spelled `revision: soopy::Revision::Worktree` as a literal, so the `files` host walked the worktree and no host walked a commit. RCA: `rulings.pl:544` (`files_naming = files_unmarked_worktree_marked_rev`, user 2026-07-31, whose own words are "thought we already had this figured out") decides a PAIR of hosts, `files(glob)` for the worktree and `files_at(rev, glob)` for a pinned rev, and `registry.pl:190-191` names both as the supported spellings. Only `files` was ever wired: `grep -rn files_at --include=*.rs v6/` answered zero rows in the engine. The unmarked host was not defaulting a column, it was standing in for the twin that did not exist, which is why no reader could see the gap — the code had no revision parameter to be suspicious of. Same class as 57's fossil `incremental_safe`: a design landed on one side of a seam and the other side's silence read as a decision. Fail-pre-fix, both proven by running them: restoring `Revision::Worktree` as the `files_at` arm reds 3 of 7 in `v6/sprefa-engine-rs/tests/revision_walk.rs` (4 passed, 3 failed); making the host match fall through to `Revision::Worktree` instead of stopping reds 1 of 7 (6 passed, 1 failed). Rail = `files_revision/2` (hosts.rs) is a total match over the roster with NO fallthrough arm, so a name off `files`/`files_at` is a named stop and `files_at` without its `rev` column is a named stop; pinned by the seven tests, whose fixture is the only shape where the walks disagree (a committed file plus an uncommitted edit to it) and which assert BOTH content ids plus the fact that only the commit-walk digest is an object the database holds. Residual, all named and none fixed here: (a) `dep_resolve.rs:478` hardcodes `Revision::Named("HEAD")` inside `CheckoutTrees::head`, so a `dep_crawl` reads every checkout at HEAD with no way to say otherwise; (b) `sprefa-extract`'s `--resolve` door cannot take `(blob oid, path)` pairs at all — `ResolveRequest` (v6/sprefa-extract/src/project.rs:77-93) carries `paths: &[PathBuf]`, no revision and no content id, `read_inputs` (project.rs:449) hashes what it reads instead of verifying an id it was handed, and `SourceTreeBlobSource::open_files` (project.rs:1009-1014) pins `Revision::Worktree` with `expected: None` at all three call sites (:174, :259, :542) while the rev-capable constructor `open(root, revision, patterns)` (project.rs:971) sits unused; (c) `SourceTreeBlobSource::open_worktree` enumerates through the FS WALK (Blake3, sees untracked files) and `open_files` through worktree `SourceRef` reads, two notions of "the worktree" under one type; (d) `decode_content_id` (hosts.rs) reads any bare 64-hex digest as a legacy SHA-256, so a bare blake3 hex would be misread — not live, because `ContentId::Display` always prefixes. |

| 63 | a scaling DEFECT is closed on a fix that is conditional, and the condition's fallback is the unfixed code -- so the same stack overflow returns on the first program that misses the condition | open (v6/prolog/** , not this lane's tree) | incident 2026-08-21 (ghcache-verbatim lane, base sha 5d5cc07cc): `v6/dl/ghcache/ghcache.dl6`, 81 rules over 84 rels, parses, plans and type-checks clean, then dies in `compile.pl:239`'s `check_step(clock, check_clock_program(Prog))` with `Stack limit exceeded` inside `clock_violation/2`'s `setof` after ~20 min at the default stack; re-run at `--stack_limit=12G` it was still running at 3 min 14 s with RSS flat at 4.4 MB and was killed. RCA: `ARCH.pl:894` marks `clock_check_path_blowup` **done**, and its own text records the identical symptom from the atlas-variants lane 2026-07-31 ("Stack limit exceeded inside clock_violation/2's setof [...] INSTEAD OF REFUSING (self-diagnosis law: cliffs must be named, not fatal). Fix = offset algebra per SCC/edge, never path enumeration; plus a resource-bounded unsupported construct"). What landed is HALF of that: `recurrence_free_clock/6` (`3_clock_check.pl:478-491`) propagates offsets only when `zero_weight_cycles_only/2` holds and otherwise falls through to the old exponential `clock_path/7`, and the resource bound was never added at all. The comment at `:470-474` argues the fallback shape "is already refused one clause further down", but `clock_path_conflict` is clause-ordered at `:336` and `unconstructive_clock_cycle` at `:347`: the non-terminating clause is tried FIRST, so the refusal meant to contain the fallback is never reached. The class is 57's and 62's one layer up -- a design landed on one side of a seam and the untaken branch's silence read as coverage -- with the twist that the arc row's `done` is what stopped anyone re-measuring. Discriminating evidence that rule count is not the cause, same rig, same tree: a 31-rule linear chain compiles in 0.25s and a 27-rule reconvergent diamond ladder carrying 2^13 distinct simple paths compiles in 0.25s, while the 81-rule program does not terminate. Two more shapes measured, both cutting early and therefore FAST (9.7s, 21.8s, 25.4s on prefixes of the same file) because `clock_path_conflict` cuts on the first conflicting pair -- which is the sharp edge: **a CORRECT program of this size costs more to check than an incorrect one**, because a correct one gives the arm nothing to cut on. No rail is proposed here and none is claimed: the fix is `offset algebra per SCC/edge` plus the resource-bounded named refusal the 2026-07-31 row already prescribed, both inside `v6/prolog/**`, which this lane does not own. Fail-pre-fix for whoever takes it: `timeout 900 swipl -q -l v6/prolog/compile.pl -l v6/prolog/emit_rust.pl -g "compile_dl6('v6/dl/ghcache/ghcache.dl6','/tmp/x.rs',[emitter(emit_rust:emit_program)])" -g halt` must exit with a NAMED reason inside its budget rather than a stack overflow or a hang. Residual: `ARCH.pl:896` `inferred_clock_path_residual` states "NOT on the compile path (check_clock_program only calls clock_violation/2) so no compile cost" -- that sentence is why the compile path was believed safe, and it is true only of `inferred_clock/4`; `clock_violation/2` reaches the same enumeration through `recurrence_free_clock/6`'s fallback arm, so the compile path does pay it. |

| 64 | a lane's uncommitted work is "saved" to a branch that is the same commit it already pushed, and the salvage is believed rather than diffed | enforced (in the brief's own first step) | incident 2026-08-21 (dl6-run-watch lane died; pr-watch-resident lane inherited): the coordinator's brief stated that `wip/dl6-run-watch-salvage` carried the dead lane's LATER uncommitted work, naming four files and their line counts (`run.rs` 848, `runtime.rs` 682, `executors/clock.rs`, `executors/watch.rs`, 1718 lines total). Measured: `git rev-parse origin/feature/dl6-run-watch origin/wip/dl6-run-watch-salvage` answers `790dea415dbd5463fb045dcfdc7f2fc2abb53292` TWICE, `git diff` between the two tips is EMPTY, and `git ls-tree -r 790dea415 -- src/` carries `run.rs` (1148 lines, not 848) and no `runtime.rs`, `clock.rs` or `watch.rs` at all. A `find` over the repo, `/tmp`, every one of the 45 worktrees `git worktree list` names, and all ten `git stash list` entries returns zero hits for those three filenames. RCA chain: (1) the salvage step ran `git branch wip/... <sha>` and pushed it, which names a COMMIT and cannot carry a dirty index or worktree, (2) nothing compared the new branch's tree to the one it was supposedly rescuing FROM, so a no-op branch and a real salvage are indistinguishable at that step, (3) the line counts in the brief were read off the dead lane's live worktree before it was removed, so they described bytes that existed and then did not. Cost: the inheriting lane spent its first six tool calls proving a negative, and would have spent far more had it built on the premise. Fail-pre-fix is the measurement itself: the two `rev-parse` outputs are equal. Rail = a salvage branch is not believed until `git diff --stat <origin-branch> <salvage-branch>` prints a non-empty diff, and the FIRST action of any brief inheriting one is to run it and report the answer; a salvage that must carry uncommitted work uses `git stash create` or `git commit` INSIDE the dying worktree, never `git branch` from outside it. Residual: nothing in boop's lane-death path takes that commit, so the next dead lane loses its dirty tree the same way; the durable fix is a lane that commits every green step, which is now a standing line in the lane brief. |
| 65 | an optional host input the program never names, whose executor-side default is the most expensive answer the endpoint has -- so the cheap-looking call reads the whole history every tick | enforced (in the program, and in a hermetic ten-tick test) | incident 2026-08-21 (coordinator measured, prwatch lane inherited): `v6/dl/prwatch/prwatch.dl6` as merged in PR #407 declared `sh pulls(repo_slug: text, bucket: int)` and polled it every 60s. `v_tick_cost` read **6857541 wire bytes on every one of six ticks** and RSS climbed 47 -> 104 MB before the coordinator killed it. RCA, and it is TWO defects with one symptom. (a) `GhPullsExecutor::run` (`v6/sprefa-engine-rs/src/executors/pulls.rs:41-44`) reads `state` from the demand env and falls back to `"all"` when the program does not name the column; prwatch declared no `state` column, so every tick asked `state=all` over `PAGE_CAP = 5` pages of `PER_PAGE = 100`, up to 500 pulls of the repository's whole history, at ~13.7 KB per pull. The program looked like it was watching open pulls and was reading everything ever merged. (b) the conditional GET DID work -- proven, not assumed: a hermetic loopback test (`tests/executors.rs pulls_second_pass_is_a_304_and_moves_zero_bytes`) shows the executor sending `If-None-Match` and the second pass moving zero bytes -- but the tag lived ONLY in `fetch.rs`'s process-global `CONDITIONAL` map, so nothing durable carried it, a restart re-read everything, and with `sort=updated&direction=desc` over `state=all` any single pull updating reshuffled all five pages and invalidated all five tags together. Defect (a) is entry 62's class one more time: a half-specified call whose SILENCE read as a default rather than as a question, and the default was chosen for a different caller. Fix: the endpoint path is spelled in the PROGRAM now (`state=all&per_page=100&page=1&sort=updated&direction=desc`, ONE page, most-recently-updated first, which is the watch semantic the program actually wanted since a merged lane pull reaches the top the moment it merges), and `poll_state_etag` is a keyed rel in the one db carrying the tag into the next `gh_rest_cond` demand. Sub-finding worth its own line, because it is a language shape and not a bug: routing the tag back through the response directly makes the cycle `poll -> rest_response -> poll_state_etag -> poll` weigh ZERO (an edge rule triggered by a LEVEL rel grades +0, TICK-MODEL.md section 3) and the compiler refuses it as `unconstructive_clock_cycle(poll/3, poll_state_etag/2, rest_response/8)` -- measured. The tag therefore rides the EDGE-WRITTEN `call_log`, which grades the second hop +1, weighs the cycle 1, and applies the tag from the next tick, which is when the next poll happens anyway. Fail-pre-fix, both proven by running them: dropping `prev_etag` from the executor's inputs reds `pulls_second_pass_is_a_304_and_moves_zero_bytes` on tick 2; restoring the etag-from-response wiring reds the compile with the named cycle above. Rail = `ten_conditional_ticks_move_the_body_once_and_leave_rss_flat` (`tests/executors.rs`), ten conditional GETs against a loopback listener asserting `wire[0] > 0`, `wire[1..] == [0; 9]` and an RSS spread under 8 MB; hermetic, so the receipt costs zero GitHub points and does not need the network. Residual: `pulls.rs`'s `"all"` fallback is still there and still silent for any future caller that omits `state`; the honest shape is a named stop, and this lane did not change it because no measurement says which of its callers relies on the default. |


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

## 68. A symlink older than the crate it names

- **Incident** (2026-08-21): grading PR #410 in `~/projects/sprefa-worktrees/grade-410` failed with `cannot find function hash_object in crate soopy`. `soopy = { path = "../../../hafley-rs/crates/soopy" }` (`v6/sprefa-engine-rs/Cargo.toml:29`) resolved through `sprefa-worktrees/hafley-rs`, a symlink made 2026-08-17 to a hafley-rs worktree four days stale. Twenty minutes on a phantom API break.
- **RCA**: a relative path dep means every tree location needs its own sibling `hafley-rs`, and nothing checked that the sibling was the live checkout.
- **Fail-pre-fix**: `bash v6/tools/doctor-deps.sh` with the old symlink prints `DEPS STALE` and exits 1.
- **Rail**: `v6/tools/doctor-deps.sh` canonicalizes every hafley-rs path dep and requires it under `~/projects/hafley-rs` (`HAFLEY_RS` overrides); `grade.sh` runs it first, `just doctor-deps` runs it alone, and every lane brief's first action includes it.
- **Entry**: the symlink now points at `~/projects/hafley-rs`.

## 69. An ARCH row marked done for a half-landed fix

- **Incident** (2026-08-21): `ARCH.pl` row `clock_check_path_blowup` read `done` since 2026-07-31. `v6/dl/ghcache/ghcache.dl6` (84 rels) hit the same symptom the row describes: `Stack limit exceeded` inside `clock_violation/2` after 20 minutes. `recurrence_free_clock/6` had landed the fast path only for zero-weight cycles and fell through to the exponential `clock_path/7`; the resource bound the row prescribed was never written.
- **RCA**: a row flipped to done on the lane's word, with no program in the corpus wide enough to hit the fallthrough.
- **Fail-pre-fix**: compile `ghcache.dl6` with the prolog flag `dl6_clock_path_walk` true.
- **Rail**: user decision `rulings.pl clock_path_check_pinned_off`: the path walk is off the compile path (`3_clock_check.pl` `clock_path_walk_enabled/0`), the checker's own battery turns it on. The row stays open as the seed of the clock calculus.
- **Entry**: `ghcache.dl6` passes the clock step in 1.6s.

## 70. A keyword deleted from the surface while the runtime still keyed on it

- **Incident** (2026-08-22): the `bind watch(glob, ...)` / `bind interval(period, ...)` declarations were retired in favor of ordinary rels routed to `/soopy/watch` and `/clock/tick`, and `served-watch-rail.dl6` / `tick_cost_beat.dl6` were re-spelled to the new form. `run.rs::stays_resident` and the whole watch loop still read `BindPlanData` exclusively, so a program with zero `bind` decls (every re-spelled fixture) always answered `false` and folded once instead of staying resident: `one_touched_file_produces_exactly_one_extra_tick` and `a_resident_run_measures_itself_and_a_storeless_program_stays_flat` both reported 0 ticks past the first fold.
- **RCA**: the compiler-facing keyword and the runtime reader that keyed off it were changed by different people at different times with nothing pinning them together; `registry.pl` also had no `arrival_executor` rows for the two new slash paths, so `hosts::executor_for` had never heard of them either.
- **Fail-pre-fix**: `dl6 run tests/fixtures/tick_cost_beat.dl6 --final-tsv --final-only --final-rels tick_cost` printed zero rows and exited after one fold.
- **Rail**: `hosts.rs` gained `ExecutorCadence` (`Once` default, `Continuing` on `ClockExecutor`/`SoopyWatchExecutor`); `registry.pl` gained the `clock__tick`/`soopy__watch` `arrival_executor` rows the roster-parity test pins against `LINKED_EXECUTORS`; `run::stays_resident` now asks `hosts::cadence_for_plan` over the loaded program's own `host_plans` instead of reading a bind literal that no longer exists.
- **Entry**: both tests pass; `tests/dl6_run.rs`'s 9-test suite is green in 3 separate full runs.

## 71. A lane ported a traversal into an executor cache

- **Incident** (2026-08-22): `dep_crawl.rs` (added 2026-08-21, `456162553`) put a
  whole frontier-closure BFS over local checkouts inside a linked host
  executor, keyed by a per-process `FamilyMemo` so repeated demands answered
  from a cache instead of the demand graph. Three siblings (`git_refs.rs`,
  `git_history.rs`, `repo_at.rs`) copied the same `FamilyMemo` shape for
  ordinary single-call reads that never needed it: `hosts.rs`'s own per-tick
  demand dedup (`HostLiveRunner::collect`, claimed demands) already settles a
  family of sibling plan names in one pass, so the memo bought nothing an
  executor caller could not already get for free, while hiding a traversal
  that belonged in the language as an executor implementation detail.
- **RCA**: "the host answers several plan names from one soopy call" was read
  as "the host must remember its own answers," conflating the runner's
  once-per-tick claim with a would-be cross-tick cache, and a BFS frontier
  closure was written in Rust because dl6 rules did not obviously reach a
  directory-scanned repo roster, instead of writing the roster as a posted
  fact and the closure as a recursive rule.
- **Fail-pre-fix**: `v6/dl/crosswalk/gate.sh` leg 2 (`4_dep_crawl`) depended on
  `arrival_executor(dep_crawl_repo, '/soopy/dep_crawl')` et al.; deleting
  `dep_crawl.rs` alone left the golden program unable to compile.
- **Rail**: `dep_crawl.rs`, its `hosts.rs` roster rows, and its
  `registry.pl` `arrival_executor` rows are deleted; `FamilyMemo` is deleted
  from `src/executors/mod.rs` and every sibling that carried one.
  `v6/dl/crosswalk/goldens/4_dep_crawl.dl6` now derives `crawl_level` /
  `crawl_step` / `crawl_hops` as a hop-ceilinged recursive closure over a
  posted `repo_known` roster and `repo_grep_at`'s go.mod reads, the same
  `hop_ceiling`/`min(Level)` shape `crosswalk.dl6`'s `reach_level` already
  uses, so the traversal is the program's own rules and not an executor.
- **Entry**: `v6/dl/crosswalk/gate.sh` graded green with the rule-based
  crawl; numbers in the PR body.

## 72. A door the whole compiler goes through, and four tools that skipped it

- **Incident** (2026-08-22): `use soopy.` landed as the executor-module import, resolved in `use_resolve.pl` where every `use` line already lives, and 35 `.dl6` programs gained one. `compile.pl` reads them through `expand_uses/8` and stayed green; `6_profile.pl`, `compile/scripts/dl6_oracle.pl`, `compile/test/scip_namespaces.test.pl` and `tools/self_map_facts.pl` each call `parse_dl_dcg`'s `parse_dl_file/4` or `parse_dl_dcg_entry/5` directly, and a `use` line has never been a statement that parser accepts. The compile-speed gate stopped at `parse error at line 14` on the first pinned program; `scip_namespaces:receiver_rail_declares_the_registry_columns` read `sh_decl(call, ...)` where the registry says `scip__call`.
- **RCA**: the front door is `use_resolve.pl`, not the parser, and nothing named that. Four tools reached past it for a single-file surface read, which was harmless while every tracked program was use-free.
- **Fail-pre-fix**: `swipl -q -g "parse_dl_file('v6/dl/reach/feature-reach.dl6', P, _, _)"` throws `dl_parse_error(statement, position(1, 5))` on the `use cargo.` line; the same file through `expand_uses/8` returns a program.
- **Rail**: the three non-lab callers now go through `expand_uses/8`. The plunit unit `executor_modules` pins that all four spellings of one program compile to the same term, so a door that resolves one and not the others fails there rather than in a gate.
- **Entry**: conformance 440/0, plunit 1054/0, `scip_namespaces` green; compile-speed stays allowlisted red, now failing inside golden-flex's emit rather than at the parse of its first program (`.github/CI-KNOWN-RED.md`).

## 73. A cache the program could not see, and every restart paid for it

- **Incident** (2026-08-22): `executors/fetch.rs:31-50` kept the ETag and the previous body in a process-private `HashMap`, and `:255-260` substituted that body on a 304. `ghcache.dl6:308` already carried `poll_state_etag` relationally, so the map was a second copy that no rule could read, no query could show, and no restart could keep. `issues/dl6-run-restart-loses-etags`: every `dl6 run` restart re-downloaded roughly a megabyte. Under it, `sql.rs::run_program_ddl` dropped every table the program declares at each boot, so the relational copy died too.
- **RCA**: an executor that answers a QUESTION grew state that answers it DIFFERENTLY on the second ask. Once the cache is invisible to the program, the program cannot be the thing that decides when to re-ask.
- **Fail-pre-fix**: `sql::tests::a_restart_keeps_a_table_whose_shape_did_not_move` reads 0 rows with the `table_shape_stands` arm deleted.
- **Rail**: the transport is `executors/http.rs` and holds nothing but a connection pool: every header on the wire, `If-None-Match` included, comes from the demand row's `headers` column. `run_program_ddl` keeps a TABLE whose CREATE is the one already standing and drops one whose shape moved.
- **Entry**: kill, restart, first poll is 8 x 304 with `bytes = 0` out of 8 stored ETags and 8 stored bodies.

## 74. Seconds compared against minutes, twice, in one program

- **Incident** (2026-08-22): `ghcache.dl6` compared `poll_interval_seconds` directly against `current_clock(60, Bucket)`, whose bucket advances once per MINUTE, so a 60s poll fired hourly (`issues/ghcache-dl6-poll`). The same shape sat in `over_budget`, comparing `x-ratelimit-reset` (epoch SECONDS) against the same minute bucket: measured live, once the stop threshold tripped, `Bucket < ResetAt` stayed true forever and the poll never resumed.
- **RCA**: two quantities with the same name (`seconds`) and different units met with no conversion and no type to stop them. The first was filed and the second was invisible until a live run drove the budget down.
- **Fail-pre-fix**: `v6/dl/ghcache/gate.sh` asserts `due=3` over three consecutive buckets; before the fix it read `due=1`.
- **Rail**: a `clock_granularity(60)` fact is the one place the unit lives; every period is `ceil(seconds / granularity)` buckets and the reset is `ResetAt / granularity`. The gate prints `due`, the 200 count, the 304 count, the 304 byte total and the minimum `rate_remaining` on every run.
- **Entry**: `GHCACHE_RUST_DOOR_HOLDS ticks=10`, `due=3 call_log 200=1 304=3 304_bytes=0 rate_remaining_min=4997`.

## 75. A statement counter that charged a batch of 48 as one

- **Incident** (2026-08-22): the shared-frontier arc's whole claim is fewer SQL statements per tick, and the Rust seam's own tally could not see the difference. `sql.rs::execute_multiple` ran `execute_batch` and recorded nothing in `SEAM_TALLY.statements`, while `execute` recorded one per call. Every per-rel clear, promote and merge goes through `execute_multiple` as one `";\n"`-joined batch, which is exactly where the shared arm removes statements. Measured on `sf_join`, both arms reported `statements=27`; with the batch counted they read 367 vs 290 on `wide_4`.
- **RCA**: the counter counted CALLS to the seam, and the name said statements. A batch is the one shape where those two numbers diverge, and it is the shape the optimization targets.
- **Fail-pre-fix**: `sql::tests::a_batch_reaches_the_seam_tally` reads `left: 0, right: 2` with the `fetch_add` removed (run and read, 2026-08-22). `a_batch_counts_every_statement_in_it` pins the quoting case a naive `split(';')` gets wrong: `INSERT INTO "t" VALUES ('a;b');\nDELETE FROM "t"` is 2 statements, not 3.
- **Rail**: `batch_statement_count/1` splits outside quotes and `execute_multiple` adds it to the tally, so `report_seam_tally`'s `statements` is every statement SQLite ran. `v6/sprefa-engine-rs/shared-frontier-bench.sh` prints it per arm.
- **Entry**: cargo 158/0 unchanged by the counter; `wide_64` reads per_rel 5,767 vs shared 4,250 statements per fold, identical across three runs of each arm.

## 76. A rule-kind check refused a construct the column type allowed

- **Incident** (2026-08-22): `analyze.pl:1104` fired `edge_body_needs_json_destructure` for any `decode/2` in a `<+` body, matching on the rule KIND alone and never reading the source column's declared type. The stated reason was the SLOT-TERM-STRUCT encoding: a compound value ARRIVING into an UNTYPED column is stored as canonical term text, which json1 cannot read. That reason has nothing to say about a column declared `json`, which is what every real use decodes. The cost was a `_seen` level twin per fold: `ghcache.dl6` carried four rels whose only job was to host a decode so the keyed `<+` beside them could read variables.
- **RCA**: the stop was written where the type environment does not exist (`edge_trigger_shape/2` sees only the body term), so the cheapest check available was the rule kind, and the cheapest check became the language rule. Nothing re-asked once `RelPlans` existed one stage later.
- **Fail-pre-fix**: conformance fixture `json_decode_in_an_edge_body_folds_a_keyed_row` (`v6/prolog/conformance/fixtures/8_json_flex.pl`) compiles to `unsupported_construct(edge_body_needs_json_destructure(...))` at the base commit; the oracle already ran it, so the two doors disagreed.
- **Rail**: the decision moved to `lower.pl:check_edge_decode_sources/3`, which calls the LEVEL arm's own `json_decode_goal/3` over the shape's positive atoms, and edge bodies compile decodes through the level arm's `compile_json_decodes/7`. The plunit unit `edge_body_json_decode` pins the emitted upsert, the guard-before-extract order, the `json_each` spread join, and that an untyped source is still named.
- **Entry**: conformance 444/0, plunit 1065/0, RUST-GRADE graded=444 byte-clean=340 (the four new fixtures pinned into `graded.tsv`), `GHCACHE_RUST_DOOR_HOLDS ticks=10` with `due=3 call_log 200=1 304=3 304_bytes=0 rate_remaining_min=4997`.

## 77. A demand whose identity its own answer rewrites

- **Incident** (2026-08-22): the resident `dl6 run v6/dl/ghcache/ghcache.dl6` died on `drain overflow: ghcache exceeded 100 host/drain ticks in one batch`. `poll_state_etag` fed BOTH the `If-None-Match` header and the `prev_etag` demand-identity column of `http.get`, and the answer wrote `poll_state_etag`. GitHub answers one resource with `W/"tag"` and `"tag"` depending on the request, so the two spellings chased each other: measured a period-4 cycle on `.../events?page=3` with `rate_remaining` flat at 4967/4964/4961/4956, no wire traffic, `change_log` gaining 64 rows every 6 drain ticks. `page_queued` is a keyed fold that never retires, so every past bucket's page kept re-deriving its question on top of that.
- **RCA**: a host demand is identified by its inputs, so an input the answer writes turns one question into a walk through the answer cache. `fold` dropped its accumulated delta lines on bail, so nothing named the rel.
- **Fail-pre-fix**: `tests/dl6_run.rs::a_drain_overflow_names_the_loudest_rels` folds `tests/fixtures/drain_identity_loop.dl6`, whose `salt` is an `env.var` identity input its own answer bumps; before the report existed the bail named no rel.
- **Rail**: `poll_state_etag` carries the tag its bucket was ASKED with beside the tag the answer gave, so `page_stored_etag` reads the same value before and after the answer lands; `page_fetch`'s page-walk arm joins `current_clock`; `due` requires `api_token`. `LiveLoop::fold` keeps the last 6 drain ticks and names the three loudest rels in the bail, rows at debug level only because a demand row carries `Authorization`.
- **Entry**: cold start walks `page=1..10` at one 200 each and goes quiet; `v6/dl/ghcache/gate.sh` reads `due=3 call_log 200=1 304=2 304_bytes=0 rate_remaining_min=4997`.

## 78. A resident runner armed the clock and never the summary

- **Incident** (2026-08-23): `dl6 run`'s live `engine_tick_cost` rows read `wall_ms=0 sqlite_ms=0` on every bucket, and every ordered-path statement traced as `verb=unlabelled relation=-`. `run.rs:694`'s `boot()` called `crate::trace::arm()`, which only pins the wall clock a fold is measured against; `trace.rs:100`'s `record()` and `:112`'s `record_scope()` both return early unless `summary_wanted()` reads `DL_TRACE_SUMMARY` or the `FORCED` flag `force_summary()` sets, and nothing in the resident path called it. The self-diagnosis law asks the system to answer "what was it doing" from its own on-disk trail; the trail read zero for every field that mattered.
- **RCA**: `arm()` and `force_summary()` are two different doors (one clock, one recording gate) that happen to sit next to each other in the doc comment; a caller that reads the name `arm` and assumes it turns tracing on skips the second call.
- **Fail-pre-fix**: `tests/tick_trace.rs::the_wall_row_carries_a_real_sqlite_ms_once_armed` folds `ghcache.dl6` against `ghcache.schedule.json` and reads `dl_tick_cost`'s `wall` row; with `force_summary()` commented out of the test it reports `sqlite_ms: Number(0)` on every field (run and read, 2026-08-23).
- **Rail**: `run.rs:694` calls `crate::trace::force_summary()` beside `arm()`, so a resident fold always records regardless of `DL_TRACE_SUMMARY`. `tests/tick_trace.rs` pins the wall row's `sqlite_ms > 0`. Labeling every ordered-path statement (the `verb=unlabelled` half of this incident) landed in `ordered.rs` via `@ordered-tick-recompute` (PR #423, merged 30fbd3669); some `unlabelled` calls remain (below), tracked separately.
- **Entry**: live `dl6 run v6/dl/ghcache/ghcache.dl6` receipt against the merged tree (main + PR #423 + the `_recent` graphql selection), bucket 29791102: `ddl wall_ms=92 sqlite_ms=92 calls=1638`, `boot wall_ms=18 sqlite_ms=18 calls=412`, `recompute/page_response wall_ms=4 sqlite_ms=4 calls=65`, `recompute/pull_request_seen wall_ms=1 sqlite_ms=1 calls=9`, `unlabelled wall_ms=0 sqlite_ms=4 calls=997` (down from 1343 pre-#423, not yet zero).

## 79. A tick that read every rel to find out nothing changed

- **Incident** (2026-08-23): `ordered.rs::run_tick` read all 154 ghcache rels five times and rebuilt all 100 levels twice per tick, then cleared 154 frontier and 154 next-frontier tables, whatever had arrived. Measured through `SEAM_TALLY`: 1878-1925 statements per tick across all 11 ticks of `ghcache.schedule.json`, with the two zero-arrival ticks costing the same as the working ones. `ARCH.pl:855` pinned this on 2026-07-30 as F5 and assigned the fix to `pre_occurrence_loop`, which landed without removing the recompute.
- **RCA**: the tick's deltas were produced by diffing whole-table before and after snapshots in Rust, so the path had no per-rel change signal and read everything to find out what moved. The signal was already there and unused: every seam write returns `rows_changed`, and the frontier tables answer their own emptiness.
- **Fail-pre-fix**: `tests/ordered_statement_count.rs::an_ordered_tick_costs_its_change_not_the_program_size` folds the ghcache schedule and reads the tally between ticks; at def5dbb63 it reports every one of the 11 ticks over its cap, zero-arrival ticks included (run and read three times, 2026-08-23).
- **Rail**: `TickDirty` marks a rel from the `rows_changed` the write already returned, after arming that rel's before-snapshot; a level recomputes only when a rel it reads moved since that level last ran; a level whose recompute reads a table that is no rel's base table (a frontier, a `__pre_` snapshot, a plane table, a CTE) never skips, and neither does one over a retained rel, whose retention runs after the tick's last recompute. One chunked `EXISTS` probe opens the tick and answers which frontiers hold rows and which base tables are empty. A TEMP table dies with the connection, so its absence is this process's first tick against this db and that tick rebuilds every level: the tick is not transactional, and a process killed mid-tick leaves level tables inconsistent with their sources, which the dirty set alone would heal only when a rel the level reads next moves. The COUNT test caps a zero-arrival tick at 100 statements and a one-arrival tick at 450, asserts tick 0 recomputes every level and tick 1 fewer than all of them, and compares the 11-line tick log byte for byte against `tests/fixtures/ghcache_ticklog_base.txt`.
- **Entry**: statements per tick 1890 -> 443 (t0, which rebuilds all 100 levels), 1902 -> 367 (t5, 36 levels), 1881 -> 59 (t9, 6 levels), three runs each, identical. conformance 444/0, plunit 1076/0, RUST-GRADE graded=444 byte-clean=340, cargo 163/0, ghcache ticks=11, goldens=6, ARCH 7/0.

## 80. One arm's body put the whole module on a rebuild loop

- **Incident** (2026-08-23): `emit_rust.pl:216` `ordered_program/1` walked the edge-statement list and, on finding ONE statement of kind `ordered_arrival`/`ordered_departure`, set a module-wide flag. `program.rs:179` branched on it into `ordered.rs::run_tick`, where every `<-` level was rebuilt from its base tables twice a tick and every rel was snapshotted and diffed in Rust. ghcache carries 5 such arms out of 52, and paid for 100 whole-level rebuilds a tick to get them. A program with 50 `<-` levels and one `pre/1` arm recomputed all 50 twice for an arrival no level reads.
- **RCA**: the sequencing requirement is per ARM (occurrence N+1 must see occurrence N's write, which is true only for a body reading `pre/1` or negating a rel another arm heads), but the flag that carried it was per MODULE, and the engine it selected answered a different question (how does a level settle) than the one the arm asked (in what order do occurrences run).
- **Fail-pre-fix**: `tests/one_tick_path.rs::a_pre_arm_does_not_pay_for_levels_it_never_touches` builds that program and reads `incremental::level_runs()` per tick; on `ordered.rs` the increment tick ran 100 level statements, here it runs 0 (run and read three times, 2026-08-23).
- **Rail**: `ArmSchedule` rides each `IncrementalEdgeStatement`, decided from the edgestmt kind, per arm. A `Sequenced` arm walks its trigger's own frontier in `(_phase, _sequence)` order inside `apply_edges`; every arm of one occurrence projects before any of them writes, so a seeded `pre` arm and its direct twin read the same store; the walk is occurrence-major across every sequenced arm, because two arms on different trigger rels folding one key are refereed by the arrival index (ruling `one_pick_order`), not by source order. A row a sequenced arm writes and then overwrites in the same tick stages its NET into the carry, never both states. `TickWork::probe` opens the tick with one chunked `EXISTS` read; every write marks, nothing unmarks, and `level_sources` maps every table name a rel owns back to that rel so a level runs only when a rel it reads moved. A level that reads a table belonging to no rel never skips. `ordered.rs` is deleted.
- **Entry**: ghcache statements/tick on the ordered path with #423's dirty set 447,178,224,483,249,505,367,70,143,256,165,658,264,58 -> on the one path 475,522,1172,1364,1318,1771,1200,702,691,676,668,1643,1208,199; the two are not comparable statement for statement (the ordered number counted whole-table rebuilds, this one counts per-level delta inserts), and the tick log is byte-identical. Settled idle tick 3 statements. `unlabelled` calls inside ticks 997 -> 0, asserted. conformance 444/0, plunit 1076/0, RUST-GRADE graded=444 byte-clean=340, cargo 163/0, ghcache ticks=14 pr_transition_open_merged=1, goldens=6, ARCH 7/0.

## 81. An aggregate never rescoped on the rel its own body negates

- **Incident** (2026-08-23, disclosed by 80): `lower.pl` `aggregate_scope_seed_sql/6` built one scope-seed arm per POSITIVE body atom. An aggregate head whose body negates a rel therefore had no seed arm for that rel, so when it moved the head kept a stale row. Probe: `head(S, min(O)) <- slot(S,O), not(drained(S,O))` still read `["s",1]` after `drained` gained `["s",1]`. The conformance fixture `concat_program_queue` covers exactly this shape and was byte-clean only because a `<+` arm in the same program put it on `ordered.rs`, which rebuilt every level from base tables and could not go stale.
- **RCA**: the predicate's own header states the law ("over-approximating the scope is SAFE, under-approximating would not be") and the code then filtered to `is_positive_use`. A negated atom is a delta source for exactly the same reason a positive one is: it decides membership.
- **Fail-pre-fix**: the probe above folded through `emit_rust_harness`, tick 2 read `{"drained":{"add":[["s",1]]}}` with no `head` delta; after the fix it reads `head add ["s",2] del ["s",1]` (run and read, 2026-08-23).
- **Rail**: a second `aggregate_scope_seed_sql/6` clause emits a seed arm per negated atom whose own args bind every group column. `lower.pl` is shared, so `emit_ts.pl` output moves for those programs; exactly one committed artifact changed, `compile/out/concat_program_queue.ts`, and it was regenerated.
- **Entry**: OPEN, needs the user. A negated atom that does NOT bind every group column still seeds nothing, which is the same silent under-approximation for a narrower shape. The two candidate closes are (a) refuse the program by name the way a positive atom does (`aggregate_group_not_delta_local`), which newly refuses programs that compile today and silently answer wrong, or (b) rescope every live group of that head from the head table when the negated rel moves, which never refuses and costs one statement. Neither was taken in this arc: it is a compile-surface decision.

## 82. Grading a live lane's worktree shares its cargo target

- **Incident** (2026-08-23): the coordinator's grade of PR #427 read `cargo passed=136 failed=1` then `94 failed=8`, and `gate.sh` read `200=2 ... pr_transition_open_merged=0` twice, while the lane reported all green. A third run with the lane's builds finished read 163/0 and `200=5 pr_transition_open_merged=1`.
- **RCA**: the coordinator ran `cargo test` and `gate.sh` inside the lane's worktree while the lane itself was running `cargo clean`/`cargo build` there; both share `target/`, so half-built binaries and a partially rebuilt `emit_rust_harness` folded the gate.
- **Fail-pre-fix**: none automated; the receipt is the three runs above.
- **Rail**: grade in a coordinator-owned checkout (`git worktree add <scratch> <branch>`), never in the lane's worktree while the lane is live; or hail the lane to stand down first. Red numbers from a shared `target/` are not evidence either way.
- **Entry**: #427 merged at ecce409d5 after the third run.

## 83. A level's own write was in the set that decided whether it ran again

- **Incident** (2026-08-23, disclosed by 80): #427 put every program on the incremental path and ghcache's 14-tick fold went 7,113 -> 16,655 statements and 150 -> 286 ms. The gate `level_runs_this_tick` asked "did a rel this head reads move THIS TICK", over a set nothing ever removes from. A head's `support_sql` reads its own base table, so `level_sources` lists the head among its own sources, and the head's own insert marked it. Every one of ghcache's 89 non-aggregate heads therefore satisfied its own gate on the second pass, and both `apply_levels_after_edges` and `recompute_levels_after_edges` re-ran work whose inputs had not moved since the first pass: `recount` 8,279 statements over the fold with no skip, `level_insert` 1,873.
- **RCA**: the question a gate over a monotone per-tick set can answer is "did anything move at all", and the question an operator has is "did anything move since I last looked". The first is the second with the clock deleted, and once the answer is yes it stays yes for the rest of the tick, including for the operator's own output.
- **Fail-pre-fix**: `tests/ordered_statement_count.rs::an_ordered_tick_costs_its_change_not_the_program_size` sums `SEAM_TALLY` over the fold; at 13527429a it reads 13,609 against the 10,400 cap and `recount` dispatches 8,279 against the 6,000 cap (run and read three times, 2026-08-23).
- **Rail**: `TickWork` carries a monotone per-tick clock. `mark` stamps the rel with it; a level records the reading its run started from, keyed by `(LevelPhase, head)`, and runs only when a source carries a newer stamp. The reading is taken AFTER the run, because the only rel a level marks while running is its own head and that write is its output; a head reading its own frontier (`recursive_heads`) keeps the pre-run reading so its rounds still close, which is why `level_sources` takes those heads as an argument rather than recomputing them and doubling the one per-program frontier scan `n_plus_one.rs` caps. The same "which table actually holds a row" question drives the two frontier clears: `stage_events` and the recount tail record the copy tables they filled, the mid-tick merge runs for rels that filled the carry, and the promote for rels holding a row in either frontier.
- **Entry**: fold statements 13,609 -> 9,884, wall 276 -> 235 ms, `recount` 49,421 us / 8,279 calls -> 36,826 / 5,630, `level_insert` 88,415 / 1,873 -> 61,939 / 1,284, three runs each, identical counts. `ghcache_ticklog_base.txt` byte-identical. NOT at the pre-#427 7,113 / 152 ms receipt, and the remainder is not in this file: `page_response`'s emitted delta insert is 248 KB of 256 UNION ALL arms, one per subset of its 8 body items, against 64 plain joins for the same rule's rebuild, and it costs 3.7 ms a call steady-state against 0.67 ms for the rebuild (`DL_EXPLAIN=1` over 1,232 distinct statements reports zero inner SCANs, so this is arm count, not a missing index). On `wide_64`, where every level is one arm, the delta wins 10 us to 20. The switch is the arm count, never a row count, and taking it needs the snapshot-and-diff machinery #427 deleted. Filed as `delta-arm-subset-expansion`. conformance 444/0, plunit 1076/0, ARCH 7/0, ghcache ticks=14 pr_transition_open_merged=1, goldens=6.

## 84. A tick was many autocommit statements, and a kill mid-tick left them half-promoted

- **Incident**: a tick's SQL runs as ordinary autocommit statements, one at a time, no surrounding transaction. Entry 79's own rail names the consequence: "a process killed mid-tick leaves level tables inconsistent with their sources, which the dirty set alone would heal only when a rel the level reads next moves." PR #423's `TickDirty` recompute guard made that window durable rather than closing it: a level whose inputs did not move again is never revisited, so half-promoted state from a killed tick can persist past every later tick, not just the first restart.
- **RCA**: SQLite autocommit means each statement is its own transaction. A tick that writes to a dozen base and level tables through a hundred-plus statements has no atomic boundary around the whole; a SIGKILL between statement 40 and 41 leaves 40 committed and the rest never run, and nothing downstream knows the tick did not finish.
- **Fail-pre-fix**: `tests/tick_transaction.rs::a_failed_tick_leaves_the_file_db_at_the_previous_tick_state` folds `diverging_measure_recursion` against a file db; tick 2's `seed(0)` arrival lands in the persistent `seed_number` table via `apply_arrivals`, then the head relation's `value := value + 1` recursion never reaches a fixpoint and the tick aborts past `round_cap` (`BoundaryError::DivergingMeasureRecursion`). With `drive_tick_transacted` reverted to call `drive_tick` directly, reopening the file reads `seed_number` row count 1 where the previous committed tick left 0. `a_tick_transaction_costs_exactly_begin_and_commit` reads a statement delta of 0 instead of 2 under the same revert (run and read, 2026-08-23).
- **Rail**: `SqliteSeam::begin_tick`/`commit_tick`/`rollback_tick` (`sql.rs`) run `BEGIN IMMEDIATE`/`COMMIT`/`ROLLBACK`, counted in `SEAM_TALLY` beside every other statement; a `begin_tick` called while a tick transaction is already open returns `TickTransactionError::NestedBegin` rather than nesting silently. `driver::drive_tick_transacted` wraps one call to `drive_tick`: an `Err` rolls back and propagates, `Ok` commits. `run_schedule`, `run_schedule_live` (`driver.rs`) and `LiveLoop::fold` (`run.rs`) call it instead of the bare `drive_tick`.
- **Entry**: PR numbers and gate numbers in the PR body.

## 85. A recount loss reason expired before downstream heads ran

- **Incident** (2026-08-23): `recount` was 5,630 of ghcache's 9,884 fold statements. The first gate sampled deleted rows before the recount pass. On tick 6, a recount deleted `pr_due(acme/widgets, 0)`, but its later topological reader did not see that new loss. The engine added `pr_batch_member(1, 0, batch/0, repo_1)` and removed it on tick 7.
- **RCA**: a loss reason belongs to the whole tick. Reading it once before the pass misses rows deleted by an earlier reconcile, and consuming it after one reader misses the after-edge pass when a staged frontier replays an earlier addition.
- **Fail-pre-fix**: conformance fixture `recount_retraction_reaches_two_heads_same_tick` sends one row through `a -> b -> c`, retracts `a`, and requires both `b` and `c` to retract in that tick. `tests/recount_gate.rs` pins five additive ticks at zero recounts, one positive retraction at one recount, one negated addition at one recount, and byte-identical tick logs against the always-recount run. `callgraph_unused_inverts_with_the_call_set` tick 4 also pins the temporary compatibility case: a negated input loss must still add `unused(main)` until `delta-arm-subset-expansion` emits that insert arm.
- **Rail**: the chunked tick probe carries four columns per rel, with the fourth reading `_sign = -1` from the delta table. `TickWork` keeps tick-long grown and shrunk sets. Arrival, edge, frontier, and reconcile writes mark their direction, so later heads and recursion rounds read changes produced inside the pass. A refCount head recounts for a positive input loss or negated input gain; negated input losses were also eligible until entry 89 retired that compatibility case.
- **Entry**: ghcache fold statements 9,884 -> 7,522 and recount calls 5,630 -> 3,268. Across three release runs, median recount time was 42,240 -> 31,311 us and total wall was 253,962 -> 256,627 us. `tests/fixtures/ghcache_ticklog_base.txt` stayed byte-identical. Conformance 445/0, plunit 1082/0, RUST-GRADE 445 with 341 byte-clean, cargo 168/0, ghcache ticks=14 with `pr_transition_open_merged=1`, goldens=6, ARCH 7/0.

## 86. One probe bit answered for two tables, and one boundary set for two moves

- **Incident** (2026-08-23): on `wide_64` (64 `source` rels, 64 `heavy` rules, 128 rels, a program that never writes a next frontier) a busy tick spent 640 of its 1,155 statements on frontier housekeeping over empty tables. `prepare_tick` emptied a rel's delta AND its next frontier off one probe bit that ORed the two. `promote_frontiers` ran three statements for every rel holding a row in EITHER frontier, so 128 rels paid a `DELETE` of an empty carry and an `INSERT` reading it. `read_staged` then probed all 128 carry tables to learn what no write had reached. On the shared arm an idle tick cost 65 statements and never fell: every head's `support_sql` writes `__support_count`, a table owning no rel, which set `always` on its `LevelSources` and defeated the level gate entries 83 and 85 landed.
- **RCA**: a rel that filled its current frontier is not a rel that filled its carry, and a boundary's `DELETE` and its `INSERT` read different tables; one set cannot gate both. The shared strategy keys every rel's rows in three tables by `relation_id`, so those names carry no rel, and `level_sources` had one branch for a table it could not attribute: never skip.
- **Fail-pre-fix**: `tests/empty_delta_skip.rs::a_wide_program_pays_the_frontier_boundary_only_for_the_rels_that_moved` folds `wide_64` on both arms, three busy ticks then three idle. With `incremental.rs` and `write_verbs.rs` reverted to effa67c95 it reads `per_rel: a busy tick cost 1155 statements over 128 rels` against the 7-per-rel cap, boundary statements 640 against the 2-per-rel cap, one carry read over a program that stages no carry, and a shared idle tick of 65 against the cap of 2 (run and read, 2026-08-23).
- **Rail**: the tick probe carries five columns per rel, the delta and the next frontier reading separately (entry 85's four became five). `TickBoundary` has one variant per table it touches, `PrepareDelta`, `PrepareNext`, `Merge`, `PromoteDrop`, `PromoteMove`, and `clear_boundary` skips the variant whose rel set is empty. `holds_frontier` answers for the current frontier alone and `carries` for the carry; `merge_next_into_current` records the current frontier its copy fills, so the promote that follows still drops it. `read_staged` runs only over rels that staged a carry row, and the departures set answers for a rel in neither. `shared_plane_table` reads `__frontier`, `__next_frontier` and `__support_count` as "any rel moved since this head last ran", against the monotone tick clock, rather than as "always".
- **Entry**: `wide_64` per_rel busy tick 1,155 -> 771 statements (9.02 -> 6.02 per rel), boundary 640 -> 256; shared busy 647 -> 644 and its settled idle tick 65 -> 1; per_rel settled idle tick 1, unchanged. ghcache fold 7,522 -> 6,886 statements. `tests/fixtures/ghcache_ticklog_base.txt` byte-identical. Conformance 445/0, plunit 1082/0, RUST-GRADE graded=445 byte-clean=341, ghcache ticks=14 with `pr_transition_open_merged=1`, goldens=6, ARCH 7/0.

## 87. A defaulted read became one clause per presence combination

- **Incident** (2026-08-23): compiler phase 45 expanded every LEVEL `coalesce/2` into present and absent clauses before lowering. Six optional reads in ghcache `page_response` became 64 recompute statements, 256 delta arms, and 248 KB of insert SQL. The statement ran 5 times per fold at about 3.7 ms per call.
- **RCA**: the shared expander encoded row absence with a second clause so SQL lowering saw only ordinary atoms and negation. Repeating that binary split for each optional read created every presence combination even though SQL and rxjs both have one use-site default operator.
- **Fail-pre-fix**: `delta_arm_count` measured one driver plus three optional reads as 8 clauses and 20 arms. The inline two-read rail now requires one clause, 5 arms, 2 outer joins, 2 `COALESCE` projections, 4 set differences, and 2 refCount change markers. The graded retraction fixture requires the default row to leave when the optional row arrives and return when it leaves.
- **Rail**: the compiler path preserves validated LEVEL wrappers through expansion, while the oracle and EDGE paths keep their prior split. LEVEL lowering emits one `LEFT JOIN` and `COALESCE` per optional read. Each optional contributes gain and loss arms over the current outer-join projection. The refCount query remains one grouped outer join and records each optional source in a zero-row `NOT EXISTS` span so runtime invalidation sees both presence transitions.
- **Entry**: `page_response` is 1 clause, 13 arms, and 10,110 bytes of insert SQL. Its three release `level_insert` readings were 1,206, 1,132, and 1,217 us over 5 calls, or 241, 226, and 243 us per call. Fold statements were 6,730 in all three count runs; release walls were 176,907, 179,476, and 174,590 us. The 7,113 statement target was reached; the historical 152 ms wall was not reached on the merged transaction, recount, and empty-delta runtime. `ghcache_ticklog_base.txt` stayed byte-identical. Generated TypeScript moved only for `coalesce_default_returns_when_source_retracts`, `coalesce_defaults_the_absent_row`, `coalesce_over_derived_source`, and `module_path_in_coalesce_wrapper`; the EDGE fixture stayed byte-identical. Three final runs each: conformance 445/0, plunit 1,084/0, RUST-GRADE 445 with 341 byte-clean, cargo 171/0, ghcache 14 ticks with `pr_transition_open_merged=1`, Rust goldens 6, ARCH 7/0.

## 88. A join delta read every moved input combination, and a negated loss had no insert arm

- **Incident** (2026-08-23): the base ghcache `page_response` insert was 248,015 bytes with 256 `UNION ALL` arms and 64 recompute clauses. After the optional-read lowering landed it still had 13 arms, one gain and one loss arm for each of six optional items beside the driver. A separate correctness gap left `unused(main)` absent when `call('b.rs', main)` retracted at tick 4 because a shrinking negated input had no insert producer.
- **RCA**: the fully expanded join delta represented every combination of moved inputs. For ordinary positive items the linear identity needs one arm per item: items before the trigger read post-promote state, the trigger reads its positive frontier, and items after it read the survivor form of old state. The runtime has all three at level execution because durable rows are promoted before the level and the positive frontier remains until the later clear. The survivor query groups the post-promote base and subtracts matching positive-frontier occurrences; departures are already absent from the base. A negated input shrinking is different: its signed delta remains readable while the post-promote base answers the final `NOT EXISTS` check.
- **Fail-pre-fix**: an inline four-positive-item rule pins exactly four arms and the before/frontier/after table order. `callgraph_unused_inverts_with_the_call_set` pins the signed-loss arm text and the corpus oracle pins tick 4 adding `unused(main)`. Nested relation fixtures also pin that survivor projections retain `__id` only when a relation-value join reads it. The ghcache tick-log check caught an intermediate optional default row when ordinary arms treated optional items as old state.
- **Rail**: plain positive items use the ordered identity. Dictionary items stay on current storage and receive no frontier arm. Optional items stay current in ordinary arms and each contributes one transition arm selected by positive frontier gain or signed-delta loss. Negated items contribute one signed-loss insert arm filtered by `NOT EXISTS` on post-promote state. Aggregate maintenance keeps its scoped delta and recount forms. No runtime file changed in this lane.
- **Entry**: `page_response` is 7,548 bytes, 7 arms, and 1 recompute clause. Three release runs read 36,418/37,178/35,554 us over 5 calls before and 760/719/794 us over 5 calls after. Median total `level_insert` was 70,128 us over 1,284 calls before and 30,027 us over 1,160 calls after, or 54.617 to 25.885 us per call. Median wall was 243,889 to 190,561 us; the 7,113 statement target was reached at 6,738, while the historical 152 ms wall was not reached. `ghcache_ticklog_base.txt` remained byte-identical at SHA-256 `6fceaedb07db6e7facf77b1364c57c0615de6382cf671e62c1e70c42fdb2f89e`. Final gates: conformance 445/0, plunit 1,088/0, RUST-GRADE 445 with 341 byte-clean, cargo 172/0, ghcache 14 ticks with `pr_transition_open_merged=1`, and Rust goldens 6.

## 89. A recount stages its retraction, so a clock reading cannot retire the second pass

- **Incident** (2026-08-23): `recount` was 3,244 of ghcache's 6,738 fold statements and 17.9 ms of a 182 ms release wall. Two causes were priced. First, #434's gate kept a head recount-eligible when a NEGATED input LOST a row, because no delta insert arm covered that case; #435 emits the arm, so the eligibility became dead weight. Second, `sequence_level_rounds` was suspected of re-running eligible recounts every recursion round.
- **RCA**: the second cause does not exist on this corpus. Instrumented over the 14-tick fold, the count of recount invocations that were not the head's first in their pass is 0, so no round-2 refire is available to gate. The volume is the two passes, `recompute_levels_before_edges` and `recompute_levels_after_edges`, each running a head once. Gating the second pass on "has an input shrunk since my own last recount" reads correct and is not: a recount STAGES its retraction into the frontier rather than deleting from the base table, so a peer head whose re-derive reads base tables cannot act on that loss until the promote between the two passes. The loss stamp is older than the reader's own first-pass run, and the reader still has work to do.
- **Fail-pre-fix**: `tests/recount_gate.rs` folds `head(V) <- a(V), !b(V)` over add `b(7)`, add `a(7)`, del `b(7)`. It reads `[1, 0, 1]` before the change and `[1, 0, 0]` after, with the tick log and the head rows byte-identical against the `DL_NO_SHRINK_GATE` run. The rejected clock gate is measured, not argued: it took `recount` to 2,130 statements and moved the tick log, adding a row to `__host_demand_http__post`, `pr_batch`, `pr_batch_alias` (2), `pr_batch_member`, `pr_post_body`, `pr_post_field`, `pr_query` and `pr_selection` (2) at tick 6 that base never derives, retracted again at tick 7 and 8.
- **Rail**: `recount_needed`'s negated arm reads `grew_at` only. A negated input loss is the delta insert's own signed-loss arm from #435, and the from-base re-derive would restage rows the insert already produced. The two recount passes stay ungated by the clock; `moved_since_run` keeps the clock reading for the insert phase, where the writes are visible where they are read.
- **Entry**: ghcache fold statements 6,738 and `recount` 3,244 in three count runs, unchanged: no ghcache head is recount-eligible on a negated loss alone, so the corpus prices the change at zero. Three release runs read total `recount` 18,085/17,812/17,908 us before and 18,010/17,686/17,518 us after, with walls 189,020/182,549/181,402 and 184,548/178,267/177,452 us. The 152 ms wall target stays unreached and the remaining `recount` volume is genuine shrink traffic: ghcache retracts answered http demands every tick. `tests/fixtures/ghcache_ticklog_base.txt` byte-identical at SHA-256 `6fceaedb07db6e7facf77b1364c57c0615de6382cf671e62c1e70c42fdb2f89e`. Conformance 445/0, plunit 1,088/0, RUST-GRADE graded=445 byte-clean=341, cargo 172/0, ghcache ticks=14 with `pr_transition_open_merged=1`, Rust goldens 6, ARCH 7/0.
