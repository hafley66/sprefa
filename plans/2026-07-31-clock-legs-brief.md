# Clock checker legs — brief (opus worktree, QUEUED behind float/avg merge)

User (2026-07-31): "make sure that the clock checker has legs, we will
maybe try to build on it later." The checker must stand on its own:
catching real bug classes, not just typing rules.

## Starting state (do not rediscover)

- ARCH task clock_check is ACTIVE with the 2026-07-30 user rulings inline
  (ring/sign/grade labeling then clock inference; bool/float value rulings;
  historical bug-class replay gate REQUIRED before completeness claims).
- clock_checker_proof_payoff (labbed): ranked zero-surface iteration —
  (1) project rule dependencies labelled ring/sign/grade from current AST
  + registry; (2) infer path offsets, reject unequal clocks; (3) accept
  only monotone-B zero-grade SCCs + positive-delay recurrence; (4)
  live-parent/missing-target as boundary antijoin; (5) proof facts exposed
  for fixture comparison. Read the payoff plan and the focused clock suite
  (24/24) first.
- TICK-MODEL.md holds the semiring/grading semantics the labels must match.

## Task (in rank order, stop cleanly wherever time runs out)

1. Items (1)+(2): dependency projection with ring/sign/grade labels +
   path-offset inference, exposed as proof facts (item 5's shape) so
   fixtures can assert them. Registry may need ring/grade metadata
   columns — you own registry.pl in this lane (float/avg lane merged
   before your base was cut).
2. THE REPLAY GATE (the "legs" proof): a fixture set replaying historical
   bug classes this checker must catch statically — at minimum: the A4
   keyed-arrival divergence class, edge_body_joins_arrival_fed_level
   (tick phase), latest-in-edge backlog replay (A12), C2 same-tick
   transition loss, not-in-arm stratification. Each: a program the
   checker REFUSES or LABELS, with the historical incident named in the
   fixture header. A checker that passes all replays has legs; one that
   cannot see a class gets that class recorded as out-of-scope BY NAME.
3. Item (3) SCC acceptance if 1-2 land with room.

## Receipts required

- Focused clock suite green (state count movement from 24).
- Replay-gate fixtures each red-before/green-after or named out-of-scope.
- Proof facts asserted in at least 3 fixtures (item-5 shape).
- Battery: conformance, sweep both modes, plunit, text-door.

## Fences

- Touch: 3_clock_check.pl + its tests/fixtures, registry ring/grade
  metadata, TICK-MODEL.md status rows, proof-fact plumbing.
- Do NOT touch: emitter statement generation, runtime, READINESS/bench
  lanes' files, 0_refusal_messages.pl.
- git worktree, base sha stated at dispatch; verify FIRST, STOP on
  mismatch. Commit per step `git commit -n`; no push.
