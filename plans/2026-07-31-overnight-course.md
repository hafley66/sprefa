# Overnight course 2026-07-31 (user asleep; directives verbatim)

- "start cooking on opus, push often, plot course for lowering the
  compiler into rust after we have language agnostic cli based benches"
- "plan a bunch of scouting or missing features and also make sure that
  the clock checker has legs, we will maybe try to build on it later"
- "i want the golden use cases of v5 ready to go tho"

Rust course: plans/2026-07-31-rust-course.md (benches gate it).
Push cadence: session branch pushed after every landing. NOTE: ff of
`main` to the session tip was CLASSIFIER-BLOCKED for the agent; needs
your hand or a settings rule (morning item).

## Lanes (opus per directive, disjoint ownership)

1. **v5 golden use-cases readiness** (opus worktree, priority per "i want
   ... tho"): grade all 9 stopping-point programs ready-to-go; the
   justfile already carries ghcacher-golden / rtkq-golden /
   multirepo-golden / flagship / lsp-diags / extraction-live receipts —
   run each, produce READINESS.md (runs today / small gap fixed in-lane /
   big gap priced), fix only small gaps.
   Brief: plans/2026-07-31-golden-readiness-brief.md.
2. **CLI bench contract** (opus worktree): rust-course phase 0.
   Brief: plans/2026-07-31-bench-cli-brief.md.
3. **Clock checker legs** (opus worktree, QUEUED behind float/avg merge —
   both want registry.pl): execute the ranked zero-surface iteration from
   the clock_checker_proof_payoff lab (ring/sign/grade dependency
   projection, path-offset inference, monotone-B SCC acceptance) + the
   historical bug-class replay gate the ARCH row demands before the
   checker may be called complete.
   Brief: plans/2026-07-31-clock-legs-brief.md.
4. **Beta lanes** (running): refusal messages (review gate in progress),
   float/avg (codex, running). Then fork_join_malformed_json +
   GETTING-STARTED per the beta plan.

## Scouting list (read-only, cheap, fill idle slots)

- v5-utility distance table refresh: which of the review's M/L gaps
  closed since (watcher landed, CLI landed, LSP landed).
- scan() spelling: 105/129 v5 examples use scan(); v6 has no spelling —
  the single biggest migration blocker, needs a design card not code.
- B4 residuals after the messages lane: swipl banner noise, refusals
  thrown from TS-side (bop server path) still raw.
- golden_flake_hunt (ARCH row): store golden.test 1-in-N under load.
- reactor_buffertime_flake (ARCH row): known fix shape, small.
- pairwise_single_tick_wrong + json_pattern_expand: priced, unowned.
- /ticks has no snapshot twin (devlog finding).
