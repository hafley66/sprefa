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

- **Doubt yourself before asserting** (user-set 2026-07-23): you are a compression
  algorithm, not an oracle; a large share of your confident claims are wrong. Hedge,
  verify against the code, and do not tell Chris what to do as if it were settled. When
  you lack enough info to answer, or he is asking outside his own expertise and needs
  more depth than you hold, SAY SO and go get it (read the code) rather than guessing.
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
2026-07-19 AM wave (uncommitted on `next`):
- **IO event trail** (src/eventlog.rs, new): `events.jsonl` beside `why.jsonl`.
  The split is the point — `why` samples COST every 2s, `events` records the
  ARGUMENTS the moment something happens. Incident that forced it: a root
  reported `"15 changed path(s)"` on ten separate ticks while `find` proved
  zero files moved in 15 min, and the trail could not name the 15 paths. The
  paths were in memory the whole time (`ServedRoot.last_changed_paths`,
  daemon/root.rs:39) and got `format!`ed to a count at root.rs:163. Causality
  is not reconstructable from periodic samples, on principle.
  Rides the tracing spine per standing law: emission is
  `tracing::info!(target: "dl::event", ...)`, the writer is a
  `tracing_subscriber::Layer` (`EventLayer`), installed in the daemon's
  `init_daemon_tracing` only (one-shot CLI leaves TRAIL unset = no-op).
  Reader: `dl daemon events [--kind K] [--root R] [--limit N]`, file-only like
  `why`, answers with the daemon wedged. Event kinds being instrumented:
  file_changed (full path list + which test fired, hash vs mtime, old/new),
  tick_start/tick_end, gen_write (wrote vs skipped-identical), effect_call/
  effect_result (filled template args, secrets redacted), db_write (per BATCH
  — a per-row emit would violate the N+1 law and is a blocking defect).
- `dl daemon health` builtin (src/cli/health.rs, 392 lines): dbstat buckets,
  per-table rows+data+index MB ranking, identical-rowset dupe probe (EXCEPT
  both directions), static copy-rule scan of the program set, orphan `roots/`
  dirs vs roots.json with an origin probe + rm hint, db/corpus ratio. Read-only
  opens inside one read transaction; answers with the daemon live or down.
  Runs in ~2s over all 3 roots. Wired in daemon_cmd.rs + docs/daemon.md.
- Class-14 rail: `hook::refuse_worktree_cold_check` + the `--check` wiring in
  cli/mod.rs (green-by-skip exit 0, `DL_ALLOW_WORKTREE_COLD=1` hatch),
  tests/it/worktree_cold_check.rs (3 cases). Docs entry in flight.
- Storage diet (a) DONE: `port_of_reach` rename layer deleted from
  .dl/flow-panel.dl (proven identical to `port_of_reach_rec`, 291k rows);
  zero external readers repo-wide (it ends in neither `_node` nor `_edge`, so
  panel layer discovery never saw it), `dl .dl/ --parse-only` exits 0.
  - bom_edge/member_edge: identical rowsets but NOT collapsible. Both names are
    read by editors/vscode-dl/media/flow-panel.html, and layer discovery pairs
    `bom_node` (distinct: carries fan_in/fan_out/weight) with `bom_edge` by
    suffix — dropping `bom_edge` would strand `bom_node` and delete the BOM
    layer from discovery. The .dl/bom.dl:106 passthrough is the price.
  - named_call_site (467k rows, 61MB): 2 independent declarations
    (.dl/rails.dl:61, examples/db-seam-callgraph-audit.dl:83), each with exactly
    ONE same-file consumer (rails.dl:70, audit.dl:93), both feeding
    `loop_entry_fn`. No SQL-name reader anywhere. Inline-vs-keep is the user's
    call; the receipts say it is 61MB serving one join each.
  - Side flag: .dl/rails.dl:62-64 uses `p`/`l`, violating the descriptive-name
    law; the examples twin already uses `call_file`/`call_line`.
- plans/2026-07-19-lazy-rel-tier.md: amplification autopsy (indexes are 57% of
  the 873MB file) + the build-vs-buy table for a lazy rel tier. VIEW-only is
  the sole zero-dependency mechanism that drops table + autoindex + idx_ bytes
  together; ~286MB of today's file is derived-from-derived staging it could
  hold at zero bytes. Syntax/polarity decisions still the user's.

(the 2026-07-18 wave landed in full: db-seam A+B, eprintln→tracing,
storage-diet 2, cold-chunk, eaten-diag S6, rails pair, small-rails,
queue-apalis, two-worlds, auto-refactor, clock bucket gate + derived
digest-skip + view-DDL skip + wildcard-bucket clock salt, perflog test-globals
lock, rusqlite-coupling Layer 5 call_def fix, _source_stage_owner batch.)

