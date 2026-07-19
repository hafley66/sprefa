# sprefa

Reactive datalog-over-code engine ("dl"), living at the **repo root** (v5 lifted
2026-07-01): SQLite-welded, facts extracted via `scan`+`regex`/`ast`/`sg`/`json`,
recursive rules lower to a SQL fixpoint. Prior iterations: v3/v4 working trees in
`~/projects/sprefa-archive-20260701` (also full git history); the OG coordinate
model (strings/refs/byte-spans) in `~/projects/sprefa-archive-20260428`.

User-facing overview (model, DSL surface, CLI, examples, known gaps): **`README.md`**.

Deep state lives in auto-memory (`project_v5_dl_engine`, etc.) + `chat_log/` session
logs + `plans/`. This file is the standing task ledger only.

**Completed-arc history** (85 landed items, full detail) lives in
`.agent/memories/sprefa-task-ledger.md` — read it on demand, not auto-loaded. This
file keeps only the standing laws + currently-open work.

## Standing laws (user-set, non-negotiable, apply to every agent at every level)

- **Build-vs-buy**: never assert "we should write our own" for any common-shaped
  problem (queues, servers, schedulers, parsers, telemetry) without FIRST running
  library research and presenting a written analysis of the candidates and why each
  does or does not fit. No one-line dismissals of libraries. The analysis comes
  before any bespoke line of code.
- **Self-diagnosis before execution**: the daemon does not run until `dl daemon why`
  can state, from the on-disk trail alone, what it was doing and what it consumed
  (CPU, disk I/O) — including after a SIGKILL or crash. No receipt runs, no smoke
  tests that start the daemon, until that capability is installed. Never make the
  user ask "why is it slow" — the system answers that itself.
- **Nothing seizes the machine**: CPU (QoS/nice), disk I/O (IOPOL_THROTTLE), and
  thread budget are all capped in `apply_daemon_budget`. First-run rebuild included.
  A change that can beachball the machine is a blocking defect, not a follow-up.
- **The failure ledger is standing** (user-set 2026-07-18): every incident that
  bites us gets an entry in docs/failure-modes.md — incident receipt, law, rail
  status — following its "how a new rail gets born" pipeline (incident -> RCA ->
  fail-pre-fix test -> rail -> entry). No incident closes without its entry. Do
  not rely on skill self-updates to carry this knowledge; the doc is the record.
- **eprintln never comes back** (user-set 2026-07-18 PM): no `eprintln!` ever
  returns to `src/**`. Diagnostics go through `tracing` macros only; the rare
  CLI-UX line that must bypass tracing carries an explicit `@eprintln-ok`
  waiver. `.dl/no-new-eprintln.dl` ratchets the count to zero and the baseline
  never rises. Applies to every agent at every level.
