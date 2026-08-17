---
created: 2026-08-15
updated: 2026-08-15
type: feature
reporter: fable
status: open
priority: high
epic: bug-mining
labels:
- size:large
- area:testing
- bugmine
- pkg:prolog
- pkg:tsv2
- pkg:engine-rs
---

# Grammar fuzzer with three-door differential judging

## Description

Generate random .dl6 programs from the registry grammar (v6/prolog/compile/registry.pl, 83 constructs), compile, run oracle + TS door + Rust door, byte-diff final state + tick logs. Shrink divergences with the dd arm (6_isolated_compiler_dd.pl). All three judges exist; only the generator is missing. Evidence: the mutual-recursion silent-wrong (fixed PR #266) was a two-rel cycle no corpus fixture had; a generator finds that class in hours. Large: generator design is creative work (weighted grammar, well-typedness constraints so most programs compile).

## Agent Runs

### 2026-08-16T01:02:42Z · @claude

plans/2026-08-15-fuzz-grammar-threedoor.PLAN.md + plans/2026-08-15-fuzz-grammar-threedoor.PLAN.visual.human.unga.md (commit 98b034b8): BUY quickcheck pack for the trial driver and shrink hooks, ADOPT SQLsmith/Csmith catalog-driven well-typed generation plus Groce swarm, BUILD only arbitrary(dl_program,P) and a term-level HDD reducer; 6_isolated_compiler_dd.pl is the differential-dataflow plan emitter, NOT a shrinker, and no reducer exists in the tree; five arcs F1-F5 with disjoint file ownership.
