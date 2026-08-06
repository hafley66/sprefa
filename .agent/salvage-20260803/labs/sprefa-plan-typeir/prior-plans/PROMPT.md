# Plan: schema facts in prolog -> generated rs/ts/sql (MVP first)

You are a planning agent in this worktree (base 92756b54). Deliverable:
`PLAN.md` at worktree root. Create ONLY `PLAN.md` and optional `plan-notes/`.
Never commit or push.

## Direction already chosen by the owner (do not relitigate)
- The type/schema source of truth lives ON DISK as PROLOG FACTS — a
  sectioned-off fact file. Prolog is the codegen engine (repo precedent:
  emit_cli_inventory.pl, emit_openapi.pl, 1_emit_registry_docs.pl marker
  sections).
- Rust/TS/SQL are emit targets. Seeding the facts may harvest existing rust
  (spine.rs sea-orm models) and ts (types.ts row interfaces) — possibly via
  sprefa-extract itself.
- The old "TS bindings frozen" note in DECISIONS.md is NOT binding.
- Stage-setting for N languages matters, but the deliverable is judged on
  the MVP slice.

## Ground truth
Read `RESEARCH.md` in this worktree first (verified inventory: schema sites,
port map, codegen precedents, gaps). Key files to open yourself:
- `v6/sprefa-store/src/spine.rs:314-417` (9 tables, sea-orm models, DDL emit)
- `v6/sprefa-store/js/src/engine/types.ts:80-97` (NodeRow/EdgeRow/SpanRow)
- `v6/sprefa-store/js/src/spine.ts` (ts DDL twin)
- `v6/prolog/compile/2_emit_cli_inventory.pl` (facts -> checked-in ts +
  staleness gate pattern)
- `v6/prolog/compile/1_emit_registry_docs.pl` (marker-section replacement)
- `~/projects/hafley-tsp/AGENTS.md:131-175` (read-only: one closed set,
  N emitters; _auto file conventions; no identifier renaming across langs)

## Required PLAN.md structure (owner's planning protocol)
1. **Fact schema, type signatures first**: the exact prolog predicates
   (table/…, column/…, type mapping predicates) with arities and argument
   types. Then pseudo-code of the two emitters as comments. Cover: sqlite
   type <-> rust type <-> ts type mapping table incl. nullability and pks.
2. **MVP slice** — the first testable AND useful codegen: smallest generated
   artifact that RETIRES a hand-kept twin. Candidates to weigh (pick one,
   justify in <=3 sentences): (a) emit ts row interfaces into a marker
   section of types.ts + staleness test vs spine.rs; (b) emit the DDL string
   both sides consume + parity test vs sea-orm create_table_from_entity
   output; (c) emit rust structs. State the exact test that proves
   non-drift (the staleness gate), runnable in CI.
3. **Seeding**: how facts get bootstrapped from spine.rs the first time
   (hand-transcribe 9 tables vs harvest via sprefa-extract ast queries);
   recommend one.
4. **Instance lifetimes / storage layout**: where the fact file lives, where
   generated files land, checked-in or not, marker sections or whole files.
5. **N-language stage-setting**: what in the fact schema must be right NOW
   so future emitters (openapi, jsonschema, typespec import) bolt on without
   fact-schema migration. Max 10 lines.
6. **Buy check**: sea-orm-cli entity gen, openapi-typescript, alloy — what
   each would replace, and why prolog emitters win or lose per candidate.
7. **Effort**: files touched, LOC estimate per step, ordered steps with the
   MVP gate first.

Terse. Tables and signatures over prose.
