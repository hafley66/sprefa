# Brief: the clock checker stops enumerating paths

Base sha: the spawner prints it. FIRST ACTION `git merge --ff-only <sha>`; failure = stop and
report. Never spawn subagents. Commit every green step. PR against `main`. `timeout` on
every command; `export CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=4`.

## The defect, measured (PR #410, 2026-08-21)
`v6/dl/ghcache/ghcache.dl6` (84 rels, 81 rules) parses, plans and type-checks, then
`compile.pl:239 check_step(clock, check_clock_program(Prog))` never returns: `Stack limit
exceeded` inside `clock_violation/2`'s `setof` after ~20 min at the default stack; 3m14s and
still running at 12G. A 31-rule linear chain and a 27-rule diamond ladder (2^13 simple paths)
both check in 0.25s, so the blowup is route count through mid-chain rels, as
`ARCH.pl:894 clock_check_path_blowup` already recorded on 2026-07-31 and marked `done`.
It is half done: `recurrence_free_clock/6` (`v6/prolog/3_clock_check.pl`, grep
`zero_weight_cycles_only`) propagates offsets only when every cycle is zero-weight and
otherwise falls through to the exponential `clock_path/7`; the resource bound was never
added; and the clause order puts `clock_path_conflict` (`3_clock_check.pl:336`) BEFORE
`unconstructive_clock_cycle` (`:348`), so the non-terminating clause runs first.

## Build exactly this
1. Offset algebra per SCC and per edge, no path enumeration anywhere on the compile path.
   For each strongly connected component (use the memoized SCC module
   `prolog_graph_cleanup` landed, `ARCH.pl:832`; grep `clock_scc` and `library(ugraphs)`),
   assign each node a potential; an edge `u -> v` with delay `d` is consistent when
   `pot(v) = pot(u) + d`; a conflict is the FIRST edge whose required potential disagrees
   with an assigned one (Bellman-Ford style relaxation over the component, O(V*E)), and
   the reported `clock_path_conflict(Origin, Ref, Left, Right)` names that edge's two
   offsets. Productive (non-zero-weight) cycles stay
   `unconstructive_clock_cycle(Component, Reason)`; test the clause order by a fixture where
   both exist and the cycle is reported without touching the path clause.
2. Resource bound: a named unsupported construct `clock_check_budget(Nodes, Edges, Limit)`
   raised BEFORE the walk when `V*E` exceeds a limit read from one constant; the message
   names the numbers. This is the self-diagnosis law: a cliff is named, never fatal.
3. `inferred_clock/4` still calls `clock_path/7` (`ARCH.pl` row `inferred_clock_path_residual`).
   Move it onto the same potentials; if a productive-delayed cycle makes propagation
   non-terminating, bound it by the same budget and name it. Delete `clock_path/7` when
   nothing calls it.
4. ARCH: flip `clock_check_path_blowup` to `done` only with this PR's receipts in the row
   text; close `inferred_clock_path_residual`.

## Receipts (all three pasted in the PR, run three times each)
- `ghcache.dl6` through `check_clock_program/1` in under 1s; then the full compile of
  `v6/dl/ghcache/ghcache.dl6` on the Rust door succeeds (`v6/dl/ghcache/gate.sh` or
  `just ghcache` if a recipe exists, else `swipl -q -l v6/prolog/compile.pl -l
  v6/prolog/emit_rust.pl -g "compile_dl6('v6/dl/ghcache/ghcache.dl6','/tmp/x.rs',
  [emitter(emit_rust:emit_program)])" -t halt`).
- COUNT test in `v6/prolog/compile/test/3_clock_check.test.pl`: inference count of the
  checker on a 40-rule, 6-route diamond ladder under a fixed ceiling (use
  `statistics(inferences, _)` before and after); sabotage receipt in the test header: with
  the old `clock_path/7` restored the ceiling is exceeded.
- The existing 694-line test file stays green; conformance `cd v6/prolog/conformance &&
  swipl -g go -t halt go.pl` stays 439 PASS or grows; `just plunit` 1041/0 or grows;
  `bash v6/sprefa-engine-rs/grade.sh` stays graded=439 byte-clean=335 (no emitted byte may
  change: the checker emits nothing).

## Ownership (disjoint from the live arrivals lane)
Yours: `v6/prolog/3_clock_check.pl`, `v6/prolog/compile/test/3_clock_check.test.pl`, a new
fixture under `v6/prolog/conformance/fixtures/`, the two `ARCH.pl` rows, `v6/dl/ghcache/gate.sh`
(only to run it). FORBIDDEN: every other file under `v6/prolog/` (parser, registry, lower,
emitters belong to `feature/arrivals-and-ticks`), `v6/sprefa-engine-rs/**`, `v6/tsv2/**`,
`v6/dl/ghcache/ghcache.dl6`.

## Style laws
No em dashes. Banned words: provenance, substrate, load-bearing, regime, refusal, "ground
truth" (say oracle). Comment budget: constraints only; sabotage receipts in TEST headers
stay. Failure ledger entry in `docs/failure-modes.md` (next number after the last, check
the tail for duplicates first): "an ARCH row marked done for a half-landed fix".
