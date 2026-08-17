# 042 Evaluate capitalized relation names

Read `/Users/chrishafley/projects/sprefa-v6/issues/capitalized-relation-names/item.md` and evaluate it. This lane is read-only except for one report file:

`v6/plans/2026-08-17-capitalized-relation-names.md`

Do the research directly in this lane. Do not delegate. The prior interrupted
attempt established two facts that must appear in the report:

- `Person` and `Commit` propagate verbatim into SQLite tables, struct types,
  ref columns, and `rel_catalog`.
- case-only `Person` and `person` declarations currently compile as distinct
  SQLite tables and catalog rows.

The lane is complete only after the report exists and is committed. Stop
research after the requested matrix is filled; do not broaden the task.

Start from the already-proven parser case:

```dl6
rel Person(id: int).
rel Commit(author: Person).
```

Trace actual code and run focused probes for parser/CST, printer and roundtrip, rule heads and calls, variables, keyword puns, dotted imports, module-qualified types, generic/template expansion, enum/list/option minted names, SQLite identifiers, collision checks, ProgramJson, Rust, TypeScript, JSON Schema, and OpenAPI.

Catalog exact files, predicates, emitted spellings, and collisions. Check lowercase compatibility and names differing only by case. Do not implement a naming change.

The report must contain:

- current behavior table
- concrete syntax examples
- naming flow from authored relation to every generated target
- collision and filesystem portability results
- migration surface with exact golden families
- recommendation: allowed, preferred, or blocked, supported by evidence

Run `git diff --check`. Commit only the report with subject `docs: evaluate capitalized relation names` and trailer `Refs-Issue: @capitalized-relation-names`.
