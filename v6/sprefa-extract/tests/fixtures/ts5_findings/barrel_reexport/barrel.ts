// Corpus finding, NOT FIXED: `--resolve` never reads the `specifier` rows, so a
// name imported through an `export * from` barrel stays ambiguous whenever a
// second file in the universe spells it the same way. TypeScript 5.9 routes its
// whole compiler through `src/compiler/_namespaces/ts.ts`, 73 `export * from`
// lines; 1241 of its 11768 ambiguous call sites narrow to exactly one def once
// the named import plus this reexport closure is applied.
//
// `--deps` already emits the closure this needs, as file_edge kind=reexport.
//
// Repro: extract --resolve --family call barrel_reexport/*.ts
// Expected: caller `run` -> `helpers.ts:normalize`, picked through the barrel.
// Observed: no edge; `normalize` has two defs and the import is not consulted.

export * from "./helpers.js";
