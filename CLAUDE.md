# sprefa

Reactive datalog-over-code engine ("dl"): v5 rust engine at the repo root; v6 =
prolog compiles .dl6 to TypeScript+SQLite (v6/prolog compiler+oracle, v6/tsv2
runtime/serve/cli, v6/sprefa-store, v6/sprefa-extract). Overview: README.md.
Archives: ~/projects/sprefa-archive-20260701 (v3/v4), -20260428 (OG).

## Where state lives (this file = laws + open items ONLY)
- Landed-arc detail: `v6/prolog/ARCH.pl` (task/5 + fork/5, the priced record;
  gate = `swipl -g go -t halt ARCH.pl` from v6/prolog).
- **What the language accepts**: `v6/prolog/compile/parse_dl.pl` is the real
  surface (text door) and `v6/prolog/compile/out/manifest.json` is the verdict
  per fixture. `v6/dl/grammar/dl.langium` is a NARROW MVP SLICE of the same
  language, not the language; a construct absent there is usually present in the
  real one. Regenerate the manifest with `cd v6/tsv2 && bash scripts/sweep.sh`.
- Narrative history: `.agent/memories/sprefa-task-ledger.md` (read on demand,
  never auto-loaded; the pre-2026-08-02 ledger is archived verbatim at its tail),
  `chat_log/` session logs, `plans/` docs, git history.
- Rulings: `v6/prolog/rulings.pl`. Roadmap: `plans/2026-07-29-v6-alpha-golden-plan.md`
  (P0-P4 complete; phase 5 = type pass float/REAL+avg, clock checker, ingest commit_ms).
- Position 2026-08-02: lane wave merged on `codex/rel-ref-file-span-lab`, UNPUSHED,
  no tag. Battery green: conformance 281/0, plunit 276, TEXT_DOOR 196/196/0, tsv2
  128/1skip, store 74/74, dl 96/96. `just green-all` is the gate.

## Standing laws (user-set, non-negotiable, every agent at every level)
- **Doubt yourself before asserting** (2026-07-23): you are a compression
  algorithm, not an oracle. Hedge, verify against the code, never tell Chris what
  to do as if settled. If you lack the info, SAY SO and go read the code.
- **Comments are not the language** (2026-08-06): "does X compile" is answered by
  `v6/prolog/compile/out/manifest.json` (513 fixtures, `bucket` +
  `reason` each), never by a header. Grep the manifest FIRST. Measured that day:
  `v6/dl/fixtures/ghcacher.dl6`'s header was wrong four times about its own
  grammar (`->`, `bind`, bare host calls, and "array-explode INEXPRESSIBLE"),
  and `v6/dl/grammar/dl.langium:5` calls JSON5 terms "deliberately absent" while
  `spread`, `$name` key holes, `**` descent and typed captures all compile green.
  An agent that reads a comment and reports a language limit has not checked.
- **Build-vs-buy**: never assert "write our own" for a common-shaped problem
  without library research + written candidate analysis first. No one-line
  dismissals.
- **Infra is bought, never built** (2026-07-18): scheduling, job queue, HTTP,
  daemon lifecycle, logging (= the `tracing` spine in v5) run on established
  libraries or the OS. The datalog engine core is the one legitimately bespoke
  layer.
- **Self-diagnosis before execution**: the system answers "why is it slow / what
  was it doing" from its own on-disk trail, including after SIGKILL. Never make
  the user ask.
- **Nothing seizes the machine**: CPU/IO/thread budgets capped
  (`apply_daemon_budget`). A change that can beachball the machine is a blocking
  defect.
- **The 10-second law** (2026-08-01): any operation over 10s (test, receipt,
  compile, rail, script) is violently wrong — a defect to investigate now, never
  a budget to normalize. Named exception: SCIP indexing.
- **Failure ledger** (2026-07-18): every incident that bites gets a
  docs/failure-modes.md entry (incident -> RCA -> fail-pre-fix test -> rail ->
  entry). No incident closes without one.
