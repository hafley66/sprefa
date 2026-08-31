# Lane `bench-extract-oracles` (sonnet): compilers as oracles, and scip reach

Read `plans/extract-bench-2026-08-29/COMMON.md` first.

## Part A: the compiler-native oracles, one per language
| lang | oracle | how |
|---|---|---|
| ts | `typescript` package TypeChecker | node script under `plans/extract-bench-2026-08-29/oracle_ts.mjs`: `createProgram` on TypeScript-5.9's own tsconfig, walk every CallExpression, `checker.getResolvedSignature(...).declaration` -> dst file + name; imports via `resolvedModules`. |
| go | `golang.org/x/tools/go/callgraph` | a go program under `oracle_go/` on typescript-go's module: `packages.Load` + `callgraph/vta` (and `cha` as a second column); imports from `pkg.Imports`. |
| rust | rust-analyzer's scip is the compiler; ALSO `ra_ap_ide` call hierarchy if the lab crate at `dae353d75` shows it links cheaply, else state the cost and skip. | |
Emit the normal-form tsvs for `module`, `call`, `type` where the oracle has them.

## Part B: scip reach, the ratio table the user asked for
For each language, on the same corpus, one table with rows:
1. raw scip index: `scip_def / scip_ref / scip_edge / scip_fn_edge / scip_impl / scip_local` counts from `extract --family scip` (the index the crawl reports already built; rebuild only if missing).
2. what our resolve CONSUMES from it: `resolved_edge` rows with kind `scip_override` per family, and which scip record kinds the arm reads (cite the fn in `src/lang/<lang>.rs` and `src/scip_rows.rs`).
3. `--family diet_scip`: our own front-ends in scip shape, same counts as row 1.
4. parse resolve (`--resolve`, no scip): `resolved_edge` by kind, `resolved_import`, `unresolved` by reason.
5. the ratios: row4/row1, row2/row1, row3/row1 per family, and the SET overlap of each against the Part A oracle through `bench.py`.
Then a table of scip record kinds we never read (relationships, symbol_roles, documentation, signature_documentation, enclosing_symbol, diagnostics) with one line each on what fact it would give us.

## Ownership
Only `plans/extract-bench-2026-08-29/**` (your files: `oracle_ts.mjs`, `oracle_go/**`, `bench.py`, `ORACLES.REPORT.md`, `<lang>.{oracle,scip,dietscip,parse}.<family>.tsv`). No `src/` edits; a defect you find is a row in the report with a `file:line`.

## Receipt
`ORACLES.REPORT.md` opens with a TOC and the per-language ratio tables. Push a branch `bench/extract-oracles`, `gh pr create --base main`, hail `boop beep --no-wait --as bench-extract-oracles sprefa-coordinator "oracles: PR #N, ts parse/oracle call <a>/<b>, go <a>/<b>, rust <a>/<b>, scip consumed <x>%"`.
