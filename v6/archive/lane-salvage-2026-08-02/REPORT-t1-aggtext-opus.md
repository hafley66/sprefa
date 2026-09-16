(Coordinator note: the agent could not write files at the worktree root through the harness; report text relayed verbatim from its final message.)

# oracle-side refusal for numeric aggregates over TEXT

See coordinator session for full text; headline findings:
- Class is FOUR aggregates (sum/avg/min/max share compile_aggregate_number_operand), not two.
- Compiler payload's middle argument is an unbound variable (names nothing) — not a shape worth copying.
- Two-layer fix: shared load-time program_violation (declared types) + engine-local runtime value guard aggregate_value_not_number for the undeclared-column residue.
- 5 plunit tests (2 red pre-fix), plunit 271->276, conformance 281/0, TEXT_DOOR 196/196/0, COMPILE_SPEED 0 regressions, lint 1/1, ARCH 7/0.
- tsv2 sweep NOT run (worktree lacks node_modules), compiler provably unmoved via compile-speed + text-door gates.
- Files: 0_program_check.pl +52, engine.pl +13, level_eval.pl +26, plunit_tests.pl +86.