- **eprintln never comes back** (2026-07-18): no `eprintln!` in src/**;
  `tracing` only; rare CLI-UX lines carry `@eprintln-ok`. `.dl/no-new-eprintln.dl`
  ratchets to zero.
- **Labs run on flash4** (user-set 2026-08-04, supersedes the 2026-07-29
  OPUS-only law): openrouter/deepseek/deepseek-v4-flash-0731 per the live
  opencode-orchestration skill, spawned via `bus dispatch`. Opus stays for
  diagnosis/mid-task trade-offs; codex lanes per
  `claude-research/skills_archive/commands/codex-delegate.md`
  (evidence: plans/2026-08-02-flash-vs-opus-lane-report.md).

## Worktree dispatch law (2026-07-28)
- Every worktree agent's FIRST action: `git merge --ff-only <sha>` (coordinator
  states the sha). Failure or missing trees = STOP AND REPORT. Working around a
  blocked command via another mechanism (archive/tar, --no-verify, copying) is a
  defect; a permission denial ends the approach.
- Coordinator verifies the agent's base sha and refuses work on any other base.
- **Lanes never spawn subagents** (2026-07-31). Fan-out is the coordinator's
  call only.

## Lab protocol (2026-07-27)
- Planner seeds a header/contract file first; no lab starts blank.
- **Plan lanes ship two docs** (user-set 2026-08-03): PLAN.md (receipts,
  citations, for the auditor) + PLAN.visual.human.unga.md (plain words, ascii
  diagrams, zero citations, for Chris). A plan without the unga doc is
  undelivered.
- Implementation agents run in worktrees; main-tree ownership = coordinator only.
- **Labs die on landing**: durable output distills to fixtures/rulings/plans/ARCH,
  lab files deleted, plan doc records the last-copy commit hash.

## Style laws
- **Comment budget** (2026-07-31): comments state only constraints the code
  cannot show. No change-log narrative, dates, arc references, or restating the
  next line. Sabotage/fail-first receipts in TEST headers and scanner-backed
  @-waivers stay. History belongs in git/plans, never source.
- **Language vocabulary** (2026-07-28): construct names and design discussion use
  ONLY rxjs, prolog, or SQL words. "support" is banned -> refCount (rename
  executed 2026-08-02).
- **Every .dl snippet shown to the user carries its intended pure-rxjs lowering**;
  a construct whose rx lowering cannot be written is a design defect.
- **Formerly-quadratic paths get COUNT tests**: statement counts / EXPLAIN
  SEARCH-not-SCAN, never end-state equality alone. Additive only.
- dl variable names are descriptive, never single-letter, in every snippet.
- N+1: never a per-row write; collect the set, one `Db::insert_rows`.
- Banned words, prose AND identifiers: provenance, substrate, load-bearing,
  regime (use source/base/critical/mode).
- **Every new class declares its interface in the package's header types.ts**
  (v6 headers: sprefa-store/js/src/{engine,lower}/types.ts, dl/src/0_types.ts);
  important functions are interface-bound (namespace object or class
  implements), never bare `export function`; interfaces carry the `I` prefix;
  type names say what the thing is on first reading.
- **Exactly ONE manual `.subscribe()` per app** (ratchet baseline 1 = main.ts).
  No Subscription fields, no Subject request/response bridges.
- **Async becomes rxjs; sync stays sync**: Promise/async banned above the
  SqlRunner seam; in-memory list work is plain array code returning arrays; SQL
  building sync, running Observable. TRAP: `await someObservable` silently never
  subscribes.
- One rel = one rule kind (source vs derived vs term-extract heads never mix;
  engine bails; split and union).
- Recompute guard: any from-scratch re-derive on a reactive rule needs a digest
  early-out or `// @recompute unguarded: <reason>`; rail =
  examples/recompute-guard.dl --check.
- Colocated consistency: inside a file, follow the file's existing style.

## Open items
**Awaiting user word (v6):** prolog folder names/numbering
(plans/2026-08-01-flash4-partition-research.md); flash-prolog worktree fate
(redo in v6/sprefa-extract, keep, or drop); bop-run idle-exit vs rail receipts
(serve or --forever); refusal re-eval kickoff (plans/2026-08-01-refusal-inventory.md,
245 decisions / 65% weak-trail); push + tag (bop gate satisfied); Q8 residual
(left-of-arrow = demand key); extraction ambiguities A4 fence-escape + A14
comment_span; Key(Type) vs `->` (plans/2026-07-27-lab-consolidation.md); smaller:
operators.pl forkJoin fixture, scope_done magic-rel decl, repeat same-tick salt,
until(F) formula presentation; **string split/substr primitive** (only concat +
regexp exist, so path-prefix work has no in-language spelling — blocks deriving
ancestor directories, see conformance/fixtures/9_pr_size.pl where in_dir/2 is
supplied as facts); **scan-into-json** (pre/1 + `:=` a document is refused by
json_value_expression; decide whether to lift it or keep json write-only through
json_group_array).
**Awaiting user word (v5):** rm 3 orphan roots (~1.86GB, `dl daemon health`
prints the line); drop rel_port_of_reach + VACUUM (15.5MB); lazy-rel-tier
(plans/2026-07-19-lazy-rel-tier.md); filesize rail ruling (29 files >500 lines);
instant dom-match.dl rewrite.
**Dispatchable (v5):** storage-diet 4a (WITHOUT ROWID junctions, dense dict ids);
erase public no-daemon split (owns failure-modes class 23); scheduler execution
steps 1-2 (d13dcf56).
**Open defect/feature rows are ARCH.pl task rows with status unbuilt/labbed** —
do not duplicate them here. Parked plans (auto-architect, vscode wave 4, measures,
friction inventory) wake on demand from plans/.
