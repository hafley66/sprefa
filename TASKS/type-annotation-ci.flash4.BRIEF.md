# Type annotation cross-target CI

Close the third card from `plans/2026-08-19-applicative-type-annotations.md` after both implementation cards land.

Scope:

- Add one authored DL6 golden covering empty, single, configured, and composed annotations.
- Prove key SQL/upsert parity with the existing key spelling.
- Exercise Prolog compiler, TS + SQLite, Rust + SQLite, ProgramJson, JSON Schema, and generated TS/Rust types.
- Check runtime erasure of compiler annotation relations and evidence.
- Review the full implementation for phase-order or duplicated-evaluator defects and fix findings.
- Commit with `Refs-Issue: @type-annotation-ci`.
- Run `boop tell-parent --kind completion --body "type-annotation-ci done commit=<sha>"` after CI.
