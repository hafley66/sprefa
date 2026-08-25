---
created: 2026-08-25
updated: 2026-08-25
type: epic
owner: hafley66
status: open
priority: high
---

# Extract as an ast-grep extension over soopy

## Description

One owner per layer: tree-sitter grammars, ast-grep match/rules/fix, extract facts + dl6, soopy staging and atomic commit. Plan: plans/2026-08-25-extract-astgrep-soopy.PLAN.md and the .visual.human.unga.md twin. Three arcs, A and B parallel, C after both.

## Goal

`extract` owns facts and file selection; ast-grep owns matching, pattern syntax, YAML rules and fix generation; soopy owns staging, the expected-hash guard and the atomic commit. No second rewrite engine.

## Issues

- [x] @astgrep-arc-a-languages (#472, #474): `ExtractLang` implements `Language` for dl6, prolog, markdown
- [x] @astgrep-arc-b-drain (#473): `Edit` drains into soopy `SourceAction`, `Act` deleted
- [x] @astgrep-arc-c-factmatcher (#475): `FactMatcher` over dl6.db, `extract move` as one YAML rule (after A and B)
- [ ] @extract-move-typescript: the move verb walks .ts/.tsx, resolves TS paths, takes a batch list

## Phases

1. A and B in parallel, disjoint files (A: `src/lang/**`; B: `src/0_move.rs`, `src/drain.rs`, `tests/1_move.rs`).
2. C after both merge.
