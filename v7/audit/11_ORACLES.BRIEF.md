# Slice 11: conformance corpus and semantic oracles

Read `0_SHARED.md` first.

Primary files:

- `v6/prolog/conformance/rulings.pl`
- all files under `v6/prolog/conformance/fixtures/`
- `v6/prolog/compile/test/plunit_tests.pl`
- focused test files under `v6/prolog/compile/test/`

Classify semantic laws by the first V7 component that must preserve them:
reader, scope resolver, type/comptime closure, checker, lowerer, emitter, or
runtime. Identify a minimal V7 parity corpus rather than porting fixtures by
file count.

Write `v7/1_AUDIT/results/11_ORACLES.md`.
