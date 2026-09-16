# feature/openapi-to-dl6: forward converter, pokeapi components round-trip

## Ruled by user 2026-08-11: openapi for pokeapi "to and from good to go".
## Decorations/meta/constraints (descriptions, formats, min/max, examples,
## titles, defaults) are OUT OF SCOPE by user word; a future @/decorator
## layer owns them. Paths/operations are OUT: the openapi emitter's routes
## are the served engine's own, by design. Round-trip = COMPONENTS.

## Read first
- v6/dl/POKEAPI_SHAPE_REPORT.md (the counted coverage table; your converter
  must beat its "no" rows that have since been built)
- v6/dl/fixtures/pokeapi_shape.dl6 (the HAND-built target shape; your
  generated output supersedes it)
- v6/dl/fixtures/pokeapi.openapi.yml (the input, 9839 lines, upstream spec)

## The converter
New tool, new files only: v6/tsv2/scripts/openapi_to_dl6.ts (use an
established yaml parser package, add it to v6/tsv2 package.json; never
hand-roll yaml). Reads an OpenAPI 3.x document, emits a .dl6 text program.

Mapping (each row is settled, do not redesign):
| openapi | dl6 |
|---|---|
| component schema (object) | rel, PascalCase -> snake_case |
| property, required | column `name: type` |
| property, nullable / anyOf [T, null] | `name: option(T)` |
| $ref to component | column typed as that rel name |
| array of scalars | json_list(scalar) |
| array of component $refs | list(rel_name) |
| inline object property | LIFT: mint `parent__prop` rel with the object's
  columns, column becomes `prop: parent__prop` (path-minted, nominal) |
| oneOf of component refs | payload enum: `rel name(variant_a(payload: rel_a) ; ...)` |
| string/integer/number/boolean | text/int/float/bool |
| everything metadata | dropped |

Nested inline objects lift recursively (`parent__prop__inner`). Name
collisions after snake_casing: refuse with a named error listing both
sources.

## The round-trip gate (the deliverable)
Script v6/tsv2/scripts/openapi_roundtrip_check.ts (or .py, pick one):
1. openapi_to_dl6 on pokeapi.openapi.yml -> gen/pokeapi_gen.dl6
2. compile it: bash v6/prolog/compile/scripts/compile_dl6.sh gen/pokeapi_gen.dl6 <out>
   MUST exit 0. If it fails on a spelling the compiler refuses, record the
   exact error in your report; do NOT fall back to `json` typing silently.
3. emit back: the compile pipeline's 4_emit_jsonschema/5_emit_openapi outputs
4. structural compare source spec vs emitted spec: component name set,
   per-component property name set, per-property {kind, ref-target,
   nullability}. Print a counted table (report style of
   POKEAPI_SHAPE_REPORT.md) with match/mismatch per category and exit
   nonzero on any mismatch outside the KNOWN emitter gaps you list at top.
5. Bank the table in v6/dl/POKEAPI_ROUNDTRIP_REPORT.md.

Known dependency: the anyOf nullable emit is being fixed by lane
fix/anyof-emit-phase; if its fix has not merged when you compare, count
nullable mismatches as a named known gap rather than failing.

## Files you own
- v6/tsv2/scripts/openapi_to_dl6.ts, openapi_roundtrip_check.*, package.json
  dependency line
- v6/dl/fixtures/pokeapi.openapi.yml stays as committed input
- generated gen/pokeapi_gen.dl6 + POKEAPI_ROUNDTRIP_REPORT.md
- a tiny hand fixture openapi doc exercising every mapping row + a test
  running converter + compile on it (put beside existing tsv2 tests)
Do NOT touch any .pl file. If the compiler refuses something the mapping
needs, that refusal goes in the report with the throw site, and you keep
going on the rest.

## Setup (REQUIRED; absolute cd each command)
```bash
cd <worktree>/v6/tsv2 && pnpm install
cd <worktree>/v6/sprefa-store/js && pnpm install
cd <worktree>/v6/sprefa-extract && cargo build --release --features cli --bin extract
```

## Gate
```bash
cd <worktree>/v6/tsv2 && pnpm vitest run tests/ --reporter=basic 2>&1 | tail -5
cd <worktree>/v6 && just typecheck && just tsv2-test
bash <worktree>/v6/tsv2/scripts/openapi_roundtrip_check.* (its own exit code is the gate)
```

## Commit rail (commit-or-report)
Up to 3 commits, prefix `tsv2:`. Blocked -> FAILURE-REPORT-OPENAPI.md, exact
command + output, exit NONZERO. Exiting 0 with uncommitted work or red gates
is a defect. NEVER --no-verify.

## Style
Comments state only constraints the code cannot show. Banned words, prose
and identifiers: provenance, substrate, load-bearing, regime, refusal.
TS: new classes declare interfaces in the package's types header per repo
law; sync stays sync; no bare export function for important entry points.
