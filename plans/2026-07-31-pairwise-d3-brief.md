# pairwise defect + bop check file:line — brief (opus worktree)

Two small, disjoint beta residuals in one lane.

## Part 1: pairwise_single_tick_wrong (beta gate item 3 residual)

ARCH row: the pairwise idiom spelled `finalize+read` pairs (10,9)+(14,9)
instead of (10,14)+(14,9) when changes land EVERY tick with no idle gap;
correct WITH a gap; compiles clean, zero diagnostic. Lab receipt Q1e
recoverable at commit 89ccaccf. Distinct from update-arm U4 same-tick
collapse (that one is DEFINED semantics).

Task, in preference order:
1. Reproduce as a fail-first fixture (both doors) from the Q1e receipt.
2. RCA: is the wrong pair a lowering bug (fixable so oracle and emitter
   agree on (10,14)) or a semantic property of the finalize+read spelling
   (the read samples the ALREADY-REPLACED row when replace and read share
   a tick)? The update-arm verdict (plans/2026-07-29-update-arm-verdict.md)
   defined `changed(K,Old,New) <+ finalize(r(K,Old)), r(K,New)` as
   replace-tick-plus-one with endpoint semantics — check whether the Q1e
   program is that exact spelling and whether the oracle itself produces
   the wrong pair (if BOTH doors agree on (10,9), this is a semantics
   ruling card, not a bug — write it up and stop).
3. If it is a bug in one door: fix that door, byte-identity receipt.
   If it is the spelling's defined behavior: the fixture pins it as
   DEFINED with the explanation in the fixture header, and the ARCH row
   closes as works-as-ruled with a doc note in SYNTAX.md's pairwise/
   update-arm section.

## Part 2: D3 — bop check loses file:line (cold-author shelf)

scripts/bop_check.pl calls compile_program/6 directly, bypassing
compile.pl's throw_text_door_error/2 which wraps reasons in
at(File, Line, Reason). Same file through compile_dl6.sh prints
`broken.dl6:4:`; through `bop check` prints `rule-index unavailable`.
Fix: route bop_check's compile through the same wrapping path (or apply
the wrapper at its catch site). Receipt: `bop check broken.dl6` prints
file:line, exit 2; the getting-started receipt script's section-5 blocks
updated if their captured output changes (run `just getting-started` —
it will tell you).

## Receipts

Battery from v6/: conformance (grader now exits 1 on any fail — trust
exit codes), roundtrip, text-door, plunit, sweep both modes (crash gate
live), bop-test, `just getting-started`. Counts stated.

## Fences

- Touch: emitter/oracle files for part 1 as RCA demands, bop_check.pl +
  getting-started doc/script captured outputs for part 2, fixtures.
- Do NOT touch: bench-cli/**, clock files, READINESS.md, labs/**.
- pnpm install per package, NEVER symlink outer node_modules.
- Commit per step `git commit -n`; no push.
