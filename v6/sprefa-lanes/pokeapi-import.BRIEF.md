# feature/pokeapi-import: live PokeAPI data through the compiled program

## Goal (the demo Chris actually wants)
One command imports real PokeAPI documents into the program compiled from
v6/dl/fixtures/pokeapi_shape.dl6 and proves it with row counts and queries.
The importer must be CATALOG-DRIVEN (reads the emitted catalog rows and maps
any JSON document mechanically), never a pokeapi-specific hand mapping. The
point being demonstrated: OpenAPI spec -> .dl6 rels -> compiled TS+SQLite ->
real data in -> queries out, with the mapper derived from the catalog.

## Read FIRST (the arrival surface is already built; do not reinvent it)
- v6/tsv2/serve/4_http.ts:239-330 — POST arrival body shape
  `[{rel, sign, row}, ...]`, `check_arrival_body`, and the comment at :272:
  nested relation-shaped values post into their own rel (golden-flex's
  `tree(tree_id, species, site: patch)` precedent). Find and read that
  golden-flex example end to end before writing the mapper.
- v6/prolog/compile/out/pokeapi_shape.ts — the compiled program + catalog rows
  (kind: primitive | json_list | rel | column | ...). This is the mapper's
  input. 212 rels; 139 array-of-ref columns are `json` columns by design in
  this version (stringify them); `__opt_*` option helpers wrap nullable
  columns — learn their arrival spelling from an existing option fixture.
- v6/tsv2/serve/main.ts + cli/bop.ts — how a program is loaded and served.

## Files owned by this lane
- v6/tsv2/import/ (new): `catalogImporter.ts` — pure functions: catalog rows +
  a JSON document + a root rel name -> arrival batches. Interface declared in
  a types.ts per the header-types law, I-prefixed. Sync array code, no
  Promises above the SqlRunner seam; the HTTP POST loop is the one effectful
  edge.
- v6/tsv2/import/pokeapiFeed.ts — fetches a BOUNDED set from pokeapi.co
  (gen-1: 151 pokemon + their abilities + types + species, ~600 documents),
  caches every response under sprefa-lanes/pokeapi-data/ so reruns hit disk,
  maps each through catalogImporter, POSTs batches. Batched arrivals, never
  one row per POST (N+1 law).
- v6/justfile: recipe `pokeapi-demo` = compile (if stale) + serve + feed +
  report. Budget-capped like the neighbors.
- v6/tsv2/import/POKEAPI_IMPORT_REPORT.md — receipts: documents fetched, rows
  per rel (top 20 table), three sample queries WITH their output (e.g. all
  grass-type gen-1 pokemon via the type rel; ability text for one pokemon;
  a count join), wall time per phase. Every phase under 10s except the
  first network fetch (state its time separately; cached reruns must be
  under 10s end to end).
- v6/tsv2/tests/import.test.ts — catalogImporter unit tests: a primitive-only
  doc, a doc with a nested summary ref, a doc with an option null, a doc with
  a json_list column, and one malformed doc rejected with the rel named.

## Rules discovered en route
If an arrival is REJECTED by check_arrival_body for a shape the catalog says
should work, that is a finding: record the exact rel, column, and rejection
text in the report. Do NOT patch serve/ or runtime/ — report only.

## Setup (REQUIRED first; absolute cd every command, cwd resets)
```bash
cd <worktree>/v6/tsv2 && pnpm install
cd <worktree>/v6/sprefa-store/js && pnpm install
```

## Validation gate
```bash
cd <worktree>/v6 && just typecheck
cd <worktree>/v6 && just tsv2-test
cd <worktree>/v6 && just pokeapi-demo   # cached rerun, under 10s
```

## Commit rail (commit-or-report)
- Commit ON THE BRANCH before exiting, up to 3 commits, prefix `tsv2:`.
  Do NOT commit sprefa-lanes/pokeapi-data (cache stays out of git).
- If blocked, FAILURE-REPORT.md at worktree root, exact command + output,
  exit nonzero. NEVER --no-verify.

## Style laws
- Interfaces in types.ts, I prefix; no bare export function for the mapper.
- Exactly one manual .subscribe() per app stands; the feeder is a script, not
  a new subscriber inside the runtime.
- Comments only for constraints code cannot show; max 2 consecutive lines.
- Banned words in prose and identifiers: provenance, substrate, load-bearing,
  regime, refusal.
