# Brief: shared SQLite frontier, round two: land it on the Rust door with a measured effect, or close it with the number

Base sha: c88ebb0fd50e91c6ccdea67157747add8eac197a (origin/main). Branch to continue: `origin/feature/shared-frontier-fable`
(6 commits, base 942cf1443, now 262 commits behind main). FIRST ACTIONS: create your
worktree on that branch, `git merge origin/main` (expect conflicts in `lower.pl`,
`compile.pl`, `emit_rust.pl`, `incremental.rs`; resolve keeping main's shape and
re-applying the branch's intent), `bash v6/tools/doctor-deps.sh` (DEPS OK x2). Never spawn
subagents. Commit every green step. PR against `main`.

## The user's word (2026-08-22)
"We found this had little effect but I'm open to another round of getting it to work
again." So this round ends in ONE of two PRs: (a) the feature landed behind its flag with a
measured win on a real program, or (b) a close-out PR that deletes the branch's code and
records the measured non-effect in `v6/labs/BENCHMARKS.md` and `docs/failure-modes.md`.
You decide which by the numbers below, and you say which in the PR title.

## What round one built (read `TASKS/shared-frontier-lowering.REPORT.md` on the branch)
- `frontier(shared)` compile option (`compile.pl` `frontier_option/2`); shared
  `__frontier` / `__next_frontier` heaps with `(relation_id, _phase)` index; per-rel TEMP
  VIEWS for read compatibility; stage/promote/merge rewritten as one statement over the
  shared pair; plan metadata `shared_frontier: {relation_id}`.
- Parity gates both doors, 4 fixtures (sf_arrivals, sf_keyed_replace, sf_join, sf_guard);
  statement counts per_rel vs shared: 60/48, 37/37, 61/45, 46/38; EXPLAIN shows SEARCH.
- NOT done: step 5, the shared `support_count(relation_id, row_id, rule_id, count)` table
  and the retraction recount against it, with a retraction battery; step 6, the default
  flip measured over the corpus; `lowered_program_data/2` filled with placeholders.
- The plan: `plans/2026-08-19-shared-sqlite-frontier.md` (PokeAPI TS program 6.08 MB,
  DDL 1.7 MB, catalog 1.8 MB, plans 1.4 MB: the motivation is codegen size and table count).

## Rust door only (user 2026-08-21: tsv2 is paused)
Drop the branch's `v6/tsv2/**` commits and files (runtime branches, tests, scripts). The
TS emitter output for every unchanged program stays byte-identical (`grade.sh` byte-clean
count is the receipt). The Rust runtime (`sprefa-engine-rs/src/incremental.rs`) is the one
that carries the shared-frontier branches.

## Build this
1. Step 5: shared `support_count` and the recount dance rewritten against it; a retraction
   battery (three arms: keyed replace, rule-derived retraction, departure through a join)
   as conformance fixtures graded by the oracle, plus the Rust parity gate
   (`v6/sprefa-engine-rs/shared-frontier-gate.sh`) extended to them.
2. Measurement on real programs, three runs each, numbers in the PR body:
   - `v6/dl/ghcache/ghcache.dl6` through `bash v6/dl/ghcache/gate.sh` per_rel vs shared:
     emitted Rust bytes, DDL statement count, tables created, statements per tick, fold
     wall_ms.
   - the largest program under `v6/prolog/compile/out/` by DDL bytes (find it, name it).
   - `just engine-bench` if it exists on main, else the reachability fixture the
     `TASKS/engine-bench.BRIEF.md` names, at 100k rows.
3. Decision rule, stated in the PR: land (a) only if shared cuts DDL bytes OR statements
   per tick by at least 15% on ghcache.dl6 with fold wall_ms not worse by more than 5%;
   otherwise (b) close-out.
4. (a) only: step 6 is NOT yours; the default stays per_rel; `ARCH.pl` gets a task row
   for the flip. (b): delete the option, the views, the gates and fixtures; keep the
   BENCHMARKS.md row and the ledger entry.

## Design link, read only
The shared `support_count(relation_id, row_id, rule_id, count)` is the Z-set weight table
the user's open `__count` storage-law row describes (a derived row carries a weight,
retraction is decrement, zero drops). Name that in the PR body where it is true; do not
design the law, do not change rule semantics, do not touch the parser.

## Gate (every number in the PR body)
- `cd v6/prolog/conformance && swipl -g go -t halt go.pl` (440 PASS at base)
- `cd v6 && just plunit` (1058/0 at base)
- `bash v6/sprefa-engine-rs/grade.sh` (graded=440 byte-clean=335 at base; byte-clean must not move)
- `cd v6/sprefa-engine-rs && cargo test` (158/0 at base)
- `bash v6/dl/ghcache/gate.sh` (ticks=10 at base), `cd v6 && just ghcacher-rust` (goldens=6)
- `swipl -g go -t halt v6/prolog/ARCH.pl`
`export CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=4`; `timeout` on everything; nothing in the
foreground over 10s: background and poll. Read `.github/CI-KNOWN-RED.md` before calling a
leg broken; measure a failing leg three times.

## Ownership
Yours: `v6/prolog/lower.pl` (frontier sections only), `v6/prolog/compile.pl` (the option),
`v6/prolog/emit_rust.pl`, `v6/sprefa-engine-rs/src/incremental.rs`, `src/sql.rs` if the
DDL path needs it, `v6/sprefa-engine-rs/shared-frontier-gate.sh`, `tests/shared_frontier/**`,
new conformance fixtures under `v6/prolog/conformance/fixtures/` named `sf_*`,
`v6/labs/BENCHMARKS.md` (append), `docs/failure-modes.md` (append), `TASKS/shared-frontier-*.md`.
FORBIDDEN: `v6/tsv2/**`, `parse_dl_dcg.pl`, `analyze.pl`, `v6/dl/ghcache/**` (measure it, do
not edit it), `rulings.pl`, `CLAUDE.md`.

## Style laws
No em dashes. Banned in prose and identifiers: provenance, substrate, load-bearing, regime,
refusal, "ground truth" (say oracle). Comments state constraints only. Descriptive dl
variable names. Surrogate INTEGER keys; no composite TEXT keys; read
`.claude/skills/sql-relational-design` and `.claude/skills/sqlite-costs` first.

## Reaching the coordinator
`boop beep hail sprefa-coordinator --from <your-lane-name> --body "<one line>"`. Use it when
blocked, when done (PR number, (a) or (b), every gate number), when this brief is wrong.
`boop beep lane list` shows your lane name. A lane that ends its turn parks idle; hail first.
