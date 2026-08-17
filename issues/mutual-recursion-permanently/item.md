---
created: 2026-08-14
updated: 2026-08-15
type: bug
reporter: fable
status: fixed
priority: high
closed: 2026-08-15
commits:
- hash: 8781e372
  summary: 'mutual recursion closes in-tick, outer rounds (PR #266)'
---

# mutual recursion permanently under-derives on both emitted doors

_Source: v6/prolog/lower.pl_

## Description

Direct self-recursion settles tick 1 on all three doors (oracle, TS, Rust), byte-identical. INDIRECT (mutual) recursion — path<-reach<-path — under-derives PERMANENTLY on both emitted doors (3/6 closure rows, never settles) while the oracle computes the full least fixpoint tick 1. TS naive mode (SPREFA_TSV2_EMITTER_MODE=naive) on the SAME emitted module is correct, so the doors' own naive path already computes the right answer: engine defect, not semantics. The typegen list_type<->element_type shape emits ZERO list_type rows ever on both doors. Silent wrong answers: the direct spelling of the same rule is refused loudly (built_text_in_recursive_head, lower.pl:5249); the two-rel spelling bypasses the guard and compiles. Defect sites: lower.pl:4445+5179-5186 (rules_read_head_recursively tests only DIRECT self-read; indirect cycle gets ExpandPlan=none), 1_incremental.ts:1120-1131 + incremental.rs:975-986 (single pass, no outer round), emit_ts.pl:2624-2631 (incremental_program_safe vacuous), emit_rust.pl:63 (incremental_safe(true) hardcoded, Rust has NO naive fallback), emit_ts.pl:2643-2651 (retraction guard same direct-only test), strat.pl:96-101 (Kahn fallback Ordered=Group, comment at emit_ts.pl:2054 claiming refusal is false). Corpus blind spot: mutual_recursion_matches_oracle (engine_core.pl:452-462) is a depth-1 even/odd cycle one pass closes. USER DECISION NEEDED: close mutual cycles in-tick (outer rounds), refuse them by name, or route to naive path. ARCH.pl:136 intent: in-tick recursion must terminate (datalog guarantee).

## Decisions

### 2026-08-15T04:05:43Z · @chris

PROVISIONAL (2026-08-15, delegated, revisit allowed — alternatives noted, not rejected): fix = close mutual cycles in-tick, outer rounds over the stratum statement pass until no statement adds a row, capped by the divergence backstop. Alternatives kept open: refuse-by-name (blocks dl6-first typegen list_type<->element_type shape), naive-path routing (no Rust naive door exists).

## Agent Runs

### 2026-08-15T04:46:11Z · @fable

FIXED 2026-08-15 (branch worktree-agent-a99e0fe8936d343fc, 7 commits off f11eb079). Outer rounds, as decided. Detection: strat.pl:cyclic_head_groups/2 pairs each head on a positive INDIRECT stratum cycle with its group index (the self edge stays dropped, so the expand wavefront keeps direct self-recursion). Both emitters render it as a level-statement field recursion_group {group, round_cap, heads}, ABSENT on an acyclic head -- only one pre-existing emitted module in the corpus changed text (out/mutual_recursion_matches_oracle.ts). Both runtimes replace the single statement pass with sequence_level_rounds over maximal same-group runs (1_incremental.ts:560, incremental.rs:785), repeating until no statement moves a row and charging fixpoint_round_cap/1; a tripped cap throws diverging_measure_recursion([path,reach], 1000) identically on both doors. SECOND DEFECT found and fixed: topo_order_group/2's cycle fallback was PROGRAM order, splitting a multi-clause head around its cycle peer, and group_adjacent_by_head/2 folds only ADJACENT rules -- the emitted module re-issued the head's __support_next DDL and crashed. Fail-first: fixtures/24_mutual_recursion.pl, pre-fix WRONG (path 3 of 6, reach 2 of 3) and EMITTED_CRASH; post-fix identical on both doors. Gates: conformance 448/0 x3, sweep 341/335/wrong=0 x3, grade byte-clean 334/448 rc=0 x3, plunit 5 known-red x3, golden-flex + typegen golden HOLD x3, typecheck 0. Vacuous gates corrected: incremental_program_safe/4, retraction_guard/2 (a mutual cycle no longer claims plain-count-acyclic). Alternatives (refuse-by-name, naive routing) not taken and not needed.
