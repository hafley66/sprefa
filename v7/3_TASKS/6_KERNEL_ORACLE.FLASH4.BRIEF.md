## Description

Write one fixture and one test that proves reader output, compiler type closure,
normalized runtime rules, runtime reference closure, and cleanup determinism.

## Acceptance Criteria

- [ ] Exactly one focused test exists.
- [ ] One complete expected term covers every output listed above.
- [ ] The fixture is compiled twice in one SWI process.
- [ ] No existence-only or fragmented assertions.
- [ ] No V6 test, generated corpus, or engine suite runs.
- [ ] Test changes stay under `v7/5_TEST/`.

## Test Run

Run the single SWI command once and record exact pass/fail counts.

## Stop condition

Report production defects to the owning card. Do not patch production code.
