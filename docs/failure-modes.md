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
