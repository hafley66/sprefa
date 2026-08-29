// Corpus finding, FIXED by the ts module plane. `--resolve` runs ECMA-262
// 16.2.1.6.3 ResolveExport over the file set, so `normalize` imported through
// this barrel binds to `helpers.ts` even though `other.ts` spells the same
// name. TypeScript 5.9 routes its whole compiler through
// `src/compiler/_namespaces/ts.ts`, 73 star lines.
//
// Repro: extract --resolve --family call barrel_reexport/*.ts
// Expected and observed: caller `run` -> `helpers.ts:normalize`, kind
// `import_resolve`. The graded version of this shape is
// tests/54_ts_module_plane.rs.

export * from "./helpers.js";
