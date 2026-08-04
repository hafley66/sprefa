# Lane report: fuse the three rxprim plans into one contract

Worktree: `/Users/chrishafley/projects/sprefa-lab-fuse`, base verified
`3d97fd4f` before anything else (matches the required head; no deviation).
Docs only, three files created, no code edits, no commits.

## Files created

- `plans/2026-08-04-rxprim-fused-contract.md`
- `plans/2026-08-04-rxprim-fused-contract.visual.human.unga.md`
- `REPORT.md` (this file)

## Inputs present (all read in full)

| input | path | status |
| --- | --- | --- |
| BASE | `~/projects/sprefa-plan-rxprim-opus/PLAN.md` | read |
| GRAFT A | `~/projects/sprefa-plan-rxprim-kimi/PLAN.md` | read |
| GRAFT B | `~/projects/sprefa-plan-rxprim-flash/PLAN.md` | read |
| GRAFT LIST | `plans/2026-08-04-rxprim-duel-verdict.md` | read |
| MARBLE | `plans/2026-08-04-marble-type-lattices.md` | read |
| RULINGS | `v6/prolog/conformance/rulings.pl` tail | read |

## What was grafted from where (section map)

| graft | source | landing section |
| --- | --- | --- |
| spine: one(Positions), S1-S3, receipts R1-R5, fixtures F1-F13, landing sites | opus PLAN.md | 2, 4, 6, 8, 9, 11 |
| reconcile table (directive's four sketches dispositioned) | flash PLAN.md preamble | 3 (Preamble: reconcile table) |
| block sugar + desugar discipline (0_merge_expand.pl before analyze), block-vs-match grammar delta (kept verbatim) | kimi PLAN.md §3a, §4a | 7 |
| cascade COUNT numbers: 6 arms = 15 edge_absence edges, 21 arm terms; the two linear COUNT tests | kimi PLAN.md §4d | 9c (grafted second COUNT test) |
| keyed-fixture rewrite safety (key_same_tick_ordered_not_conflict, merge_family.pl:74-80) | kimi PLAN.md §6.2 | 9a (safety receipt) |
| loud-loser requirement as open question | flash PLAN.md §Construct 2, §Emitter | 12 Q4 |
| marble: three lattice value sets + extension laws, kept verbatim; summarized to ~40 lines, doc linked | plans/2026-08-04-marble-type-lattices.md | 13 |
| ruled ground: tick_boundary, admission_word, block_lowering_first, quoted verbatim | v6/prolog/conformance/rulings.pl (last three entries) | 1 |

## What was discarded and why (one line each, citing the verdict doc)

| discarded | why (per plans/2026-08-04-rxprim-duel-verdict.md) |
| --- | --- |
| kimi's edge_head_conflict_risk stand-down | "reopens the source-arm-order tiebreak the ruling removes" (verdict Discards) |
| kimi §5 + flash §3 typed merge | "both assume enum-name column typing and tag-column exhaustiveness that measurably do not exist today; opus R3+S3 replace them with four named checker edits" (verdict Discards) |
| flash's COUNT-as-conformance-expectation | "form the harness does not have" (verdict Discards) |

These three are also recorded in section 14 of the contract.

## Ruling rewrites applied

- `admission_word`: every `throttle(...)` / `zip(perKey, ticks$)` live spelling
  rewritten to lossless queued admission, concat-family, concatMap-shaped rx
  lowering, exact surface spelling left OPEN (contract sections 1, 10a, 12).
  The two historic names appear only in the verdict-recap section 15.
- `block_lowering_first`: every block-sugar wording rewritten so the lowering
  is the construct (flat mangled rels + catalog rows + captured-arg
  distribution), braces a later sugar wave (contract sections 1, 7, 10b).

## Banned-word compliance

- content scanned for: provenance, substrate, load-bearing, regime, support,
  demand, gen, scan (file enumeration), em dashes. Not present in the new
  prose or introduced identifiers. Verbatim ruling quotes (section 1) retain
  source wording including "demand-key" inside the block_lowering_first quote
  and "concatMap" inside admission_word, as required by the quote-verbatim
  instruction.
- dl variable names are descriptive (`Repo`, `Bucket`, `DispatchId`,
  `SealedId`, `Row`), never single letters.

## Validation outputs (verbatim)

```
$ git status --short
?? REPORT.md
?? brief.md
?? plans/2026-08-04-rxprim-fused-contract.md
?? plans/2026-08-04-rxprim-fused-contract.visual.human.unga.md
```

Note: `brief.md` is untracked and pre-existing in this worktree (present at
lane start, per the initial directory listing); it is not created by this lane.
The three files created by this lane are the three remaining untracked items.

```
$ grep -c "concatMap" plans/2026-08-04-rxprim-fused-contract.md
6
```

```
$ grep -c "throttle(1)" plans/2026-08-04-rxprim-fused-contract.md
1
```

Quickest unambiguous output (run directly):

`grep -n "throttle(1)" plans/2026-08-04-rxprim-fused-contract.md`
-> `808: word 1 between the opus `throttle(1)` spelling and the shelf sketch's`

`throttle(1)` count: 1 occurrence, located on line 808 in section 15 "Verdict
recap" (the history/verdict recap section that reports the duel fork and the
admission_word rejection). Zero occurrences in the live body of the contract.
