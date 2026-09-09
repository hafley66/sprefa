---
created: 2026-09-08
updated: 2026-09-08
type: feature
reporter: fable
status: open
priority: high
epic: extract-port-closeout
labels:
- pkg:extract
- review-field-test
---

# cfg family: exception edges for the try family plus cfg_dom / cfg_postdom

## Description

## Description

Field test 2026-09-08 (see epic Agent Runs, and research/2026-09-08-fact-sources-beyond-scip.md §2.10, §5 row 1): the best finding of the session, a double release across two nested `finally` clauses, had to be rebuilt by hand from `try_statement` / `finally_clause` span containment because `--family cfg` has no exception edges and no post-dominance.

Lowering rule, per tree-sitter grammar with a try family: every call, `throw`, `await`, `yield` inside a `try` block gets successors to the matching `catch` and `finally`; `finally` gets successors to the continuation and to the rethrow path; `return` inside `try` routes through `finally`. Every call is assumed able to throw (CodeQL's JavaScript CFG uses the same approximation). From that CFG emit `cfg_dom` and `cfg_postdom` by the standard iterative algorithm.

## Acceptance Criteria

- [ ] `extract --family cfg FILE` emits `cfg_edge kind=exception` rows for TS, Rust, Go, Python, Kotlin fixtures with try/catch/finally
- [ ] `cfg_postdom` relation emitted; a fixture with a `finally` post-dominating a `try` body has the row
- [ ] a dl6 rule over cfg facts flags the 2026-09-08 double-release fixture in one query
- [ ] receipt in the epic

## Tests Run

- [ ] fixtures per language above

## Implementation Notes

- [ ] one lowering rule per grammar; dominance once

## Comments

### 2026-09-08T17:48:07Z · @fable

Acceptance fixture and reference queries: research/2026-09-08-codeql-review-queries/ (CodeQL reproduces the expected rows: fixture 3/1/1/1, fixed package 0/0/1/0). Doc: research/2026-09-08-fact-sources-beyond-scip.md §8.