- **Infra is bought, never built** (user-set 2026-07-18 PM, supersedes the
  scheduler plan's build-on-jobq verdict): scheduling, job queue, HTTP serving,
  daemon lifecycle/supervision, and logging/telemetry run on established Rust
  libraries (or the OS service manager). Logging = the `tracing` crate spine —
  new signals land as tracing events/subscribers, never a parallel bespoke
  pipeline (invlog/why/verdict are migration targets onto subscribers).
  Bespoke versions of these subsystems are migration targets, and no new
  bespoke line lands in them beyond keep-the-lights-on fixes. The datalog
  engine core (lowering, fixpoint, extraction) remains the one legitimately
  bespoke layer.

## v5 Work — open items only (landed detail: .agent/memories/sprefa-task-ledger.md)

### In flight
(none — the 2026-07-18 wave landed in full: db-seam A+B, eprintln→tracing,
storage-diet 2, cold-chunk, eaten-diag S6, rails pair, small-rails,
queue-apalis, two-worlds, auto-refactor, clock bucket gate + derived
digest-skip + view-DDL skip + wildcard-bucket clock salt, perflog test-globals
lock, rusqlite-coupling Layer 5 call_def fix, _source_stage_owner batch.)

### Blocked on user word
- [ ] **orphan root-dir rm** (classifier-blocked): `cd ~/.local/state/sprefa/roots && rm -rf 0e463c6218bcb16b 13891e3c75f6e3ea 1d5ecc7d13f302e2 21ca57244c1bc6ee 35441d857ff4f1e6 5b5deabf0077f814 9b393f17c4d62844 d782d1a9759048b2 df98753265add20c` (~910MB, verified absent from roots.json).
- [ ] **DROP TABLE _job** in ~/.local/state/sprefa/jobs.sqlite (classifier-blocked; 6 terminal rows, dead bespoke-queue schema).
- [ ] **filesize-rail ruling**: verify.sh exits 2 — 29 src files >500 lines are NOT in scripts/filesize-allow.txt (all already over budget at pushed main a3c09e3f, none crossed this session). Grandfather (allowlist + .dl/file-size.dl rows, shrink-only law) or schedule splits.
- [ ] **instant dom-match.dl rewrite** (user-side repo): drop pull/matches_latest/matches_body + both bucket columns onto `matches_resp(body) <- @async clock(5, _), matches() -> (body).` — caveat: matches_resp then accumulates distinct bodies unordered; keep a bound bucket if strict latest-wins matters.
- [ ] **surviving worktrees** (refreshed 2026-07-19 early AM): only vscode-flow-panel remains non-agent (unmerged parked branch). a5b6cd2c + a93779ab vetted as earlier drafts of landed work and removed (patch backups in the job dir), ext-wave3 salvaged (S7/S8/S9 + perf RCA landed on next) and removed, a305bb unlocked+removed. Active agent trees: index-audit, reqid-midtick (ast-tree-share merged b5bb8ef2, tree removable).

### Next up (dispatchable, not started)
- [ ] **storage-diet 4a**: WITHOUT ROWID junctions; then A=1a dense dictionary ids; step 5 coordinate-composite elimination rides ref-spine.
- [ ] **erase public no-daemon split** (user directive 2026-07-18): one server code path, `--no-daemon` internal-only; erases the two-db-worlds split. Big it-suite touch — schedule alone. Now also owns failure-modes class 23 (a one-shot positional under a daemon-served root silently returns the watched program set's results — `run_file_via_daemon` sends only `{"root"}`).
- [ ] **scheduler execution steps 1-2** (scope rows + readiness; shard = schedulable unit for every family, perf-fed costs, demand join as rows — d13dcf56). Write-volume budget lever lands here.
- [ ] **class 18 residuals**: ~~sg/ast_yaml internal ast-grep tree not shared with AstTreeCache~~ CLOSED 2026-07-19 (branch ast-tree-share: per-file SgRootCache embedded in AstTreeCache); remaining: daemon-side req_id mid-tick cancellation (JobRow.req_id always None).

### Parked (wake on demand; plans exist)
- Auto-architect umbrella (docs/vision-auto-architect.md); decomposition + resource-scheduler children written, unexecuted.
- ~~Auto-refactor residuals~~ CLOSED 2026-07-18 (branch auto-refactor): audit found both "residuals" (brace-head rewrite, physical move + mod surgery) landed 2026-06-12 (#17, f859585e); this arc added the last gap, statement-level regroup when a brace leaf's rewrite exits its head. Audit table in plans/2026-05-31.
- vscode Wave 4; LSP thin client; turnkey query surface (`dl q`, verbs, MCP tools); measures top-K views; deck-graph sym-key migration.
- Change-cost friction inventory (plans/2026-07-10-change-cost-friction-inventory.md); ambient-config hermeticity top.
- Kimi trio prompts (reading-order/lib-taint/session-compile) — worktrees stale off old next; recut or delete.
- Low: 159-changed-paths mystery; tick_root pairing residual (c33ffc04).

### Style notes for this repo
- dl variable names are descriptive, never single-letter: `path`/`line`/`callee_name`, not `p`/`l`/`q`. Applies to every snippet in skills, examples, book, tests, and agent prompts; rename opportunistically when touching old files.
- N+1: never a per-row write. Collect the set, call `Db::insert_rows` once. The tick counter screams if you don't.
- No `provenance`/`substrate`/`load-bearing`/`regime` as prose or identifiers (use source/base/critical/mode).
- Sync tick engine: plural-API + collect-then-flush, NOT async DataLoader (the redux-out-of-hand trap).
- One rel = one rule kind: never head a rel with both a source rule (scan/match/ast/sg/json/cmd/comment) and a derived rule. `rebuild_derived` does a full `DELETE FROM rel` that would wipe the reconciled source rows. The engine now bails; split into two rels and union in a third derived rule. SAME hazard, separately guarded, for a **term-extract** rule (a `json`/`jsonp` body predicate over a bound string) headed together with a derived rule: `eval_extract_rules` fills the extract rows, then `rebuild_derived` (which runs after it so derived rules can read the extract output) drops them. Notably a term-extract rule cannot feed a `@next` carry directly for this reason — route it through its own rel first (the `pr_number -> change_log` split in gh-cache.dl). Engine bails as of the ghcacher-parity arc.
- Recompute guard: a fn that re-derives a relation/embedding FROM SCRATCH (a global op like `embed_graph`, run on a reactive rule) must early-out when its input is unchanged — a `load_rel_digest` digest skip (see `eval_node2vec_rule`, the scc/closure `ConditionCache.digest`) — or carry a `// @recompute unguarded: <reason>` waiver in its body. `examples/recompute-guard.dl --check` (exit 2) is the rail that enforces it; an unguarded recompute re-runs on every git-checkout re-tick under the daemon lock.
