---
created: 2026-08-15
updated: 2026-08-15
type: feature
reporter: fable
status: open
priority: normal
epic: bug-mining
labels:
- size:med
- area:testing
- bugmine
- pkg:prolog
- pkg:tsv2
- pkg:engine-rs
related: ['@fuzz-grammar-threedoor']
---

# Pairwise construct interaction matrix

## Description

Goldens cover each registry construct once; bugs live in interactions (option(enum), list-in-key, recursion+aggregate). Enumerate registry x registry, generate the minimal program per pair, bucket compile/run outcomes, diff doors on the compiled ones. Output: a matrix report naming every pair that is silent-wrong, crash, or asymmetric-refusal. Mechanical once the per-pair template exists: med.
