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
**v5-port + perf-tracing arc** (2026-07-27 late, plans/2026-07-27-v5-port-perf-header.md):
scopes fixtures LANDED (conformance 97 -> 109, merged d481159e); opus diff review
LANDED (plans/2026-07-27-diff-review-findings.md — finding 1 double-fire = USER
ACCEPTED no-fix, 2+3 http fixes dispatched, rest banked); SLOT-LIB filled
(tracingChannel + pino, user approved the pino dep,
plans/2026-07-27-perf-tracing-buy-verdict.md). http fixes 2+3 LANDED (mergeMap
body read + SSE response.end, 2 regression tests, dl 76/76). ghcacher phase 1
LANDED (v6/dl/fixtures/ghcacher.dl ACCEPTED by the server + ghcacher-findings.md,
9 findings F1-F9): HEADLINE = F7 engine crash, first real host response commit
dies `SQLITE_ERROR: no such column: NaN` (1_hosts.ts:491 commit path, statement
text not surfaced by LibsqlError; root cause OPEN, fix agent queued BEHIND the
P0 tracing merge since per-statement tracing surfaces the failing SQL).
PROVEN GAPS awaiting user word (the zero-new-constructs exception clause):
F2 no clock/cadence = the SLOT-SWR-defining gap (spelling A in-language chosen,
B external-cron documented); F3 no json term-extract, array-explode
inexpressible; F8 rel(1) is whole-table sweep + silently inert on rule-headed
rels, Key(text) unimplemented (feeds the Q8/Key ruling); F9 no effect_log rel
(self-diagnosis law gap). F4 confirmed the not_stratified guard fires correctly
on the v5 etag idiom. P0 tracing spine LANDED (0_trace.ts: tracingChannel +
pino, DL_PERF_LOG opt-in, one JSONL line/tick, overhead -0.02% within noise,
dl 79/79; ratchet filter tightened to Channel\.subscribe call shape; seam gap
recorded in 0_trace.ts header: EDB-plane writes bypass SqlRunner via hand-
rolled execute$). FIRST PARITY NUMBER, ugly and now visible: ingest_corpus
over 251 rxjs .ts files = ~103s (~2.4 files/s) vs v5's 7,244 files/s; the
harness's per-file rt.rows() full-table read is superlinear and suspect, but
extract_ms is only ~21ms/file so engine-side cost dominates — the perf JSONL
now exists to decompose this. TSV2 PHASE A LANDED (v6/tsv2: IGenProgram seam,
generic tickLoop via rxjs expand, 2 hand-carved gen files BYTE-IDENTICAL to
the prolog oracle incl a perturbed schedule, import gate green, 6/6 tests,
conformance 109; emitter-spec margins recorded in the agent report: keyed()
inert on raw arrivals vs live on edge heads, TEXT-collapse + LIKE compound
matching, one multiset-diff covers log+set, carryPending simplification
FINDING 3 in switch gen file). Agents running: tsv2 phase B (prolog
compiler), F7 NaN-commit crash hunt.

