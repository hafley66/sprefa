# Typed annotation evaluation and key bridge

Implement the second card from `plans/2026-08-19-applicative-type-annotations.md` after the surface commit is available.

Scope:

- Validate annotator relations as `Target:type` first input plus one `return:type` output.
- Execute ordered applications through the existing compiler-relation evaluator.
- Require exactly one type result per step with named diagnostics.
- Retain evidence keyed by member site and sequence position.
- Make `key(Target) -> Target` evidence feed the existing key normalization and SQL behavior.
- Ensure compiler relations/evidence never enter runtime DDL, boot facts, or DD inputs.
- Add focused compiler and SQLite behavioral CI.
- Commit with `Refs-Issue: @type-annotation-eval`.
- Run `boop tell-parent --kind completion --body "type-annotation-eval done commit=<sha>"` after CI.
