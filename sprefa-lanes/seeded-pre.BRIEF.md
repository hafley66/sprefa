# feature/seeded-pre: pre/2 — the fold seed, killing every base arm

## RESPAWN NOTE (2026-08-11): the first attempt died wedged in an unresolved
## `git merge` it ran mid-work. NEVER merge, pull, or rebase in this worktree;
## you work from your base sha only, the coordinator handles integration.
## Salvage diff from the dead attempt: sprefa-lanes/seeded-pre-wedged-salvage.patch
## (3209 lines; consult it, trust nothing in it untested).

## Ruled by user 2026-08-10 ("yes, we should be able to ref the pre anywhere,
## its basically a cached let"): pre gains an optional SEED. Semantics extend
## the existing user rows R6 (evolving read) and the R1 rider (chains within
## tick) — read v6/prolog/conformance/rulings.pl:70-93 FIRST; nothing there
## changes, the seed only defines the no-prior-row case.

## The construct
`pre(head(KeyArgs..., Before), Seed)` — body form, edge rules:
- prior row exists for the key: Before binds to it (R6 evolving read, exactly
  today's pre/1).
- NO prior row: Before binds to Seed (a constant or a variable bound earlier
  in the body). The rule FIRES — this is the whole point: the seed arm and its
  not(head(Key, _)) guard become unnecessary.
pre/1 keeps today's behavior (no prior row -> rule does not fire).

Reference before/after (both must be fixtures):
```
# today, two arms:
hop_count(Id, 1)  <+ ev(Id, _), not(hop_count(Id, _Seed)).
hop_count(Id, N)  <+ ev(Id, _), pre(hop_count(Id, Prev)), N := Prev + 1.
# with pre/2, one arm, same final rows tick for tick:
hop_count(Id, N)  <+ ev(Id, _), pre(hop_count(Id, Prev), 0), N := Prev + 1.
```

## The work
1. parse_dl.pl: pre/2 spelling (pre is already a keyword-shaped call ~:1372);
   print_dl round-trips it.
2. Oracle: the no-prior-row binding in the occurrence path that implements
   R6/R1 (find pre/1's implementation via the fixtures in engine_core.pl,
   merge_family.pl, scopes.pl and extend it).
3. Emitter/tsv2: the lowered SQL takes the seed as a COALESCE/default on the
   prior-row read, inlined, never a second statement.
4. Fixtures, fail-first: (a) the one-arm hop_count above, byte-identical
   final state vs the two-arm spelling across 3+ ticks incl a retraction;
   (b) seed as a body-bound variable; (c) pre/2 on a multi-column state rel
   (the fee_stats tuple-accumulator shape); (d) text accumulator with pre/2
   (concat), the breadcrumb fold. TEXT_DOOR + roundtrip fixtures for the
   spelling.
5. Decision row in conformance/rulings.pl (file's row format, user
   2026-08-10): pre_seed, one_arm_folds — quote: "yes, we should be able to
   ref the pre anywhere, its basically a cached let i spose".
6. Clock checker: confirm pre/2 projects the same ring/role as pre/1
   (3_clock_check.pl reads the expanded program; state in the PR body what
   ring the seed read lands in).

## Setup (REQUIRED; absolute cd each command)
```bash
cd <worktree>/v6/tsv2 && pnpm install
cd <worktree>/v6/sprefa-store/js && pnpm install
cd <worktree>/v6/sprefa-extract && cargo build --release
```

## Gate
```bash
cd <worktree>/v6 && just conformance && just plunit && just text-door && just roundtrip
cd <worktree>/v6/tsv2 && bash scripts/sweep.sh
git checkout -- v6/prolog/compile/out/pokeapi_shape.ts
cd <worktree>/v6 && just typecheck && just tsv2-test
```
Manifest: new fixtures compiled, zero bucket flips elsewhere.

## Commit rail (commit-or-report)
Up to 3 commits, prefix `prolog:`. Blocked -> FAILURE-REPORT.md, exact
command + output, exit nonzero. NEVER --no-verify.

## Style
Comments only constraints code cannot show, max 2 consecutive lines. Banned:
provenance, substrate, load-bearing, regime, refusal. Follow each file's
existing style.
