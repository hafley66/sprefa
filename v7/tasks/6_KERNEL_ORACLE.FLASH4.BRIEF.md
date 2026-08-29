# Pin the DL7 vertical kernel with one consolidated oracle

## Description

Extend the existing entrypoint test module with one vertical test for the
Partial fixture. Keep the expected result as one normalized term so absolute
source paths and semantic IDs can vary while graph meaning remains exact.

## Acceptance Criteria

- [ ] One focused Partial test is added to the existing entrypoint test module.
- [ ] One expected term covers compiler diagnostics and row count.
- [ ] The same term covers generated Partial node, classifier, labels, targets, and indices.
- [ ] The same term covers checked runtime graph and program counts plus normalized call shapes.
- [ ] The same term proves evaluator temporary clauses are empty after compilation.
- [ ] The fixture compiles twice in one SWI process with identical compiler and runtime terms.
- [ ] No existence-only assertion or additional test file is added.
- [ ] No V6, Rust, TypeScript, generated corpus, or engine suite runs.

## Tests Run

- [ ] One focused SWI command passes all 7 consolidated V7 tests.

## Implementation Notes

The test lives in `v7/test/1_entrypoints.test.pl` and reuses
`v7/test/fixtures/2_partial.dl7`.