### Blocked on user word
- [x] ~~orphan root-dir rm~~ DONE 2026-07-19 (user ran it; +the hook-minted 32f74dffe orphan; ~1.5GB freed, roots/ = exactly the 3 registered). ~~DROP TABLE _job~~ DONE same session.
- [x] ~~one-off VACUUM of the sprefa root db~~ DONE 2026-07-19 AM (1,010 -> 814MB). NOTE: back to 877MB by 09:00 with freelist ~0, so that is new pages, not churn. The 39x db/corpus ratio is the standing defect; see plans/2026-07-19-lazy-rel-tier.md.
- [ ] **drop the orphaned `rel_port_of_reach` table + VACUUM** (one rewrite, not two): daemon stopped, `DROP VIEW IF EXISTS rel_port_of_reach_txt; DROP TABLE IF EXISTS rel_port_of_reach; VACUUM;` against `~/.local/state/sprefa/roots/fbabddda40d22347/db.sqlite`. Table 7.6MB + its PK autoindex 8.6MB = 15.5MB reclaimed; the deleted rule leaves the table behind.
- [ ] **rm the 3 overnight orphan roots** (~1.86GB, minted by agent-worktree pre-commit hooks before the class-14 rail existed): `cd ~/.local/state/sprefa/roots && rm -rf 5658fb5a59d0f252 c22f2b330d2dd1f7 ea3041acfc1af14c`. `dl daemon health` prints this exact line now.
- [ ] **lazy rel tier decisions** (plans/2026-07-19-lazy-rel-tier.md): syntax (`rel lazy foo(...)` vs `@lazy`), opt-in vs health-suggested, and whether demand-materialize-with-eviction is wanted at all or VIEW-only suffices (VIEW-only = zero new deps, zero policy code).
- [ ] **filesize-rail ruling**: verify.sh exits 2 — 29 src files >500 lines are NOT in scripts/filesize-allow.txt (all already over budget at pushed main a3c09e3f, none crossed this session). Grandfather (allowlist + .dl/file-size.dl rows, shrink-only law) or schedule splits.
- [ ] **instant dom-match.dl rewrite** (user-side repo): drop pull/matches_latest/matches_body + both bucket columns onto `matches_resp(body) <- @async clock(5, _), matches() -> (body).` — caveat: matches_resp then accumulates distinct bodies unordered; keep a bound bucket if strict latest-wins matters.
- [ ] **surviving worktrees** (refreshed 2026-07-19 early AM): only vscode-flow-panel remains non-agent (unmerged parked branch). a5b6cd2c + a93779ab vetted as earlier drafts of landed work and removed (patch backups in the job dir), ext-wave3 salvaged (S7/S8/S9 + perf RCA landed on next) and removed, a305bb unlocked+removed. All three agent arcs merged (ast-tree-share b5bb8ef2, reqid-midtick a49a0718, index-audit dc9b67b1); their trees and branches removed.