**v5 BACKGROUND OPS (overnight 2026-07-27, user asleep)**: daemon swapped to
current binary (~/.cargo/bin/dl restored from target/release, was missing —
plist pointed at nothing while dl.old-1301 held the socket since Sunday);
launchd plist gained EnvironmentVariables PATH (homebrew+cargo) because every
sh effect exit-127'd under launchd's bare PATH — the doc-gen trigger then
fired and its output is committed (f76b7c10). Roots watched: sprefa (.dl/*.dl
rails + flow-interproc loaded), smashy, instant. CROSS-REPO IS LIVE:
~/orgs/.dl/{go-deps,xrepo-rev}.dl run against SPREFA_CONFIG=~/orgs/
all.config.toml (800 repos) settles in 3 ticks with real fan-in/rev-fan rows
(79 hubs). MORNING DECISION: the daemon runs the safe selfv5-only global
config; watching the orgs root persistently needs either a daemon-level
SPREFA_CONFIG (puts 800 repos under EVERY wildcard rail — the safe-default
comment warns against exactly this), a per-root config feature, or a cron
one-shot. Health also showed: sprefa root db regrew to 4.3GB (lazy-rel-tier
decision pending), 4 orphan roots incl one minted TODAY (class-14 rail may
have a gap — worth a look).
gen-index.sh now excludes node_modules (INDEX.md was flip-flopping 1714 lines).
ARCH covers/2 rows for scopes.pl landed (departure_form fixture-covered,
uncovered 10 -> 9, map re-emitted). failure-modes class 35 filed (dangling dev
servers; stdin-watch rail proposed, awaiting word).

(v5 side: none. The 2026-07-19 AM wave is CONFIRMED LANDED on main, verified 2026-07-27:
src/eventlog.rs event trail + `dl daemon events`; `dl daemon health`
(src/cli/health.rs); class-14 rail (`hook::refuse_worktree_cold_check` +
tests/it/worktree_cold_check.rs); storage diet (a). `next` is 0 ahead / 244
behind main — nothing lives there. The 2026-07-18 wave landed in full earlier.
Detail for both: .agent/memories/sprefa-task-ledger.md. Receipts still live
from that wave: named_call_site is 61MB serving one join each,
inline-vs-keep = user call; .dl/rails.dl:62-64 still uses `p`/`l` and owes the
descriptive-name rename.))

### Blocked on user word
- [ ] **drop the orphaned `rel_port_of_reach` table + VACUUM** (one rewrite, not two): daemon stopped, `DROP VIEW IF EXISTS rel_port_of_reach_txt; DROP TABLE IF EXISTS rel_port_of_reach; VACUUM;` against `~/.local/state/sprefa/roots/fbabddda40d22347/db.sqlite`. Table 7.6MB + its PK autoindex 8.6MB = 15.5MB reclaimed; the deleted rule leaves the table behind.
- [ ] **rm the 3 overnight orphan roots** (~1.86GB, minted by agent-worktree pre-commit hooks before the class-14 rail existed): `cd ~/.local/state/sprefa/roots && rm -rf 5658fb5a59d0f252 c22f2b330d2dd1f7 ea3041acfc1af14c`. `dl daemon health` prints this exact line now.
- [ ] **lazy rel tier decisions** (plans/2026-07-19-lazy-rel-tier.md): syntax (`rel lazy foo(...)` vs `@lazy`), opt-in vs health-suggested, and whether demand-materialize-with-eviction is wanted at all or VIEW-only suffices (VIEW-only = zero new deps, zero policy code). Context: post-VACUUM the root db regrew 814 -> 877MB in hours with freelist ~0 (new pages, not churn); the 39x db/corpus ratio is the standing defect this decides.
- [ ] **filesize-rail ruling**: verify.sh exits 2 — 29 src files >500 lines are NOT in scripts/filesize-allow.txt (all already over budget at pushed main a3c09e3f, none crossed this session). Grandfather (allowlist + .dl/file-size.dl rows, shrink-only law) or schedule splits.
- [ ] **instant dom-match.dl rewrite** (user-side repo): drop pull/matches_latest/matches_body + both bucket columns onto `matches_resp(body) <- @async clock(5, _), matches() -> (body).` — caveat: matches_resp then accumulates distinct bodies unordered; keep a bound bucket if strict latest-wins matters.
- [ ] **worktree removal** (refreshed 2026-07-27, supersedes the 2026-07-19 row which undercounted by 40): reconcile pass found 42 worktrees. 34 are fully merged into main; all their uncommitted work is banked as 13 patches in archive/worktree-salvage-2026-07-27/ (README has per-patch inventory). `git worktree remove` was permission-blocked for the agent — the exact removal + merged-branch-deletion commands are in that README, run them. 8 unmerged trees stay alive (lsp-diags ahead 12, types, codex-intern, codex-qscip, g4-unify, refactor/file-splits ahead 7, vscode-flow-panel, extract-golden-plan ahead 76 + 4 uncommitted chat_logs — that last one needs a merge-or-kill decision).

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

### v6 STANDING PLAN (user-set 2026-07-25, execute IN ORDER, do not improvise past it)
1. ~~Restore green + commit~~ DONE (verified 2026-07-27: store 89/89, dl 74/74, both
   typechecks clean, `src/lib/rxjs.ts` orphan gone). Every green state gets a commit,
   standing. NOTE for item 2: the restored `sequence` helper still sits in
   engine.ts:115 with 2 call sites (:743, :744) — it is the first thing item 2 deletes.
2. ~~Undo rxjs over sync code~~ DONE 2026-07-27 PM (agent arc, merged): `sequence`,
   `run_then` (both copies), `execBatch`, `run$`, `inOrder` deleted; sequential run is
   `concat(...).pipe(toArray())` inline; rowsAffected flows through SqlRunner/batch/
   cascade/reconcile/TemporalStore/runAll; side-effect maps became `tap`; sync unwraps
   (`from(rows)->map->toArray`, rxjs `groupBy` over in-memory keys) are plain array
   code. Legitimate voids kept with reasons: `executeMultiple` (driver resolves
   nothing), rollback-path `catchError` swallows. Receipts: store 89/89, dl 74/74,
   both typechecks, ratchet 3, goal-endurance 3/3, statement counts unchanged.
3. ~~Single subscribe point~~ DONE 2026-07-27 late PM (agent arc, merged): ratchet
   reads 1, baseline lowered to 1. `serveDl(cfg): Observable<DlAppEvent>` in 6_http.ts
   IS the app, cold; main.ts's one `.subscribe` starts it. Program swap =
   `switchMap` on accepted loads only (bad program -> 400, running program survives);
   SSE clients are inners with `takeUntil(socket close)`; HostRunner lost
   start/dispose/Subscription for one cold `effects$` (boot replay under `defer`,
   semantics unchanged); `DlRuntime.commit()` now throws instead of hanging when the
   loop isn't running. Receipts: store 89/89, dl 74/74, ratchet 1, endurance 3/3,
   golden curl-session PASS, no Subscription fields. Honest residue: the
   `commits$`/`reportsSubject` Subject pair remains (not a collapse blocker, still
   the open item against the no-Subject-bridge corollary); `server.close`/`readBody`
   Promise wrappers remain (the Promise-above-the-seam arc); `tasks.d.ts:128` names
   `StartServer` in a past-tense M10 record (renamed to `ServeDl` in 0_types.ts).
   One golden flake in 1/10 runs under heavy parallel load, not reproducible,
   recorded in the agent report.
4. **Rxjs rule of engagement**: before writing ANY new rxjs, stop and ask the user
   first: is this making sense, is there a shorter/more direct way, fewer variables,
   fewer methods. No new operator chains land without that check.

### v6 primed queue (user, 2026-07-27 PM, unordered — "i want a lot of things")
- diags done + LSP hosted from TS (best-buy research first; note: v5 `dl --lsp
  --diag-db` boots NO engine and polls `diag_v5`, which 5_diag.ts already creates —
  the zero-code interim is pointing v5 at the v6 db).
- endurance goal: v6/dl/scripts/goal-endurance.sh IS the end-goal definition
  (kill -9 mid-delay, reboot, value lands exactly once). Phase 0 green; phase 1 =
  the pending-witness wedge + no boot replay of unanswered demand.
- snippets proving each v5 builtin rel's v6 behavior, ZERO new language features.
- bootstrap story: how the language owns its own utilities (swipl-to-C analogy);
  rust return eventually (souffle-of-rust + rx logic); formalizing the v8 event loop.
- self-diags on our own .pl files (pick up by pattern/extension/marker word).
- generic `--changed` concept (biome-style recent-change-lines gating) directable
  from dl; the old pre-commit hooks did this.
- `input/distinctUntil(shallowEquals|deepEquals)` on rels — mostly already physics
  here (R7 boundary diffing = distinctUntilChanged at every rel edge; set/keyed
  identical writes are zero-delta); the real residue is WHICH columns count as
  identity (= the Key/Q8 ruling) and digest-vs-value for structured blob columns
  (the content_hash pattern).

### v6 rulings RESOLVED 2026-07-27 late PM (three grunts; rulings.pl is the record)
- **salt_minting = content_addressed** ("one hunt"): shared in-flight effects, IVM
  support refcounting for free, freshness = explicit extra salt column. Consequence:
  **stale_fill_policy = not_applicable** — under content salts a fill is a cache
  update, never stale; no orphan rel, no fill tick-item, no per-instance identity.
- **effect_abort = best_effort_cancel_on_support_zero** ("rope arrow" + the
  invariant: "no arrow stop exist, is lie" — cancellation is cost optimization,
  never semantics; warn-paint at the abort site + debug line per attempt). Lowering
  owed: AbortSignal through HostDef.run + cancel map + pending-row delete (ARCH task
  effect_abort).
- **subscription_kernel = minimal_with_coverage_check_and_ghost_view**: zero stored
  rels, zero new phases; obligations = scope-coverage static check (ARCH task
  scope_cover_check, answers the zombie-scope break) + ghost forest diagnostic view
  (ARCH task ghost_forest_view). Shared DRed-depth hazard (recursive rels in scope
  cones = f(depth) statements vs n1_statement_budget) filed separately, owner
  unassigned.

### v6 REORIENTATION (user-set 2026-07-27 night): TSV2, prolog compiles TO TypeScript
NEW PRIMARY EFFORT (plans/2026-07-27-tsv2-compile-target-header.md): prolog owns
the whole compiler front (parse/AST/typecheck/lowering); it EMITS literal
TypeScript program files with the real SQLite statements and real rxjs chains
visible in the generated file. TypeScript keeps only (a) a hand-written static
runtime reusing the NAMED v6 symbols (SqlRunner, spine.ts fact plane, IVM
machinery, HostRunner lift, P0 tracing channels — class-34 law, import-gate
checked) and (b) the generated gen/*.ts programs. No AST/parser/lowering in TS
on this path. v6/dl stays untouched and running as the sibling; langium/
ast_bridge/lower are dead weight for tsv2 only. Grading = the item-9 tick-log
JSONL diffed byte-for-byte against the prolog oracle (the 109-fixture corpus is
the compiler test suite). Phases: A hand-carved target exemplar (2 scopes
fixtures) -> B prolog emitter matches it byte-identically -> C fixture sweep ->
D .dl DCG surface + hosts (ghcacher rides D). The stopping-point program list
below still defines DONE; programs land against the tsv2 target as it matures.

### v6 STOPPING POINT (user-set 2026-07-27 late PM): express the real programs
The milestone that ends this arc: the real programs written in the v6 surface and
graded, zero new constructs unless a program PROVES a gap (extraction-lab discipline):
1. **ghcacher** (poll -> fetch -> cache -> change_log carry; mode-lattice prog facts
   are the draft; content-addressed salts now ruled, so SWR spelling is open).
2. **diags for LSP** (diag rels -> diag_v5 view; the lsp-v5-bridge receipt is live).
3. **git pre-commit --changed** (biome-style recent-change-lines gating, generic and
   directable from dl).
4. **sprefa-extract run**: scan/scanwork, repo/rev extraction, lazy finding, lazy
   heads.
5. **auto-synced repo list**: HEAD the repo list itself (repo rows the system keeps
   synced; v5 repo-rev-scanning receipts research in flight).
6. **v5 bench parity target**: the v5 multirepo crawl benchmarks (grafana-class
   corpora) are the perf yardstick the v6 expressions must eventually meet.
7. **rtkq examples through sprefa-extract**: the redux-toolkit-query example corpus
   as an extraction+analysis target program.
8. **file watcher scaling, cross-platform preferably** (i:file-watching skill is the
   reference; watcher is a BIND per spine_residency, never kernel).
9. **standardized tick-log format**: the per-tick delta log serialized in ONE stable
   format (the marble record) so later runners (rust, python, ts) are graded by
   diffing logs against the oracle's log, never by embedding in the language. This
   is the json-rx cross-target agreement record made concrete.
Directive riding the milestone (rulings.pl spine_residency): the git/fs spine is
HOSTED IN THE LANGUAGE (stdlib rels + binds + salts over generic effect machinery),
never kernel; where the native concepts fail to host it intuitively, that is a
language finding, not a reason to special-case the spine.

### v6 still awaiting user word (small, none blocking the absorption arc)
- **Q8 residual**: confirm left-of-arrow = demand key on effect rels, `Key()` never
  appears there (the shipped TS reading; extraction lab's preference).
- **filesize rail + lazy-rel-tier + dom-match rewrite** (v5 side, unchanged).
- Tabling question CLOSED (plans/2026-07-27-tabling-verdict.md): SHIFTS SEMANTICS,
  hand-rolled fixpoint stays (the not_stratified guard IS semantics).
- **extraction ambiguities** A12 (from-world = nullary `->`?), A1 (glob residency),
  A4 (fence escape), A14 (comment_span bind). plans/2026-07-27-extraction-spellings.md.
- **Key(Type) vs `->`**: labs split three ways; present both files' arguments, no fiat.
  plans/2026-07-27-lab-consolidation.md bottom.
- Queued smaller: operators.pl models forkJoin as a level rule (correct only while
  inputs are unscoped — refixture when the sub forest absorbs); `scope_done`
  read-by-name violates the magic-rel ban (needs a decl); repeat's arrival-tick salt
  collides on two same-tick resubscribes; `until(F)` formula presentation in CLI output.

### Lab protocol (user-set 2026-07-27, applies to every agent at every level)
- **Planner seeds the header first.** Every lab starts from a planner-written contract
  file: the predicates/checks the lab must implement, the questions it must grade, and
  named slots for ambiguities it may discover. No lab starts from a blank file.
- **Implementation agents run in worktrees** (Agent `isolation: "worktree"`), never in
  the main tree. Main-tree file ownership belongs to the coordinator only.
- **Labs die on landing.** In the same arc that a lab lands: durable output distills to
  its permanent home (conformance/fixtures, rulings.pl, plans/, ARCH.pl), the lab files
  are deleted, and the plan doc records the commit hash holding the last copy
  (`git show <hash>:<path>` recovers it). Git history is the archive.
- `v6/prolog/labs/` was deleted 2026-07-27 (last full copy at 2fff3f61) and stays
  deleted; a lab file surviving its landing commit is a defect, not a follow-up.

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
  Ratchet: TARGET REACHED 2026-07-27 (baseline = 1, never rises): the one site is
  `dl/src/main.ts` subscribing `serveDl(...)`. Remaining law debt, not ratchet debt:
  the `commits$`/`reportsSubject` Subject pair (3_runtime.ts) vs the no-Subject-bridge
  corollary, and the `server.close`/`readBody` Promise wrappers above the seam.
- **A type name must say what the thing is on first reading** (user-set 2026-07-25): no
  library-flavoured or abbreviation names that carry no content. `Rx` is the rejected example.
  If one interface needs a vague name it is usually two interfaces glued together; split it and
  both names get obvious.
- **Async becomes rxjs; sync stays sync** (user-set 2026-07-25, CORRECTING the earlier
  "make the whole code rxjs" instruction, which the user withdrew: "i should not have said
  make it all rxjs, just make the async into rxjs"):
  - `Promise`/`async`/`await` are banned above the single driver seam. That seam is
    `SqliteDb.execute`, wrapped exactly once in `SqlRunner` (`engine/sqlRunner.ts`).
  - Loops, branching, and list building over in-memory data stay **plain array code** and
    **return arrays**. `map`/`filter`/`flatMap`/`reduce`, not `from -> concatMap -> toArray`.
    A function that computes a `string[]` returns `string[]`.
  - The dividing line that works in practice (see `lower/lowerSql.ts`): SQL *building* is
    sync and returns statements; only *running* statements is an Observable. `runAll` is the
    single place a `string[]` becomes execution.
  - Symptom that the line was crossed: an Observable pipeline that ends by throwing its
    values away (`count()`, `toArray()` then ignore, `ignoreElements()`). That is sync work
    wearing an Observable. It also hides real values, which cost 8 redundant
    `SELECT count(*)` scans per conformance run before `rowsAffected` was let through.
  - `Observable<never>` is not used here. An effect emits one `void` when done and callers
    chain with `concatMap`; `concat` would union the effect's type into the value type.
  TRAP: `await someObservable` returns the observable without subscribing and TypeScript accepts it
  silently. Use `firstValueFrom`, or better, do not leave an `await` to convert.
- One rel = one rule kind: never head a rel with both a source rule (scan/match/ast/sg/json/cmd/comment) and a derived rule. `rebuild_derived` does a full `DELETE FROM rel` that would wipe the reconciled source rows. The engine now bails; split into two rels and union in a third derived rule. SAME hazard, separately guarded, for a **term-extract** rule (a `json`/`jsonp` body predicate over a bound string) headed together with a derived rule: `eval_extract_rules` fills the extract rows, then `rebuild_derived` (which runs after it so derived rules can read the extract output) drops them. Notably a term-extract rule cannot feed a `@next` carry directly for this reason — route it through its own rel first (the `pr_number -> change_log` split in gh-cache.dl). Engine bails as of the ghcacher-parity arc.
- Recompute guard: a fn that re-derives a relation/embedding FROM SCRATCH (a global op like `embed_graph`, run on a reactive rule) must early-out when its input is unchanged — a `load_rel_digest` digest skip (see `eval_node2vec_rule`, the scc/closure `ConditionCache.digest`) — or carry a `// @recompute unguarded: <reason>` waiver in its body. `examples/recompute-guard.dl --check` (exit 2) is the rail that enforces it; an unguarded recompute re-runs on every git-checkout re-tick under the daemon lock.
