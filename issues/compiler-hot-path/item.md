---
created: 2026-08-21
updated: 2026-08-21
type: improvement
reporter: chris
assignee: chris
status: in-progress
priority: normal
epic: cheap-fast-analysis
labels: [prolog, perf, lane-c]
---

# Compiler hot path: load time and expand_program_with_bindings

## Description

swipl load 348ms; rail compile 248ms of which expand_program_with_bindings 66ms (27%). qlf or saved state, sub-step trace, fix hot predicates byte-identically (grade.sh 439/335, text-door 341/335/6). Lane C.