### Next up (dispatchable, not started)
- [ ] **storage-diet 4a**: WITHOUT ROWID junctions; then A=1a dense dictionary ids; step 5 coordinate-composite elimination rides ref-spine. Direction 5 CLOSED 2026-07-19 (branch index-audit dc9b67b1: planner-honest demand filters in create_auto_indexes — PK-prefix on rowid tables, tiny-rel floor, constant-column; 771 -> 262 idx_, -117.7MB dbstat on the root snapshot; two policies measured-and-rejected with receipts: broad low-selectivity loses to value skew, PK-prefix on WITHOUT ROWID flips fixpoint join sides).
- [ ] **erase public no-daemon split** (user directive 2026-07-18): one server code path, `--no-daemon` internal-only; erases the two-db-worlds split. Big it-suite touch — schedule alone. Now also owns failure-modes class 23 (a one-shot positional under a daemon-served root silently returns the watched program set's results — `run_file_via_daemon` sends only `{"root"}`).
- [ ] **scheduler execution steps 1-2** (scope rows + readiness; shard = schedulable unit for every family, perf-fed costs, demand join as rows — d13dcf56). Write-volume budget lever lands here.
- [ ] **class 18 residuals**: ~~sg/ast_yaml internal ast-grep tree not shared with AstTreeCache~~ CLOSED 2026-07-19 (branch ast-tree-share: per-file SgRootCache embedded in AstTreeCache); ~~daemon-side req_id mid-tick cancellation~~ CLOSED 2026-07-19 (branch reqid-midtick 9ddf1280: run_job re-enters the causing request's reqid scope, cancel probe at component boundaries, abort-consistency test) — class 18 fully closed.

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
- **Every new class declares its interface in the package's header `types.ts`** (user-set 2026-07-25): a
  class that ships without a contract in the header is an incomplete change, not a follow-up. The
  header declares each name exactly once, no `export type Foo = SomeFoo` aliases. v6 headers are
  `v6/sprefa-store/js/src/engine/types.ts`, `v6/sprefa-store/js/src/lower/types.ts`,
  `v6/dl/src/0_types.ts`. Currently uncovered: `tasks.ts` `Namespaced`/`Independent`/`Evidence`,
  `engine.ts` `AscendingIdQueue`. `Error` subclasses are exempt.
- **Important functions are interface-bound, never bare `export function`** (user-set 2026-07-25):
  TypeScript cannot conformance-check a standalone function against anything. A free
  `export function foo()` can drift from its documented signature and the compiler stays silent.
  So any function that matters gets bound to a header interface one of two ways:
  - namespace object, the default: `interface ISqlRunner { ... }` in the header,
    `export const SqlRunner: ISqlRunner = { ... }` in the module.
  - a class `implements` the interface, when there is real per-instance state or arg-object envy.
  The annotation is what buys the check. `satisfies` also checks and additionally keeps the
  literal's narrow inferred type; use it only when a caller needs that narrower type.
  Small leaf helpers that would be a `.map` callback or a plain method call in another language stay
  bare functions. This is the same exemption as the rxjs law.
- **Interfaces carry the `I` prefix** (user-set 2026-07-25): `IStore`, `IGraphNs`, `IDlRuntime`.
  The prefix is what lets the interface and its implementing object hold the same root word
  without an alias. `lower/types.ts` (`RelTable`, `Graph`, `Stratum`, `IDatalog`) is inconsistent
  and is the rename target, not the other way round.
- **Exactly ONE manual `.subscribe()` in the whole app, ever** (user-set 2026-07-25): React does
  not ask you to call `ReactDOM.render` three times. One terminal subscription at the bottom of
  `main.ts`; everything above it is cold and composed with `merge`/`concatMap`. A second
  `.subscribe()` anywhere is a design failure, not a style preference, because it means that
  branch of the graph is started imperatively and its lifetime is tracked by hand.
  Corollary: no `Subscription` field held on a class, and no `Subject` used as a request/response
  bridge (a method that pushes into one Subject and awaits a matching id on another is RPC wearing
  a stream costume, and it forces every caller back into `await`).
  Ratchet, 2026-07-25 baseline = 3, target 1, never rises:
  `1_hosts.ts:402` (host effects), `6_http.ts:281` (per-client SSE), `3_runtime.ts:793`
  (`keepAlive`). Blockers to collapsing them: 7 `new Promise` wrappers around Node callbacks
  (`server.listen`/`server.close`/`readBody`, 3 `spawn` sites, `extractFile`'s exit code) and the
  `commits$`/`reportsSubject` Subject pair at `3_runtime.ts:768`/`:95`.
- **A type name must say what the thing is on first reading** (user-set 2026-07-25): no
  library-flavoured or abbreviation names that carry no content. `Rx` is the rejected example.
  If one interface needs a vague name it is usually two interfaces glued together; split it and
  both names get obvious.
- **No async in v6, rxjs instead** (user-set 2026-07-25): `Promise`/`async`/`await` are banned above
  the single driver seam. That seam is `SqliteDb.execute`, wrapped exactly once per package in a
  `defer(() => from(...))` helper (`makeExec` in lowerSql.ts, `execute$` in 3_runtime.ts). Sync
  control flow also goes through rx operators: a nested if/for/switch becomes `from -> concatMap ->
  toArray`, a fixpoint becomes `expand`, a fan-out becomes `groupBy -> mergeMap`. Exempt: tiny pure
  functions that would be a `.map` callback or a plain method call in any other language.
  TRAP: `await someObservable` returns the observable without subscribing and TypeScript accepts it
  silently. Use `firstValueFrom`, or better, do not leave an `await` to convert.
- One rel = one rule kind: never head a rel with both a source rule (scan/match/ast/sg/json/cmd/comment) and a derived rule. `rebuild_derived` does a full `DELETE FROM rel` that would wipe the reconciled source rows. The engine now bails; split into two rels and union in a third derived rule. SAME hazard, separately guarded, for a **term-extract** rule (a `json`/`jsonp` body predicate over a bound string) headed together with a derived rule: `eval_extract_rules` fills the extract rows, then `rebuild_derived` (which runs after it so derived rules can read the extract output) drops them. Notably a term-extract rule cannot feed a `@next` carry directly for this reason — route it through its own rel first (the `pr_number -> change_log` split in gh-cache.dl). Engine bails as of the ghcacher-parity arc.
- Recompute guard: a fn that re-derives a relation/embedding FROM SCRATCH (a global op like `embed_graph`, run on a reactive rule) must early-out when its input is unchanged — a `load_rel_digest` digest skip (see `eval_node2vec_rule`, the scc/closure `ConditionCache.digest`) — or carry a `// @recompute unguarded: <reason>` waiver in its body. `examples/recompute-guard.dl --check` (exit 2) is the rail that enforces it; an unguarded recompute re-runs on every git-checkout re-tick under the daemon lock.
