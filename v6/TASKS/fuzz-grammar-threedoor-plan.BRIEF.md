# fuzz-grammar-threedoor-plan (issue: fuzz-grammar-threedoor, size:large, PLAN ONLY)

FIRST ACTION: `git merge --ff-only e23893b2ef8d3e4c5f60f0a98f015b95dea23128`. Failure = STOP AND REPORT.
Read CLAUDE.md at repo root. Issue body:
/Users/chrishafley/projects/sprefa/issues/fuzz-grammar-threedoor/item.md

GOAL: a dispatch-ready PLAN for the grammar fuzzer with three-door differential
judging. PLAN ONLY — zero implementation. The three judges exist (oracle, TS
door, Rust door, byte-diff of final state + tick logs); the dd shrinker exists
(6_isolated_compiler_dd.pl); ONLY the generator is missing. Evidence this
pays: the PR #266 mutual-recursion silent-wrong was a two-rel cycle no corpus
fixture spelled; a generator finds that class in hours.

BUILD-VS-BUY IS MANDATORY AND FIRST: grammar/property-based generation is a
common-shaped problem. Research SWI-Prolog property-testing/generation
libraries (quickcheck ports, plunit generators, anything on pack list), plus
grammar-fuzzer approaches worth porting (weighted-grammar / csmith-style /
Hypothesis-style swarm). Written candidate-by-candidate table with a verdict
per candidate. No one-line dismissals. Only after that may the plan contain a
bespoke generator design, and it must say exactly what the libraries cannot do.

PLAN CONTENT (in this order):
1. Candidate table (library research above).
2. Generator design: seed grammar source (v6/prolog/compile/registry.pl, the
   construct registry — count them yourself), weighting, well-typedness
   constraints so most programs compile (state the target compile rate and how
   it is measured), reproducible seeds.
3. Judging loop: how a generated program flows through oracle/TS/Rust and what
   "divergence" means precisely (byte-diff of what artifacts).
4. Shrinking: hand-off contract to 6_isolated_compiler_dd.pl.
5. Budget: runtime per program, per-batch caps (the 10-second law applies per
   operation), where results/corpus additions land.
6. Phasing: 3-5 dispatchable arcs, each with its own gate and files-owned list,
   sized for pro4/flash4 lanes.

DELIVERABLE (both docs mandatory):
1. plans/2026-08-15-fuzz-grammar-threedoor.PLAN.md — receipts + citations,
   opens with a TOC.
2. plans/2026-08-15-fuzz-grammar-threedoor.PLAN.visual.human.unga.md — plain
   words, mermaid (generator->doors->diff->shrink pipeline), zero citations.

FILES YOU OWN: those two docs only. Everything else read-only.
FORBIDDEN: all code, fixtures, scripts, out/. Do NOT close the issue (it is
the implementation issue; note the plan path on it with
`issuectl note fuzz-grammar-threedoor --agent-run "<plan paths + one-line verdict>"`).

VALIDATION: every existing-component claim (judges, dd arm, registry construct
count) verified by opening the named file; counts from your own greps, pasted.

COMMIT plain (docs only). Report: chosen approach, candidate-table verdict
line, the 3-5 arcs with sizes.
